//! Phase 12.2 — benchmark of the production WalBatcher group commit
//! vs the legacy per-op store.put path.
//! Benchmark-only harness; uses the production batcher crate.

use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use toroidal_store::{BatcherOptions, ToroidalStore, Wal, WalBatcher, WalFrameKind};

/// `iterations` calls load the `f`; `ops_per_iter` is how many logical
/// operations each call submits.  Reports per-operation throughput.
fn bench(label: &str, iterations: usize, ops_per_iter: usize, mut f: impl FnMut()) {
    for _ in 0..20 {
        f();
    }
    let t = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let el = t.elapsed();
    let per_call_us = el.as_secs_f64() / iterations as f64 * 1e6;
    let total_ops = (iterations * ops_per_iter) as f64;
    let ops_per_sec = total_ops / el.as_secs_f64();
    println!(
        "{label:<50} {:>9} ops  {:>10.1}us/call  {:>12.0} ops/s  ({:>6.1}us/op)",
        iterations * ops_per_iter,
        per_call_us,
        ops_per_sec,
        per_call_us / ops_per_iter as f64,
    );
}

fn key_seq(i: usize) -> Vec<u8> {
    format!("key-{:08x}", i).into_bytes()
}

const VAL: &[u8] = b"benchmark-value-64bytes";

fn main() {
    let iters = 5_000;

    // 1. Legacy: store.put() — buffered per-op write (NO fsync on put)
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        let mut i = 0usize;
        bench("legacy store.put() (no fsync)", iters, 1, || {
            store.put(key_seq(i), VAL.to_vec()).unwrap();
            i += 1;
        });
    }

    // 2-5. Batcher: one fsync per durability unit of group_size
    for (label, group) in [
        ("batcher group=1", 1),
        ("batcher group=8", 8),
        ("batcher group=64", 64),
        ("batcher group=512", 512),
    ] {
        let dir = TempDir::new().unwrap();
        let wal = Arc::new(Wal::open(&dir.path().join("wal")).unwrap());
        let b = WalBatcher::new(
            wal,
            BatcherOptions {
                max_batch_size: group.max(1024),
                max_wait: std::time::Duration::from_millis(1),
            },
        );
        let mut i = 0usize;
        bench(label, iters, group, || {
            let ops: Vec<WalFrameKind> = (0..group)
                .map(|j| WalFrameKind::Put {
                    key: key_seq(i + j),
                    value: VAL.to_vec(),
                })
                .collect();
            b.append_many_sync(&ops).unwrap();
            i += group;
        });
        b.shutdown().unwrap();
    }

    // 6. Read regression: store.get() after group-commit writes
    {
        let dir = TempDir::new().unwrap();
        let wal = Arc::new(Wal::open(&dir.path().join("wal")).unwrap());
        let b = WalBatcher::new(
            wal,
            BatcherOptions {
                max_batch_size: 1024,
                max_wait: std::time::Duration::from_millis(1),
            },
        );
        for i in 0..10_000 {
            let op = WalFrameKind::Put {
                key: key_seq(i),
                value: VAL.to_vec(),
            };
            b.append_many_sync(&[op]).unwrap();
        }
        b.shutdown().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        let mut j = 0usize;
        bench("read store.get() after group commit", 10_000, 1, || {
            let _ = store.get(&key_seq(j % 10_000));
            j += 1;
        });
    }
}
