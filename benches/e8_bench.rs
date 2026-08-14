use criterion::{black_box, criterion_group, criterion_main, Criterion};
use toroidal_db::math::{toroidal_distance, MatryoshkaDim};

fn bench_toroidal_distance(c: &mut Criterion) {
    let vec1 = vec![0.5f32; 384];
    let vec2 = vec![0.6f32; 384];
    
    c.bench_function("toroidal_distance_d384", |b| {
        b.iter(|| toroidal_distance(black_box(&vec1), black_box(&vec2)))
    });
}

fn bench_toroidal_distance_d768(c: &mut Criterion) {
    let vec1 = vec![0.5f32; 768];
    let vec2 = vec![0.6f32; 768];
    
    c.bench_function("toroidal_distance_d768", |b| {
        b.iter(|| toroidal_distance(black_box(&vec1), black_box(&vec2)))
    });
}

fn bench_toroidal_distance_d1536(c: &mut Criterion) {
    let vec1 = vec![0.5f32; 1536];
    let vec2 = vec![0.6f32; 1536];
    
    c.bench_function("toroidal_distance_d1536", |b| {
        b.iter(|| toroidal_distance(black_box(&vec1), black_box(&vec2)))
    });
}

criterion_group!(
    benches,
    bench_toroidal_distance,
    bench_toroidal_distance_d768,
    bench_toroidal_distance_d1536,
);
criterion_main!(benches);
