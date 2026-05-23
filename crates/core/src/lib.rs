//! CRDT-based collaborative text editing engine.
//!
//! The crate implements a Replicated Growable Array (RGA) that lets multiple
//! clients edit the same document at the same time and eventually agree on the
//! same content without any coordination.
//!
//! # Core abstractions
//!
//! [`Replica`] is the main entry point. Each connected client owns one. It
//! wraps an [`Rga`] and tracks which operations have already been applied so
//! that duplicates are safely ignored.
//!
//! Local edits go through [`Replica::local_insert`] and
//! [`Replica::local_delete`]. Both apply the change immediately and return an
//! [`Op`] that should be broadcast to every other client. Incoming remote ops
//! arrive through [`Replica::apply_remote`].
//!
//! The raw [`Rga`] is also public for cases where the higher-level replica
//! bookkeeping is not needed.

pub mod id;
mod node;
pub mod op;
pub mod replica;
pub mod rga;

pub use id::{NodeId, OperationId, ReplicaId, SessionId};
pub use op::{ApplyOutcome, Op};
pub use replica::Replica;
pub use rga::{Rga, RgaError, RgaTree, RgaTreeNode};

#[cfg(target_arch = "wasm32")]
mod wasm;
