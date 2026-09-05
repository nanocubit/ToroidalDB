use crate::math::{pad_or_truncate, toroidal_distance, MatryoshkaDim};
use crate::topology::edges::{HomotopyClass, InterToroidalEdge, ToroidalLevel};
use crate::tql::wal::Wal;
use anyhow::{Context, Result};
use bincode;
use dashmap::DashMap;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, RwLock,
};
use std::time::SystemTime;
use toroidal_storage::{BatchOp, Snapshot, Storage, StorageFactory};

#[cfg(feature = "fjall-storage")]
use toroidal_storage::FjallFactory;
#[cfg(feature = "toroidal-store-backend")]
use toroidal_storage::ToroidalStoreFactory;

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
    /// Properties are stored as an embedded JSON string so that bincode
    /// (which does not support `deserialize_any`) can round-trip them
    /// through the storage backend.
    #[serde(with = "json_value_string")]
    pub properties: serde_json::Value,
    pub edges: Vec<Edge>,
}

/// Serializes `serde_json::Value` as a JSON string, making it compatible
/// with bincode's (de)serialization (bincode rejects `deserialize_any`).
mod json_value_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &serde_json::Value, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = serde_json::to_string(value).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<serde_json::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        serde_json::from_str(&s).map_err(serde::de::Error::custom)
    }
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
        Self {
            cache: Arc::new(DashMap::new()),
            max_size,
            ttl_seconds,
        }
    }
    pub fn get(&self, key: u64) -> Option<Vec<(u64, f32)>> {
        if let Some(cached) = self.cache.get(&key) {
            if cached.timestamp.elapsed().unwrap_or_default().as_secs() < self.ttl_seconds {
                Some(cached.value.clone())
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
            let keys: Vec<u64> = self
                .cache
                .iter()
                .take(self.max_size / 4)
                .map(|e| *e.key())
                .collect();
            for k in keys {
                self.cache.remove(&k);
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
    pub storage: Arc<dyn Storage>,
    cache: Arc<DashMap<u64, Arc<Node>>>,
    node_count: Arc<AtomicUsize>,
    pub(crate) query_cache: QueryCache,
    pub(crate) hybrid_query_cache: QueryCache,
    wal: Option<Arc<Wal>>,
}

fn key(id: u64) -> [u8; 8] {
    id.to_be_bytes()
}

const NODE_PREFIX: &[u8] = b"node:";

fn node_key(id: u64) -> Vec<u8> {
    let mut k = NODE_PREFIX.to_vec();
    k.extend_from_slice(&key(id));
    k
}

fn edge_key(from: u64, to: u64) -> Vec<u8> {
    let mut k = b"edge:".to_vec();
    k.extend_from_slice(&key(from));
    k.extend_from_slice(&key(to));
    k
}

impl HybridPersistentStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        #[cfg(feature = "toroidal-store-backend")]
        let factory = ToroidalStoreFactory;
        #[cfg(all(not(feature = "toroidal-store-backend"), feature = "fjall-storage"))]
        let factory = FjallFactory;
        #[cfg(not(any(feature = "toroidal-store-backend", feature = "fjall-storage")))]
        let factory = toroidal_storage::MemoryFactory;

        let storage = factory.open(path)?;
        let node_count = {
            let snapshot = storage.snapshot()?;
            let mut count = 0;
            let iter = snapshot.scan(
                Some(NODE_PREFIX),
                Some(b"node;\xff\xff\xff\xff\xff\xff\xff\xff"),
            )?;
            for _ in iter {
                count += 1;
            }
            count
        };

        Ok(HybridPersistentStore {
            storage: Arc::from(storage),
            cache: Arc::new(DashMap::new()),
            node_count: Arc::new(AtomicUsize::new(node_count)),
            query_cache: QueryCache::new(1000, 3600),
            hybrid_query_cache: QueryCache::new(500, 3600),
            wal: match Wal::open(path) {
                Ok(w) => Some(Arc::new(w)),
                Err(_) => None,
            },
        })
    }

    pub fn insert(&self, node: Node) -> Result<bool> {
        let exists = self.storage.get(&node_key(node.id))?.is_some();
        if !exists {
            let serialized = bincode::serialize(&node).context("Failed to serialize")?;
            self.storage.batch(&[BatchOp::Put {
                key: node_key(node.id),
                value: serialized,
            }])?;
            self.cache.insert(node.id, Arc::new(node));
            self.node_count.fetch_add(1, Ordering::Relaxed);
        }
        Ok(!exists)
    }

    pub fn get(&self, id: u64) -> Result<Option<Node>> {
        if let Some(cached) = self.cache.get(&id) {
            return Ok(Some((**cached).clone()));
        }
        let bytes = self.storage.get(&node_key(id))?;
        let node = match bytes {
            Some(b) => {
                let n: Node = bincode::deserialize(&b).context("Failed to deserialize")?;
                Some(n)
            }
            None => None,
        };
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
        let snapshot = self.storage.snapshot()?;
        let iter = snapshot.scan(
            Some(NODE_PREFIX),
            Some(b"node;\xff\xff\xff\xff\xff\xff\xff\xff"),
        )?;
        for entry in iter {
            let (_, value) = entry?;
            nodes.push(bincode::deserialize(&value)?);
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
            Some(n) => n,
            None => return Err(anyhow::anyhow!("Source node {} not found", from_id)),
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
            Some(n) => n,
            None => return Ok(Vec::new()),
        };
        let executor = rayon::ThreadPoolBuilder::new().build().unwrap();
        let results: Vec<Result<Node>> = node
            .edges
            .par_iter()
            .map(|e| self.get(e.target_id))
            .filter_map(|r| r.transpose())
            .collect();
        results.into_iter().collect()
    }

    pub fn matryoshka_search(
        &self,
        query: &[f32],
        dimension: MatryoshkaDim,
        threshold: f32,
    ) -> Result<Vec<(u64, f32)>> {
        let hash = self.hash_query(query, dimension, threshold);
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
        let mut results = self.matryoshka_search(query, dimension, threshold)?;
        if results.len() < 10 {
            let approx = self.matryoshka_search(query, dimension, threshold * 1.5)?;
            let mut map: HashMap<u64, f32> = results.into_iter().collect();
            for (id, d) in approx {
                map.entry(id).or_insert(d);
            }
            results = map.into_iter().collect();
        }
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        self.hybrid_query_cache.put(hash, results.clone());
        Ok(results)
    }

    fn hash_query(&self, vector: &[f32], dim: MatryoshkaDim, threshold: f32) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
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
        if cfg!(feature = "toroidal-store-backend") {
            "toroidal-store"
        } else if cfg!(feature = "fjall-storage") {
            "fjall"
        } else {
            "memory"
        }
    }

    /// Durability checkpoint: freeze then flush all buffered writes.
    /// After this call, all acknowledged writes survive process termination.
    pub fn flush(&self) -> Result<()> {
        self.storage.checkpoint().context("flush failed")
    }

    /// Trigger space reclamation.  On the ToroidalStore backend this merges
    /// all segments into one, applying safe retention horizon.
    pub fn compact(&self) -> Result<()> {
        self.storage.compact().context("compact failed")
    }

    /// Returns a point-in-time snapshot of the storage backend.
    /// Use `get_at` for consistent reads across multiple keys.
    pub fn snapshot(&self) -> Result<Box<dyn toroidal_storage::Snapshot>> {
        self.storage.snapshot().context("snapshot failed")
    }

    /// Read a node at a specific snapshot.  Returns `None` if the node
    /// did not exist at that point in time, without visible cache effects.
    pub fn get_at(&self, snap: &dyn toroidal_storage::Snapshot, id: u64) -> Result<Option<Node>> {
        let bytes = snap.get(&node_key(id)).context("get_at failed")?;
        match bytes {
            Some(b) => {
                let node: Node = bincode::deserialize(&b).context("Failed to deserialize")?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    pub fn should_migrate_to_rocksdb(&self) -> bool {
        false
    }

    pub fn remove(&self, id: u64) -> Result<bool> {
        if self.storage.get(&node_key(id))?.is_some() {
            self.storage.delete(&node_key(id))?;
            self.cache.remove(&id);
            self.node_count.fetch_sub(1, Ordering::Relaxed);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_node(&self, id: u64, node: Node) -> Result<()> {
        if node.id != id {
            return Err(anyhow::anyhow!("Node ID mismatch"));
        }
        let serialized = bincode::serialize(&node).context("Failed to serialize")?;
        self.storage.put(&node_key(id), &serialized)?;
        self.cache.insert(id, Arc::new(node));
        Ok(())
    }

    pub fn get_node_count(&self) -> usize {
        self.node_count.load(Ordering::Relaxed)
    }
    pub fn hybrid_query_cache(&self) -> &QueryCache {
        &self.hybrid_query_cache
    }

    pub fn add_inter_toroidal_edge(
        &self,
        source_id: u64,
        _source_level: MatryoshkaDim,
        target_id: u64,
        _target_level: MatryoshkaDim,
        relation_type: String,
        _properties: serde_json::Value,
    ) -> Result<InterToroidalEdge> {
        self.add_edge(source_id, target_id, relation_type, 1.0)?;
        Ok(InterToroidalEdge {
            source: (ToroidalLevel::D384, source_id),
            target: (ToroidalLevel::D384, target_id),
            relation_type: String::new(),
            topological_distance: 0.0,
            homotopy_class: HomotopyClass::Direct,
            properties: serde_json::json!({}),
        })
    }

    pub fn get_inter_toroidal_edges(&self, node_id: u64) -> Result<Vec<InterToroidalEdge>> {
        Ok(self
            .get(node_id)?
            .map(|n| {
                n.edges
                    .into_iter()
                    .map(|e| InterToroidalEdge {
                        source: (ToroidalLevel::D384, node_id),
                        target: (ToroidalLevel::D384, e.target_id),
                        relation_type: e.relation_type,
                        topological_distance: e.weight,
                        homotopy_class: HomotopyClass::Direct,
                        properties: serde_json::json!({}),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}
