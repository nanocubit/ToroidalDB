use crate::index::{IndexConfig, MetricType, VectorIndex};
use crate::tql::functions::{cosine_similarity, euclidean_distance, manhattan_distance, Vector};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::Rng;
use rand::SeedableRng;
use std::collections::HashMap;

const IVF_DEFAULT_NLIST: usize = 100;
const IVF_DEFAULT_NPROBE: usize = 10;

pub struct IvfIndex {
    config: IndexConfig,
    centroids: Vec<Vec<f32>>,
    clusters: Vec<Vec<(usize, Vec<f32>)>>,
    all_vectors: HashMap<usize, Vec<f32>>,
}

impl IvfIndex {
    pub fn new(config: IndexConfig) -> Self {
        let nlist = config.ivf_nlist.unwrap_or(IVF_DEFAULT_NLIST);

        IvfIndex {
            config,
            centroids: Vec::with_capacity(nlist),
            clusters: vec![vec![]; nlist],
            all_vectors: HashMap::new(),
        }
    }

    fn get_nlist(&self) -> usize {
        self.config.ivf_nlist.unwrap_or(IVF_DEFAULT_NLIST)
    }

    fn get_nprobe(&self) -> usize {
        self.config.ivf_nprobe.unwrap_or(IVF_DEFAULT_NPROBE)
    }

    fn distance(&self, v1: &[f32], v2: &[f32]) -> f32 {
        match self.config.metric {
            MetricType::Cosine => 1.0 - cosine_similarity(v1, v2),
            MetricType::Euclidean => euclidean_distance(v1, v2),
            MetricType::Manhattan => manhattan_distance(v1, v2),
        }
    }

    fn initialize_centroids(&mut self, vectors: &[Vec<f32>]) -> Result<(), String> {
        let nlist = self.get_nlist();
        let dim = self.config.dimension;

        if vectors.is_empty() {
            return Err("No vectors to index".to_string());
        }

        let mut rng = StdRng::from_entropy();
        let sample_size = nlist.min(vectors.len());

        self.centroids.clear();
        let mut indices: Vec<usize> = (0..vectors.len()).collect();
        indices.shuffle(&mut rng);

        for i in 0..sample_size {
            self.centroids.push(vectors[indices[i]].clone());
        }

        for _ in sample_size..nlist {
            let mut centroid = vec![0.0f32; dim];
            for j in 0..dim {
                centroid[j] = rng.gen::<f32>();
            }
            self.centroids.push(centroid);
        }

        Ok(())
    }

    fn assign_to_clusters(&mut self, vectors: &[(usize, &[f32])]) {
        for (id, vec) in vectors {
            let mut min_dist = f32::MAX;
            let mut closest_centroid = 0;

            for (i, centroid) in self.centroids.iter().enumerate() {
                let dist = self.distance(vec, centroid);
                if dist < min_dist {
                    min_dist = dist;
                    closest_centroid = i;
                }
            }

            self.clusters[closest_centroid].push((*id, vec.to_vec()));
        }
    }

    fn update_centroids(&mut self) {
        let dim = self.config.dimension;

        for (i, cluster) in self.clusters.iter().enumerate() {
            if cluster.is_empty() {
                continue;
            }

            let mut new_centroid = vec![0.0f32; dim];
            for (_, vec) in cluster {
                for (j, val) in vec.iter().enumerate() {
                    new_centroid[j] += val;
                }
            }

            let count = cluster.len() as f32;
            for j in 0..dim {
                new_centroid[j] /= count;
            }

            self.centroids[i] = new_centroid;
        }
    }

    fn kmeans_iteration(&mut self, vectors: &[(usize, &[f32])], max_iterations: usize) {
        for _ in 0..max_iterations {
            for cluster in &mut self.clusters {
                cluster.clear();
            }

            self.assign_to_clusters(vectors);
            self.update_centroids();
        }
    }

    fn search_in_cluster(
        &self,
        query: &[f32],
        cluster: &[(usize, Vec<f32>)],
        k: usize,
    ) -> Vec<(usize, f32)> {
        let mut results: Vec<(usize, f32)> = cluster
            .iter()
            .map(|(id, vec)| {
                let dist = self.distance(query, vec);
                let score = match self.config.metric {
                    MetricType::Cosine => 1.0 - dist,
                    _ => -dist,
                };
                (*id, score)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k);
        results
    }
}

impl VectorIndex for IvfIndex {
    fn add(&mut self, id: usize, vector: &Vector) -> Result<(), String> {
        if vector.dimension != self.config.dimension {
            return Err("Dimension mismatch".to_string());
        }

        self.all_vectors.insert(id, vector.data.clone());

        if self.centroids.is_empty() {
            self.initialize_centroids(&self.all_vectors.values().cloned().collect::<Vec<_>>())?;
        }

        let mut min_dist = f32::MAX;
        let mut closest_centroid = 0;

        for (i, centroid) in self.centroids.iter().enumerate() {
            let dist = self.distance(&vector.data, centroid);
            if dist < min_dist {
                min_dist = dist;
                closest_centroid = i;
            }
        }

        self.clusters[closest_centroid].push((id, vector.data.clone()));

        Ok(())
    }

    fn search(&self, query: &Vector, k: usize) -> Result<Vec<(usize, f32)>, String> {
        if query.dimension != self.config.dimension {
            return Err("Dimension mismatch".to_string());
        }

        let nprobe = self.get_nprobe().min(self.centroids.len());

        let mut centroid_distances: Vec<(usize, f32)> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(i, centroid)| {
                let dist = self.distance(&query.data, centroid);
                (i, dist)
            })
            .collect();

        centroid_distances
            .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut all_results: Vec<(usize, f32)> = Vec::new();

        for (centroid_idx, _) in centroid_distances.into_iter().take(nprobe) {
            let cluster_results =
                self.search_in_cluster(&query.data, &self.clusters[centroid_idx], k);
            all_results.extend(cluster_results);
        }

        all_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(k);

        Ok(all_results)
    }

    fn remove(&mut self, id: usize) -> Result<(), String> {
        self.all_vectors.remove(&id);

        for cluster in &mut self.clusters {
            cluster.retain(|(i, _)| *i != id);
        }

        Ok(())
    }

    fn build(&mut self) -> Result<(), String> {
        if self.all_vectors.is_empty() {
            return Ok(());
        }

        let vector_refs: Vec<(usize, Vec<f32>)> = self
            .all_vectors
            .iter()
            .map(|(id, vec)| (*id, vec.clone()))
            .collect();

        let vectors: Vec<(usize, &[f32])> = vector_refs
            .iter()
            .map(|(id, vec)| (*id, vec.as_slice()))
            .collect();

        let vectors_for_init: Vec<Vec<f32>> = vector_refs.iter().map(|(_, v)| v.clone()).collect();

        self.initialize_centroids(&vectors_for_init)?;
        self.kmeans_iteration(&vectors, 20);

        Ok(())
    }

    fn size(&self) -> usize {
        self.all_vectors.len()
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
    fn test_ivf_add_and_search() {
        let config = IndexConfig::with_ivf(4, 4);
        let mut index = IvfIndex::new(config);

        let vectors = create_test_vectors(10, 4);

        for (i, v) in vectors.iter().enumerate() {
            index.add(i, v).unwrap();
        }

        let query = Vector::new(vec![0.3, 0.3, 0.3, 0.3]);
        let results = index.search(&query, 3).unwrap();

        assert!(results.len() <= 3);
    }

    #[test]
    fn test_ivf_build() {
        let config = IndexConfig::with_ivf(4, 4);
        let mut index = IvfIndex::new(config);

        let vectors = create_test_vectors(10, 4);

        for (i, v) in vectors.iter().enumerate() {
            index.add(i, v).unwrap();
        }

        index.build().unwrap();
        assert!(index.size() > 0);
    }

    #[test]
    fn test_ivf_remove() {
        let config = IndexConfig::with_ivf(4, 4);
        let mut index = IvfIndex::new(config);

        let v = Vector::new(vec![1.0, 2.0, 3.0, 4.0]);
        index.add(0, &v).unwrap();

        assert_eq!(index.size(), 1);

        index.remove(0).unwrap();
        assert_eq!(index.size(), 0);
    }

    #[test]
    fn test_ivf_dimension_mismatch() {
        let config = IndexConfig::with_ivf(4, 4);
        let mut index = IvfIndex::new(config);

        let v = Vector::new(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let result = index.add(0, &v);

        assert!(result.is_err());
    }
}
