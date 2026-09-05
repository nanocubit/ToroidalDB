//! Legacy storage adapter — wraps `HybridPersistentStore` (`ToroidalStore` backend)
//! for visualization modules. Previously used sled.

use crate::hybrid_storage::HybridPersistentStore;
use crate::math::MatryoshkaDim;
use crate::topology::edges::InterToroidalEdge;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

pub use crate::hybrid_storage::{Edge, Node};

#[derive(Clone)]
pub struct PersistentStore {
    inner: Arc<HybridPersistentStore>,
}

impl PersistentStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let store = HybridPersistentStore::open(path)?;
        Ok(PersistentStore {
            inner: Arc::new(store),
        })
    }

    pub fn insert(&self, node: Node) -> Result<bool> {
        self.inner.insert(node)
    }

    pub fn get(&self, id: u64) -> Result<Option<Node>> {
        self.inner.get(id)
    }

    pub fn len(&self) -> Result<usize> {
        self.inner.len()
    }

    pub fn is_empty(&self) -> Result<bool> {
        self.inner.len().map(|n| n == 0)
    }

    pub fn get_all(&self) -> Result<Vec<Node>> {
        self.inner.get_all()
    }

    pub fn add_edge(
        &self,
        from_id: u64,
        to_id: u64,
        relation_type: String,
        weight: f32,
    ) -> Result<()> {
        self.inner.add_edge(from_id, to_id, relation_type, weight)
    }

    pub fn get_neighbors(&self, node_id: u64) -> Result<Vec<Node>> {
        self.inner.get_neighbors(node_id)
    }

    pub fn graph_search_bfs(&self, start_id: u64, max_depth: usize) -> Result<Vec<(u64, usize)>> {
        use std::collections::{HashSet, VecDeque};

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        queue.push_back((start_id, 0));
        visited.insert(start_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth > max_depth {
                continue;
            }
            results.push((current_id, depth));
            if depth == max_depth {
                continue;
            }
            let neighbors = self.get_neighbors(current_id)?;
            for neighbor in neighbors {
                if !visited.contains(&neighbor.id) {
                    visited.insert(neighbor.id);
                    queue.push_back((neighbor.id, depth + 1));
                }
            }
        }
        Ok(results)
    }

    pub fn matryoshka_search(
        &self,
        query_vector: &[f32],
        dim: MatryoshkaDim,
        threshold: f32,
        _limit: Option<usize>,
    ) -> Result<Vec<(u64, f32)>> {
        // Note: limit is not forwarded — HybridPersistentStore does not support
        // it natively. Caller can truncate.
        self.inner.matryoshka_search(query_vector, dim, threshold)
    }

    pub fn add_inter_toroidal_edge(
        &self,
        source_id: u64,
        source_level: MatryoshkaDim,
        target_id: u64,
        target_level: MatryoshkaDim,
        relation_type: String,
        properties: serde_json::Value,
    ) -> Result<InterToroidalEdge> {
        self.inner.add_inter_toroidal_edge(
            source_id,
            source_level,
            target_id,
            target_level,
            relation_type,
            properties,
        )
    }

    pub fn get_inter_toroidal_edges(&self, node_id: u64) -> Result<Vec<InterToroidalEdge>> {
        self.inner.get_inter_toroidal_edges(node_id)
    }
}
