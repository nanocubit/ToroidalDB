# TQL v2.0 - Документация

TQL (Toroidal Query Language) v2.0 - это гибридный язык запросов для ToroidalDB, сочетающий возможности векторного, графового и реляционного поиска с учетом тороидальной топологии пространства.

## Содержание

1. [Синтаксис](syntax.md) - полная грамматика с примерами
2. [Руководство для новичков](tutorial.md) - пошаговое руководство
3. [Оптимизация запросов](performance.md) - рекомендации по производительности
4. [Расширенные возможности](advanced.md) - агрегации, транзакции, подзапросы

## Особенности TQL v2.0

- **Гибридный поиск**: сочетание векторного и графового поиска
- **Тороидальная топология**: учет циклических зависимостей в данных
- **Распределенные запросы**: масштабирование на несколько узлов
- **Агрегации**: COUNT, SUM, AVG, MIN, MAX и другие функции
- **Транзакции**: атомарные операции с данными
- **Подзапросы**: вложенные выражения для сложных сценариев

## Примеры запросов

### Простой поиск
```
MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10
```

### Графовый обход
```
MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN a.id, b.id
```

### Специфичный для топологии запрос
```
MATCH (doc:Document)
CONNECTEDTO(doc, "TAGGED_WITH", "quantum")
WITHIN 2 HOPS
RETURN doc.id
LIMIT 20
```

### Агрегация
```
MATCH (user:User)-[:LIKES]->(content:Content)
RETURN user.id, COUNT(content) as likes
ORDER BY likes DESC
LIMIT 10
```

### Распределенный запрос
```
DISTRIBUTED MATCH (node:Label) WHERE TOROIDALDISTANCE(node.vector, 0.4) RETURN node.id, node.score LIMIT 5
```

### Транзакция
```
BEGIN TRANSACTION
MATCH (a:User {id: 1}), (b:User {id: 2})
CREATE (a)-[:FOLLOWS {since: "2024-01-01"}]->(b)
COMMIT
```