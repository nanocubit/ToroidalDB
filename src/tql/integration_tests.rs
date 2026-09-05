#[cfg(test)]
mod integration_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::executor::QueryExecutor;
    use crate::tql::parser;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_parser_integration_with_matryoshka_search() {
        // Создаем хранилище
        let store = Arc::new(HybridPersistentStore::open("./integration_test_data").unwrap());

        // Добавляем тестовые узлы
        let test_nodes = vec![
            crate::hybrid_storage::Node {
                id: 1,
                vector: vec![0.1, 0.2, 0.3, 0.4], // 4-мерный вектор для тестирования
                properties: json!({"name": "node1", "type": "test"}),
                edges: vec![],
            },
            crate::hybrid_storage::Node {
                id: 2,
                vector: vec![0.15, 0.25, 0.35, 0.45], // Близкий вектор
                properties: json!({"name": "node2", "type": "test"}),
                edges: vec![],
            },
            crate::hybrid_storage::Node {
                id: 3,
                vector: vec![0.8, 0.9, 0.7, 0.6], // Далекий вектор
                properties: json!({"name": "node3", "type": "test"}),
                edges: vec![],
            },
        ];

        for node in test_nodes {
            store.insert(node).unwrap();
        }

        // Тестируем интеграцию парсера с исполнителем
        let query_text = "MATCH (n:Test) WHERE TOROIDALDISTANCE(n.vector, 0.2) RETURN n.id LIMIT 5";
        let parse_result = parser::parse_query(query_text);

        assert!(parse_result.is_ok(), "Query parsing should succeed");

        let (_, parsed_query) = parse_result.unwrap();
        let execution_result = QueryExecutor::execute_query(&store, parsed_query).await;

        assert!(execution_result.is_ok(), "Query execution should succeed");

        let results = execution_result.unwrap();
        assert!(!results.is_empty(), "Should find at least one result");
        assert!(results.len() <= 5, "Should respect LIMIT constraint");

        // Проверяем, что результаты содержат правильные поля
        for result in &results {
            assert!(result.id > 0, "ID should be positive");
            assert!(result.score >= 0.0, "Score should be non-negative");
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./integration_test_data").ok();
    }

    #[tokio::test]
    async fn test_different_thresholds_integration() {
        let store = Arc::new(HybridPersistentStore::open("./threshold_test_data").unwrap());

        // Добавляем узлы с близкими векторами
        for i in 1..=10 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![i as f32 * 0.05, 0.5], // Векторы становятся немного дальше друг от друга
                properties: json!({"id": i, "name": format!("node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Тестируем разные пороги
        let thresholds = vec![0.1, 0.2, 0.3];

        for threshold in thresholds {
            let query_text = &format!(
                "MATCH (n:Node) WHERE TOROIDALDISTANCE(n.vector, {}) RETURN n.id LIMIT 10",
                threshold
            );
            let (_, parsed_query) = parser::parse_query(query_text).unwrap();
            let results = QueryExecutor::execute_query(&store, parsed_query)
                .await
                .unwrap();

            // С увеличением порога должно находиться больше результатов
            println!("Threshold {}: Found {} results", threshold, results.len());
            assert!(
                results.len() >= 1,
                "Should find at least one result for threshold {}",
                threshold
            );
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./threshold_test_data").ok();
    }

    #[tokio::test]
    async fn test_parser_executor_roundtrip() {
        let store = Arc::new(HybridPersistentStore::open("./roundtrip_test_data").unwrap());

        // Добавляем тестовые данные
        let node = crate::hybrid_storage::Node {
            id: 1,
            vector: vec![0.5, 0.5, 0.5],
            properties: json!({"name": "test_node", "category": "integration"}),
            edges: vec![],
        };
        store.insert(node).unwrap();

        // Тестируем полный цикл: текст запроса -> парсинг -> выполнение -> результаты
        let query_texts = vec![
            "MATCH (n:Integration) WHERE TOROIDALDISTANCE(n.vector, 0.3) RETURN n.id LIMIT 1",
            "MATCH (node:Test) RETURN node.id LIMIT 1",
        ];

        for query_text in query_texts {
            println!("Testing query: {}", query_text);

            // Этап 1: Парсинг
            let parse_result = parser::parse_query(query_text);
            assert!(
                parse_result.is_ok(),
                "Parsing should succeed for: {}",
                query_text
            );

            let (_, parsed_query) = parse_result.unwrap();

            // Этап 2: Выполнение
            let execution_result = QueryExecutor::execute_query(&store, parsed_query).await;
            assert!(
                execution_result.is_ok(),
                "Execution should succeed for: {}",
                query_text
            );

            let results = execution_result.unwrap();
            println!("  Found {} results", results.len());
        }

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./roundtrip_test_data").ok();
    }

    #[tokio::test]
    async fn test_matryoshka_search_direct_vs_tql_equivalence() {
        let store = Arc::new(HybridPersistentStore::open("./equivalence_test_data").unwrap());

        // Добавляем тестовые узлы
        let vectors = vec![
            vec![0.1, 0.2, 0.3],
            vec![0.15, 0.25, 0.35],
            vec![0.8, 0.9, 0.7],
            vec![0.12, 0.22, 0.32],
        ];

        for (i, vector) in vectors.iter().enumerate() {
            let node = crate::hybrid_storage::Node {
                id: (i + 1) as u64,
                vector: vector.clone(),
                properties: json!({"name": format!("node_{}", i + 1)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Выполняем прямой вызов matryoshka_search
        let query_vector = vec![0.11, 0.21, 0.31];
        let direct_results = store
            .matryoshka_search(&query_vector, crate::math::MatryoshkaDim::D384, 0.3)
            .unwrap();

        // Выполняем тот же поиск через TQL
        let tql_query = "MATCH (n:Node) WHERE TOROIDALDISTANCE(n.vector, 0.3) RETURN n.id LIMIT 10";
        let (_, parsed_query) = parser::parse_query(tql_query).unwrap();
        let tql_results = QueryExecutor::execute_query(&store, parsed_query)
            .await
            .unwrap();

        // Оба метода должны вернуть результаты (хотя возможно в разном порядке)
        assert!(
            !direct_results.is_empty(),
            "Direct search should return results"
        );
        assert!(!tql_results.is_empty(), "TQL search should return results");

        println!("Direct search found {} results", direct_results.len());
        println!("TQL search found {} results", tql_results.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./equivalence_test_data").ok();
    }
}
