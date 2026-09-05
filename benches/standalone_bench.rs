// Standalone HNSW + quantization + BM25 benchmarks — без зависимостей от librocksdb
use std::collections::HashMap;
use std::time::Instant;

mod dummy {
    use std::collections::HashMap;

    pub struct Vector {
        pub data: Vec<f32>,
        pub dimension: usize,
    }
    impl Vector {
        pub fn new(data: Vec<f32>) -> Self {
            let d = data.len();
            Self { data, dimension: d }
        }
    }

    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum();
        let nb: f32 = b.iter().map(|x| x * x).sum();
        dot / (na.sqrt() * nb.sqrt() + 1e-10)
    }

    pub struct HnswNode {
        pub id: usize,
        pub vector: Vec<f32>,
        pub edges: Vec<(usize, f32)>,
        pub payload: HashMap<String, String>,
        pub payload_edges: Vec<(usize, f32)>,
    }

    pub struct HnswIndex {
        nodes: Vec<HnswNode>,
        dim: usize,
    }

    impl HnswIndex {
        pub fn new(dim: usize) -> Self {
            Self {
                nodes: Vec::new(),
                dim,
            }
        }
        pub fn add(&mut self, id: usize, vector: Vec<f32>, payload: HashMap<String, String>) {
            let payload_m = 8;
            let mut node = HnswNode {
                id,
                vector,
                edges: Vec::new(),
                payload,
                payload_edges: Vec::new(),
            };
            // Build distance-based edges
            for other in &self.nodes {
                let dist = 1.0 - cosine_similarity(&node.vector, &other.vector);
                node.edges.push((other.id, dist));
            }
            // Build payload edges
            if payload_m > 0 {
                for other in &self.nodes {
                    for key in node.payload.keys() {
                        if node.payload.get(key) == other.payload.get(key) {
                            let dist = 1.0 - cosine_similarity(&node.vector, &other.vector);
                            node.payload_edges.push((other.id, dist));
                            break;
                        }
                    }
                }
            }
            node.edges.truncate(16);
            node.payload_edges.truncate(payload_m);
            self.nodes.push(node);
        }
        pub fn search(&self, query: &[f32], k: usize) -> Vec<(usize, f32)> {
            let mut results: Vec<(usize, f32)> = self
                .nodes
                .iter()
                .map(|n| {
                    let dist = 1.0 - cosine_similarity(query, &n.vector);
                    (n.id, dist)
                })
                .collect();
            results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            results.truncate(k);
            results
        }
        pub fn size(&self) -> usize {
            self.nodes.len()
        }
    }

    pub enum QuantType {
        Scalar,
        Binary,
    }
    pub enum QuantizedVector {
        Scalar { data: Vec<u8>, min: f32, max: f32 },
        Binary { data: Vec<u64>, dim: usize },
    }
    pub struct Quantizer;
    impl Quantizer {
        pub fn quantize(v: &[f32], qt: QuantType) -> QuantizedVector {
            match qt {
                QuantType::Scalar => {
                    let min = v.iter().cloned().fold(f32::MAX, f32::min);
                    let max = v.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let range = if max - min > 1e-6 { max - min } else { 1.0 };
                    let data = v
                        .iter()
                        .map(|&x| ((x - min) / range * 255.0) as u8)
                        .collect();
                    QuantizedVector::Scalar { data, min, max }
                }
                QuantType::Binary => {
                    let dim = v.len();
                    let words = (dim + 63) / 64;
                    let mut data = vec![0u64; words];
                    for (i, &x) in v.iter().enumerate() {
                        if x > 0.0 {
                            data[i / 64] |= 1 << (i % 64);
                        }
                    }
                    QuantizedVector::Binary { data, dim }
                }
            }
        }
        pub fn distance_asymmetric(query: &[f32], qv: &QuantizedVector) -> f32 {
            match qv {
                QuantizedVector::Scalar { data, min, max } => {
                    let range = max - min;
                    let scale = range / 255.0;
                    let mut dot = 0.0f32;
                    for j in 0..query.len().min(data.len()) {
                        dot += query[j] * (data[j] as f32 * scale + min);
                    }
                    let nq: f32 = query.iter().map(|x| x * x).sum();
                    1.0 - (dot / ((nq * max * max).sqrt() + 1e-10))
                }
                QuantizedVector::Binary { data, .. } => {
                    let mut h = 0u32;
                    for (i, &w) in data.iter().enumerate() {
                        for b in 0..64 {
                            let idx = i * 64 + b;
                            if idx >= query.len() {
                                break;
                            }
                            let qb = if query[idx] > 0.0 { 1 } else { 0 };
                            let vb = (w >> b) & 1;
                            h += qb ^ vb as u32;
                        }
                    }
                    h as f32
                }
            }
        }
    }

    pub struct Bm25Index {
        terms: Vec<(String, Vec<u64>)>,
        doc_lengths: HashMap<u64, usize>,
        total_docs: usize,
        avg_dl: f32,
    }
    impl Bm25Index {
        pub fn new() -> Self {
            Self {
                terms: Vec::new(),
                doc_lengths: HashMap::new(),
                total_docs: 0,
                avg_dl: 0.0,
            }
        }
        pub fn add_document(&mut self, doc_id: u64, text: &str) {
            let tokens: Vec<String> = text
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty() && s.len() > 1)
                .map(|s| s.to_string())
                .collect();
            let doc_len = tokens.len();
            for token in &tokens {
                let pos = self.terms.binary_search_by(|e| e.0.as_str().cmp(token));
                match pos {
                    Ok(i) => self.terms[i].1.push(doc_id),
                    Err(i) => self.terms.insert(i, (token.clone(), vec![doc_id])),
                }
            }
            self.doc_lengths.insert(doc_id, doc_len);
            self.total_docs += 1;
            self.avg_dl = ((self.avg_dl * (self.total_docs - 1) as f32) + doc_len as f32)
                / self.total_docs as f32;
        }
        pub fn search(&self, query: &str, k: usize) -> Vec<(u64, f32)> {
            let tokens: Vec<String> = query
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            let mut scores: HashMap<u64, f32> = HashMap::new();
            if self.total_docs == 0 {
                return vec![];
            }
            let k1 = 1.5;
            let b = 0.75;
            for token in &tokens {
                if let Ok(i) = self.terms.binary_search_by(|e| e.0.as_str().cmp(token)) {
                    let postings = &self.terms[i].1;
                    let df = postings.len() as f32;
                    let idf = ((self.total_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();
                    for &doc_id in postings {
                        let doc_len = self.doc_lengths.get(&doc_id).copied().unwrap_or(100) as f32;
                        let tf = 1.0;
                        let norm = k1 * (1.0 - b + b * doc_len / self.avg_dl);
                        let score = idf * (k1 + 1.0) * tf / (tf + norm);
                        *scores.entry(doc_id).or_insert(0.0) += score;
                    }
                }
            }
            let mut results: Vec<(u64, f32)> = scores.into_iter().collect();
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            results.truncate(k);
            results
        }
    }
}

use dummy::*;

fn bench_hnsw() {
    let dim = 128;
    let mut index = HnswIndex::new(dim);
    let n = 5000;
    let start = Instant::now();
    for i in 0..n {
        let data: Vec<f32> = (0..dim)
            .map(|j| ((i * dim + j) as f32) / (n * dim) as f32)
            .collect();
        let mut payload = HashMap::new();
        payload.insert("tenant".to_string(), format!("tenant_{}", i % 10));
        index.add(i, data, payload);
    }
    let insert_time = start.elapsed();
    println!(
        "HNSW insert {} nodes (D={}): {:?} ({:.0} nodes/sec)",
        n,
        dim,
        insert_time,
        n as f64 / insert_time.as_secs_f64()
    );

    let query = vec![0.5; dim];
    let start = Instant::now();
    let mut total = 0;
    for _ in 0..100 {
        let r = index.search(&query, 10);
        total += r.len();
    }
    let search_time = start.elapsed();
    println!(
        "HNSW search 100x (top-10): {:?} ({:.0} QPS)",
        search_time,
        100.0 / search_time.as_secs_f64()
    );
    println!("  results: {}", total);
}

fn bench_quantization() {
    let dim = 768;
    let v: Vec<f32> = (0..dim)
        .map(|i| (i as f32 - dim as f32 / 2.0) / (dim as f32 / 2.0))
        .collect();

    let start = Instant::now();
    for _ in 0..10000 {
        let qv = Quantizer::quantize(&v, QuantType::Scalar);
        let _ = Quantizer::distance_asymmetric(&v, &qv);
    }
    let t = start.elapsed();
    println!(
        "Scalar quantization asymmetric (D={}, 10000x): {:?} ({:.0} ops/sec)",
        dim,
        t,
        10000.0 / t.as_secs_f64()
    );

    let start = Instant::now();
    for _ in 0..10000 {
        let qv = Quantizer::quantize(&v, QuantType::Binary);
        let _ = Quantizer::distance_asymmetric(&v, &qv);
    }
    let t = start.elapsed();
    println!(
        "Binary quantization asymmetric (D={}, 10000x): {:?} ({:.0} ops/sec)",
        dim,
        t,
        10000.0 / t.as_secs_f64()
    );
}

fn bench_bm25() {
    let mut index = Bm25Index::new();
    let n = 5000;
    let start = Instant::now();
    for i in 0..n {
        index.add_document(i, &format!("Document number {} contains some words about graph databases vector search technology and quantum computing", i));
    }
    let add_time = start.elapsed();
    println!(
        "BM25 add {} docs: {:?} ({:.0} docs/sec)",
        n,
        add_time,
        n as f64 / add_time.as_secs_f64()
    );

    let start = Instant::now();
    let mut total = 0;
    for _ in 0..100 {
        let r = index.search("graph vector search quantum", 10);
        total += r.len();
    }
    let search_time = start.elapsed();
    println!(
        "BM25 search 100x: {:?} ({:.0} QPS)",
        search_time,
        100.0 / search_time.as_secs_f64()
    );
    println!("  results: {}", total);
}

fn main() {
    use std::collections::HashMap;

    println!("=== ToroidalDB Benchmarks ===\n");

    println!("--- HNSW ---");
    bench_hnsw();
    println!();

    println!("--- Quantization ---");
    bench_quantization();
    println!();

    println!("--- BM25 ---");
    bench_bm25();
    println!();
}
