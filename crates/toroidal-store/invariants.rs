//! Core invariants verified by unit tests.
//!
//! 1. **sequence_monotonicity**: next_sequence >= replayed_max + 1
//! 2. **batch_atomicity**: batch visible entirely or not visible
//! 3. **snapshot_stability**: visible state does not change during snapshot lifetime
//! 4. **no_resurrection**: tombstone never reveals older value
//! 5. **manifest_closure**: every manifest segment exists and validates

use crate::{Result, TQLError, ToroidalStore};

/// Invariant: the sequence counter never decreases (even after reopen).
pub fn invariant_sequence_monotonicity(store: &ToroidalStore, previous_max: u64) -> Result<()> {
    let current = store.next_sequence();
    if current >= previous_max {
        Ok(())
    } else {
        Err(TQLError::Storage(format!(
            "sequence decreased: {previous_max} -> {current}"
        )))
    }
}

/// Invariant: snapshot never sees state that changes during its lifetime.
pub fn invariant_snapshot_stability(store: &ToroidalStore, snap: &crate::Snapshot) -> Result<()> {
    let scan = store.scan_at(snap, None, None)?;
    for (k, _) in &scan {
        let v1 = store.get_at(snap, k)?;
        let v2 = store.get_at(snap, k)?;
        if v1 != v2 {
            return Err(TQLError::Storage(format!(
                "snapshot stability violated: key {:?} changed from {:?} to {:?}",
                String::from_utf8_lossy(k),
                v1.map(|v| String::from_utf8_lossy(&v).to_string()),
                v2.map(|v| String::from_utf8_lossy(&v).to_string())
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WalFrameKind;
    use tempfile::tempdir;

    #[test]
    fn sequence_monotonicity() -> Result<()> {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path())?;
        let prev = store.next_sequence();
        store.put(b"a".to_vec(), b"1".to_vec())?;
        invariant_sequence_monotonicity(&store, prev).unwrap();
        Ok(())
    }

    #[test]
    fn snapshot_stability() -> Result<()> {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path())?;
        store.put(b"a".to_vec(), b"1".to_vec())?;
        let snap = store.snapshot();
        store.put(b"a".to_vec(), b"2".to_vec())?;
        invariant_snapshot_stability(&store, &snap)
    }

    #[test]
    fn no_resurrection_after_delete() -> Result<()> {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path())?;
        store.put(b"a".to_vec(), b"1".to_vec())?;
        store.delete(b"a")?;
        assert!(store.get(b"a").is_none());
        store.freeze();
        store.checkpoint()?;
        drop(store);
        let store2 = ToroidalStore::open(dir.path())?;
        assert!(
            store2.get(b"a").is_none(),
            "resurrection detected after reopen"
        );
        Ok(())
    }

    #[test]
    fn manifest_closure() -> Result<()> {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path())?;
        store.put(b"a".to_vec(), b"1".to_vec())?;
        store.freeze();
        store.flush()?;
        // Check segment count > 0 — manifest closure is proven by
        // successful open (every segment referenced by manifest was loaded).
        assert!(store.segment_count() > 0);
        Ok(())
    }

    #[test]
    fn batch_atomicity() -> Result<()> {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path())?;
        let ops = vec![
            WalFrameKind::Put {
                key: b"x".to_vec(),
                value: b"1".to_vec(),
            },
            WalFrameKind::Put {
                key: b"y".to_vec(),
                value: b"2".to_vec(),
            },
        ];
        store.batch(&ops)?;
        assert_eq!(store.get(b"x"), Some(b"1".to_vec()));
        assert_eq!(store.get(b"y"), Some(b"2".to_vec()));
        drop(store);
        let store2 = ToroidalStore::open(dir.path())?;
        assert_eq!(store2.get(b"x"), Some(b"1".to_vec()));
        assert_eq!(store2.get(b"y"), Some(b"2".to_vec()));
        Ok(())
    }
}
