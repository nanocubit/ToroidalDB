# Примеры использования TQL v2.0

## Обзор

В этом документе представлены практические примеры использования TQL v2.0 для решения реальных задач с векторными, графовыми и топологическими данными.

## Базовые примеры

### 1. Простой векторный поиск

```sql
-- Найти 10 документов, наиболее похожих на заданный вектор
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.embedding, 0.3)
RETURN doc.id, doc.score
LIMIT 10
```

### 2. Поиск с фильтрацией

```sql
-- Найти документы с определёнными свойствами
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.vector, 0.25)
AND doc.category = "technology"
RETURN doc.id, doc.title, doc.score
LIMIT 5
```

### 3. Графовый обход

```sql
-- Найти друзей друзей
MATCH (user:User)-[:FOLLOWS]->(friend:User)-[:FOLLOWS]->(friend_of_friend:User)
WHERE friend_of_friend.active = true
RETURN user.id, friend_of_friend.id, friend_of_friend.name
LIMIT 20
```

## Агрегации

### 1. Подсчет количества

```sql
-- Подсчет количества пользователей
MATCH (user:User)
RETURN COUNT(user) AS user_count
```

### 2. Статистика по полям

```sql
-- Среднее, минимум и максимум по полю
MATCH (product:Product)
RETURN 
  COUNT(product) AS total_products,
  AVG(product.price) AS avg_price,
  MIN(product.price) AS min_price,
  MAX(product.price) AS max_price,
  SUM(product.quantity) AS total_quantity
```

### 3. Агрегации с группировкой

```sql
-- Количество продуктов по категориям
MATCH (product:Product)
RETURN product.category, COUNT(product) AS count
GROUP BY product.category
ORDER BY count DESC
LIMIT 10
```

## Подзапросы

### 1. Подзапрос в WHERE

```sql
-- Найти пользователей, которые купили продукты из топ-5 дорогих
MATCH (user:User)
WHERE user.id IN (
  MATCH (expensive_product:Product)
  RETURN expensive_product.buyer_id
  ORDER BY expensive_product.price DESC
  LIMIT 5
)
RETURN user.name, user.email
```

### 2. Подзапрос с агрегацией

```sql
-- Найти пользователей с количеством покупок выше среднего
MATCH (user:User)
WHERE user.purchase_count > (
  MATCH (all_users:User)
  RETURN AVG(all_users.purchase_count)
)
RETURN user.id, user.name, user.purchase_count
ORDER BY user.purchase_count DESC
LIMIT 10
```

## Транзакции

### 1. Простая транзакция

```sql
BEGIN TRANSACTION
CREATE (user:User {name: "Alice", email: "alice@example.com", created_at: "2024-01-01"})
MATCH (group:Group {name: "Developers"})
CREATE (user)-[:MEMBER_OF {since: "2024-01-01"}]->(group)
UPDATE group SET member_count = group.member_count + 1
COMMIT
```

### 2. Сложная транзакция

```sql
BEGIN TRANSACTION
-- Создаем нового пользователя
CREATE (new_user:User {
  name: "Bob Wilson",
  email: "bob@example.com",
  join_date: "2024-01-15",
  status: "active"
})

-- Находим подходящих друзей
MATCH (potential_friend:User)
WHERE 
  TOROIDALDISTANCE(potential_friend.profile_vector, new_user.profile_vector, 0.2)
  AND potential_friend.id != new_user.id
WITH potential_friend
LIMIT 5

-- Создаем связи дружбы
CREATE (new_user)-[:FRIENDS_WITH {strength: 0.8}]->(potential_friend)

-- Обновляем статистику
UPDATE new_user SET friend_count = 5
COMMIT
```

### 3. Транзакция с откатом

```sql
BEGIN TRANSACTION
CREATE (order:Order {user_id: 123, amount: 99.99, status: "pending"})
MATCH (user:User {id: 123})
UPDATE user SET balance = user.balance - 99.99

-- Проверяем, достаточно ли средств
IF user.balance < 0 THEN
  ROLLBACK
ELSE
  UPDATE order SET status = "confirmed"
  COMMIT
END
```

## Распределённые запросы

### 1. Простой распределённый запрос

```sql
DISTRIBUTED MATCH (item:Item)
WHERE TOROIDALDISTANCE(item.vector, 0.2)
RETURN item.id, item.score
LIMIT 50
```

### 2. Распределённая агрегация

```sql
DISTRIBUTED MATCH (user:User)-[:PURCHASED]->(product:Product)
RETURN 
  COUNT(DISTINCT user) AS unique_users,
  COUNT(DISTINCT product) AS unique_products,
  AVG(product.price) AS avg_price,
  SUM(product.quantity) AS total_quantity
```

### 3. Распределённый графовый обход

```sql
DISTRIBUTED MATCH (start:User {id: 123})-[:FOLLOWS*1..3]->(reachable:User)
RETURN reachable.id, reachable.name
LIMIT 100
```

## Топологические операции

### 1. CONNECTEDTO и WITHIN HOPS

```sql
-- Найти документы, соединенные с определённой темой в пределах 2 шагов
MATCH (doc:Document)
CONNECTEDTO(doc, "TAGGED_WITH", "quantum_computing")
WITHIN 2 HOPS
RETURN doc.id, doc.title, doc.similarity_score
LIMIT 20
```

### 2. Топологический поиск с циклами

```sql
-- Поиск узлов с учетом тороидальной топологии (0.99 близко к 0.01)
MATCH (periodic_data:PeriodicData)
WHERE TOROIDALDISTANCE(periodic_data.angle_vector, 0.1)
RETURN periodic_data.id, periodic_data.angle, periodic_data.score
ORDER BY periodic_data.score ASC
LIMIT 15
```

### 3. Bidirectional BFS

```sql
-- Найти кратчайшие пути между двумя множествами узлов
MATCH (source:User)-[:CONNECTED_TO*1..5]-(target:User)
WHERE source.id IN [1, 2, 3] AND target.id IN [100, 101, 102]
RETURN source.id, target.id, length(path) AS path_length
ORDER BY path_length ASC
LIMIT 10
```

## Гибридный поиск (вектор + граф)

### 1. Комбинированный поиск

```sql
-- Найти похожие документы среди друзей пользователя
MATCH (user:User {id: 123})-[:FRIENDS_WITH]->(friend:User)
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.content_vector, friend.profile_vector, 0.3)
RETURN doc.id, friend.id, doc.score
ORDER BY doc.score ASC
LIMIT 10
```

### 2. Рекомендательная система

```sql
-- Рекомендация товаров на основе профиля пользователя и предпочтений друзей
MATCH (target_user:User {id: 456})-[:FRIENDS_WITH]->(friend:User)-[:PURCHASED]->(item:Item)
WHERE TOROIDALDISTANCE(item.feature_vector, target_user.preference_vector, 0.25)
RETURN item.id, item.name, item.recommendation_score
ORDER BY item.recommendation_score DESC
LIMIT 15
```

### 3. Анализ сообществ

```sql
-- Найти центральные узлы в сообществе
MATCH (member:User)-[:MEMBER_OF]->(community:Community {id: "tech"})
WHERE TOROIDALDISTANCE(member.interest_vector, community.focus_vector, 0.3)
MATCH (member)-[:FOLLOWS*1..2]-(other_member:User)
WITH member, COUNT(other_member) AS connectivity
RETURN member.id, member.name, connectivity
ORDER BY connectivity DESC
LIMIT 10
```

## Продвинутые примеры

### 1. Аналитика пользовательского поведения

```sql
-- Найти пользователей с похожим поведением
MATCH (user_a:User)-[:VISITED]->(page_a:Page)<-[:VISITED]-(user_b:User)
WHERE TOROIDALDISTANCE(user_a.behavior_vector, user_b.behavior_vector, 0.2)
WITH user_a, user_b, 
  COUNT(page_a) AS common_pages,
  AVG(page_a.engagement_score) AS avg_engagement
WHERE common_pages >= 3 AND avg_engagement > 0.7
RETURN user_a.id, user_b.id, common_pages, avg_engagement
ORDER BY avg_engagement DESC, common_pages DESC
LIMIT 20
```

### 2. Анализ топологических паттернов

```sql
-- Найти циклы в графе с учетом тороидальной топологии
MATCH (start:Node)-[:CONNECTED_TO*3..5]->(start)
WHERE 
  // Проверяем, что путь замыкается с учетом топологии
  TOROIDALDISTANCE(start.position_vector, start.position_vector, 0.01)
RETURN path, length(path) AS cycle_length
LIMIT 5
```

### 3. Оптимизация через поток Риччи

```sql
-- Оптимизировать вложения пользователей
RICCI_FLOW_OPTIMIZE(
  iterations = 100,
  target_dim = d768,
  learning_rate = 0.01
)
MATCH (user:User)
WHERE user.needs_optimization = true
RETURN user.id, user.optimization_status
```

## Практические сценарии

### 1. Поиск похожих изображений

```sql
-- Поиск похожих изображений по векторным эмбеддингам
MATCH (image:Image)
WHERE TOROIDALDISTANCE(image.embedding, [0.1, 0.8, 0.3, 0.9, ...], 0.25)
RETURN image.id, image.url, image.similarity_score
ORDER BY image.similarity_score ASC
LIMIT 12
```

### 2. Рекомендательная система контента

```sql
-- Рекомендация статей на основе интересов и социальных связей
MATCH (reader:User {id: 123})-[:FOLLOWS]->(influencer:User)-[:READ]->(article:Article)
WHERE 
  TOROIDALDISTANCE(article.topic_vector, reader.interest_vector, 0.3)
  AND article.published_date > "2024-01-01"
WITH article, COUNT(influencer) AS influence_score
RETURN 
  article.id, 
  article.title, 
  influence_score,
  article.relevance_score
ORDER BY influence_score DESC, article.relevance_score ASC
LIMIT 10
```

### 3. Анализ социальных сетей

```sql
-- Найти влиятельных пользователей в определённой тематике
MATCH (user:User)-[:POSTED]->(content:Content)
WHERE 
  TOROIDALDISTANCE(content.topic_vector, [0.7, 0.2, 0.8, ...], 0.2)
  AND content.engagement > 100
WITH user, 
  COUNT(content) AS posts_count,
  AVG(content.engagement) AS avg_engagement,
  SUM(content.engagement) AS total_engagement
MATCH (user)-[:FOLLOWS]-(follower:User)
WITH user, posts_count, avg_engagement, total_engagement, COUNT(follower) AS followers_count
WHERE followers_count > 1000
RETURN 
  user.id,
  user.name,
  followers_count,
  posts_count,
  avg_engagement,
  total_engagement
ORDER BY total_engagement DESC
LIMIT 20
```

## Заключение

Эти примеры демонстрируют широкие возможности TQL v2.0 для решения сложных задач с векторными, графовыми и топологическими данными. Язык позволяет комбинировать различные типы анализа в одном запросе, обеспечивая гибкость и мощность для аналитических задач.