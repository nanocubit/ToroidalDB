use std::sync::Arc;
use tokio;

#[tokio::test]
async fn test_complete_extended_functionality() {
    use crate::hybrid_storage::HybridPersistentStore;
    use crate::tql::ast::*;
    use crate::tql::executor::QueryExecutor;
    use serde_json::json;

    println!("🧪 Тестирование полной расширенной функциональности...");

    // Создаем хранилище для тестирования
    let store = Arc::new(HybridPersistentStore::open("./complete_ext_test_data").unwrap());

    // Создаем тестовые узлы
    for i in 1..=50 {
        let node = crate::hybrid_storage::Node {
            id: i as u64,
            vector: vec![(i as f32 * 0.02) % 1.0, 0.5],
            properties: json!({
                "id": i,
                "name": format!("node_{}", i),
                "value": i as f64,
                "category": if i % 2 == 0 { "even" } else { "odd" }
            }),
            edges: vec![],
        };
        store.insert(node).unwrap();
    }

    // Создаем связи между узлами
    for i in 1..50 {
        if i + 1 <= 50 {
            store.add_edge(i, i + 1, "NEXT".to_string(), 0.8).unwrap();
        }
        if i + 5 <= 50 {
            store.add_edge(i, i + 5, "JUMP".to_string(), 0.6).unwrap();
        }
    }

    // Тест 1: Агрегации
    println!("  - Тест агрегаций...");
    let aggregation_query = Query {
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
        return_fields: vec![],
        aggregation_fields: vec![
            AggregationField {
                function: AggregationFunction::Count,
                alias: Some("total_count".to_string()),
            },
            AggregationField {
                function: AggregationFunction::Sum("value".to_string()),
                alias: Some("sum_values".to_string()),
            },
            AggregationField {
                function: AggregationFunction::Avg("value".to_string()),
                alias: Some("avg_value".to_string()),
            },
            AggregationField {
                function: AggregationFunction::Min("value".to_string()),
                alias: Some("min_value".to_string()),
            },
            AggregationField {
                function: AggregationFunction::Max("value".to_string()),
                alias: Some("max_value".to_string()),
            },
        ],
        order_by: None,
        subqueries: vec![],
        transaction: None,
        limit: 10,
        distributed: false,
    };

    let aggregation_results = QueryExecutor::execute_query(&store, aggregation_query)
        .await
        .unwrap();
    assert!(!aggregation_results.is_empty());
    println!(
        "    ✅ Агрегации: {} результатов",
        aggregation_results.len()
    );

    // Тест 2: Транзакции
    println!("  - Тест транзакций...");
    let transaction = Transaction {
        operations: vec![TransactionOperation::CreateNode(NodePattern {
            alias: "new_node".to_string(),
            label: "Test".to_string(),
            properties: Some(vec![
                (
                    "name".to_string(),
                    PropertyValue::String("New Test Node".to_string()),
                ),
                ("value".to_string(), PropertyValue::Number(999.0)),
            ]),
        })],
    };

    let transaction_result = QueryExecutor::execute_transaction(&store, transaction).await;
    assert!(transaction_result.is_ok());
    println!("    ✅ Транзакции: успешно выполнены");

    // Тест 3: Распределённый запрос (с использованием локальных шардов)
    println!("  - Тест распределённого выполнения...");
    let shards = vec![store.clone(), store.clone(), store.clone()];
    let coordinator = crate::tql::QueryCoordinator::new(3, shards);
    let distributed_executor = crate::tql::DistributedExecutor::new(Arc::new(coordinator));

    let distributed_query = Query {
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

    let dist_results = distributed_executor
        .execute_distributed(distributed_query)
        .await
        .unwrap();
    assert!(!dist_results.is_empty());
    println!(
        "    ✅ Распределённое выполнение: {} результатов",
        dist_results.len()
    );

    // Удаляем тестовые данные
    std::fs::remove_dir_all("./complete_ext_test_data").ok();

    println!("✅ Все компоненты расширенной функциональности работают корректно!");
}
