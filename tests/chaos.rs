//! Chaos/resilience tests: concurrent mixed workloads against a real store.

use rand::Rng;
use serde_json::json;
use std::sync::Arc;
use toroidal_db::hybrid_storage::{HybridPersistentStore, Node};

#[tokio::test]
async fn concurrent_inserts_and_reads_stay_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(HybridPersistentStore::open(dir.path()).unwrap());
    let store_w = store.clone();

    let writer = tokio::task::spawn_blocking(move || {
        let mut rng = rand::thread_rng();
        for i in 1..=200u64 {
            let dim = 4;
            let vector: Vec<f32> = (0..dim).map(|_| rng.gen::<f32>()).collect();
            store_w
                .insert(Node {
                    id: i,
                    vector,
                    properties: json!({"i": i}),
                    edges: vec![],
                })
                .unwrap();
        }
    });

    let reader = {
        let store = store.clone();
        tokio::task::spawn_blocking(move || {
            for _ in 0..200 {
                // Count must never exceed what has been inserted.
                assert!(store.get_node_count() <= 200);
            }
        })
    };

    writer.await.unwrap();
    reader.await.unwrap();
    assert_eq!(store.get_node_count(), 200);
}

#[tokio::test]
async fn duplicate_inserts_do_not_create_ghost_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    let node = Node {
        id: 42,
        vector: vec![0.5; 8],
        properties: json!({"stable": true}),
        edges: vec![],
    };
    store.insert(node.clone()).unwrap();
    store.insert(node).unwrap();
    assert_eq!(store.get_node_count(), 1);
}

#[test]
fn edge_to_missing_node_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let store = HybridPersistentStore::open(dir.path()).unwrap();
    assert!(store
        .add_edge(999, 1000, "SIMILAR".to_string(), 1.0)
        .is_err());
}
