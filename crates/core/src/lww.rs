//! Last-writer-wins register CRDT.
//!
//! The editor document itself uses the RGA in [`crate::rga`], but LWW is a useful
//! smaller CRDT for metadata-style values where only the newest value should be
//! kept. Examples are document title, selected color, or simple presence fields.

use crate::OperationId;

/// A register that keeps the value with the greatest operation ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LwwRegister<T> {
    inner: Option<(T, OperationId)>,
}

impl<T> LwwRegister<T> {
    /// Creates an empty register.
    pub fn new() -> Self {
        Self { inner: None }
    }

    /// Returns the currently visible value, if any.
    pub fn value(&self) -> Option<&T> {
        self.inner.as_ref().map(|(value, _)| value)
    }

    /// Returns the timestamp of the currently visible value, if any.
    pub fn timestamp(&self) -> Option<&OperationId> {
        self.inner.as_ref().map(|(_, timestamp)| timestamp)
    }

    /// Writes a value with the given timestamp.
    ///
    /// Returns `true` when the write became the visible value. Older writes and
    /// exact duplicate timestamps are ignored.
    pub fn set(&mut self, value: T, timestamp: OperationId) -> bool {
        if self
            .timestamp()
            .is_some_and(|current| current >= &timestamp)
        {
            return false;
        }

        self.inner = Some((value, timestamp));
        true
    }

    /// Merges another register into this one.
    ///
    /// Returns `true` when the merge changed the visible value.
    pub fn merge(&mut self, other: &Self) -> bool
    where
        T: Clone,
    {
        let Some((value, timestamp)) = other.inner.clone() else {
            return false;
        };

        self.set(value, timestamp)
    }
}

impl<T> Default for LwwRegister<T> {
    fn default() -> Self {
        Self::new()
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
    fn empty_register_has_no_value() {
        let register = LwwRegister::<String>::new();

        assert_eq!(register.value(), None);
        assert_eq!(register.timestamp(), None);
    }

    #[test]
    fn newer_write_wins() {
        let mut register = LwwRegister::new();

        assert!(register.set("old", id("a", 1)));
        assert!(register.set("new", id("a", 2)));

        assert_eq!(register.value(), Some(&"new"));
    }

    #[test]
    fn older_write_is_ignored() {
        let mut register = LwwRegister::new();

        assert!(register.set("new", id("a", 2)));
        assert!(!register.set("old", id("a", 1)));

        assert_eq!(register.value(), Some(&"new"));
    }

    #[test]
    fn merge_converges_to_same_value() {
        let mut left = LwwRegister::new();
        let mut right = LwwRegister::new();
        left.set("left", id("a", 1));
        right.set("right", id("b", 1));

        let mut left_first = left.clone();
        let mut right_first = right.clone();
        left_first.merge(&right);
        right_first.merge(&left);

        assert_eq!(left_first, right_first);
        assert_eq!(left_first.value(), Some(&"right"));
    }
}
