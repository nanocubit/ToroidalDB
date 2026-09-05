//! Quick bench — только чистое измерение, setup исключён.
use std::time::Instant;
use tempfile::TempDir;
use toroidal_store::{ToroidalStore, WalFrameKind};

fn bench(label: &str, n: u64, setup: impl Fn(), f: impl Fn()) {
    setup();
    let start = Instant::now();
    f();
    let elapsed = start.elapsed();
    let ops = n as f64 / elapsed.as_secs_f64();
    println!("{label:30} {n:>8} ops  {elapsed:>8.3?}  {ops:>10.0} ops/s");
}

fn main() {
    // 1. WAL fsync microbench
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("fsync_test");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        let start = Instant::now();
        for _ in 0..1000 {
            file.write_all(&[0u8; 64]).unwrap();
            file.sync_all().unwrap();
        }
        let elapsed = start.elapsed();
        println!(
            "raw fsync (1000 × 64b)              1000 ops   {:>8.3?}  {:>10.0} ops/s",
            elapsed,
            1000f64 / elapsed.as_secs_f64()
        );
    }

    // 2. PUT — чистые puts без open()
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        // прогрев
        for i in 0..100 {
            store
                .put(format!("warm-{:04}", i).into_bytes(), b"v".to_vec())
                .unwrap();
        }
        bench(
            "put (sync wal, 1K)",
            1000,
            || {},
            || {
                for i in 0..1000 {
                    store
                        .put(
                            format!("key-{:04}", i).into_bytes(),
                            b"value-1234567890".to_vec(),
                        )
                        .unwrap();
                }
            },
        );
    }

    // 3. GET hot (memtable)
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..10000 {
            store
                .put(format!("key-{:04}", i).into_bytes(), b"value".to_vec())
                .unwrap();
        }
        bench(
            "get (memtable, 10K)",
            10000,
            || {},
            || {
                for i in 0..10000 {
                    let _ = store.get(&format!("key-{:04}", i).into_bytes());
                }
            },
        );
    }

    // 4. GET segment (все данные в одном сегменте)
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..10000 {
            store
                .put(format!("key-{:04}", i).into_bytes(), b"value".to_vec())
                .unwrap();
        }
        store.freeze();
        store.flush().unwrap();
        bench(
            "get (segment, 10K)",
            10000,
            || {},
            || {
                for i in (0..10000).rev() {
                    let _ = store.get(&format!("key-{:04}", i).into_bytes());
                }
            },
        );
    }

    // 5. BATCH — что даёт группировка
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        let mut ops = Vec::with_capacity(1000);
        for i in 0..1000 {
            ops.push(WalFrameKind::Put {
                key: format!("key-{:04}", i).into_bytes(),
                value: b"value-1234567890".to_vec(),
            });
        }
        bench(
            "batch (1K puts, 1 fsync)",
            1000,
            || {},
            || {
                store.batch(&ops).unwrap();
            },
        );
    }

    // 6. FLUSH — запись сегмента
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..1000 {
            store
                .put(format!("key-{:04}", i).into_bytes(), b"value".to_vec())
                .unwrap();
        }
        store.freeze();
        bench(
            "flush (1K entries)",
            1,
            || {},
            || {
                store.flush().unwrap();
            },
        );
    }

    // 7. CHECKPOINT — только тайминг checkpoint, без setup
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        for _ in 0..10 {
            for i in 0..1000 {
                store
                    .put(format!("key-{:04}", i).into_bytes(), b"value".to_vec())
                    .unwrap();
            }
            store.freeze();
        }
        // data ready in immutables
        bench(
            "checkpoint (10 imm, 10K)",
            1,
            || {},
            || {
                store.checkpoint().unwrap();
            },
        );
    }

    // 8. COMPACT
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        for s in 0..50 {
            for i in 0..100 {
                let key = format!("key-{:04}", (s * 100 + i) % 5000);
                store
                    .put(key.into_bytes(), format!("val-{:04}", s).into_bytes())
                    .unwrap();
            }
            store.freeze();
            store.flush().unwrap();
        }
        bench(
            "compact (50 seg, 5K keys)",
            1,
            || {},
            || {
                store.compact().unwrap();
            },
        );
    }

    // 9. SCAN — все слои
    {
        let dir = TempDir::new().unwrap();
        let store = ToroidalStore::open(dir.path()).unwrap();
        for _ in 0..10 {
            for i in 0..100 {
                store
                    .put(format!("key-{:04}", i).into_bytes(), b"value".to_vec())
                    .unwrap();
            }
            store.freeze();
            store.flush().unwrap();
        }
        bench(
            "scan (10 seg, 100 keys)",
            100,
            || {},
            || {
                let r = store.scan(None, None);
                assert_eq!(r.len(), 100);
            },
        );
    }

    // 10. RECOVERY — сегмент + WAL
    {
        let dir = TempDir::new().unwrap();
        {
            let store = ToroidalStore::open(dir.path()).unwrap();
            for i in 0..10000 {
                store
                    .put(format!("key-{:04}", i).into_bytes(), b"value".to_vec())
                    .unwrap();
            }
            store.freeze();
            store.flush().unwrap();
            for i in 0..1000 {
                store
                    .put(format!("post-{:04}", i).into_bytes(), b"value".to_vec())
                    .unwrap();
            }
        }
        bench(
            "recovery (10K seg + 1K wal)",
            1,
            || {},
            || {
                let store = ToroidalStore::open(dir.path()).unwrap();
                assert_eq!(store.segment_count(), 1);
                let _ = store.get(b"key-0000");
                std::mem::drop(store);
            },
        );
    }
}
