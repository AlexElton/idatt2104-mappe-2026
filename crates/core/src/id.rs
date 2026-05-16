use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

pub type ReplicaId = String;
pub type SessionId = String;
pub type NodeId = OperationId;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId {
    pub session_id: SessionId,
    pub replica_id: ReplicaId,
    pub lamport: u64,
    pub seq: u64,
}

impl OperationId {
    pub fn new(session_id: SessionId, replica_id: ReplicaId, lamport: u64, seq: u64) -> Self {
        Self {
            session_id,
            replica_id,
            lamport,
            seq,
        }
    }

    pub fn precedes(&self, other: &OperationId) -> bool {
        self.cmp(other) == Ordering::Less
    }
}

impl Ord for OperationId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.lamport
            .cmp(&other.lamport)
            .then_with(|| self.replica_id.cmp(&other.replica_id))
            .then_with(|| self.session_id.cmp(&other.session_id))
            .then_with(|| self.seq.cmp(&other.seq))
    }
}

impl PartialOrd for OperationId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.session_id, self.replica_id, self.lamport, self.seq
        )
    }
}
