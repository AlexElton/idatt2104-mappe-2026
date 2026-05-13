use crate::crdt::char_id::CharId;
use crate::crdt::op::Op;

#[derive(Debug, Clone)]
struct RgaChar {
    id: CharId,
    //none means tombstone
    value: Option<char>,
}

impl RgaChar {
    fn new(id: CharId, value: char) -> Self {
        Self { id, value: Some(value) }
    }

    fn is_visible(&self) -> bool {
        self.value.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct RGA {
    chars: Vec<RgaChar>,
    pub replica_id: u64,
    lamport: u64,
}

impl RGA {
    pub fn new(replica_id: u64) -> Self {
        Self {
            chars: Vec::new(),
            replica_id,
            lamport: 0,
        }
    }

    pub fn local_insert(&mut self, after_index: Option<usize>, value: char) -> Op {
        let after_id = after_index.map(|i| self.visible_id_at(i));
        self.lamport += 1;
        let id = CharId::new(self.lamport, self.replica_id);
        let op = Op::insert(id, value, after_id);
        self.apply_insert(id, value, after_id);
        op
    }

    pub fn local_delete(&mut self, index: usize) -> Op {
        let target = self.visible_id_at(index);
        let op = Op::delete(target, self.replica_id);
        self.apply_delete(target);
        op
    }


    pub fn apply(&mut self, op: &Op) {
        match op {
            Op::Insert { id, value, after } => {
                // Sync Lamport clock before checking for duplicates
                self.lamport = self.lamport.max(id.lamport) + 1;

                if self.find_index_by_id(*id).is_some() {
                    return; // already applied, skip
                }
                self.apply_insert(*id, *value, *after);
            }
            Op::Delete { target, .. } => {
                self.apply_delete(*target);
            }
        }
    }

    pub fn text(&self) -> String {
        self.chars
            .iter()
            .filter_map(|c| c.value)
            .collect()
    }


    pub fn len(&self) -> usize {
        self.chars.iter().filter(|c| c.is_visible()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn apply_insert(&mut self, id: CharId, value: char, after: Option<CharId>) {
        let start = match after {
            None => 0,
            Some(pred_id) => {
                match self.find_index_by_id(pred_id) {
                    Some(i) => i + 1,
                    None => self.chars.len(),
                }
            }
        };

        let mut insert_pos = start;
        while insert_pos < self.chars.len() {
            let existing = &self.chars[insert_pos];

            if existing.id < id {
                break;
            }

            insert_pos += 1;
        }

        self.chars.insert(insert_pos, RgaChar::new(id, value));
    }

    fn apply_delete(&mut self, target: CharId) {
        if let Some(i) = self.find_index_by_id(target) {
            self.chars[i].value = None; // tombstone
        }
    }

    fn find_index_by_id(&self, id: CharId) -> Option<usize> {
        self.chars.iter().position(|c| c.id == id)
    }

    fn visible_id_at(&self, index: usize) -> CharId {
        self.chars
            .iter()
            .filter(|c| c.is_visible())
            .nth(index)
            .expect("visible index out of bounds")
            .id
    }
}
