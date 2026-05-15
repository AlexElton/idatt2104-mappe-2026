use serde::{Deserialize, Serialize};

use crate::crdt::s4vector::S4Vector;

/// An operation that can be applied to an RGA.
/// These are broadcast over WebSocket to remote sites.
///
/// Insert: add a new character after `left` (None = at head).
///   `s_k` is the new node's permanent identity (used as SVI hash key).
///
/// Delete: mark the node identified by `target` as a tombstone.
///   `s_k` is this Delete operation's own s4vector (stored as new s_p on the node).
///
/// Update: replace the object in node `target` if this op has higher precedence.
///   `s_k` is this Update operation's own s4vector (compared against node's s_p).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Op {
    Insert {
        left: Option<S4Vector>,
        obj: char,
        s_k: S4Vector,
    },
    Delete {
        target: S4Vector,
        s_k: S4Vector,
    },
    Update {
        target: S4Vector,
        obj: char,
        s_k: S4Vector,
    },
}

/// WebSocket wire envelope — wraps an Op with the originating site's ID.
/// The server stamps `site_id` before broadcasting so clients know the source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub site_id: u64,
    pub op: Op,
}
