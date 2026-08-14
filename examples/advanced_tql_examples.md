# Примеры использования TQL v2.0

## Обзор

В этом документе представлены практические примеры использования всех возможностей TQL v2.0, включая агрегации, подзапросы, транзакции и распределённые операции.

## 1. Агрегации

### COUNT - Подсчет элементов

```sql
-- Подсчет общего количества документов
MATCH (doc:Document)
RETURN COUNT(doc) AS total_documents

-- Подсчет документов с определённым свойством
MATCH (user:User)
WHERE user.active = true
RETURN COUNT(user) AS active_users

-- Подсчет связей
MATCH (user:User)-[:FOLLOWS]->(friend:User)
RETURN user.id, COUNT(friend) AS friend_count
ORDER BY friend_count DESC
LIMIT 10
```

### SUM - Сумма значений

```sql
-- Сумма всех значений в поле
MATCH (transaction:Transaction)
RETURN SUM(transaction.amount) AS total_amount

-- Сумма с фильтрацией
MATCH (product:Product)
WHERE product.category = "electronics"
RETURN SUM(product.price * product.quantity) AS total_inventory_value
```

### AVG - Среднее значение

```sql
-- Средний рейтинг пользователей
MATCH (user:User)
RETURN AVG(user.rating) AS average_user_rating

-- Среднее количество связей
MATCH (user:User)-[:FRIENDS_WITH]->(friend:User)
RETURN user.id, AVG(friend.connection_strength) AS avg_friendship_strength
```

### MIN/MAX - Минимальное/максимальное значение

```sql
-- Минимальная и максимальная цена
MATCH (product:Product)
RETURN MIN(product.price) AS min_price, MAX(product.price) AS max_price

-- Наиболее и наименее популярные теги
MATCH (doc:Document)-[:TAGGED_WITH]->(tag:Tag)
RETURN tag.name, COUNT(doc) AS usage_count
ORDER BY usage_count DESC
LIMIT 1  -- MAX
UNION
ORDER BY usage_count ASC
LIMIT 1  -- MIN
```

## 2. Подзапросы

### Подзапросы в WHERE

```sql
-- Найти пользователей, которые купили товары из топ-5 дорогих категорий
MATCH (user:User)
WHERE user.id IN (
  MATCH (category:Category)-[:HAS_PRODUCT]->(expensive_product:Product)
  RETURN expensive_product.seller_id
  ORDER BY expensive_product.price DESC
  LIMIT 5
)
RETURN user.name, user.email
```

### Подзапросы с агрегациями

```sql
-- Найти пользователей с количеством заказов выше среднего
MATCH (user:User)
WHERE (
  MATCH (user)-[:PLACED]->(order:Order)
  RETURN COUNT(order)
) > (
  MATCH (all_users:User)-[:PLACED]->(all_orders:Order)
  RETURN AVG(COUNT(all_orders))
)
RETURN user.id, user.name, user.order_count
```

### Вложенные подзапросы

```sql
-- Сложный запрос с вложенными подзапросами
MATCH (main_category:Category)
WHERE main_category.id IN (
  MATCH (sub_category:Category)
  WHERE sub_category.parent_id IN (
    MATCH (top_category:Category)
    WHERE top_category.popularity > 0.8
    RETURN top_category.id
  )
  RETURN sub_category.id
)
RETURN main_category.name, main_category.description
```

## 3. Транзакции

### Простая транзакция

```sql
BEGIN TRANSACTION
CREATE (user:User {name: "Alice Johnson", email: "alice@example.com", join_date: "2024-01-15"})
MATCH (group:Group {name: "Data Science"})
CREATE (user)-[:MEMBER_OF {since: "2024-01-15", role: "member"}]->(group)
UPDATE group SET member_count = group.member_count + 1
COMMIT
```

### Сложная транзакция с несколькими операциями

```sql
BEGIN TRANSACTION
-- Создаем нового пользователя
CREATE (new_user:User {
  id: 12345,
  name: "Bob Smith",
  email: "bob@example.com",
  profile_vector: [0.2, 0.8, 0.1, 0.9]
})

-- Находим похожих пользователей
MATCH (similar_user:User)
WHERE TOROIDALDISTANCE(similar_user.profile_vector, new_user.profile_vector, 0.2)
WITH similar_user
LIMIT 5

-- Создаем связи дружбы
CREATE (new_user)-[:FRIENDS_WITH {strength: 0.8}]->(similar_user)

-- Обновляем профиль нового пользователя
UPDATE new_user SET 
  friend_count = 5,
  last_updated = "2024-01-15T10:30:00Z"

-- Обновляем статистику похожих пользователей
MATCH (similar_user)
UPDATE similar_user SET 
  friend_suggestions_count = similar_user.friend_suggestions_count + 1

COMMIT
```

### Транзакция с обработкой ошибок

```sql
BEGIN TRANSACTION
CREATE (order:Order {
  id: 99999,
  user_id: 123,
  amount: 99.99,
  status: "pending"
})

MATCH (user:User {id: 123})
UPDATE user SET balance = user.balance - 99.99

-- Проверяем, что баланс не стал отрицательным
IF user.balance < 0 THEN
  ROLLBACK
  ERROR "Insufficient funds"
END

UPDATE order SET status = "confirmed"
COMMIT
```

## 4. Распределённые запросы

### Простой распределённый запрос

```sql
DISTRIBUTED MATCH (item:Item)
WHERE TOROIDALDISTANCE(item.vector, 0.25)
RETURN item.id, item.score, item.shard_id
LIMIT 50
```

### Распределённая агрегация

```sql
DISTRIBUTED MATCH (user:User)-[:PURCHASED]->(product:Product)
RETURN 
  COUNT(DISTINCT user) AS unique_users,
  COUNT(DISTINCT product) AS unique_products,
  AVG(product.price) AS avg_price,
  SUM(product.quantity) AS total_quantity
```

### Распределённый графовый обход

```sql
DISTRIBUTED MATCH (start:User {id: 123})-[:FOLLOWS*1..3]-(reachable:User)
RETURN start.id, reachable.id, length(path) AS path_length
ORDER BY path_length ASC
LIMIT 100
```

### Распределённая транзакция

```sql
DISTRIBUTED BEGIN TRANSACTION
MATCH (user1:User {id: 1}) ON SHARD 1
MATCH (user2:User {id: 2}) ON SHARD 2
CREATE (user1)-[:CONNECTED_TO {type: "business", strength: 0.7}]->(user2) ON BOTH SHARDS
UPDATE user1 SET connections_count = user1.connections_count + 1 ON SHARD 1
UPDATE user2 SET connections_count = user2.connections_count + 1 ON SHARD 2
COMMIT ON ALL SHARDS
```

## 5. Топологические операции

### CONNECTEDTO и WITHIN HOPS

```sql
-- Найти документы, соединенные с определённой темой в пределах 2 шагов
MATCH (doc:Document)
CONNECTEDTO(doc, "TAGGED_WITH", "quantum_physics")
WITHIN 2 HOPS
RETURN doc.id, doc.title, doc.topological_distance
ORDER BY doc.topological_distance ASC
LIMIT 20
```

### Топологический поиск с циклами

```sql
-- Поиск узлов с учетом тороидальной топологии (0.99 близко к 0.01)
MATCH (periodic_data:PeriodicData)
WHERE TOROIDALDISTANCE(periodic_data.angle_vector, [0.05, 0.95], 0.1)
RETURN periodic_data.id, periodic_data.angle, periodic_data.score
ORDER BY periodic_data.score ASC
LIMIT 15
```

### Bidirectional BFS

```sql
-- Найти кратчайшие пути между двумя множествами узлов
MATCH (source:User)-[:CONNECTED_TO*1..5]-(target:User)
WHERE source.id IN [1, 2, 3] AND target.id IN [100, 101, 102]
RETURN source.id, target.id, length(path) AS path_length
ORDER BY path_length ASC
LIMIT 10
```

## 6. Гибридный поиск (вектор + граф)

### Комбинированный поиск

```sql
-- Найти похожие документы среди друзей пользователя
MATCH (current_user:User {id: 123})-[:FRIENDS_WITH]->(friend:User)
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.content_vector, friend.profile_vector, 0.3)
RETURN doc.id, friend.id, doc.score
ORDER BY doc.score ASC
LIMIT 10
```

### Рекомендательная система

```sql
-- Рекомендация товаров на основе профиля пользователя и предпочтений друзей
MATCH (target_user:User {id: 456})-[:FRIENDS_WITH]->(friend:User)-[:PURCHASED]->(item:Item)
WHERE TOROIDALDISTANCE(item.feature_vector, target_user.preference_vector, 0.25)
RETURN item.id, item.name, item.recommendation_score
ORDER BY item.recommendation_score DESC
LIMIT 15
```

### Анализ сообществ

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

## 7. Комплексные примеры

### Аналитика пользовательского поведения

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

### Анализ топологических паттернов

```sql
-- Найти циклы в графе с учетом тороидальной топологии
MATCH (start:Node)-[:CONNECTED_TO*3..5]->(start)
WHERE 
  // Проверяем, что путь замыкается с учетом топологии
  TOROIDALDISTANCE(start.position_vector, start.position_vector, 0.01)
RETURN path, length(path) AS cycle_length
LIMIT 5
```

### Оптимизация через поток Риччи

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

## 8. Практические сценарии

### Поиск похожих изображений

```sql
-- Поиск похожих изображений по векторным эмбеддингам
MATCH (image:Image)
WHERE TOROIDALDISTANCE(image.embedding, [0.1, 0.8, 0.3, 0.9, ...], 0.25)
RETURN image.id, image.url, image.similarity_score
ORDER BY image.similarity_score ASC
LIMIT 12
```

### Рекомендательная система контента

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

### Анализ социальных сетей

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

## 9. Оптимизации производительности

### Использование кэша

```sql
-- Запросы с одинаковыми параметрами будут кэшироваться автоматически
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.vector, [0.5, 0.3], 0.2)
RETURN doc.id, doc.score
LIMIT 10
// Этот запрос будет выполнен быстрее при повторном вызове
```

### Параллельный поиск

```sql
-- Использование параллельного поиска для ускорения
DISTRIBUTED MATCH (item:Item)
WHERE TOROIDALDISTANCE(item.embedding, [0.1, 0.2, 0.3, ...], 0.15)
WITH item, TOROIDALDISTANCE(item.embedding, [0.1, 0.2, 0.3, ...]) AS distance
ORDER BY distance ASC
LIMIT 25
```

### Ранняя остановка

```sql
-- Поиск с ранней остановкой при достижении лимита
MATCH (product:Product)
WHERE TOROIDALDISTANCE(product.feature_vector, [0.4, 0.6], 0.3)
RETURN product.id, product.name
LIMIT 5  // Поиск остановится после нахождения 5 результатов
```

## Заключение

Эти примеры демонстрируют широкие возможности TQL v2.0 для решения сложных задач с векторными, графовыми и топологическими данными. Язык позволяет комбинировать различные типы анализа в одном запросе, обеспечивая гибкость и мощность для аналитических задач.