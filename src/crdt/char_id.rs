use std::fmt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharId {
    pub lamport: u64,
    pub replica_id: u64,
}

impl CharId {
    pub fn new(lamport: u64, replica_id: u64) -> Self {
        Self { lamport, replica_id }
    }
}

impl Ord for CharId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.lamport
            .cmp(&other.lamport)
            .then_with(|| self.replica_id.cmp(&other.replica_id))
    }
}

impl PartialOrd for CharId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for CharId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.lamport, self.replica_id)
    }
}
