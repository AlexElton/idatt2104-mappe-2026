use serde::{Deserialize, Serialize};
use crate::crdt::OpBatch;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Join {
        replica_id: u64,
    },

    Welcome {
        /// The full operation history needed to reconstruct current state.
        history: Vec<crate::crdt::Op>,
        /// How many peers are currently connected.
        peer_count: usize,
    },

    /// A batch of ops flushed from a client's delay buffer.
    Ops(OpBatch),

    /// Broadcast by the server when a new peer connects.
    PeerJoined {
        replica_id: u64,
    },

    /// Broadcast by the server when a peer disconnects.
    PeerLeft {
        replica_id: u64,
    },

    UpdateDelay {
        /// Delay in milliseconds before flushing buffered ops.
        delay_ms: u64,
    },

    Ack,

    Error {
        message: String,
    },
}

impl Message {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

