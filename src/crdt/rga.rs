use std::collections::HashMap;

use crate::crdt::{node::Node, op::Op, s4vector::S4Vector};

/// Replicated Growable Array (Roh et al. 2011).
///
/// Internally a linked list (via indices into `nodes`) combined with
/// a hash table keyed by s4vector (the SVI scheme, Section 5.4).
pub struct Rga {
    nodes:   Vec<Node>,
    hash:    HashMap<S4Vector, usize>, // SVI: s_k → index in `nodes`
    head:    Option<usize>,            // index of first node in linked list
    site_id: u64,
    session: u64,
    counter: u64, // monotonically increasing; used as a simplified vector clock
}

impl Rga {
    pub fn new(site_id: u64, session: u64) -> Self {
        Self {
            nodes: Vec::new(),
            hash: HashMap::new(),
            head: None,
            site_id,
            session,
            counter: 0,
        }
    }

    // -------------------------------------------------------------------------
    // Local operations — called by the local editor, return an Op to broadcast
    // -------------------------------------------------------------------------

    /// Insert `obj` after the `pos`-th visible character (0 = insert at head).
    /// Generates a S4Vector op with this site's id, applies it locally, returns it for broadcast.
    pub fn local_insert(&mut self, pos: usize, obj: char) -> Option<Op> {
        let left_s4v = self.findlist(pos).map(|idx| self.nodes[idx].s_k.clone());
        let s_k = self.next_s4vector();
        self.remote_insert(left_s4v.clone(), obj, s_k.clone());
        Some(Op::Insert { left: left_s4v, obj, s_k })
    }

    /// Delete the visible character at 0-indexed position pos (pos=0 = first char).
    pub fn local_delete(&mut self, pos: usize) -> Option<Op> {
        let target_idx = self.findlist(pos + 1)?; // findlist is 1-indexed
        let target = self.nodes[target_idx].s_k.clone();
        let s_k = self.next_s4vector();
        self.remote_delete(target.clone(), s_k.clone());
        Some(Op::Delete { target, s_k })
    }

    /// Replace the visible character at 0-indexed position pos with obj.
    pub fn local_update(&mut self, pos: usize, obj: char) -> Option<Op> {
        let target_idx = self.findlist(pos + 1)?;
        let target = self.nodes[target_idx].s_k.clone();
        let s_k = self.next_s4vector();
        self.remote_update(target.clone(), obj, s_k.clone());
        Some(Op::Update { target, obj, s_k })
    }

    // -------------------------------------------------------------------------
    // Remote operations — called when an Op arrives over WebSocket
    // -------------------------------------------------------------------------

    /// Algorithm 8 (Roh et al. 2011).
    /// Insert a new node after `left` (None = insert at head).
    /// Scan rightward, skipping nodes whose s_k has HIGHER priority than s_k,
    /// until we find one with lower priority — insert before it.
    pub fn remote_insert(&mut self, left: Option<S4Vector>, obj: char, s_k: S4Vector) {
        let new_idx = self.nodes.len();
        self.nodes.push(Node::new(obj, s_k.clone()));
        self.hash.insert(s_k.clone(), new_idx);

        let left_idx = left.and_then(|s4v| self.hash.get(&s4v).copied());

        let mut prev = left_idx;
        let mut cur = match left_idx {
            None      => self.head,
            Some(idx) => self.nodes[idx].link,
        };

        // Skip nodes with strictly higher priority (concurrent insert at same position).
        // Stop at the first node with lower priority — insert before it.
        while let Some(c) = cur {
            if self.nodes[c].s_k.precedes(&s_k) {
                break; // c.s_k lower priority → insert before c
            }
            prev = Some(c);
            cur  = self.nodes[c].link;
        }

        self.nodes[new_idx].link = cur;
        match prev {
            None    => self.head = Some(new_idx),
            Some(p) => self.nodes[p].link = Some(new_idx),
        }
    }

    /// Algorithm 9 (Roh et al. 2011).
    /// Tombstone the node identified by `target`; record `s_o` as new s_p.
    pub fn remote_delete(&mut self, target: S4Vector, s_o: S4Vector) {
        if let Some(&idx) = self.hash.get(&target) {
            self.nodes[idx].tombstone = true;
            self.nodes[idx].s_p = s_o;
        }
    }

    /// Algorithm 10 (Roh et al. 2011).
    /// Replace obj if s_o has HIGHER priority than the node's current s_p.
    /// If the node is already tombstoned (Delete won), do nothing.
    pub fn remote_update(&mut self, target: S4Vector, obj: char, s_o: S4Vector) {
        if let Some(&idx) = self.hash.get(&target) {
            if self.nodes[idx].is_tombstone() {
                return; // Delete wins over Update
            }
            if self.nodes[idx].s_p.precedes(&s_o) {
                self.nodes[idx].obj = obj;
                self.nodes[idx].s_p = s_o;
            }
        }
    }

    /// Dispatch a remote Op to the correct algorithm.
    pub fn apply(&mut self, op: Op) {
        match op {
            Op::Insert { left, obj, s_k }    => self.remote_insert(left, obj, s_k),
            Op::Delete { target, s_k }        => self.remote_delete(target, s_k),
            Op::Update { target, obj, s_k }   => self.remote_update(target, obj, s_k),
        }
    }

    /// Walk the linked list and collect all non-tombstone characters in order.
    pub fn to_string(&self) -> String {
        let mut result = String::new();
        let mut cur = self.head;
        while let Some(idx) = cur {
            if !self.nodes[idx].is_tombstone() {
                result.push(self.nodes[idx].obj);
            }
            cur = self.nodes[idx].link;
        }
        result
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    /// Generate the next s4vector for a locally-issued operation.
    /// Simplified: uses local counter for both `sum` and `seq` (no full vector clock).
    fn next_s4vector(&mut self) -> S4Vector {
        self.counter += 1;
        S4Vector::new(self.session, self.site_id, self.counter, self.counter)
    }

    /// Algorithm 4 (findlist): return the index of the pos-th visible node (1-indexed).
    /// pos=0 → None (virtual head, used as left=None for insert-at-head).
    fn findlist(&self, pos: usize) -> Option<usize> {
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

    /// Returns Insert + Delete ops to replay the full current document state
    /// into a fresh RGA. Tombstoned nodes are included (future ops reference them
    /// by s_k), and each tombstone is followed by its Delete op.
    pub fn hydration_ops(&self) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut prev: Option<S4Vector> = None;
        let mut cur = self.head;
        while let Some(idx) = cur {
            let node = &self.nodes[idx];
            ops.push(Op::Insert {
                left: prev.clone(),
                obj:  node.obj,
                s_k:  node.s_k.clone(),
            });
            if node.tombstone {
                ops.push(Op::Delete {
                    target: node.s_k.clone(),
                    s_k:    node.s_p.clone(),
                });
            }
            prev = Some(node.s_k.clone());
            cur  = node.link;
        }
        ops
    }
}

// -------------------------------------------------------------------------
// Tests — Student 1 owns these
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn s4v(ssn: u64, sid: u64, sum: u64) -> S4Vector {
        S4Vector::new(ssn, sid, sum, sum)
    }

    // -----------------------------------------------------------------------
    // Task 2 tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_rga_is_empty_string() {
        let rga = Rga::new(1, 0);
        assert_eq!(rga.to_string(), "");
    }

    #[test]
    fn test_single_remote_insert_at_head() {
        let mut rga = Rga::new(1, 0);
        rga.remote_insert(None, 'a', s4v(0, 1, 1));
        assert_eq!(rga.to_string(), "a");
    }

    #[test]
    fn test_sequential_remote_inserts() {
        let mut rga = Rga::new(1, 0);
        let s_a = s4v(0, 1, 1);
        let s_b = s4v(0, 1, 2);
        rga.remote_insert(None, 'a', s_a.clone());
        rga.remote_insert(Some(s_a), 'b', s_b);
        assert_eq!(rga.to_string(), "ab");
    }

    /// dOPT puzzle — concurrent inserts at the same position converge.
    /// Site 1 (sid=1) inserts 'a', site 2 (sid=2) inserts 'b', both at head.
    /// Higher sid wins → 'b' (sid=2) appears before 'a' (sid=1).
    #[test]
    fn test_concurrent_insert_same_position() {
        let s_a = s4v(0, 1, 1); // lower priority
        let s_b = s4v(0, 2, 1); // higher priority (sid 2 > 1)

        // Order 1: insert 'a' first, then 'b'
        let mut rga1 = Rga::new(1, 0);
        rga1.remote_insert(None, 'a', s_a.clone());
        rga1.remote_insert(None, 'b', s_b.clone());

        // Order 2: insert 'b' first, then 'a'
        let mut rga2 = Rga::new(2, 0);
        rga2.remote_insert(None, 'b', s_b.clone());
        rga2.remote_insert(None, 'a', s_a.clone());

        assert_eq!(rga1.to_string(), rga2.to_string());
        assert_eq!(rga1.to_string(), "ba"); // b (sid=2) wins → appears first
    }

    // -----------------------------------------------------------------------
    // Task 3–5 tests — filled in later tasks
    // -----------------------------------------------------------------------

    #[test]
    fn test_single_insert() {
        let mut rga = Rga::new(1, 0);
        let op = rga.local_insert(0, 'a').unwrap();
        assert_eq!(rga.to_string(), "a");
        match op {
            Op::Insert { left, obj, s_k } => {
                assert_eq!(left, None);
                assert_eq!(obj, 'a');
                assert_eq!(s_k.sid, 1);
            }
            _ => panic!("expected Insert op"),
        }
    }

    #[test]
    fn test_concurrent_insert_same_position_local() {
        let mut rga1 = Rga::new(1, 0);
        let mut rga2 = Rga::new(2, 0);

        let op1 = rga1.local_insert(0, 'a').unwrap(); // s_k.sid = 1
        let op2 = rga2.local_insert(0, 'b').unwrap(); // s_k.sid = 2

        rga1.apply(op2.clone());
        rga2.apply(op1.clone());

        assert_eq!(rga1.to_string(), rga2.to_string());
        assert_eq!(rga1.to_string(), "ba"); // sid=2 higher priority → first
    }

    /// Example 2 from the paper: delete a node, then a concurrent insert lands
    /// after the same (now-tombstoned) node. The insert must still be placed correctly.
    #[test]
    fn test_delete_then_concurrent_insert() {
        let mut rga1 = Rga::new(1, 0);
        let mut rga2 = Rga::new(2, 0);

        let op_a = rga1.local_insert(0, 'a').unwrap();
        let op_b = rga1.local_insert(1, 'b').unwrap();
        rga2.apply(op_a);
        rga2.apply(op_b);
        assert_eq!(rga1.to_string(), "ab");
        assert_eq!(rga2.to_string(), "ab");

        let del_op = rga1.local_delete(0).unwrap();
        let ins_op = rga2.local_insert(1, 'x').unwrap(); // insert after 'a' (pos 1)

        rga1.apply(ins_op);
        rga2.apply(del_op);

        assert_eq!(rga1.to_string(), rga2.to_string());
        // 'b' has s_k.sum=2; 'x' has s_k.sum=1 → b has higher priority → b before x
        assert_eq!(rga1.to_string(), "bx");
    }

    #[test]
    fn test_convergence_two_orderings() {
        let mut rga1 = Rga::new(1, 0);
        let mut rga2 = Rga::new(2, 0);

        let op1 = rga1.local_insert(0, 'a').unwrap();
        let op2 = rga2.local_insert(0, 'b').unwrap();

        rga1.apply(op2.clone());
        rga2.apply(op1.clone());

        assert_eq!(rga1.to_string(), rga2.to_string());

        let del1 = rga1.local_delete(1).unwrap(); // "ba" → delete pos 1 = 'a'
        let del2 = rga2.local_delete(0).unwrap(); // "ba" → delete pos 0 = 'b'
        rga1.apply(del2);
        rga2.apply(del1);

        assert_eq!(rga1.to_string(), rga2.to_string());
        assert_eq!(rga1.to_string(), "");
    }

    /// Concurrent Delete and Update on the same node: Delete wins (Algorithm 10).
    #[test]
    fn test_update_loses_to_delete() {
        let mut rga1 = Rga::new(1, 0);
        let mut rga2 = Rga::new(2, 0);

        let op_a = rga1.local_insert(0, 'a').unwrap();
        rga2.apply(op_a);

        let del_op = rga1.local_delete(0).unwrap();
        let upd_op = rga2.local_update(0, 'z').unwrap();

        rga1.apply(upd_op); // site 1 deleted first; update must lose
        rga2.apply(del_op); // site 2 updated first; delete must win

        assert_eq!(rga1.to_string(), "");
        assert_eq!(rga2.to_string(), "");
    }
}
