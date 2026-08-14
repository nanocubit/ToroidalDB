use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub struct Cache<K, V> {
    map: RwLock<HashMap<K, Arc<CacheEntry<V>>>>,
    access_order: RwLock<VecDeque<K>>,
    max_size: usize,
    ttl: Duration,
    hits: RwLock<u64>,
    misses: RwLock<u64>,
}

struct CacheEntry<V> {
    value: V,
    created_at: Instant,
    access_count: usize,
    last_accessed: Instant,
}

impl<V> CacheEntry<V> {
    fn new(value: V) -> Self {
        let now = Instant::now();
        CacheEntry {
            value,
            created_at: now,
            access_count: 0,
            last_accessed: now,
        }
    }

    fn touch(&mut self) {
        self.access_count += 1;
        self.last_accessed = Instant::now();
    }
}

impl<K: Hash + Eq + Clone, V: Clone> Cache<K, V> {
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        Cache {
            map: RwLock::new(HashMap::new()),
            access_order: RwLock::new(VecDeque::new()),
            max_size,
            ttl,
            hits: RwLock::new(0),
            misses: RwLock::new(0),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let map = self.map.read().unwrap();

        if let Some(entry) = map.get(key) {
            if entry.created_at.elapsed() > self.ttl {
                drop(map);
                self.remove(key);
                *self.misses.write().unwrap() += 1;
                return None;
            }

            *self.hits.write().unwrap() += 1;
            return Some(entry.value.clone());
        }

        *self.misses.write().unwrap() += 1;
        None
    }

    pub fn put(&self, key: K, value: V) {
        let mut map = self.map.write().unwrap();

        if map.len() >= self.max_size {
            self.evict_lru(&mut map);
        }

        let entry = Arc::new(CacheEntry::new(value));
        map.insert(key.clone(), entry);

        let mut order = self.access_order.write().unwrap();
        if !order.contains(&key) {
            order.push_back(key);
        }
    }

    fn evict_lru(&self, map: &mut HashMap<K, Arc<CacheEntry<V>>>) {
        let mut order = self.access_order.write().unwrap();

        while let Some(key) = order.pop_front() {
            if map.remove(&key).is_some() {
                break;
            }
        }
    }

    fn remove(&self, key: &K) {
        let mut map = self.map.write().unwrap();
        map.remove(key);

        let mut order = self.access_order.write().unwrap();
        order.retain(|k| k != key);
    }

    pub fn invalidate(&self, key: &K) {
        self.remove(key);
    }

    pub fn clear(&self) {
        let mut map = self.map.write().unwrap();
        map.clear();

        let mut order = self.access_order.write().unwrap();
        order.clear();
    }

    pub fn size(&self) -> usize {
        self.map.read().unwrap().len()
    }

    pub fn hit_rate(&self) -> f64 {
        let hits = *self.hits.read().unwrap();
        let misses = *self.misses.read().unwrap();
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn hits(&self) -> u64 {
        *self.hits.read().unwrap()
    }

    pub fn misses(&self) -> u64 {
        *self.misses.read().unwrap()
    }
}

pub struct LruKCache<K, V> {
    cache: RwLock<HashMap<K, Arc<LruKEntry<V>>>>,
    access_list: RwLock<VecDeque<K>>,
    max_size: usize,
    k: usize,
    ttl: Duration,
    recent_accesses: RwLock<HashMap<K, Vec<Instant>>>,
    hits: RwLock<u64>,
    misses: RwLock<u64>,
}

struct LruKEntry<V> {
    value: V,
    created_at: Instant,
    k_history: VecDeque<Instant>,
}

impl<V> LruKEntry<V> {
    fn new(value: V) -> Self {
        LruKEntry {
            value,
            created_at: Instant::now(),
            k_history: VecDeque::new(),
        }
    }
}

impl<K: Hash + Eq + Clone, V: Clone> LruKCache<K, V> {
    pub fn new(max_size: usize, k: usize, ttl: Duration) -> Self {
        LruKCache {
            cache: RwLock::new(HashMap::new()),
            access_list: RwLock::new(VecDeque::new()),
            max_size,
            k,
            ttl,
            recent_accesses: RwLock::new(HashMap::new()),
            hits: RwLock::new(0),
            misses: RwLock::new(0),
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().unwrap();

        if let Some(entry) = cache.get(key) {
            if entry.created_at.elapsed() > self.ttl {
                drop(cache);
                self.remove(key);
                *self.misses.write().unwrap() += 1;
                return None;
            }

            let now = Instant::now();

            {
                let mut recent = self.recent_accesses.write().unwrap();
                let times = recent.entry(key.clone()).or_insert_with(Vec::new);
                times.push(now);

                if times.len() > self.k {
                    times.remove(0);
                }
            }

            {
                let mut order = self.access_list.write().unwrap();
                order.retain(|k| k != key);
                order.push_back(key.clone());
            }

            *self.hits.write().unwrap() += 1;
            return Some(entry.value.clone());
        }

        *self.misses.write().unwrap() += 1;
        None
    }

    pub fn put(&self, key: K, value: V) {
        let key_clone = key.clone();

        let mut cache = self.cache.write().unwrap();

        if cache.len() >= self.max_size {
            self.evict_lru_k(&mut cache);
        }

        let entry = Arc::new(LruKEntry::new(value));
        cache.insert(key_clone.clone(), entry);

        let mut order = self.access_list.write().unwrap();
        if !order.contains(&key_clone) {
            order.push_back(key_clone.clone());
        }

        let mut recent = self.recent_accesses.write().unwrap();
        recent.entry(key_clone).or_insert_with(Vec::new);
    }

    fn evict_lru_k(&self, cache: &mut HashMap<K, Arc<LruKEntry<V>>>) {
        let recent = self.recent_accesses.read().unwrap();

        let mut oldest_key: Option<K> = None;
        let mut oldest_time: Option<Instant> = None;

        for key in self.access_list.read().unwrap().iter() {
            if let Some(times) = recent.get(key) {
                if let Some(oldest) = times.first() {
                    if oldest_time.is_none() || *oldest < oldest_time.unwrap() {
                        oldest_time = Some(*oldest);
                        oldest_key = Some(key.clone());
                    }
                }
            }
        }

        if let Some(key) = oldest_key {
            cache.remove(&key);

            let mut order = self.access_list.write().unwrap();
            order.retain(|k| k != &key);

            let mut recent = self.recent_accesses.write().unwrap();
            recent.remove(&key);
        } else if let Some(key) = self.access_list.read().unwrap().front().cloned() {
            cache.remove(&key);

            let mut order = self.access_list.write().unwrap();
            order.pop_front();

            let mut recent = self.recent_accesses.write().unwrap();
            recent.remove(&key);
        }
    }

    fn access_order_write(&self) -> std::sync::RwLockWriteGuard<'_, VecDeque<K>> {
        self.access_list.write().unwrap()
    }

    fn remove(&self, key: &K) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);

        let mut order = self.access_list.write().unwrap();
        order.retain(|k| k != key);

        let mut recent = self.recent_accesses.write().unwrap();
        recent.remove(key);
    }

    pub fn invalidate(&self, key: &K) {
        self.remove(key);
    }

    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();

        let mut order = self.access_list.write().unwrap();
        order.clear();

        let mut recent = self.recent_accesses.write().unwrap();
        recent.clear();
    }

    pub fn size(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    pub fn hit_rate(&self) -> f64 {
        let hits = *self.hits.read().unwrap();
        let misses = *self.misses.read().unwrap();
        let total = hits + misses;

        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache_basic() {
        let cache = Cache::new(3, Duration::from_secs(60));

        cache.put("key1", "value1");
        assert_eq!(cache.get(&"key1"), Some("value1".to_string()));
        assert_eq!(cache.size(), 1);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = Cache::new(2, Duration::from_secs(60));

        cache.put("key1", "value1");
        cache.put("key2", "value2");
        cache.put("key3", "value3");

        assert_eq!(cache.get(&"key1"), None);
        assert_eq!(cache.get(&"key2"), Some("value2".to_string()));
        assert_eq!(cache.get(&"key3"), Some("value3".to_string()));
    }

    #[test]
    fn test_cache_ttl() {
        let cache = Cache::new(10, Duration::from_millis(50));

        cache.put("key1", "value1");
        assert_eq!(cache.get(&"key1"), Some("value1".to_string()));

        thread::sleep(Duration::from_millis(100));

        assert_eq!(cache.get(&"key1"), None);
    }

    #[test]
    fn test_cache_hit_rate() {
        let cache = Cache::new(10, Duration::from_secs(60));

        cache.put("key1", "value1");
        cache.get(&"key1");
        cache.get(&"key2");

        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert!((cache.hit_rate() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_lru_k_cache() {
        let cache = LruKCache::new(3, 2, Duration::from_secs(60));

        cache.put("key1", "value1");
        cache.get(&"key1");
        cache.get(&"key1");

        assert_eq!(cache.get(&"key1"), Some("value1".to_string()));
    }
}
