//! Id types for operations and RGA nodes.
//!
//! Every operation has a unique [`OperationId`]. A node is created by an insert
//! operation, so [`NodeId`] is the same identifier as the insert operation that
//! created that node.
//!
//! The total order on [`OperationId`] is the tie-breaker used when
//! multiple inserts share the same parent node. The comparison uses:
//!
//! 1. Lamport timestamp
//! 2. Replica ID
//! 3. Session ID
//! 4. Local sequence number

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// Id for one logical replica, such as a browser profile.
pub type ReplicaId = String;

/// Id for one runtime session of a replica.
pub type SessionId = String;

/// Id for a node in the RGA tree.
pub type NodeId = OperationId;

/// A globally unique identifier for a single operation.
///
/// The four fields together make operations unique and give inserts that is
/// done at the same time a stable order on every replica.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OperationId {
    /// Runtime session that produced the operation.
    pub session_id: SessionId,
    /// Logical replica that produced the operation.
    pub replica_id: ReplicaId,
    /// Lamport timestamp observed by the producing replica.
    pub lamport: u64,
    /// Local sequence number within the producing replica.
    pub seq: u64,
}

impl OperationId {
    /// Creates a new operation identifier.
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
