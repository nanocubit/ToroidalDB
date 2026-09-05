//! Block cache — optional read-optimisation layer for SegmentReader.
//!
//! Design decisions:
//!
//! 1. **Correctness-neutral** — cache never participates in domain logic.
//!    Checksum verification happens *before* insert; corruption on a
//!    cached read comes from the original segment file, not from the cache.
//!
//! 2. **Does not replace the pre-decoded BTreeMap path** — SegmentReader
//!    keeps its current hot path. The cache is consulted only when
//!    mode is `BlockCached` or when the segment is too large to pre-decode.
//!
//! 3. **Sharded LRU** — contention-free concurrent access via power-of-two
//!    shards, each holding an independent LruCache.  Defaults to 8 shards
//!    (one per hardware thread on a typical desktop).

use parking_lot::Mutex;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// BlockId
// ---------------------------------------------------------------------------

/// Opaque segment identifier, monotonically increasing and globally unique
/// within a store.
pub type SegmentId = u64;

/// Uniquely identifies a block inside a segment.
///
/// `offset` and `length` pinpoint the byte range in the segment file.
/// Two blocks from different segments with the same `(offset, length)` are
/// considered distinct (different `segment_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId {
    pub segment_id: SegmentId,
    pub offset: u64,
    pub length: u32,
}

impl BlockId {
    pub fn new(segment_id: SegmentId, offset: u64, length: u32) -> Self {
        Self {
            segment_id,
            offset,
            length,
        }
    }
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

/// Opaque block of segment data, verified by CRC on insert.
#[derive(Debug, Clone)]
pub struct Block {
    pub data: Vec<u8>,
}

impl Block {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

// ---------------------------------------------------------------------------
// CacheStats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub insertions: u64,
    pub evictions: u64,
    pub bytes_resident: u64,
    pub bytes_evicted: u64,
}

// ---------------------------------------------------------------------------
// BlockCache trait
// ---------------------------------------------------------------------------

/// Block cache contract.  Every operation is cheap (no I/O) and infallible.
pub trait BlockCache: Send + Sync {
    fn get(&self, id: BlockId) -> Option<Arc<Block>>;

    fn insert(&self, id: BlockId, block: Arc<Block>);

    /// Evict every block belonging to `segment_id`.  Called when a segment
    /// is removed from the manifest.
    fn invalidate_segment(&self, segment_id: SegmentId);

    fn stats(&self) -> CacheStats;

    fn hit_ratio(&self) -> f64 {
        let s = self.stats();
        let total = s.hits + s.misses;
        if total == 0 {
            0.0
        } else {
            s.hits as f64 / total as f64
        }
    }
}

// ---------------------------------------------------------------------------
// ShardedLruCache
// ---------------------------------------------------------------------------

/// A shard of the LRU cache — one `LruInner` plus atomic counters.
struct LruShard {
    inner: Mutex<LruInner>,
    hits: AtomicU64,
    misses: AtomicU64,
    insertions: AtomicU64,
    evictions: AtomicU64,
    bytes_evicted: AtomicU64,
}

struct LruInner {
    map: HashMap<BlockId, Arc<Block>>,
    order: VecDeque<BlockId>,
    capacity: u64, // bytes
    resident: u64, // bytes
}

impl LruShard {
    fn new(capacity: u64) -> Self {
        Self {
            inner: Mutex::new(LruInner {
                map: HashMap::new(),
                order: VecDeque::new(),
                capacity,
                resident: 0,
            }),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            insertions: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            bytes_evicted: AtomicU64::new(0),
        }
    }

    fn get(&self, id: BlockId) -> Option<Arc<Block>> {
        let mut inner = self.inner.lock();
        if inner.map.contains_key(&id) {
            let block = inner.map.get(&id).cloned();
            // Move to back (most-recently-used).
            if let Some(pos) = inner.order.iter().position(|x| *x == id) {
                inner.order.remove(pos);
                inner.order.push_back(id);
            }
            self.hits.fetch_add(1, Ordering::Relaxed);
            block
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    fn insert(&self, id: BlockId, block: Arc<Block>) {
        let block_len = block.data.len() as u64;
        let mut inner = self.inner.lock();

        // Evict until capacity is available.
        while inner.resident + block_len > inner.capacity && !inner.order.is_empty() {
            let evict_id = inner.order.pop_front().unwrap();
            if let Some(evicted) = inner.map.remove(&evict_id) {
                let evicted_len = evicted.data.len() as u64;
                inner.resident = inner.resident.saturating_sub(evicted_len);
                self.evictions.fetch_add(1, Ordering::Relaxed);
                self.bytes_evicted.fetch_add(evicted_len, Ordering::Relaxed);
            }
        }

        // If still no room (single block > capacity), do not insert.
        if inner.resident + block_len > inner.capacity {
            return;
        }

        inner.map.insert(id, block);
        inner.order.push_back(id);
        inner.resident += block_len;
        self.insertions.fetch_add(1, Ordering::Relaxed);
    }

    fn invalidate_segment(&self, segment_id: SegmentId) {
        let mut inner = self.inner.lock();
        inner.order.retain(|id| id.segment_id != segment_id);
        inner.map.retain(|id, _| id.segment_id != segment_id);
        // Recalculate resident from retained blocks.
        inner.resident = inner.map.values().map(|b| b.data.len() as u64).sum();
    }

    fn resident_bytes(&self) -> u64 {
        self.inner.lock().resident
    }
}

/// Power-of-two sharded LRU block cache.
///
/// Default shard count = 8.
pub struct Cache {
    shards: Vec<LruShard>,
    shard_bits: u32,
}

impl Cache {
    /// Create a new cache with `capacity_bytes` total capacity spread across
    /// `shard_count` shards (must be a power of two).
    pub fn new(capacity_bytes: u64, shard_count: u32) -> Self {
        assert!(
            shard_count.is_power_of_two(),
            "shard_count must be a power of two"
        );
        assert!(shard_count > 0);
        let shard_cap = capacity_bytes / shard_count as u64;
        let shards = (0..shard_count).map(|_| LruShard::new(shard_cap)).collect();
        Self {
            shards,
            shard_bits: shard_count.trailing_zeros(),
        }
    }

    fn shard(&self, id: BlockId) -> &LruShard {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut h);
        let hash = h.finish();
        if self.shard_bits == 0 {
            &self.shards[0]
        } else {
            let idx = (hash >> (64 - self.shard_bits)) as usize;
            &self.shards[idx % self.shards.len()]
        }
    }
}

impl BlockCache for Cache {
    fn get(&self, id: BlockId) -> Option<Arc<Block>> {
        self.shard(id).get(id)
    }

    fn insert(&self, id: BlockId, block: Arc<Block>) {
        self.shard(id).insert(id, block);
    }

    fn invalidate_segment(&self, segment_id: SegmentId) {
        for shard in &self.shards {
            shard.invalidate_segment(segment_id);
        }
    }

    fn stats(&self) -> CacheStats {
        let mut s = CacheStats::default();
        for shard in &self.shards {
            s.hits += shard.hits.load(Ordering::Relaxed);
            s.misses += shard.misses.load(Ordering::Relaxed);
            s.insertions += shard.insertions.load(Ordering::Relaxed);
            s.evictions += shard.evictions.load(Ordering::Relaxed);
            s.bytes_evicted += shard.bytes_evicted.load(Ordering::Relaxed);
        }
        s.bytes_resident = self.shards.iter().map(|s| s.resident_bytes()).sum();
        s
    }
}

// ---------------------------------------------------------------------------
// NoCache — pass-through implementation for tests
// ---------------------------------------------------------------------------

pub struct NoCache;

impl BlockCache for NoCache {
    fn get(&self, _: BlockId) -> Option<Arc<Block>> {
        None
    }

    fn insert(&self, _: BlockId, _: Arc<Block>) {}

    fn invalidate_segment(&self, _: SegmentId) {}

    fn stats(&self) -> CacheStats {
        CacheStats::default()
    }
}

// ---------------------------------------------------------------------------
// SegmentReadMode
// ---------------------------------------------------------------------------

/// How `SegmentReader` resolves block reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentReadMode {
    /// Fully pre-decode on open (current fast path). No cache consulted.
    Predecoded,
    /// Lazy block reads through `BlockCache`.  Uses `SegmentId` from the
    /// segment metadata.
    BlockCached,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn small_cache() -> Cache {
        Cache::new(4 * 1024, 2) // 4 KiB across 2 shards
    }

    fn block(data: &[u8]) -> Arc<Block> {
        Arc::new(Block::new(data.to_vec()))
    }

    fn bid(seg: SegmentId, off: u64, len: u32) -> BlockId {
        BlockId::new(seg, off, len)
    }

    #[test]
    fn miss_returns_none() {
        let c = small_cache();
        assert!(c.get(bid(0, 0, 64)).is_none());
    }

    #[test]
    fn insert_then_get() {
        let c = small_cache();
        let id = bid(1, 100, 64);
        c.insert(id, block(&[0; 64]));
        assert!(c.get(id).is_some());
    }

    #[test]
    fn repeated_get_is_hit() {
        let c = small_cache();
        let id = bid(1, 100, 64);
        c.insert(id, block(&[0; 64]));
        c.get(id);
        c.get(id);
        let s = c.stats();
        assert_eq!(s.hits, 2);
        assert_eq!(s.misses, 0);
    }

    #[test]
    fn capacity_evicts_old_blocks() {
        let c = Cache::new(192, 1); // single shard, 192 bytes
        let id1 = bid(1, 0, 100);
        let id2 = bid(1, 100, 100);
        let id3 = bid(1, 200, 100);

        c.insert(id1, block(&[0; 100]));
        c.insert(id2, block(&[0; 100]));
        // id1 should be evicted by now.
        assert!(c.get(id1).is_none());
        assert!(c.get(id2).is_some());

        c.insert(id3, block(&[0; 100]));
        // id2 was accessed, so id3 should evict id1 (already gone) or id2.
        // At this point at least one of the first two is gone.
        let s = c.stats();
        assert!(s.evictions >= 1);
    }

    #[test]
    fn lru_order() {
        let c = Cache::new(250, 1);
        let id_a = bid(1, 0, 100);
        let id_b = bid(1, 100, 100);
        let id_c = bid(1, 200, 100);

        c.insert(id_a, block(&[0; 100]));
        c.insert(id_b, block(&[0; 100]));
        c.get(id_a); // makes A most-recently-used
        c.insert(id_c, block(&[0; 100])); // should evict B (LRU)

        assert!(c.get(id_a).is_some(), "A was touched, should survive");
        assert!(c.get(id_b).is_none(), "B was LRU, should be evicted");
        assert!(c.get(id_c).is_some(), "C is newest, should survive");
    }

    #[test]
    fn invalidate_segment() {
        let c = small_cache();
        c.insert(bid(1, 0, 64), block(&[0; 64]));
        c.insert(bid(2, 0, 64), block(&[0; 64]));
        c.invalidate_segment(1);
        assert!(c.get(bid(1, 0, 64)).is_none());
        assert!(c.get(bid(2, 0, 64)).is_some());
    }

    #[test]
    fn stats_are_accurate() {
        let c = small_cache();
        let id = bid(1, 0, 64);
        c.get(id); // miss
        c.insert(id, block(&[0; 64]));
        c.get(id); // hit
        let s = c.stats();
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 1);
        assert_eq!(s.insertions, 1);
    }

    #[test]
    fn no_cache_always_misses() {
        let nc = NoCache;
        assert!(nc.get(bid(0, 0, 64)).is_none());
        nc.insert(bid(0, 0, 64), block(&[0; 64]));
        assert!(nc.get(bid(0, 0, 64)).is_none());
    }

    #[test]
    fn single_block_over_capacity_not_inserted() {
        let c = Cache::new(50, 1);
        let id = bid(1, 0, 100);
        c.insert(id, block(&[0; 100]));
        assert!(c.get(id).is_none());
    }

    #[test]
    fn concurrent_get_no_panic() {
        use std::sync::Arc;
        use std::thread;

        let c = Arc::new(Cache::new(1_000_000, 4));
        let mut handles = Vec::new();
        for t in 0..8 {
            let c = c.clone();
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    let id = BlockId::new(t, i as u64 * 64, 64);
                    let _ = c.get(id);
                    c.insert(id, block(&[i as u8; 64]));
                    let _ = c.get(id);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }
}
