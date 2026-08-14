# Расширенная функциональность TQL v2.0

## Обзор

TQL v2.0 включает расширенную функциональность, включающую агрегации, подзапросы и транзакции. Эти возможности позволяют выполнять сложные аналитические операции и обеспечивать атомарность операций с данными.

## Агрегации

### Поддерживаемые агрегации

TQL v2.0 поддерживает следующие агрегационные функции:

- `COUNT(*)` - подсчет количества элементов
- `SUM(field)` - сумма значений поля
- `AVG(field)` - среднее значение поля
- `MIN(field)` - минимальное значение поля
- `MAX(field)` - максимальное значение поля

### Синтаксис агрегаций

```
MATCH (node:Label)
RETURN COUNT(node), SUM(node.value), AVG(node.score), MIN(node.age), MAX(node.rating)
```

### Примеры запросов с агрегациями

#### Подсчет количества узлов
```
MATCH (user:User)
RETURN COUNT(user) AS user_count
```

#### Статистика по числовым полям
```
MATCH (product:Product)
RETURN 
  COUNT(product) AS total_products,
  AVG(product.price) AS average_price,
  MIN(product.price) AS min_price,
  MAX(product.price) AS max_price,
  SUM(product.quantity) AS total_quantity
```

#### Агрегации с фильтрацией
```
MATCH (order:Order)
WHERE order.amount > 100
RETURN 
  COUNT(order) AS high_value_orders,
  AVG(order.amount) AS avg_high_value
```

## Подзапросы

### Синтаксис подзапросов

Подзапросы позволяют включать результаты одного запроса в другой:

```
MATCH (user:User)
WHERE user.id IN (
  MATCH (premium:User) 
  WHERE premium.membership = "premium" 
  RETURN premium.id
)
RETURN user.name
```

### Реализация подзапросов

В текущей реализации подзапросы представлены в AST структуре:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubQuery {
    pub query: Box<Query>,
    pub alias: String,
}
```

### Примеры использования подзапросов

#### Поиск пользователей по результатам подзапроса
```
MATCH (follower:User)
WHERE follower.id IN (
  MATCH (target:User)-[:FOLLOWS]->(celebrity:Celebrity)
  RETURN target.id
)
RETURN follower.name
```

#### Подзапросы с агрегациями
```
MATCH (user:User)
WHERE user.age > (
  MATCH (all_users:User)
  RETURN AVG(all_users.age)
)
RETURN user.name, user.age
```

## Транзакции

### Атомарные операции

TQL v2.0 поддерживает транзакции для обеспечения атомарности операций:

- `CREATE` - создание узлов
- `UPDATE` - обновление узлов
- `DELETE` - удаление узлов
- `CREATE EDGE` - создание рёбер

### Синтаксис транзакций

```
BEGIN TRANSACTION
CREATE (user:User {name: "John", age: 30})
MATCH (existing:User {id: 1})
CREATE (existing)-[:FRIENDS_WITH]->(user)
UPDATE existing SET last_interaction = "2024-01-01"
COMMIT
```

### Реализация транзакций

В системе реализованы следующие операции транзакций:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionOperation {
    CreateNode(NodePattern),
    UpdateNode(NodePattern, Vec<(String, PropertyValue)>),
    DeleteNode(NodePattern),
    CreateEdge(String, String, String), // source, target, relationship
}
```

### Примеры транзакций

#### Простая транзакция создания
```
BEGIN TRANSACTION
CREATE (user:User {name: "Alice", email: "alice@example.com"})
CREATE (profile:Profile {user_id: user.id, bio: "Software Engineer"})
CREATE (user)-[:HAS_PROFILE]->(profile)
COMMIT
```

#### Транзакция с обновлением
```
BEGIN TRANSACTION
MATCH (user:User {id: 123})
UPDATE user SET last_login = "2024-01-01", login_count = login_count + 1
CREATE (session:Session {user_id: 123, timestamp: "2024-01-01T10:00:00Z"})
CREATE (user)-[:HAS_SESSION]->(session)
COMMIT
```

#### Транзакция с удалением
```
BEGIN TRANSACTION
MATCH (user:User {id: 456})
MATCH (user)-[r:FRIENDS_WITH]-(friend:User)
DELETE r
DELETE user
COMMIT
```

## Комбинация возможностей

### Агрегации в транзакциях

Можно использовать агрегации в условиях транзакций:

```
BEGIN TRANSACTION
MATCH (group:Group)
WHERE COUNT(group.members) > 100
MATCH (group)-[:HAS_MEMBER]->(inactive:User)
WHERE inactive.last_active < "2023-01-01"
DELETE inactive
COMMIT
```

### Подзапросы с агрегациями

Комбинирование подзапросов и агрегаций:

```
MATCH (user:User)
WHERE user.id IN (
  MATCH (active:User)
  WHERE active.login_count > (
    MATCH (all:User)
    RETURN AVG(all.login_count) * 2
  )
  RETURN active.id
)
RETURN user.name, user.login_count
```

## Производительность и оптимизация

### Агрегации
- Выполняются на уровне шардов в распределённой системе
- Результаты объединяются на координаторе
- Поддерживают кэширование

### Подзапросы
- Оптимизируются планировщиком запросов
- Могут быть преобразованы в JOIN операции
- Поддерживают индексацию

### Транзакции
- Используют двухфазный коммит в распределённой системе
- Поддерживают изоляцию уровня SERIALIZABLE
- Включают механизмы восстановления

## Ограничения и будущие улучшения

### Текущие ограничения
- Подзапросы поддерживаются только в WHERE клаузах
- Вложенные транзакции не поддерживаются
- Некоторые комбинации агрегаций могут быть неэффективны

### Будущие улучшения
- Поддержка рекурсивных подзапросов
- Улучшенная оптимизация плана выполнения
- Расширенная поддержка оконных функций

## Заключение

Расширенная функциональность TQL v2.0 позволяет выполнять сложные аналитические операции, обеспечивает атомарность изменений данных и поддерживает сложные бизнес-логики. Комбинация агрегаций, подзапросов и транзакций делает TQL мощным инструментом для анализа и управления сложными графовыми и векторными данными.