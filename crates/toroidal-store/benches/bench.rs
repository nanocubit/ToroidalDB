use std::time::Instant;
use tempfile::TempDir;
use toroidal_store::{ToroidalStore, WalFrameKind};

fn bench_write_ops() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let n = 100_000;
    let start = Instant::now();
    for i in 0..n {
        store
            .put(
                format!("key-{:08}", i).into_bytes(),
                b"value-1234567890".to_vec(),
            )
            .unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "PUT {} ops: {:?} ({:.0} ops/sec)",
        n,
        elapsed,
        n as f64 / elapsed.as_secs_f64()
    );
}

fn bench_batch_ops() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let n = 100_000;
    let batch_size = 1000;
    let start = Instant::now();
    for i in (0..n).step_by(batch_size) {
        let batch: Vec<WalFrameKind> = (0..batch_size)
            .map(|j| WalFrameKind::Put {
                key: format!("key-{:08}", i + j).into_bytes(),
                value: b"value-1234567890".to_vec(),
            })
            .collect();
        store.batch(&batch).unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "BATCH {} ops (batch={}): {:?} ({:.0} ops/sec)",
        n,
        batch_size,
        elapsed,
        n as f64 / elapsed.as_secs_f64()
    );
}

fn bench_read() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let n = 100_000;
    for i in 0..n {
        store
            .put(format!("key-{:08}", i).into_bytes(), b"value".to_vec())
            .unwrap();
    }
    let start = Instant::now();
    for i in 0..n {
        let _ = store.get(&format!("key-{:08}", i).into_bytes());
    }
    let elapsed = start.elapsed();
    println!(
        "GET {} ops: {:?} ({:.0} ops/sec)",
        n,
        elapsed,
        n as f64 / elapsed.as_secs_f64()
    );
}

fn bench_scan() {
    let dir = TempDir::new().unwrap();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let n = 100_000;
    for i in 0..n {
        store
            .put(format!("key-{:08}", i).into_bytes(), b"value".to_vec())
            .unwrap();
    }
    let start = Instant::now();
    let results = store.scan(Some(b"key-00000000"), Some(b"key-00010000"));
    let elapsed = start.elapsed();
    println!(
        "SCAN {} results: {:?} ({:.0} items/sec)",
        results.len(),
        elapsed,
        results.len() as f64 / elapsed.as_secs_f64()
    );
}

fn bench_crash_recovery() {
    let dir = TempDir::new().unwrap();
    let n = 10_000;
    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..n {
            store
                .put(format!("key-{:08}", i).into_bytes(), b"value".to_vec())
                .unwrap();
        }
    }
    let start = Instant::now();
    let store = ToroidalStore::open(dir.path()).unwrap();
    let elapsed = start.elapsed();
    let count = (0..n)
        .filter(|i| store.get(&format!("key-{:08}", i).into_bytes()).is_some())
        .count();
    println!("RECOVERY {} nodes: {:?}, recovered: {}", n, elapsed, count);
}

fn main() {
    println!("=== ToroidalStore Benchmarks ===\n");
    for (name, f) in [
        ("Write (sequential PUT)", bench_write_ops as fn()),
        ("Batch write", bench_batch_ops),
        ("Read (sequential GET)", bench_read),
        ("Scan range", bench_scan),
        ("Crash recovery", bench_crash_recovery),
    ] {
        println!("--- {} ---", name);
        f();
        println!();
    }
}
