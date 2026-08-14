# TQL v2.1 - Документация

## Обзор

TQL v2.1 добавляет типизацию данных, схемы и интроспекцию запросов к существующему TQL v2.0.

## Что реализовано

### 1. DDL (Data Definition Language)

**Файл**: `src/tql/ast.rs` (строки 130-177)

Новые типы для DDL операторов:
- `Statement` - объединяющий enum для всех типов запросов (Query, Explain, DDL)
- `DdlStatement` - DDL операции (CreateNodeType, CreateEdgeType, Drop, ShowSchema)
- `NodeTypeDef` / `EdgeTypeDef` - определения типов
- `FieldDef` - определение полей с типами данных
- `DataType` - поддерживаемые типы: Int, Float, Bool, Text, Timestamp, Vector(n)
- `FieldConstraint` - ограничения: NotNull, Unique, Index, VectorIndex

**Примеры синтаксиса**:

```tql
-- Создание типа узла
CREATE NODE TYPE Document (
    id          INT PRIMARY KEY,
    title       TEXT NOT NULL,
    content_t3  VECTOR(1536) VECTOR_INDEX(phi = 5.71),
    created_at  TIMESTAMP
);

-- Создание типа связи
CREATE EDGE TYPE AUTHORED (
    from Author,
    to Document,
    weight FLOAT
);

-- Удаление типов
DROP NODE TYPE Document;
DROP EDGE TYPE AUTHORED;

-- Просмотр схемы
SHOW SCHEMA;
```

### 2. DDL Parser

**Файл**: `src/tql/ddl_parser.rs`

Полноценный парсер на nom для всех DDL операций:
- `parse_ddl_statement()` - точка входа
- `parse_create_node_type()` - парсинг CREATE NODE TYPE
- `parse_create_edge_type()` - парсинг CREATE EDGE TYPE
- `parse_data_type()` - поддержка всех типов данных
- `parse_field_constraint()` - парсинг ограничений полей

**Особенности**:
- Поддержка PRIMARY KEY
- VECTOR с размерностью
- VECTOR_INDEX с параметром phi
- NOT NULL, UNIQUE, INDEX constraints

### 3. SchemaRegistry

**Файл**: `src/tql/schema.rs`

Центральный реестр для управления схемами данных:

```rust
pub struct SchemaRegistry {
    nodes: RwLock<HashMap<String, NodeTypeDef>>,
    edges: RwLock<HashMap<String, EdgeTypeDef>>,
}
```

**Методы**:
- `create_node_type()` / `create_edge_type()` - регистрация типов
- `drop_node_type()` / `drop_edge_type()` - удаление типов
- `get_node_type()` / `get_edge_type()` - получение определений
- `validate_field()` - проверка существования поля
- `validate_vector_field()` - проверка векторных полей с размерностью
- `list_node_types()` / `list_edge_types()` - интроспекция

**Валидации**:
- Проверка дублирования типов
- Проверка ссылочной целостности (edge types ссылаются на существующие node types)
- Проверка множественных PRIMARY KEY
- Валидация размерностей векторов

### 4. LogicalPlan и EXPLAIN

**Файл**: `src/tql/planner.rs`

Система планирования запросов:

```rust
pub struct LogicalPlan {
    pub steps: Vec<PlanStep>,
    pub estimated_cost: f64,
    pub estimated_rows: u64,
}
```

**Типы шагов плана**:
- `RouteShards` - маршрутизация на шарды
- `LocalVectorSearch` - локальный векторный поиск
- `GraphTraversal` - обход графа
- `TopKMerge` - слияние top-K результатов
- `Filter` - применение фильтров
- `Aggregate` - агрегации
- `Sort` - сортировка
- `Project` - проекция полей

**PlanBuilder**:
```rust
let plan = PlanBuilder::build(&query);
```

Автоматически строит план на основе:
- TOROIDALDISTANCE → LocalVectorSearch
- CONNECTEDTO/WITHIN → GraphTraversal
- Aggregation → Aggregate
- ORDER BY → Sort

**EXPLAIN**:
```tql
EXPLAIN MATCH (d:Document) 
WHERE TOROIDALDISTANCE(content_t3, 0.3) 
RETURN d.id 
LIMIT 10;
```

Выводит:
```
QUERY PLAN:
Estimated cost: 100.00
Estimated rows: 100

1. ROUTE_SHARDS strategy=HashRing
2. LOCAL_VECTOR_SEARCH field=content_t3 threshold=0.3 limit=10 backend=Auto
3. TOP_K_MERGE k=10
```

### 5. TqlEngine

**Файл**: `src/tql/engine.rs`

Унифицированный движок для выполнения всех типов TQL операций:

```rust
pub struct TqlEngine {
    schema_registry: Arc<SchemaRegistry>,
}

impl TqlEngine {
    pub async fn execute(&self, sql: &str) -> Result<TqlResult, TqlError>;
}
```

**Поддерживает**:
- DDL операции (CREATE/DROP/SHOW)
- EXPLAIN для query plans
- Валидацию запросов по схеме
- Проверку существования типов
- Валидацию векторных полей

**Результаты**:
```rust
pub enum TqlResult {
    Query(Vec<QueryResult>),
    Explain(String),
    DdlSuccess(String),
}
```

## Примеры использования

### Создание схемы

```rust
use toroidal_db::tql::TqlEngine;

let engine = TqlEngine::new();

// Создаем тип Document
let ddl = r#"
    CREATE NODE TYPE Document (
        id INT PRIMARY KEY,
        title TEXT NOT NULL,
        embedding VECTOR(768) VECTOR_INDEX(phi = 5.71)
    )
"#;

let result = engine.execute(ddl).await?;
```

### Валидация запросов

```rust
// Это вызовет ошибку - тип не существует
let bad_query = "MATCH (u:UnknownType) RETURN u.id LIMIT 10";
let result = engine.execute(bad_query).await;
assert!(result.is_err());
```

### EXPLAIN

```rust
let explain = r#"
    EXPLAIN MATCH (d:Document) 
    WHERE TOROIDALDISTANCE(embedding, 0.3) 
    RETURN d.id 
    LIMIT 10
"#;

let result = engine.execute(explain).await?;
if let TqlResult::Explain(plan) = result {
    println!("{}", plan);
}
```

### Интроспекция

```rust
// Получить список всех типов
let schema = engine.execute("SHOW SCHEMA").await?;

// Проверить поле
let field_type = engine.schema_registry()
    .validate_field("Document", "embedding")?;
    
// Проверить векторное поле
let dim = engine.schema_registry()
    .validate_vector_field("Document", "embedding", Some(768))?;
```

## Архитектура интеграции

```
┌─────────────────────────────────────────────┐
│           TqlEngine                         │
│  ┌──────────────┐  ┌────────────────────┐  │
│  │   DDL Parser │  │   Query Parser     │  │
│  └──────┬───────┘  └─────────┬──────────┘  │
│         │                    │             │
│  ┌──────▼───────┐  ┌─────────▼──────────┐  │
│  │SchemaRegistry│  │   PlanBuilder      │  │
│  │              │  │   (EXPLAIN)        │  │
│  └──────┬───────┘  └─────────┬──────────┘  │
│         │                    │             │
│  ┌──────▼────────────────────▼──────────┐  │
│  │         Validation Layer             │  │
│  │  - Check type exists                 │  │
│  │  - Validate fields                   │  │
│  │  - Check vector dimensions           │  │
│  └──────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

## Следующие шаги (v2.2)

Для перехода к TQL v2.2 нужно:

1. **Исправить QueryExecutor** - обновить для работы с новой схемой
2. **Добавить Cost-based Optimizer** - использовать статистику из SchemaRegistry
3. **Hints** - `USING GPU`, `SCATTER N SHARDS`
4. **Backend Selection** - автовыбор AVX2/AVX512/GPU

## Тесты

Все компоненты имеют unit-тесты:
- `schema.rs` - тесты валидации, создания/удаления типов
- `ddl_parser.rs` - тесты парсинга всех DDL операций
- `planner.rs` - тесты построения планов
- `engine.rs` - интеграционные тесты

Запуск тестов:
```bash
cargo test tql::schema
cargo test tql::ddl_parser
cargo test tql::planner
cargo test tql::engine
```

## Примечания

- `QueryExecutor` временно отключен из-за несовместимости с текущим API storage
- Для полной интеграции необходимо обновить зависимости проекта
- TQL v2.1 готов к использованию для DDL операций и EXPLAIN
