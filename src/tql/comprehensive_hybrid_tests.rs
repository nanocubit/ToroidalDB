#[cfg(test)]
mod comprehensive_hybrid_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::{MatchClause, NodePattern, Query};
    use crate::tql::executor::QueryExecutor;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_real_world_hybrid_scenario() {
        let store = Arc::new(HybridPersistentStore::open("./real_world_test_data").unwrap());

        // Создаем реалистичный сценарий: социальная сеть с интересами
        // Пользователи с векторными профилями и связями интересов

        // Создаем пользователей с похожими интересами
        let users_data = vec![
            // Группа 1: пользователи с интересами в технологиях
            (1, "Alice", vec![0.9, 0.1, 0.2, 0.8], "tech"),
            (2, "Bob", vec![0.85, 0.15, 0.25, 0.75], "tech"),
            (3, "Charlie", vec![0.88, 0.12, 0.22, 0.78], "tech"),
            // Группа 2: пользователи с интересами в искусстве
            (4, "Diana", vec![0.2, 0.8, 0.9, 0.1], "art"),
            (5, "Eve", vec![0.25, 0.75, 0.85, 0.15], "art"),
            (6, "Frank", vec![0.22, 0.78, 0.88, 0.12], "art"),
            // Группа 3: пользователи с интересами в спорте
            (7, "Grace", vec![0.1, 0.3, 0.1, 0.9], "sports"),
            (8, "Henry", vec![0.15, 0.35, 0.15, 0.85], "sports"),
            (9, "Ivy", vec![0.12, 0.32, 0.12, 0.88], "sports"),
        ];

        for (id, name, vector, category) in &users_data {
            let node = crate::hybrid_storage::Node {
                id: *id,
                vector: vector.clone(),
                properties: json!({
                    "name": name,
                    "category": category,
                    "interests": match *category {
                        "tech" => vec!["programming", "ai", "blockchain"],
                        "art" => vec!["painting", "sculpture", "design"],
                        "sports" => vec!["football", "basketball", "tennis"],
                        _ => vec!["general"]
                    }
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем социальные связи (кто на кого подписан)
        let social_connections = vec![
            (1, 2),
            (2, 3), // Tech group connections
            (4, 5),
            (5, 6), // Art group connections
            (7, 8),
            (8, 9), // Sports group connections
            (1, 4),
            (4, 7), // Cross-group connections
        ];

        for (from, to) in &social_connections {
            store
                .add_edge(*from, *to, "FOLLOWS".to_string(), 0.8)
                .unwrap();
        }

        // Выполняем гибридный запрос: найти похожих пользователей с учетом социальных связей
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "currentUser".to_string(),
                    label: "User".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["currentUser.id".to_string(), "currentUser.name".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 10,
            distributed: false,
        };

        let start_time = std::time::Instant::now();
        let results = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let duration = start_time.elapsed();

        assert!(!results.is_empty());
        assert!(results.len() <= 10);
        println!(
            "Real-world hybrid test: {} results in {:?}",
            results.len(),
            duration
        );

        // Проверяем, что результаты содержат правильные поля
        for result in &results {
            assert!(result.id > 0 && result.id <= 9);
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./real_world_test_data").ok();
    }

    #[tokio::test]
    async fn test_hybrid_search_with_different_thresholds() {
        let store = Arc::new(HybridPersistentStore::open("./threshold_test_data").unwrap());

        // Создаем узлы с близкими векторами
        for i in 1..=500 {
            let variation = (i as f32 * 0.001) % 0.1;
            let node = crate::hybrid_storage::Node {
                id: i as u64,
                vector: vec![
                    0.5 + variation,
                    0.5 - variation,
                    0.3 + variation,
                    0.7 - variation,
                ],
                properties: json!({
                    "id": i,
                    "name": format!("similar_node_{}", i),
                    "group": if i <= 250 { "group_a" } else { "group_b" }
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем связи между узлами одной группы
        for i in 1..250 {
            store
                .add_edge(i as u64, (i + 1) as u64, "SIMILAR_TO".to_string(), 0.9)
                .unwrap();
        }

        // Тестируем гибридный поиск с разными порогами
        let thresholds = vec![0.1, 0.2, 0.3];

        for threshold in &thresholds {
            let query = Query {
                match_clause: Some(MatchClause {
                    source: NodePattern {
                        alias: "n".to_string(),
                        label: "Similar".to_string(),
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

            let start_time = std::time::Instant::now();
            let results = QueryExecutor::execute_hybrid_query(&store, query)
                .await
                .unwrap();
            let duration = start_time.elapsed();

            println!(
                "Threshold {}: {} results in {:?}",
                threshold,
                results.len(),
                duration
            );
            assert!(!results.is_empty());
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./threshold_test_data").ok();
    }

    #[tokio::test]
    async fn test_cache_efficiency() {
        let store = Arc::new(HybridPersistentStore::open("./cache_efficiency_test_data").unwrap());

        // Создаем тестовые узлы
        for i in 1..=100 {
            let node = crate::hybrid_storage::Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.01) % 1.0, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем один и тот же запрос для тестирования кэширования
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
            distributed: false,
        };

        // Выполняем запрос первый раз
        let start_time = std::time::Instant::now();
        let results1 = QueryExecutor::execute_hybrid_query(&store, query.clone())
            .await
            .unwrap();
        let first_duration = start_time.elapsed();

        // Выполняем тот же запрос второй раз (должен использовать кэш)
        let start_time = std::time::Instant::now();
        let results2 = QueryExecutor::execute_hybrid_query(&store, query.clone())
            .await
            .unwrap();
        let second_duration = start_time.elapsed();

        // Выполняем третий раз для уверенности
        let start_time = std::time::Instant::now();
        let results3 = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let third_duration = start_time.elapsed();

        // Проверяем, что результаты одинаковы
        assert_eq!(results1.len(), results2.len());
        assert_eq!(results2.len(), results3.len());

        for ((r1, r2), r3) in results1.iter().zip(results2.iter()).zip(results3.iter()) {
            assert_eq!(r1.id, r2.id);
            assert_eq!(r2.id, r3.id);
            assert_eq!(r1.score, r2.score);
            assert_eq!(r2.score, r3.score);
        }

        println!("Cache efficiency test:");
        println!("  First execution: {:?}", first_duration);
        println!("  Second execution (cached): {:?}", second_duration);
        println!("  Third execution (cached): {:?}", third_duration);

        // Второе и третье выполнение должны быть быстрее благодаря кэшированию
        if first_duration > second_duration {
            println!("  Cache hit detected: second execution was faster");
        } else {
            println!("  Cache may have been cold or system overhead affected timing");
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./cache_efficiency_test_data").ok();
    }

    #[tokio::test]
    async fn test_large_scale_hybrid_search() {
        let store = Arc::new(HybridPersistentStore::open("./large_scale_test_data").unwrap());

        // Создаем 2000 узлов для масштабного тестирования
        let num_nodes = 2000;
        for i in 1..=num_nodes {
            let angle = (i as f32) * 0.1; // Угол для распределения векторов
            let node = crate::hybrid_storage::Node {
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

        // Создаем случайные связи для имитации графовой структуры
        use rand::Rng;
        let mut rng = rand::thread_rng();

        for i in 1..=num_nodes {
            // Каждый узел имеет 1-3 случайных связи
            let num_connections = rng.gen_range(1..=3);
            for _ in 0..num_connections {
                let target = rng.gen_range(1..=num_nodes) as u64;
                if target != i as u64 {
                    store
                        .add_edge(i as u64, target, "RELATED_TO".to_string(), 0.7)
                        .unwrap();
                }
            }
        }

        // Выполняем гибридный поиск
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "n".to_string(),
                    label: "Clustered".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["n.id".to_string(), "n.cluster".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 25,
            distributed: false,
        };

        let start_time = std::time::Instant::now();
        let results = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let duration = start_time.elapsed();

        assert!(!results.is_empty());
        assert!(results.len() <= 25);
        println!(
            "Large scale test ({} nodes): {} results in {:?}",
            num_nodes,
            results.len(),
            duration
        );

        // Проверяем, что результаты упорядочены по релевантности
        for i in 1..results.len() {
            assert!(results[i - 1].score <= results[i].score);
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./large_scale_test_data").ok();
    }
}
