//! Optimized graph traversal with Magic Set and Semi-naïve evaluation.
//!
//! Magic Set Rewrites: переписывает рекурсивный запрос так, чтобы рекурсия
//! шла только от начальных узлов, а не от всего графа.
//!
//! Semi-naïve evaluation: на каждом шаге рекурсии использует только НОВЫЕ
//! узлы (дельта), не повторяет уже найденные.

use crate::hybrid_storage::HybridPersistentStore;
use std::collections::HashSet;
use std::sync::Arc;

/// Результат рекурсивного обхода.
#[derive(Debug, Clone)]
pub struct TraversalResult {
    pub visited: Vec<u64>,
    pub levels: Vec<Vec<u64>>,
    pub total_visited: usize,
}

/// Оптимизированный графовый обход с Magic Set + Semi-naïve.
pub struct MagicTraversal {
    store: Arc<HybridPersistentStore>,
}

impl MagicTraversal {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self { store }
    }

    /// Semi-naïve BFS: на каждом шаге использует только новые узлы.
    ///
    /// В отличие от классического BFS, где visited проверяется постфактум,
    /// semi-naïve явно разделяет "новые" и "уже посещённые" узлы на каждом уровне.
    ///
    /// ```
    /// delta_0 = {start_nodes}
    /// visited = delta_0
    /// for i = 1..max_hops:
    ///     delta_i = neighbors(delta_{i-1}) \ visited
    ///     visited = visited ∪ delta_i
    ///     if delta_i = ∅: break
    /// ```
    pub fn semi_naive_bfs(
        &self,
        start_nodes: &[u64],
        max_hops: u32,
        min_hops: u32,
        edge_type: Option<&str>,
    ) -> Result<TraversalResult, String> {
        let mut visited: HashSet<u64> = HashSet::new();
        let mut delta: HashSet<u64> = start_nodes.iter().copied().collect();
        let mut levels: Vec<Vec<u64>> = Vec::new();

        // Level 0: start nodes
        levels.push(delta.iter().copied().collect());
        visited.extend(&delta);

        for _hop in 1..=max_hops {
            if delta.is_empty() {
                break;
            }

            // Compute neighbors of delta (new frontier)
            let mut next_delta: HashSet<u64> = HashSet::new();

            for &node_id in &delta {
                let neighbors = self
                    .store
                    .get_neighbors(node_id)
                    .map_err(|e| format!("Failed to get neighbors: {e}"))?;

                for neighbor in &neighbors {
                    // Filter by edge type if specified
                    if let Some(et) = edge_type {
                        let has_edge = neighbor
                            .edges
                            .iter()
                            .any(|e| e.relation_type == et && e.target_id == node_id);
                        if !has_edge {
                            continue;
                        }
                    }

                    // Only add if not already visited (semi-naïve: delta = new \ visited)
                    if !visited.contains(&neighbor.id) {
                        next_delta.insert(neighbor.id);
                    }
                }
            }

            // Record level
            let level_nodes: Vec<u64> = next_delta.iter().copied().collect();
            if !level_nodes.is_empty() {
                levels.push(level_nodes.clone());
            }

            // Update visited with new nodes
            visited.extend(&next_delta);
            delta = next_delta;
        }

        // Filter by min_hops
        let mut result: Vec<u64> = Vec::new();
        for (level, nodes) in levels.iter().enumerate() {
            if level as u32 >= min_hops {
                result.extend(nodes);
            }
        }

        // Remove duplicates and sort
        result.sort_unstable();
        result.dedup();

        Ok(TraversalResult {
            visited: result,
            levels,
            total_visited: visited.len(),
        })
    }

    /// Magic Set: находит достижимые узлы, начиная от seed, с учётом фильтра.
    ///
    /// Magic Set — переписывание запроса: вместо "найти все пути от всех узлов"
    /// мы "начинаем от seed и идём только по достижимым".
    /// Это эквивалентно тому, что в Datalog называют "magic set rewriting".
    pub fn magic_set_traversal(
        &self,
        seed: u64,
        max_hops: u32,
        edge_type: Option<&str>,
    ) -> Result<TraversalResult, String> {
        // Magic set: начальный узел — единственный seed
        // Все остальные вычисляются как "достижимые из seed"
        self.semi_naive_bfs(&[seed], max_hops, 0, edge_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_storage::{Edge, Node};
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_test_graph() -> Arc<HybridPersistentStore> {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());

        // Create nodes: 0-1-2-3-4 (chain)
        for i in 0..5 {
            let node = Node {
                id: i,
                vector: vec![i as f32 / 5.0; 4],
                properties: serde_json::json!({"id": i}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Add edges: 0→1, 1→2, 2→3, 3→4, 0→2 (shortcut)
        for (from, to) in &[(0, 1), (1, 2), (2, 3), (3, 4), (0, 2)] {
            store
                .add_edge(*from, *to, "knows".to_string(), 1.0)
                .unwrap();
        }

        store
    }

    #[test]
    fn test_semi_naive_bfs_chain() {
        let store = create_test_graph();
        let traversal = MagicTraversal::new(store);

        let result = traversal.semi_naive_bfs(&[0], 3, 0, None).unwrap();
        // 0→1→2→3 (3 hops from 0)
        assert!(result.total_visited >= 4);
    }

    #[test]
    fn test_semi_naive_min_hops() {
        let store = create_test_graph();
        let traversal = MagicTraversal::new(store);

        let result = traversal.semi_naive_bfs(&[0], 3, 2, None).unwrap();
        // min_hops=2: only nodes at distance >= 2
        assert!(!result.visited.contains(&1)); // distance 1
                                               // May contain 2, 3
    }

    #[test]
    fn test_magic_set_traversal() {
        let store = create_test_graph();
        let traversal = MagicTraversal::new(store);

        let result = traversal.magic_set_traversal(0, 2, Some("knows")).unwrap();
        // 0→1, 0→2 (through shortcut), 1→2 (through 1-2)
        assert!(result.total_visited >= 3);
    }
}
