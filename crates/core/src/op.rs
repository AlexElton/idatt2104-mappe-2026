//! Operation types sent between replicas.
//!
//! A local edit produces an [`Op`] that is sent to the backend and broadcast to
//! the other clients. Remote replicas apply it with
//! [`crate::Replica::apply_remote`], which returns an [`ApplyOutcome`] describing
//! whether the operation changed the document.

use serde::{Deserialize, Serialize};

use crate::{NodeId, OperationId};

/// An operation produced by a local edit and sent to remote replicas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Op {
    /// Insert `value` to the right of `left`. `left: None` means head of document.
    Insert {
        left: Option<NodeId>,
        value: char,
        id: NodeId,
    },
    /// Mark the node identified by `target` as deleted.
    Delete { target: NodeId, id: OperationId },
}

impl Op {
    /// Returns the unique ID of this operation.
    pub fn id(&self) -> &OperationId {
        match self {
            Op::Insert { id, .. } | Op::Delete { id, .. } => id,
        }
    }
}

/// Result of applying a remote operation via [`crate::Replica::apply_remote`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyOutcome {
    /// The operation was new and applied successfully.
    Applied,
    /// The operation had already been applied; it was safely ignored.
    Duplicate,
    /// The operation references a node not yet present on this replica.
    MissingDependency,
    /// The operation is structurally invalid (e.g. an insert referencing
    /// itself as its own left anchor).
    Invalid,
}

impl ApplyOutcome {
    /// Returns the string used by the WebAssembly API.
    pub fn as_str(self) -> &'static str {
        match self {
            ApplyOutcome::Applied => "applied",
            ApplyOutcome::Duplicate => "duplicate",
            ApplyOutcome::MissingDependency => "missing_dependency",
            ApplyOutcome::Invalid => "invalid",
        }
    }
}
