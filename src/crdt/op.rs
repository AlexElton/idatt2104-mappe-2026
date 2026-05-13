use serde::{Deserialize, Serialize};
use crate::crdt::char_id::CharId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {

    Insert {
        id: CharId,
        value: char,
        after: Option<CharId>,
    },

    Delete {
        target: CharId,
        deleted_by: u64,
    },
}

impl Op {
    pub fn insert(id: CharId, value: char, after: Option<CharId>) -> Self {
        Op::Insert { id, value, after }
    }

    pub fn delete(target: CharId, deleted_by: u64) -> Self {
        Op::Delete { target, deleted_by }
    }

    pub fn id(&self) -> Option<CharId> {
        match self {
            Op::Insert { id, .. } => Some(*id),
            Op::Delete { .. } => None,
        }
    }

    pub fn is_duplicate<F>(&self, known: F) -> bool
    where
        F: Fn(CharId) -> bool,
    {
        match self {
            Op::Insert { id, .. } => known(*id),
            Op::Delete { .. } => false, 
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpBatch {
    pub from_replica: u64,
    pub ops: Vec<Op>,
}

impl OpBatch {
    pub fn new(from_replica: u64, ops: Vec<Op>) -> Self {
        Self { from_replica, ops }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}
