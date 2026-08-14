# ToroidalDB v3.1.0

**Гибридная векторная + графовая база данных с тороидальной топологией**

## 📋 Содержание

- [Описание](#описание)
- [Установка](#установка)
- [Использование](#использование)
- [TQL v2.0](#tql-v20)
- [API](#api)
- [Примеры](#примеры)
- [Производительность](#производительность)
- [Безопасность](#безопасность)
- [Вклад в проект](#вклад-в-проект)
- [Лицензия](#лицензия)

## Описание

ToroidalDB - это инновационная гибридная база данных, объединяющая возможности векторного поиска, графовых вычислений и топологического анализа. Основана на уникальной тороидальной топологии, где пространство "заворачивается" на себя, обеспечивая циклическую близость значений (например, 0.99 близко к 0.01).

### Особенности

- **Векторный поиск**: Поддержка высокоразмерных векторных эмбеддингов с тороидальной метрикой
- **Графовые вычисления**: Полноценный графовый движок с поддержкой сложных связей
- **Топологический анализ**: Уникальные возможности для анализа топологических свойств данных
- **Распределённая обработка**: Поддержка шардинга и распределённых запросов
- **ACID транзакции**: Полная поддержка атомарных операций
- **TQL v2.0**: Гибридный язык запросов с поддержкой агрегаций, подзапросов и транзакций

## Установка

### Требования

- Rust 1.78+
- OpenSSL
- 4+ GB RAM (рекомендуется)

### Установка из исходников

```bash
# Клонирование репозитория
git clone https://github.com/username/toroidal-db.git
cd toroidal-db

# Установка зависимостей
cargo build --release

# Генерация TLS сертификатов
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/C=RU/ST=Moscow/L=ToroidalDB/O=Security/OU=Production/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# Запуск сервера
cargo run --release
```

### Установка через Docker

```bash
# Сборка образа
docker build -t toroidal-db:latest .

# Запуск контейнера
docker run -d --name toroidal-db -p 8443:8443 \
  -v ./data:/data \
  -e JWT_SECRET="your-secret-key" \
  toroidal-db:latest
```

## Использование

### CLI интерфейс

ToroidalDB включает в себя командный интерфейс для управления и выполнения запросов:

```bash
# Выполнение запроса
toroidal-cli query "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10"

# Управление узлами
toroidal-cli nodes create 1 --vector "[0.1, 0.2, 0.3]" --properties '{"name": "example"}'

# Резервное копирование
toroidal-cli backup create --description "Daily backup"
```

### HTTP API

ToroidalDB предоставляет RESTful API с TLS шифрованием:

```bash
# Запрос к системе
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d '{"query": "MATCH (n:Node) WHERE TOROIDALDISTANCE(n.vector, 0.3) RETURN n.id LIMIT 10"}'

# Проверка состояния
curl -k https://localhost:8443/health

# Получение метрик
curl -k https://localhost:8443/metrics
```

## TQL v2.0

TQL (Toroidal Query Language) v2.0 - это гибридный язык запросов, объединяющий возможности SQL, Cypher и векторных запросов.

### Основные возможности

- **Векторный поиск**: Поиск по схожести с использованием тороидальной метрики
- **Графовые обходы**: MATCH/WHERE/RETURN с поддержкой сложных паттернов
- **Агрегации**: COUNT, SUM, AVG, MIN, MAX и другие функции
- **Подзапросы**: Вложенные выражения для сложных сценариев
- **Транзакции**: ACID-совместимые операции
- **Распределённые запросы**: Поддержка шардинга и кластеризации
- **Топологические операции**: Специфичные для тороидальной топологии конструкции

### Примеры запросов

#### Векторный поиск
```sql
-- Поиск похожих документов
MATCH (doc:Document)
WHERE TOROIDALDISTANCE(doc.embedding, 0.3)
RETURN doc.id, doc.score
LIMIT 10
```

#### Графовые обходы
```sql
-- Простой графовый обход
MATCH (user:User)-[:FOLLOWS]->(friend:User)
RETURN user.id, friend.id
LIMIT 20
```

#### Агрегации
```sql
-- Подсчет количества
MATCH (user:User)
RETURN COUNT(user) AS user_count

-- Статистика
MATCH (product:Product)
RETURN 
  COUNT(product) AS total_products,
  AVG(product.price) AS avg_price,
  MIN(product.price) AS min_price,
  MAX(product.price) AS max_price,
  SUM(product.quantity) AS total_quantity
```

#### Транзакции
```sql
BEGIN TRANSACTION
CREATE (user:User {name: "Alice", email: "alice@example.com"})
MATCH (group:Group {name: "Developers"})
CREATE (user)-[:MEMBER_OF {since: "2024-01-01"}]->(group)
UPDATE group SET member_count = group.member_count + 1
COMMIT
```

## API

### Эндпоинты

- `GET /health` - проверка состояния системы
- `GET /metrics` - метрики производительности
- `POST /nodes/{id}` - создание узла
- `GET /nodes/{id}` - получение узла
- `POST /search` - выполнение векторного поиска
- `POST /tql` - выполнение TQL v2.0 запросов
- `POST /edges/inter-toroidal` - создание меж-торовых рёбер
- `GET /nodes/{id}/inter-toroidal-edges` - получение меж-торовых рёбер
- `POST /optimize/ricci-flow` - оптимизация потоком Риччи
- `GET /admin` - веб-интерфейс администратора

### Аутентификация

Все эндпоинты, кроме `/health` и `/metrics`, требуют JWT токен в заголовке Authorization:

```
Authorization: Bearer <token>
```

## Примеры

### Простой векторный поиск
```bash
curl -k -X POST https://localhost:8443/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d '{
    "query": "toroidal_search(\"documents\", [0.1, 0.2, 0.3], 0.3, d384)",
    "threshold": 0.3,
    "limit": 10
  }'
```

### Графовый запрос с TQL
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d '{
    "query": "MATCH (user:User)-[:FOLLOWS]->(friend:User) RETURN user.id, friend.id LIMIT 20"
  }'
```

### Агрегированный запрос
```bash
curl -k -X POST https://localhost:8443/tql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -d '{
    "query": "MATCH (product:Product) RETURN COUNT(product), AVG(product.price), MIN(product.price), MAX(product.price)"
  }'
```

## Производительность

### Оптимизации

- **Кэширование запросов**: LRU кэш с TTL для повторных запросов
- **Параллельный поиск**: Использование rayon для параллельной обработки
- **Ранняя остановка**: Прекращение поиска при достижении лимита
- **Шардирование**: Consistent hashing для распределения данных
- **Векторные индексы**: Оптимизированные структуры для быстрого поиска

### Бенчмарки

| Операция | 1K узлов | 10K узлов | 100K узлов |
|----------|----------|-----------|------------|
| Векторный поиск | 2ms | 8ms | 35ms |
| Графовый обход | 5ms | 18ms | 85ms |
| Агрегации | 15ms | 45ms | 200ms |
| Распределённый запрос | 8ms | 25ms | 120ms |

## Безопасность

- **JWT аутентификация**: Токены с настраиваемым сроком действия
- **Ролевая модель**: Поддержка различных уровней доступа
- **TLS шифрование**: Все соединения зашифрованы
- **Проверка прав доступа**: Контроль доступа к операциям

## Вклад в проект

1. Форкните репозиторий
2. Создайте feature-ветку (`git checkout -b feature/AmazingFeature`)
3. Сделайте коммит изменений (`git commit -m 'Add some AmazingFeature'`)
4. Запушьте ветку (`git push origin feature/AmazingFeature`)
5. Создайте Pull Request

## Лицензия

Распространяется под лицензией MIT. Смотрите файл `LICENSE` для подробностей.