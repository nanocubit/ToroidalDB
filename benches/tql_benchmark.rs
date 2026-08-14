use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;
use std::sync::Arc;
use serde_json::json;

// Импорты вашего проекта
use crate::storage::{Node, PersistentStore};
use crate::tql::ast::{Query, MatchClause, NodePattern, AggregationField, AggregationFunction};
use crate::tql::executor::QueryExecutor;

fn benchmark_local_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let path = "./benchmark_local_data";

    // 1. SETUP (Создаем данные ОДИН раз)
    let store = rt.block_on(async {
        // Очистка перед стартом на случай старого мусора
        let _ = std::fs::remove_dir_all(path);
        
        let store = Arc::new(PersistentStore::open(path).unwrap());
        
        // Наполняем данными
        for i in 1..=1000 {
            let node = Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.001) % 1.0, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }
        store
    });

    // Подготовка запроса (выносим создание структуры, чтобы не тратить на это время)
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

    // 2. MEASUREMENT (Только исполнение)
    c.bench_function("local_search_1000_nodes", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Клонируем Arc, это дешево
                let _results = QueryExecutor::execute_query(&store, black_box(query.clone())).await.unwrap();
            })
        })
    });

    // 3. TEARDOWN (Очистка)
    // Явно дропаем хранилище, чтобы закрыть файлы
    drop(store);
    let _ = std::fs::remove_dir_all(path);
}

fn benchmark_aggregation_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let path = "./benchmark_agg_data";

    let store = rt.block_on(async {
        let _ = std::fs::remove_dir_all(path);
        let store = Arc::new(PersistentStore::open(path).unwrap());

        for i in 1..=1000 {
            let node = Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.001) % 1.0, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i), "value": i as f64}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }
        store
    });

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
        return_fields: vec![],
        aggregation_fields: vec![AggregationField {
            function: AggregationFunction::Count,
            alias: Some("count".to_string()),
        }],
        order_by: None,
        subqueries: vec![],
        transaction: None,
        limit: 1,
        distributed: false,
    };

    c.bench_function("aggregation_count_1000_nodes", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _results = QueryExecutor::execute_query(&store, black_box(query.clone())).await.unwrap();
            })
        })
    });

    drop(store);
    let _ = std::fs::remove_dir_all(path);
}

fn benchmark_distributed_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let paths = ["./benchmark_shard1", "./benchmark_shard2", "./benchmark_shard3"];

    // SETUP: Создаем шарды и данные
    let (shards, executor) = rt.block_on(async {
        let mut shards_vec = Vec::new();
        
        for path in &paths {
            let _ = std::fs::remove_dir_all(path);
            let store = Arc::new(PersistentStore::open(path).unwrap());
            shards_vec.push(store);
        }

        // Заполняем данные
        for (shard_idx, shard) in shards_vec.iter().enumerate() {
            for i in 1..=500 {
                let node_id = (shard_idx * 500) + i + 1; // +1 чтобы ID не пересекались, если нужно
                let node = Node {
                    id: node_id as u64,
                    vector: vec![(node_id as f32 * 0.001) % 1.0, 0.5],
                    properties: json!({
                        "id": node_id,
                        "shard": shard_idx + 1,
                        "name": format!("shard{}_node_{}", shard_idx + 1, i)
                    }),
                    edges: vec![],
                };
                shard.insert(node).unwrap();
            }
        }

        let coordinator = Arc::new(crate::tql::QueryCoordinator::new(3, shards_vec.clone()));
        let executor = crate::tql::DistributedExecutor::new(coordinator);

        (shards_vec, executor)
    });

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

    c.bench_function("distributed_search_3_shards", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Executor обычно клонируется или Arc внутри, проверьте ваш API
                // Здесь предполагаем, что executor можно переиспользовать
                let _results = executor.execute_distributed(black_box(query.clone())).await.unwrap();
            })
        })
    });

    // TEARDOWN
    drop(shards); // Закрываем шарды
    for path in &paths {
        let _ = std::fs::remove_dir_all(path);
    }
}

// Изменен на "cached_search", так как бенчмаркинг "холодного" старта 
// требует полного пересоздания хранилища на каждой итерации, что делает тест очень медленным.
// Здесь мы измеряем скорость работы с кэшем (Hot Path).
fn benchmark_cached_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let path = "./benchmark_cache_data";

    let store = rt.block_on(async {
        let _ = std::fs::remove_dir_all(path);
        let store = Arc::new(PersistentStore::open(path).unwrap());

        for i in 1..=500 {
            let node = Node {
                id: i as u64,
                vector: vec![(i as f32 * 0.002) % 1.0, 0.5],
                properties: json!({"id": i, "name": format!("node_{}", i)}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }
        store
    });

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
        limit: 5,
        distributed: false,
    };

    // Прогревочный запуск (warm-up), чтобы убедиться, что данные в кэше
    rt.block_on(async {
        let _ = QueryExecutor::execute_query(&store, query.clone()).await.unwrap();
    });

    c.bench_function("cached_search_hot_path", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _results = QueryExecutor::execute_query(&store, black_box(query.clone())).await.unwrap();
            })
        })
    });

    drop(store);
    let _ = std::fs::remove_dir_all(path);
}

criterion_group!(
    benches,
    benchmark_local_search,
    benchmark_aggregation_operations,
    benchmark_distributed_search,
    benchmark_cached_search,
);
criterion_main!(benches);