//! Fault injection — configurable failure points for crash and corruption
//! testing.  In production the injector is a no-op with zero cost.
//!
//! Points are checked at strategic locations in WAL, MemTable, Segment,
//! Manifest, Checkpoint, and Compaction.  The injector returns an error
//! when the point is active, causing the caller to abort the operation
//! cleanly and leave a deterministically corrupted state.

use crate::Result;

// ---------------------------------------------------------------------------
// FailurePoint
// ---------------------------------------------------------------------------

/// Every location where a synthetic fault can be injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailurePoint {
    AfterWalAppend,
    AfterWalSync,
    AfterBatchCommit,
    AfterMemtableFreeze,
    AfterSegmentWrite,
    AfterSegmentSync,
    AfterManifestWrite,
    AfterManifestSync,
    BeforeWalTruncate,
    AfterWalTruncate,
    BeforeCompaction,
    DuringCompaction,
    AfterCompactionOutput,
    AfterCompactionPublish,
    /// After the compaction manifest ADD+REMOVE batch is durable.
    CompactionAfterManifestBatch,
    /// After old segment files are physically deleted (manifest REMOVE already durable).
    CompactionAfterOldFileDelete,
}

// ---------------------------------------------------------------------------
// FailureInjector trait
// ---------------------------------------------------------------------------

/// Injectable failure strategy.  The default implementation (`NoFault`)
/// returns `Ok(())` for every point — zero overhead in production.
pub trait FailureInjector: Send + Sync {
    fn check(&self, point: FailurePoint) -> Result<()>;
}

/// Production injector — no faults.
pub struct NoFault;

impl FailureInjector for NoFault {
    fn check(&self, _: FailurePoint) -> Result<()> {
        Ok(())
    }
}

/// Test injector — fails at a single specified point.
pub struct FailAt {
    pub point: FailurePoint,
}

impl FailureInjector for FailAt {
    fn check(&self, point: FailurePoint) -> Result<()> {
        if point == self.point {
            Err(crate::TQLError::Storage(format!(
                "injected fault at {:?}",
                point
            )))
        } else {
            Ok(())
        }
    }
}

/// Test injector — fails at every point in a set.
pub struct FailAtSet {
    pub points: std::collections::HashSet<FailurePoint>,
}

impl FailureInjector for FailAtSet {
    fn check(&self, point: FailurePoint) -> Result<()> {
        if self.points.contains(&point) {
            Err(crate::TQLError::Storage(format!(
                "injected fault at {:?}",
                point
            )))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_fault_never_fails() {
        let nf = NoFault;
        assert!(nf.check(FailurePoint::AfterWalAppend).is_ok());
        assert!(nf.check(FailurePoint::AfterSegmentSync).is_ok());
    }

    #[test]
    fn fail_at_precise_point() {
        let fa = FailAt {
            point: FailurePoint::AfterWalSync,
        };
        assert!(fa.check(FailurePoint::AfterWalAppend).is_ok());
        assert!(fa.check(FailurePoint::AfterWalSync).is_err());
    }

    #[test]
    fn fail_at_set() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FailurePoint::AfterSegmentWrite);
        set.insert(FailurePoint::AfterManifestWrite);
        let fas = FailAtSet { points: set };
        assert!(fas.check(FailurePoint::AfterWalAppend).is_ok());
        assert!(fas.check(FailurePoint::AfterSegmentWrite).is_err());
        assert!(fas.check(FailurePoint::AfterManifestWrite).is_err());
    }
}
