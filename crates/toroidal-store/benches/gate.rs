//! Benchmark Gate — expanded workload matrix (Phase 7D, schema v2 per Phase 10A).
//!
//! Protocol:
//! 1. warmup: N runs (page cache / allocator / CPU freq)
//! 2. measure: M runs
//! 3. stats: median (p50), p95, p99
//!
//! Output: pipe-delimited `name|key=value` lines with **explicit units**:
//!   sample_unit     — what one measurement sample covers (operation, batch, scan, reopen)
//!   throughput_unit — unit of the published throughput (puts/s, entries/s, items/s, recoveries/s)
//!   latency_unit    — what the latency applies to (put, batch commit, scan, recovery)
//!
//! Every line carries full workload metadata (entries, bytes, warmup, measure),
//! so results are comparable across runs/machines (Phase 7C compare).

use std::io::Write;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use toroidal_store::cache::{Cache, SegmentReadMode};
use toroidal_store::{SegmentReader, ToroidalStore, WalFrameKind};

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

const WARMUP: usize = 10;
const MEASURE: usize = 25;
const SEED: u64 = 42;

/// Deterministic xorshift64 PRNG.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Deterministic key set (seed-based).
fn gen_keys(n: usize) -> Vec<Vec<u8>> {
    let mut rng = Rng(SEED);
    (0..n)
        .map(|_| format!("key-{:016x}", rng.next()).into_bytes())
        .collect()
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn summarize(samples_ns: &[u64]) -> (f64, f64, f64) {
    let mut v: Vec<f64> = samples_ns.iter().map(|&x| x as f64).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (
        percentile(&v, 0.50),
        percentile(&v, 0.95),
        percentile(&v, 0.99),
    )
}

fn sample<F: FnMut()>(mut f: F) -> Vec<u64> {
    for _ in 0..WARMUP {
        f();
    }
    let mut s = Vec::with_capacity(MEASURE);
    for _ in 0..MEASURE {
        let t = Instant::now();
        f();
        s.push(t.elapsed().as_nanos() as u64);
    }
    s
}

/// Report one workload with explicit, unambiguous units.
///
/// `operations` is the number of operations performed per sample, `entries`
/// the number of logical key ops (== operations for single-put, == batch size
/// for batch, == item count for scan, == 1 for reopen), `bytes` the dataset
/// bytes per sample.
#[allow(clippy::too_many_arguments)]
fn report(
    workload: &str,
    sample_unit: &str,
    throughput_unit: &str,
    latency_unit: &str,
    operations: u64,
    entries: u64,
    bytes: u64,
    samples_ns: &[u64],
) {
    let (p50, p95, p99) = summarize(samples_ns);
    // Throughput uses `operations` as the counting unit (e.g. 1000 entries per
    // batch sample, 1 reopen per recovery sample).
    let throughput = if p50 > 0.0 {
        operations as f64 / (p50 / 1e9)
    } else {
        0.0
    };
    println!(
        "{}|sample_unit={}|throughput_unit={}|latency_unit={}|operations={}|entries={}|bytes={}|warmup={}|measure={}|p50_ns={:.0}|p95_ns={:.0}|p99_ns={:.0}|throughput={:.0}",
        workload, sample_unit, throughput_unit, latency_unit,
        operations, entries, bytes,
        WARMUP, samples_ns.len(),
        p50, p95, p99, throughput,
    );
}

// ---------------------------------------------------------------------------
// Write workloads
// ---------------------------------------------------------------------------

fn run_put_sync() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(2000);
    let samples = sample(|| {
        for k in &keys {
            store.put(k.clone(), b"value".to_vec()).unwrap();
        }
    });
    report(
        "put_sync_2000",
        "operation",
        "puts_per_second",
        "put",
        2000,
        2000,
        2000 * 5,
        &samples,
    );
}

fn run_batch_sizes(size: usize) {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(size);
    let ops: Vec<WalFrameKind> = keys
        .iter()
        .map(|k| WalFrameKind::Put {
            key: k.clone(),
            value: b"value".to_vec(),
        })
        .collect();
    for _ in 0..3 {
        let _ = store.batch(&ops);
    }
    let samples = sample(|| {
        let _ = store.batch(&ops);
    });
    let label = format!("batch_{}_puts", size);
    // batch latency is publishable via report(); we pass operations = size
    // so throughput_unit "entries_per_second" gives entries/s directly.
    report(
        &label,               // workload
        "batch",              // sample_unit
        "entries_per_second", // throughput_unit (size entries per commit)
        "batch_commit",       // latency_unit
        size as u64,          // operations counted in throughput
        size as u64,          // entries per sample
        size as u64 * 5,      // bytes (5-byte values)
        &samples,
    );
}

fn run_delete_heavy() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(2000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    let samples = sample(|| {
        for k in &keys {
            store.delete(k).unwrap();
        }
    });
    report(
        "delete_heavy_2000",
        "operation",
        "deletes_per_second",
        "delete",
        2000,
        2000,
        0,
        &samples,
    );
}

fn run_large_values() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(200);
    let big = vec![b'x'; 4096];
    let samples = sample(|| {
        for k in &keys {
            store.put(k.clone(), big.clone()).unwrap();
        }
    });
    report(
        "put_large_4KiB_200",
        "operation",
        "puts_per_second",
        "put",
        200,
        200,
        200 * 4096,
        &samples,
    );
}

// ---------------------------------------------------------------------------
// Read workloads
// ---------------------------------------------------------------------------

fn run_memtable_get() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    let samples = sample(|| {
        for k in &keys {
            let _ = store.get(k);
        }
    });
    report(
        "memtable_get",
        "operation",
        "gets_per_second",
        "get",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_memtable_get_at() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    let snap = store.snapshot();
    let samples = sample(|| {
        for k in &keys {
            let _ = store.get_at(&snap, k).unwrap();
        }
    });
    report(
        "memtable_get_at_snapshot",
        "operation",
        "gets_per_second",
        "get_at",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_seg_get_predecoded() {
    let dir = TempDir::new().unwrap();
    let keys = gen_keys(10_000);
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for k in &keys {
            store.put(k.clone(), b"value".to_vec()).unwrap();
        }
        store.freeze();
        store.flush().unwrap();
    }
    // Find segment file — ID may start at 1.
    let seg_path = segment_file_path(dir.path());
    let reader = SegmentReader::open(&seg_path).unwrap();
    let samples = sample(|| {
        for k in &keys {
            let _ = reader.get(k);
        }
    });
    report(
        "seg_get_predecoded",
        "operation",
        "gets_per_second",
        "get",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_seg_get_blockcached(cold: bool) {
    let dir = TempDir::new().unwrap();
    let keys = gen_keys(10_000);
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for k in &keys {
            store.put(k.clone(), b"value".to_vec()).unwrap();
        }
        store.freeze();
        store.flush().unwrap();
    }
    let seg_path = segment_file_path(dir.path());
    let cache: Arc<dyn toroidal_store::cache::BlockCache> = Arc::new(Cache::new(1 << 20, 4));
    if !cold {
        let w = SegmentReader::open_with(&seg_path, SegmentReadMode::BlockCached, cache.clone())
            .unwrap();
        for k in &keys {
            let _ = w.get(k);
        }
    }
    let reader =
        SegmentReader::open_with(&seg_path, SegmentReadMode::BlockCached, cache.clone()).unwrap();
    let samples = sample(|| {
        for k in &keys {
            let _ = reader.get(k);
        }
    });
    let label = if cold {
        "seg_get_blockcached_cold"
    } else {
        "seg_get_blockcached_warm"
    };
    report(
        label,
        "operation",
        "gets_per_second",
        "get",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

/// Find the first `.seg` file in the directory.
fn segment_file_path(dir: &std::path::Path) -> std::path::PathBuf {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|s| s.to_str()) == Some("seg") {
            return p;
        }
    }
    panic!("no segment file found in {:?}", dir);
}

fn run_mixed(read_ratio: f64, label: &str) {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    let samples = sample(|| {
        for (i, k) in keys.iter().enumerate() {
            if i % 100 < (read_ratio * 100.0) as usize {
                let _ = store.get(k);
            } else {
                store.put(k.clone(), b"value2".to_vec()).unwrap();
            }
        }
    });
    report(
        label,
        "operation",
        "ops_per_second",
        "mixed_op",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

// ---------------------------------------------------------------------------
// Scan workloads
// ---------------------------------------------------------------------------

fn run_scan_short() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    store.freeze();
    store.flush().unwrap();
    let mid = keys[5000].clone();
    let samples = sample(|| {
        let r = store.scan(Some(&mid[..mid.len() / 2]), Some(&mid));
        assert!(!r.is_empty());
    });
    report(
        "scan_short_5000",
        "scan",
        "items_per_second",
        "scan",
        2500,
        2500,
        2500 * 5,
        &samples,
    );
}

fn run_scan_full() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    store.freeze();
    store.flush().unwrap();
    let samples = sample(|| {
        let r = store.scan(None, None);
        assert_eq!(r.len(), 10_000);
    });
    report(
        "scan_full_10K",
        "scan",
        "items_per_second",
        "scan",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_scan_at() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    let snap = store.snapshot();
    store.put(b"zzz".to_vec(), b"hidden".to_vec()).unwrap();
    let samples = sample(|| {
        let r = store.scan_at(&snap, None, None).unwrap();
        assert_eq!(r.len(), 10_000);
    });
    report(
        "scan_at_snapshot_10K",
        "scan",
        "items_per_second",
        "scan_at",
        10_000,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_scan_overlap() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    for seg in 0..5u32 {
        for i in 0..2000u32 {
            let key = format!("key-{:08x}", i * 7 + seg * 3);
            store
                .put(key.into_bytes(), format!("v{}", seg).into_bytes())
                .unwrap();
        }
        store.freeze();
        store.flush().unwrap();
    }
    let samples = sample(|| {
        let r = store.scan(None, None);
        assert_eq!(r.len(), 1999);
    });
    report(
        "scan_overlap_5seg",
        "scan",
        "items_per_second",
        "scan",
        1999,
        1999,
        1999 * 2,
        &samples,
    );
}

fn run_scan_tombstone() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    store.freeze();
    for k in keys.iter().step_by(2) {
        store.delete(k).unwrap();
    }
    store.freeze();
    store.flush().unwrap();
    let samples = sample(|| {
        let r = store.scan(None, None);
        assert_eq!(r.len(), 5000);
    });
    report(
        "scan_tombstone_half",
        "scan",
        "items_per_second",
        "scan",
        5000,
        5000,
        5000 * 5,
        &samples,
    );
}

// ---------------------------------------------------------------------------
// Lifecycle workloads
// ---------------------------------------------------------------------------

fn run_freeze() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
    }
    let samples = sample(|| {
        store.freeze();
    });
    report(
        "freeze_10K",
        "freeze",
        "freezes_per_second",
        "freeze",
        1,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_checkpoint() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let keys = gen_keys(10_000);
    for k in &keys {
        store.put(k.clone(), b"value".to_vec()).unwrap();
        if store.active_len() % 1000 == 999 {
            store.freeze();
        }
    }
    let samples = sample(|| {
        let _ = store.checkpoint();
    });
    report(
        "checkpoint_10K_10imm",
        "checkpoint",
        "checkpoints_per_second",
        "checkpoint",
        1,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_compact() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    for seg in 0..10u32 {
        for i in 0..1000u32 {
            let key = format!("key-{:08x}", i * 3 + seg);
            store
                .put(key.into_bytes(), format!("v{}", seg).into_bytes())
                .unwrap();
        }
        store.freeze();
        store.flush().unwrap();
    }
    let samples = sample(|| {
        let _ = store.compact();
    });
    report(
        "compact_10seg_10K",
        "compaction",
        "compactions_per_second",
        "compaction",
        1,
        10_000,
        10_000 * 2,
        &samples,
    );
}

fn run_recovery() {
    let dir = TempDir::new().unwrap();
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for k in gen_keys(10_000) {
            store.put(k, b"value".to_vec()).unwrap();
        }
        store.freeze();
        store.flush().unwrap();
        for k in gen_keys(1000) {
            store.put(k, b"value".to_vec()).unwrap();
        }
    }
    let samples = sample(|| {
        let store = ToroidalStore::open(dir.path()).unwrap();
        assert_eq!(store.segment_count(), 1);
        drop(store);
    });
    report(
        "recovery_10K_seg_1K_wal",
        "reopen",
        "recoveries_per_second",
        "recovery",
        1,
        11_000,
        11_000 * 5,
        &samples,
    );
}

fn run_recovery_partial_wal() {
    let dir = TempDir::new().unwrap();
    let water = dir.path().join("wal");
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for k in gen_keys(5000) {
            store.put(k, b"value".to_vec()).unwrap();
        }
        store.freeze();
        store.flush().unwrap();
        for k in gen_keys(5000) {
            store.put(k, b"value".to_vec()).unwrap();
        }
        std::fs::OpenOptions::new()
            .append(true)
            .open(&water)
            .unwrap()
            .write_all(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
            .unwrap();
    }
    let samples = sample(|| {
        let store = ToroidalStore::open(dir.path()).unwrap();
        assert!(store.segment_count() >= 1);
        drop(store);
    });
    report(
        "recovery_partial_wal_tail",
        "reopen",
        "recoveries_per_second",
        "recovery",
        1,
        10_000,
        10_000 * 5,
        &samples,
    );
}

fn run_recovery_interrupted_compact() {
    let dir = TempDir::new().unwrap();
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for seg in 0..4u32 {
            for i in 0..500u32 {
                let key = format!("key-{:08x}", i * 5 + seg);
                store
                    .put(key.into_bytes(), format!("v{}", seg).into_bytes())
                    .unwrap();
            }
            store.freeze();
            store.flush().unwrap();
        }
        store.compact().unwrap();
    }
    let samples = sample(|| {
        let store = ToroidalStore::open(dir.path()).unwrap();
        assert!(store.segment_count() >= 1);
        drop(store);
    });
    report(
        "recovery_after_compact",
        "reopen",
        "recoveries_per_second",
        "recovery",
        1,
        2000,
        2000 * 2,
        &samples,
    );
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    println!("# benchmark-gate v2 seed={SEED} warmup={WARMUP} measure={MEASURE}");
    // Write
    run_put_sync();
    run_batch_sizes(1);
    run_batch_sizes(32);
    run_batch_sizes(256);
    run_batch_sizes(1000);
    run_delete_heavy();
    run_large_values();
    // Read
    run_memtable_get();
    run_memtable_get_at();
    run_seg_get_predecoded();
    run_seg_get_blockcached(true);
    run_seg_get_blockcached(false);
    run_mixed(0.95, "mixed_read95_write5");
    run_mixed(0.80, "mixed_read80_write20");
    // Scan
    run_scan_short();
    run_scan_full();
    run_scan_at();
    run_scan_overlap();
    run_scan_tombstone();
    // Lifecycle
    run_freeze();
    run_checkpoint();
    run_compact();
    run_recovery();
    run_recovery_partial_wal();
    run_recovery_interrupted_compact();
}
