pub mod hnsw;
pub mod hybrid;
pub mod ivf;
pub mod quantization;

pub use hnsw::HnswIndex;
pub use hybrid::{
    HybridIndex, HybridIndexConfig, HybridSearchResult, IndexRecommendation, QueryCharacteristics,
    SearchFilters,
};
pub use ivf::IvfIndex;
pub use quantization::{QuantizationType, QuantizedVector, Quantizer};

use crate::tql::functions::Vector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub trait VectorIndex: Send + Sync {
    fn add(&mut self, id: usize, vector: &Vector) -> Result<(), String>;
    fn add_with_payload(
        &mut self,
        id: usize,
        vector: &Vector,
        payload: HashMap<String, String>,
    ) -> Result<(), String> {
        self.add(id, vector)
    }
    fn search(&self, query: &Vector, k: usize) -> Result<Vec<(usize, f32)>, String>;
    fn search_with_filter(
        &self,
        query: &Vector,
        k: usize,
        filter: &Filter,
    ) -> Result<Vec<(usize, f32)>, String> {
        self.search(query, k)
    }
    fn remove(&mut self, id: usize) -> Result<(), String>;
    fn build(&mut self) -> Result<(), String>;
    fn size(&self) -> usize;
}

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub must: Vec<Condition>,
    pub should: Vec<Condition>,
    pub must_not: Vec<Condition>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.must.is_empty() && self.should.is_empty() && self.must_not.is_empty()
    }
    pub fn matches(&self, payload: &HashMap<String, String>) -> bool {
        for cond in &self.must {
            if !cond.matches(payload) {
                return false;
            }
        }
        for cond in &self.must_not {
            if cond.matches(payload) {
                return false;
            }
        }
        if !self.should.is_empty() {
            let any_match = self.should.iter().any(|c| c.matches(payload));
            if !any_match {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub enum Condition {
    Match {
        key: String,
        value: String,
    },
    MatchAny {
        key: String,
        values: Vec<String>,
    },
    Range {
        key: String,
        min: Option<f64>,
        max: Option<f64>,
    },
}

impl Condition {
    pub fn matches(&self, payload: &HashMap<String, String>) -> bool {
        match self {
            Condition::Match { key, value } => {
                payload.get(key).map(|v| v == value).unwrap_or(false)
            }
            Condition::MatchAny { key, values } => payload
                .get(key)
                .map(|v| values.contains(v))
                .unwrap_or(false),
            Condition::Range { key, min, max } => {
                let val: f64 = match payload.get(key).and_then(|v| v.parse().ok()) {
                    Some(v) => v,
                    None => return false,
                };
                let above_min = min.map(|m| val >= m).unwrap_or(true);
                let below_max = max.map(|m| val <= m).unwrap_or(true);
                above_min && below_max
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryTier {
    Pinned,
    Cached,
    Cold,
}
impl Default for MemoryTier {
    fn default() -> Self {
        MemoryTier::Cached
    }
}

#[derive(Debug, Clone)]
pub enum IndexType {
    Hnsw,
    Ivf,
    BruteForce,
}
impl Default for IndexType {
    fn default() -> Self {
        IndexType::Hnsw
    }
}

pub struct IndexConfig {
    pub index_type: IndexType,
    pub dimension: usize,
    pub hnsw_ef: Option<usize>,
    pub hnsw_m: Option<usize>,
    pub hnsw_payload_m: Option<usize>,
    pub ivf_nlist: Option<usize>,
    pub ivf_nprobe: Option<usize>,
    pub metric: MetricType,
    pub memory_tier: MemoryTier,
}

impl Clone for IndexConfig {
    fn clone(&self) -> Self {
        IndexConfig {
            index_type: self.index_type.clone(),
            dimension: self.dimension,
            hnsw_ef: self.hnsw_ef,
            hnsw_m: self.hnsw_m,
            hnsw_payload_m: self.hnsw_payload_m,
            ivf_nlist: self.ivf_nlist,
            ivf_nprobe: self.ivf_nprobe,
            metric: self.metric.clone(),
            memory_tier: self.memory_tier,
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        IndexConfig {
            index_type: IndexType::Hnsw,
            dimension: 128,
            hnsw_ef: Some(16),
            hnsw_m: Some(16),
            hnsw_payload_m: None,
            ivf_nlist: Some(100),
            ivf_nprobe: Some(10),
            metric: MetricType::Cosine,
            memory_tier: MemoryTier::Cached,
        }
    }
}

pub enum MetricType {
    Cosine,
    Euclidean,
    Manhattan,
}
impl Clone for MetricType {
    fn clone(&self) -> Self {
        match self {
            MetricType::Cosine => MetricType::Cosine,
            MetricType::Euclidean => MetricType::Euclidean,
            MetricType::Manhattan => MetricType::Manhattan,
        }
    }
}

impl IndexConfig {
    pub fn with_hnsw(dimension: usize) -> Self {
        IndexConfig {
            dimension,
            ..Default::default()
        }
    }
    pub fn with_hnsw_filtered(dimension: usize, payload_m: usize) -> Self {
        IndexConfig {
            index_type: IndexType::Hnsw,
            dimension,
            hnsw_ef: Some(16),
            hnsw_m: Some(16),
            hnsw_payload_m: Some(payload_m),
            ivf_nlist: None,
            ivf_nprobe: None,
            metric: MetricType::Cosine,
            memory_tier: MemoryTier::Cached,
        }
    }
    pub fn with_ivf(dimension: usize, nlist: usize) -> Self {
        IndexConfig {
            index_type: IndexType::Ivf,
            dimension,
            ivf_nlist: Some(nlist),
            ..Default::default()
        }
    }
}

pub fn create_index(config: IndexConfig) -> Box<dyn VectorIndex> {
    match config.index_type {
        IndexType::Hnsw => Box::new(HnswIndex::new(config)),
        IndexType::Ivf => Box::new(IvfIndex::new(config)),
        IndexType::BruteForce => Box::new(BruteForceIndex::new(config)),
    }
}

pub struct BruteForceIndex {
    vectors: HashMap<usize, Vec<f32>>,
    dimension: usize,
    metric: MetricType,
}
impl BruteForceIndex {
    pub fn new(config: IndexConfig) -> Self {
        BruteForceIndex {
            vectors: HashMap::new(),
            dimension: config.dimension,
            metric: config.metric,
        }
    }
}
impl VectorIndex for BruteForceIndex {
    fn add(&mut self, id: usize, vector: &Vector) -> Result<(), String> {
        if vector.dimension != self.dimension {
            return Err("Dimension mismatch".to_string());
        }
        self.vectors.insert(id, vector.data.clone());
        Ok(())
    }
    fn search(&self, query: &Vector, k: usize) -> Result<Vec<(usize, f32)>, String> {
        let mut results: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| {
                let score = match self.metric {
                    MetricType::Cosine => {
                        crate::tql::functions::cosine_similarity(&query.data, vec)
                    }
                    MetricType::Euclidean => {
                        -crate::tql::functions::euclidean_distance(&query.data, vec)
                    }
                    MetricType::Manhattan => {
                        -crate::tql::functions::manhattan_distance(&query.data, vec)
                    }
                };
                (*id, score)
            })
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        Ok(results)
    }
    fn remove(&mut self, id: usize) -> Result<(), String> {
        self.vectors.remove(&id);
        Ok(())
    }
    fn build(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn size(&self) -> usize {
        self.vectors.len()
    }
}
