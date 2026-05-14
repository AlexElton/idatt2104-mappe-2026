use crate::crdt::s4vector::S4Vector;

/// A single node in the RGA linked list.
///
/// `obj`  — the stored character; None means the node is a tombstone (deleted).
/// `s_k`  — immutable s4vector set on Insert; used as the SVI hash key and
///           for precedence ordering among concurrent Inserts (Algorithm 8).
/// `s_p`  — mutable s4vector tracking the last eoperation on this node;
///           updated by Delete and Update for precedence (Algorithms 9 & 10).
/// `link` — index of the next node in the linked list (None = list tail).
/// `next` — index of the next node in the SVI hash table chain (separate chaining).
#[derive(Debug, Clone)]
pub struct Node {
    pub obj:  Option<char>,
    pub s_k:  S4Vector,
    pub s_p:  S4Vector,
    pub link: Option<usize>,
    pub next: Option<usize>,
}

impl Node {
    pub fn new(obj: char, s_k: S4Vector) -> Self {
        Self {
            obj:  Some(obj),
            s_p:  s_k.clone(),
            s_k,
            link: None,
            next: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.obj.is_none()
    }
}
