//! Snapshot + versioned entry types.
//!
//! Visibility rule (lightweight MVCC):
//!
//! ```text
//! visible(entry, snapshot) = entry.sequence <= snapshot.sequence
//! newest visible version wins
//! ```
//!
//! A `Snapshot` captures a sequence boundary. Reads at that boundary see
//! exactly the versions with `sequence <= snapshot.sequence`.

use crate::wal::Sequence;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

/// Upper bound for "current" reads when no explicit snapshot is given.
/// All real sequences are below `u64::MAX`, so this sees every version.
pub const LATEST: Sequence = u64::MAX;

// ---------------------------------------------------------------------------
// EntryValue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryValue {
    Value(Vec<u8>),
    Tombstone,
}

impl EntryValue {
    pub fn value(&self) -> Option<&[u8]> {
        match self {
            Self::Value(v) => Some(v),
            Self::Tombstone => None,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        matches!(self, Self::Tombstone)
    }

    pub fn byte_size(&self) -> usize {
        match self {
            Self::Value(v) => v.len(),
            Self::Tombstone => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// VersionedEntry
// ---------------------------------------------------------------------------

/// A single version of a key: the value (or tombstone) written at `sequence`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedEntry {
    pub sequence: Sequence,
    pub value: EntryValue,
}

impl VersionedEntry {
    pub fn new(sequence: Sequence, value: EntryValue) -> Self {
        Self { sequence, value }
    }

    pub fn value_version(sequence: Sequence, value: Vec<u8>) -> Self {
        Self::new(sequence, EntryValue::Value(value))
    }

    pub fn tombstone(sequence: Sequence) -> Self {
        Self::new(sequence, EntryValue::Tombstone)
    }
}

// ---------------------------------------------------------------------------
// Snapshot (RAII guard)
// ---------------------------------------------------------------------------

/// An immutable point-in-time view identified by a sequence boundary.
///
/// Holds an `Arc` reference to the [`SnapshotManager`] so that dropping the
/// last copy of this snapshot automatically unregisters its sequence.
/// Implements `Clone` manually to ensure every clone registers its sequence
/// — a dropped clone must not unregister a sequence still held by the original.
#[derive(Debug)]
pub struct Snapshot {
    sequence: Sequence,
    manager: Option<Arc<SnapshotManager>>,
}

impl Clone for Snapshot {
    fn clone(&self) -> Self {
        if let Some(ref m) = self.manager {
            m.register(self.sequence);
        }
        Self {
            sequence: self.sequence,
            manager: self.manager.clone(),
        }
    }
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.sequence == other.sequence
    }
}

impl Eq for Snapshot {}

impl PartialOrd for Snapshot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Snapshot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence.cmp(&other.sequence)
    }
}

impl std::hash::Hash for Snapshot {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.sequence.hash(state);
    }
}

impl Snapshot {
    pub fn new(sequence: Sequence) -> Self {
        Self {
            sequence,
            manager: None,
        }
    }

    /// Create a managed snapshot that registers with the manager.
    pub(crate) fn new_managed(sequence: Sequence, manager: Arc<SnapshotManager>) -> Self {
        manager.register(sequence);
        Self {
            sequence,
            manager: Some(manager),
        }
    }

    pub fn sequence(&self) -> Sequence {
        self.sequence
    }

    pub fn is_visible(&self, entry: &VersionedEntry) -> bool {
        entry.sequence <= self.sequence
    }

    /// The snapshot that sees every version (used for current reads).
    pub fn latest() -> Self {
        Self::new(LATEST)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if let Some(ref m) = self.manager {
            m.unregister(self.sequence);
        }
    }
}

// ---------------------------------------------------------------------------
// SnapshotManager — live-snapshot registry for compaction safety
// ---------------------------------------------------------------------------

/// Tracks which sequence boundaries are still in use by live snapshots,
/// with reference counting so that `N` clones of the same snapshot keep
/// the sequence registered until the last handle is dropped.
///
/// Compaction consults [`SnapshotManager::floor`] to decide which versions it
/// may drop: a version is safe to drop only when no live snapshot can observe
/// it after the drop.
#[derive(Default)]
pub struct SnapshotManager {
    active: Mutex<HashMap<Sequence, usize>>,
}

impl std::fmt::Debug for SnapshotManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnapshotManager").finish_non_exhaustive()
    }
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
        }
    }

    /// Register a snapshot sequence.  Increments the refcount if already
    /// registered.
    pub fn register(&self, sequence: Sequence) {
        let mut map = self.active.lock().unwrap();
        *map.entry(sequence).or_insert(0) += 1;
    }

    /// Unregister a snapshot sequence.  Decrements the refcount and removes
    /// the entry only when the count reaches zero.
    pub fn unregister(&self, sequence: Sequence) {
        let mut map = self.active.lock().unwrap();
        let count = map.entry(sequence).or_insert(1);
        *count = count.saturating_sub(1);
        if *count == 0 {
            map.remove(&sequence);
        }
    }

    /// The lowest sequence boundary any live snapshot needs, or
    /// [`LATEST`] (keep-newest-only) when no snapshot is live.
    pub fn floor(&self) -> Sequence {
        self.active
            .lock()
            .unwrap()
            .keys()
            .copied()
            .min()
            .unwrap_or(LATEST)
    }

    pub fn is_empty(&self) -> bool {
        self.active.lock().unwrap().is_empty()
    }

    /// Returns the number of live snapshot handles across all sequences.
    /// Each clone counts as a separate handle; this is the sum of every
    /// per-sequence refcount.
    pub fn active_count(&self) -> usize {
        self.active.lock().unwrap().values().sum()
    }
}

// ---------------------------------------------------------------------------
// RetentionHorizon
// ---------------------------------------------------------------------------

/// Watermarks that govern how aggressively compaction may drop old versions.
///
/// Compaction may only delete a version when *no* consumer can still observe
/// it.  The horizon is the minimum of every relevant sequence lower bound:
/// - oldest active in-process snapshot
/// - oldest backup / replication consumer
/// - the WAL checkpoint (frames below it are already in segments)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionHorizon {
    pub oldest_active_snapshot: Option<Sequence>,
    pub oldest_backup: Option<Sequence>,
    pub oldest_required_wal: Sequence,
}

impl RetentionHorizon {
    /// The sequence below which versions are eligible for removal.
    pub fn floor(&self) -> Sequence {
        let mut floor = self.oldest_required_wal;
        if let Some(s) = self.oldest_active_snapshot {
            floor = floor.min(s);
        }
        if let Some(b) = self.oldest_backup {
            floor = floor.min(b);
        }
        floor
    }

    /// Default — no live consumers: only the WAL watermark constrains GC.
    pub fn default_with_wal(wal_checkpoint: Sequence) -> Self {
        Self {
            oldest_active_snapshot: None,
            oldest_backup: None,
            oldest_required_wal: wal_checkpoint,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToroidalStore;
    use tempfile::tempdir;

    #[test]
    fn visibility_rule() {
        let snap20 = Snapshot::new(20);
        assert!(snap20.is_visible(&VersionedEntry::value_version(10, b"v1".to_vec())));
        assert!(snap20.is_visible(&VersionedEntry::value_version(20, b"v2".to_vec())));
        assert!(!snap20.is_visible(&VersionedEntry::value_version(30, b"v3".to_vec())));
    }

    #[test]
    fn snapshot_sequence_accessor() {
        let snap = Snapshot::new(42);
        assert_eq!(snap.sequence(), 42);
    }

    #[test]
    fn snapshot_ord() {
        assert!(Snapshot::new(1) < Snapshot::new(2));
        assert_eq!(Snapshot::new(5), Snapshot::new(5));
    }

    #[test]
    fn manager_floor_without_snapshots_is_latest() {
        let m = SnapshotManager::new();
        assert_eq!(m.floor(), LATEST);
        assert!(m.is_empty());
    }

    #[test]
    fn manager_floor_tracks_oldest() {
        let m = SnapshotManager::new();
        m.register(50);
        m.register(30);
        m.register(40);
        assert_eq!(m.floor(), 30);
        m.unregister(30);
        assert_eq!(m.floor(), 40);
        m.unregister(40);
        m.unregister(50);
        assert_eq!(m.floor(), LATEST);
    }

    #[test]
    fn entry_value_accessors() {
        let v = EntryValue::Value(b"x".to_vec());
        assert_eq!(v.value(), Some(&b"x"[..]));
        assert!(!v.is_tombstone());
        assert!(EntryValue::Tombstone.is_tombstone());
        assert!(EntryValue::Tombstone.value().is_none());
    }

    #[test]
    fn snapshot_registers_on_create() {
        let m = Arc::new(SnapshotManager::new());
        let seq = 42;
        let _snap = Snapshot::new_managed(seq, m.clone());
        assert!(!m.is_empty());
        assert_eq!(m.floor(), seq);
    }

    #[test]
    fn snapshot_unregisters_on_last_drop() {
        let m = Arc::new(SnapshotManager::new());
        let snap = Snapshot::new_managed(42, m.clone());
        assert!(!m.is_empty());
        drop(snap);
        assert!(m.is_empty());
    }

    #[test]
    fn clone_does_not_unregister_early() {
        let m = Arc::new(SnapshotManager::new());
        let snap = Snapshot::new_managed(42, m.clone());
        let clone = snap.clone();
        drop(snap); // clone still holds it
        assert!(!m.is_empty(), "clone must keep the sequence alive");
        assert_eq!(m.floor(), 42);
        drop(clone);
        assert!(m.is_empty());
    }

    #[test]
    fn multiple_snapshots_floor() {
        let m = Arc::new(SnapshotManager::new());
        let _s1 = Snapshot::new_managed(10, m.clone());
        let _s2 = Snapshot::new_managed(30, m.clone());
        let _s3 = Snapshot::new_managed(20, m.clone());
        assert_eq!(m.floor(), 10);
    }

    #[test]
    fn drop_oldest_snapshot_advances_floor() {
        let m = Arc::new(SnapshotManager::new());
        let s_low = Snapshot::new_managed(10, m.clone());
        let _s_high = Snapshot::new_managed(30, m.clone());
        assert_eq!(m.floor(), 10);
        drop(s_low);
        assert_eq!(m.floor(), 30);
    }

    #[test]
    fn double_unregister_does_not_underflow() {
        let m = SnapshotManager::new();
        m.register(10);
        m.register(10);
        m.unregister(10);
        assert!(!m.is_empty(), "refcount should still be 1");
        assert_eq!(m.floor(), 10);
        m.unregister(10);
        assert!(m.is_empty());
        // Underflow: should not panic or set negative.
        m.unregister(10);
        assert!(m.is_empty());
    }

    #[test]
    fn active_count_reflects_clones() {
        let m = Arc::new(SnapshotManager::new());
        let s1 = Snapshot::new_managed(10, m.clone());
        assert_eq!(m.active_count(), 1);
        let s2 = s1.clone();
        assert_eq!(m.active_count(), 2);
        drop(s1);
        assert_eq!(m.active_count(), 1);
        drop(s2);
        assert_eq!(m.active_count(), 0);
    }

    #[test]
    fn active_count_multiple_sequences() {
        let m = Arc::new(SnapshotManager::new());
        let _a = Snapshot::new_managed(10, m.clone());
        let _b = Snapshot::new_managed(20, m.clone());
        assert_eq!(m.active_count(), 2);
    }

    #[test]
    fn active_count_via_store() {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        assert_eq!(store.active_snapshot_count(), 0);
        let s1 = store.snapshot();
        assert_eq!(store.active_snapshot_count(), 1);
        let s2 = s1.clone();
        assert_eq!(store.active_snapshot_count(), 2);
        drop(s1);
        assert_eq!(store.active_snapshot_count(), 1);
        drop(s2);
        assert_eq!(store.active_snapshot_count(), 0);
    }
}
