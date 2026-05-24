//! Shared document state and client broadcast logic.
//!
//! This essentially acts just like the client-side. The instance trackes its own
//! [`Replica`], we also provide functionality to re-broadcast any messages received
//! to all connected clients. The [`Registry`] is responsible for maintaining the
//! state of the shared document and broadcasting updates to all clients.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rga_core::{ApplyOutcome, Op, Replica};
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};

pub type Rx = mpsc::UnboundedReceiver<ServerMsg>;
type Tx = mpsc::UnboundedSender<ServerMsg>;

/// A client's current cursor position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presence {
    pub replica_id: String,
    pub cursor: usize,
}

/// A message sent from a client to the server over WebSocket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Registers the client's identity. Sent once after the socket opens.
    Hello {
        replica_id: String,
        session_id: String,
    },

    /// One or more RGA operations to apply and relay to other clients.
    Ops { ops: Vec<Op> },

    /// A cursor update, broadcast's to all other clients.
    Presence { presence: Presence },

    /// Requests all clients to perform garbage collection by removing all tombstones
    GarbageCollect,
}

/// A message sent from the server to a client over WebSocket.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Sent immediately on connection with the full document and presence state.
    Hydrate {
        ops: Vec<Op>,
        presence: HashMap<String, Presence>,
        clients: usize,
    },

    /// One or more operations accepted from another client.
    Ops { ops: Vec<Op> },

    /// Updated presence map, sent on connect, disconnect, and cursor moves.
    Presence {
        presence: HashMap<String, Presence>,
        clients: usize,
    },

    /// Sent after a GC pass with the number of tombstoned nodes removed.
    GarbageCollect { removed: usize },
}

struct ClientInfo {
    tx: Tx,
    replica_id: Option<String>,
    _session_id: Option<String>,
}

struct DocumentSession {
    replica: Replica,
    op_log: Vec<Op>,
    clients: HashMap<u64, ClientInfo>,
    presence: HashMap<String, Presence>,
}

/// Shared document state for all connected clients.
///
/// Cheaply cloneable because all clones point to the same memory location.
#[derive(Clone)]
pub struct Registry {
    inner: Arc<RwLock<DocumentSession>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(DocumentSession {
                replica: Replica::new("server".to_string(), new_server_session_id()),
                op_log: Vec::new(),
                clients: HashMap::new(),
                presence: HashMap::new(),
            })),
        }
    }

    /// Registers a new connection and returns its receive channel and hydration message.
    ///
    /// The hydration message contains the full op log and presence map so the
    /// client can reconstruct the document without a separate request.
    pub async fn connect(&self, connection_id: u64) -> (Rx, ServerMsg) {
        let (tx, rx) = mpsc::unbounded_channel::<ServerMsg>();
        let mut session = self.inner.write().await;

        session.clients.insert(
            connection_id,
            ClientInfo {
                tx,
                replica_id: None,
                _session_id: None,
            },
        );

        let hydrate = ServerMsg::Hydrate {
            ops: session.op_log.clone(),
            presence: session.presence.clone(),
            clients: session.clients.len(),
        };

        // TODO: Should be replaced with propper logging system
        println!(
            "[INFO] connection {} connected ({} clients)",
            connection_id,
            session.clients.len()
        );
        (rx, hydrate)
    }

    /// Removes the connection and clears its presence entry.
    ///
    /// Broadcasts an updated presence map to all remaining clients.
    pub async fn disconnect(&self, connection_id: u64) {
        let mut session = self.inner.write().await;
        let removed = session.clients.remove(&connection_id);
        if let Some(client) = removed
            && let Some(replica_id) = client.replica_id
        {
            session.presence.remove(&replica_id);
        }

        let message = ServerMsg::Presence {
            presence: session.presence.clone(),
            clients: session.clients.len(),
        };
        broadcast(&session.clients, message, None);

        // TODO: Should be replaced with propper logging system
        println!(
            "[INFO] connection {} disconnected ({} clients)",
            connection_id,
            session.clients.len()
        );
    }

    /// Associates a `replica_id` and `session_id` with an existing connection.
    ///
    /// Called when the client sends a `Hello` message after the socket opens.
    pub async fn set_identity(&self, connection_id: u64, replica_id: String, session_id: String) {
        let mut session = self.inner.write().await;
        let Some(client) = session.clients.get_mut(&connection_id) else {
            // TODO: Should be replaced with propper logging system
            eprintln!("[WARN] hello for unknown connection {}", connection_id);
            return;
        };

        client.replica_id = Some(replica_id);
        client._session_id = Some(session_id);
    }

    /// Applies ops from `from_connection` to the shared replica and broadcasts accepted ones.
    ///
    /// Invalid and duplicate ops are logged and dropped. The sending client
    /// does not receive the broadcast.
    pub async fn process_ops(&self, from_connection: u64, ops: Vec<Op>) {
        let mut session = self.inner.write().await;
        if !session.clients.contains_key(&from_connection) {
            // TODO: Should be replaced with propper logging system
            eprintln!("[WARN] ops for unknown connection {}", from_connection);
            return;
        }

        let mut accepted = Vec::new();
        for op in ops {
            let outcome = session.replica.apply_remote(op.clone());
            match outcome {
                ApplyOutcome::Applied => {
                    session.op_log.push(op.clone());
                    accepted.push(op);
                }
                ApplyOutcome::Duplicate => {}
                ApplyOutcome::MissingDependency | ApplyOutcome::Invalid => {
                    // TODO: Should be replaced with propper logging system
                    eprintln!(
                        "[WARN] rejected op from connection {}: {:?}",
                        from_connection, outcome
                    );
                }
            }
        }

        if accepted.is_empty() {
            return;
        }

        broadcast(
            &session.clients,
            ServerMsg::Ops { ops: accepted },
            Some(from_connection),
        );
    }

    /// Updates the cursor position for a connection and broadcasts to all clients.
    pub async fn update_presence(&self, connection_id: u64, presence: Presence) {
        let mut session = self.inner.write().await;
        let Some(client) = session.clients.get_mut(&connection_id) else {
            // TODO: Should be replaced with propper logging system
            eprintln!("[WARN] presence for unknown connection {}", connection_id);
            return;
        };

        if client.replica_id.as_deref() != Some(presence.replica_id.as_str()) {
            client.replica_id = Some(presence.replica_id.clone());
        }
        session
            .presence
            .insert(presence.replica_id.clone(), presence);

        let message = ServerMsg::Presence {
            presence: session.presence.clone(),
            clients: session.clients.len(),
        };
        broadcast(&session.clients, message, None);
    }

    /// Compacts the shared document by removing tombstoned nodes.
    ///
    /// Rebuilds the op log from the surviving nodes and broadcasts a
    /// `GarbageCollect` message with the removal count to all other clients.
    pub async fn garbage_collect(&self, from_connection: u64) {
        let mut session = self.inner.write().await;
        if !session.clients.contains_key(&from_connection) {
            // TODO: Should be replaced with propper logging system
            eprintln!(
                "[WARN] garbage_collect for unknown connection {}",
                from_connection
            );
            return;
        }

        let removed = session.replica.clear_deleted_nodes();
        session.op_log = session.replica.hydration_ops();

        broadcast(
            &session.clients,
            ServerMsg::GarbageCollect { removed },
            Some(from_connection),
        );
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

fn broadcast(clients: &HashMap<u64, ClientInfo>, message: ServerMsg, skip: Option<u64>) {
    for (connection_id, client) in clients {
        if Some(*connection_id) == skip {
            continue;
        }
        let _ = client.tx.send(message.clone());
    }
}

fn new_server_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("server-{nanos}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replica(name: &str) -> Replica {
        Replica::new(name.to_string(), format!("{name}-session"))
    }

    #[tokio::test]
    async fn hydration_contains_accepted_ops() {
        let registry = Registry::new();
        let (_rx1, _) = registry.connect(1).await;
        let mut source = replica("a");
        let op = source.local_insert(0, 'x').unwrap();

        registry.process_ops(1, vec![op]).await;

        let (_rx2, hydrate) = registry.connect(2).await;
        let ServerMsg::Hydrate { ops, .. } = hydrate else {
            panic!("expected hydrate");
        };
        assert_eq!(ops.len(), 1);
    }

    #[tokio::test]
    async fn process_ops_does_not_echo_sender() {
        let registry = Registry::new();
        let (mut rx1, _) = registry.connect(1).await;
        let (mut rx2, _) = registry.connect(2).await;
        let mut source = replica("a");
        let op = source.local_insert(0, 'x').unwrap();

        registry.process_ops(1, vec![op]).await;

        assert!(rx1.try_recv().is_err());
        assert!(matches!(rx2.try_recv().unwrap(), ServerMsg::Ops { .. }));
    }

    #[tokio::test]
    async fn duplicate_ops_are_not_rebroadcast() {
        let registry = Registry::new();
        let (_rx1, _) = registry.connect(1).await;
        let (mut rx2, _) = registry.connect(2).await;
        let mut source = replica("a");
        let op = source.local_insert(0, 'x').unwrap();

        registry.process_ops(1, vec![op.clone()]).await;
        let _ = rx2.try_recv().unwrap();
        registry.process_ops(1, vec![op]).await;

        assert!(rx2.try_recv().is_err());
    }

    #[tokio::test]
    async fn presence_is_removed_on_disconnect() {
        let registry = Registry::new();
        let (_rx1, _) = registry.connect(1).await;
        let (mut rx2, _) = registry.connect(2).await;

        registry
            .update_presence(
                1,
                Presence {
                    replica_id: "a".to_string(),
                    cursor: 3,
                },
            )
            .await;
        let _ = rx2.try_recv().unwrap();

        registry.disconnect(1).await;

        let ServerMsg::Presence { presence, clients } = rx2.try_recv().unwrap() else {
            panic!("expected presence");
        };
        assert_eq!(clients, 1);
        assert!(!presence.contains_key("a"));
    }

    #[tokio::test]
    async fn garbage_collect_rewrites_hydration_log_and_broadcasts() {
        let registry = Registry::new();
        let (_rx1, _) = registry.connect(1).await;
        let (mut rx2, _) = registry.connect(2).await;
        let mut source = replica("a");
        let insert_a = source.local_insert(0, 'a').unwrap();
        let insert_b = source.local_insert(1, 'b').unwrap();
        let delete_a = source.local_delete(0).unwrap();

        registry
            .process_ops(1, vec![insert_a, insert_b, delete_a])
            .await;
        let _ = rx2.try_recv().unwrap();

        registry.garbage_collect(1).await;

        assert!(matches!(
            rx2.try_recv().unwrap(),
            ServerMsg::GarbageCollect { removed: 1 }
        ));
        let (_rx3, hydrate) = registry.connect(3).await;
        let ServerMsg::Hydrate { ops, .. } = hydrate else {
            panic!("expected hydrate");
        };
        assert_eq!(ops.len(), 1);
    }
}
