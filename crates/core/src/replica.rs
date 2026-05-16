use std::collections::HashSet;

use crate::{ApplyOutcome, NodeId, Op, OperationId, ReplicaId, Rga, RgaError, SessionId};

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

    pub fn local_insert(&mut self, pos: usize, value: char) -> Option<Op> {
        let left = self.rga.left_id_for_insert(pos);
        if pos > 0 && left.is_none() {
            return None;
        }

        let id = self.next_id();
        let op = Op::Insert {
            left,
            value,
            id: id.clone(),
        };

        self.rga
            .insert(
                match &op {
                    Op::Insert { left, .. } => left.clone(),
                    Op::Delete { .. } => unreachable!(),
                },
                value,
                id.clone(),
            )
            .ok()?;
        self.mark_applied(op.clone());
        Some(op)
    }

    pub fn local_delete(&mut self, pos: usize) -> Option<Op> {
        let target = self.rga.target_id_for_delete(pos)?;
        let id = self.next_id();
        let op = Op::Delete {
            target: target.clone(),
            id: id.clone(),
        };

        self.rga.delete(target, id).ok()?;
        self.mark_applied(op.clone());
        Some(op)
    }

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

    pub fn apply_remote_batch(&mut self, ops: impl IntoIterator<Item = Op>) -> Vec<ApplyOutcome> {
        ops.into_iter().map(|op| self.apply_remote(op)).collect()
    }

    pub fn text(&self) -> String {
        self.rga.text()
    }

    pub fn hydration_ops(&self) -> Vec<Op> {
        self.op_log.clone()
    }

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
}
