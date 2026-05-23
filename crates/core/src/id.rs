//! Identity types for operations and nodes.
//!
//! Every operation gets a unique [`OperationId`]. Because a node's identity is
//! always the ID of the insert that created it, `NodeId` is just an alias for
//! `OperationId`.
//!
//! The total order on [`OperationId`] is what the RGA uses to settle concurrent
//! inserts: Lamport timestamp first (causal ordering), then `replica_id`,
//! `session_id`, and `seq` to break ties deterministically when two ops share
//! the same timestamp.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

pub type ReplicaId = String;
pub type SessionId = String;

/// A node's identity is the ID of the insert that created it.
pub type NodeId = OperationId;

/// A globally unique identifier for a single operation.
///
/// The four fields together guarantee uniqueness across replicas and sessions.
/// `lamport` advances monotonically and is updated whenever a remote op is
/// observed, so it reflects causal ordering. When two ops have the same
/// `lamport` (i.e. they were concurrent), the remaining fields break the tie
/// consistently on every replica.
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

    /// Returns `true` if this ID is ordered before `other`.
    pub fn precedes(&self, other: &OperationId) -> bool {
        self < other
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
