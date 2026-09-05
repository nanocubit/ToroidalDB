use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

// Импорты модулей вашего проекта.
// Убедитесь, что этот файл находится внутри src/, иначе используйте имя вашего крейта вместо `crate::`
// Импорты модулей вашего проекта.
// Убедитесь, что этот файл находится внутри src/, иначе используйте имя вашего крейта вместо `crate::`
use toroidal_db::hybrid_storage::{HybridPersistentStore, Node};
use toroidal_db::tql::executor::QueryExecutor;
use toroidal_db::tql::parser;

#[tokio::main]
async fn main() {
    println!("🔍 Начинаем тестирование стабильности и поиск багов...");

    // Тест 1: Граничные условия
    test_boundary_conditions();

    // Тест 2: Обработка ошибок
    test_error_handling();

    // Тест 3: Долгосрочный запуск
    test_long_running().await;

    // Тест 4: Проверка утечек памяти
    test_memory_leaks().await;

    // Тест 5: Параллельная обработка
    test_parallel_processing().await;

    println!("✅ Тестирование стабильности завершено!");
}

// Вспомогательная функция для создания тестового хранилища
fn setup_test_store(path: &str, node_count: usize) -> Arc<HybridPersistentStore> {
    // Очистка перед созданием, если вдруг остался мусор от прошлого прогона
    let _ = std::fs::remove_dir_all(path);

    let store = Arc::new(HybridPersistentStore::open(path).expect("Не удалось открыть хранилище"));

    for i in 1..=node_count {
        let node = Node {
            id: i as u64,
            vector: vec![i as f32 * 0.01, 0.5],
            properties: json!({"id": i, "name": format!("node_{}", i)}),
            edges: vec![],
        };
        store.insert(node).expect("Не удалось вставить ноду");
    }
    store
}

fn test_boundary_conditions() {
    println!("🧪 Тест граничных условий...");

    // Тест пустого запроса
    let empty_result = parser::parse_query("");
    assert!(
        empty_result.is_err(),
        "Пустой запрос должен возвращать ошибку"
    );

    // Тест очень длинного запроса
    let long_query = "MATCH (n:Node) ".repeat(1000) + "RETURN n.id LIMIT 1";
    let long_result = parser::parse_query(&long_query);
    assert!(
        long_result.is_err(),
        "Огромный невалидный запрос должен возвращать ошибку"
    );

    // Тест минимального корректного запроса
    let minimal_query = "MATCH (n:Node) RETURN n.id LIMIT 1";
    let minimal_result = parser::parse_query(minimal_query);
    assert!(
        minimal_result.is_ok(),
        "Минимальный запрос должен быть валидным"
    );

    println!("   ✅ Граничные условия протестированы");
}

fn test_error_handling() {
    println!("🧪 Тест обработки ошибок...");

    let invalid_queries = vec![
        "INVALID SYNTAX",
        "MATCH () RETURN",
        "MATCH (n:Node WHERE RETURN n.id",
        "MATCH (n:Node) RETURN n.id LIMIT abc",
        "MATCH (n:Node) WHERE TOROIDALDISTANCE() RETURN n.id",
    ];

    for query in invalid_queries {
        let result = parser::parse_query(query);
        assert!(
            result.is_err(),
            "Query '{}' должен вернуть ошибку, но получил {:?}",
            query,
            result
        );
    }

    println!("   ✅ Обработка ошибок протестирована");
}

async fn test_long_running() {
    println!("🧪 Тест долгосрочного запуска...");

    let store = setup_test_store("./long_run_test_data", 100);
    let query_text = "MATCH (n:Node) WHERE TOROIDALDISTANCE(n.vector, 0.2) RETURN n.id LIMIT 5";

    // Парсим запрос один раз перед циклом
    let parsed_query = parser::parse_query(query_text)
        .expect("Ошибка парсинга в долгосрочном тесте")
        .1; // Предполагаем, что парсер возвращает кортеж (остаток, запрос)

    let start_time = std::time::Instant::now();
    let duration = Duration::from_secs(5);

    while start_time.elapsed() < duration {
        let result = QueryExecutor::execute_query(&store, parsed_query.clone()).await;
        assert!(
            result.is_ok(),
            "Ошибка выполнения при долгосрочном тесте: {:?}",
            result
        );
        sleep(Duration::from_millis(10)).await;
    }

    println!("   ✅ Долгосрочный запуск протестирован");

    // Очистка
    let _ = std::fs::remove_dir_all("./long_run_test_data");
}

async fn test_memory_leaks() {
    println!("🧪 Тест на утечки памяти...");

    let store = setup_test_store("./memory_test_data", 50);

    for i in 0..1000 {
        let query_text = format!(
            "MATCH (n:Node) WHERE TOROIDALDISTANCE(n.vector, 0.{}) RETURN n.id LIMIT 1",
            (i % 5) + 1
        );

        if let Ok((_, parsed_query)) = parser::parse_query(&query_text) {
            let _ = QueryExecutor::execute_query(&store, parsed_query).await;
        }

        if i % 100 == 0 {
            println!("   Выполнено {} итераций проверки утечек", i);
        }
    }

    println!("   ✅ Проверка утечек памяти завершена");

    // Очистка
    let _ = std::fs::remove_dir_all("./memory_test_data");
}

async fn test_parallel_processing() {
    println!("🧪 Тест параллельной обработки...");

    let store = setup_test_store("./parallel_test_data", 100);
    let query_text = "MATCH (n:Node) WHERE TOROIDALDISTANCE(n.vector, 0.3) RETURN n.id LIMIT 3";

    let parsed_query = parser::parse_query(query_text)
        .expect("Ошибка парсинга в параллельном тесте")
        .1;

    let num_tasks = 50;
    let mut handles = Vec::new();

    for _ in 0..num_tasks {
        let store_clone = Arc::clone(&store);
        let query_clone = parsed_query.clone();

        let handle =
            tokio::spawn(
                async move { QueryExecutor::execute_query(&store_clone, query_clone).await },
            );

        handles.push(handle);
    }

    // Ждем завершения всех задач
    for handle in handles {
        let result = handle.await;
        assert!(result.is_ok(), "Panic в одной из параллельных задач");
        assert!(
            result.unwrap().is_ok(),
            "Ошибка выполнения запроса в параллельной задаче"
        );
    }

    println!("   ✅ Параллельная обработка протестирована");

    // Очистка
    let _ = std::fs::remove_dir_all("./parallel_test_data");
}
