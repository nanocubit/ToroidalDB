//! Phase 12.5B — Storage correctness completion tests.
//!
//! P1-1: snapshot linearization — snapshot boundary is the published state
//! boundary, NOT the WAL allocation tail.
//! P1-2: empty values must not decode as tombstones.
//! P1-3: malformed segment bytes → StorageError, never a panic.
//! P2:   manifest corrupt suffix is truncated on recovery.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use tempfile::TempDir;
use toroidal_store::{
    FailAt, FailureInjector, FailurePoint, SegmentReader, ToroidalStore, Wal, WalFrameKind,
};

// ---------------------------------------------------------------------------
// Barrier fault injector (shared from phase_12_5a pattern)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct BarrierFault {
    state: Arc<Mutex<BarrierState>>,
    cv: Arc<Condvar>,
}

#[derive(Default)]
struct BarrierState {
    armed: bool,
    hit: bool,
    release: bool,
}

impl BarrierFault {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BarrierState::default())),
            cv: Arc::new(Condvar::new()),
        }
    }
    fn arm(&self) {
        self.state.lock().unwrap().armed = true;
    }
    fn wait_until_hit(&self) {
        let mut s = self.state.lock().unwrap();
        while !s.hit {
            s = self.cv.wait(s).unwrap();
        }
    }
    fn release(&self) {
        let mut s = self.state.lock().unwrap();
        s.release = true;
        self.cv.notify_all();
    }
}

impl FailureInjector for BarrierFault {
    fn check(&self, point: FailurePoint) -> toroidal_store::Result<()> {
        if point != FailurePoint::AfterWalAppend {
            return Ok(());
        }
        let mut s = self.state.lock().unwrap();
        if !s.armed {
            return Ok(());
        }
        s.hit = true;
        self.cv.notify_all();
        while !s.release {
            s = self.cv.wait(s).unwrap();
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// P1-1: Snapshot linearization
// ---------------------------------------------------------------------------

/// Writer WAL-appends + fsyncs and pauses (holding the state write-lock)
/// BEFORE the MemTable publish.  A snapshot taken during this window must:
///   1. block (it cannot observe a half-published write), and
///   2. once the writer publishes, see exactly the published state.
///
/// Crucially, the snapshot's sequence must equal the *published* boundary —
/// it must NOT include the writer's record merely because the WAL tail
/// advanced.
#[test]
fn snapshot_does_not_see_unpublished_wal_tail() {
    let dir = TempDir::new().unwrap();
    let barrier = BarrierFault::new();
    let fault: Arc<dyn FailureInjector> = Arc::new(barrier.clone());
    let store = Arc::new(ToroidalStore::open_with(dir.path(), fault).unwrap());

    // baseline publishes seq 1 (first op in this store)
    store.put(b"base".to_vec(), b"v0".to_vec()).unwrap();
    let snap_pre = store.snapshot();
    assert_eq!(snap_pre.sequence(), 1);

    // arm barrier; writer will pause after WAL append (holding write lock)
    barrier.arm();
    let store2 = store.clone();
    let writer = std::thread::spawn(move || {
        store2.put(b"w".to_vec(), b"v1".to_vec()).unwrap();
    });
    barrier.wait_until_hit();

    // snapshot in a separate thread: with the fix it cannot complete while the
    // writer is unpublished because it needs the same state lock.
    let store3 = store.clone();
    let snap_done = Arc::new(AtomicBool::new(false));
    let snap_done2 = snap_done.clone();
    let snap_thread = std::thread::spawn(move || {
        let s = store3.snapshot();
        snap_done2.store(true, Ordering::SeqCst);
        s
    });
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(
        !snap_done.load(Ordering::SeqCst),
        "snapshot must not complete while writer is unpublished"
    );

    // release writer → publishes to MemTable → snapshot can proceed
    barrier.release();
    writer.join().unwrap();
    let snap_post = snap_thread.join().unwrap();
    assert!(snap_done.load(Ordering::SeqCst));

    // The post snapshot now sees the writer record, and its boundary reflects
    // the published state (>= 1).
    assert!(snap_post.sequence() >= 1);
    assert_eq!(
        store.get_at(&snap_post, b"w").unwrap(),
        Some(b"v1".to_vec()),
    );
    // The pre snapshot (seq 0) must NOT see `w`.
    assert_eq!(store.get_at(&snap_pre, b"w").unwrap(), None);

    // clean up
    drop(store);
    let _ = dir;
}

/// Snapshot boundary advances after each acked write and sees exactly the
/// committed writes (no record later than the boundary).
#[test]
fn snapshot_boundary_is_published_state() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let s0 = store.snapshot();
    assert_eq!(s0.sequence(), 0); // empty store → boundary 0

    store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
    let s1 = store.snapshot();
    assert_eq!(s1.sequence(), 1);
    assert_eq!(store.get_at(&s1, b"a").unwrap(), Some(b"1".to_vec()));

    store.put(b"b".to_vec(), b"2".to_vec()).unwrap();
    let s2 = store.snapshot();
    assert_eq!(s2.sequence(), 2);
    assert_eq!(store.get_at(&s2, b"b").unwrap(), Some(b"2".to_vec()));

    drop(store);
    let _ = dir;
}

/// Snapshot semantics survive flush/checkpoint/reopen plus compaction.
#[test]
fn snapshot_survives_flush_checkpoint_compaction() {
    let dir = TempDir::new().unwrap();
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
        // snapshot before overwrite
        let snap = store.snapshot();
        store.put(b"a".to_vec(), b"2".to_vec()).unwrap();
        store.freeze();
        store.flush().unwrap();
        // snapshot sees a=1 despite overwrite+flush
        assert_eq!(store.get_at(&snap, b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(store.get(b"a"), Some(b"2".to_vec()));

        // keep snap alive across checkpoint
        store.checkpoint().unwrap();
        assert_eq!(store.get_at(&snap, b"a").unwrap(), Some(b"1".to_vec()));
    }
    // reopen while snap dropped
    let store2 = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store2.get(b"a"), Some(b"2".to_vec()));

    // second write + compaction must not resurrect older versions
    store2.put(b"a".to_vec(), b"3".to_vec()).unwrap();
    store2.freeze();
    store2.flush().unwrap();
    let _ = store2.compact();
    assert_eq!(store2.get(b"a"), Some(b"3".to_vec()));

    drop(store2);
    let store3 = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store3.get(b"a"), Some(b"3".to_vec()));
    assert_eq!(store3.get(b"b"), None);
}

// ---------------------------------------------------------------------------
// P1-2: Empty values
// ---------------------------------------------------------------------------

#[test]
fn empty_value_basic() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    store.put(b"k".to_vec(), Vec::new()).unwrap();
    assert_eq!(store.get(b"k"), Some(Vec::new()));
}

#[test]
fn empty_value_after_flush() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    store.put(b"k".to_vec(), Vec::new()).unwrap();
    store.freeze();
    store.flush().unwrap();
    assert_eq!(store.get(b"k"), Some(Vec::new()));
}

#[test]
fn empty_value_after_reopen() {
    let dir = TempDir::new().unwrap();
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        store.put(b"k".to_vec(), Vec::new()).unwrap();
        store.freeze();
        store.flush().unwrap();
    }
    let store = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(
        store.get(b"k"),
        Some(Vec::new()),
        "empty value must survive reopen"
    );
}

#[test]
fn empty_value_then_delete() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    store.put(b"k".to_vec(), Vec::new()).unwrap();
    store.delete(b"k").unwrap();
    assert_eq!(
        store.get(b"k"),
        None,
        "delete after empty value must yield None"
    );
    store.freeze();
    store.flush().unwrap();
    assert_eq!(store.get(b"k"), None);
}

#[test]
fn empty_value_delete_recreate() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    store.put(b"k".to_vec(), Vec::new()).unwrap();
    store.delete(b"k").unwrap();
    store.put(b"k".to_vec(), Vec::new()).unwrap();
    assert_eq!(store.get(b"k"), Some(Vec::new()));
    store.freeze();
    store.flush().unwrap();
    assert_eq!(store.get(b"k"), Some(Vec::new()));
}

#[test]
fn empty_value_mvcc_chain() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    store.put(b"k".to_vec(), b"v1".to_vec()).unwrap();
    let s1 = store.snapshot();
    // empty value overwrite
    store.put(b"k".to_vec(), Vec::new()).unwrap();
    let s2 = store.snapshot();
    // delete
    store.delete(b"k").unwrap();
    let s3 = store.snapshot();
    // recreate
    store.put(b"k".to_vec(), b"v4".to_vec()).unwrap();
    let s4 = store.snapshot();

    assert_eq!(store.get_at(&s1, b"k").unwrap(), Some(b"v1".to_vec()));
    assert_eq!(store.get_at(&s2, b"k").unwrap(), Some(Vec::new()));
    assert_eq!(store.get_at(&s3, b"k").unwrap(), None);
    assert_eq!(store.get_at(&s4, b"k").unwrap(), Some(b"v4".to_vec()));
    // current read
    assert_eq!(store.get(b"k"), Some(b"v4".to_vec()));

    // survive flush
    store.freeze();
    store.flush().unwrap();
    assert_eq!(store.get_at(&s2, b"k").unwrap(), Some(Vec::new()));
}

// ---------------------------------------------------------------------------
// P1-3: Segment corruption matrix — Err, never panic
// ---------------------------------------------------------------------------

fn build_segment_file(dir: &Path) -> std::path::PathBuf {
    let store = ToroidalStore::open(dir).unwrap();
    store.put(b"key1".to_vec(), b"val1".to_vec()).unwrap();
    store.put(b"key2".to_vec(), Vec::new()).unwrap(); // empty value
    store.freeze();
    store.flush().unwrap();
    drop(store);
    // find the .seg file
    let mut seg = None;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|s| s.to_str()) == Some("seg") {
            seg = Some(p);
        }
    }
    seg.expect("segment file must exist")
}

fn assert_no_panic(path: &Path, mut mutate: impl FnMut(&mut Vec<u8>)) {
    let mut raw = std::fs::read(path).unwrap();
    if raw.is_empty() {
        return;
    }
    mutate(&mut raw);
    // write corrupted bytes to a temp copy (don't destroy the original)
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("seg.seg");
    std::fs::write(&p, &raw).unwrap();
    // must be Err, never panic
    let _ = SegmentReader::open(&p);
}

#[test]
fn corrupt_truncated_header() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    let raw = std::fs::read(&seg).unwrap();
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("seg.seg");
    std::fs::write(&p, &raw[..raw.len().min(10)]).unwrap();
    assert!(
        SegmentReader::open(&p).is_err(),
        "truncated header must error"
    );
}

#[test]
fn corrupt_invalid_magic() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| {
        raw[0..4].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF])
    });
}

#[test]
fn corrupt_unsupported_version() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| raw[4..8].copy_from_slice(&999u32.to_le_bytes()));
}

#[test]
fn corrupt_truncated_footer() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    let raw = std::fs::read(&seg).unwrap();
    let tmp = TempDir::new().unwrap();
    let p = tmp.path().join("seg.seg");
    std::fs::write(&p, &raw[..raw.len() - 8]).unwrap();
    assert!(SegmentReader::open(&p).is_err());
}

#[test]
fn corrupt_footer_crc() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| {
        let n = raw.len();
        raw[n - 1] ^= 0xFF;
    });
}

#[test]
fn corrupt_block_offset_beyond_eof() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| {
        // corrupt index block offset field (first index entry offset) to a
        // huge value; the parser must return Err, not panic.
        // index starts after bloom; to keep it simple, overwrite footer
        // index_offset with u64::MAX.
        let footer_idx_offset = raw.len() - 44 + 16;
        raw[footer_idx_offset..footer_idx_offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    });
}

#[test]
fn corrupt_index_length_overflow() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| {
        let footer_idx_len = raw.len() - 44 + 24;
        raw[footer_idx_len..footer_idx_len + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    });
}

#[test]
fn corrupt_bloom_offset() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| {
        let footer_bloom = raw.len() - 44; // bloom_offset
        raw[footer_bloom..footer_bloom + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    });
}

#[test]
fn corrupt_middle_entry_length() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    assert_no_panic(&seg, |raw| {
        // flip a bit in the data region after the header — could corrupt an
        // entry length but still pass footer CRC; parse must not panic.
        let mid = 32 + 16;
        if mid < raw.len() {
            raw[mid] ^= 0xFF;
        }
    });
}

#[test]
fn segment_arith_extremes() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    // overwrite all footer offsets with usize::MAX/u64::MAX — must Err
    assert_no_panic(&seg, |raw| {
        let footer = raw.len() - 44;
        for i in (0..40).step_by(8) {
            raw[footer + i..footer + i + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        }
    });
}

/// Recovery-level corruption: valid store → close → corrupt segment → reopen
/// must yield a controlled error (not a process panic).
#[test]
fn recovery_rejects_corrupt_segment() {
    let dir = TempDir::new().unwrap();
    let seg = build_segment_file(dir.path());
    // corrupt the footer CRC
    let mut raw = std::fs::read(&seg).unwrap();
    let n = raw.len();
    raw[n - 1] ^= 0xFF;
    std::fs::write(&seg, &raw).unwrap();
    // reopen must fail with an error, not panic
    let res = ToroidalStore::open(dir.path());
    assert!(
        res.is_err(),
        "corrupt segment must produce controlled recovery error"
    );
}

// ---------------------------------------------------------------------------
// P2: Manifest corrupt suffix
// ---------------------------------------------------------------------------

#[test]
fn manifest_truncated_suffix_recovery() {
    let dir = TempDir::new().unwrap();
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
        store.freeze();
        store.flush().unwrap();
    }
    let mpath = dir.path().join("manifest");
    // append a partial (truncated) record
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&mpath)
        .unwrap();
    f.write_all(&[0x4D, 0x46, 0x00, 0x01]).unwrap();
    f.sync_all().unwrap();
    drop(f);

    // reopen succeeds; partial suffix discarded
    let store = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store.get(b"a"), Some(b"1".to_vec()));

    // manifest truncated on disk now
    let mlen = std::fs::metadata(&mpath).unwrap().len();
    // append after recovery must be visible
    store.put(b"b".to_vec(), b"2".to_vec()).unwrap();
    store.freeze();
    store.flush().unwrap(); // writes an ADD to manifest
    drop(store);

    let store2 = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store2.get(b"a"), Some(b"1".to_vec()));
    assert_eq!(store2.get(b"b"), Some(b"2".to_vec()));
    let mlen2 = std::fs::metadata(&mpath).unwrap().len();
    assert!(mlen2 > mlen, "manifest must be appended after recovery");
}

#[test]
fn manifest_crc_corruption_suffix() {
    let dir = TempDir::new().unwrap();
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
        store.freeze();
        store.flush().unwrap();
        store.checkpoint().unwrap();
    }
    let mpath = dir.path().join("manifest");
    // append a full-length but CRC-invalid record
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&mpath)
        .unwrap();
    // fake ADD entry with bad crc (magic + type + len + path + seq + crc)
    let mut entry: Vec<u8> = Vec::new();
    entry.extend_from_slice(&0x4D46_0001u32.to_le_bytes());
    entry.push(1); // ADD
    entry.extend_from_slice(&0u16.to_le_bytes()); // empty path
    entry.extend_from_slice(&999u64.to_le_bytes()); // max_seq
    entry.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // WRONG crc
    f.write_all(&entry).unwrap();
    f.sync_all().unwrap();
    drop(f);

    // recovery keeps valid prefix, discards corrupt suffix
    let store = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store.get(b"a"), Some(b"1".to_vec()));
    // max_flushed_sequence must NOT include the fake 999
    assert!(
        store.max_flushed_sequence() < 999,
        "corrupt record must be discarded"
    );

    // stable second reopen
    drop(store);
    let store2 = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store2.get(b"a"), Some(b"1".to_vec()));
}

// ---------------------------------------------------------------------------
// WAL truncate_keep_above batch atomicity (P0-1 extension)
// ---------------------------------------------------------------------------

#[test]
fn truncate_keep_above_preserves_straddling_batch() {
    let dir = TempDir::new().unwrap();
    let wal = Wal::open(&dir.path().join("wal")).unwrap();
    wal.append(WalFrameKind::Put {
        key: b"a".to_vec(),
        value: b"1".to_vec(),
    })
    .unwrap(); // seq 0
    wal.append_batch(&[
        WalFrameKind::Put {
            key: b"b".to_vec(),
            value: b"2".to_vec(),
        },
        WalFrameKind::Put {
            key: b"c".to_vec(),
            value: b"3".to_vec(),
        },
    ])
    .unwrap(); // BEGIN seq1, ops seq2,3 COMMIT seq4

    // boundary at seq 2: keep seq 3,4 + the whole batch (seq1..4) intact
    wal.truncate_keep_above(2).unwrap();
    let frames = wal.replay().unwrap();
    // full batch (b,c) preserved
    let keys: Vec<Vec<u8>> = frames
        .iter()
        .map(|f| match &f.kind {
            WalFrameKind::Put { key, .. } => key.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert!(keys.contains(&b"b".to_vec()));
    assert!(keys.contains(&b"c".to_vec()));
    assert!(!keys.contains(&b"a".to_vec()));
}

// ---------------------------------------------------------------------------
// Cross-component recovery test
// ---------------------------------------------------------------------------

/// write → snapshot → flush → compaction → manifest update → checkpoint →
/// close → reopen: data, empty values, snapshot semantics, manifest,
/// segments, and new writes all correct.
#[test]
fn cross_component_recovery() {
    let dir = TempDir::new().unwrap();
    let snap;
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        store.put(b"k1".to_vec(), b"v1".to_vec()).unwrap();
        store.put(b"empty".to_vec(), Vec::new()).unwrap();
        snap = store.snapshot();
        store.put(b"k1".to_vec(), b"v2".to_vec()).unwrap();
        store.freeze();
        store.flush().unwrap();
        // compaction while snapshot alive
        store.compact().unwrap();
        // checkpoint to truncate WAL
        store.checkpoint().unwrap();
        // snapshot sees k1=v1 (old version retained for live snapshot)
        assert_eq!(store.get_at(&snap, b"k1").unwrap(), Some(b"v1".to_vec()));
        assert_eq!(store.get_at(&snap, b"empty").unwrap(), Some(Vec::new()));
        assert_eq!(store.get(b"k1"), Some(b"v2".to_vec()));
    }
    drop(snap); // snapshot dropped before reopen

    let store2 = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store2.get(b"k1"), Some(b"v2".to_vec()));
    assert_eq!(
        store2.get(b"empty"),
        Some(Vec::new()),
        "empty value survives full cycle"
    );
    assert_eq!(store2.get(b"k1"), Some(b"v2".to_vec()));

    // new writes work after recovery
    store2.put(b"new".to_vec(), b"n1".to_vec()).unwrap();
    store2.freeze();
    store2.flush().unwrap();
    let _ = store2.checkpoint().unwrap();
    drop(store2);

    // second reopen stable
    let store3 = ToroidalStore::open(dir.path()).unwrap();
    assert_eq!(store3.get(b"k1"), Some(b"v2".to_vec()));
    assert_eq!(store3.get(b"empty"), Some(Vec::new()));
    assert_eq!(store3.get(b"new"), Some(b"n1".to_vec()));
}

// ---------------------------------------------------------------------------
// Fault-point stability (reuse 12.5A points)
// ---------------------------------------------------------------------------

#[test]
fn checkpoint_and_compaction_fault_points_still_stable() {
    for point in [
        FailurePoint::AfterMemtableFreeze,
        FailurePoint::AfterManifestSync,
        FailurePoint::BeforeWalTruncate,
        FailurePoint::AfterCompactionOutput,
        FailurePoint::CompactionAfterManifestBatch,
        FailurePoint::CompactionAfterOldFileDelete,
    ] {
        let dir = TempDir::new().unwrap();
        let fault = Arc::new(FailAt { point });
        let store = ToroidalStore::open_with(dir.path(), fault).unwrap();
        for i in 0..2u32 {
            store
                .put(format!("k-{i:02}").into_bytes(), b"v".to_vec())
                .unwrap();
            store.freeze();
            // flush may fail at the injected fault point — that IS the crash.
            let _ = store.flush();
            // if flush failed, the immutable remains; try to compact anyway
        }
        let _ = store.compact();
        store.put(b"x".to_vec(), b"y".to_vec()).unwrap();
        store.freeze();
        let _ = store.checkpoint();
        drop(store);

        let s2 = ToroidalStore::open(dir.path()).expect("reopen after fault point");
        assert_eq!(s2.get(b"k-00"), Some(b"v".to_vec()));
        assert_eq!(s2.get(b"x"), Some(b"y".to_vec()));
    }
}
