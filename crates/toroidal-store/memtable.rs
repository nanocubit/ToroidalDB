//! Concurrent in-memory table with versioned entries.
//!
//! Each key maps to a **version chain** (newest first). Overwrites append a
//! new version rather than replacing the old one, so snapshots can still read
//! older versions of a key. This mirrors RocksDB's multi-version MemTable
//! semantics and is required for Snapshot visibility.
//!
//! Example:
//! ```text
//! key A: [ (v2, seq=20), (v1, seq=10) ]
//! snapshot(15) → v1
//! snapshot(25) → v2
//! ```

use crate::wal::Sequence;
use crate::{Result, TQLError, VersionedEntry};
use dashmap::{mapref::entry::Entry, DashMap};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const MAX_KEY_SIZE: usize = 16 * 1024 * 1024;
const MAX_VALUE_SIZE: usize = 16 * 1024 * 1024;

/// Per-entry bookkeeping overhead (key Arc ptr, hash slot, alignment, etc.)
const ENTRY_OVERHEAD: usize = 16;

/// Version chain: index 0 is the newest version.
type VersionChain = Vec<VersionedEntry>;

// ---------------------------------------------------------------------------
// MemTable
// ---------------------------------------------------------------------------

/// Thread-safe in-memory map backed by `DashMap`, holding version chains.
#[derive(Clone)]
pub struct MemTable {
    data: Arc<DashMap<Vec<u8>, VersionChain>>,
    size: Arc<AtomicUsize>,
}

impl MemTable {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
            size: Arc::new(AtomicUsize::new(0)),
        }
    }

    // -----------------------------------------------------------------------
    // Validation (shared with store)
    // -----------------------------------------------------------------------

    pub fn validate_put(key: &[u8], value: &[u8]) -> Result<()> {
        if key.len() > MAX_KEY_SIZE {
            return Err(TQLError::Storage("key exceeds maximum size".into()));
        }
        if value.len() > MAX_VALUE_SIZE {
            return Err(TQLError::Storage("value exceeds maximum size".into()));
        }
        Ok(())
    }

    pub fn validate_delete(key: &[u8]) -> Result<()> {
        if key.len() > MAX_KEY_SIZE {
            return Err(TQLError::Storage("key exceeds maximum size".into()));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Writes (versioned)
    // -----------------------------------------------------------------------

    /// Insert a value version at `sequence`. The version is prepended to the
    /// key's chain, so newer versions are found first.
    pub fn insert_with_seq(&self, key: Vec<u8>, value: Vec<u8>, sequence: Sequence) -> Result<()> {
        Self::validate_put(&key, &value)?;
        self.push_version(key, VersionedEntry::value_version(sequence, value))
    }

    /// Insert a tombstone version at `sequence`.
    pub fn delete_with_seq(&self, key: &[u8], sequence: Sequence) -> Result<()> {
        Self::validate_delete(key)?;
        self.push_version(key.to_vec(), VersionedEntry::tombstone(sequence))
    }

    /// Non-versioned convenience (uses sequence 0).
    pub fn insert(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        self.insert_with_seq(key, value, 0)
    }

    pub fn delete(&self, key: &[u8]) -> Result<()> {
        self.delete_with_seq(key, 0)
    }

    fn push_version(&self, key: Vec<u8>, version: VersionedEntry) -> Result<()> {
        let new_bytes = key.len() + ENTRY_OVERHEAD + version.value.byte_size();
        match self.data.entry(key) {
            Entry::Occupied(mut slot) => {
                slot.get_mut().insert(0, version);
            }
            Entry::Vacant(slot) => {
                slot.insert(vec![version]);
            }
        }
        self.size.fetch_add(new_bytes, Ordering::Relaxed);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Reads
    // -----------------------------------------------------------------------

    /// Returns the newest value for `key`, or `None` if absent or tombstoned.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.get_entry(key)
            .and_then(|ve| ve.value.value().map(|v| v.to_vec()))
    }

    /// Returns the newest versioned entry (including tombstones).
    pub fn get_entry(&self, key: &[u8]) -> Option<VersionedEntry> {
        self.data.get(key).and_then(|r| r.value().first().cloned())
    }

    /// Returns the newest version visible at `seq`.
    pub fn get_visible(&self, key: &[u8], seq: Sequence) -> Option<VersionedEntry> {
        let chain = self.data.get(key)?;
        chain.value().iter().find(|ve| ve.sequence <= seq).cloned()
    }

    /// Iterate over newest live values in `[start, end)`, sorted by key.
    pub fn scan(&self, start: Option<&[u8]>, end: Option<&[u8]>) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.scan_entries(start, end)
            .into_iter()
            .filter_map(|(k, ve)| ve.value.value().map(|v| (k, v.to_vec())))
            .collect()
    }

    /// Iterate over all newest versioned entries in `[start, end)`, sorted by key.
    pub fn scan_entries(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Vec<(Vec<u8>, VersionedEntry)> {
        self.scan_entries_visible(start, end, u64::MAX)
    }

    /// Iterate over the newest version visible at `seq` for each key in `[start, end)`.
    pub fn scan_entries_visible(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        seq: Sequence,
    ) -> Vec<(Vec<u8>, VersionedEntry)> {
        let mut out: Vec<(Vec<u8>, VersionedEntry)> = self
            .data
            .iter()
            .filter(|r| {
                let k = r.key().as_slice();
                start.is_none_or(|s| k >= s) && end.is_none_or(|e| k < e)
            })
            .filter_map(|r| {
                r.value()
                    .iter()
                    .find(|ve| ve.sequence <= seq)
                    .map(|ve| (r.key().clone(), ve.clone()))
            })
            .collect();
        out.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        out
    }

    /// Drain all newest entries sorted by key. Does not clear the table.
    pub fn drain(&self) -> Vec<(Vec<u8>, VersionedEntry)> {
        self.scan_entries(None, None)
    }

    /// Return all versions of every key, ordered by key then by sequence
    /// descending (newest first). Used by segment writer to persist the
    /// full version chain so snapshots can still see older versions.
    pub fn scan_all_versions(&self) -> Vec<(Vec<u8>, VersionedEntry)> {
        let mut out: Vec<(Vec<u8>, VersionedEntry)> = Vec::new();
        for r in self.data.iter() {
            let key = r.key().clone();
            let chain: Vec<VersionedEntry> = r.value().clone();
            for ve in chain {
                out.push((key.clone(), ve));
            }
        }
        // Sort by key, then sequence descending (newest first).
        out.sort_unstable_by(|(a, va), (b, vb)| {
            a.cmp(b).then_with(|| vb.sequence.cmp(&va.sequence))
        });
        out
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns the sequence of the most recent entry, or None if empty.
    pub fn max_sequence(&self) -> Option<u64> {
        let mut max: Option<u64> = None;
        for entry in self.data.iter() {
            if let Some(ve) = entry.value().first() {
                let seq = ve.sequence;
                if max.map_or(true, |m| seq > m) {
                    max = Some(seq);
                }
            }
        }
        max
    }

    /// Approximate memory usage in bytes. Over-estimates under overwrite
    /// (old versions are retained for snapshot visibility).
    pub fn approximate_size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    pub fn clear(&self) {
        self.data.clear();
        self.size.store(0, Ordering::Relaxed);
    }
}

impl Default for MemTable {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntryValue, Snapshot, VersionedEntry};

    #[test]
    fn insert_and_get() {
        let m = MemTable::new();
        m.insert_with_seq(b"k".to_vec(), b"v".to_vec(), 10).unwrap();
        assert_eq!(m.get(b"k"), Some(b"v".to_vec()));
        assert_eq!(
            m.get_entry(b"k"),
            Some(VersionedEntry::value_version(10, b"v".to_vec()))
        );
    }

    #[test]
    fn overwrite_adds_version_keeps_old() {
        let m = MemTable::new();
        m.insert_with_seq(b"k".to_vec(), b"v1".to_vec(), 1).unwrap();
        m.insert_with_seq(b"k".to_vec(), b"v2".to_vec(), 2).unwrap();
        // Newest is returned for current reads.
        assert_eq!(m.get(b"k"), Some(b"v2".to_vec()));
        assert_eq!(m.get_entry(b"k").unwrap().sequence, 2);
        // But old version is still visible to snapshot(1).
        assert_eq!(
            m.get_visible(b"k", 1),
            Some(VersionedEntry::value_version(1, b"v1".to_vec()))
        );
    }

    #[test]
    fn tombstone_hides_value_current_but_keeps_history() {
        let m = MemTable::new();
        m.insert_with_seq(b"k".to_vec(), b"v".to_vec(), 1).unwrap();
        m.delete_with_seq(b"k", 2).unwrap();
        assert_eq!(m.get(b"k"), None);
        assert_eq!(m.get_entry(b"k"), Some(VersionedEntry::tombstone(2)));
        // snapshot(1) still sees the value before deletion.
        assert_eq!(
            m.get_visible(b"k", 1),
            Some(VersionedEntry::value_version(1, b"v".to_vec()))
        );
    }

    #[test]
    fn scan_excludes_tombstones() {
        let m = MemTable::new();
        m.insert_with_seq(b"a".to_vec(), b"1".to_vec(), 1).unwrap();
        m.delete_with_seq(b"b", 2).unwrap();
        let live = m.scan(None, None);
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].0, b"a");
    }

    #[test]
    fn scan_entries_includes_tombstones() {
        let m = MemTable::new();
        m.insert_with_seq(b"a".to_vec(), b"1".to_vec(), 1).unwrap();
        m.delete_with_seq(b"b", 2).unwrap();
        let all = m.scan_entries(None, None);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].1.value, EntryValue::Value(b"1".to_vec()));
        assert_eq!(all[1].1.value, EntryValue::Tombstone);
    }

    #[test]
    fn scan_visible_filters_by_sequence() {
        let m = MemTable::new();
        m.insert_with_seq(b"a".to_vec(), b"1".to_vec(), 1).unwrap();
        m.delete_with_seq(b"a", 5).unwrap();
        // At seq 2, 'a' is still visible as a value.
        let vis = m.scan_entries_visible(None, None, 2);
        assert_eq!(vis.len(), 1);
        assert_eq!(vis[0].1.value, EntryValue::Value(b"1".to_vec()));
        // At seq 6, the newest visible version is the tombstone (seq 5).
        let vis = m.scan_entries_visible(None, None, 6);
        assert_eq!(vis.len(), 1);
        assert_eq!(vis[0].1.value, EntryValue::Tombstone);
        // Value scan (live values only) at seq 6 must be empty.
        let live = m.scan(None, None);
        assert!(live.is_empty());
    }

    #[test]
    fn scan_range() {
        let m = MemTable::new();
        for (k, v) in [("a", "1"), ("b", "2"), ("c", "3"), ("d", "4")] {
            m.insert_with_seq(k.as_bytes().to_vec(), v.as_bytes().to_vec(), 1)
                .unwrap();
        }
        let range = m.scan(Some(b"b"), Some(b"d"));
        assert_eq!(range.len(), 2);
        assert_eq!(range[0].0, b"b");
        assert_eq!(range[1].0, b"c");
    }

    #[test]
    fn approximate_size_increases_on_insert() {
        let m = MemTable::new();
        let before = m.approximate_size();
        m.insert_with_seq(b"key".to_vec(), b"value".to_vec(), 1)
            .unwrap();
        assert!(m.approximate_size() > before);
    }

    #[test]
    fn versioned_entry_visibility() {
        let snap = Snapshot::new(20);
        let v10 = VersionedEntry::value_version(10, b"v1".to_vec());
        let v30 = VersionedEntry::value_version(30, b"v3".to_vec());
        assert!(snap.is_visible(&v10));
        assert!(!snap.is_visible(&v30));
    }
}
