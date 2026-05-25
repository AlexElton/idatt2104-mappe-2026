//! The Replicated Growable Array (RGA) tree.
//!
//! This module is the low-level data structure behind [`crate::Replica`]. It
//! knows how to insert nodes, mark nodes as deleted, and render the current
//! visible text.
//!
//! # Tree shape
//!
//! Each insert names the node to its left. In the tree, that makes the new node a
//! child of the left anchor. Inserts at the head of the document have no anchor.
//! The document order is a depth-first traversal of the
//! roots and children.
//!
//! # Concurrent inserts
//!
//! If two replicas insert at the same visible position, both operations have the
//! same left anchor and become siblings. Siblings are ordered deterministically by
//! [`OperationId`].
//!
//! # Tombstones
//!
//! Deletes do not remove nodes immediately. A deleted node becomes a tombstone so
//! future inserts that still reference it as their left anchor can be applied.
//! [`Rga::clear_tombstones`] "physically" removes tombstones and rewrites anchors,
//! but that is only safe when no replica will later send operations that depend
//! on the removed nodes.
//!
//! # Debug snapshots
//!
//! [`Rga::tree`] returns the full tree structure.
//! It includes visible text, tombstones, anchors, child links, and the
//! next node in traversal order.

use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use crate::{node::Node, NodeId, OperationId};

/// RGA tree, including tombstones.
///
/// Returned by [`Rga::tree`] and [`crate::Replica::rga_tree`]. Useful for debugging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RgaTree {
    /// Visible document text with tombstones skipped.
    pub text: String,
    /// All nodes in internal storage order, including tombstones.
    pub nodes: Vec<RgaTreeNode>,
}

/// A single node in an [`RgaTree`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RgaTreeNode {
    pub index: usize,
    pub visible_index: Option<usize>,
    pub value: char,
    pub tombstone: bool,
    pub id: NodeId,
    pub left: Option<NodeId>,
    pub children: Vec<NodeId>,
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
    /// The operation is invalid.
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

/// The RGA tree for a single document.
#[derive(Debug, Clone, Default)]
pub struct Rga {
    nodes: Vec<Node>,
    node_indices: HashMap<NodeId, usize>,
    roots: Vec<usize>,
}

impl Rga {
    /// Creates an empty RGA tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new character to the right of `left`.
    ///
    /// # Errors
    ///
    /// - [`RgaError::DuplicateNode`] if `id` is already in the tree.
    /// - [`RgaError::MissingDependency`] if `left` is `Some` but not yet in the tree.
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

        match left_idx {
            None => insert_child(&mut self.roots, &self.nodes, new_idx),
            Some(parent_idx) => {
                let mut children = std::mem::take(&mut self.nodes[parent_idx].children);
                insert_child(&mut children, &self.nodes, new_idx);
                self.nodes[parent_idx].children = children;
            }
        }

        Ok(())
    }

    /// Marks the node identified by `target` as deleted.
    ///
    /// The node stays in the tree as a tombstone.
    ///
    /// # Errors
    ///
    /// - [`RgaError::MissingDependency`] if `target` is not in the tree.
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

    /// Returns whether a node exists, including tombstoned nodes.
    pub fn contains_node(&self, id: &NodeId) -> bool {
        self.node_indices.contains_key(id)
    }

    /// Returns the [`NodeId`] at the specified position of the visible characters.
    ///
    /// Returns `None` for `pos == 0` (head position) or when `pos` is outside
    /// the current visible length.
    pub fn get_node_id_by_position(&self, pos: usize) -> Option<NodeId> {
        self.find_visible(pos).map(|idx| self.nodes[idx].id.clone())
    }

    fn find_visible(&self, pos: usize) -> Option<usize> {
        if pos == 0 {
            return None;
        }

        let mut count = 0;
        for idx in self.dfs_order() {
            if !self.nodes[idx].is_tombstone() {
                count += 1;
                if count == pos {
                    return Some(idx);
                }
            }
        }
        None
    }

    fn dfs_order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.nodes.len());
        for &root in &self.roots {
            self.push_dfs(root, &mut order);
        }
        order
    }

    fn push_dfs(&self, idx: usize, order: &mut Vec<usize>) {
        order.push(idx);
        for &child in &self.nodes[idx].children {
            self.push_dfs(child, order);
        }
    }

    /// Returns the full RGA tree, including tombstones.
    ///
    /// Useful for debugging and for displaying in UIs showing deleted characters.
    pub fn tree(&self) -> RgaTree {
        let order = self.dfs_order();
        let mut visible_indices = vec![None; self.nodes.len()];
        let mut next_ids = vec![None; self.nodes.len()];
        let mut visible_index = 0;

        for window in order.windows(2) {
            next_ids[window[0]] = Some(self.nodes[window[1]].id.clone());
        }

        for idx in order {
            if !self.nodes[idx].is_tombstone() {
                visible_indices[idx] = Some(visible_index);
                visible_index += 1;
            }
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
                children: node
                    .children
                    .iter()
                    .map(|child_idx| self.nodes[*child_idx].id.clone())
                    .collect(),
                next: next_ids[index].clone(),
                deleted_by: node.deleted_by.clone(),
            })
            .collect();

        RgaTree {
            text: self.to_string(),
            nodes,
        }
    }

    /// Removes tombstoned nodes and rebuilds anchors and indexes.
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

        let live_order: Vec<_> = self
            .dfs_order()
            .into_iter()
            .filter(|idx| !self.nodes[*idx].tombstone)
            .collect();

        let mut nodes = Vec::with_capacity(live_order.len());
        let mut node_indices = HashMap::with_capacity(live_order.len());

        for old_idx in live_order {
            let mut node = self.nodes[old_idx].clone();
            node.left = live_left(node.left, &removed_lefts);
            node.children.clear();
            node_indices.insert(node.id.clone(), nodes.len());
            nodes.push(node);
        }

        let roots = rebuild_children(&mut nodes, &node_indices);

        let removed = removed_lefts.len();
        self.roots = roots;
        self.nodes = nodes;
        self.node_indices = node_indices;
        removed
    }
}

fn insert_child(children: &mut Vec<usize>, nodes: &[Node], child_idx: usize) {
    let child_id = &nodes[child_idx].id;
    let pos = children
        .iter()
        .position(|idx| nodes[*idx].id.precedes(child_id))
        .unwrap_or(children.len());
    children.insert(pos, child_idx);
}

fn rebuild_children(nodes: &mut [Node], node_indices: &HashMap<NodeId, usize>) -> Vec<usize> {
    let mut roots = Vec::new();

    for idx in 0..nodes.len() {
        match nodes[idx]
            .left
            .as_ref()
            .and_then(|left| node_indices.get(left).copied())
        {
            Some(parent_idx) => {
                let mut children = std::mem::take(&mut nodes[parent_idx].children);
                insert_child(&mut children, nodes, idx);
                nodes[parent_idx].children = children;
            }
            None => insert_child(&mut roots, nodes, idx),
        }
    }

    roots
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
        for idx in self.dfs_order() {
            if !self.nodes[idx].is_tombstone() {
                write!(f, "{}", self.nodes[idx].value)?;
            }
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
        assert_eq!(rga.to_string(), "");
    }

    #[test]
    fn remote_insert_at_head() {
        let mut rga = Rga::new();
        rga.insert(None, 'a', id("a", 1)).unwrap();
        assert_eq!(rga.to_string(), "a");
    }

    #[test]
    fn sequential_remote_inserts() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("a", 2);
        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a), 'b', b).unwrap();
        assert_eq!(rga.to_string(), "ab");
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

        assert_eq!(rga1.to_string(), rga2.to_string());
        assert_eq!(rga1.to_string(), "ba");
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

        assert_eq!(rga.to_string(), "xb");
    }

    #[test]
    fn missing_insert_anchor_is_reported() {
        let mut rga = Rga::new();
        let result = rga.insert(Some(id("missing", 1)), 'a', id("a", 1));
        assert_eq!(result, Err(RgaError::MissingDependency));
    }

    #[test]
    fn tree_exposes_anchor_children_and_traversal_order() {
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
