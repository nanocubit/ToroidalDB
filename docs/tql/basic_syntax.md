# Базовый синтаксис TQL v2.0

## Введение

TQL (Toroidal Query Language) v2.0 предоставляет гибкий способ выполнения гибридных запросов в ToroidalDB, сочетая возможности векторного, графового и реляционного поиска с учетом тороидальной топологии пространства.

## Базовые клаузы

### MATCH клауза

Клауза `MATCH` используется для определения паттернов узлов и связей в графе:

```
MATCH (alias:Label)
```

- `alias` - псевдоним узла (любое допустимое имя переменной)
- `Label` - метка узла (должна начинаться с буквы)

Примеры:
- `MATCH (user:User)`
- `MATCH (doc:Document)`
- `MATCH (product:Product)`

### WHERE клауза

Клауза `WHERE` используется для фильтрации результатов:

```
WHERE TOROIDALDISTANCE(field, threshold)
```

- `field` - поле вектора для сравнения (например, `node.vector`)
- `threshold` - порог расстояния (число с плавающей точкой)

Примеры:
- `WHERE TOROIDALDISTANCE(user.profile_vector, 0.3)`
- `WHERE TOROIDALDISTANCE(doc.content_vector, 0.25)`

### RETURN клауза

Клауза `RETURN` определяет, какие данные возвращать:

```
RETURN field1, field2, ...
```

- `field` - поле узла для возврата (например, `node.id`, `user.name`)

Примеры:
- `RETURN user.id`
- `RETURN doc.id, doc.title`
- `RETURN product.id, product.name, product.price`

### LIMIT клауза

Клауза `LIMIT` ограничивает количество возвращаемых результатов:

```
LIMIT number
```

- `number` - максимальное количество возвращаемых результатов

Примеры:
- `LIMIT 10`
- `LIMIT 100`
- `LIMIT 1`

## Примеры базовых запросов

### Простой поиск

```
MATCH (doc:Document) 
WHERE TOROIDALDISTANCE(doc.vector, 0.3) 
RETURN doc.id 
LIMIT 10
```

Этот запрос:
1. Находит узлы с меткой `Document`
2. Фильтрует их по тороидальному расстоянию с порогом 0.3
3. Возвращает ID документов
4. Ограничивает результат 10 записями

### Возврат нескольких полей

```
MATCH (user:User) 
WHERE TOROIDALDISTANCE(user.profile_vector, 0.25) 
RETURN user.id, user.name, user.email 
LIMIT 5
```

### Поиск с высоким порогом

```
MATCH (item:Item) 
WHERE TOROIDALDISTANCE(item.features, 0.4) 
RETURN item.id, item.description 
LIMIT 20
```

## Структура AST

Внутренне каждый запрос представлен в виде абстрактного синтаксического дерева (AST) с следующими основными структурами:

- `Query` - основная структура запроса
- `MatchClause` - представление MATCH клаузы
- `WhereCondition` - представление WHERE клаузы
- `NodePattern` - представление паттерна узла
- `Direction` - направление связи
- `RelationshipPattern` - представление паттерна связи
- `WhereCondition` - условия фильтрации
- `ReturnClause` - представление RETURN клаузы
- `LimitClause` - представление LIMIT клаузы

## Парсер

Парсер использует библиотеку `nom` для создания эффективного синтаксического анализатора, который преобразует текст запроса в структуры AST. Парсер обрабатывает:

- Разбор идентификаторов и меток
- Разбор числовых значений (включая дробные)
- Разбор строковых литералов
- Разбор операторов и ключевых слов
- Обработку пробелов и комментариев
- Обработку ошибок синтаксиса

## Тестирование

Базовый синтаксис тщательно протестирован с использованием 5 различных тестовых запросов:

1. Простой запрос: `MATCH (node:Label) RETURN node.id LIMIT 10`
2. Запрос с WHERE: `MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 5`
3. Запрос с несколькими полями RETURN: `MATCH (user:User) WHERE TOROIDALDISTANCE(user.profile, 0.4) RETURN user.id, user.name, user.email LIMIT 20`
4. Запрос с другим порогом: `MATCH (item:Item) WHERE TOROIDALDISTANCE(item.embedding, 0.15) RETURN item.id LIMIT 100`
5. Запросы с различными метками: Включая `Node`, `Document`, `User`, `Product`, `Post`

## Заключение

Базовый синтаксис TQL v2.0 предоставляет мощный, но простой в использовании способ для выполнения гибридных запросов в ToroidalDB. Он служит основой для более сложных возможностей, таких как агрегации, подзапросы, транзакции и распределенные операции.