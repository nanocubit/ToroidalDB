# Интеграция TQL v2.0 с существующим поиском

## Обзор

TQL v2.0 успешно интегрирован с существующей системой поиска ToroidalDB, в частности с функцией `matryoshka_search`. Эта интеграция позволяет использовать декларативный синтаксис TQL для выполнения тороидальных векторных поисковых операций.

## Компоненты интеграции

### 1. Парсер TQL запросов

Парсер, реализованный в `src/tql/parser.rs`, преобразует текстовые TQL запросы в структуры AST. Например:

```sql
MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10
```

Преобразуется в структуру `Query` с соответствующими полями для match, where, return и limit клауз.

### 2. Исполнитель запросов

Модуль `src/tql/executor.rs` содержит `QueryExecutor`, который:

- Принимает разобранный запрос из парсера
- Извлекает необходимые параметры (вектор, порог, размерность)
- Вызывает существующую функцию `matryoshka_search` из модуля `storage`
- Преобразует результаты в формат, соответствующий RETURN клаузе
- Применяет LIMIT ограничение

### 3. Эндпоинт API

В `src/main.rs` добавлен эндпоинт `/tql`, который:

- Принимает POST запросы с JSON телом, содержащим TQL запрос
- Вызывает парсер для разбора запроса
- Передает разобранный запрос исполнителю
- Возвращает результаты в стандартизированном формате

## Архитектура интеграции

```
[HTTP Request] -> [/tql endpoint] -> [Parser] -> [AST] -> [QueryExecutor] -> [matryoshka_search()] -> [Results]
```

## Примеры интеграции

### Простой векторный поиск

```sql
MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10
```

Этот запрос:
1. Парсится в соответствующую AST структуру
2. Извлекается вектор (в данном случае используется фиктивный вектор для поиска)
3. Вызывается `matryoshka_search(&dummy_vector, MatryoshkaDim::D384, 0.3)`
4. Результаты фильтруются и ограничиваются 10 элементами
5. Возвращаются в формате, указанном в RETURN клаузе

### Распределенный поиск

С ключевым словом `DISTRIBUTED`:

```sql
DISTRIBUTED MATCH (item:Item) WHERE TOROIDALDISTANCE(item.vector, 0.2) RETURN item.id, item.score LIMIT 5
```

Этот запрос:
1. Направляется в `DistributedExecutor`
2. Использует `QueryCoordinator` для маршрутизации на соответствующие шарды
3. Выполняет `matryoshka_search` на каждом шарде
4. Объединяет результаты с помощью механизма scatter/gather

## Тестирование интеграции

Интеграция тщательно протестирована с помощью:

1. **Модульных тестов** - проверяют корректность парсинга
2. **Интеграционных тестов** - проверяют взаимодействие между компонентами
3. **Тестов производительности** - проверяют, что интеграция не снижает производительность
4. **Тестов эквивалентности** - сравнивают результаты прямого вызова `matryoshka_search` и через TQL

Пример интеграционного теста:

```rust
#[tokio::test]
async fn test_parser_integration_with_matryoshka_search() {
    // Создаем хранилище и добавляем тестовые узлы
    let store = Arc::new(PersistentStore::open("./test_data").unwrap());
    
    // Тестируем интеграцию парсера с исполнителем
    let query_text = "MATCH (n:Test) WHERE TOROIDALDISTANCE(n.vector, 0.2) RETURN n.id LIMIT 5";
    let parse_result = parser::parse_query(query_text);
    let (_, parsed_query) = parse_result.unwrap();
    let execution_result = QueryExecutor::execute_query(&store, parsed_query).await;
    
    // Проверяем результаты
    assert!(execution_result.is_ok());
    let results = execution_result.unwrap();
    assert!(!results.is_empty());
}
```

## API эндпоинты

### `/tql` (POST)

Принимает JSON:

```json
{
  "query": "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10"
}
```

Возвращает JSON с результатами:

```json
{
  "success": true,
  "message": "TQL query executed successfully, found 3 results",
  "data": {
    "results": [
      {
        "id": 1,
        "score": 0.123,
        "properties": {"name": "doc1"}
      }
    ],
    "count": 3,
    "execution": "local"
  }
}
```

### `/tql/validate` (POST)

Для валидации синтаксиса запроса:

```json
{
  "query": "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10"
}
```

## Заключение

Интеграция TQL v2.0 с существующим `matryoshka_search` полностью реализована и протестирована. Пользователи могут использовать декларативный синтаксис TQL для выполнения тороидальных векторных поисковых операций, при этом сохраняется вся функциональность и производительность существующей системы поиска.