use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tempfile::TempDir;
use toroidal_db::hybrid_storage::{HybridPersistentStore, Node};
use toroidal_db::math::MatryoshkaDim;

fn setup_test_store() -> (HybridPersistentStore, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = HybridPersistentStore::open(&temp_dir).unwrap();
    (store, temp_dir)
}

fn bench_matryoshka_search_d384(c: &mut Criterion) {
    let (store, _temp_dir) = setup_test_store();

    // Insert test nodes
    for i in 0..1000 {
        let node = Node {
            id: i,
            vector: vec![0.5f32; 384],
            properties: serde_json::json!({"test": i}),
            edges: Vec::new(),
        };
        store.insert(node).unwrap();
    }

    let query = vec![0.5f32; 384];

    c.bench_function("matryoshka_search_d384_1000_nodes", |b| {
        b.iter(|| {
            store.matryoshka_search(
                black_box(&query),
                black_box(MatryoshkaDim::D384),
                black_box(0.3),
            )
        })
    });
}

fn bench_matryoshka_search_d768(c: &mut Criterion) {
    let (store, _temp_dir) = setup_test_store();

    // Insert test nodes
    for i in 0..1000 {
        let node = Node {
            id: i,
            vector: vec![0.5f32; 768],
            properties: serde_json::json!({"test": i}),
            edges: Vec::new(),
        };
        store.insert(node).unwrap();
    }

    let query = vec![0.5f32; 768];

    c.bench_function("matryoshka_search_d768_1000_nodes", |b| {
        b.iter(|| {
            store.matryoshka_search(
                black_box(&query),
                black_box(MatryoshkaDim::D768),
                black_box(0.3),
            )
        })
    });
}

fn bench_node_insertion(c: &mut Criterion) {
    let (store, _temp_dir) = setup_test_store();

    c.bench_function("node_insertion", |b| {
        b.iter(|| {
            let node = Node {
                id: black_box(1000),
                vector: black_box(vec![0.5f32; 384]),
                properties: black_box(serde_json::json!({"test": "value"})),
                edges: Vec::new(),
            };
            store.insert(node)
        })
    });
}

criterion_group!(
    benches,
    bench_matryoshka_search_d384,
    bench_matryoshka_search_d768,
    bench_node_insertion,
);
criterion_main!(benches);
