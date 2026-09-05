//! Isolated: scan() vs scan_at(snapshot) after flush+reopen.
//! get() sees 50 but snapshot.scan() sees 0 → suspicion on scan_at.

use tempfile::TempDir;
use toroidal_store::ToroidalStore;

fn put_node(store: &ToroidalStore, i: u64) {
    let key = format!("node:{:08x}", i).into_bytes();
    store.put(key, b"value".to_vec()).unwrap();
}

#[test]
fn scan_after_flush_reopen() {
    let dir = TempDir::new().unwrap();
    let n = 50usize;

    {
        let store = ToroidalStore::open(dir.path()).unwrap();
        for i in 0..n {
            put_node(&store, i as u64);
        }
        store.checkpoint().unwrap();
    }

    let store = ToroidalStore::open(dir.path()).unwrap();

    // 1. Current get() count.
    let mut cur = 0;
    for i in 0..n {
        let key = format!("node:{:08x}", i).into_bytes();
        if store.get(&key).is_some() {
            cur += 1;
        }
    }
    eprintln!("get() after reopen: {}", cur);

    // 2. Current scan() count.
    let scan = store.scan(None, None);
    eprintln!("scan() after reopen: {}", scan.len());

    // 3. scan_at(LATEST).
    let latest = toroidal_store::Snapshot::latest();
    let scan_at = store.scan_at(&latest, None, None).unwrap();
    eprintln!("scan_at(LATEST) after reopen: {}", scan_at.len());

    // 4. Fresh snapshot (engine snapshot()).
    let snap = store.snapshot();
    let scan_snap = store.scan_at(&snap, None, None).unwrap();
    eprintln!("scan_at(engine snapshot) after reopen: {}", scan_snap.len());

    // 5. get_at(engine snapshot).
    let mut got_at = 0;
    for i in 0..n {
        let key = format!("node:{:08x}", i).into_bytes();
        if store.get_at(&snap, &key).unwrap().is_some() {
            got_at += 1;
        }
    }
    eprintln!("get_at(engine snapshot) after reopen: {}", got_at);

    assert_eq!(
        scan_at.len(),
        n,
        "scan_at(LATEST) lost data after flush+reopen"
    );
    assert_eq!(scan_snap.len(), n, "scan_at(engine snapshot) lost data");
    assert_eq!(got_at, n, "get_at(engine snapshot) lost data");
}
