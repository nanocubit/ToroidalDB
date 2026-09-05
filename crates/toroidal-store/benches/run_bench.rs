use std::time::Instant;
use tempfile::TempDir;
use toroidal_store::{ToroidalStore, WalFrameKind};

fn main() {
    println!("=== ToroidalStore Benchmarks (release) ===\n");

    // PUT sequential
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
    let t = start.elapsed();
    println!(
        "PUT {} ops: {:?} ({:.0} ops/sec)",
        n,
        t,
        n as f64 / t.as_secs_f64()
    );

    // GET sequential
    let start = Instant::now();
    for i in 0..n {
        let _ = store.get(&format!("key-{:08}", i).into_bytes());
    }
    let t = start.elapsed();
    println!(
        "GET {} ops: {:?} ({:.0} ops/sec)",
        n,
        t,
        n as f64 / t.as_secs_f64()
    );

    // BATCH write
    let dir2 = TempDir::new().unwrap();
    let store2 = ToroidalStore::open(dir2.path()).unwrap();
    let batch_size = 1000;
    let start = Instant::now();
    for i in (0..n).step_by(batch_size) {
        let batch: Vec<WalFrameKind> = (0..batch_size)
            .map(|j| WalFrameKind::Put {
                key: format!("batch-key-{:08}", i + j).into_bytes(),
                value: b"value-1234567890".to_vec(),
            })
            .collect();
        store2.batch(&batch).unwrap();
    }
    let t = start.elapsed();
    println!(
        "BATCH {} ops (batch={}): {:?} ({:.0} ops/sec)",
        n,
        batch_size,
        t,
        n as f64 / t.as_secs_f64()
    );

    // SCAN
    let start = Instant::now();
    let results = store.scan(Some(b"key-00000000"), Some(b"key-00010000"));
    let t = start.elapsed();
    println!(
        "SCAN {} results: {:?} ({:.0} items/sec)",
        results.len(),
        t,
        results.len() as f64 / t.as_secs_f64()
    );

    // Crash recovery
    let dir3 = TempDir::new().unwrap();
    let n_rec = 10_000;
    {
        let s = ToroidalStore::open(dir3.path()).unwrap();
        for i in 0..n_rec {
            s.put(format!("key-{:08}", i).into_bytes(), b"value".to_vec())
                .unwrap();
        }
    }
    let start = Instant::now();
    let s = ToroidalStore::open(dir3.path()).unwrap();
    let t = start.elapsed();
    let recovered = (0..n_rec)
        .filter(|i| s.get(&format!("key-{:08}", i).into_bytes()).is_some())
        .count();
    println!(
        "RECOVERY {} nodes: {:?}, recovered: {}/{}",
        n_rec, t, recovered, n_rec
    );
}
