use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use serde::Serialize;
use crate::crdt::{Rga, Op};

pub type Rx = mpsc::UnboundedReceiver<String>;
type Tx  = mpsc::UnboundedSender<String>;

/// A position-based op from the client — no S4Vectors needed in the browser.
#[derive(Debug)]
pub struct ClientOp {
    pub op:  String,       // "insert", "delete", or "update"
    pub pos: usize,        // 0-indexed visible-character position
    pub ch:  Option<char>, // present for insert/update, absent for delete
}

struct ClientInfo {
    tx:          Tx,
    rga:         Rga,
    pending_ops: Vec<Op>,       // remote ops queued until this client's next sync
    cursor_pos:  Option<usize>, // LWW register: last reported caret position
}

/// Server → client: broadcast after every sync.
#[derive(Serialize)]
struct StateMsg<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    text:     &'a str,
    clients:  usize,
    cursors:  HashMap<u64, usize>,
}

#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<RwLock<HashMap<u64, ClientInfo>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new client. Returns (rx channel, current canonical text for init msg,
    /// and the current cursor positions of all already-connected clients).
    /// Hydrates the new client's RGA by replaying existing document ops (see hydration_ops),
    /// so late joiners can delete and update pre-existing content correctly.
    pub async fn connect(&self, site_id: u64) -> (Rx, String, HashMap<u64, usize>) {
        let mut clients = self.inner.write().await;
        let (tx, rx) = mpsc::unbounded_channel::<String>();

        // Replay the existing document into the new client's RGA
        let mut new_rga = Rga::new(site_id, 0);
        if let Some(existing) = clients.values().next() {
            for op in existing.rga.hydration_ops() {
                new_rga.apply(op);
            }
        }
        let current_text = new_rga.to_string();

        clients.insert(site_id, ClientInfo {
            tx,
            rga: new_rga,
            pending_ops: Vec::new(),
            cursor_pos:  None,
        });

        let cursors: HashMap<u64, usize> = clients.iter()
            .filter_map(|(sid, c)| c.cursor_pos.map(|pos| (*sid, pos)))
            .collect();

        println!("[INFO] site {} connected ({} clients)", site_id, clients.len());
        (rx, current_text, cursors)
    }

    /// Remove a disconnected client.
    pub async fn disconnect(&self, site_id: u64) {
        let mut clients = self.inner.write().await;
        clients.remove(&site_id);
        println!("[INFO] site {} disconnected ({} clients)", site_id, clients.len());
    }

    /// Core CRDT sync algorithm (spec §4).
    ///
    /// Step 1: apply incoming local ops to X's RGA (uses X's s_k / sid).
    /// Step 2: queue the resulting S4Vector ops into every OTHER client's pending_ops.
    /// Step 3: drain X's accumulated pending remote ops and apply them to X's RGA.
    /// Step 4: broadcast canonical state to all clients.
    pub async fn process_sync(&self, from_site: u64, ops: Vec<ClientOp>, cursor: Option<usize>) {
        let mut clients = self.inner.write().await;

        if !clients.contains_key(&from_site) {
            eprintln!("[WARN] process_sync for unknown site {}", from_site);
            return;
        }

        // LWW write — overwrite unconditionally; sequential write lock enforces ordering.
        // Reference: Shapiro et al. 2011, Section 3.1 (LWW Register).
        if let Some(pos) = cursor {
            clients.get_mut(&from_site).unwrap().cursor_pos = Some(pos);
        }

        // Step 1 — local ops → S4Vector ops
        let mut s4v_ops: Vec<Op> = Vec::new();
        for client_op in ops {
            let client = clients.get_mut(&from_site).unwrap();
            let result = match client_op.op.as_str() {
                "insert" => match client_op.ch {
                    Some(ch) => client.rga.local_insert(client_op.pos, ch),
                    None => {
                        eprintln!("[WARN] insert from site {} missing char", from_site);
                        None
                    }
                },
                "delete" => client.rga.local_delete(client_op.pos),
                "update" => match client_op.ch {
                    Some(ch) => client.rga.local_update(client_op.pos, ch),
                    None => None,
                },
                other => {
                    eprintln!("[WARN] unknown op '{}' from site {}", other, from_site);
                    None
                }
            };
            if let Some(op) = result {
                s4v_ops.push(op);
            }
        }

        // Step 2 — queue to all other clients
        for (site_id, client) in clients.iter_mut() {
            if *site_id != from_site {
                client.pending_ops.extend(s4v_ops.iter().cloned());
            }
        }

        // Step 3 — drain X's pending remote ops
        let pending = std::mem::take(&mut clients.get_mut(&from_site).unwrap().pending_ops);
        let client = clients.get_mut(&from_site).unwrap();
        for op in pending {
            client.rga.apply(op);
        }

        // Step 4 — broadcast canonical state to all
        let canonical = clients[&from_site].rga.to_string();
        let count = clients.len();
        let cursors: HashMap<u64, usize> = clients
            .iter()
            .filter_map(|(sid, c)| c.cursor_pos.map(|pos| (*sid, pos)))
            .collect();
        let json = serde_json::to_string(&StateMsg {
            msg_type: "state",
            text:     &canonical,
            clients:  count,
            cursors,
        })
        .expect("serialize StateMsg");

        for client in clients.values() {
            if client.tx.send(json.clone()).is_err() {
                // Recipient will be cleaned up when their WS task notices the disconnect
            }
        }

        // Step 5 — GC pass: prune tombstones acknowledged by all connected clients.
        // A tombstone at s_k is safe to remove iff:
        //   (a) every client RGA has s_k tombstoned, AND
        //   (b) no client's pending_ops contains Insert{left: Some(s_k)}.
        // Condition (b) guards against a race: a peer may have generated
        // Insert{left=s_k} before learning of the Delete, queued it here, but not
        // yet had it applied to its RGA. If we GC while that Insert is pending,
        // remote_insert finds no anchor and falls back to head → divergence.
        // Reference: Section 5.6, Roh et al. 2011.
        let site_ids: Vec<u64> = clients.keys().copied().collect();
        let candidates = clients[&from_site].rga.tombstoned_keys();
        for s_k in candidates {
            let safe = site_ids.iter().all(|sid| {
                let c = &clients[sid];
                c.rga.is_tombstoned(&s_k)
                    && !c.pending_ops.iter().any(|op| {
                        matches!(op, Op::Insert { left: Some(l), .. } if l == &s_k)
                    })
            });
            if safe {
                for sid in &site_ids {
                    clients.get_mut(sid).unwrap().rga.gc_node(&s_k);
                }
                println!("[INFO] [GC] pruned tombstone {}", s_k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cursor_lww_overwrite() {
        let registry = Registry::new();
        let (mut _rx1, _, _) = registry.connect(1).await;
        let (mut rx2, _, _) = registry.connect(2).await;

        registry.process_sync(1, vec![], Some(5)).await;
        registry.process_sync(1, vec![], Some(10)).await;

        let msg1: serde_json::Value =
            serde_json::from_str(&rx2.try_recv().unwrap()).unwrap();
        let msg2: serde_json::Value =
            serde_json::from_str(&rx2.try_recv().unwrap()).unwrap();

        assert_eq!(msg1["cursors"]["1"], 5);
        assert_eq!(msg2["cursors"]["1"], 10);
    }

    #[tokio::test]
    async fn test_cursor_none_preserves_previous() {
        let registry = Registry::new();
        let (mut _rx1, _, _) = registry.connect(1).await;
        let (mut rx2, _, _) = registry.connect(2).await;

        registry.process_sync(1, vec![], Some(7)).await;
        registry.process_sync(1, vec![], None).await;

        let _: serde_json::Value =
            serde_json::from_str(&rx2.try_recv().unwrap()).unwrap();
        let msg2: serde_json::Value =
            serde_json::from_str(&rx2.try_recv().unwrap()).unwrap();

        assert_eq!(msg2["cursors"]["1"], 7);
    }

    #[tokio::test]
    async fn test_cursor_broadcast_includes_all_sites() {
        let registry = Registry::new();
        let (mut rx1, _, _) = registry.connect(1).await;
        let (mut _rx2, _, _) = registry.connect(2).await;

        registry.process_sync(1, vec![], Some(3)).await;
        registry.process_sync(2, vec![], Some(8)).await;

        let _: serde_json::Value =
            serde_json::from_str(&rx1.try_recv().unwrap()).unwrap();
        let msg2: serde_json::Value =
            serde_json::from_str(&rx1.try_recv().unwrap()).unwrap();

        assert_eq!(msg2["cursors"]["1"], 3);
        assert_eq!(msg2["cursors"]["2"], 8);
    }

    #[tokio::test]
    async fn test_cursor_cleared_on_disconnect() {
        let registry = Registry::new();
        let (mut _rx1, _, _) = registry.connect(1).await;
        let (mut rx2, _, _) = registry.connect(2).await;

        registry.process_sync(1, vec![], Some(5)).await;
        let _: serde_json::Value =
            serde_json::from_str(&rx2.try_recv().unwrap()).unwrap();

        registry.disconnect(1).await;
        registry.process_sync(2, vec![], Some(2)).await;

        let msg: serde_json::Value =
            serde_json::from_str(&rx2.try_recv().unwrap()).unwrap();

        assert!(
            msg["cursors"].get("1").is_none(),
            "disconnected site should not appear in cursors"
        );
    }
}
