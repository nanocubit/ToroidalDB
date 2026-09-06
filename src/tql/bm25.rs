//! BM25 full-text search with hybrid (vector + text) fusion.
//!
//! - BM25: Okapi BM25 with configurable k1 and b parameters
//! - TF-кэш: предвычисленный TF-компонент для всех 256 значений fieldnorm
//! - Term dictionary: `HashMap` для O(1) lookup
//! - IDF-кэш: предвычисленный IDF для каждого терма
//! - Hybrid search: Reciprocal Rank Fusion (RRF) для vector + text

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

const K1: f32 = 1.5;
const B: f32 = 0.75;

/// BM25 configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bm25Config {
    pub k1: f32,
    pub b: f32,
    pub rrf_k: f32,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            k1: K1,
            b: B,
            rrf_k: 60.0,
        }
    }
}

/// BM25 full-text index with `HashMap` + IDF-кэш + TF-кэш.
pub struct Bm25Index {
    config: Bm25Config,
    /// Term dictionary: `HashMap` для O(1) lookup.
    terms: RwLock<HashMap<String, Vec<u64>>>,
    /// IDF-кэш: предвычисленный IDF для каждого терма.
    idf_cache: RwLock<HashMap<String, f32>>,
    /// Document lengths: `doc_id` → total terms
    doc_lengths: RwLock<HashMap<u64, usize>>,
    /// Total number of documents
    total_docs: RwLock<usize>,
    /// Average document length
    avg_doc_length: RwLock<f32>,
    /// TF-кэш: предвычисленный TF-компонент для всех 256 значений fieldnorm
    tf_cache: RwLock<[f32; 256]>,
}

impl Bm25Index {
    pub fn new(config: Bm25Config) -> Self {
        let k1 = config.k1;
        let b = config.b;
        Self {
            config,
            terms: RwLock::new(HashMap::with_capacity(1024)),
            idf_cache: RwLock::new(HashMap::with_capacity(1024)),
            doc_lengths: RwLock::new(HashMap::new()),
            total_docs: RwLock::new(0),
            avg_doc_length: RwLock::new(0.0),
            tf_cache: RwLock::new(compute_tf_cache(k1, b, 0.0)),
        }
    }

    /// Index a document.
    pub fn add_document(&mut self, doc_id: u64, text: &str) {
        let tokens = tokenize(text);
        let doc_len = tokens.len();
        let mut terms = self.terms.write().unwrap();
        for token in &tokens {
            terms.entry(token.clone()).or_default().push(doc_id);
        }
        drop(terms);

        let mut doc_lengths = self.doc_lengths.write().unwrap();
        doc_lengths.insert(doc_id, doc_len);
        let mut total = self.total_docs.write().unwrap();
        *total += 1;
        let mut avg = self.avg_doc_length.write().unwrap();
        *avg = ((*avg * (*total - 1) as f32) + doc_len as f32) / *total as f32;

        if *total > 0 {
            let avg_dl = *avg;
            drop(avg);
            *self.tf_cache.write().unwrap() =
                compute_tf_cache(self.config.k1, self.config.b, avg_dl);
        }
    }

    /// Rebuild IDF cache from current term frequencies.
    fn rebuild_idf_cache(&self) {
        let total_docs = *self.total_docs.read().unwrap();
        let terms = self.terms.read().unwrap();
        let mut idf_cache = self.idf_cache.write().unwrap();
        idf_cache.clear();
        for (term, postings) in terms.iter() {
            let df = postings.len() as f32;
            let idf = ((total_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();
            idf_cache.insert(term.clone(), idf);
        }
    }

    /// BM25 search with TF-кэш + IDF-кэш.
    pub fn search(&self, query: &str, k: usize) -> Vec<(u64, f32)> {
        let tokens = tokenize(query);
        let mut scores: HashMap<u64, f32> = HashMap::new();
        let total_docs = *self.total_docs.read().unwrap();
        let doc_lengths = self.doc_lengths.read().unwrap();
        let terms = self.terms.read().unwrap();
        let tf_cache = *self.tf_cache.read().unwrap();

        if total_docs == 0 {
            return Vec::new();
        }

        for token in &tokens {
            if let Some(postings) = terms.get(token) {
                // IDF из кэша или вычисляем
                let idf = {
                    let idf_cache = self.idf_cache.read().unwrap();
                    idf_cache.get(token).copied().unwrap_or_else(|| {
                        let df = postings.len() as f32;
                        ((total_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln()
                    })
                };

                for &doc_id in postings {
                    if let Some(&doc_len) = doc_lengths.get(&doc_id) {
                        let tf = 1.0;
                        let fieldnorm_id = (doc_len.min(255) as u8) as usize;
                        let cached_norm = tf_cache[fieldnorm_id];
                        let score = idf * (self.config.k1 + 1.0) * tf / (tf + cached_norm);
                        *scores.entry(doc_id).or_insert(0.0) += score;
                    }
                }
            }
        }

        let mut results: Vec<(u64, f32)> = scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }

    /// Number of unique terms in dictionary.
    pub fn num_terms(&self) -> usize {
        self.terms.read().unwrap().len()
    }
}

/// Предвычисляет TF-компонент для всех 256 значений fieldnorm.
fn compute_tf_cache(k1: f32, b: f32, avg_fieldnorm: f32) -> [f32; 256] {
    let mut cache = [0.0f32; 256];
    let avg = if avg_fieldnorm > 1e-6 {
        avg_fieldnorm
    } else {
        100.0
    };
    for i in 0..256 {
        cache[i] = k1 * (1.0 - b + b * (i as f32) / avg);
    }
    cache
}

/// Hybrid search: combine BM25 + vector scores via RRF.
pub fn hybrid_search(
    text_results: Vec<(u64, f32)>,
    vector_results: Vec<(u64, f32)>,
    k: usize,
    rrf_k: f32,
) -> Vec<(u64, f32)> {
    let mut rrf_scores: HashMap<u64, f32> = HashMap::new();
    for (rank, (id, _)) in text_results.iter().enumerate() {
        *rrf_scores.entry(*id).or_insert(0.0) += 1.0 / (rrf_k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in vector_results.iter().enumerate() {
        *rrf_scores.entry(*id).or_insert(0.0) += 1.0 / (rrf_k + rank as f32 + 1.0);
    }
    let mut results: Vec<(u64, f32)> = rrf_scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(k);
    results
}

/// Simple Unicode-aware tokenizer.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(std::string::ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_single_doc() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_document(1, "Hello world this is a test document");
        let results = index.search("test", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_bm25_multi_doc() {
        let mut index = Bm25Index::new(Bm25Config::default());
        index.add_document(1, "Graph databases are great");
        index.add_document(2, "Vector search is also great");
        index.add_document(3, "Cooking recipes are delicious");
        let results = index.search("great", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_hybrid_rrf() {
        let text = vec![(1, 0.9), (2, 0.8), (3, 0.7)];
        let vector = vec![(2, 0.95), (1, 0.85), (4, 0.8)];
        let results = hybrid_search(text, vector, 3, 60.0);
        assert_eq!(results.len(), 3);
        assert!(results[0].0 == 1 || results[0].0 == 2);
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello, World! This is a test.");
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"test".to_string()));
    }
}
