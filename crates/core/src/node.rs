//! Internal node storage for the RGA tree.
//!
//! A node is one inserted character plus the metadata needed to keep it ordered
//! relative to other nodes. The type is not exported directly; public snapshots
//! use [`crate::RgaTreeNode`] instead.

use crate::{NodeId, OperationId};

/// A single character in the RGA tree.
///
/// Deleted characters are not removed from the tree. `tombstone` is set to
/// `true` and the node stays in place so inserts anchored to it still resolve
/// correctly.
#[derive(Debug, Clone)]
pub struct Node {
    pub value: char,
    pub tombstone: bool,
    pub left: Option<NodeId>,
    pub id: NodeId,
    pub deleted_by: Option<OperationId>,
    pub children: Vec<usize>,
}

impl Node {
    /// Creates a live node for one inserted character.
    pub fn new(value: char, left: Option<NodeId>, id: NodeId) -> Self {
        Self {
            value,
            tombstone: false,
            left,
            id,
            deleted_by: None,
            children: Vec::new(),
        }
    }

    /// Returns whether this node has been deleted.
    pub fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}
