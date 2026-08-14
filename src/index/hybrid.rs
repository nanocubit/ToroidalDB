use crate::index::{IndexConfig, MetricType, VectorIndex};
use crate::topology::topological_indices::{HomotopyClassIndex, TopologicalFeatureIndex};
use crate::tql::functions::{cosine_similarity, euclidean_distance, manhattan_distance, Vector};
use std::collections::HashMap;

pub struct HybridIndex {
    vector_index: Box<dyn VectorIndex>,
    homotopy_index: Option<HomotopyClassIndex>,
    topological_index: Option<TopologicalFeatureIndex>,
    config: HybridIndexConfig,
    dimension: usize,
}

#[derive(Clone, Debug)]
pub struct HybridIndexConfig {
    pub use_vector_index: bool,
    pub use_topological_filter: bool,
    pub topological_weight: f32,
    pub vector_weight: f32,
    pub rerank_limit: usize,
}

impl Default for HybridIndexConfig {
    fn default() -> Self {
        HybridIndexConfig {
            use_vector_index: true,
            use_topological_filter: true,
            topological_weight: 0.3,
            vector_weight: 0.7,
            rerank_limit: 100,
        }
    }
}

impl HybridIndex {
    pub fn new(
        vector_index: Box<dyn VectorIndex>,
        dimension: usize,
        config: HybridIndexConfig,
    ) -> Self {
        HybridIndex {
            vector_index,
            homotopy_index: None,
            topological_index: None,
            config,
            dimension,
        }
    }

    pub fn with_homotopy_index(mut self, index: HomotopyClassIndex) -> Self {
        self.homotopy_index = Some(index);
        self
    }

    pub fn with_topological_index(mut self, index: TopologicalFeatureIndex) -> Self {
        self.topological_index = Some(index);
        self
    }

    pub fn add(&mut self, id: usize, vector: &Vector) -> Result<(), String> {
        self.vector_index.add(id, vector)
    }

    pub fn search_hybrid(
        &self,
        query: &Vector,
        k: usize,
        filters: &SearchFilters,
    ) -> Result<Vec<HybridSearchResult>, String> {
        if query.dimension != self.dimension {
            return Err("Dimension mismatch".to_string());
        }

        let vector_results = if self.config.use_vector_index {
            self.vector_index.search(query, self.config.rerank_limit)?
        } else {
            Vec::new()
        };

        let mut candidates: HashMap<usize, CandidateScore> = HashMap::new();

        for (id, vector_score) in vector_results {
            let normalized_vector_score = if vector_score >= 0.0 {
                vector_score
            } else {
                1.0 + vector_score.min(0.0)
            };

            candidates.insert(
                id,
                CandidateScore {
                    vector_score: normalized_vector_score,
                    topological_score: 0.0,
                    combined_score: normalized_vector_score * self.config.vector_weight,
                },
            );
        }

        if self.config.use_topological_filter {
            if let Some(ref homotopy_idx) = self.homotopy_index {
                if let Some(ref target_class) = filters.homotopy_class {
                    let matching_ids = homotopy_idx.get_nodes_in_class(target_class);

                    candidates.retain(|id, _| matching_ids.contains(id));
                }
            }

            if let Some(ref topo_idx) = self.topological_index {
                if let Some(ref features) = filters.topological_features {
                    for (id, candidate) in candidates.iter_mut() {
                        if let Some(feature_vec) = topo_idx.get_features(*id) {
                            let similarity =
                                self.compute_feature_similarity(features, &feature_vec);
                            candidate.topological_score = similarity;
                            candidate.combined_score += similarity * self.config.topological_weight;
                        }
                    }
                }
            }
        }

        let mut results: Vec<HybridSearchResult> = candidates
            .into_iter()
            .map(|(id, scores)| HybridSearchResult {
                id,
                vector_score: scores.vector_score,
                topological_score: scores.topological_score,
                combined_score: scores.combined_score,
            })
            .collect();

        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(k);

        Ok(results)
    }

    fn compute_feature_similarity(&self, f1: &[f32], f2: &[f32]) -> f32 {
        if f1.len() != f2.len() || f1.is_empty() {
            return 0.0;
        }

        cosine_similarity(f1, f2)
    }

    pub fn auto_select_index(query: &QueryCharacteristics) -> IndexRecommendation {
        let mut score_vector = 0.0;
        let mut score_topological = 0.0;

        if query.has_vector_filter {
            score_vector += 1.0;
        }
        if query.has_similarity_search {
            score_vector += 2.0;
        }

        if query.has_homotopy_filter {
            score_topological += 2.0;
        }
        if query.has_topology_constraint {
            score_topological += 1.5;
        }
        if query.has_path_query {
            score_topological += 1.0;
        }

        if score_vector > 0.0 && score_topological > 0.0 {
            IndexRecommendation::Hybrid
        } else if score_vector > score_topological {
            IndexRecommendation::Vector
        } else if score_topological > 0.0 {
            IndexRecommendation::Topological
        } else {
            IndexRecommendation::Vector
        }
    }

    pub fn size(&self) -> usize {
        self.vector_index.size()
    }
}

#[derive(Clone, Debug)]
pub struct CandidateScore {
    pub vector_score: f32,
    pub topological_score: f32,
    pub combined_score: f32,
}

#[derive(Clone, Debug)]
pub struct HybridSearchResult {
    pub id: usize,
    pub vector_score: f32,
    pub topological_score: f32,
    pub combined_score: f32,
}

#[derive(Clone, Debug, Default)]
pub struct SearchFilters {
    pub homotopy_class: Option<String>,
    pub topological_features: Option<Vec<f32>>,
    pub vector_threshold: Option<f32>,
    pub metadata_filter: Option<HashMap<String, String>>,
}

#[derive(Clone, Debug)]
pub struct QueryCharacteristics {
    pub has_vector_filter: bool,
    pub has_similarity_search: bool,
    pub has_homotopy_filter: bool,
    pub has_topology_constraint: bool,
    pub has_path_query: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IndexRecommendation {
    Vector,
    Topological,
    Hybrid,
    BruteForce,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{create_index, IndexConfig, IndexType};

    #[test]
    fn test_hybrid_search() {
        let config = IndexConfig::with_hnsw(4);
        let vector_index = create_index(config);

        let hybrid_config = HybridIndexConfig::default();
        let mut hybrid = HybridIndex::new(vector_index, 4, hybrid_config);

        let v1 = Vector::new(vec![1.0, 0.0, 0.0, 0.0]);
        let v2 = Vector::new(vec![0.0, 1.0, 0.0, 0.0]);
        let v3 = Vector::new(vec![0.9, 0.1, 0.0, 0.0]);

        hybrid.add(1, &v1).unwrap();
        hybrid.add(2, &v2).unwrap();
        hybrid.add(3, &v3).unwrap();

        let query = Vector::new(vec![1.0, 0.0, 0.0, 0.0]);
        let filters = SearchFilters::default();

        let results = hybrid.search_hybrid(&query, 3, &filters).unwrap();

        assert!(!results.is_empty());
        assert!(results[0].combined_score > 0.0);
    }

    #[test]
    fn test_auto_select_index() {
        let query = QueryCharacteristics {
            has_vector_filter: true,
            has_similarity_search: true,
            has_homotopy_filter: false,
            has_topology_constraint: false,
            has_path_query: false,
        };

        let rec = HybridIndex::auto_select_index(&query);
        assert_eq!(rec, IndexRecommendation::Vector);

        let query2 = QueryCharacteristics {
            has_vector_filter: false,
            has_similarity_search: false,
            has_homotopy_filter: true,
            has_topology_constraint: true,
            has_path_query: false,
        };

        let rec2 = HybridIndex::auto_select_index(&query2);
        assert_eq!(rec2, IndexRecommendation::Topological);

        let query3 = QueryCharacteristics {
            has_vector_filter: true,
            has_similarity_search: true,
            has_homotopy_filter: true,
            has_topology_constraint: false,
            has_path_query: false,
        };

        let rec3 = HybridIndex::auto_select_index(&query3);
        assert_eq!(rec3, IndexRecommendation::Hybrid);
    }
}
