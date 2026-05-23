use crate::{NodeId, OperationId};

/// A single character in the RGA linked list.
///
/// Deleted characters are not removed from the list. `tombstone` is set to
/// `true` and the node stays in place so that concurrent inserts anchored to
/// it still resolve correctly. `link` is the vec-index of the next node in
/// list order (not insertion order).
#[derive(Debug, Clone)]
pub struct Node {
    pub value: char,
    pub tombstone: bool,
    pub left: Option<NodeId>,
    pub id: NodeId,
    pub deleted_by: Option<OperationId>,
    pub link: Option<usize>,
}

impl Node {
    pub fn new(value: char, left: Option<NodeId>, id: NodeId) -> Self {
        Self {
            value,
            tombstone: false,
            left,
            id,
            deleted_by: None,
            link: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}
