# Руководство по TQL v2.0 для новичков

## Введение

TQL (Toroidal Query Language) v2.0 - это мощный гибридный язык запросов для ToroidalDB, который сочетает в себе возможности векторного, графового и реляционного поиска с учетом тороидальной топологии пространства.

В этом руководстве вы узнаете основы TQL v2.0 и научитесь создавать свои первые запросы.

## 1. Простые запросы

### Поиск документов

Начнем с простого запроса для поиска документов:

```
MATCH (doc:Document) 
WHERE TOROIDALDISTANCE(doc.vector, 0.3) 
RETURN doc.id 
LIMIT 10
```

Разберем по частям:
- `MATCH (doc:Document)` - ищем узлы с меткой "Document" и даем им псевдоним "doc"
- `WHERE TOROIDALDISTANCE(doc.vector, 0.3)` - фильтруем по тороидальному расстоянию с порогом 0.3
- `RETURN doc.id` - возвращаем только ID документов
- `LIMIT 10` - ограничиваем результат 10 записями

### Возврат дополнительных полей

Вы можете возвращать больше информации:

```
MATCH (doc:Document) 
WHERE TOROIDALDISTANCE(doc.vector, 0.3) 
RETURN doc.id, doc.title, doc.score 
LIMIT 5
```

## 2. Графовые запросы

### Поиск связей

Для поиска связей между узлами используйте стрелки:

```
MATCH (user:User)-[:FRIEND_OF]->(friend:User) 
RETURN user.name, friend.name
```

Этот запрос находит всех друзей пользователей.

### Обратные связи

Для поиска обратных связей используйте стрелку в другую сторону:

```
MATCH (user:User)<-[:FRIEND_OF]-(friend:User) 
RETURN user.name, friend.name
```

## 3. Специфичные для топологии запросы

### CONNECTEDTO и WITHIN HOPS

Одна из уникальных возможностей TQL v2.0 - это поиск узлов, связанных через определенное количество шагов:

```
MATCH (doc:Document) 
CONNECTEDTO(doc, "TAGGED_WITH", "quantum") 
WITHIN 2 HOPS 
RETURN doc.id, doc.title 
LIMIT 20
```

Этот запрос находит документы, которые связаны с тегом "quantum" через не более чем 2 шага в графе.

## 4. Агрегации

### Подсчет количества

Для подсчета количества элементов используйте COUNT:

```
MATCH (user:User)-[:LIKES]->(post:Post) 
RETURN user.id, COUNT(post) AS like_count 
ORDER BY like_count DESC 
LIMIT 10
```

### Другие агрегационные функции

- `SUM(field)` - сумма значений
- `AVG(field)` - среднее значение
- `MIN(field)` - минимальное значение
- `MAX(field)` - максимальное значение

Пример с AVG:

```
MATCH (product:Product)-[:REVIEWED_BY]->(review:Review) 
RETURN product.name, AVG(review.rating) AS avg_rating 
ORDER BY avg_rating DESC
```

## 5. Сортировка и ограничения

### Сортировка

Используйте ORDER BY для сортировки результатов:

```
MATCH (article:Article) 
WHERE TOROIDALDISTANCE(article.embedding, 0.2) 
RETURN article.title, article.score 
ORDER BY article.score ASC 
LIMIT 20
```

### Комбинирование условий

Вы можете комбинировать несколько условий:

```
MATCH (user:User) 
WHERE TOROIDALDISTANCE(user.profile_vector, 0.4) 
AND user.age > 18 
RETURN user.id, user.name 
ORDER BY user.name 
LIMIT 10
```

## 6. Распределенные запросы

Для выполнения запроса на нескольких шардах используйте ключевое слово DISTRIBUTED:

```
DISTRIBUTED MATCH (item:Item) 
WHERE TOROIDALDISTANCE(item.embedding, 0.2) 
RETURN item.id, item.similarity_score 
LIMIT 50
```

## 7. Практические примеры

### Пример 1: Рекомендательная система

```
MATCH (user:User {id: 123})-[:RATED]->(movie:Movie) 
WHERE movie.rating > 4.0 
MATCH (similar_user:User)-[:RATED]->(similar_movie:Movie) 
WHERE similar_user.id IN [/* список похожих пользователей */] 
RETURN similar_movie.title, similar_movie.genre 
LIMIT 10
```

### Пример 2: Анализ социальной сети

```
MATCH (person:Person)-[:FOLLOWS]->(influencer:Influencer) 
RETURN influencer.name, COUNT(person) AS follower_count 
ORDER BY follower_count DESC 
LIMIT 5
```

### Пример 3: Поиск похожих документов

```
MATCH (doc:Document) 
WHERE TOROIDALDISTANCE(doc.content_vector, 0.25) 
RETURN doc.id, doc.title, doc.similarity_score 
ORDER BY doc.similarity_score ASC 
LIMIT 15
```

## 8. Советы для начинающих

1. **Начинайте с простого** - создавайте простые запросы и постепенно усложняйте их
2. **Используйте псевдонимы** - они делают запросы более читаемыми
3. **Тестируйте с LIMIT** - всегда используйте LIMIT при разработке запросов
4. **Проверяйте синтаксис** - используйте валидацию в админке
5. **Изучайте примеры** - смотрите готовые примеры в документации

## 9. Частые ошибки и решения

### Ошибка: "Отсутствует RETURN клауза"
Решение: Добавьте RETURN клаузу в ваш запрос

### Ошибка: "Неверный формат MATCH клаузы"
Решение: Убедитесь, что вы используете правильный формат: `(alias:Label)`

### Ошибка: "Неизвестный тип запроса"
Решение: Проверьте, что вы используете поддерживаемые конструкции TQL v2.0

## 10. Следующие шаги

После освоения основ вы можете:

1. Изучить [синтаксис](syntax.md) более подробно
2. Ознакомиться с [рекомендациями по производительности](performance.md)
3. Попробовать [расширенные возможности](advanced.md)
4. Экспериментировать с транзакциями и подзапросами

Удачи в изучении TQL v2.0!