//! Stress test — concurrent writers, multiple snapshots, periodic
//! checkpoint/compaction, crash/reopen loop.
//!
//! Marked `#[ignore]` by default so normal `cargo test` stays fast.
//! Run explicitly: `cargo test --test stress --release -- --ignored`

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;
use toroidal_store::ToroidalStore;

const OPS_PER_WRITER: u64 = 8_000;
const NUM_WRITERS: usize = 2;
const CHECKPOINT_EVERY: u64 = 2_000;

#[test]
#[ignore = "stress test — run explicitly in release"]
fn stress_concurrent_writers_snapshots() {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(ToroidalStore::open(dir.path()).unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    // Writer threads
    for t in 0..NUM_WRITERS {
        let s = store.clone();
        let stop = stop.clone();
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_WRITER {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let key = format!("w{}-{:08x}", t, i);
                s.put(key.into_bytes(), b"value".to_vec()).unwrap();

                if i % CHECKPOINT_EVERY == CHECKPOINT_EVERY - 1 {
                    let _ = s.checkpoint();
                }
                if i % 8_000 == 7_999 {
                    let _ = s.compact();
                }
            }
        }));
    }

    // Reader thread with snapshots
    let s = store.clone();
    let stop = stop.clone();
    handles.push(thread::spawn(move || {
        for _ in 0..2_000 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let snap = s.snapshot();
            let _ = s.scan_at(&snap, None, None);
            for t in 0..NUM_WRITERS {
                let key = format!("w{}-{:08x}", t, 0u64);
                let _ = s.get_at(&snap, key.as_bytes());
            }
        }
    }));

    for h in handles {
        h.join().unwrap();
    }

    // Verify all writes visible
    for t in 0..NUM_WRITERS {
        for i in 0..OPS_PER_WRITER {
            let key = format!("w{}-{:08x}", t, i);
            assert!(store.get(key.as_bytes()).is_some(), "missing key {}", key);
        }
    }

    // Recovery — reopen and verify
    let store2 = ToroidalStore::open(dir.path()).unwrap();
    for t in 0..NUM_WRITERS {
        for i in 0..OPS_PER_WRITER {
            let key = format!("w{}-{:08x}", t, i);
            assert!(
                store2.get(key.as_bytes()).is_some(),
                "recovery lost key {}",
                key
            );
        }
    }
}

#[test]
#[ignore = "stress test — run explicitly in release"]
fn stress_crash_reopen_loop() {
    let dir = TempDir::new().unwrap();
    let mut rng: u64 = 42;

    for cycle in 0..5 {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for _ in 0..2_000 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let key = format!("k-{:016x}", rng);
            store.put(key.into_bytes(), b"v".to_vec()).unwrap();
        }
        store.freeze();
        let _ = store.checkpoint();
        drop(store); // simulated crash

        let store2 = ToroidalStore::open(dir.path()).unwrap();
        assert!(
            store2.active_len() > 0 || store2.segment_count() > 0,
            "cycle {}: empty store after reopen",
            cycle
        );
    }
}
