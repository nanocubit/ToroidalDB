# Руководство по TQL v2.0

## Введение

TQL (Toroidal Query Language) v2.0 - это гибридный язык запросов для ToroidalDB, объединяющий возможности SQL, Cypher и векторных запросов. Язык разработан для работы с тороидальной топологией данных, где пространство "заворачивается" на себя.

## Синтаксис

### Основные конструкции

#### MATCH клауза

Определяет паттерны узлов и связей:

```
MATCH (alias:Label)
MATCH (a:User)-[:FOLLOWS]->(b:User)
MATCH (user:User)-[:TAGGED_AS]->(tag:Tag)
```

#### WHERE клауза

Фильтрует результаты:

```
WHERE TOROIDALDISTANCE(field.vector, 0.3)
WHERE property = "value"
WHERE property > 100
```

#### RETURN клауза

Определяет, какие данные возвращать:

```
RETURN node.id
RETURN node.id, node.name
RETURN COUNT(node), AVG(node.score)
```

#### LIMIT клауза

Ограничивает количество результатов:

```
LIMIT 10
```

### Агрегации

TQL v2.0 поддерживает следующие агрегационные функции:

- `COUNT(*)` - подсчет количества
- `SUM(field)` - сумма значений поля
- `AVG(field)` - среднее значение поля
- `MIN(field)` - минимальное значение поля
- `MAX(field)` - максимальное значение поля

Примеры:

```
MATCH (user:User)
RETURN COUNT(user) AS user_count

MATCH (product:Product)
RETURN 
  AVG(product.price) AS avg_price,
  MIN(product.price) AS min_price,
  MAX(product.price) AS max_price,
  SUM(product.quantity) AS total_quantity
```

### Подзапросы

Подзапросы позволяют включать результаты одного запроса в другой:

```
MATCH (user:User)
WHERE user.id IN (
  MATCH (premium:User)
  WHERE user.membership = "premium"
  RETURN user.id
)
RETURN user.name
```

### Транзакции

TQL v2.0 поддерживает ACID транзакции:

```
BEGIN TRANSACTION
CREATE (user:User {name: "Alice", email: "alice@example.com"})
MATCH (group:Group {name: "Developers"})
CREATE (user)-[:MEMBER_OF]->(group)
UPDATE group SET member_count = group.member_count + 1
COMMIT
```

### Распределённые запросы

Для выполнения запросов на нескольких шардах используйте ключевое слово DISTRIBUTED:

```
DISTRIBUTED MATCH (item:Item)
WHERE TOROIDALDISTANCE(item.vector, 0.2)
RETURN item.id, item.score
LIMIT 50
```

### Топологические операции

Специфичные для тороидальной топологии конструкции:

```
MATCH (doc:Document)
CONNECTEDTO(doc, "TAGGED_WITH", "quantum")
WITHIN 2 HOPS
RETURN doc.id, doc.similarity_score
LIMIT 20
```

## Примеры запросов

### Простой векторный поиск

```
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.embedding, 0.3)
RETURN doc.id, doc.score
LIMIT 10
```

### Графовый обход

```
MATCH (user:User)-[:FOLLOWS]->(friend:User)
WHERE friend.active = true
RETURN user.id, friend.id
LIMIT 20
```

### Агрегированный запрос

```
MATCH (user:User)-[:PURCHASED]->(product:Product)
RETURN 
  user.id,
  COUNT(product) AS purchase_count,
  AVG(product.price) AS avg_spent
ORDER BY purchase_count DESC
LIMIT 10
```

### Гибридный запрос (вектор + граф)

```
MATCH (user:User)-[:FRIEND_OF]->(friend:User)
WHERE TOROIDALDISTANCE(friend.profile_vector, 0.25)
RETURN user.id, friend.id, friend.score
ORDER BY friend.score
LIMIT 15
```

### Транзакционный запрос

```
BEGIN TRANSACTION
MATCH (user:User {id: 123})
UPDATE user SET last_seen = "2024-01-01T10:00:00Z", login_count = user.login_count + 1
CREATE (session:Session {user_id: 123, timestamp: "2024-01-01T10:00:00Z"})
CREATE (user)-[:HAS_SESSION]->(session)
COMMIT
```

## Заключение

TQL v2.0 предоставляет мощный и гибкий способ взаимодействия с ToroidalDB, объединяя возможности векторного, графового и реляционного поиска в едином языке запросов.