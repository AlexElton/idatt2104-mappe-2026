use crate::{NodeId, OperationId};

/// A single node in the RGA linked list.
#[derive(Debug, Clone)]
pub struct Node {
    pub value: char,
    pub tombstone: bool,
    pub id: NodeId,
    pub deleted_by: Option<OperationId>,
    pub link: Option<usize>,
}

impl Node {
    pub fn new(value: char, id: NodeId) -> Self {
        Self {
            value,
            tombstone: false,
            id,
            deleted_by: None,
            link: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}
