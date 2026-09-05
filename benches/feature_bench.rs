use criterion::{black_box, criterion_group, criterion_main, Criterion};
use toroidal_db::index::hnsw::HnswIndex;
use toroidal_db::index::quantization::{QuantizationType, Quantizer};
use toroidal_db::index::{IndexConfig, VectorIndex};
use toroidal_db::tql::bm25::{Bm25Config, Bm25Index};
use toroidal_db::tql::functions::Vector;

fn bench_hnsw_insert(c: &mut Criterion) {
    let config = IndexConfig::with_hnsw(128);
    let mut group = c.benchmark_group("HNSW Insert");

    // Warmup: insert 1000 vectors
    let mut index = HnswIndex::new(config.clone());
    for i in 0..1000 {
        let data: Vec<f32> = (0..128).map(|j| ((i * 128 + j) as f32) / 1000.0).collect();
        index.add(i, &Vector::new(data)).unwrap();
    }

    group.bench_function("insert_1000", |b| {
        b.iter(|| {
            let mut idx = HnswIndex::new(config.clone());
            for i in 0..1000 {
                let data: Vec<f32> = (0..128).map(|j| ((i * 128 + j) as f32) / 1000.0).collect();
                idx.add(black_box(i), &Vector::new(data)).unwrap();
            }
        });
    });

    group.finish();
}

fn bench_hnsw_search(c: &mut Criterion) {
    let config = IndexConfig::with_hnsw(128);
    let mut index = HnswIndex::new(config.clone());

    for i in 0..1000 {
        let data: Vec<f32> = (0..128).map(|j| ((i * 128 + j) as f32) / 1000.0).collect();
        index.add(i, &Vector::new(data)).unwrap();
    }

    let query = Vector::new(vec![0.5; 128]);

    c.bench_function("HNSW search top-10", |b| {
        b.iter(|| {
            index.search(black_box(&query), black_box(10)).unwrap();
        });
    });
}

fn bench_quantization_scalar(c: &mut Criterion) {
    let q = Quantizer::new(QuantizationType::Scalar);
    let vector: Vec<f32> = (0..768).map(|i| (i as f32 - 384.0) / 384.0).collect();
    let qv = q.quantize(&vector);

    c.bench_function("quantization scalar distance asymmetric", |b| {
        b.iter(|| {
            q.distance_asymmetric(black_box(&vector), black_box(&qv));
        });
    });
}

fn bench_quantization_binary(c: &mut Criterion) {
    let q = Quantizer::new(QuantizationType::Binary);
    let vector: Vec<f32> = (0..768).map(|i| (i as f32 - 384.0) / 384.0).collect();
    let qv = q.quantize(&vector);

    c.bench_function("quantization binary distance asymmetric", |b| {
        b.iter(|| {
            q.distance_asymmetric(black_box(&vector), black_box(&qv));
        });
    });
}

fn bench_bm25_search(c: &mut Criterion) {
    let mut index = Bm25Index::new(Bm25Config::default());

    // Add 1000 documents
    for i in 0..1000 {
        let text = format!("Document number {} contains some words about graph databases and vector search technology", i);
        index.add_document(i, &text);
    }

    c.bench_function("BM25 search", |b| {
        b.iter(|| {
            index.search(black_box("graph vector search"), black_box(10));
        });
    });
}

fn bench_filterable_hnsw_add(c: &mut Criterion) {
    let config = IndexConfig::with_hnsw_filtered(64, 8);
    let mut group = c.benchmark_group("Filterable HNSW");

    group.bench_function("add_with_payload_1000", |b| {
        b.iter(|| {
            let mut idx = HnswIndex::new(config.clone());
            for i in 0..1000 {
                let data: Vec<f32> = (0..64).map(|j| ((i * 64 + j) as f32) / 1000.0).collect();
                let mut payload = std::collections::HashMap::new();
                payload.insert("tenant".to_string(), format!("tenant_{}", i % 10));
                payload.insert(
                    "label".to_string(),
                    if i % 2 == 0 {
                        "A".to_string()
                    } else {
                        "B".to_string()
                    },
                );
                idx.add_with_payload(black_box(i), &Vector::new(data), payload)
                    .unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_hnsw_insert,
    bench_hnsw_search,
    bench_quantization_scalar,
    bench_quantization_binary,
    bench_bm25_search,
    bench_filterable_hnsw_add,
);
criterion_main!(benches);
