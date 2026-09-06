//! Phase 12.5A — Storage correctness repair: crash/recovery tests.
//!
//! P0-1: checkpoint concurrent-writer data loss
//! P0-2: compaction crash publication

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use tempfile::TempDir;
use toroidal_store::{FailAt, FailureInjector, FailurePoint, ToroidalStore, Wal, WalFrameKind};

// ---------------------------------------------------------------------------
// Barrier fault injector — deterministic pause after WAL append
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
    fn is_hit(&self) -> bool {
        self.state.lock().unwrap().hit
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
// P0-1: checkpoint concurrent-writer race — deterministic
// ---------------------------------------------------------------------------

/// The critical regression scenario (deterministic, no sleeps):
///
/// 1. write baseline record (acked)
/// 2. concurrent writer performs WAL append + fsync, then *pauses* at the
///    `AfterWalAppend` fault point — this is the exact instant where the fix
///    holds the state write-lock, so the writer cannot be observed mid-flight.
/// 3. checkpoint runs while the writer is paused; it must *serialize* behind
///    the writer rather than truncate its WAL record.
/// 4. writer resumes (record becomes visible).
/// 5. crash (drop without close) and reopen fresh.
/// 6. both baseline and concurrent records must exist; store writable.
#[test]
fn checkpoint_concurrent_writer_preserved() {
    let dir = TempDir::new().unwrap();
    let barrier = BarrierFault::new();
    let fault: Arc<dyn FailureInjector> = Arc::new(barrier.clone());
    let store = Arc::new(ToroidalStore::open_with(dir.path(), fault).unwrap());

    // 1. baseline
    store.put(b"baseline".to_vec(), b"v0".to_vec()).unwrap();

    // arm: next put pauses after WAL append (holding the state write-lock)
    barrier.arm();

    // 2. concurrent writer
    let store2 = store.clone();
    let writer = std::thread::spawn(move || {
        store2.put(b"concurrent".to_vec(), b"v1".to_vec()).unwrap();
    });
    barrier.wait_until_hit();
    assert!(barrier.is_hit(), "writer must be paused at AfterWalAppend");

    // 3. checkpoint runs in its own thread while the writer is paused.
    let store3 = store.clone();
    let ckpt_done = Arc::new(AtomicBool::new(false));
    let ckpt_done2 = ckpt_done.clone();
    let ckpt = std::thread::spawn(move || {
        let n = store3.checkpoint().unwrap();
        assert!(n >= 1, "checkpoint must flush baseline");
        ckpt_done2.store(true, Ordering::SeqCst);
    });

    // With the fix the writer holds the state lock while paused, so the
    // checkpoint cannot have completed before the writer is released.
    std::thread::yield_now();
    let checkpoint_ran_early = ckpt_done.load(Ordering::SeqCst);
    // (Do not assert on timing — just let both threads finish correctly.)

    // 4. release the writer
    barrier.release();
    writer.join().unwrap();
    ckpt.join().unwrap();
    assert_eq!(store.get(b"concurrent"), Some(b"v1".to_vec()));
    let _ = checkpoint_ran_early;

    // 5. crash + reopen
    drop(store);
    let store4 = ToroidalStore::open(dir.path()).unwrap();

    // 6. both must exist; store writable
    assert_eq!(store4.get(b"baseline"), Some(b"v0".to_vec()));
    assert_eq!(store4.get(b"concurrent"), Some(b"v1".to_vec()));
    store4.put(b"post".to_vec(), b"ok".to_vec()).unwrap();
    assert_eq!(store4.get(b"post"), Some(b"ok".to_vec()));
}

// ---------------------------------------------------------------------------
// P0-1: WAL truncate_keep_above unit
// ---------------------------------------------------------------------------

#[test]
fn truncate_keep_above_preserves_tail() {
    let dir = TempDir::new().unwrap();
    let wal = Wal::open(&dir.path().join("wal")).unwrap();
    wal.append(WalFrameKind::Put {
        key: b"a".to_vec(),
        value: b"1".to_vec(),
    })
    .unwrap(); // seq 0
    wal.append(WalFrameKind::Put {
        key: b"b".to_vec(),
        value: b"2".to_vec(),
    })
    .unwrap(); // seq 1
    wal.append(WalFrameKind::Put {
        key: b"c".to_vec(),
        value: b"3".to_vec(),
    })
    .unwrap(); // seq 2

    // boundary at seq 1: keep seq 2 only
    wal.truncate_keep_above(1).unwrap();
    let frames = wal.replay().unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].sequence, 2);
    assert_eq!(wal.next_sequence(), 3);
}

// ---------------------------------------------------------------------------
// P0-2: compaction crash matrix
// ---------------------------------------------------------------------------

fn build_segments(dir: &Path) {
    let store = ToroidalStore::open(dir).unwrap();
    for i in 0..5u32 {
        store
            .put(format!("k-{:04}", i).into_bytes(), b"v".to_vec())
            .unwrap();
        store.freeze();
        store.flush().unwrap();
    }
    assert!(store.segment_count() >= 2, "need segments to compact");
}

fn crash_and_recover(dir: &Path, point: FailurePoint) {
    build_segments(dir);
    let fault = Arc::new(FailAt { point });
    let store = ToroidalStore::open_with(dir, fault).unwrap();
    let _ = store.compact(); // may fail at injected point (simulated crash)
    drop(store); // crash without close

    // reopen must succeed; all data present; writable
    let s2 = ToroidalStore::open(dir).expect("reopen after compaction crash");
    for i in 0..5u32 {
        let k = format!("k-{:04}", i);
        assert_eq!(s2.get(k.as_bytes()), Some(b"v".to_vec()), "key {k} lost");
    }
    s2.put(b"post".to_vec(), b"ok".to_vec()).unwrap();
    assert_eq!(s2.get(b"post"), Some(b"ok".to_vec()));

    // second reopen stable (recovery must leave a stable state)
    drop(s2);
    let s3 = ToroidalStore::open(dir).expect("second reopen");
    for i in 0..5u32 {
        let k = format!("k-{:04}", i);
        assert_eq!(
            s3.get(k.as_bytes()),
            Some(b"v".to_vec()),
            "key {k} lost after 2nd reopen"
        );
    }
}

#[test]
fn compaction_crash_after_output() {
    crash_and_recover(
        TempDir::new().unwrap().path(),
        FailurePoint::AfterCompactionOutput,
    )
}

#[test]
fn compaction_crash_after_manifest_batch() {
    crash_and_recover(
        TempDir::new().unwrap().path(),
        FailurePoint::CompactionAfterManifestBatch,
    )
}

#[test]
fn compaction_crash_after_old_file_delete() {
    crash_and_recover(
        TempDir::new().unwrap().path(),
        FailurePoint::CompactionAfterOldFileDelete,
    )
}

// ---------------------------------------------------------------------------
// Checkpoint crash matrix
// ---------------------------------------------------------------------------

fn checkpoint_crash_and_recover(dir: &Path, point: FailurePoint) {
    let fault = Arc::new(FailAt { point });
    let store = ToroidalStore::open_with(dir, fault).unwrap();
    store.put(b"a".to_vec(), b"1".to_vec()).unwrap();
    store.freeze();
    let _ = store.checkpoint(); // may fail at injected point
    drop(store); // crash

    let s2 = ToroidalStore::open(dir).expect("reopen after checkpoint crash");
    assert_eq!(s2.get(b"a"), Some(b"1".to_vec()));
    s2.put(b"b".to_vec(), b"2".to_vec()).unwrap();
    assert_eq!(s2.get(b"b"), Some(b"2".to_vec()));

    drop(s2);
    let s3 = ToroidalStore::open(dir).expect("second reopen");
    assert_eq!(s3.get(b"a"), Some(b"1".to_vec()));
    assert_eq!(s3.get(b"b"), Some(b"2".to_vec()));
}

#[test]
fn checkpoint_crash_after_freeze() {
    let dir = TempDir::new().unwrap();
    checkpoint_crash_and_recover(dir.path(), FailurePoint::AfterMemtableFreeze);
}

#[test]
fn checkpoint_crash_after_manifest_sync() {
    let dir = TempDir::new().unwrap();
    checkpoint_crash_and_recover(dir.path(), FailurePoint::AfterManifestSync);
}

#[test]
fn checkpoint_crash_before_wal_truncate() {
    let dir = TempDir::new().unwrap();
    checkpoint_crash_and_recover(dir.path(), FailurePoint::BeforeWalTruncate);
}

// ---------------------------------------------------------------------------
// Manifest segment existence invariant (every fault point)
// ---------------------------------------------------------------------------

#[test]
fn manifest_segment_existence_invariant() {
    for point in [
        FailurePoint::DuringCompaction,
        FailurePoint::AfterCompactionOutput,
        FailurePoint::CompactionAfterManifestBatch,
        FailurePoint::CompactionAfterOldFileDelete,
    ] {
        let dir = TempDir::new().unwrap();
        build_segments(dir.path());
        let fault = Arc::new(FailAt { point });
        let store = ToroidalStore::open_with(dir.path(), fault).unwrap();
        let _ = store.compact();
        drop(store);

        // Reopen must succeed and never reference a missing segment — we
        // verify indirectly: open succeeds, keys present, store writable,
        // second reopen succeeds.
        let s = ToroidalStore::open(dir.path()).expect("reopen must succeed");
        assert_eq!(s.get(b"k-0000"), Some(b"v".to_vec()));
        s.put(b"check".to_vec(), b"ok".to_vec()).unwrap();
        drop(s);
        let s2 = ToroidalStore::open(dir.path()).expect("second reopen must succeed");
        assert_eq!(s2.get(b"check"), Some(b"ok".to_vec()));
    }
}
