//! The RGA linked list.
//!
//! Text is stored as a singly-linked list of nodes. The list order is the
//! canonical character order. Tombstones (deleted characters) stay in the list
//! until [`Rga::clear_tombstones`] is called.
//!
//! # Concurrent insert resolution
//!
//! When two replicas insert at the same position concurrently, both ops arrive
//! with the same `left` anchor. The list resolves this by scanning forward
//! past any nodes whose [`OperationId`] is greater than the new node's ID.
//! Because that ordering is total and deterministic, every replica ends up
//! with the same sequence regardless of arrival order.
//!
//! # Why tombstones persist
//!
//! Removing a deleted node immediately would break any in-flight insert that
//! references it as its `left` anchor. The insert would come in later, fail
//! with [`RgaError::MissingDependency`], and be lost. Keeping the tombstone
//! means the anchor is always resolvable until an explicit GC pass.

use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use crate::{node::Node, NodeId, OperationId};

/// A point-in-time snapshot of the RGA list, including tombstones.
///
/// Returned by [`Rga::tree`] and [`crate::Replica::rga_tree`]. Useful for debugging
/// and for driving a UI that needs to display or animate deleted characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RgaTree {
    pub text: String,
    pub nodes: Vec<RgaTreeNode>,
}

/// A single node in an [`RgaTree`] snapshot.
///
/// `visible_index` is `Some(n)` when the character is not a tombstone, where
/// `n` is its 0-based position in the visible text. Tombstoned nodes have
/// `visible_index: None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RgaTreeNode {
    pub index: usize,
    pub visible_index: Option<usize>,
    pub value: char,
    pub tombstone: bool,
    pub id: NodeId,
    pub left: Option<NodeId>,
    pub next: Option<NodeId>,
    pub deleted_by: Option<OperationId>,
}

/// Errors returned by [`Rga`] operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgaError {
    /// The same [`OperationId`] has been inserted twice.
    DuplicateNode,
    /// The operation references a node that does not yet exist in this replica.
    MissingDependency,
    /// The operation is structurally invalid.
    Invalid,
}

impl fmt::Display for RgaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RgaError::DuplicateNode => f.write_str("duplicate node"),
            RgaError::MissingDependency => f.write_str("missing dependency"),
            RgaError::Invalid => f.write_str("invalid operation"),
        }
    }
}

impl std::error::Error for RgaError {}

/// The RGA linked list for a single document.
#[derive(Debug, Clone, Default)]
pub struct Rga {
    nodes: Vec<Node>,
    node_indices: HashMap<NodeId, usize>,
    head: Option<usize>,
}

impl Rga {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new character to the right of `left`.
    ///
    /// Pass `left: None` to insert at the head of the document. If two inserts
    /// arrive with the same `left`, the one with the lower [`OperationId`] is
    /// placed further right; the scan walks forward until it finds a node whose
    /// ID is smaller, then stops.
    ///
    /// # Errors
    ///
    /// - [`RgaError::DuplicateNode`] if `id` is already in the list.
    /// - [`RgaError::MissingDependency`] if `left` is `Some` but not yet in the list.
    pub fn insert(
        &mut self,
        left: Option<NodeId>,
        value: char,
        id: NodeId,
    ) -> Result<(), RgaError> {
        if self.node_indices.contains_key(&id) {
            return Err(RgaError::DuplicateNode);
        }

        let left_idx = match left.as_ref() {
            Some(left_id) => Some(
                self.node_indices
                    .get(left_id)
                    .copied()
                    .ok_or(RgaError::MissingDependency)?,
            ),
            None => None,
        };

        let new_idx = self.nodes.len();
        self.nodes.push(Node::new(value, left.clone(), id.clone()));
        self.node_indices.insert(id.clone(), new_idx);

        let mut prev = left_idx;
        let mut cur = match left_idx {
            None => self.head,
            Some(idx) => self.nodes[idx].link,
        };

        while let Some(cur_idx) = cur {
            if self.nodes[cur_idx].id.precedes(&id) {
                break;
            }
            prev = Some(cur_idx);
            cur = self.nodes[cur_idx].link;
        }

        self.nodes[new_idx].link = cur;
        match prev {
            None => self.head = Some(new_idx),
            Some(prev_idx) => self.nodes[prev_idx].link = Some(new_idx),
        }

        Ok(())
    }

    /// Marks the node identified by `target` as deleted.
    ///
    /// The node stays in the list as a tombstone. Its position continues to
    /// anchor concurrent inserts that used it as their `left` reference.
    ///
    /// # Errors
    ///
    /// - [`RgaError::MissingDependency`] if `target` is not in the list.
    /// - [`RgaError::Invalid`] if `target` and `deleted_by` are the same ID.
    pub fn delete(&mut self, target: NodeId, deleted_by: OperationId) -> Result<(), RgaError> {
        if target == deleted_by {
            return Err(RgaError::Invalid);
        }

        let idx = self
            .node_indices
            .get(&target)
            .copied()
            .ok_or(RgaError::MissingDependency)?;

        self.nodes[idx].tombstone = true;
        self.nodes[idx].deleted_by = Some(deleted_by);
        Ok(())
    }

    pub fn contains_node(&self, id: &NodeId) -> bool {
        self.node_indices.contains_key(id)
    }

    /// Returns the [`NodeId`] of the `pos`-th visible character to use as the
    /// `left` anchor for a local insert at that position.
    ///
    /// Returns `None` for `pos == 0` (head insert) or when `pos` is beyond
    /// the current visible length.
    pub fn left_id_for_insert(&self, pos: usize) -> Option<NodeId> {
        self.find_visible(pos).map(|idx| self.nodes[idx].id.clone())
    }

    /// Returns the [`NodeId`] of the visible character at `pos` for use as a
    /// deletion target. `pos` is 0-based over visible characters only.
    ///
    /// Returns `None` if `pos` is out of range.
    pub fn target_id_for_delete(&self, pos: usize) -> Option<NodeId> {
        self.find_visible(pos.checked_add(1)?)
            .map(|idx| self.nodes[idx].id.clone())
    }

    fn find_visible(&self, pos: usize) -> Option<usize> {
        if pos == 0 {
            return None;
        }

        let mut count = 0;
        let mut cur = self.head;
        while let Some(idx) = cur {
            if !self.nodes[idx].is_tombstone() {
                count += 1;
                if count == pos {
                    return Some(idx);
                }
            }
            cur = self.nodes[idx].link;
        }
        None
    }

    /// Returns the visible text, skipping tombstoned characters.
    pub fn text(&self) -> String {
        self.to_string()
    }

    /// Returns a full snapshot of the list, including tombstones.
    ///
    /// Useful for debugging and for driving UIs that need to show or animate
    /// deleted characters. For just the visible content, use [`text`][Rga::text].
    pub fn tree(&self) -> RgaTree {
        let mut visible_indices = vec![None; self.nodes.len()];
        let mut visible_index = 0;
        let mut cur = self.head;

        while let Some(idx) = cur {
            if !self.nodes[idx].is_tombstone() {
                visible_indices[idx] = Some(visible_index);
                visible_index += 1;
            }
            cur = self.nodes[idx].link;
        }

        let nodes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| RgaTreeNode {
                index,
                visible_index: visible_indices[index],
                value: node.value,
                tombstone: node.tombstone,
                id: node.id.clone(),
                left: node.left.clone(),
                next: node
                    .link
                    .and_then(|next_idx| self.nodes.get(next_idx))
                    .map(|next| next.id.clone()),
                deleted_by: node.deleted_by.clone(),
            })
            .collect();

        RgaTree {
            text: self.text(),
            nodes,
        }
    }

    /// Removes tombstoned nodes and rebuilds the index.
    ///
    /// After compaction, any in-flight op that references a removed node as its
    /// `left` anchor will fail with [`RgaError::MissingDependency`]. Only call
    /// this when all connected peers are fully caught up.
    ///
    /// Returns the number of nodes removed.
    pub fn clear_tombstones(&mut self) -> usize {
        let removed_lefts: HashMap<NodeId, Option<NodeId>> = self
            .nodes
            .iter()
            .filter(|node| node.tombstone)
            .map(|node| (node.id.clone(), node.left.clone()))
            .collect();

        if removed_lefts.is_empty() {
            return 0;
        }

        let mut live_order = Vec::new();
        let mut cur = self.head;
        while let Some(idx) = cur {
            if !self.nodes[idx].tombstone {
                live_order.push(idx);
            }
            cur = self.nodes[idx].link;
        }

        let mut nodes = Vec::with_capacity(live_order.len());
        let mut node_indices = HashMap::with_capacity(live_order.len());

        for old_idx in live_order {
            let mut node = self.nodes[old_idx].clone();
            node.left = live_left(node.left, &removed_lefts);
            node.link = None;
            node_indices.insert(node.id.clone(), nodes.len());
            nodes.push(node);
        }

        let len = nodes.len();
        for (idx, node) in nodes.iter_mut().enumerate() {
            node.link = (idx + 1 < len).then_some(idx + 1);
        }

        let removed = removed_lefts.len();
        self.head = (!nodes.is_empty()).then_some(0);
        self.nodes = nodes;
        self.node_indices = node_indices;
        removed
    }
}

fn live_left(
    mut left: Option<NodeId>,
    removed_lefts: &HashMap<NodeId, Option<NodeId>>,
) -> Option<NodeId> {
    while let Some(id) = left.as_ref() {
        let Some(next_left) = removed_lefts.get(id) else {
            break;
        };
        left = next_left.clone();
    }
    left
}

impl fmt::Display for Rga {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut cur = self.head;
        while let Some(idx) = cur {
            if !self.nodes[idx].is_tombstone() {
                write!(f, "{}", self.nodes[idx].value)?;
            }
            cur = self.nodes[idx].link;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(replica_id: &str, lamport: u64) -> OperationId {
        OperationId::new(
            "session".to_string(),
            replica_id.to_string(),
            lamport,
            lamport,
        )
    }

    #[test]
    fn empty_rga_is_empty_string() {
        let rga = Rga::new();
        assert_eq!(rga.text(), "");
    }

    #[test]
    fn remote_insert_at_head() {
        let mut rga = Rga::new();
        rga.insert(None, 'a', id("a", 1)).unwrap();
        assert_eq!(rga.text(), "a");
    }

    #[test]
    fn sequential_remote_inserts() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("a", 2);
        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a), 'b', b).unwrap();
        assert_eq!(rga.text(), "ab");
    }

    #[test]
    fn concurrent_insert_same_position_converges() {
        let a = id("a", 1);
        let b = id("b", 1);

        let mut rga1 = Rga::new();
        rga1.insert(None, 'a', a.clone()).unwrap();
        rga1.insert(None, 'b', b.clone()).unwrap();

        let mut rga2 = Rga::new();
        rga2.insert(None, 'b', b).unwrap();
        rga2.insert(None, 'a', a).unwrap();

        assert_eq!(rga1.text(), rga2.text());
        assert_eq!(rga1.text(), "ba");
    }

    #[test]
    fn delete_hides_node_but_keeps_anchor_for_concurrent_insert() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("a", 2);
        let x = id("b", 3);

        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a.clone()), 'b', b).unwrap();
        rga.delete(a.clone(), id("a", 3)).unwrap();
        rga.insert(Some(a), 'x', x).unwrap();

        assert_eq!(rga.text(), "xb");
    }

    #[test]
    fn missing_insert_anchor_is_reported() {
        let mut rga = Rga::new();
        let result = rga.insert(Some(id("missing", 1)), 'a', id("a", 1));
        assert_eq!(result, Err(RgaError::MissingDependency));
    }

    #[test]
    fn tree_exposes_anchor_and_link_order() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("b", 1);
        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a.clone()), 'b', b.clone()).unwrap();
        rga.delete(a.clone(), id("a", 2)).unwrap();

        let tree = rga.tree();

        assert_eq!(tree.text, "b");
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].left, None);
        assert_eq!(tree.nodes[0].next, Some(b.clone()));
        assert!(tree.nodes[0].tombstone);
        assert_eq!(tree.nodes[0].visible_index, None);
        assert_eq!(tree.nodes[1].left, Some(a));
        assert_eq!(tree.nodes[1].visible_index, Some(0));
    }

    #[test]
    fn clear_tombstones_removes_deleted_nodes_and_preserves_text() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("a", 2);
        let x = id("b", 3);
        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a.clone()), 'b', b.clone()).unwrap();
        rga.delete(a.clone(), id("a", 3)).unwrap();
        rga.insert(Some(a), 'x', x.clone()).unwrap();

        let removed = rga.clear_tombstones();
        let tree = rga.tree();

        assert_eq!(removed, 1);
        assert_eq!(tree.text, "xb");
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].id, x);
        assert_eq!(tree.nodes[0].left, None);
        assert_eq!(tree.nodes[0].next, Some(b.clone()));
        assert_eq!(tree.nodes[1].id, b);
    }
}
