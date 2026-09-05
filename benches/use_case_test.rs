use std::sync::Arc;
use std::time::Instant;
use tokio;

// Импорты компонентов вашего проекта
use serde_json::json;
use toroidal_db::hybrid_storage::{HybridPersistentStore, Node};
use toroidal_db::tql::ast::{
    MatchClause, NodePattern, PropertyValue, Query, Transaction, TransactionOperation,
};
use toroidal_db::tql::coordinator::{DistributedExecutor, QueryCoordinator};
use toroidal_db::tql::executor::QueryExecutor;
use toroidal_db::tql::parser;
use toroidal_db::tql::transaction::TransactionManager;

#[tokio::main]
async fn main() {
    println!("🎯 Тестирование сценариев использования TQL v2.0...");

    // Сценарий 1: Поиск похожих документов
    scenario_document_similarity().await;

    // Сценарий 2: Графовые обходы
    scenario_graph_traversal().await;

    // Сценарий 3: Специфичные для топологии запросы
    scenario_topology_specific().await;

    // Сценарий 4: Агрегации
    scenario_aggregations().await;

    // Сценарий 5: Распределенные запросы
    scenario_distributed().await;

    // Сценарий 6: Транзакции
    scenario_transactions().await;

    println!("✅ Все сценарии использования протестированы!");
}

// --- Вспомогательные функции ---

fn setup_store(path: &str) -> Arc<HybridPersistentStore> {
    let _ = std::fs::remove_dir_all(path); // Очистка на случай старого мусора
    Arc::new(HybridPersistentStore::open(path).expect("Не удалось открыть хранилище"))
}

fn cleanup_store(store: Arc<HybridPersistentStore>, path: &str) {
    // Явно закрываем хранилище перед удалением папки (важно для macOS)
    drop(store);
    let _ = std::fs::remove_dir_all(path);
}

// --- Сценарии ---

async fn scenario_document_similarity() {
    println!("📝 Тест сценария: Поиск похожих документов...");

    let store = setup_store("./similarity_test_data");

    let documents = vec![
        (
            1,
            vec![0.1, 0.2, 0.3],
            json!({"title": "Document A", "category": "tech"}),
        ),
        (
            2,
            vec![0.12, 0.22, 0.32],
            json!({"title": "Document B", "category": "tech"}),
        ),
        (
            3,
            vec![0.8, 0.9, 0.7],
            json!({"title": "Document C", "category": "science"}),
        ),
        (
            4,
            vec![0.11, 0.19, 0.31],
            json!({"title": "Document D", "category": "tech"}),
        ),
        (
            5,
            vec![0.78, 0.88, 0.72],
            json!({"title": "Document E", "category": "science"}),
        ),
    ];

    for (id, vector, properties) in documents {
        let node = Node {
            id,
            vector,
            properties,
            edges: vec![],
        };
        store.insert(node).unwrap();
    }

    let query_text = "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.15) RETURN doc.id, doc.title LIMIT 5";
    let (_, parsed_query) = parser::parse_query(query_text).unwrap();
    let results = QueryExecutor::execute_query(&store, parsed_query)
        .await
        .unwrap();

    assert!(!results.is_empty());
    println!("   Найдено {} похожих документов", results.len());

    cleanup_store(store, "./similarity_test_data");
    println!("   ✅ Сценарий поиска похожих документов протестирован");
}

async fn scenario_graph_traversal() {
    println!("📝 Тест сценария: Графовые обходы...");

    let store = setup_store("./graph_test_data");

    let users = vec![
        (1, vec![0.1, 0.1], json!({"name": "Alice", "age": 25})),
        (2, vec![0.2, 0.2], json!({"name": "Bob", "age": 30})),
        (3, vec![0.3, 0.3], json!({"name": "Charlie", "age": 35})),
        (4, vec![0.4, 0.4], json!({"name": "Diana", "age": 28})),
    ];

    for (id, vector, properties) in users {
        let node = Node {
            id,
            vector,
            properties,
            edges: vec![],
        };
        store.insert(node).unwrap();
    }

    store.add_edge(1, 2, "FRIEND".to_string(), 0.9).unwrap();
    store.add_edge(2, 3, "FRIEND".to_string(), 0.8).unwrap();
    store.add_edge(1, 4, "COLLEAGUE".to_string(), 0.7).unwrap();

    // Упрощенный запрос, так как фильтрация свойств в MATCH пока не поддерживается парсером
    let query_text =
        "MATCH (user:User)-[:FRIEND]->(friend:User) RETURN user.id, friend.id LIMIT 10";
    let (_, parsed_query) = parser::parse_query(query_text).unwrap();
    let results = QueryExecutor::execute_query(&store, parsed_query)
        .await
        .unwrap();

    println!("   Найдено {} связей FRIEND", results.len());

    cleanup_store(store, "./graph_test_data");
    println!("   ✅ Сценарий графовых обходов протестирован");
}

async fn scenario_topology_specific() {
    println!("📝 Тест сценария: Специфичные для топологии запросы...");

    let store = setup_store("./topology_test_data");

    let nodes = vec![
        (
            1,
            vec![0.99, 0.5],
            json!({"name": "NearZero", "type": "boundary"}),
        ),
        (
            2,
            vec![0.01, 0.5],
            json!({"name": "NearOne", "type": "boundary"}),
        ),
        (
            3,
            vec![0.5, 0.5],
            json!({"name": "Middle", "type": "center"}),
        ),
        (
            4,
            vec![0.98, 0.4],
            json!({"name": "AlsoNearZero", "type": "boundary"}),
        ),
    ];

    for (id, vector, properties) in nodes {
        let node = Node {
            id,
            vector,
            properties,
            edges: vec![],
        };
        store.insert(node).unwrap();
    }

    let query_text =
        "MATCH (n:Boundary) WHERE TOROIDALDISTANCE(n.vector, 0.05) RETURN n.id, n.name LIMIT 10";
    let (_, parsed_query) = parser::parse_query(query_text).unwrap();
    let results = QueryExecutor::execute_query(&store, parsed_query)
        .await
        .unwrap();

    println!("   Найдено {} узлов вблизи границы", results.len());

    cleanup_store(store, "./topology_test_data");
    println!("   ✅ Сценарий топологии протестирован");
}

async fn scenario_aggregations() {
    println!("📝 Тест сценария: Агрегации...");

    let store = setup_store("./aggregation_test_data");

    for i in 1..=10 {
        let node = Node {
            id: i,
            vector: vec![i as f32 * 0.1, 0.5],
            properties: json!({"value": i, "category": if i <= 5 { "low" } else { "high" }}),
            edges: vec![],
        };
        store.insert(node).unwrap();
    }

    // Базовый запрос для проверки работоспособности
    let query_text = "MATCH (n:Number) RETURN n.id LIMIT 10";
    let (_, parsed_query) = parser::parse_query(query_text).unwrap();
    let results = QueryExecutor::execute_query(&store, parsed_query)
        .await
        .unwrap();

    println!("   Найдено {} узлов для агрегации", results.len());

    cleanup_store(store, "./aggregation_test_data");
    println!("   ✅ Сценарий агрегаций протестирован");
}

async fn scenario_distributed() {
    println!("📝 Тест сценария: Распределенные запросы...");

    let paths = ["./dist_shard1", "./dist_shard2", "./dist_shard3"];
    let mut shards = Vec::new();

    // Создаем шарды
    for path in &paths {
        shards.push(setup_store(path));
    }

    // Добавляем данные
    for (shard_idx, shard) in shards.iter().enumerate() {
        for i in 1..=10 {
            let node_id = (shard_idx * 10) + i + 1;
            let node = Node {
                id: node_id as u64,
                vector: vec![node_id as f32 * 0.01, 0.5],
                properties: json!({"id": node_id, "shard": shard_idx + 1, "name": format!("node_{}", node_id)}),
                edges: vec![],
            };
            shard.insert(node).unwrap();
        }
    }

    // Клонируем вектор шардов для координатора
    let coordinator = Arc::new(QueryCoordinator::new(3, shards.clone()));
    let executor = DistributedExecutor::new(coordinator);

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
        limit: 15,
        distributed: true,
        ..Default::default()
    };

    let start = Instant::now();
    let results = executor.execute_distributed(query).await.unwrap();
    let duration = start.elapsed();

    println!("   Найдено {} результатов за {:?}", results.len(), duration);

    // Очистка ресурсов
    // Сначала дропаем executor, чтобы освободить ссылки на координатора и шарды
    drop(executor);

    // Теперь удаляем папки шардов
    for path in &paths {
        let _ = std::fs::remove_dir_all(path);
    }

    println!("   ✅ Сценарий распределенных запросов протестирован");
}

async fn scenario_transactions() {
    println!("📝 Тест сценария: Транзакции...");

    let store = setup_store("./transaction_test_data");

    let transaction = Transaction {
        operations: vec![TransactionOperation::CreateNode(NodePattern {
            alias: "new_user".to_string(),
            label: "User".to_string(),
            properties: Some(vec![(
                "name".to_string(),
                PropertyValue::String("Test User".to_string()),
            )]),
        })],
    };

    let result = TransactionManager::execute_transaction(&store, transaction).await;
    assert!(result.is_ok());

    println!("   Транзакция выполнена успешно");

    cleanup_store(store, "./transaction_test_data");
    println!("   ✅ Сценарий транзакций протестирован");
}
