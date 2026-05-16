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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presence {
    pub replica_id: String,
    pub cursor: usize,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Hello {
        replica_id: String,
        session_id: String,
    },
    Ops {
        ops: Vec<Op>,
    },
    Presence {
        presence: Presence,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Hydrate {
        ops: Vec<Op>,
        presence: HashMap<String, Presence>,
        clients: usize,
    },
    Ops {
        ops: Vec<Op>,
    },
    Presence {
        presence: HashMap<String, Presence>,
        clients: usize,
    },
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

        println!(
            "[INFO] connection {} connected ({} clients)",
            connection_id,
            session.clients.len()
        );
        (rx, hydrate)
    }

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

        println!(
            "[INFO] connection {} disconnected ({} clients)",
            connection_id,
            session.clients.len()
        );
    }

    pub async fn set_identity(&self, connection_id: u64, replica_id: String, session_id: String) {
        let mut session = self.inner.write().await;
        let Some(client) = session.clients.get_mut(&connection_id) else {
            eprintln!("[WARN] hello for unknown connection {}", connection_id);
            return;
        };

        client.replica_id = Some(replica_id);
        client._session_id = Some(session_id);
    }

    pub async fn process_ops(&self, from_connection: u64, ops: Vec<Op>) {
        let mut session = self.inner.write().await;
        if !session.clients.contains_key(&from_connection) {
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

    pub async fn update_presence(&self, connection_id: u64, presence: Presence) {
        let mut session = self.inner.write().await;
        let Some(client) = session.clients.get_mut(&connection_id) else {
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
}
