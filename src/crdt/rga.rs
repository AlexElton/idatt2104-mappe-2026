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
    pub fn local_insert(&mut self, pos: usize, obj: char) -> Option<Op> {
        todo!("findlist(pos) to get left cobject, build Op::Insert with next_s4vector()")
    }

    /// Delete the `pos`-th visible character (1-indexed).
    pub fn local_delete(&mut self, pos: usize) -> Option<Op> {
        todo!("findlist(pos) to get target node, build Op::Delete with next_s4vector()")
    }

    /// Replace the `pos`-th visible character with `obj`.
    pub fn local_update(&mut self, pos: usize, obj: char) -> Option<Op> {
        todo!("findlist(pos) to get target node, build Op::Update with next_s4vector()")
    }

    // -------------------------------------------------------------------------
    // Remote operations — called when an Op arrives over WebSocket
    // -------------------------------------------------------------------------

    /// Algorithm 8 from Roh et al. 2011.
    /// `left`  — s_k of the left cobject (None = insert at head).
    /// `obj`   — character to insert.
    /// `s_k`   — this node's permanent identity (stored in the new Node).
    pub fn remote_insert(&mut self, left: Option<S4Vector>, obj: char, s_k: S4Vector) {
        todo!(
            "
            (i)  Find left cobject in hash table via SVI.
            (ii) Create new Node, add to hash table.
            (iii) Scan linked list from left cobject; skip Nodes whose s_k succeeds ins.s_k.
            (iv) Link new Node in front of the first non-succeeding Node.
            See Algorithm 8 in the paper.
            "
        )
    }

    /// Algorithm 9 from Roh et al. 2011.
    /// `target` — s_k of the node to delete.
    /// `s_o`    — this Delete op's own s4vector (stored as the node's new s_p).
    pub fn remote_delete(&mut self, target: S4Vector, s_o: S4Vector) {
        todo!(
            "
            Find target in hash table.
            Set node.obj = None (tombstone) and node.s_p = s_o.
            See Algorithm 9 in the paper.
            "
        )
    }

    /// Algorithm 10 from Roh et al. 2011.
    /// `target` — s_k of the node to update.
    /// `obj`    — new character.
    /// `s_o`    — this Update op's own s4vector (compared against node's s_p for precedence).
    pub fn remote_update(&mut self, target: S4Vector, obj: char, s_o: S4Vector) {
        todo!(
            "
            Find target in hash table.
            If node is tombstone: do nothing (always Update --> Delete).
            If s_o precedes node.s_p: do nothing (lost the race).
            Otherwise: set node.obj = obj, node.s_p = s_o.
            See Algorithm 10 in the paper.
            "
        )
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
        todo!("walk self.head via node.link, push node.obj.unwrap() for non-tombstones")
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

    /// Algorithm 4 (findlist): find the nth visible (non-tombstone) node.
    /// Returns the node's index in `self.nodes`, or None if out of bounds.
    /// pos = 0 returns None (represents the virtual head / "insert before everything").
    fn findlist(&self, pos: usize) -> Option<usize> {
        todo!("walk self.head via node.link, skip tombstones, count to pos")
    }
}

// -------------------------------------------------------------------------
// Tests — Student 1 owns these
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_insert() {
        todo!()
    }

    /// dOPT puzzle — Example 1 from the paper.
    /// Three concurrent inserts at the same position must converge.
    #[test]
    fn test_concurrent_insert_same_position() {
        todo!()
    }

    /// Example 2 from the paper.
    #[test]
    fn test_delete_then_concurrent_insert() {
        todo!()
    }

    /// Apply ops in two different orders, assert same final state.
    #[test]
    fn test_convergence_two_orderings() {
        todo!()
    }

    /// A concurrent Update and Delete on the same node — Delete must win.
    #[test]
    fn test_update_loses_to_delete() {
        todo!()
    }
}
