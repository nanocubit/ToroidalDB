use crate::math::{pad_or_truncate, toroidal_distance, MatryoshkaDim};
use crate::topology::edges::{HomotopyClass, InterToroidalEdge, ToroidalLevel};
use anyhow::{Context, Result};
use bincode;
use dashmap::DashMap;
use rayon::prelude::*;
use rocksdb::{DBIterator, Options as RocksOptions, DB as RocksDB};
use serde::{Deserialize, Serialize};
use serde_json;
use sled::{Db, Tree};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub target_id: u64,
    pub relation_type: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: u64,
    pub vector: Vec<f32>,
    pub properties: serde_json::Value,
    pub edges: Vec<Edge>,
}

#[derive(Clone)]
pub enum StorageBackend {
    Sled(Arc<Db>),
    RocksDB(Arc<RocksDB>),
}

#[derive(Clone)]
pub struct QueryCache {
    cache: Arc<DashMap<u64, CachedResult>>,
    max_size: usize,
    ttl_seconds: u64,
}

#[derive(Clone)]
struct CachedResult {
    value: Vec<(u64, f32)>,
    timestamp: SystemTime,
}

impl QueryCache {
    pub fn new(max_size: usize, ttl_seconds: u64) -> Self {
        QueryCache {
            cache: Arc::new(DashMap::new()),
            max_size,
            ttl_seconds,
        }
    }

    pub fn get(&self, key: u64) -> Option<Vec<(u64, f32)>> {
        if let Some(cached_result) = self.cache.get(&key) {
            if cached_result
                .timestamp
                .elapsed()
                .unwrap_or_default()
                .as_secs()
                < self.ttl_seconds
            {
                Some(cached_result.value.clone())
            } else {
                self.cache.remove(&key);
                None
            }
        } else {
            None
        }
    }

    pub fn put(&self, key: u64, value: Vec<(u64, f32)>) {
        if self.cache.len() >= self.max_size {
            // Remove oldest entries (simplified LRU)
            let keys_to_remove: Vec<u64> = self
                .cache
                .iter()
                .take(self.max_size / 4)
                .map(|entry| *entry.key())
                .collect();
            for key in keys_to_remove {
                self.cache.remove(&key);
            }
        }

        self.cache.insert(
            key,
            CachedResult {
                value,
                timestamp: SystemTime::now(),
            },
        );
    }

    pub fn clear(&self) {
        self.cache.clear();
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

#[derive(Clone)]
pub struct HybridPersistentStore {
    backend: StorageBackend,
    cache: Arc<DashMap<u64, Arc<Node>>>,
    node_count: Arc<AtomicUsize>,
    query_cache: QueryCache,
    hybrid_query_cache: QueryCache,
}

impl HybridPersistentStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Try to determine which backend to use based on existing data or size
        let sled_path = path.join("sled");
        let rocksdb_path = path.join("rocksdb");

        let backend = if rocksdb_path.exists() {
            // Use RocksDB if it exists
            let mut opts = RocksOptions::default();
            opts.create_if_missing(true);
            let rocks_db = RocksDB::open(&opts, rocksdb_path)?;
            StorageBackend::RocksDB(Arc::new(rocks_db))
        } else if sled_path.exists() {
            // Use sled if it exists
            let sled_db = sled::open(&sled_path)?;
            let count = sled_db.len() as usize;

            // Migrate to RocksDB if too many nodes
            if count > 100_000 {
                let rocks_db = Self::migrate_sled_to_rocksdb(&sled_db, &rocksdb_path)?;
                StorageBackend::RocksDB(Arc::new(rocks_db))
            } else {
                StorageBackend::Sled(Arc::new(sled_db))
            }
        } else {
            // Start with sled for new databases
            let sled_db = sled::open(&sled_path)?;
            StorageBackend::Sled(Arc::new(sled_db))
        };

        let node_count = match &backend {
            StorageBackend::Sled(db) => db.len(),
            StorageBackend::RocksDB(db) => {
                let mut iter = db.raw_iterator();
                let mut count = 0;
                iter.seek_to_first();
                while iter.valid() {
                    count += 1;
                    iter.next();
                }
                count
            }
        };

        Ok(HybridPersistentStore {
            backend,
            cache: Arc::new(DashMap::new()),
            node_count: Arc::new(AtomicUsize::new(node_count)),
            query_cache: QueryCache::new(1000, 3600),
            hybrid_query_cache: QueryCache::new(500, 3600),
        })
    }

    fn migrate_sled_to_rocksdb(sled_db: &Db, rocksdb_path: &Path) -> Result<RocksDB> {
        println!("🔄 Migrating from sled to RocksDB...");

        let mut opts = RocksOptions::default();
        opts.create_if_missing(true);
        let rocks_db = RocksDB::open(&opts, rocksdb_path)?;

        // Migrate all data
        for item in sled_db.iter() {
            let (key, value) = item?;
            rocks_db.put(&key, &value)?;
        }

        println!("✅ Migration completed");
        Ok(rocks_db)
    }

    pub fn insert(&self, node: Node) -> Result<bool> {
        let key = node.id.to_be_bytes();
        let serialized = bincode::serialize(&node).context("Failed to serialize node")?;

        // Check if exists
        let exists = match &self.backend {
            StorageBackend::Sled(db) => {
                let tree = db.open_tree("nodes")?;
                tree.get(&key)?.is_some()
            }
            StorageBackend::RocksDB(db) => db.get(&key)?.is_some(),
        };

        if !exists {
            match &self.backend {
                StorageBackend::Sled(db) => {
                    let tree = db.open_tree("nodes")?;
                    tree.insert(&key, serialized)?;
                }
                StorageBackend::RocksDB(db) => {
                    db.put(&key, serialized)?;
                }
            }

            self.cache.insert(node.id, Arc::new(node));
            self.node_count.fetch_add(1, Ordering::Relaxed);
        }

        Ok(!exists)
    }

    pub fn get(&self, id: u64) -> Result<Option<Node>> {
        // Check cache first
        if let Some(cached) = self.cache.get(&id) {
            return Ok(Some((**cached).clone()));
        }

        let key = id.to_be_bytes();
        let node = match &self.backend {
            StorageBackend::Sled(db) => {
                let tree = db.open_tree("nodes")?;
                match tree.get(&key)? {
                    Some(bytes) => {
                        let node: Node =
                            bincode::deserialize(&bytes).context("Failed to deserialize node")?;
                        Some(node)
                    }
                    None => None,
                }
            }
            StorageBackend::RocksDB(db) => match db.get(&key)? {
                Some(bytes) => {
                    let node: Node =
                        bincode::deserialize(&bytes).context("Failed to deserialize node")?;
                    Some(node)
                }
                None => None,
            },
        };

        // Cache the result
        if let Some(ref node) = node {
            self.cache.insert(id, Arc::new(node.clone()));
        }

        Ok(node)
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self.node_count.load(Ordering::Relaxed))
    }

    pub fn get_all(&self) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();

        match &self.backend {
            StorageBackend::Sled(db) => {
                let tree = db.open_tree("nodes")?;
                for item in tree.iter() {
                    let (_, value) = item?;
                    let node: Node =
                        bincode::deserialize(&value).context("Failed to deserialize node")?;
                    nodes.push(node);
                }
            }
            StorageBackend::RocksDB(db) => {
                let mut iter = db.raw_iterator();
                iter.seek_to_first();
                while iter.valid() {
                    if let Some(value) = iter.value() {
                        if let Ok(node) = bincode::deserialize(&value) {
                            nodes.push(node);
                        }
                    }
                    iter.next();
                }
            }
        }

        Ok(nodes)
    }

    pub fn add_edge(
        &self,
        from_id: u64,
        to_id: u64,
        relation_type: String,
        weight: f32,
    ) -> Result<()> {
        let mut from_node = match self.get(from_id)? {
            Some(node) => node,
            None => return Err(anyhow::anyhow!("Source node not found: {}", from_id)),
        };

        from_node.edges.push(Edge {
            target_id: to_id,
            relation_type,
            weight,
        });

        self.insert(from_node)?;
        Ok(())
    }

    pub fn get_neighbors(&self, node_id: u64) -> Result<Vec<Node>> {
        let node = match self.get(node_id)? {
            Some(node) => node,
            None => return Ok(Vec::new()),
        };

        let neighbors: Result<Vec<Node>> = node
            .edges
            .par_iter()
            .map(|edge| self.get(edge.target_id))
            .filter_map(|result| result.transpose())
            .collect();

        neighbors
    }

    pub fn matryoshka_search(
        &self,
        query: &[f32],
        dimension: MatryoshkaDim,
        threshold: f32,
    ) -> Result<Vec<(u64, f32)>> {
        let hash = self.hash_query(query, dimension, threshold);

        // Check cache first
        if let Some(cached) = self.query_cache.get(hash) {
            return Ok(cached);
        }

        let target_dim = dimension.size();
        let padded_query = pad_or_truncate(query, target_dim);

        let results: Vec<(u64, f32)> = self
            .get_all()?
            .par_iter()
            .filter_map(|node| {
                if node.vector.len() >= target_dim {
                    let node_vec = pad_or_truncate(&node.vector, target_dim);
                    let distance = toroidal_distance(&padded_query, &node_vec);

                    if distance <= threshold {
                        Some((node.id, distance))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Cache results
        self.query_cache.put(hash, results.clone());
        Ok(results)
    }

    pub fn hybrid_matryoshka_search(
        &self,
        query: &[f32],
        dimension: MatryoshkaDim,
        threshold: f32,
    ) -> Result<Vec<(u64, f32)>> {
        let hash = self.hash_query(query, dimension, threshold);

        if let Some(cached) = self.hybrid_query_cache.get(hash) {
            return Ok(cached);
        }

        // First try exact match search
        let exact_results = self.matryoshka_search(query, dimension, threshold)?;

        // If not enough results, try approximate search
        let mut results = exact_results;
        if results.len() < 10 {
            let approx_threshold = threshold * 1.5;
            let approx_results = self.matryoshka_search(query, dimension, approx_threshold)?;

            // Merge and deduplicate
            let mut result_map: HashMap<u64, f32> = results.into_iter().collect();
            for (id, distance) in approx_results {
                result_map.entry(id).or_insert_with(|| distance);
            }
            results = result_map.into_iter().collect();
        }

        // Sort by distance
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Cache results
        self.hybrid_query_cache.put(hash, results.clone());
        Ok(results)
    }

    fn hash_query(&self, vector: &[f32], dim: MatryoshkaDim, threshold: f32) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        // Hash vector with rounding for stability
        for &val in vector {
            ((val * 10000.0).round() as i64).hash(&mut hasher);
        }
        (dim as i32).hash(&mut hasher);
        ((threshold * 10000.0).round() as i64).hash(&mut hasher);
        hasher.finish()
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
        self.query_cache.clear();
        self.hybrid_query_cache.clear();
    }

    pub fn get_storage_type(&self) -> &'static str {
        match &self.backend {
            StorageBackend::Sled(_) => "sled",
            StorageBackend::RocksDB(_) => "rocksdb",
        }
    }

    pub fn should_migrate_to_rocksdb(&self) -> bool {
        self.node_count.load(Ordering::Relaxed) > 100_000
            && matches!(self.backend, StorageBackend::Sled(_))
    }

    pub fn remove(&self, id: u64) -> Result<bool> {
        let key = id.to_be_bytes();

        // Check if exists
        let exists = match &self.backend {
            StorageBackend::Sled(db) => {
                let tree = db.open_tree("nodes")?;
                tree.get(&key)?.is_some()
            }
            StorageBackend::RocksDB(db) => db.get(&key)?.is_some(),
        };

        if exists {
            // Remove from backend
            match &self.backend {
                StorageBackend::Sled(db) => {
                    let tree = db.open_tree("nodes")?;
                    tree.remove(&key)?;
                }
                StorageBackend::RocksDB(db) => {
                    db.delete(&key)?;
                }
            }

            // Remove from cache
            self.cache.remove(&id);
            self.node_count.fetch_sub(1, Ordering::Relaxed);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_node(&self, id: u64, node: Node) -> Result<()> {
        // Verify node ID matches
        if node.id != id {
            return Err(anyhow::anyhow!("Node ID mismatch"));
        }

        let key = id.to_be_bytes();
        let serialized = bincode::serialize(&node).context("Failed to serialize node")?;

        match &self.backend {
            StorageBackend::Sled(db) => {
                let tree = db.open_tree("nodes")?;
                tree.insert(&key, serialized)?;
            }
            StorageBackend::RocksDB(db) => {
                db.put(&key, serialized)?;
            }
        }

        // Update cache
        self.cache.insert(id, Arc::new(node));
        Ok(())
    }

    pub fn get_node_count(&self) -> usize {
        self.node_count.load(Ordering::Relaxed)
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
        self.query_cache.clear();
        self.hybrid_query_cache.clear();
    }

    /// Get a reference to the hybrid query cache (public for executor access)
    pub fn hybrid_query_cache(&self) -> &QueryCache {
        &self.hybrid_query_cache
    }

    /// Add an inter-toroidal edge between nodes at different levels
    pub fn add_inter_toroidal_edge(
        &self,
        source_id: u64,
        source_level: MatryoshkaDim,
        target_id: u64,
        target_level: MatryoshkaDim,
        relation_type: String,
        properties: serde_json::Value,
    ) -> Result<InterToroidalEdge> {
        // Get source and target nodes
        let source_node = self
            .get(source_id)?
            .ok_or_else(|| anyhow::anyhow!("Source node not found: {}", source_id))?;

        let target_node = self
            .get(target_id)?
            .ok_or_else(|| anyhow::anyhow!("Target node not found: {}", target_id))?;

        // Convert MatryoshkaDim to ToroidalLevel
        let source_toroidal = ToroidalLevel::from_dim(source_level);
        let target_toroidal = ToroidalLevel::from_dim(target_level);

        // Create inter-toroidal edge with automatic distance calculation
        let edge = InterToroidalEdge::new(
            (source_toroidal, source_id),
            (target_toroidal, target_id),
            relation_type,
            &source_node.vector,
            &target_node.vector,
            properties,
        );

        // Also add a regular edge for backward compatibility
        self.add_edge(source_id, target_id, edge.relation_type.clone(), edge.topological_distance)?;

        Ok(edge)
    }

    /// Get all inter-toroidal edges for a node
    pub fn get_inter_toroidal_edges(&self, node_id: u64) -> Result<Vec<InterToroidalEdge>> {
        let node = self
            .get(node_id)?
            .ok_or_else(|| anyhow::anyhow!("Node not found: {}", node_id))?;

        let mut inter_toroidal_edges = Vec::new();

        // For each edge, check if it connects to a different toroidal level
        for edge in &node.edges {
            if let Ok(Some(target_node)) = self.get(edge.target_id) {
                // In a full implementation, we would store the toroidal level information
                // with each edge. For now, we infer it from vector dimensions.
                let source_level = ToroidalLevel::from_dim(MatryoshkaDim::from_size(node.vector.len()));
                let target_level = ToroidalLevel::from_dim(MatryoshkaDim::from_size(target_node.vector.len()));

                if source_level != target_level {
                    inter_toroidal_edges.push(InterToroidalEdge {
                        source: (source_level, node_id),
                        target: (target_level, edge.target_id),
                        relation_type: edge.relation_type.clone(),
                        topological_distance: edge.weight,
                        homotopy_class: HomotopyClass::Direct,
                        properties: serde_json::json!({}),
                    });
                }
            }
        }

        Ok(inter_toroidal_edges)
    }
}
