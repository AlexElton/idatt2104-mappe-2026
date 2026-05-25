//! CRDT-based collaborative text editing engine.
//!
//! This crate contains the Replicated Growable Array (RGA) implementation.
//! It is built both as a normal Rust library for the backend and as
//! WebAssembly for the React frontend, so both sides apply exactly the same CRDT
//! logic.
//!
//! # Model
//!
//! The document is stored as single-character nodes. An insert operation names
//! the node to its left, and a delete operation turns a node into a tombstone
//! instead of removing it. Keeping tombstones makes later operations
//! that still reference the deleted node safe to apply.
//!
//! # Main workflow
//!
//! [`Replica`] is the public entry point for normal use. A replica owns an
//! [`Rga`], remembers which [`Op`] IDs have already been applied, and keeps an
//! operation log that can hydrate newly connected peers.
//!
//! Local edits go through [`Replica::local_insert`] and
//! [`Replica::local_delete`]. They update the local document immediately and
//! return an [`Op`] that can be sent to the server or to other clients. Incoming
//! operations are applied with [`Replica::apply_remote`] or
//! [`Replica::apply_remote_batch`].

pub mod id;
pub mod lww;
mod node;
pub mod op;
pub mod replica;
pub mod rga;

pub use id::{NodeId, OperationId, ReplicaId, SessionId};
pub use lww::LwwRegister;
pub use op::{ApplyOutcome, Op};
pub use replica::Replica;
pub use rga::{Rga, RgaError, RgaTree, RgaTreeNode};

#[cfg(target_arch = "wasm32")]
mod wasm;
