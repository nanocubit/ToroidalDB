//! Isolated diagnostic: 100 batch ops (1 put each) → reopen → count.
//! Tests whether the engine loses data across reopen with many small batches.

use tempfile::TempDir;
use toroidal_store::{ToroidalStore, WalFrameKind};

#[test]
fn many_small_batches_survive_reopen() {
    let dir = TempDir::new().unwrap();
    let n = 100;

    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..n {
            let key = format!("node:{:08x}", i).into_bytes();
            store
                .batch(&[WalFrameKind::Put {
                    key,
                    value: b"value".to_vec(),
                }])
                .unwrap();
        }
        // Sanity: all visible before reopen.
        for i in 0..n {
            let key = format!("node:{:08x}", i).into_bytes();
            assert!(store.get(&key).is_some(), "missing key {} before reopen", i);
        }
        eprintln!("before reopen: n={}", n);
    }

    let store = ToroidalStore::open(dir.path()).unwrap();
    eprintln!("after reopen: next_sequence={}", store.next_sequence());
    let mut found = 0;
    for i in 0..n {
        let key = format!("node:{:08x}", i).into_bytes();
        if store.get(&key).is_some() {
            found += 1;
        } else {
            eprintln!("  MISSING after reopen: key {}", i);
        }
    }
    eprintln!("found after reopen: {}", found);
    assert_eq!(found, n, "data lost across reopen with small batches");
}

#[test]
fn many_puts_survive_reopen() {
    let dir = TempDir::new().unwrap();
    let n = 100;

    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..n {
            let key = format!("node:{:08x}", i).into_bytes();
            store.put(key, b"value".to_vec()).unwrap();
        }
    }

    let store = ToroidalStore::open(dir.path()).unwrap();
    let mut found = 0;
    for i in 0..n {
        let key = format!("node:{:08x}", i).into_bytes();
        if store.get(&key).is_some() {
            found += 1;
        }
    }
    eprintln!("puts found after reopen: {}", found);
    assert_eq!(found, n, "data lost across reopen with puts");
}
