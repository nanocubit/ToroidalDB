# API Reference для ToroidalDB

## Обзор

ToroidalDB предоставляет RESTful API с TLS шифрованием для взаимодействия с базой данных. Все эндпоинты требуют JWT аутентификации, за исключением `/health` и `/metrics`.

## Аутентификация

Для доступа к защищенным эндпоинтам используйте заголовок:

```
Authorization: Bearer <jwt_token>
```

## Эндпоинты

### /health

**GET** - Проверка состояния системы

#### Пример запроса:
```
curl -k https://localhost:8443/health
```

#### Ответ:
```json
{
  "status": "healthy",
  "version": "3.1.0",
  "storage": {
    "nodes": 1234,
    "type": "persistent (sled + graph + matryoshka + topology)",
    "optimization": "enabled (cached)",
    "path": "./data"
  }
}
```

### /metrics

**GET** - Получение метрик производительности (Prometheus формат)

#### Пример запроса:
```
curl -k https://localhost:8443/metrics
```

#### Ответ:
```
# Метрики в формате Prometheus
toroidal_requests_total{method="GET",status="200"} 15
toroidal_request_duration_seconds_bucket{le="0.005"} 10
...
```

### /nodes/{id}

**POST** - Создание или обновление узла

#### Пример запроса:
```
curl -k -X POST https://localhost:8443/nodes/123 \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "vector": [0.1, 0.2, 0.3, 0.4],
    "properties": {
      "name": "example",
      "type": "document",
      "category": "tech"
    },
    "edges": []
  }'
```

#### Ответ:
```json
{
  "success": true,
  "message": "Node inserted successfully",
  "data": null
}
```

**GET** - Получение узла

#### Пример запроса:
```
curl -k -H "Authorization: Bearer <token>" https://localhost:8443/nodes/123
```

#### Ответ:
```json
{
  "success": true,
  "message": null,
  "data": {
    "id": 123,
    "vector": [0.1, 0.2, 0.3, 0.4],
    "properties": {
      "name": "example",
      "type": "document",
      "category": "tech"
    },
    "edges": []
  }
}
```

### /search

**POST** - Выполнение векторного поиска

#### Пример запроса:
```
curl -k -X POST https://localhost:8443/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "query": "toroidal_search(\"documents\", [0.1, 0.2], 0.3, d384)",
    "threshold": 0.3,
    "limit": 10
  }'
```

#### Ответ:
```json
{
  "success": true,
  "message": "Search completed (cached)",
  "data": {
    "collection": "documents",
    "results": [
      {
        "id": 123,
        "distance": 0.1234,
        "properties": {...},
        "edges_count": 5,
        "dimension": "d384"
      }
    ],
    "count": 1,
    "threshold": 0.3,
    "dimension": 384,
    "optimization": "cached"
  }
}
```

### /tql

**POST** - Выполнение TQL v2.0 запроса

#### Пример запроса:
```
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "query": "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10"
  }'
```

#### Ответ:
```json
{
  "success": true,
  "message": "TQL query executed successfully, found 3 results",
  "data": {
    "results": [
      {
        "id": 123,
        "score": 0.1234,
        "properties": {...}
      }
    ],
    "count": 3,
    "execution": "local"
  }
}
```

### /edges/inter-toroidal

**POST** - Создание меж-торового ребра

#### Пример запроса:
```
curl -k -X POST https://localhost:8443/edges/inter-toroidal \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "source_id": 1,
    "source_level": "d384",
    "target_id": 2,
    "target_level": "d768",
    "relation_type": "RELATED_TO",
    "properties": {
      "weight": 0.85,
      "timestamp": "2024-01-01T00:00:00Z"
    }
  }'
```

#### Ответ:
```json
{
  "success": true,
  "message": "Inter-toroidal edge created: 1(d384) --[RELATED_TO](0.85)-> 2(d768)",
  "data": {
    "edge": {
      "source": {"level": 384, "node_id": 1},
      "target": {"level": 768, "node_id": 2},
      "relation_type": "RELATED_TO",
      "topological_distance": 0.15,
      "homotopy_class": "direct",
      "properties": {...}
    }
  }
}
```

### /nodes/{id}/inter-toroidal-edges

**GET** - Получение меж-торовых рёбер узла

#### Пример запроса:
```
curl -k -H "Authorization: Bearer <token>" https://localhost:8443/nodes/123/inter-toroidal-edges
```

#### Ответ:
```json
{
  "success": true,
  "message": "Found 2 inter-toroidal edges for node 123",
  "data": {
    "node_id": 123,
    "edges": [...],
    "count": 2
  }
}
```

### /optimize/ricci-flow

**POST** - Оптимизация вложений потоком Риччи

#### Пример запроса:
```
curl -k -X POST https://localhost:8443/optimize/ricci-flow \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "query": "ricci_flow(iterations=100, target_dim=d768)"
  }'
```

#### Ответ:
```json
{
  "success": true,
  "message": "Ricci flow optimization completed: 500 nodes optimized in 100 iterations",
  "data": {
    "iterations": 100,
    "target_dimension": 768,
    "optimized_nodes": 500,
    "total_nodes": 1000
  }
}
```

### /admin

**GET** - Веб-интерфейс администратора

#### Пример запроса:
```
curl -k -H "Authorization: Bearer <token>" https://localhost:8443/admin
```

#### Ответ:
HTML страница админ-панели

## Ошибки

Все ошибки возвращаются в формате:

```json
{
  "error": "Описание ошибки"
}
```

### Коды ошибок

- `400 Bad Request` - Неверный синтаксис запроса
- `401 Unauthorized` - Отсутствует или неверный токен аутентификации
- `403 Forbidden` - Недостаточно прав доступа
- `404 Not Found` - Ресурс не найден
- `409 Conflict` - Конфликт (например, узел уже существует)
- `500 Internal Server Error` - Внутренняя ошибка сервера

## Заголовки

- `Content-Type: application/json` - для JSON запросов
- `Authorization: Bearer <token>` - для аутентификации
- `Accept: application/json` - для получения JSON ответа