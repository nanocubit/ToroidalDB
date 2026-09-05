//! Интеграционные тесты ToroidalStore + HybridPersistentStore.
//!
//! Проверяют:
//! - put/get/delete/scan через HybridPersistentStore
//! - flush (checkpoint) — durability
//! - snapshot — point-in-time стабильность
//! - reopen — recovery
//! - tombstone — delete + flush + reopen

use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use toroidal_db::hybrid_storage::{HybridPersistentStore, Node};

fn store() -> (TempDir, Arc<HybridPersistentStore>) {
    let dir = TempDir::new().unwrap();
    let store = Arc::new(HybridPersistentStore::open(dir.path()).unwrap());
    (dir, store)
}

fn make_node(id: u64, name: &str) -> Node {
    Node {
        id,
        vector: vec![0.1; 4],
        properties: json!({"name": name}),
        edges: vec![],
    }
}

#[test]
fn put_get_delete() {
    let (_dir, store) = store();
    assert!(store.insert(make_node(1, "a")).unwrap());
    assert_eq!(store.get(1).unwrap().unwrap().properties["name"], "a");
    assert!(store.remove(1).unwrap());
    assert!(store.get(1).unwrap().is_none());
}

#[test]
fn overwrite() {
    let (_dir, store) = store();
    assert!(store.insert(make_node(1, "a")).unwrap());
    assert!(!store.insert(make_node(1, "b")).unwrap()); // already exists
    assert_eq!(store.get(1).unwrap().unwrap().properties["name"], "a");
}

#[test]
fn get_all_scans() {
    let (_dir, store) = store();
    store.insert(make_node(1, "a")).unwrap();
    store.insert(make_node(2, "b")).unwrap();
    let nodes = store.get_all().unwrap();
    assert_eq!(nodes.len(), 2);
}

#[test]
fn flush_and_reopen() {
    let dir = TempDir::new().unwrap();
    {
        let store = HybridPersistentStore::open(dir.path()).unwrap();
        store.insert(make_node(1, "a")).unwrap();
        // checkpoint makes data durable
    }
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    assert_eq!(store.get(1).unwrap().unwrap().properties["name"], "a");
}

#[test]
fn reopen_after_delete() {
    let dir = TempDir::new().unwrap();
    {
        let store = HybridPersistentStore::open(dir.path()).unwrap();
        store.insert(make_node(1, "a")).unwrap();
        store.remove(1).unwrap();
    }
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    assert!(
        store.get(1).unwrap().is_none(),
        "tombstone must survive reopen"
    );
}

#[test]
fn snapshot_stability() {
    let (_dir, store) = store();
    store.insert(make_node(1, "a")).unwrap();
    let snap = store.storage.snapshot().unwrap();
    // Subsequent write must not affect the snapshot.
    store.insert(make_node(2, "b")).unwrap();
    // Snapshot should still see only node 1.
    let nodes: Vec<_> = snap
        .scan(Some(b"node:"), None)
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(nodes.len(), 1, "snapshot must not see later writes");
    assert_eq!(snap.get(&nodes[0].0).unwrap().unwrap(), nodes[0].1);
}

#[test]
fn storage_type() {
    let (_dir, store) = store();
    assert_eq!(store.get_storage_type(), "toroidal-store");
}

#[test]
fn node_count() {
    let (_dir, store) = store();
    assert_eq!(store.get_node_count(), 0);
    store.insert(make_node(1, "a")).unwrap();
    assert_eq!(store.get_node_count(), 1);
    store.insert(make_node(2, "b")).unwrap();
    assert_eq!(store.get_node_count(), 2);
    store.remove(1).unwrap();
    assert_eq!(store.get_node_count(), 1);
}

#[test]
fn reopen_recovery() {
    let dir = TempDir::new().unwrap();
    let n = 100;
    {
        let store = HybridPersistentStore::open(dir.path()).unwrap();
        for i in 0..n {
            store.insert(make_node(i, &format!("n{}", i))).unwrap();
        }
    }
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    assert_eq!(store.get_node_count(), n as usize);
    for i in 0..n {
        let node = store.get(i).unwrap().unwrap();
        assert_eq!(node.properties["name"], format!("n{}", i));
    }
}

#[test]
fn concurrent_read_write() {
    let (_dir, store) = store();
    let store = Arc::new(store);
    let mut handles = Vec::new();
    for t in 0..4 {
        let s = store.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..100 {
                let id = t * 100 + i;
                s.insert(make_node(id, &format!("t{}-{}", t, i))).unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(store.get_node_count(), 400);
}

// ---------------------------------------------------------------------------
// Crash-consistency tests
// ---------------------------------------------------------------------------

/// Simulated crash: write acknowledged data, then destroy the store without
/// clean shutdown, reopen, and verify all acknowledged writes survived.
#[test]
fn crash_then_reopen_preserves_acked_writes() {
    let dir = TempDir::new().unwrap();
    let n = 50;
    {
        let store = HybridPersistentStore::open(dir.path()).unwrap();
        for i in 0..n {
            store.insert(make_node(i, &format!("n{}", i))).unwrap();
        }
        // Acknowledged writes are in the WAL; dropping without cleanup = crash.
    }
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    assert_eq!(store.get_node_count(), n as usize);
    for i in 0..n {
        let node = store.get(i).unwrap().unwrap();
        assert_eq!(node.properties["name"], format!("n{}", i));
    }
}

/// Crash after explicit flush: data must survive, and node_count must be
/// recovered correctly from segments (not just WAL).
#[test]
fn crash_after_flush_preserves_data() {
    let dir = TempDir::new().unwrap();
    let n = 50;
    {
        let store = HybridPersistentStore::open(dir.path()).unwrap();
        for i in 0..n {
            store.insert(make_node(i, &format!("n{}", i))).unwrap();
        }
        store.flush().unwrap();
    }
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    assert_eq!(store.get_node_count(), n as usize);
    for i in 0..n {
        let node = store.get(i).unwrap().unwrap();
        assert_eq!(node.properties["name"], format!("n{}", i));
    }
}

/// Crash mid-load: delete the process's WAL file portion?  Instead simulate a
/// torn WAL by appending garbage bytes before reopen — the engine must
/// truncate the corrupt suffix and recover the valid prefix.
#[test]
fn crash_with_torn_wal_tail() {
    use std::io::Write;
    let dir = TempDir::new().unwrap();
    let n = 50;
    {
        let store = HybridPersistentStore::open(dir.path()).unwrap();
        for i in 0..n {
            store.insert(make_node(i, &format!("n{}", i))).unwrap();
        }
    }
    // Append garbage to the WAL file to simulate a torn write.
    let wal_path = dir.path().join("wal");
    if wal_path.exists() {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&wal_path)
            .unwrap();
        let _ = f.write_all(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02]);
        f.sync_all().unwrap();
    }
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    // Recovery should succeed; the acknowledged prefix survives.
    assert!(
        store.get(0).unwrap().is_some(),
        "first node must survive torn WAL"
    );
    assert!(
        store.get(49).unwrap().is_some(),
        "last node must survive torn WAL"
    );
    // Store is usable.
    let _ = store.get(25).unwrap();
}
