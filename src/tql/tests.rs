#[cfg(test)]
mod tql_integration_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::*;
    use crate::tql::coordinator::{DistributedExecutor, QueryCoordinator};
    use crate::tql::executor::QueryExecutor;
    use crate::tql::parser;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_basic_tql_parsing() {
        let query_text =
            "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10";
        let result = parser::parse_query(query_text);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());
        assert_eq!(parsed_query.limit, 10);
        if let Some(ref match_clause) = parsed_query.match_clause {
            assert_eq!(match_clause.source.label, "Document");
        }
    }

    #[tokio::test]
    async fn test_connectedto_parsing() {
        let query_text = r#"MATCH (doc:Document) CONNECTEDTO(doc, "TAGGED_WITH", "quantum") WITHIN 2 HOPS RETURN doc.id LIMIT 20"#;
        let result = parser::parse_query(query_text);

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
    async fn test_aggregation_parsing() {
        let query_text = r#"MATCH (user:User)-[:LIKES]->(content:Content) RETURN user.id, COUNT(content) as likes ORDER BY likes DESC LIMIT 10"#;
        let result = parser::parse_query(query_text);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(!parsed_query.aggregation_fields.is_empty());
        assert!(parsed_query.order_by.is_some());

        // Проверяем, что есть агрегация COUNT
        let count_agg = parsed_query
            .aggregation_fields
            .iter()
            .find(|field| matches!(field.function, AggregationFunction::Count));
        assert!(count_agg.is_some());
    }

    #[tokio::test]
    async fn test_distributed_flag_parsing() {
        let query_text = r#"DISTRIBUTED MATCH (node:Label) WHERE TOROIDALDISTANCE(node.vector, 0.4) RETURN node.id, node.score LIMIT 5"#;
        let result = parser::parse_query(query_text);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.distributed);
    }

    #[tokio::test]
    async fn test_query_execution_with_mock_data() {
        // Создаем временное хранилище для тестирования
        let store = HybridPersistentStore::open("./test_data_temp").unwrap();

        // Добавляем тестовые узлы
        let test_nodes = vec![
            crate::hybrid_storage::Node {
                id: 1,
                vector: vec![0.1, 0.2, 0.3],
                properties: json!({"name": "test1", "type": "document"}),
                edges: vec![],
            },
            crate::hybrid_storage::Node {
                id: 2,
                vector: vec![0.4, 0.5, 0.6],
                properties: json!({"name": "test2", "type": "document"}),
                edges: vec![],
            },
            crate::hybrid_storage::Node {
                id: 3,
                vector: vec![0.7, 0.8, 0.9],
                properties: json!({"name": "test3", "type": "document"}),
                edges: vec![],
            },
        ];

        for node in test_nodes {
            store.insert(node).unwrap();
        }

        // Тестируем выполнение запроса
        let query_text =
            "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.5) RETURN doc.id LIMIT 5";
        let parse_result = parser::parse_query(query_text);
        assert!(parse_result.is_ok());

        let (_, parsed_query) = parse_result.unwrap();
        let execution_result = QueryExecutor::execute_query(&store, parsed_query).await;

        assert!(execution_result.is_ok());
        let results = execution_result.unwrap();
        assert!(!results.is_empty());

        // Удаляем временные данные
        std::fs::remove_dir_all("./test_data_temp").ok();
    }

    #[tokio::test]
    async fn test_distributed_query_execution() {
        // Создаем несколько шардов для тестирования
        let shard1 = Arc::new(HybridPersistentStore::open("./test_shard1").unwrap());
        let shard2 = Arc::new(HybridPersistentStore::open("./test_shard2").unwrap());
        let shards = vec![shard1.clone(), shard2.clone()];

        // Добавляем тестовые данные в шарды
        let test_node1 = crate::hybrid_storage::Node {
            id: 1,
            vector: vec![0.99, 0.5],
            properties: json!({"name": "shard1_node", "type": "test"}),
            edges: vec![],
        };

        let test_node2 = crate::hybrid_storage::Node {
            id: 2,
            vector: vec![0.01, 0.5],
            properties: json!({"name": "shard2_node", "type": "test"}),
            edges: vec![],
        };

        let test_node2 = crate::hybrid_storage::Node {
            id: 2,
            vector: vec![0.4, 0.5, 0.6],
            properties: json!({"name": "shard2_node", "type": "test"}),
            edges: vec![],
        };

        shard1.insert(test_node1).unwrap();
        shard2.insert(test_node2).unwrap();

        // Создаем координатора и исполнитель
        let coordinator = Arc::new(QueryCoordinator::new(2, shards));
        let executor = DistributedExecutor::new(coordinator);

        // Создаем тестовый запрос
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "node".to_string(),
                    label: "Test".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["node.id".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 10,
            distributed: true,
            ..Default::default()
        };

        // Выполняем распределенный запрос
        let result = executor.execute_distributed(query).await;
        assert!(result.is_ok());

        // Удаляем временные данные
        std::fs::remove_dir_all("./test_shard1").ok();
        std::fs::remove_dir_all("./test_shard2").ok();
    }

    #[tokio::test]
    async fn test_toroidal_distance_search() {
        let store = HybridPersistentStore::open("./test_toroidal_temp").unwrap();

        // Добавляем узлы с векторами, которые должны быть близки в торическом пространстве
        // (0.99 и 0.01 должны быть близки из-за цикличности)
        let node1 = crate::hybrid_storage::Node {
            id: 1,
            vector: vec![0.99, 0.5],
            properties: json!({"name": "near_zero", "type": "boundary"}),
            edges: vec![],
        };

        let node2 = crate::hybrid_storage::Node {
            id: 2,
            vector: vec![0.01, 0.5],
            properties: json!({"name": "near_one", "type": "boundary"}),
            edges: vec![],
        };

        let node3 = crate::hybrid_storage::Node {
            id: 3,
            vector: vec![0.5, 0.5],
            properties: json!({"name": "middle", "type": "center"}),
            edges: vec![],
        };

        store.insert(node1).unwrap();
        store.insert(node2).unwrap();
        store.insert(node3).unwrap();

        // Выполняем запрос, который должен найти узлы близкие к (0.0, 0.5)
        // из-за торической топологии узлы 1 и 2 должны быть близки
        let query_text =
            "MATCH (n:Boundary) WHERE TOROIDALDISTANCE(n.vector, 0.3) RETURN n.id LIMIT 10";
        let parse_result = parser::parse_query(query_text);
        assert!(parse_result.is_ok());

        let (_, parsed_query) = parse_result.unwrap();
        let execution_result = QueryExecutor::execute_query(&store, parsed_query).await;

        assert!(execution_result.is_ok());
        let results = execution_result.unwrap();
        assert!(!results.is_empty());

        // Удаляем временные данные
        std::fs::remove_dir_all("./test_toroidal_temp").ok();
    }

    #[tokio::test]
    async fn test_aggregation_execution() {
        let store = HybridPersistentStore::open("./test_agg_temp").unwrap();

        // Добавляем узлы с числовыми свойствами для тестирования агрегаций
        for i in 1..=5 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![i as f32 * 0.1, 0.5],
                properties: json!({"value": i, "type": "numeric"}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем запрос с агрегацией
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "n".to_string(),
                    label: "Numeric".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: None,
            connected_clause: None,
            within_clause: None,
            return_fields: vec![],
            aggregation_fields: vec![AggregationField {
                function: AggregationFunction::Count,
                alias: Some("total".to_string()),
            }],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 10,
            distributed: false,
            ..Default::default()
        };

        let execution_result = QueryExecutor::execute_query(&store, query).await;
        assert!(execution_result.is_ok());
        let results = execution_result.unwrap();
        assert!(!results.is_empty());

        // Удаляем временные данные
        std::fs::remove_dir_all("./test_agg_temp").ok();
    }
}
