//! Isolated: batch puts → checkpoint → reopen → count.
//! Focus: does data persist through checkpoint when written via batch()?

use tempfile::TempDir;
use toroidal_store::{ToroidalStore, WalFrameKind};

fn put_via_batch(store: &ToroidalStore, i: u64) {
    let key = format!("node:{:08x}", i).into_bytes();
    store
        .batch(&[WalFrameKind::Put {
            key,
            value: b"value".to_vec(),
        }])
        .unwrap();
}

#[test]
fn batch_checkpoint_reopen_roundtrip() {
    let dir = TempDir::new().unwrap();
    let n = 50;

    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..n {
            put_via_batch(&store, i);
        }
        // Verify all visible before checkpoint.
        for i in 0..n {
            let key = format!("node:{:08x}", i).into_bytes();
            assert!(store.get(&key).is_some(), "missing {} before checkpoint", i);
        }
        // Explicit checkpoint.
        let written = store.checkpoint().unwrap();
        eprintln!("checkpoint wrote {} segments", written);
        // Verify still visible after checkpoint.
        for i in 0..n {
            let key = format!("node:{:08x}", i).into_bytes();
            assert!(store.get(&key).is_some(), "missing {} after checkpoint", i);
        }
    }

    let store = ToroidalStore::open(dir.path()).unwrap();
    eprintln!(
        "after reopen: segments={} next_seq={}",
        store.segment_count(),
        store.next_sequence()
    );
    let mut found = 0;
    for i in 0..n {
        let key = format!("node:{:08x}", i).into_bytes();
        if store.get(&key).is_some() {
            found += 1;
        } else {
            eprintln!("  MISSING after reopen: {}", i);
        }
    }
    eprintln!("found after reopen: {}", found);
    assert_eq!(found, n, "data lost across checkpoint+reopen");
}

#[test]
fn put_checkpoint_reopen_roundtrip() {
    let dir = TempDir::new().unwrap();
    let n = 50;

    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..n {
            let key = format!("node:{:08x}", i).into_bytes();
            store.put(key, b"value".to_vec()).unwrap();
        }
        store.checkpoint().unwrap();
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
    assert_eq!(found, n, "data lost with put+checkpoint");
}
