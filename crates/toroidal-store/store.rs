//! ToroidalStore — write-coordinated store with WAL + MemTable + Segment layers.
//!
//! # Storage layout (directory)
//!
//! ```text
//! db_dir/
//!   wal          — Write-Ahead Log (standalone + batched ops)
//!   manifest     — Append-only log of ADD_SEGMENT / REMOVE_SEGMENT
//!   seg_*.seg    — Sorted on-disk snapshots of frozen MemTables
//! ```
//!
//! # Correctness properties
//!
//! 1. **Atomic write boundary**: put / delete / batch acquire the state
//!    write-lock for their full duration (WAL append + MemTable insert).
//!
//! 2. **Freeze is serialized with writes**: freeze acquires the same write-lock.
//!
//! 3. **Coherent snapshot**: get / scan acquire the state read-lock once and
//!    observe a consistent view of active + immutable + segment generations.
//!
//! 4. **Newest-wins merge with tombstone suppression**: scan iterates
//!    active → immutable (newest → oldest) → segments (newest → oldest).
//!    First visible entry wins. A Tombstone stops search.
//!
//! 5. **Snapshot visibility**: reads at snapshot `S` see only entries with
//!    `sequence <= S`. Newer entries are invisible and fall through to older
//!    layers, preserving correct point-in-time semantics.

use crate::batcher::{BatcherOptions, WalBatcher};
use crate::fault::{FailureInjector, FailurePoint, NoFault};
use crate::snapshot::SnapshotManager;
use crate::{
    write_segment, write_segment_nosync, EntryValue, Manifest, MemTable, Result, SegmentReader,
    Snapshot, TQLError, VersionedEntry, Wal, WalFrameKind,
};
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// StoreState — all mutable state behind one RwLock
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// StoreState — all mutable state behind one RwLock
// ---------------------------------------------------------------------------

struct StoreState {
    active: Arc<MemTable>,
    immutable: VecDeque<Arc<MemTable>>,
    /// Segments sorted newest-first (for newest-wins scan).
    segments: Vec<SegmentReader>,
}

impl StoreState {
    fn new(mem: MemTable, segments: Vec<SegmentReader>) -> Self {
        Self {
            active: Arc::new(mem),
            immutable: VecDeque::new(),
            segments,
        }
    }

    fn freeze(&mut self) -> Arc<MemTable> {
        let frozen = std::mem::replace(&mut self.active, Arc::new(MemTable::new()));
        self.immutable.push_back(Arc::clone(&frozen));
        frozen
    }

    fn take_oldest_immutable(&mut self) -> Option<Arc<MemTable>> {
        self.immutable.pop_front()
    }

    fn push_segment_newest(&mut self, reader: SegmentReader) {
        self.segments.insert(0, reader);
    }
}

// ---------------------------------------------------------------------------
// ToroidalStoreOptions
// ---------------------------------------------------------------------------

/// Configuration for opening a [`ToroidalStore`].
#[derive(Debug, Clone, Default)]
pub struct ToroidalStoreOptions {
    pub wal_options: BatcherOptions,
}

pub struct ToroidalStore {
    wal: Arc<Wal>,
    batcher: parking_lot::Mutex<Option<Arc<WalBatcher>>>,
    manifest: Manifest,
    dir: PathBuf,
    state: RwLock<StoreState>,
    next_seg_id: AtomicU64,
    fault: Arc<dyn FailureInjector>,
    snapshot_manager: Arc<SnapshotManager>,
    closed: parking_lot::Mutex<bool>,
}

impl ToroidalStore {
    /// Returns `true` if the store has been closed (shutdown).
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }

    /// Close the store: flush batcher, shut down worker, stop accepting writes.
    pub fn close(&self) -> Result<()> {
        *self.closed.lock() = true;
        if let Some(b) = self.batcher.lock().take() {
            drop(b);
        }
        Ok(())
    }

    fn batcher_arc(&self) -> Result<Arc<WalBatcher>> {
        self.batcher
            .lock()
            .as_ref()
            .cloned()
            .ok_or_else(|| TQLError::Storage("store is closed".into()))
    }

    fn check_open(&self) -> Result<()> {
        if *self.closed.lock() {
            Err(TQLError::Storage("store is closed".into()))
        } else {
            Ok(())
        }
    }

    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_with_options(dir, ToroidalStoreOptions::default())
    }

    pub fn open_with(dir: &Path, fault: Arc<dyn FailureInjector>) -> Result<Self> {
        Self::open_with_options(dir, ToroidalStoreOptions::default()).map(|mut s| {
            s.fault = fault;
            s
        })
    }

    pub fn open_with_options(dir: &Path, options: ToroidalStoreOptions) -> Result<Self> {
        fs::create_dir_all(dir).map_err(TQLError::Io)?;

        let manifest_path = dir.join("manifest");
        let manifest = Manifest::open(&manifest_path)?;

        let wal_path = dir.join("wal");
        // The WAL may be empty after a checkpoint (its frames were flushed
        // into segments and truncated). Sequence must never go backwards:
        // restore the floor from the manifest's max_flushed_sequence.
        let floor = manifest
            .max_flushed_sequence()
            .checked_add(1)
            .ok_or_else(|| TQLError::Storage("manifest sequence overflow".into()))?;
        let wal = Arc::new(Wal::open_with_sequence(&wal_path, floor)?);
        let mem = replay_into_memtable(&wal)?;

        let mut segments: Vec<SegmentReader> = manifest
            .live_segments()
            .iter()
            .map(|p| SegmentReader::open(p))
            .collect::<Result<Vec<_>>>()?;
        segments.sort_unstable_by_key(|s| std::cmp::Reverse(s.max_sequence()));

        let max_seg_id = segments
            .iter()
            .filter_map(|s| {
                let name = s.path().file_stem()?.to_str()?;
                name.strip_prefix("seg_")
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0);

        // Create the group-commit batcher wrapping the WAL.
        let batcher = Arc::new(WalBatcher::new(wal.clone(), options.wal_options));

        Ok(Self {
            wal,
            batcher: parking_lot::Mutex::new(Some(batcher)),
            manifest,
            dir: dir.to_path_buf(),
            state: RwLock::new(StoreState::new(mem, segments)),
            next_seg_id: AtomicU64::new(max_seg_id + 1),
            fault: Arc::new(NoFault),
            snapshot_manager: Arc::new(SnapshotManager::new()),
            closed: parking_lot::Mutex::new(false),
        })
    }

    // -----------------------------------------------------------------------
    // Single-key writes (versioned with sequence) — direct-sync batcher path
    // -----------------------------------------------------------------------

    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        MemTable::validate_put(&key, &value)?;
        self.check_open()?;
        let batcher = self.batcher_arc()?;
        let seq = batcher.append_sync_direct(WalFrameKind::Put {
            key: key.clone(),
            value: value.clone(),
        })?;
        self.fault.check(FailurePoint::AfterWalAppend)?;
        let state = self.state.write();
        state.active.insert_with_seq(key, value, seq)
    }

    pub fn delete(&self, key: &[u8]) -> Result<()> {
        MemTable::validate_delete(key)?;
        self.check_open()?;
        let batcher = self.batcher_arc()?;
        let seq = batcher.append_sync_direct(WalFrameKind::Delete { key: key.to_vec() })?;
        self.fault.check(FailurePoint::AfterWalAppend)?;
        let state = self.state.write();
        state.active.delete_with_seq(key, seq)
    }

    // -----------------------------------------------------------------------
    // Atomic batch write — group commit via batcher
    // -----------------------------------------------------------------------

    pub fn batch(&self, ops: &[WalFrameKind]) -> Result<()> {
        if ops.is_empty() {
            return Ok(());
        }
        for op in ops {
            match op {
                WalFrameKind::Put { key, value } => MemTable::validate_put(key, value)?,
                WalFrameKind::Delete { key } => MemTable::validate_delete(key)?,
                _ => {
                    return Err(TQLError::Storage(
                        "batch may only contain PUT or DELETE operations".into(),
                    ))
                }
            }
        }
        let batcher = self.batcher_arc()?;
        let first = batcher.append_many_sync(ops)?;
        // Apply to MemTable under write lock.
        let state = self.state.write();
        for (i, op) in ops.iter().enumerate() {
            let seq = first + i as u64;
            match op {
                WalFrameKind::Put { key, value } => {
                    state
                        .active
                        .insert_with_seq(key.clone(), value.clone(), seq)?;
                }
                WalFrameKind::Delete { key } => {
                    state.active.delete_with_seq(key, seq)?;
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Snapshot — capture a point-in-time sequence boundary
    // -----------------------------------------------------------------------

    /// Return a managed snapshot at the last committed sequence.
    /// Registers with the SnapshotManager so the version is retained until
    /// the snapshot is dropped.
    pub fn snapshot(&self) -> Snapshot {
        let seq = self.wal.next_sequence().saturating_sub(1);
        Snapshot::new_managed(seq, self.snapshot_manager.clone())
    }

    /// Return the sequence of the oldest active snapshot, if any.
    pub fn oldest_active_snapshot(&self) -> Option<u64> {
        let f = self.snapshot_manager.floor();
        if self.snapshot_manager.is_empty() {
            None
        } else {
            Some(f)
        }
    }

    /// Returns the number of live snapshot handles (each clone counts as a
    /// separate handle).  Equivalent to the sum of per-sequence refcounts in
    /// the SnapshotManager.
    pub fn active_snapshot_count(&self) -> usize {
        self.snapshot_manager.active_count()
    }

    // -----------------------------------------------------------------------
    // Reads — coherent snapshot under a single read-lock
    // -----------------------------------------------------------------------

    /// Convenience: current read (no explicit snapshot, sees everything).
    /// Fast path: avoids sequence checks per entry.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let state = self.state.read();

        for mem in
            std::iter::once(&*state.active).chain(state.immutable.iter().rev().map(|m| m.as_ref()))
        {
            if let Some(ve) = mem.get_entry(key) {
                match ve.value {
                    EntryValue::Value(v) => return Some(v),
                    EntryValue::Tombstone => return None,
                }
            }
        }

        for seg in &state.segments {
            if let Some(ve) = seg.get(key) {
                match ve.value {
                    EntryValue::Value(v) => return Some(v),
                    EntryValue::Tombstone => return None,
                }
            }
        }

        None
    }

    /// Snapshot-aware read.
    ///
    /// Returns:
    /// - `Ok(Some(v))` — key found and visible to the snapshot.
    /// - `Ok(None)` — key absent or tombstoned at the snapshot time.
    /// - `Err(SnapshotExpired{..})` — snapshot is below the retention floor.
    pub fn get_at(&self, snap: &Snapshot, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let state = self.state.read();
        let seq = snap.sequence();

        if seq < self.retention_floor() {
            return Err(TQLError::SnapshotExpired {
                requested: seq,
                oldest_available: self.retention_floor(),
            });
        }

        for mem in
            std::iter::once(&*state.active).chain(state.immutable.iter().rev().map(|m| m.as_ref()))
        {
            if let Some(ve) = mem.get_visible(key, seq) {
                match ve.value {
                    EntryValue::Value(v) => return Ok(Some(v)),
                    EntryValue::Tombstone => return Ok(None),
                }
            }
        }

        for seg in &state.segments {
            if let Some(ve) = seg.get_visible(key, seq) {
                match ve.value {
                    EntryValue::Value(v) => return Ok(Some(v)),
                    EntryValue::Tombstone => return Ok(None),
                }
            }
        }

        Ok(None)
    }

    /// Convenience: current scan (no explicit snapshot, sees everything).
    pub fn scan(&self, start: Option<&[u8]>, end: Option<&[u8]>) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.scan_at(&Snapshot::latest(), start, end)
            .unwrap_or_default()
    }

    /// Scan at a specific snapshot. Only entries with `sequence <= snap`
    /// are visible. Newest-wins, tombstone suppresses older values.
    ///
    /// Returns `Err(SnapshotExpired{..})` when the snapshot is below the
    /// retention floor.
    pub fn scan_at(
        &self,
        snap: &Snapshot,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let state = self.state.read();
        let seq = snap.sequence();

        if seq < self.retention_floor() {
            return Err(TQLError::SnapshotExpired {
                requested: seq,
                oldest_available: self.retention_floor(),
            });
        }

        let mut seen: HashSet<Vec<u8>> = HashSet::new();
        let mut result: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();

        // Active (newest). Use scan_entries_visible to get the newest visible
        // version per key.
        for (key, ve) in state.active.scan_entries_visible(start, end, seq) {
            if seen.insert(key.clone()) {
                if let EntryValue::Value(v) = ve.value {
                    result.push((key, v));
                }
            }
        }
        // Immutables (newest to oldest).
        for mem in state.immutable.iter().rev() {
            for (key, ve) in mem.scan_entries_visible(start, end, seq) {
                if seen.insert(key.clone()) {
                    if let EntryValue::Value(v) = ve.value {
                        result.push((key, v));
                    }
                }
            }
        }
        // Segments (newest to oldest). Each stores one version per key.
        for seg in &state.segments {
            for (key, ve) in seg.scan_entries(start, end) {
                if ve.sequence <= seq && seen.insert(key.clone()) {
                    if let EntryValue::Value(v) = ve.value {
                        result.push((key, v));
                    }
                }
            }
        }

        result.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Freeze
    // -----------------------------------------------------------------------

    pub fn freeze(&self) -> Arc<MemTable> {
        // Durability barrier first: complete all pending batched writes
        // (fsync) before promoting the active MemTable to immutable.
        if let Ok(b) = self.batcher_arc() {
            let _ = b.flush();
        }
        let _ = self.wal.sync();
        self.state.write().freeze()
    }

    // -----------------------------------------------------------------------
    // Flush (one immutable)
    // -----------------------------------------------------------------------

    pub fn flush(&self) -> Result<Option<usize>> {
        let frozen = {
            let mut state = self.state.write();
            match state.take_oldest_immutable() {
                Some(f) => f,
                None => return Ok(None),
            }
        };

        let n_entries = frozen.len();
        let max_seq = self.wal.next_sequence().saturating_sub(1);

        let seg_id = self.next_seg_id.fetch_add(1, Ordering::Relaxed);
        let seg_name = format!("seg_{:016}.seg", seg_id);
        let seg_path = self.dir.join(&seg_name);

        write_segment(&frozen, &seg_path, max_seq)?;
        self.fault.check(FailurePoint::AfterSegmentWrite)?;
        self.manifest.add_segment(&seg_path, max_seq)?;
        self.fault.check(FailurePoint::AfterManifestWrite)?;
        self.fault.check(FailurePoint::AfterManifestSync)?;

        let reader = SegmentReader::open(&seg_path)?;
        {
            let mut state = self.state.write();
            state.push_segment_newest(reader);
        }

        Ok(Some(n_entries))
    }

    // -----------------------------------------------------------------------
    // Checkpoint — group flush ALL immutables, one durability barrier
    // -----------------------------------------------------------------------

    pub fn checkpoint(&self) -> Result<usize> {
        self.freeze();
        self.fault.check(FailurePoint::AfterMemtableFreeze)?;

        let batch: Vec<Arc<MemTable>> = {
            let mut state = self.state.write();
            let mut v = Vec::new();
            while let Some(f) = state.take_oldest_immutable() {
                v.push(f);
            }
            v
        };

        if batch.is_empty() {
            return Ok(0);
        }

        let mut seg_paths: Vec<PathBuf> = Vec::with_capacity(batch.len());
        let mut manifest_adds: Vec<(PathBuf, u64)> = Vec::with_capacity(batch.len());

        for frozen in &batch {
            let max_seq = self.wal.next_sequence().saturating_sub(1);
            let seg_id = self.next_seg_id.fetch_add(1, Ordering::Relaxed);
            let seg_name = format!("seg_{:016}.seg", seg_id);
            let seg_path = self.dir.join(&seg_name);

            write_segment_nosync(frozen, &seg_path, max_seq)?;
            self.fault.check(FailurePoint::AfterSegmentWrite)?;
            manifest_adds.push((seg_path.clone(), max_seq));
            seg_paths.push(seg_path);
        }

        for p in &seg_paths {
            use std::fs::File;
            let f = File::open(p).map_err(TQLError::Io)?;
            f.sync_all().map_err(TQLError::Io)?;
        }
        self.fault.check(FailurePoint::AfterSegmentSync)?;

        let checkpoint_seq = self.wal.next_sequence();
        let manifest_refs: Vec<(&Path, u64)> = manifest_adds
            .iter()
            .map(|(p, s)| (p.as_path(), *s))
            .collect();
        self.manifest.append_batch(&manifest_refs, checkpoint_seq)?;
        self.fault.check(FailurePoint::AfterManifestWrite)?;
        self.manifest.sync()?;
        self.fault.check(FailurePoint::AfterManifestSync)?;

        let mut readers: Vec<SegmentReader> = Vec::with_capacity(seg_paths.len());
        for p in &seg_paths {
            readers.push(SegmentReader::open(p)?);
        }
        {
            let mut state = self.state.write();
            // readers are ordered oldest→newest (batch order). Each
            // push_segment_newest inserts at index 0, so the final list is
            // newest-first: the last (newest) reader lands at index 0.
            // Must NOT reverse — reversing would put the oldest segment
            // first, so a tombstone in a newer segment would fail to
            // suppress the older value (tombstone resurrection).
            for reader in readers {
                state.push_segment_newest(reader);
            }
        }

        let floor = checkpoint_seq;
        self.fault.check(FailurePoint::BeforeWalTruncate)?;
        self.wal.truncate_with_floor(floor)?;
        self.fault.check(FailurePoint::AfterWalTruncate)?;

        Ok(batch.len())
    }

    // -----------------------------------------------------------------------
    // Compaction — merge N generations into one
    // -----------------------------------------------------------------------

    /// Merge all segments into a single compacted segment, safe against
    /// tombstone resurrection and snapshot expiry.
    ///
    /// The safe retention floor is computed internally from live snapshots
    /// and the WAL checkpoint.  For every key, the newest version with
    /// `sequence >= floor` is kept — it also overrides every older version
    /// for any live snapshot.  Versions strictly below that pivot are dropped.
    ///
    /// Tombstones retained by this rule continue to suppress older values;
    /// dropping them only happens when no snapshot can observe the older
    /// values they suppress.
    pub fn compact(&self) -> Result<usize> {
        let floor = self.retention_floor();
        self.compact_internal(floor)
    }

    /// Internal compaction with a floor computed by `compact()`.
    ///
    /// Three-phase approach that is safe under concurrent flush/checkpoint:
    /// 1. Read-lock: clone the current segment readers (cheap — data is in
    ///    shared Arc).  Readers continue to see old segments during the
    ///    compaction, so no read is ever starved or sees a gap.
    /// 2. No lock held: merge versions from the cloned segments, write the
    ///    merged segment file, register it in the manifest.
    /// 3. Write-lock publish: remove exactly the old segments we compacted,
    ///    insert the merged segment, then re-sort by `max_sequence` desc.
    ///    Any segment flushed concurrently (newer max_sequence) is naturally
    ///    ordered ahead of the merged result.
    ///
    /// Old segment files are deleted only AFTER publish, once no reader can
    /// still be using them (readers hold pre-publish readers in memory).
    fn compact_internal(&self, floor: u64) -> Result<usize> {
        // 1. Clone current segments under read-lock.
        let old_segments: Vec<SegmentReader> = {
            let state = self.state.read();
            if state.segments.len() < 2 {
                return Ok(state.segments.len());
            }
            state.segments.clone()
        };
        let old_paths: Vec<PathBuf> = old_segments
            .iter()
            .map(|s| s.path().to_path_buf())
            .collect();
        let old_max_seq = old_segments[0].max_sequence();
        let old_len = old_segments.len();

        // 2. Merge versions (no lock — data is in shared memory).
        let mut merged: BTreeMap<Vec<u8>, Vec<VersionedEntry>> = BTreeMap::new();
        for seg in &old_segments {
            for (key, ve) in seg.scan_entries(None, None) {
                let chain = merged.entry(key).or_default();
                if !chain
                    .iter()
                    .any(|existing| existing.sequence == ve.sequence)
                {
                    chain.push(ve);
                }
            }
        }
        for chain in merged.values_mut() {
            chain.sort_by(|a, b| b.sequence.cmp(&a.sequence));
            match chain.iter().position(|ve| ve.sequence >= floor) {
                Some(i) => chain.truncate(i + 1),
                None => {
                    chain.truncate(1);
                }
            }
        }
        merged.retain(|_, chain| !chain.is_empty());

        let merged_mem = MemTable::new();
        for (key, chain) in &merged {
            for ve in chain {
                match &ve.value {
                    EntryValue::Value(v) => {
                        merged_mem.insert_with_seq(key.clone(), v.clone(), ve.sequence)?
                    }
                    EntryValue::Tombstone => merged_mem.delete_with_seq(key, ve.sequence)?,
                }
            }
        }

        let seg_id = self.next_seg_id.fetch_add(1, Ordering::Relaxed);
        let seg_name = format!("seg_{:016}.seg", seg_id);
        let seg_path = self.dir.join(&seg_name);

        write_segment(&merged_mem, &seg_path, old_max_seq)?;
        self.fault.check(FailurePoint::DuringCompaction)?;
        self.manifest.add_segment(&seg_path, old_max_seq)?;
        self.fault.check(FailurePoint::AfterCompactionOutput)?;
        let new_reader = SegmentReader::open(&seg_path)?;

        // 3. Atomic publish under write-lock.
        {
            let mut state = self.state.write();
            state
                .segments
                .retain(|s| !old_paths.contains(&s.path().to_path_buf()));
            state.segments.push(new_reader);
            state
                .segments
                .sort_unstable_by_key(|s| std::cmp::Reverse(s.max_sequence()));
        }
        self.fault.check(FailurePoint::AfterCompactionPublish)?;

        // Delete old segment files only after publish.
        for seg_path in &old_paths {
            let _ = fs::remove_file(seg_path);
        }
        for seg_path in &old_paths {
            self.manifest.remove_segment(seg_path)?;
        }

        Ok(old_len)
    }

    /// Return the lowest sequence at which a read can still be served
    /// correctly — the retention floor derived from live snapshots and the
    /// WAL checkpoint.  Snapshots at or above this sequence remain valid.
    pub fn retention_floor(&self) -> u64 {
        // The floor is the minimum of:
        // - WAL checkpoint (all data below it is in segments)
        // - oldest active snapshot (must not lose visible versions)
        let wal_floor = self.manifest.max_flushed_sequence();
        let snap_floor = self.snapshot_manager.floor();
        wal_floor.min(snap_floor)
    }

    // -----------------------------------------------------------------------
    // Metrics / introspection
    // -----------------------------------------------------------------------

    pub fn next_sequence(&self) -> u64 {
        self.wal.next_sequence()
    }

    pub fn active_len(&self) -> usize {
        self.state.read().active.len()
    }

    pub fn immutable_count(&self) -> usize {
        self.state.read().immutable.len()
    }

    pub fn segment_count(&self) -> usize {
        self.state.read().segments.len()
    }

    pub fn max_flushed_sequence(&self) -> u64 {
        self.manifest.max_flushed_sequence()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn replay_into_memtable(wal: &Wal) -> Result<MemTable> {
    let mem = MemTable::new();
    for frame in wal.replay()? {
        match frame.kind {
            WalFrameKind::Put { key, value } => mem.insert_with_seq(key, value, frame.sequence)?,
            WalFrameKind::Delete { key } => mem.delete_with_seq(&key, frame.sequence)?,
            other => {
                return Err(TQLError::Storage(format!(
                    "unexpected WAL frame during recovery: {other:?}"
                )))
            }
        }
    }
    Ok(mem)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_store() -> (tempfile::TempDir, ToroidalStore) {
        let dir = tempdir().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn put_get_delete() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"k".to_vec(), b"v".to_vec())?;
        assert_eq!(s.get(b"k"), Some(b"v".to_vec()));
        s.delete(b"k")?;
        assert_eq!(s.get(b"k"), None);
        Ok(())
    }

    #[test]
    fn overwrite() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"k".to_vec(), b"v1".to_vec())?;
        s.put(b"k".to_vec(), b"v2".to_vec())?;
        assert_eq!(s.get(b"k"), Some(b"v2".to_vec()));
        Ok(())
    }

    #[test]
    fn scan_merges_generations() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        s.freeze();
        s.put(b"c".to_vec(), b"3".to_vec())?;
        let r = s.scan(None, None);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0], (b"a".to_vec(), b"1".to_vec()));
        assert_eq!(r[1], (b"b".to_vec(), b"2".to_vec()));
        assert_eq!(r[2], (b"c".to_vec(), b"3".to_vec()));
        Ok(())
    }

    #[test]
    fn snapshot_isolation() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"v1".to_vec())?;
        let snap = s.snapshot();
        s.put(b"a".to_vec(), b"v2".to_vec())?;
        // snapshot sees v1, current sees v2
        assert_eq!(s.get_at(&snap, b"a").unwrap(), Some(b"v1".to_vec()));
        assert_eq!(s.get(b"a"), Some(b"v2".to_vec()));
        Ok(())
    }

    #[test]
    fn snapshot_hides_newer_keys() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        let snap = s.snapshot();
        s.put(b"b".to_vec(), b"2".to_vec())?;
        // snapshot was taken before 'b' existed
        assert_eq!(s.get_at(&snap, b"b").unwrap(), None);
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        Ok(())
    }

    #[test]
    fn snapshot_tombstone_visibility() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        let snap_before_del = s.snapshot();
        s.delete(b"a")?;
        let snap_after_del = s.snapshot();
        // before delete: sees 'a' = '1'
        assert_eq!(
            s.get_at(&snap_before_del, b"a").unwrap(),
            Some(b"1".to_vec())
        );
        // after delete: sees tombstone → None
        assert_eq!(s.get_at(&snap_after_del, b"a").unwrap(), None);
        Ok(())
    }

    #[test]
    fn snapshot_scan_at() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        let snap = s.snapshot();
        s.put(b"c".to_vec(), b"3".to_vec())?;
        let r = s.scan_at(&snap, None, None).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].0, b"a");
        assert_eq!(r[1].0, b"b");
        Ok(())
    }

    #[test]
    fn snapshot_across_freeze_boundary() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.put(b"b".to_vec(), b"2".to_vec())?;
        let snap = s.snapshot();
        s.put(b"c".to_vec(), b"3".to_vec())?;
        // snapshot sees 'a' (frozen) and 'b' (active), not 'c'
        assert_eq!(s.get_at(&snap, b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(s.get_at(&snap, b"b").unwrap(), Some(b"2".to_vec()));
        assert_eq!(s.get_at(&snap, b"c").unwrap(), None);
        Ok(())
    }

    #[test]
    fn snapshot_across_segment() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.flush()?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        let snap = s.snapshot();
        s.put(b"c".to_vec(), b"3".to_vec())?;
        // snapshot sees 'a' (segment) and 'b' (active), not 'c'
        assert_eq!(s.get_at(&snap, b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(s.get_at(&snap, b"b").unwrap(), Some(b"2".to_vec()));
        assert_eq!(s.get_at(&snap, b"c").unwrap(), None);
        Ok(())
    }

    #[test]
    fn flush_and_read() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        s.freeze();
        let flushed = s.flush()?;
        assert_eq!(flushed, Some(2));
        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.get(b"c"), None);
        let r = s.scan(None, None);
        assert_eq!(r.len(), 2);
        Ok(())
    }

    #[test]
    fn flush_then_write_then_read() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.flush()?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.scan(None, None).len(), 2);
        Ok(())
    }

    #[test]
    fn flush_tombstone_in_segment() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.delete(b"a")?;
        s.freeze();
        s.flush()?;
        assert_eq!(s.get(b"a"), None);
        s.put(b"a".to_vec(), b"new".to_vec())?;
        assert_eq!(s.get(b"a"), Some(b"new".to_vec()));
        Ok(())
    }

    #[test]
    fn flush_no_immutable_returns_none() -> Result<()> {
        let (_dir, s) = open_store();
        assert_eq!(s.flush()?, None);
        Ok(())
    }

    #[test]
    fn recovery_with_segments() -> Result<()> {
        let dir = tempdir().unwrap();
        {
            let s = ToroidalStore::open(dir.path())?;
            s.put(b"a".to_vec(), b"1".to_vec())?;
            s.freeze();
            s.flush()?;
            s.put(b"b".to_vec(), b"2".to_vec())?;
        }
        let s = ToroidalStore::open(dir.path())?;
        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.segment_count(), 1);
        Ok(())
    }

    #[test]
    fn recovery_multiple_segments() -> Result<()> {
        let dir = tempdir().unwrap();
        {
            let s = ToroidalStore::open(dir.path())?;
            s.put(b"a".to_vec(), b"1".to_vec())?;
            s.freeze();
            s.flush()?;
            s.put(b"b".to_vec(), b"2".to_vec())?;
            s.freeze();
            s.flush()?;
        }
        let s = ToroidalStore::open(dir.path())?;
        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.segment_count(), 2);
        Ok(())
    }

    #[test]
    fn concurrent_write_freeze_no_race() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let dir = tempdir().unwrap();
        let store = Arc::new(ToroidalStore::open(dir.path())?);
        let mut handles = Vec::new();

        for t in 0..4 {
            let s = store.clone();
            handles.push(thread::spawn(move || -> Result<()> {
                for i in 0..100 {
                    let key = format!("w{}-{:04}", t, i);
                    s.put(key.into_bytes(), b"v".to_vec())?;
                }
                Ok(())
            }));
        }

        for _ in 0..10 {
            let s = store.clone();
            handles.push(thread::spawn(move || -> Result<()> {
                s.freeze();
                Ok(())
            }));
        }

        for h in handles {
            h.join().unwrap()?;
        }

        for t in 0..4 {
            for i in 0..100 {
                let key = format!("w{}-{:04}", t, i);
                assert!(store.get(&key.into_bytes()).is_some());
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------------
    // Checkpoint tests
    // -------------------------------------------------------------------

    #[test]
    fn checkpoint_flushes_all_and_truncates_wal() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        s.freeze();
        s.put(b"c".to_vec(), b"3".to_vec())?;
        s.freeze();
        s.put(b"d".to_vec(), b"4".to_vec())?;

        let n = s.checkpoint()?;
        assert_eq!(n, 3);
        assert_eq!(s.segment_count(), 3);

        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.get(b"c"), Some(b"3".to_vec()));
        assert_eq!(s.get(b"d"), Some(b"4".to_vec()));
        Ok(())
    }

    #[test]
    fn checkpoint_then_recovery() -> Result<()> {
        let dir = tempdir().unwrap();
        {
            let s = ToroidalStore::open(dir.path())?;
            s.put(b"a".to_vec(), b"1".to_vec())?;
            s.put(b"b".to_vec(), b"2".to_vec())?;
            s.freeze();
            s.put(b"c".to_vec(), b"3".to_vec())?;
            s.checkpoint()?;
        }
        let s = ToroidalStore::open(dir.path())?;
        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.get(b"c"), Some(b"3".to_vec()));
        assert_eq!(s.active_len(), 0);
        Ok(())
    }

    // -------------------------------------------------------------------
    // Compaction tests
    // -------------------------------------------------------------------

    #[test]
    fn compaction_merges_segments() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.flush()?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
        s.freeze();
        s.flush()?;
        s.put(b"c".to_vec(), b"3".to_vec())?;
        s.freeze();
        s.flush()?;

        assert_eq!(s.segment_count(), 3);
        let old_count = s.compact()?;
        assert_eq!(old_count, 3);
        assert_eq!(s.segment_count(), 1);

        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        assert_eq!(s.get(b"c"), Some(b"3".to_vec()));
        Ok(())
    }

    #[test]
    fn compaction_newest_wins() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"k".to_vec(), b"old".to_vec())?;
        s.freeze();
        s.flush()?;
        s.put(b"k".to_vec(), b"new".to_vec())?;
        s.freeze();
        s.flush()?;

        s.compact()?;
        assert_eq!(s.segment_count(), 1);
        assert_eq!(s.get(b"k"), Some(b"new".to_vec()));
        Ok(())
    }

    #[test]
    fn compaction_tombstone_suppression() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.flush()?;
        s.delete(b"a")?;
        s.freeze();
        s.flush()?;

        s.compact()?;
        assert_eq!(s.segment_count(), 1);
        assert_eq!(s.get(b"a"), None);
        Ok(())
    }

    #[test]
    fn compaction_skips_single_segment() -> Result<()> {
        let (_dir, s) = open_store();
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.flush()?;
        assert_eq!(s.compact()?, 1);
        assert_eq!(s.segment_count(), 1);
        Ok(())
    }

    #[test]
    fn compact_then_recovery() -> Result<()> {
        let dir = tempdir().unwrap();
        {
            let s = ToroidalStore::open(dir.path())?;
            s.put(b"a".to_vec(), b"1".to_vec())?;
            s.freeze();
            s.flush()?;
            s.put(b"b".to_vec(), b"2".to_vec())?;
            s.freeze();
            s.flush()?;
            s.compact()?;
        }
        let s = ToroidalStore::open(dir.path())?;
        assert_eq!(s.segment_count(), 1);
        assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
        Ok(())
    }
}
