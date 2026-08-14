# Примеры curl запросов для TQL v2.0

## Убедитесь, что сервер запущен

```bash
# Генерация сертификатов (если еще не выполнено)
# openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 30 -nodes \
#   -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost'

# Запуск сервера
cargo run --release
```

## Тестирование эндпоинта /tql

### 1. Простой TQL запрос
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 5"}'
```

### 2. Запрос с другим порогом
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "MATCH (user:User) WHERE TOROIDALDISTANCE(user.vector, 0.5) RETURN user.id, user.name LIMIT 10"}'
```

### 3. Запрос с распределенным выполнением
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "DISTRIBUTED MATCH (item:Item) WHERE TOROIDALDISTANCE(item.vector, 0.2) RETURN item.id, item.score LIMIT 5"}'
```

### 4. Запрос с агрегацией (если поддерживается)
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "MATCH (user:User)-[:LIKES]->(content:Content) RETURN user.id, COUNT(content) as likes ORDER BY likes DESC LIMIT 5"}'
```

### 5. Тестирование эндпоинта валидации
```bash
curl -k -X POST https://localhost:8443/tql/validate \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 5"}'
```

## Тестирование с предварительно созданными данными

### 1. Создание тестового узла
```bash
curl -k -X POST https://localhost:8443/nodes/1 \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"vector": [0.1, 0.2, 0.3], "properties": {"name": "test_doc", "type": "document"}}'
```

### 2. Создание еще одного узла
```bash
curl -k -X POST https://localhost:8443/nodes/2 \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"vector": [0.15, 0.25, 0.35], "properties": {"name": "similar_doc", "type": "document"}}'
```

### 3. Выполнение TQL запроса для поиска похожих узлов
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.2) RETURN doc.id, doc.name LIMIT 10"}'
```

## Тестирование ошибок

### 1. Неверный синтаксис запроса
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": "INVALID QUERY SYNTAX"}'
```

### 2. Пустой запрос
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer admin-token-xyz" \
  -d '{"query": ""}'
```