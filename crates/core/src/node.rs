use crate::crdt::s4vector::S4Vector;

/// A single node in the RGA linked list.
///
/// `obj`       — stored character; always present (tombstones keep their original char).
/// `tombstone` — true means this node has been deleted (visible only to Algorithm 8).
/// `s_k`       — immutable S4Vector set on Insert; used as SVI hash key and precedence key.
/// `s_p`       — mutable S4Vector; updated by Delete/Update for precedence (Algorithms 9 & 10).
/// `link`      — index of next node in linked list (None = tail).
/// `next`      — index of next node in SVI hash chain (separate chaining, unused for now).
#[derive(Debug, Clone)]
pub struct Node {
    pub obj:       char,
    pub tombstone: bool,
    pub s_k:       S4Vector,
    pub s_p:       S4Vector,
    pub link:      Option<usize>,
    pub next:      Option<usize>,
}

impl Node {
    pub fn new(obj: char, s_k: S4Vector) -> Self {
        Self {
            obj,
            tombstone: false,
            s_p: s_k.clone(),
            s_k,
            link: None,
            next: None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}
