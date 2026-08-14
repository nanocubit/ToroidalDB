use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::ast::Query;
use crate::tql::executor::{QueryExecutor, QueryResult};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio;

pub mod two_phase_commit;
pub use two_phase_commit::TwoPhaseCommit;

pub struct QueryCoordinator {
    pub shard_count: u32,
    pub hash_ring: BTreeMap<u64, u32>, // Consistent hashing ring
    pub shards: Vec<Arc<HybridPersistentStore>>, // Local shards for testing
}

impl QueryCoordinator {
    pub fn new(shard_count: u32, shards: Vec<Arc<HybridPersistentStore>>) -> Self {
        let hash_ring = BTreeMap::new();
        let mut coordinator = QueryCoordinator {
            shard_count,
            hash_ring,
            shards,
        };

        // Инициализируем кольцо хеширования
        coordinator.initialize_hash_ring();

        coordinator
    }

    fn initialize_hash_ring(&mut self) {
        // Простая реализация: равномерно распределяем виртуальные узлы по кольцу
        for shard_id in 0..self.shard_count {
            // Добавляем несколько виртуальных узлов для каждого шарда для лучшего распределения
            for replica in 0..3 {
                let hash = Self::hash_key(&format!("shard_{}_replica_{}", shard_id, replica));
                self.hash_ring.insert(hash, shard_id);
            }
        }
    }

    fn hash_key(key: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }

    pub fn route_query(&self, query_hash: u64, radius: u32) -> Vec<u32> {
        // Возвращаем `radius` ближайших шардов на хэш-кольце
        let mut shard_distances = Vec::new();

        for (&hash, &shard_id) in &self.hash_ring {
            let distance = if query_hash <= hash {
                hash - query_hash
            } else {
                u64::MAX - query_hash + hash
            };
            shard_distances.push((distance, shard_id));
        }

        // Сортируем по расстоянию и берем первые radius шардов
        shard_distances.sort_by_key(|(dist, _)| *dist);

        let mut result = Vec::new();
        for (_, shard_id) in shard_distances.into_iter().take(radius as usize) {
            if !result.contains(&shard_id) {
                result.push(shard_id);
            }
        }

        result
    }

    pub fn route_node(&self, node_id: u64) -> u32 {
        // Определяем, на каком шарде должен находиться узел
        let key = format!("node_{}", node_id);
        let hash = Self::hash_key(&key);

        // Находим ближайший шард в кольце
        match self.hash_ring.range(hash..).next() {
            Some((_, &shard_id)) => shard_id,
            None => {
                // Если не нашли, начинаем с начала кольца
                if let Some((_, &shard_id)) = self.hash_ring.iter().next() {
                    shard_id
                } else {
                    0 // По умолчанию шард 0
                }
            }
        }
    }
}

pub struct DistributedExecutor {
    pub coordinator: Arc<QueryCoordinator>,
}

impl DistributedExecutor {
    pub fn new(coordinator: Arc<QueryCoordinator>) -> Self {
        DistributedExecutor { coordinator }
    }

    pub async fn execute_distributed(&self, query: Query) -> Result<Vec<QueryResult>, String> {
        // 1. Определяем целевые шарды
        let query_hash = Self::hash_query(&query);
        let shards = self.coordinator.route_query(query_hash, 5); // используем 5 ближайших шардов

        // 2. Scatter: параллельный поиск по шардам
        let mut partial_results = Vec::new();

        for shard_id in shards {
            if (shard_id as usize) < self.coordinator.shards.len() {
                let shard = self.coordinator.shards[shard_id as usize].clone();
                let query_clone = query.clone();

                // Выполняем запрос на конкретном шарде
                match QueryExecutor::execute_query(&shard, query_clone).await {
                    Ok(results) => partial_results.push(results),
                    Err(e) => {
                        return Err(format!(
                            "Error executing query on shard {}: {}",
                            shard_id, e
                        ))
                    }
                }
            }
        }

        // 3. Gather: глобальное слияние результатов (топ-K куча)
        let global_results = self.merge_top_k(partial_results, query.limit);

        Ok(global_results)
    }

    fn merge_top_k(&self, partial_results: Vec<Vec<QueryResult>>, limit: u32) -> Vec<QueryResult> {
        // Объединяем все результаты
        let mut all_results = Vec::new();
        for results in partial_results {
            all_results.extend(results);
        }

        // Сортируем по оценке (чем меньше score, тем лучше)
        all_results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());

        // Возвращаем только top-K результатов
        all_results.into_iter().take(limit as usize).collect()
    }

    fn hash_query(query: &Query) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        if let Some(ref match_clause) = query.match_clause {
            format!("{:?}", match_clause).hash(&mut hasher);
        }
        if let Some(ref where_clause) = query.where_clause {
            format!("{:?}", where_clause).hash(&mut hasher);
        }
        query.limit.hash(&mut hasher);
        query.distributed.hash(&mut hasher);

        hasher.finish()
    }

    pub async fn execute_transaction_distributed(
        &self,
        transaction: crate::tql::ast::Transaction,
    ) -> Result<(), String> {
        // В распределенной транзакции нужно координировать выполнение на всех шардах
        // Это упрощенная реализация - в реальной системе потребуется двухфазный коммит или другой протокол

        for shard in &self.coordinator.shards {
            // Выполняем транзакцию на каждом шарде
            // В реальной системе нужно координировать транзакции между шардами
            match crate::tql::QueryExecutor::execute_transaction(shard, transaction.clone()).await {
                Ok(_) => continue,
                Err(e) => return Err(format!("Transaction failed on shard: {}", e)),
            }
        }

        Ok(())
    }
}
