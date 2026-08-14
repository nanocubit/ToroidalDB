pub mod hnsw;
pub mod hybrid;
pub mod ivf;

pub use hnsw::HnswIndex;
pub use hybrid::{
    HybridIndex, HybridIndexConfig, HybridSearchResult, IndexRecommendation, QueryCharacteristics,
    SearchFilters,
};
pub use ivf::IvfIndex;

use crate::tql::functions::Vector;
use std::collections::HashMap;

pub trait VectorIndex: Send + Sync {
    fn add(&mut self, id: usize, vector: &Vector) -> Result<(), String>;
    fn search(&self, query: &Vector, k: usize) -> Result<Vec<(usize, f32)>, String>;
    fn remove(&mut self, id: usize) -> Result<(), String>;
    fn build(&mut self) -> Result<(), String>;
    fn size(&self) -> usize;
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
    pub ivf_nlist: Option<usize>,
    pub ivf_nprobe: Option<usize>,
    pub metric: MetricType,
}

impl Clone for IndexConfig {
    fn clone(&self) -> Self {
        IndexConfig {
            index_type: self.index_type.clone(),
            dimension: self.dimension,
            hnsw_ef: self.hnsw_ef,
            hnsw_m: self.hnsw_m,
            ivf_nlist: self.ivf_nlist,
            ivf_nprobe: self.ivf_nprobe,
            metric: self.metric.clone(),
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
            ivf_nlist: Some(100),
            ivf_nprobe: Some(10),
            metric: MetricType::Cosine,
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
