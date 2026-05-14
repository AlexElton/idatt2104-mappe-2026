use serde::{Deserialize, Serialize};

/// Quadruple vector from Roh et al. 2011, Section 5.1.
/// Provides a globally unique, totally ordered identifier for every operation.
///
/// Fields:
///   ssn — session number (increments on membership change or fresh start)
///   sid — site ID (unique per connected client)
///   sum — sum of all vector clock entries at operation time (simplified: local counter)
///   seq — site's own vector clock entry (simplified: same as sum)
///
/// Order (Definition 9): sa ≺ sb iff
///   sa.ssn < sb.ssn, OR
///   sa.ssn == sb.ssn AND sa.sum < sb.sum, OR
///   sa.ssn == sb.ssn AND sa.sum == sb.sum AND sa.sid < sb.sid
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct S4Vector {
    pub ssn: u64,
    pub sid: u64,
    pub sum: u64,
    pub seq: u64,
}

impl S4Vector {
    pub fn new(ssn: u64, sid: u64, sum: u64, seq: u64) -> Self {
        Self { ssn, sid, sum, seq }
    }

    /// Returns true if self precedes other (self has lower priority than other).
    /// Definition 9 from Roh et al. 2011.
    pub fn precedes(&self, other: &S4Vector) -> bool {
        todo!("implement s4vector ordering — see Definition 9 in the paper")
    }
}

impl std::fmt::Display for S4Vector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "⟨{},{},{},{}⟩", self.ssn, self.sid, self.sum, self.seq)
    }
}
