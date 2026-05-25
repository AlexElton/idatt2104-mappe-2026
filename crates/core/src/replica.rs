//! Per-client replica state.
//!
//! A [`Replica`] is the API the application code usually interacts with.
//! It wraps an [`Rga`], tracks operation IDs that have already been applied,
//! assigns Lamport IDs to local edits, and keeps an operation log.
//!
//! # Editing
//!
//! [`Replica::local_insert`] inserts before the character currently at
//! `pos` (or at the end when `pos` is the text length).
//!
//! [`Replica::local_delete`] deletes the visible character at
//! `pos`. Both methods apply the edit immediately and return an [`Op`] that the
//! caller can send to other replicas.
//!
//! # Remote operations
//!
//! Remote operations go through [`Replica::apply_remote`] or
//! [`Replica::apply_remote_batch`]. Duplicates are ignored. Missing dependencies
//! are reported so the caller waith for the dependencies and retry when they arrive.
//!
//! # Garbage collection
//!
//! [`Replica::clear_deleted_nodes`] removes tombstones and
//! rebuilds that log from surviving inserts, which is useful for the demo but is
//! not safe because some clients will be outof sync if they're not connected.

use std::collections::HashSet;

use crate::{ApplyOutcome, NodeId, Op, OperationId, ReplicaId, Rga, RgaError, RgaTree, SessionId};

/// A single client's view of a shared document.
#[derive(Debug, Clone)]
pub struct Replica {
    rga: Rga,
    applied_ops: HashSet<OperationId>,
    op_log: Vec<Op>,
    replica_id: ReplicaId,
    session_id: SessionId,
    lamport: u64,
    seq: u64,
}

impl Replica {
    /// Creates an empty replica with the given identity.
    pub fn new(replica_id: ReplicaId, session_id: SessionId) -> Self {
        Self {
            rga: Rga::new(),
            applied_ops: HashSet::new(),
            op_log: Vec::new(),
            replica_id,
            session_id,
            lamport: 0,
            seq: 0,
        }
    }

    /// Inserts `value` at visible position `pos` and returns the op to broadcast.
    pub fn local_insert(&mut self, pos: usize, value: char) -> Option<Op> {
        let left = self.rga.get_node_id_by_position(pos);
        if pos > 0 && left.is_none() {
            return None;
        }

        let id = self.next_id();
        let op = Op::Insert {
            left: left.clone(),
            value,
            id: id.clone(),
        };

        self.rga.insert(left, value, id).ok()?;
        self.mark_applied(op.clone());
        Some(op)
    }

    /// Deletes the visible character at position `pos` and returns the op to broadcast.
    pub fn local_delete(&mut self, pos: usize) -> Option<Op> {
        let target = self.rga.get_node_id_by_position(pos.checked_add(1)?)?;
        let id = self.next_id();
        let op = Op::Delete {
            target: target.clone(),
            id: id.clone(),
        };

        self.rga.delete(target, id).ok()?;
        self.mark_applied(op.clone());
        Some(op)
    }

    /// Applies an operation received from another replica.
    ///
    /// Returns [`ApplyOutcome::Duplicate`] if this op has already been seen.
    /// Returns [`ApplyOutcome::MissingDependency`] if the op references a node
    /// not yet present in the RGA. The caller is responsible for buffering and
    /// retrying once the dependency arrives.
    pub fn apply_remote(&mut self, op: Op) -> ApplyOutcome {
        if self.applied_ops.contains(op.id()) {
            return ApplyOutcome::Duplicate;
        }

        let result = match &op {
            Op::Insert { left, value, id } => {
                if left.as_ref().is_some_and(|left_id| left_id == id) {
                    Err(RgaError::Invalid)
                } else {
                    self.rga.insert(left.clone(), *value, id.clone())
                }
            }
            Op::Delete { target, id } => self.rga.delete(target.clone(), id.clone()),
        };

        match result {
            Ok(()) => {
                self.observe(op.id());
                self.mark_applied(op);
                ApplyOutcome::Applied
            }
            Err(RgaError::DuplicateNode) => ApplyOutcome::Invalid,
            Err(RgaError::MissingDependency) => ApplyOutcome::MissingDependency,
            Err(RgaError::Invalid) => ApplyOutcome::Invalid,
        }
    }

    /// Applies a batch of remote operations in order, returning one [`ApplyOutcome`] per op.
    pub fn apply_remote_batch(&mut self, ops: impl IntoIterator<Item = Op>) -> Vec<ApplyOutcome> {
        ops.into_iter().map(|op| self.apply_remote(op)).collect()
    }

    /// Returns the current visible text.
    pub fn text(&self) -> String {
        self.rga.to_string()
    }

    /// Returns all operations needed to reconstruct the current document state.
    ///
    /// A newly connected peer can apply this batch and end up with the same
    /// text.
    pub fn hydration_ops(&self) -> Vec<Op> {
        self.op_log.clone()
    }

    /// Returns a full snapshot of the RGA list, including tombstones.
    pub fn rga_tree(&self) -> RgaTree {
        self.rga.tree()
    }

    /// Removes tombstoned nodes from the RGA and rebuilds the op log.
    ///
    /// Returns the number of nodes removed.
    pub fn clear_deleted_nodes(&mut self) -> usize {
        let removed = self.rga.clear_tombstones();
        if removed > 0 {
            self.op_log = self
                .rga
                .tree()
                .nodes
                .into_iter()
                .map(|node| Op::Insert {
                    left: node.left,
                    value: node.value,
                    id: node.id,
                })
                .collect();
        }
        removed
    }

    /// Returns `true` if a node with the given ID exists, including tombstoned nodes.
    pub fn has_node(&self, id: &NodeId) -> bool {
        self.rga.contains_node(id)
    }

    fn next_id(&mut self) -> OperationId {
        self.lamport += 1;
        self.seq += 1;
        OperationId::new(
            self.session_id.clone(),
            self.replica_id.clone(),
            self.lamport,
            self.seq,
        )
    }

    fn observe(&mut self, id: &OperationId) {
        self.lamport = self.lamport.max(id.lamport);
    }

    fn mark_applied(&mut self, op: Op) {
        self.applied_ops.insert(op.id().clone());
        self.op_log.push(op);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replica(name: &str) -> Replica {
        Replica::new(name.to_string(), format!("{name}-session"))
    }

    #[test]
    fn local_insert_generates_operation_and_updates_text() {
        let mut replica = replica("a");

        let op = replica.local_insert(0, 'x').unwrap();

        assert_eq!(replica.text(), "x");
        match op {
            Op::Insert { left, value, id } => {
                assert_eq!(left, None);
                assert_eq!(value, 'x');
                assert_eq!(id.replica_id, "a");
                assert_eq!(id.lamport, 1);
                assert_eq!(id.seq, 1);
            }
            Op::Delete { .. } => panic!("expected insert"),
        }
    }

    #[test]
    fn local_delete_generates_operation_and_updates_text() {
        let mut replica = replica("a");
        replica.local_insert(0, 'x').unwrap();

        let op = replica.local_delete(0).unwrap();

        assert_eq!(replica.text(), "");
        assert!(matches!(op, Op::Delete { .. }));
    }

    #[test]
    fn remote_duplicate_is_reported_once() {
        let mut source = replica("a");
        let op = source.local_insert(0, 'x').unwrap();
        let mut target = replica("b");

        assert_eq!(target.apply_remote(op.clone()), ApplyOutcome::Applied);
        assert_eq!(target.apply_remote(op), ApplyOutcome::Duplicate);
        assert_eq!(target.text(), "x");
    }

    #[test]
    fn missing_dependency_is_reported() {
        let mut source = replica("a");
        let anchor = source.local_insert(0, 'a').unwrap();
        let Op::Insert { id: anchor_id, .. } = anchor else {
            panic!("expected insert");
        };
        let op = Op::Insert {
            left: Some(anchor_id),
            value: 'x',
            id: OperationId::new("session".to_string(), "b".to_string(), 1, 1),
        };
        let mut target = replica("b");

        assert_eq!(target.apply_remote(op), ApplyOutcome::MissingDependency);
        assert_eq!(target.text(), "");
    }

    #[test]
    fn batch_reports_duplicates_in_order() {
        let mut source = replica("a");
        let op = source.local_insert(0, 'x').unwrap();
        let mut target = replica("b");

        let outcomes = target.apply_remote_batch(vec![op.clone(), op]);

        assert_eq!(
            outcomes,
            vec![ApplyOutcome::Applied, ApplyOutcome::Duplicate]
        );
    }

    #[test]
    fn hydration_ops_replay_document() {
        let mut source = replica("a");
        source.local_insert(0, 'a').unwrap();
        source.local_insert(1, 'b').unwrap();
        source.local_delete(0).unwrap();

        let mut target = replica("b");
        target.apply_remote_batch(source.hydration_ops());

        assert_eq!(target.text(), "b");
    }

    #[test]
    fn concurrent_insert_same_position_converges() {
        let mut a = replica("a");
        let mut b = replica("b");

        let op_a = a.local_insert(0, 'a').unwrap();
        let op_b = b.local_insert(0, 'b').unwrap();

        assert_eq!(a.apply_remote(op_b), ApplyOutcome::Applied);
        assert_eq!(b.apply_remote(op_a), ApplyOutcome::Applied);
        assert_eq!(a.text(), b.text());
        assert_eq!(a.text(), "ba");
    }

    #[test]
    fn delete_then_concurrent_insert_keeps_tombstone_anchor() {
        let mut a = replica("a");
        let mut b = replica("b");

        let op_a = a.local_insert(0, 'a').unwrap();
        let op_b = a.local_insert(1, 'b').unwrap();
        b.apply_remote_batch(vec![op_a, op_b]);

        let delete_a = a.local_delete(0).unwrap();
        let insert_x = b.local_insert(1, 'x').unwrap();

        a.apply_remote(insert_x);
        b.apply_remote(delete_a);

        assert_eq!(a.text(), b.text());
        assert_eq!(a.text(), "xb");
    }

    #[test]
    fn rga_tree_reflects_applied_ops() {
        let mut replica = replica("a");
        replica.local_insert(0, 'a').unwrap();
        replica.local_insert(1, 'b').unwrap();
        replica.local_delete(0).unwrap();

        let tree = replica.rga_tree();

        assert_eq!(tree.text, "b");
        assert_eq!(tree.nodes.len(), 2);
        assert!(tree.nodes[0].tombstone);
        assert_eq!(tree.nodes[1].value, 'b');
    }

    #[test]
    fn clear_deleted_nodes_prunes_tombstones() {
        let mut replica = replica("a");
        replica.local_insert(0, 'a').unwrap();
        replica.local_insert(1, 'b').unwrap();
        replica.local_delete(0).unwrap();

        let removed = replica.clear_deleted_nodes();

        assert_eq!(removed, 1);
        assert_eq!(replica.text(), "b");
        assert_eq!(replica.rga_tree().nodes.len(), 1);
        assert_eq!(replica.hydration_ops().len(), 1);
    }
}
