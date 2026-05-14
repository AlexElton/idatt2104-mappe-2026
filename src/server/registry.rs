use std::{collections::HashMap, sync::Arc};

use tokio::sync::{mpsc, RwLock};

use crate::crdt::op::Message;

pub type Tx = mpsc::UnboundedSender<Message>;
pub type Rx = mpsc::UnboundedReceiver<Message>;

/// Tracks all connected WebSocket clients.
/// Each client is identified by a unique `site_id` and has an unbounded channel
/// the server uses to push outbound messages to it.
#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<RwLock<HashMap<u64, Tx>>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new client and return the receiving end of its channel.
    pub async fn connect(&self, site_id: u64) -> Rx {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner.write().await.insert(site_id, tx);
        rx
    }

    /// Send `msg` to every connected client except the sender.
    pub async fn broadcast(&self, from_site: u64, msg: &Message) {
        todo!("iterate inner, skip from_site, call tx.send(msg.clone()) for each")
    }

    /// Remove a client when it disconnects.
    pub async fn disconnect(&self, site_id: u64) {
        self.inner.write().await.remove(&site_id);
    }
}
