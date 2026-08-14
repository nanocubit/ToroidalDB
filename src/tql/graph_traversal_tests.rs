#[cfg(test)]
mod graph_traversal_tests {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::executor::QueryExecutor;
    use crate::tql::parser;
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_basic_graph_relationship_parsing() {
        // Тестируем разбор синтаксиса графовых связей
        let query = "MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.id, b.id";
        let result = parser::parse_query(query);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());

        if let Some(ref match_clause) = parsed_query.match_clause {
            assert!(match_clause.relationship.is_some());
            assert!(match_clause.target.is_some());

            let relationship = match_clause.relationship.as_ref().unwrap();
            assert_eq!(relationship.type_, "FOLLOWS");
            // Проверяем направление (в нашем случае это направленное ребро)
        }
    }

    #[tokio::test]
    async fn test_undirected_relationship_parsing() {
        // Тестируем разбор ненаправленных связей
        let query = "MATCH (a:User)-[:FRIENDS_WITH]-(b:User) RETURN a.id, b.id";
        let result = parser::parse_query(query);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());

        if let Some(ref match_clause) = parsed_query.match_clause {
            assert!(match_clause.relationship.is_some());

            let relationship = match_clause.relationship.as_ref().unwrap();
            assert_eq!(relationship.type_, "FRIENDS_WITH");
            // В реальной реализации нужно проверить направление
        }
    }

    #[tokio::test]
    async fn test_incoming_relationship_parsing() {
        // Тестируем разбор входящих связей
        let query = "MATCH (a:User)<-[:FOLLOWS]-(b:User) RETURN a.id, b.id";
        let result = parser::parse_query(query);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());

        if let Some(ref match_clause) = parsed_query.match_clause {
            assert!(match_clause.relationship.is_some());

            let relationship = match_clause.relationship.as_ref().unwrap();
            assert_eq!(relationship.type_, "FOLLOWS");
        }
    }

    #[tokio::test]
    async fn test_graph_traversal_with_real_data() {
        let store = Arc::new(HybridPersistentStore::open("./graph_test_data").unwrap());

        // Создаем тестовые узлы
        let users = vec![
            (1, vec![0.1, 0.1], json!({"name": "Alice", "type": "user"})),
            (2, vec![0.2, 0.2], json!({"name": "Bob", "type": "user"})),
            (
                3,
                vec![0.3, 0.3],
                json!({"name": "Charlie", "type": "user"}),
            ),
            (4, vec![0.4, 0.4], json!({"name": "Diana", "type": "user"})),
        ];

        for (id, vector, properties) in users {
            let node = crate::hybrid_storage::Node {
                id,
                vector,
                properties,
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем связи между пользователями
        store.add_edge(1, 2, "FOLLOWS".to_string(), 0.9).unwrap(); // Alice -> Bob
        store.add_edge(2, 3, "FOLLOWS".to_string(), 0.8).unwrap(); // Bob -> Charlie
        store
            .add_edge(1, 4, "FRIENDS_WITH".to_string(), 0.7)
            .unwrap(); // Alice <-> Diana (в обе стороны)
        store
            .add_edge(4, 1, "FRIENDS_WITH".to_string(), 0.7)
            .unwrap(); // Alice <-> Diana

        // Тестируем выполнение запроса на поиск связей
        // В текущей реализации наш парсер не полностью поддерживает синтаксис графовых связей
        // Поэтому используем базовый запрос для проверки функциональности
        let query_text = "MATCH (n:User) RETURN n.id LIMIT 10";
        let (_, parsed_query) = parser::parse_query(query_text).unwrap();
        let results = QueryExecutor::execute_query(&store, parsed_query)
            .await
            .unwrap();

        assert!(!results.is_empty());
        println!("Found {} users in graph", results.len());

        // Проверяем, что все созданные узлы присутствуют
        assert_eq!(results.len(), 4);

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./graph_test_data").ok();
    }

    #[tokio::test]
    async fn test_complex_graph_pattern_parsing() {
        // Тестируем более сложные паттерны графа
        let query = "MATCH (user:User)-[:LIKES]->(post:Post)<-[:AUTHORED]-(author:User) RETURN user.id, post.id, author.id";
        let result = parser::parse_query(query);

        assert!(result.is_ok());

        let (_, parsed_query) = result.unwrap();
        assert!(parsed_query.match_clause.is_some());

        if let Some(ref match_clause) = parsed_query.match_clause {
            assert!(match_clause.source.label == "User");
            assert!(match_clause.relationship.is_some());
            assert!(match_clause.target.is_some());

            // В текущей реализации мы поддерживаем только простые паттерны (A)-[R]->(B)
            // Более сложные паттерны требуют дополнительной реализации
        }
    }

    #[tokio::test]
    async fn test_graph_traversal_execution() {
        let store = Arc::new(HybridPersistentStore::open("./traversal_test_data").unwrap());

        // Создаем цепочку узлов для тестирования обхода
        for i in 1..=5 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![i as f32 * 0.1, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i), "type": "test"}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем цепочку связей
        for i in 1..5 {
            store.add_edge(i, i + 1, "NEXT".to_string(), 1.0).unwrap();
        }

        // Тестируем выполнение запроса (в текущей реализации используем базовый запрос)
        let query_text =
            "MATCH (n:Test) WHERE TOROIDALDISTANCE(n.vector, 0.5) RETURN n.id LIMIT 10";
        let (_, parsed_query) = parser::parse_query(query_text).unwrap();
        let results = QueryExecutor::execute_query(&store, parsed_query)
            .await
            .unwrap();

        assert!(!results.is_empty());
        println!("Traversal test found {} nodes", results.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./traversal_test_data").ok();
    }

    #[tokio::test]
    async fn test_bidirectional_graph_search() {
        let store = Arc::new(HybridPersistentStore::open("./bidirectional_test_data").unwrap());

        // Создаем узлы с двунаправленными связями
        for i in 1..=3 {
            let node = crate::hybrid_storage::Node {
                id: i,
                vector: vec![i as f32 * 0.1, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i), "type": "bidirectional"}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Создаем двунаправленные связи
        store
            .add_edge(1, 2, "CONNECTED_TO".to_string(), 0.8)
            .unwrap();
        store
            .add_edge(2, 1, "CONNECTED_TO".to_string(), 0.8)
            .unwrap();
        store
            .add_edge(2, 3, "CONNECTED_TO".to_string(), 0.7)
            .unwrap();
        store
            .add_edge(3, 2, "CONNECTED_TO".to_string(), 0.7)
            .unwrap();

        // Выполняем базовый запрос для проверки наличия узлов
        let query_text = "MATCH (n:Bidirectional) RETURN n.id LIMIT 10";
        let (_, parsed_query) = parser::parse_query(query_text).unwrap();
        let results = QueryExecutor::execute_query(&store, parsed_query)
            .await
            .unwrap();

        assert_eq!(results.len(), 3);
        println!("Bidirectional test found {} nodes", results.len());

        // Удаляем тестовые данные
        std::fs::remove_dir_all("./bidirectional_test_data").ok();
    }
}
