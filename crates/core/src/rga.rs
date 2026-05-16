use std::collections::HashMap;

use crate::{node::Node, NodeId, OperationId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RgaError {
    DuplicateNode,
    MissingDependency,
    Invalid,
}

/// Replicated Growable Array linked-list storage and deterministic merge logic.
///
/// `Replica` owns operation identity, duplicate detection, and op history. `Rga`
/// only stores nodes and applies already-identified insert/delete operations.
#[derive(Debug, Clone, Default)]
pub struct Rga {
    nodes: Vec<Node>,
    hash: HashMap<NodeId, usize>,
    head: Option<usize>,
}

impl Rga {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        left: Option<NodeId>,
        value: char,
        id: NodeId,
    ) -> Result<(), RgaError> {
        if self.hash.contains_key(&id) {
            return Err(RgaError::DuplicateNode);
        }

        let left_idx = match left {
            Some(left_id) => Some(
                self.hash
                    .get(&left_id)
                    .copied()
                    .ok_or(RgaError::MissingDependency)?,
            ),
            None => None,
        };

        let new_idx = self.nodes.len();
        self.nodes.push(Node::new(value, id.clone()));
        self.hash.insert(id.clone(), new_idx);

        let mut prev = left_idx;
        let mut cur = match left_idx {
            None => self.head,
            Some(idx) => self.nodes[idx].link,
        };

        while let Some(cur_idx) = cur {
            if self.nodes[cur_idx].id.precedes(&id) {
                break;
            }
            prev = Some(cur_idx);
            cur = self.nodes[cur_idx].link;
        }

        self.nodes[new_idx].link = cur;
        match prev {
            None => self.head = Some(new_idx),
            Some(prev_idx) => self.nodes[prev_idx].link = Some(new_idx),
        }

        Ok(())
    }

    pub fn delete(&mut self, target: NodeId, deleted_by: OperationId) -> Result<(), RgaError> {
        if target == deleted_by {
            return Err(RgaError::Invalid);
        }

        let idx = self
            .hash
            .get(&target)
            .copied()
            .ok_or(RgaError::MissingDependency)?;

        self.nodes[idx].tombstone = true;
        self.nodes[idx].deleted_by = Some(deleted_by);
        Ok(())
    }

    pub fn contains_node(&self, id: &NodeId) -> bool {
        self.hash.contains_key(id)
    }

    pub fn left_id_for_insert(&self, pos: usize) -> Option<NodeId> {
        self.find_visible(pos).map(|idx| self.nodes[idx].id.clone())
    }

    pub fn target_id_for_delete(&self, pos: usize) -> Option<NodeId> {
        self.find_visible(pos + 1)
            .map(|idx| self.nodes[idx].id.clone())
    }

    fn find_visible(&self, pos: usize) -> Option<usize> {
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

    pub fn text(&self) -> String {
        self.to_string()
    }
}

impl std::fmt::Display for Rga {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut cur = self.head;
        while let Some(idx) = cur {
            if !self.nodes[idx].is_tombstone() {
                write!(f, "{}", self.nodes[idx].value)?;
            }
            cur = self.nodes[idx].link;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(replica_id: &str, lamport: u64) -> OperationId {
        OperationId::new(
            "session".to_string(),
            replica_id.to_string(),
            lamport,
            lamport,
        )
    }

    #[test]
    fn empty_rga_is_empty_string() {
        let rga = Rga::new();
        assert_eq!(rga.text(), "");
    }

    #[test]
    fn remote_insert_at_head() {
        let mut rga = Rga::new();
        rga.insert(None, 'a', id("a", 1)).unwrap();
        assert_eq!(rga.text(), "a");
    }

    #[test]
    fn sequential_remote_inserts() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("a", 2);
        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a), 'b', b).unwrap();
        assert_eq!(rga.text(), "ab");
    }

    #[test]
    fn concurrent_insert_same_position_converges() {
        let a = id("a", 1);
        let b = id("b", 1);

        let mut rga1 = Rga::new();
        rga1.insert(None, 'a', a.clone()).unwrap();
        rga1.insert(None, 'b', b.clone()).unwrap();

        let mut rga2 = Rga::new();
        rga2.insert(None, 'b', b).unwrap();
        rga2.insert(None, 'a', a).unwrap();

        assert_eq!(rga1.text(), rga2.text());
        assert_eq!(rga1.text(), "ba");
    }

    #[test]
    fn delete_hides_node_but_keeps_anchor_for_concurrent_insert() {
        let mut rga = Rga::new();
        let a = id("a", 1);
        let b = id("a", 2);
        let x = id("b", 3);

        rga.insert(None, 'a', a.clone()).unwrap();
        rga.insert(Some(a.clone()), 'b', b).unwrap();
        rga.delete(a.clone(), id("a", 3)).unwrap();
        rga.insert(Some(a), 'x', x).unwrap();

        assert_eq!(rga.text(), "xb");
    }

    #[test]
    fn missing_insert_anchor_is_reported() {
        let mut rga = Rga::new();
        let result = rga.insert(Some(id("missing", 1)), 'a', id("a", 1));
        assert_eq!(result, Err(RgaError::MissingDependency));
    }
}
