use crate::tql::functions::vector::{comparison, Vector};

pub struct SimilaritySearch {
    vectors: Vec<Vector>,
}

impl SimilaritySearch {
    pub fn new(vectors: Vec<Vector>) -> Self {
        SimilaritySearch { vectors }
    }

    pub fn similar_to(&self, query: &Vector, threshold: f32) -> Vec<(usize, f32)> {
        self.vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let similarity = query.cosine_similarity(v);
                (idx, similarity)
            })
            .filter(|(_, sim)| *sim >= threshold)
            .collect()
    }

    pub fn similar_to_euclidean(&self, query: &Vector, threshold: f32) -> Vec<(usize, f32)> {
        self.vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let distance = query.euclidean_distance(v);
                (idx, distance)
            })
            .filter(|(_, dist)| *dist <= threshold)
            .collect()
    }

    pub fn nearest_neighbors(&self, query: &Vector, k: usize) -> Vec<(usize, f32)> {
        let mut distances: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| (idx, query.cosine_similarity(v)))
            .collect();

        distances.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        distances.into_iter().take(k).collect()
    }

    pub fn nearest_neighbors_euclidean(&self, query: &Vector, k: usize) -> Vec<(usize, f32)> {
        let mut distances: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| (idx, query.euclidean_distance(v)))
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        distances.into_iter().take(k).collect()
    }

    pub fn nearest_neighbors_manhattan(&self, query: &Vector, k: usize) -> Vec<(usize, f32)> {
        let mut distances: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| (idx, query.manhattan_distance(v)))
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        distances.into_iter().take(k).collect()
    }

    pub fn range_search(&self, query: &Vector, radius: f32) -> Vec<(usize, f32)> {
        self.vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let distance = query.euclidean_distance(v);
                (idx, distance)
            })
            .filter(|(_, dist)| *dist <= radius)
            .collect()
    }

    pub fn count(&self) -> usize {
        self.vectors.len()
    }
}

pub struct BruteForceSearch {
    vectors: Vec<Vector>,
}

impl BruteForceSearch {
    pub fn new(vectors: Vec<Vector>) -> Self {
        BruteForceSearch { vectors }
    }

    pub fn search_cosine(&self, query: &Vector, k: usize) -> Vec<(usize, f32)> {
        let mut scores: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| (idx, comparison::cosine_similarity(&query.data, &v.data)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        scores
    }

    pub fn search_euclidean(&self, query: &Vector, k: usize) -> Vec<(usize, f32)> {
        let mut distances: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| (idx, comparison::euclidean_distance(&query.data, &v.data)))
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        distances.truncate(k);
        distances
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vectors() -> Vec<Vector> {
        vec![
            Vector::new(vec![1.0, 0.0, 0.0]),
            Vector::new(vec![0.0, 1.0, 0.0]),
            Vector::new(vec![0.0, 0.0, 1.0]),
            Vector::new(vec![1.0, 1.0, 0.0]),
            Vector::new(vec![1.0, 0.0, 1.0]),
        ]
    }

    #[test]
    fn test_similar_to() {
        let vectors = create_test_vectors();
        let search = SimilaritySearch::new(vectors);

        let query = Vector::new(vec![1.0, 0.0, 0.0]);
        let results = search.similar_to(&query, 0.5);

        assert!(!results.is_empty());
        assert!(results.iter().all(|(_, sim)| *sim >= 0.5));
    }

    #[test]
    fn test_nearest_neighbors() {
        let vectors = create_test_vectors();
        let search = SimilaritySearch::new(vectors);

        let query = Vector::new(vec![1.0, 0.0, 0.0]);
        let results = search.nearest_neighbors(&query, 3);

        assert_eq!(results.len(), 3);
        assert!(results[0].1 >= results[1].1);
        assert!(results[1].1 >= results[2].1);
    }

    #[test]
    fn test_nearest_neighbors_euclidean() {
        let vectors = create_test_vectors();
        let search = SimilaritySearch::new(vectors);

        let query = Vector::new(vec![0.9, 0.1, 0.0]);
        let results = search.nearest_neighbors_euclidean(&query, 2);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_range_search() {
        let vectors = create_test_vectors();
        let search = SimilaritySearch::new(vectors);

        let query = Vector::new(vec![1.0, 0.0, 0.0]);
        let results = search.range_search(&query, 0.5);

        assert!(!results.is_empty());
        assert!(results.iter().all(|(_, dist)| *dist <= 0.5));
    }

    #[test]
    fn test_brute_force_search() {
        let vectors = create_test_vectors();
        let search = BruteForceSearch::new(vectors);

        let query = Vector::new(vec![1.0, 0.0, 0.0]);
        let results = search.search_cosine(&query, 2);

        assert_eq!(results.len(), 2);
    }
}
