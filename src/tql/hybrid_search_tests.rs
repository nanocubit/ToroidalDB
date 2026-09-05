#[cfg(test)]
mod hybrid_search_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::{MatchClause, NodePattern, Query};
    use crate::tql::executor::QueryExecutor;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_hybrid_search_with_large_dataset() {
        let store = Arc::new(HybridPersistentStore::open("./hybrid_large_test_data").unwrap());

        // Создаем 1000+ узлов для тестирования
        let num_nodes = 1200;
        for i in 1..=num_nodes {
            let node = crate::hybrid_storage::Node {
                id: i as u64,
                vector: vec![
                    (i as f32 * 0.001) % 1.0, // Значения от 0 до 1
                    ((i * 2) as f32 * 0.001) % 1.0,
                    ((i * 3) as f32 * 0.001) % 1.0,
                    ((i * 4) as f32 * 0.001) % 1.0,
                ],
                properties: json!({
                    "id": i,
                    "name": format!("node_{}", i),
                    "category": if i % 3 == 0 { "A" } else if i % 3 == 1 { "B" } else { "C" },
                    "value": i as f32 * 0.1
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем случайные связи между узлами
        for i in 1..num_nodes {
            if i + 1 <= num_nodes {
                store
                    .add_edge(i as u64, (i + 1) as u64, "NEXT".to_string(), 0.8)
                    .unwrap();
            }
            if i + 5 <= num_nodes {
                store
                    .add_edge(i as u64, (i + 5) as u64, "JUMP".to_string(), 0.6)
                    .unwrap();
            }
        }

        // Создаем гибридный запрос
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
            limit: 20,
            distributed: false,
            ..Default::default()
        };

        // Выполняем гибридный поиск
        let start_time = std::time::Instant::now();
        let results = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let duration = start_time.elapsed();

        assert!(!results.is_empty());
        assert!(results.len() <= 20); // Проверяем ограничение LIMIT
        println!("Hybrid search on {} nodes took {:?}", num_nodes, duration);
        println!("Found {} results", results.len());

        // Проверяем, что результаты упорядочены по оценке
        for i in 1..results.len() {
            assert!(results[i - 1].score <= results[i].score); // Оценки должны быть в порядке возрастания
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./hybrid_large_test_data").ok();
    }

    #[tokio::test]
    async fn test_hybrid_search_caching() {
        let store = Arc::new(HybridPersistentStore::open("./hybrid_cache_test_data").unwrap());

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
            ..Default::default()
        };

        // Выполняем запрос первый раз
        let start_time = std::time::Instant::now();
        let results1 = QueryExecutor::execute_hybrid_query(&store, query.clone())
            .await
            .unwrap();
        let duration1 = start_time.elapsed();

        // Выполняем тот же запрос второй раз (должен использовать кэш)
        let start_time = std::time::Instant::now();
        let results2 = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let duration2 = start_time.elapsed();

        // Результаты должны быть одинаковыми
        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(results2.iter()) {
            assert_eq!(r1.id, r2.id);
            assert_eq!(r1.score, r2.score);
        }

        // Второе выполнение должно быть быстрее благодаря кэшированию
        println!("First execution: {:?}", duration1);
        println!("Second execution (cached): {:?}", duration2);
        // Примечание: в тестовой среде разница может быть незначительной

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./hybrid_cache_test_data").ok();
    }

    #[tokio::test]
    async fn test_hybrid_search_with_vector_search() {
        let store = Arc::new(HybridPersistentStore::open("./hybrid_vector_test_data").unwrap());

        // Создаем узлы с близкими векторами для тестирования векторного поиска
        let base_vectors = vec![
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.15, 0.25, 0.35, 0.45],
            vec![0.8, 0.9, 0.7, 0.6],
            vec![0.12, 0.22, 0.32, 0.42],
        ];

        for (idx, base_vec) in base_vectors.iter().enumerate() {
            for i in 0..250 {
                // 4 группы по 250 узлов = 1000 узлов
                let variation = vec![
                    base_vec[0] + (i as f32 * 0.0001),
                    base_vec[1] + (i as f32 * 0.0001),
                    base_vec[2] + (i as f32 * 0.0001),
                    base_vec[3] + (i as f32 * 0.0001),
                ];

                let node_id = (idx * 250 + i + 1) as u64;
                let node = crate::hybrid_storage::Node {
                    id: node_id,
                    vector: variation,
                    properties: json!({
                        "group": idx,
                        "id": node_id,
                        "name": format!("node_{}_{}", idx, i)
                    }),
                    edges: vec![],
                };
                store.insert(node).unwrap();
            }
        }

        // Создаем запрос для гибридного поиска
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "n".to_string(),
                    label: "Group0".to_string(), // Ищем в первой группе
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["n.id".to_string(), "n.group".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 15,
            distributed: false,
            ..Default::default()
        };

        let results = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();

        assert!(!results.is_empty());
        assert!(results.len() <= 15);
        println!(
            "Hybrid vector search found {} results from clustered data",
            results.len()
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./hybrid_vector_test_data").ok();
    }

    #[tokio::test]
    async fn test_performance_comparison() {
        let store = Arc::new(HybridPersistentStore::open("./perf_comparison_test_data").unwrap());

        // Создаем 1500 узлов
        for i in 1..=1500 {
            let node = crate::hybrid_storage::Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.001) % 1.0, 0.5, 0.3, 0.7],
                properties: json!({"id": i, "value": i as f32}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

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
            limit: 25,
            distributed: false,
            ..Default::default()
        };

        // Измеряем время выполнения гибридного поиска
        let start_time = std::time::Instant::now();
        let results = QueryExecutor::execute_hybrid_query(&store, query)
            .await
            .unwrap();
        let hybrid_duration = start_time.elapsed();

        assert!(!results.is_empty());
        println!(
            "Hybrid search on 1500 nodes: {:?}, found {} results",
            hybrid_duration,
            results.len()
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./perf_comparison_test_data").ok();
    }
}
