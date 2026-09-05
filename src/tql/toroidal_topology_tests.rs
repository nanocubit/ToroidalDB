#[cfg(test)]
mod toroidal_topology_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::{
        ConnectedClause, MatchClause, NodePattern, PropertyValue, Query, WithinClause,
    };
    use crate::tql::executor::QueryExecutor;
    use crate::tql::graph::GraphOperations;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_connectedto_parsing() {
        use crate::tql::parser;

        let query = r#"MATCH (doc:Document) CONNECTEDTO(doc, "TAGGED_WITH", "quantum") WITHIN 2 HOPS RETURN doc.id LIMIT 20"#;
        let result = parser::parse_query(query);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.connected_clause.is_some());
        assert!(parsed_query.within_clause.is_some());

        if let Some(ref connected) = parsed_query.connected_clause {
            assert_eq!(connected.target_label, "doc");
            assert_eq!(connected.relationship_type, "quantum");
        }

        if let Some(ref within) = parsed_query.within_clause {
            assert_eq!(within.min_hops, 2);
            assert_eq!(within.max_hops, 2);
        }
    }

    #[tokio::test]
    async fn test_bidirectional_bfs_algorithm() {
        let store = Arc::new(HybridPersistentStore::open("./bfs_test_data").unwrap());

        // Создаем циклический граф: 1 -> 2 -> 3 -> 4 -> 1 (цикл)
        for i in 1..=4 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![i as f32 * 0.1, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i), "type": "cycle"}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем циклические связи
        store.add_edge(1, 2, "NEXT".to_string(), 1.0).unwrap();
        store.add_edge(2, 3, "NEXT".to_string(), 1.0).unwrap();
        store.add_edge(3, 4, "NEXT".to_string(), 1.0).unwrap();
        store.add_edge(4, 1, "NEXT".to_string(), 1.0).unwrap(); // Замыкаем цикл

        // Тестируем bidirectional BFS
        let start_nodes = vec![1];
        let results = GraphOperations::bidirectional_bfs(&store, &start_nodes, 1, 2, "NEXT")
            .await
            .unwrap();

        // При обходе в 1-2 шагах от узла 1 в циклическом графе должны быть найдены узлы 2, 3, 4
        assert!(!results.is_empty());
        println!("Bidirectional BFS found {} nodes", results.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./bfs_test_data").ok();
    }

    #[tokio::test]
    async fn test_toroidal_topology_with_cycles() {
        let store = Arc::new(HybridPersistentStore::open("./toroidal_cycle_test_data").unwrap());

        // Создаем граф с циклами, характерными для тороидальной топологии
        // Узлы расположены так, что на краях пространства (0.0 и 1.0) есть связи
        let nodes = vec![
            (
                1,
                vec![0.01, 0.5],
                json!({"name": "start_near_zero", "type": "boundary"}),
            ),
            (
                2,
                vec![0.99, 0.5],
                json!({"name": "end_near_one", "type": "boundary"}),
            ), // Должен быть близок к узлу 1 из-за тороидальности
            (
                3,
                vec![0.5, 0.01],
                json!({"name": "bottom", "type": "boundary"}),
            ),
            (
                4,
                vec![0.5, 0.99],
                json!({"name": "top", "type": "boundary"}),
            ), // Должен быть близок к узлу 3 из-за тороидальности
            (
                5,
                vec![0.5, 0.5],
                json!({"name": "center", "type": "center"}),
            ),
        ];

        for (id, vector, properties) in nodes {
            let node = crate::hybrid_storage::Node {
                id,
                vector,
                properties,
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем связи, моделирующие тороидальную топологию
        store
            .add_edge(1, 2, "TOROIDAL_WRAP".to_string(), 0.95)
            .unwrap(); // Связь между 0.01 и 0.99
        store
            .add_edge(3, 4, "TOROIDAL_WRAP".to_string(), 0.95)
            .unwrap(); // Связь между 0.01 и 0.99 по Y

        // Создаем запрос с CONNECTEDTO и WITHIN HOPS
        let connected_clause = ConnectedClause {
            target_label: "boundary".to_string(),
            relationship_type: "TOROIDAL_WRAP".to_string(),
            property_filter: None,
        };

        let within_clause = WithinClause {
            min_hops: 1,
            max_hops: 2,
        };

        // Тестируем выполнение связанного запроса
        let start_nodes = vec![1];
        let results = GraphOperations::find_connected_nodes(
            &store,
            &start_nodes,
            &connected_clause,
            &within_clause,
        )
        .await
        .unwrap();

        assert!(!results.is_empty());
        println!(
            "Toroidal topology test found {} connected nodes",
            results.len()
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./toroidal_cycle_test_data").ok();
    }

    #[tokio::test]
    async fn test_within_hops_range() {
        let store = Arc::new(HybridPersistentStore::open("./hops_range_test_data").unwrap());

        // Создаем линейный граф: 1-2-3-4-5
        for i in 1..=5 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![i as f32 * 0.1, 0.5],
                properties: json!({"id": i, "name": format!("linear_node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем линейные связи
        for i in 1..5 {
            store
                .add_edge(i, i + 1, "SEQUENTIAL".to_string(), 0.8)
                .unwrap();
        }

        // Тестируем bidirectional BFS с диапазоном hops
        let start_nodes = vec![3]; // Начинаем с центрального узла

        // Сначала тест с 1-2 hops
        let results_1_2 =
            GraphOperations::bidirectional_bfs(&store, &start_nodes, 1, 2, "SEQUENTIAL")
                .await
                .unwrap();

        // Затем тест с 1-3 hops
        let results_1_3 =
            GraphOperations::bidirectional_bfs(&store, &start_nodes, 1, 3, "SEQUENTIAL")
                .await
                .unwrap();

        // Результатов с большим диапазоном должно быть больше или столько же
        assert!(results_1_3.len() >= results_1_2.len());
        println!(
            "Hops range test: 1-2 hops: {}, 1-3 hops: {}",
            results_1_2.len(),
            results_1_3.len()
        );

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./hops_range_test_data").ok();
    }

    #[tokio::test]
    async fn test_complex_toroidal_query() {
        let store = Arc::new(HybridPersistentStore::open("./complex_toroidal_test_data").unwrap());

        // Создаем более сложный граф с тороидальными свойствами
        for i in 1..=10 {
            // Распределяем узлы по краям тороидального пространства
            let x = if i % 3 == 0 {
                0.01
            } else if i % 3 == 1 {
                0.99
            } else {
                i as f32 * 0.1
            };
            let y = if i % 2 == 0 { 0.01 } else { 0.99 };

            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![x, y],
                properties: json!({
                    "id": i,
                    "name": format!("toroidal_node_{}", i),
                    "x": x,
                    "y": y
                }),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем связи, отражающие тороидальную топологию
        // Связи между "крайними" точками
        store.add_edge(1, 2, "TOROIDAL_X".to_string(), 0.9).unwrap();
        store.add_edge(3, 4, "TOROIDAL_Y".to_string(), 0.9).unwrap();

        // Выполняем гибридный запрос, используя специфичные для топологии конструкции
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "start".to_string(),
                    label: "Boundary".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: Some(ConnectedClause {
                target_label: "Boundary".to_string(),
                relationship_type: "TOROIDAL_X".to_string(),
                property_filter: None,
            }),
            within_clause: Some(WithinClause {
                min_hops: 1,
                max_hops: 3,
            }),
            return_fields: vec!["start.id".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 10,
            distributed: false,
            ..Default::default()
        };

        // Выполняем запрос через исполнитель
        let results = QueryExecutor::execute_connected_query(
            &store,
            query,
            &ConnectedClause {
                target_label: "Boundary".to_string(),
                relationship_type: "TOROIDAL_X".to_string(),
                property_filter: None,
            },
            &WithinClause {
                min_hops: 1,
                max_hops: 3,
            },
        )
        .await
        .unwrap();

        assert!(!results.is_empty());
        println!("Complex toroidal query found {} results", results.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./complex_toroidal_test_data").ok();
    }

    #[tokio::test]
    async fn test_toroidal_topology_edge_cases() {
        let store = Arc::new(HybridPersistentStore::open("./edge_cases_test_data").unwrap());

        // Тест крайних случаев тороидальной топологии
        let boundary_nodes = vec![
            (
                1,
                vec![0.0, 0.0],
                json!({"name": "corner_00", "type": "corner"}),
            ),
            (
                2,
                vec![1.0, 0.0],
                json!({"name": "corner_10", "type": "corner"}),
            ),
            (
                3,
                vec![0.0, 1.0],
                json!({"name": "corner_01", "type": "corner"}),
            ),
            (
                4,
                vec![1.0, 1.0],
                json!({"name": "corner_11", "type": "corner"}),
            ),
            (
                5,
                vec![0.5, 0.5],
                json!({"name": "center", "type": "center"}),
            ),
        ];

        for (id, vector, properties) in boundary_nodes {
            let node = crate::hybrid_storage::Node {
                id,
                vector,
                properties,
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем связи между угловыми узлами, моделируя тороидальные свойства
        store
            .add_edge(1, 2, "TOROIDAL_X".to_string(), 0.99)
            .unwrap(); // (0,0) - (1,0)
        store
            .add_edge(1, 3, "TOROIDAL_Y".to_string(), 0.99)
            .unwrap(); // (0,0) - (0,1)
        store
            .add_edge(2, 4, "TOROIDAL_Y".to_string(), 0.99)
            .unwrap(); // (1,0) - (1,1)
        store
            .add_edge(3, 4, "TOROIDAL_X".to_string(), 0.99)
            .unwrap(); // (0,1) - (1,1)

        // Тестируем bidirectional BFS на граничных узлах
        let start_nodes = vec![1]; // Начинаем с (0,0)
        let results = GraphOperations::bidirectional_bfs(&store, &start_nodes, 1, 2, "TOROIDAL_X")
            .await
            .unwrap();

        assert!(!results.is_empty());
        println!("Edge cases test found {} nodes from corner", results.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./edge_cases_test_data").ok();
    }
}
