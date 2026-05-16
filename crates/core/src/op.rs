use serde::{Deserialize, Serialize};

use crate::{NodeId, OperationId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Op {
    Insert {
        left: Option<NodeId>,
        value: char,
        id: NodeId,
    },
    Delete {
        target: NodeId,
        id: OperationId,
    },
}

impl Op {
    pub fn id(&self) -> &OperationId {
        match self {
            Op::Insert { id, .. } | Op::Delete { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyOutcome {
    Applied,
    Duplicate,
    MissingDependency,
    Invalid,
}

impl ApplyOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            ApplyOutcome::Applied => "applied",
            ApplyOutcome::Duplicate => "duplicate",
            ApplyOutcome::MissingDependency => "missing_dependency",
            ApplyOutcome::Invalid => "invalid",
        }
    }
}
