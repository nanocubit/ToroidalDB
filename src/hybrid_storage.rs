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
    Arc,
};
use std::time::SystemTime;
use toroidal_store::ToroidalStore;

// ---------------------------------------------------------------------------
// Backend-neutral storage contract (previously toroidal-storage crate)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchOp {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

pub trait Storage: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
    fn batch(&self, operations: &[BatchOp]) -> Result<()>;
    fn scan(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>>;
    fn flush(&self) -> Result<()>;
    fn checkpoint(&self) -> Result<()> {
        self.flush()
    }
    fn compact(&self) -> Result<()> {
        Ok(())
    }
    fn snapshot(&self) -> Result<Box<dyn Snapshot>>;
}

pub trait Snapshot: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn scan(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>>;
}

pub trait StorageFactory: Send + Sync {
    fn open(&self, path: &Path) -> Result<Box<dyn Storage>>;
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// ToroidalStore backend
// ---------------------------------------------------------------------------

pub struct ToroidalBackend {
    store: Arc<ToroidalStore>,
}

impl ToroidalBackend {
    pub fn open(path: &Path) -> Result<Self> {
        let store =
            ToroidalStore::open(path).map_err(|e| anyhow::anyhow!("ToroidalStore open: {e}"))?;
        Ok(Self {
            store: Arc::new(store),
        })
    }
}

impl Storage for ToroidalBackend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.store.get(key))
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.store
            .put(key.to_vec(), value.to_vec())
            .map_err(|e| anyhow::anyhow!("put: {e}"))
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.store
            .delete(key)
            .map_err(|e| anyhow::anyhow!("delete: {e}"))
    }

    fn batch(&self, operations: &[BatchOp]) -> Result<()> {
        let ops: Vec<toroidal_store::WalFrameKind> = operations
            .iter()
            .map(|op| match op {
                BatchOp::Put { key, value } => toroidal_store::WalFrameKind::Put {
                    key: key.clone(),
                    value: value.clone(),
                },
                BatchOp::Delete { key } => {
                    toroidal_store::WalFrameKind::Delete { key: key.clone() }
                }
            })
            .collect();
        self.store
            .batch(&ops)
            .map_err(|e| anyhow::anyhow!("batch: {e}"))
    }

    fn scan(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>> {
        if let (Some(s), Some(e)) = (start, end) {
            if s > e {
                return Err(anyhow::anyhow!("invalid scan range"));
            }
        }
        let items = self.store.scan(start, end);
        Ok(Box::new(items.into_iter().map(Ok)))
    }

    fn flush(&self) -> Result<()> {
        self.store
            .checkpoint()
            .map_err(|e| anyhow::anyhow!("flush: {e}"))?;
        Ok(())
    }

    fn checkpoint(&self) -> Result<()> {
        self.store
            .checkpoint()
            .map_err(|e| anyhow::anyhow!("checkpoint: {e}"))?;
        Ok(())
    }

    fn compact(&self) -> Result<()> {
        self.store
            .compact()
            .map_err(|e| anyhow::anyhow!("compact: {e}"))?;
        Ok(())
    }

    fn snapshot(&self) -> Result<Box<dyn Snapshot>> {
        let snap = self.store.snapshot();
        Ok(Box::new(ToroidalSnapshot {
            store: self.store.clone(),
            snapshot: snap,
        }))
    }
}

pub struct ToroidalSnapshot {
    store: Arc<ToroidalStore>,
    snapshot: toroidal_store::Snapshot,
}

impl Snapshot for ToroidalSnapshot {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.store
            .get_at(&self.snapshot, key)
            .map_err(|e| anyhow::anyhow!("snapshot get: {e}"))
    }

    fn scan(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>> {
        if let (Some(s), Some(e)) = (start, end) {
            if s > e {
                return Err(anyhow::anyhow!("invalid scan range"));
            }
        }
        let items = self
            .store
            .scan_at(&self.snapshot, start, end)
            .map_err(|e| anyhow::anyhow!("snapshot scan: {e}"))?;
        Ok(Box::new(items.into_iter().map(Ok)))
    }
}

pub struct ToroidalStoreFactory;

impl StorageFactory for ToroidalStoreFactory {
    fn open(&self, path: &Path) -> Result<Box<dyn Storage>> {
        Ok(Box::new(ToroidalBackend::open(path)?))
    }

    fn name(&self) -> &'static str {
        "toroidal-store"
    }
}

// ---------------------------------------------------------------------------
// In-memory backend (fallback for tests)
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::sync::Mutex;

struct MemInner {
    data: BTreeMap<Vec<u8>, Vec<u8>>,
}

pub struct MemoryBackend {
    inner: Arc<Mutex<MemInner>>,
}

impl MemoryBackend {
    pub fn open() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemInner {
                data: BTreeMap::new(),
            })),
        }
    }
}

impl Storage for MemoryBackend {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.inner.lock().unwrap().data.get(key).cloned())
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .data
            .insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.inner.lock().unwrap().data.remove(key);
        Ok(())
    }

    fn batch(&self, operations: &[BatchOp]) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        for op in operations {
            match op {
                BatchOp::Put { key, value } => {
                    inner.data.insert(key.clone(), value.clone());
                }
                BatchOp::Delete { key } => {
                    inner.data.remove(key);
                }
            }
        }
        Ok(())
    }

    fn scan(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>> {
        let inner = self.inner.lock().unwrap();
        let items: Vec<_> = match (start, end) {
            (Some(s), Some(e)) => inner
                .data
                .range(s.to_vec()..e.to_vec())
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
            (Some(s), None) => inner
                .data
                .range(s.to_vec()..)
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
            (None, Some(e)) => inner
                .data
                .range(..e.to_vec())
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
            (None, None) => inner
                .data
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
        };
        Ok(Box::new(items.into_iter()))
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }

    fn snapshot(&self) -> Result<Box<dyn Snapshot>> {
        let inner = self.inner.lock().unwrap();
        let data = inner.data.clone();
        Ok(Box::new(MemSnapshot { data }))
    }
}

struct MemSnapshot {
    data: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl Snapshot for MemSnapshot {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        Ok(self.data.get(key).cloned())
    }

    fn scan(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>> {
        let items: Vec<_> = match (start, end) {
            (Some(s), Some(e)) => self
                .data
                .range(s.to_vec()..e.to_vec())
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
            (Some(s), None) => self
                .data
                .range(s.to_vec()..)
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
            (None, Some(e)) => self
                .data
                .range(..e.to_vec())
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
            (None, None) => self
                .data
                .iter()
                .map(|(k, v)| Ok((k.clone(), v.clone())))
                .collect(),
        };
        Ok(Box::new(items.into_iter()))
    }
}

pub struct MemoryFactory;

impl StorageFactory for MemoryFactory {
    fn open(&self, _path: &Path) -> Result<Box<dyn Storage>> {
        Ok(Box::new(MemoryBackend::open()))
    }

    fn name(&self) -> &'static str {
        "memory"
    }
}

// ---------------------------------------------------------------------------
// HybridPersistentStore (unchanged public API)
// ---------------------------------------------------------------------------

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
        let factory = ToroidalStoreFactory;
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
            None => return Err(anyhow::anyhow!("Source node {from_id} not found")),
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
        let _executor = rayon::ThreadPoolBuilder::new().build().unwrap();
        let results: Vec<Result<Node>> = node
            .edges
            .par_iter()
            .map(|e| self.get(e.target_id))
            .filter_map(std::result::Result::transpose)
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
        "toroidal-store"
    }

    /// Returns the canonical backend name for diagnostics.
    pub fn backend_name(&self) -> &'static str {
        "toroidal"
    }

    /// Durability checkpoint: freeze then flush all buffered writes.
    /// After this call, all acknowledged writes survive process termination.
    pub fn flush(&self) -> Result<()> {
        self.storage.checkpoint().context("flush failed")
    }

    /// Trigger space reclamation.  On the `ToroidalStore` backend this merges
    /// all segments into one, applying safe retention horizon.
    pub fn compact(&self) -> Result<()> {
        self.storage.compact().context("compact failed")
    }

    /// Returns a point-in-time snapshot of the storage backend.
    /// Use `get_at` for consistent reads across multiple keys.
    pub fn snapshot(&self) -> Result<Box<dyn Snapshot>> {
        self.storage.snapshot().context("snapshot failed")
    }

    /// Read a node at a specific snapshot.  Returns `None` if the node
    /// did not exist at that point in time, without visible cache effects.
    pub fn get_at(&self, snap: &dyn Snapshot, id: u64) -> Result<Option<Node>> {
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
