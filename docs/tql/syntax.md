# Синтаксис TQL v2.0

## Полная грамматика

### Основные конструкции

#### MATCH клауза
```
MATCH (alias:Label {property: value})
```

- `alias` - псевдоним узла (любое имя переменной)
- `Label` - метка узла (должна начинаться с буквы)
- `{property: value}` - необязательные свойства для фильтрации

#### WHERE клауза
```
WHERE TOROIDALDISTANCE(field, threshold)
```

- `field` - поле вектора для сравнения (например, `node.vector`)
- `threshold` - порог расстояния (число с плавающей точкой)

#### CONNECTEDTO клауза
```
CONNECTEDTO(target_label, "relationship_type", "property_value")
```

- `target_label` - метка целевого узла
- `"relationship_type"` - тип связи (в кавычках)
- `"property_value"` - значение свойства для фильтрации (необязательно)

#### WITHIN HOPS клауза
```
WITHIN min_hops TO max_hops HOPS
```

- `min_hops` - минимальное количество шагов
- `max_hops` - максимальное количество шагов

#### RETURN клауза
```
RETURN field1, field2, AGGREGATION_FUNCTION(field) AS alias
```

- `field` - поле для возврата
- `AGGREGATION_FUNCTION` - одна из агрегационных функций (COUNT, SUM, AVG, MIN, MAX)
- `AS alias` - необязательный псевдоним для результата

#### ORDER BY клауза
```
ORDER BY field ASC|DESC
```

- `field` - поле для сортировки
- `ASC` - по возрастанию (по умолчанию)
- `DESC` - по убыванию

#### LIMIT клауза
```
LIMIT number
```

- `number` - максимальное количество возвращаемых результатов

### Распределенные запросы

#### DISTRIBUTED префикс
```
DISTRIBUTED MATCH ...
```

Указывает, что запрос должен выполняться распределенно на нескольких шардах.

### Агрегационные функции

- `COUNT(*)` - количество элементов
- `SUM(field)` - сумма значений поля
- `AVG(field)` - среднее значение поля
- `MIN(field)` - минимальное значение поля
- `MAX(field)` - максимальное значение поля

### Примеры полных запросов

#### Простой векторный поиск
```
MATCH (doc:Document) 
WHERE TOROIDALDISTANCE(doc.vector, 0.3) 
RETURN doc.id, doc.score 
LIMIT 10
```

#### Графовый поиск с фильтрацией
```
MATCH (user:User)-[:FRIEND_OF]->(friend:User) 
WHERE friend.age > 18 
RETURN user.name, friend.name 
LIMIT 5
```

#### Специфичный для топологии запрос
```
MATCH (doc:Document) 
CONNECTEDTO(doc, "TAGGED_WITH", "science") 
WITHIN 1 TO 3 HOPS 
RETURN doc.id, doc.title 
LIMIT 20
```

#### Агрегированный запрос
```
MATCH (user:User)-[:LIKES]->(post:Post) 
RETURN user.id, COUNT(post) AS like_count 
ORDER BY like_count DESC 
LIMIT 10
```

#### Распределенный запрос
```
DISTRIBUTED MATCH (item:Item) 
WHERE TOROIDALDISTANCE(item.embedding, 0.2) 
RETURN item.id, item.similarity_score 
LIMIT 50
```

### Транзакции

#### Структура транзакции
```
BEGIN TRANSACTION
-- операции
COMMIT
```

Или:
```
BEGIN TRANSACTION
-- операции
ROLLBACK
```

#### Поддерживаемые операции в транзакциях

- `CREATE (node:Label {properties})` - создание узла
- `CREATE (a)-[:TYPE {properties}]->(b)` - создание ребра
- `UPDATE node SET property = value` - обновление узла
- `DELETE node` - удаление узла