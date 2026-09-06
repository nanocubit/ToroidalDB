use crate::index::{IndexConfig, MetricType, VectorIndex};
use crate::tql::functions::{cosine_similarity, euclidean_distance, manhattan_distance, Vector};
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::collections::{BinaryHeap, HashMap};
use std::sync::RwLock;

type SearchResult = Result<Vec<(usize, f32)>, String>;

const HNSW_MAX_M: usize = 32;
const HNSW_DEFAULT_EF: usize = 16;
const HNSW_DEFAULT_M: usize = 16;

#[derive(Clone)]
struct HnswNode {
    id: usize,
    vector: Vec<f32>,
    edges: Vec<(usize, f32)>,
}

pub struct HnswIndex {
    config: IndexConfig,
    nodes: RwLock<HashMap<usize, HnswNode>>,
    entry_point: RwLock<Option<usize>>,
    level_generator: RwLock<StdRng>,
    max_level: RwLock<usize>,
}

impl HnswIndex {
    pub fn new(config: IndexConfig) -> Self {
        HnswIndex {
            config: config.clone(),
            nodes: RwLock::new(HashMap::new()),
            entry_point: RwLock::new(None),
            level_generator: RwLock::new(StdRng::from_entropy()),
            max_level: RwLock::new(0),
        }
    }

    fn get_ef(&self) -> usize {
        self.config.hnsw_ef.unwrap_or(HNSW_DEFAULT_EF)
    }

    fn get_m(&self) -> usize {
        self.config.hnsw_m.unwrap_or(HNSW_DEFAULT_M).min(HNSW_MAX_M)
    }

    fn get_random_level(&self) -> usize {
        let mut rng = self.level_generator.write().unwrap();
        let m = self.get_m() as f64;
        let level = (-rng.gen::<f64>().ln()) as usize;
        level.min((m.ln() as usize) + 1)
    }

    fn distance(&self, v1: &[f32], v2: &[f32]) -> f32 {
        match self.config.metric {
            MetricType::Cosine => 1.0 - cosine_similarity(v1, v2),
            MetricType::Euclidean => euclidean_distance(v1, v2),
            MetricType::Manhattan => manhattan_distance(v1, v2),
        }
    }

    fn search_layer(&self, query: &[f32], ef: usize, entry_point: usize) -> Vec<(usize, f32)> {
        let nodes = self.nodes.read().unwrap();

        let mut visited = HashMap::new();
        let mut current_best: Option<(usize, f32)> = None;
        let mut candidates = BinaryHeap::new();

        if let Some(ep) = nodes.get(&entry_point) {
            let dist = self.distance(query, &ep.vector);
            current_best = Some((entry_point, dist));
            candidates.push(NodeDist {
                id: entry_point,
                dist,
            });
        }

        while let Some(candidate) = candidates.pop() {
            if let Some(best) = current_best {
                if candidate.dist > best.1 {
                    break;
                }
            }

            if visited.contains_key(&candidate.id) {
                continue;
            }
            visited.insert(candidate.id, true);

            if let Some(node) = nodes.get(&candidate.id) {
                for (neighbor_id, _dist) in &node.edges {
                    if *neighbor_id == candidate.id {
                        continue;
                    }

                    if let Some(neighbor) = nodes.get(neighbor_id) {
                        let dist = self.distance(query, &neighbor.vector);

                        if let Some(best) = current_best {
                            if dist < best.1 || candidates.len() < ef {
                                candidates.push(NodeDist {
                                    id: *neighbor_id,
                                    dist,
                                });

                                if dist < best.1 {
                                    current_best = Some((*neighbor_id, dist));
                                }
                            }
                        } else {
                            current_best = Some((*neighbor_id, dist));
                            candidates.push(NodeDist {
                                id: *neighbor_id,
                                dist,
                            });
                        }
                    }
                }
            }
        }

        let mut results: Vec<(usize, f32)> = visited
            .keys()
            .filter_map(|id| {
                nodes.get(id).map(|node| {
                    let dist = self.distance(query, &node.vector);
                    (*id, dist)
                })
            })
            .collect();

        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(ef);
        results
    }
}

#[derive(Clone, PartialEq)]
struct NodeDist {
    id: usize,
    dist: f32,
}

impl PartialOrd for NodeDist {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        other.dist.partial_cmp(&self.dist)
    }
}

impl Ord for NodeDist {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.dist
            .partial_cmp(&other.dist)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Eq for NodeDist {}

impl VectorIndex for HnswIndex {
    fn add(&mut self, id: usize, vector: &Vector) -> Result<(), String> {
        if vector.dimension != self.config.dimension {
            return Err("Dimension mismatch".to_string());
        }

        let level = self.get_random_level();

        {
            let mut nodes = self.nodes.write().unwrap();
            nodes.insert(
                id,
                HnswNode {
                    id,
                    vector: vector.data.clone(),
                    edges: vec![],
                },
            );
        }

        let entry = {
            let ep = self.entry_point.read().unwrap();
            *ep
        };

        if let Some(entry_point) = entry {
            let mut max_lvl = self.max_level.write().unwrap();
            *max_lvl = (*max_lvl).max(level);
            drop(max_lvl);

            let mut current_level = {
                let m = self.max_level.read().unwrap();
                *m
            };

            let mut current_point = entry_point;
            let mut neighbors: Vec<Vec<(usize, f32)>> = vec![vec![]; current_level + 1];

            while current_level > level {
                let results = self.search_layer(&vector.data, 1, current_point);
                if let Some((next_point, _)) = results.first() {
                    current_point = *next_point;
                }
                current_level -= 1;
            }

            current_level = level;
            while current_level >= 0 {
                let results = self.search_layer(&vector.data, self.get_m(), current_point);

                for (neighbor_id, dist) in results {
                    if neighbor_id != id {
                        neighbors[current_level].push((neighbor_id, dist));
                    }
                }

                if !neighbors[current_level].is_empty() {
                    current_point = neighbors[current_level][0].0;
                }

                current_level = current_level.saturating_sub(1);
            }

            let m = self.get_m();
            for (_lvl, nbrs) in neighbors.iter().enumerate() {
                let mut final_nbrs = nbrs.clone();
                final_nbrs.truncate(m);
                {
                    let mut nodes = self.nodes.write().unwrap();
                    if let Some(node) = nodes.get_mut(&id) {
                        node.edges = final_nbrs.clone();
                    }
                    for (neighbor_id, dist) in &final_nbrs {
                        if let Some(neighbor) = nodes.get_mut(neighbor_id) {
                            if neighbor.edges.len() < m {
                                neighbor.edges.push((id, *dist));
                            }
                        }
                    }
                }
            }
        } else {
            let mut ep = self.entry_point.write().unwrap();
            *ep = Some(id);
        }

        Ok(())
    }

    fn search(&self, query: &Vector, k: usize) -> SearchResult {
        if query.dimension != self.config.dimension {
            return Err("Dimension mismatch".to_string());
        }

        let entry = {
            let ep = self.entry_point.read().unwrap();
            *ep
        };

        if entry.is_none() {
            return Ok(vec![]);
        }

        let entry_point = entry.unwrap();
        let max_level = *self.max_level.read().unwrap();

        let mut current_point = entry_point;
        let mut current_level = max_level;

        while current_level > 0 {
            let results = self.search_layer(&query.data, 1, current_point);
            if let Some((next_point, _)) = results.first() {
                current_point = *next_point;
            }
            current_level = current_level.saturating_sub(1);
        }

        let ef = self.get_ef().max(k);
        let results = self.search_layer(&query.data, ef, current_point);

        let mut final_results: Vec<(usize, f32)> = results
            .into_iter()
            .take(k)
            .map(|(id, dist)| {
                let score = match self.config.metric {
                    MetricType::Cosine => 1.0 - dist,
                    _ => -dist,
                };
                (id, score)
            })
            .collect();

        final_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        final_results.truncate(k);

        Ok(final_results)
    }

    fn remove(&mut self, id: usize) -> Result<(), String> {
        let mut nodes = self.nodes.write().unwrap();
        nodes.remove(&id);

        let ep = self.entry_point.read().unwrap();
        if *ep == Some(id) {
            drop(ep);
            let mut new_ep = self.entry_point.write().unwrap();
            *new_ep = nodes.keys().next().copied();
        }

        Ok(())
    }

    fn build(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn size(&self) -> usize {
        self.nodes.read().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vectors(count: usize, dimension: usize) -> Vec<Vector> {
        let mut vectors = Vec::with_capacity(count);
        for i in 0..count {
            let mut data = Vec::with_capacity(dimension);
            for j in 0..dimension {
                data.push(((i * dimension + j) as f32) / (count * dimension) as f32);
            }
            vectors.push(Vector::new(data));
        }
        vectors
    }

    #[test]
    fn test_hnsw_add_and_search() {
        let config = IndexConfig::with_hnsw(4);
        let mut index = HnswIndex::new(config);

        let vectors = create_test_vectors(10, 4);

        for (i, v) in vectors.iter().enumerate() {
            index.add(i, v).unwrap();
        }

        let query = Vector::new(vec![0.3, 0.3, 0.3, 0.3]);
        let results = index.search(&query, 3).unwrap();

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_hnsw_remove() {
        let config = IndexConfig::with_hnsw(4);
        let mut index = HnswIndex::new(config);

        let v = Vector::new(vec![1.0, 2.0, 3.0, 4.0]);
        index.add(0, &v).unwrap();

        assert_eq!(index.size(), 1);

        index.remove(0).unwrap();
        assert_eq!(index.size(), 0);
    }

    #[test]
    fn test_hnsw_dimension_mismatch() {
        let config = IndexConfig::with_hnsw(4);
        let mut index = HnswIndex::new(config);

        let v = Vector::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let result = index.add(0, &v);

        assert!(result.is_err());
    }
}
