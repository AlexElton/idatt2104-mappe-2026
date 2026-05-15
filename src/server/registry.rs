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
    pending_ops: Vec<Op>, // remote ops queued until this client's next sync
}

/// Server → client: broadcast after every sync.
#[derive(Serialize)]
struct StateMsg<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    text:     &'a str,
    clients:  usize,
}

#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<RwLock<HashMap<u64, ClientInfo>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new client. Returns (rx channel, current canonical text for init msg).
    /// Hydrates the new client's RGA by replaying existing document ops (see hydration_ops),
    /// so late joiners can delete and update pre-existing content correctly.
    pub async fn connect(&self, site_id: u64) -> (Rx, String) {
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
        });

        println!("[INFO] site {} connected ({} clients)", site_id, clients.len());
        (rx, current_text)
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
    pub async fn process_sync(&self, from_site: u64, ops: Vec<ClientOp>) {
        let mut clients = self.inner.write().await;

        if !clients.contains_key(&from_site) {
            eprintln!("[WARN] process_sync for unknown site {}", from_site);
            return;
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
        let json = serde_json::to_string(&StateMsg {
            msg_type: "state",
            text:     &canonical,
            clients:  count,
        })
        .expect("serialize StateMsg");

        for client in clients.values() {
            if client.tx.send(json.clone()).is_err() {
                // Recipient will be cleaned up when their WS task notices the disconnect
            }
        }

        // Step 5 — GC pass: prune tombstones acknowledged by all connected clients.
        // A tombstone is safe to remove when every client RGA has it marked tombstoned
        // (i.e., no client's pending_ops can still reference it as a left cobject).
        // Runs inside the write lock — no extra synchronisation needed.
        // Reference: Section 5.6, Roh et al. 2011.
        let site_ids: Vec<u64> = clients.keys().copied().collect();
        let candidates = clients[&from_site].rga.tombstoned_keys();
        for s_k in candidates {
            if site_ids.iter().all(|sid| clients[sid].rga.is_tombstoned(&s_k)) {
                for sid in &site_ids {
                    clients.get_mut(sid).unwrap().rga.gc_node(&s_k);
                }
                println!("[INFO] [GC] pruned tombstone {}", s_k);
            }
        }
    }
}
