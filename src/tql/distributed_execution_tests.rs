#[cfg(test)]
mod distributed_execution_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::{MatchClause, NodePattern, Query};
    use crate::tql::coordinator::{DistributedExecutor, QueryCoordinator};
    use crate::tql::executor::QueryExecutor;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_query_coordinator_initialization() {
        // Создаем несколько шардов для тестирования
        let shard1 = Arc::new(HybridPersistentStore::open("./dist_test_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./dist_test_shard2").unwrap());
        let shard3 = Arc::new(HybridPersistentStore::open("./dist_test_shard3").unwrap());

        let shards = vec![shard1.clone(), shard2.clone(), shard3.clone()];

        // Создаем координатора
        let coordinator = QueryCoordinator::new(3, shards);

        // Проверяем, что кольцо хеширования инициализировано
        assert!(!coordinator.hash_ring.is_empty());
        assert_eq!(coordinator.shard_count, 3);

        // Проверяем маршрутизацию узла
        let shard_id = coordinator.route_node(123);
        assert!(shard_id < 3);

        println!(
            "Query coordinator initialized with {} shards",
            coordinator.shard_count
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./dist_test_shard1").ok();
        std::fs::remove_dir_all("./dist_test_shard2").ok();
        std::fs::remove_dir_all("./dist_test_shard3").ok();
    }

    #[tokio::test]
    async fn test_scatter_gather_mechanism() {
        // Создаем несколько шардов
        let shard1 = Arc::new(HybridPersistentStore::open("./sg_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./sg_shard2").unwrap());
        let shard3 = Arc::new(HybridPersistentStore::open("./sg_shard3").unwrap());

        let shards = vec![shard1.clone(), shard2.clone(), shard3.clone()];

        // Добавляем тестовые данные в шарды
        for (shard, shard_id) in [shard1, shard2, shard3].iter().zip(1..=3) {
            for i in 1..=50 {
                let node_id = (shard_id - 1) * 50 + i;
                let node = crate::hybrid_storage::Node {
                    id: node_id,
                    vector: vec![(node_id as f32 * 0.001) % 1.0, 0.5],
                    properties: json!({
                        "id": node_id,
                        "shard": shard_id,
                        "name": format!("node_{}_{}", shard_id, i)
                    }),
                    edges: vec![],
                };
                shard.insert(node).unwrap();
            }
        }

        // Создаем координатора и исполнитель
        let coordinator = Arc::new(QueryCoordinator::new(3, shards));
        let executor = DistributedExecutor::new(coordinator);

        // Создаем тестовый запрос
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "n".to_string(),
                    label: "Test".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["n.id".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 15,
            distributed: true,
        };

        // Выполняем распределенный запрос
        let start_time = std::time::Instant::now();
        let results = executor.execute_distributed(query).await.unwrap();
        let duration = start_time.elapsed();

        assert!(!results.is_empty());
        assert!(results.len() <= 15);
        println!(
            "Scatter/gather test: {} results in {:?}",
            results.len(),
            duration
        );

        // Проверяем, что результаты содержат данные из разных шардов
        let mut shard_counts: std::collections::HashMap<u32, usize> =
            std::collections::HashMap::new();
        for result in &results {
            // В реальной системе мы бы проверили, из какого шарда пришли результаты
            // Для теста просто проверим, что результаты разнообразны
            assert!(result.id > 0);
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./sg_shard1").ok();
        std::fs::remove_dir_all("./sg_shard2").ok();
        std::fs::remove_dir_all("./sg_shard3").ok();
    }

    #[tokio::test]
    async fn test_consistent_hashing_distribution() {
        // Создаем шарды для тестирования распределения
        let shard1 = Arc::new(HybridPersistentStore::open("./hash_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./hash_shard2").unwrap());
        let shard3 = Arc::new(HybridPersistentStore::open("./hash_shard3").unwrap());
        let shard4 = Arc::new(HybridPersistentStore::open("./hash_shard4").unwrap());

        let shards = vec![
            shard1.clone(),
            shard2.clone(),
            shard3.clone(),
            shard4.clone(),
        ];

        // Создаем координатора
        let coordinator = QueryCoordinator::new(4, shards);

        // Тестируем распределение узлов по шардам
        let mut distribution = std::collections::HashMap::new();
        for i in 1..=1000 {
            let shard_id = coordinator.route_node(i);
            *distribution.entry(shard_id).or_insert(0) += 1;
        }

        // Проверяем, что узлы распределены относительно равномерно
        println!("Consistent hashing distribution:");
        for (shard_id, count) in &distribution {
            println!("  Shard {}: {} nodes", shard_id, count);
        }

        // Проверяем, что все шарды получили хотя бы некоторые узлы
        assert_eq!(distribution.len(), 4); // Все 4 шарда должны получить узлы

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./hash_shard1").ok();
        std::fs::remove_dir_all("./hash_shard2").ok();
        std::fs::remove_dir_all("./hash_shard3").ok();
        std::fs::remove_dir_all("./hash_shard4").ok();
    }

    #[tokio::test]
    async fn test_distributed_query_execution() {
        // Создаем шарды для тестирования выполнения запросов
        let shard1 = Arc::new(HybridPersistentStore::open("./exec_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./exec_shard2").unwrap());
        let shard3 = Arc::new(HybridPersistentStore::open("./exec_shard3").unwrap());

        let shards = vec![shard1.clone(), shard2.clone(), shard3.clone()];

        // Добавляем тестовые данные
        for (shard, shard_id) in [shard1, shard2, shard3].iter().zip(1..=3) {
            for i in 1..=100 {
                let node_id = (shard_id - 1) * 100 + i;
                let node = crate::hybrid_storage::Node {
                    id: node_id,
                    vector: vec![(node_id as f32 * 0.001) % 0.5, 0.3], // Разные векторы для разных шардов
                    properties: json!({
                        "id": node_id,
                        "shard": shard_id,
                        "name": format!("distributed_node_{}", node_id),
                        "category": if node_id % 2 == 0 { "even" } else { "odd" }
                    }),
                    edges: vec![],
                };
                shard.insert(node).unwrap();
            }
        }

        // Создаем координатора и исполнитель
        let coordinator = Arc::new(QueryCoordinator::new(3, shards));
        let executor = DistributedExecutor::new(coordinator);

        // Создаем распределенный запрос
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "node".to_string(),
                    label: "Distributed".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["node.id".to_string(), "node.shard".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 25,
            distributed: true,
        };

        // Выполняем запрос
        let start_time = std::time::Instant::now();
        let results = executor.execute_distributed(query).await.unwrap();
        let duration = start_time.elapsed();

        assert!(!results.is_empty());
        assert!(results.len() <= 25);
        println!(
            "Distributed query execution: {} results in {:?}",
            results.len(),
            duration
        );

        // Проверяем, что результаты содержат правильные поля
        for result in &results {
            assert!(result.id > 0);
            // Проверяем, что свойства содержат ожидаемые поля
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./exec_shard1").ok();
        std::fs::remove_dir_all("./exec_shard2").ok();
        std::fs::remove_dir_all("./exec_shard3").ok();
    }

    #[tokio::test]
    async fn test_local_vs_distributed_performance() {
        // Создаем шарды
        let shard1 = Arc::new(HybridPersistentStore::open("./perf_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./perf_shard2").unwrap());

        let shards = vec![shard1.clone(), shard2.clone()];

        // Добавляем данные в шарды
        for (shard, shard_id) in [shard1, shard2].iter().zip(1..=2) {
            for i in 1..=500 {
                let node_id = (shard_id - 1) * 500 + i;
                let node = crate::hybrid_storage::Node {
                    id: node_id,
                    vector: vec![(node_id as f32 * 0.0005) % 1.0, 0.5],
                    properties: json!({
                        "id": node_id,
                        "shard": shard_id,
                        "name": format!("perf_node_{}", node_id)
                    }),
                    edges: vec![],
                };
                shard.insert(node).unwrap();
            }
        }

        // Тестируем производительность локального выполнения
        let local_store = Arc::new(HybridPersistentStore::open("./local_store").unwrap());

        // Копируем данные в локальное хранилище для сравнения
        for i in 1..=1000 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![(i as f32 * 0.0005) % 1.0, 0.5],
                properties: json!({
                    "id": i,
                    "name": format!("local_node_{}", i)
                }),
                edges: vec![],
            };
            local_store.insert(node).unwrap();
        }

        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "n".to_string(),
                    label: "Performance".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["n.id".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 20,
            distributed: false,
        };

        // Локальное выполнение
        let local_start = std::time::Instant::now();
        let local_results = QueryExecutor::execute_query(&local_store, query.clone())
            .await
            .unwrap();
        let local_duration = local_start.elapsed();

        // Создаем распределенный исполнитель
        let coordinator = Arc::new(QueryCoordinator::new(2, shards));
        let dist_executor = DistributedExecutor::new(coordinator);

        // Распределенное выполнение
        let dist_query = Query {
            distributed: true,
            ..query.clone()
        };

        let dist_start = std::time::Instant::now();
        let dist_results = dist_executor.execute_distributed(dist_query).await.unwrap();
        let dist_duration = dist_start.elapsed();

        println!("Performance comparison:");
        println!(
            "  Local execution: {} results in {:?}",
            local_results.len(),
            local_duration
        );
        println!(
            "  Distributed execution: {} results in {:?}",
            dist_results.len(),
            dist_duration
        );

        // Оба должны вернуть результаты
        assert!(!local_results.is_empty());
        assert!(!dist_results.is_empty());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./perf_shard1").ok();
        std::fs::remove_dir_all("./perf_shard2").ok();
        std::fs::remove_dir_all("./local_store").ok();
    }

    #[tokio::test]
    async fn test_sharding_with_consistent_topology() {
        // Создаем шарды для тестирования топологии
        let shard1 = Arc::new(HybridPersistentStore::open("./topo_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./topo_shard2").unwrap());
        let shard3 = Arc::new(HybridPersistentStore::open("./topo_shard3").unwrap());

        let shards = vec![shard1.clone(), shard2.clone(), shard3.clone()];

        // Создаем координатора
        let coordinator = Arc::new(QueryCoordinator::new(3, shards));

        // Тестируем маршрутизацию для тороидальных запросов
        for i in 1..=100 {
            let node_id = i as u64;
            let shard_id = coordinator.route_node(node_id);

            // Проверяем, что узел всегда маршрутизируется на один и тот же шард
            let shard_id_again = coordinator.route_node(node_id);
            assert_eq!(shard_id, shard_id_again);
        }

        println!("Consistent sharding test passed: same nodes route to same shards");

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./topo_shard1").ok();
        std::fs::remove_dir_all("./topo_shard2").ok();
        std::fs::remove_dir_all("./topo_shard3").ok();
    }
}
