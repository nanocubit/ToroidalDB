use crate::math::{pad_or_truncate, toroidal_distance, MatryoshkaDim};
use crate::topology::edges::{InterToroidalEdge, ToroidalLevel};
use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json;
use sled::{Db, Tree};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, RwLock};

// Добавляем кэш для результатов запросов
#[derive(Clone)]
pub struct QueryCache {
    cache: Arc<RwLock<HashMap<u64, CachedResult>>>,
    max_size: usize,
    ttl_seconds: u64, // Время жизни кэша в секундах
}

#[derive(Clone)]
struct CachedResult {
    value: Vec<(u64, f32)>,
    timestamp: std::time::SystemTime,
}

impl QueryCache {
    pub fn new(max_size: usize, ttl_seconds: u64) -> Self {
        QueryCache {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            ttl_seconds,
        }
    }

    pub fn get(&self, key: u64) -> Option<Vec<(u64, f32)>> {
        let cache = self.cache.read().unwrap();
        if let Some(cached_result) = cache.get(&key) {
            // Проверяем TTL
            if cached_result
                .timestamp
                .elapsed()
                .unwrap_or_default()
                .as_secs()
                < self.ttl_seconds
            {
                Some(cached_result.value.clone())
            } else {
                // Результат устарел, удаляем из кэша
                drop(cache);
                self.remove(&key);
                None
            }
        } else {
            None
        }
    }

    pub fn put(&self, key: u64, value: Vec<(u64, f32)>) {
        let mut cache = self.cache.write().unwrap();

        // Проверяем размер кэша
        if cache.len() >= self.max_size {
            // Используем LRU стратегию - удаляем старейшие записи
            let now = std::time::SystemTime::now();
            let mut oldest_entries: Vec<(u64, std::time::SystemTime)> =
                cache.iter().map(|(k, v)| (*k, v.timestamp)).collect();

            // Сортируем по времени и удаляем старейшие
            oldest_entries.sort_by(|a, b| a.1.cmp(&b.1));
            for (key_to_remove, _) in oldest_entries.iter().take(self.max_size / 4) {
                cache.remove(key_to_remove);
            }
        }

        let cached_result = CachedResult {
            value,
            timestamp: std::time::SystemTime::now(),
        };

        cache.insert(key, cached_result);
    }

    pub fn remove(&self, key: &u64) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);
    }

    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    // Очистка устаревших записей
    pub fn cleanup_expired(&self) {
        let mut cache = self.cache.write().unwrap();
        let ttl_duration = std::time::Duration::from_secs(self.ttl_seconds);

        cache.retain(|_, cached_result| {
            cached_result.timestamp.elapsed().unwrap_or_default() < ttl_duration
        });
    }
}

// Добавляем кэш для гибридных запросов
#[derive(Clone)]
pub struct HybridQueryCache {
    cache: Arc<RwLock<HashMap<u64, CachedHybridResult>>>,
    max_size: usize,
    ttl_seconds: u64,
}

#[derive(Clone)]
struct CachedHybridResult {
    value: Vec<(u64, f32)>,
    timestamp: std::time::SystemTime,
}

impl HybridQueryCache {
    pub fn new(max_size: usize, ttl_seconds: u64) -> Self {
        HybridQueryCache {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            ttl_seconds,
        }
    }

    pub fn get(&self, key: u64) -> Option<Vec<(u64, f32)>> {
        let cache = self.cache.read().unwrap();
        if let Some(cached_result) = cache.get(&key) {
            if cached_result
                .timestamp
                .elapsed()
                .unwrap_or_default()
                .as_secs()
                < self.ttl_seconds
            {
                Some(cached_result.value.clone())
            } else {
                drop(cache);
                self.remove(&key);
                None
            }
        } else {
            None
        }
    }

    pub fn put(&self, key: u64, value: Vec<(u64, f32)>) {
        let mut cache = self.cache.write().unwrap();

        if cache.len() >= self.max_size {
            // LRU очистка
            let now = std::time::SystemTime::now();
            let mut oldest_entries: Vec<(u64, std::time::SystemTime)> =
                cache.iter().map(|(k, v)| (*k, v.timestamp)).collect();

            oldest_entries.sort_by(|a, b| a.1.cmp(&b.1));
            for (key_to_remove, _) in oldest_entries.iter().take(self.max_size / 4) {
                cache.remove(key_to_remove);
            }
        }

        let cached_result = CachedHybridResult {
            value,
            timestamp: std::time::SystemTime::now(),
        };

        cache.insert(key, cached_result);
    }

    pub fn remove(&self, key: &u64) {
        let mut cache = self.cache.write().unwrap();
        cache.remove(key);
    }

    pub fn cleanup_expired(&self) {
        let mut cache = self.cache.write().unwrap();
        let ttl_duration = std::time::Duration::from_secs(self.ttl_seconds);

        cache.retain(|_, cached_result| {
            cached_result.timestamp.elapsed().unwrap_or_default() < ttl_duration
        });
    }
}

// Хэшируем вектор и параметры поиска для использования в кэше
pub fn hash_query(vector: &[f32], dim: MatryoshkaDim, threshold: f32) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Хэшируем вектор (с округлением до 4 знаков после запятой для стабильности)
    for &val in vector {
        ((val * 10000.0).round() as i64).hash(&mut hasher);
    }
    (dim as i32).hash(&mut hasher);
    ((threshold * 10000.0).round() as i64).hash(&mut hasher);
    hasher.finish()
}

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
pub struct PersistentStore {
    pub db: Db,
    pub nodes_tree: Tree,
    pub query_cache: QueryCache,              // Кэш для обычных запросов
    pub hybrid_query_cache: HybridQueryCache, // Кэш для гибридных запросов
}

impl PersistentStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = sled::open(path)?;
        let nodes_tree = db.open_tree("nodes")?;

        Ok(PersistentStore {
            db,
            nodes_tree,
            query_cache: QueryCache::new(1000, 3600), // Кэш на 1000 запросов, TTL 1 час
            hybrid_query_cache: HybridQueryCache::new(500, 3600), // Кэш для гибридных запросов, TTL 1 час
        })
    }

    pub fn insert(&self, node: Node) -> Result<bool> {
        let key = node.id.to_be_bytes();
        let serialized = serde_json::to_vec(&node).context("Failed to serialize node")?;

        match self.nodes_tree.insert(&key, &*serialized)? {
            Some(_) => Ok(false), // Узел уже существовал
            None => {
                Ok(true) // Узел успешно вставлен
            }
        }
    }

    pub fn get(&self, id: u64) -> Result<Option<Node>> {
        let key = id.to_be_bytes();
        match self.nodes_tree.get(&key)? {
            Some(bytes) => {
                let node: Node =
                    serde_json::from_slice(&bytes).context("Failed to deserialize node")?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self.nodes_tree.len())
    }

    pub fn get_all(&self) -> Result<Vec<Node>> {
        let mut nodes = Vec::new();
        for item in self.nodes_tree.iter() {
            let (_, value) = item?;
            let node: Node =
                serde_json::from_slice(&value).context("Failed to deserialize node")?;
            nodes.push(node);
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
            None => return Ok(Vec::new()), // Узел не найден, возвращаем пустой список
        };

        let mut neighbors = Vec::new();
        for edge in node.edges {
            if let Some(neighbor) = self.get(edge.target_id)? {
                neighbors.push(neighbor);
            }
        }

        Ok(neighbors)
    }

    pub fn graph_search_bfs(&self, start_id: u64, max_depth: usize) -> Result<Vec<(u64, usize)>> {
        use std::collections::VecDeque;

        let mut visited = std::collections::HashSet::new();
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
                continue; // Не идем дальше максимальной глубины
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

    // Оптимизированная функция поиска с кэшированием и параллелизмом
    pub fn matryoshka_search(
        &self,
        query_vector: &[f32],
        dim: MatryoshkaDim,
        threshold: f32,
        limit: Option<usize>,
    ) -> Result<Vec<(u64, f32)>> {
        // Проверяем кэш
        let cache_key = hash_query(query_vector, dim, threshold);
        if let Some(cached_result) = self.query_cache.get(cache_key) {
            return Ok(cached_result);
        }

        // Подгоняем размер вектора под размерность
        let padded_query = pad_or_truncate(query_vector, dim.size());

        // Собираем все узлы для поиска
        let all_nodes: Vec<Node> = self.get_all()?;

        // Сохраняем длину all_nodes до перемещения
        let all_nodes_len = all_nodes.len();

        // Используем rayon для параллельного поиска
        let mut search_results: Vec<(u64, f32)> = all_nodes
            .into_par_iter()
            .filter_map(|node| {
                let padded_node_vector = pad_or_truncate(&node.vector, dim.size());
                let distance = toroidal_distance(&padded_query, &padded_node_vector);

                if distance <= threshold {
                    Some((node.id, distance))
                } else {
                    None
                }
            })
            .collect();

        // Сохраняем длину до сортировки и фильтрации
        let search_results_len = search_results.len();

        // Сортируем результаты по расстоянию
        search_results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Ограничиваем количество результатов
        let final_results: Vec<(u64, f32)> = search_results
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .collect();

        // Кэшируем результат (всё, без ограничения, чтобы кэш был полным)
        // Но не кэшируем ограниченные результаты, чтобы не путать кэш
        // Вместо этого кэшируем полный результат, если он не превышает лимит
        if search_results_len == all_nodes_len || limit.map_or(false, |l| search_results_len >= l) {
            // Если результаты не были ограничены (т.е. все узлы удовлетворяют условию), кэшируем
            // Но это может быть неэффективно, поэтому кэшируем только если результатов не слишком много
            if search_results_len < 10000 {
                // Порог для кэширования
                // Note: We can't use search_results here anymore since it's been moved
                // So we'll only cache if we're returning all results (no limit applied)
                if limit.is_none() || final_results.len() == search_results_len {
                    self.query_cache.put(cache_key, final_results.clone());
                }
            }
        }

        Ok(final_results)
    }

    // Добавляем меж-торовое ребро
    pub fn add_inter_toroidal_edge(
        &self,
        source_id: u64,
        source_level: MatryoshkaDim,
        target_id: u64,
        target_level: MatryoshkaDim,
        relation_type: String,
        properties: serde_json::Value,
    ) -> Result<InterToroidalEdge> {
        let source_node = self
            .get(source_id)?
            .ok_or_else(|| anyhow::anyhow!("Source node not found: {}", source_id))?;
        let target_node = self
            .get(target_id)?
            .ok_or_else(|| anyhow::anyhow!("Target node not found: {}", target_id))?;

        let edge = InterToroidalEdge::new(
            (ToroidalLevel::from_dim(source_level), source_id),
            (ToroidalLevel::from_dim(target_level), target_id),
            relation_type,
            &source_node.vector,
            &target_node.vector,
            properties,
        );

        // В реальной реализации нужно сохранить ребро в хранилище
        // Пока возвращаем созданный объект

        Ok(edge)
    }

    // Получаем меж-торовые ребра для узла
    pub fn get_inter_toroidal_edges(&self, node_id: u64) -> Result<Vec<InterToroidalEdge>> {
        // В реальной реализации нужно получить ребра из хранилища
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }
}
