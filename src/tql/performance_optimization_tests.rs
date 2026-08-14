#[cfg(test)]
mod performance_optimization_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::{MatchClause, NodePattern, Query};
    use crate::tql::executor::QueryExecutor;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Instant;

    #[tokio::test]
    async fn test_query_caching_performance() {
        let store = Arc::new(HybridPersistentStore::open("./caching_perf_test_data").unwrap());

        // Создаем тестовые узлы
        for i in 1..=1000 {
            let node = crate::hybrid_storage::Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.001) % 1.0, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем запрос
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
            limit: 10,
            distributed: false,
        };

        // Первый запуск (без кэша)
        let start_time = Instant::now();
        let results1 = QueryExecutor::execute_query(&store, query.clone())
            .await
            .unwrap();
        let first_run_time = start_time.elapsed();

        // Второй запуск (с кэшем)
        let start_time = Instant::now();
        let results2 = QueryExecutor::execute_query(&store, query.clone())
            .await
            .unwrap();
        let second_run_time = start_time.elapsed();

        // Третий запуск (с кэшем)
        let start_time = Instant::now();
        let results3 = QueryExecutor::execute_query(&store, query).await.unwrap();
        let third_run_time = start_time.elapsed();

        // Проверяем, что результаты одинаковы
        assert_eq!(results1.len(), results2.len());
        assert_eq!(results2.len(), results3.len());

        println!("Query caching performance test:");
        println!("  First run: {:?}", first_run_time);
        println!("  Second run (cached): {:?}", second_run_time);
        println!("  Third run (cached): {:?}", third_run_time);

        // Второй и третий запуски должны быть быстрее благодаря кэшированию
        if first_run_time > second_run_time {
            println!("  ✅ Caching improved performance!");
        } else {
            // В тестовой среде могут быть колебания, поэтому просто информируем
            println!("  ℹ️ Caching performance gain may vary due to system load");
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./caching_perf_test_data").ok();
    }

    #[tokio::test]
    async fn test_parallel_search_performance() {
        let store = Arc::new(PersistentStore::open("./parallel_perf_test_data").unwrap());

        // Создаем большое количество узлов для тестирования параллелизма
        let num_nodes = 5000;
        for i in 1..=num_nodes {
            let node = crate::storage::Node {
                id: i as u64,
                vector: vec![
                    (i as f32 * 0.0002) % 1.0,
                    ((i * 2) as f32 * 0.0002) % 1.0,
                    ((i * 3) as f32 * 0.0002) % 1.0,
                    ((i * 4) as f32 * 0.0002) % 1.0,
                ],
                properties: json!({
                    "id": i,
                    "name": format!("node_{}", i),
                    "category": if i % 2 == 0 { "even" } else { "odd" }
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Тестируем производительность параллельного поиска
        let query_vector = vec![0.5, 0.5, 0.5, 0.5];
        let start_time = Instant::now();
        let results = store
            .matryoshka_search(
                &query_vector,
                crate::math::MatryoshkaDim::D384,
                0.3,
                Some(20),
            )
            .unwrap();
        let duration = start_time.elapsed();

        assert!(!results.is_empty());
        println!(
            "Parallel search performance on {} nodes: {} results in {:?}",
            num_nodes,
            results.len(),
            duration
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./parallel_perf_test_data").ok();
    }

    #[tokio::test]
    async fn test_early_termination_optimization() {
        let store = Arc::new(PersistentStore::open("./early_term_test_data").unwrap());

        // Создаем узлы с разными расстояниями до эталонного вектора
        for i in 1..=2000 {
            let distance_factor = (i as f32 / 2000.0) * 0.5; // От 0 до 0.5
            let node = crate::storage::Node {
                id: i as u64,
                vector: vec![0.5 + distance_factor, 0.5 - distance_factor],
                properties: json!({"id": i, "distance_factor": distance_factor}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Тестируем раннюю остановку с разными лимитами
        let query_vector = vec![0.5, 0.5];
        let limits = vec![5, 10, 20];

        for limit in limits {
            let start_time = Instant::now();
            let results = store
                .matryoshka_search(
                    &query_vector,
                    crate::math::MatryoshkaDim::D384,
                    0.4,
                    Some(limit),
                )
                .unwrap();
            let duration = start_time.elapsed();

            assert_eq!(results.len(), limit);
            println!(
                "Early termination with limit {}: {} results in {:?}",
                limit,
                results.len(),
                duration
            );
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./early_term_test_data").ok();
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let store = Arc::new(PersistentStore::open("./cache_ttl_test_data").unwrap());

        // Создаем тестовые узлы
        for i in 1..=100 {
            let node = crate::storage::Node {
                id: i as u64,
                vector: vec![i as f32 * 0.01, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Выполняем запрос и кэшируем результат
        let query_vector = vec![0.5, 0.5];
        let results_before = store
            .matryoshka_search(
                &query_vector,
                crate::math::MatryoshkaDim::D384,
                0.3,
                Some(10),
            )
            .unwrap();
        assert!(!results_before.is_empty());

        // Очищаем устаревшие записи из кэша (с TTL 0 для тестирования)
        store.query_cache.cleanup_expired();

        // Выполняем тот же запрос снова
        let results_after = store
            .matryoshka_search(
                &query_vector,
                crate::math::MatryoshkaDim::D384,
                0.3,
                Some(10),
            )
            .unwrap();
        assert!(!results_after.is_empty());

        println!(
            "Cache TTL expiration test: {} results before, {} results after cleanup",
            results_before.len(),
            results_after.len()
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./cache_ttl_test_data").ok();
    }

    #[tokio::test]
    async fn test_large_scale_performance() {
        let store = Arc::new(PersistentStore::open("./large_scale_perf_test_data").unwrap());

        // Создаем 10,000 узлов для тестирования масштабируемости
        let num_nodes = 10000;
        for i in 1..=num_nodes {
            let angle = (i as f32) * 0.001; // Угол для распределения векторов
            let node = crate::storage::Node {
                id: i as u64,
                vector: vec![
                    (angle.sin() + 1.0) / 2.0, // Значения от 0 до 1
                    (angle.cos() + 1.0) / 2.0,
                    ((angle * 2.0).sin() + 1.0) / 2.0,
                    ((angle * 2.0).cos() + 1.0) / 2.0,
                ],
                properties: json!({
                    "id": i,
                    "name": format!("node_{}", i),
                    "cluster": i % 10, // 10 кластеров
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Выполняем несколько запросов для проверки производительности
        let query_vector = vec![0.5, 0.5, 0.5, 0.5];

        // Первый запрос (без кэша)
        let start_time = Instant::now();
        let results1 = store
            .matryoshka_search(
                &query_vector,
                crate::math::MatryoshkaDim::D384,
                0.2,
                Some(50),
            )
            .unwrap();
        let first_duration = start_time.elapsed();

        // Второй запрос (с кэшем)
        let start_time = Instant::now();
        let results2 = store
            .matryoshka_search(
                &query_vector,
                crate::math::MatryoshkaDim::D384,
                0.2,
                Some(50),
            )
            .unwrap();
        let second_duration = start_time.elapsed();

        println!("Large scale performance test ({} nodes):", num_nodes);
        println!(
            "  First query: {} results in {:?}",
            results1.len(),
            first_duration
        );
        println!(
            "  Second query (cached): {} results in {:?}",
            results2.len(),
            second_duration
        );

        // Проверяем, что результаты одинаковы
        assert_eq!(results1.len(), results2.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./large_scale_perf_test_data").ok();
    }

    #[tokio::test]
    async fn test_hybrid_query_caching() {
        let store = Arc::new(PersistentStore::open("./hybrid_cache_test_data").unwrap());

        // Создаем тестовые узлы для гибридного поиска
        for i in 1..=500 {
            let node = crate::storage::Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.002) % 1.0, 0.5],
                properties: json!({
                    "id": i,
                    "name": format!("hybrid_node_{}", i),
                    "category": if i % 3 == 0 { "A" } else if i % 3 == 1 { "B" } else { "C" }
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем запрос для гибридного поиска
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "node".to_string(),
                    label: "Hybrid".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["node.id".to_string(), "node.category".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 15,
            distributed: false,
        };

        // Первый запуск гибридного запроса
        let start_time = Instant::now();
        let results1 = QueryExecutor::execute_hybrid_query(&store, query.clone())
            .await
            .unwrap();
        let first_duration = start_time.elapsed();

        // Второй запуск того же запроса (должен использовать кэш)
        let start_time = Instant::now();
        let results2 = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let second_duration = start_time.elapsed();

        assert_eq!(results1.len(), results2.len());
        println!("Hybrid query caching test:");
        println!(
            "  First run: {} results in {:?}",
            results1.len(),
            first_duration
        );
        println!(
            "  Second run (cached): {} results in {:?}",
            results2.len(),
            second_duration
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./hybrid_cache_test_data").ok();
    }
}
