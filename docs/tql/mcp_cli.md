# MCP (Model Configuration Protocol) и CLI для ToroidalDB

## Обзор

MCP (Model Configuration Protocol) и CLI (Command Line Interface) обеспечивают удобное управление и конфигурацию ToroidalDB. CLI позволяет выполнять запросы, управлять узлами и транзакциями из командной строки, а MCP предоставляет протокол для настройки параметров системы и моделей.

## CLI (Command Line Interface)

### Установка и запуск

```bash
# Установка CLI
cargo install toroidal-cli

# Или запуск напрямую
cargo run --bin toroidal-cli -- [аргументы]
```

### Основные команды

#### Выполнение TQL запросов

```bash
# Простой запрос
toroidal-cli --host https://localhost:8443 --token YOUR_TOKEN query "MATCH (n:Node) RETURN n.id LIMIT 10"

# Запрос с агрегацией
toroidal-cli query "MATCH (user:User) RETURN COUNT(user) AS user_count"

# Запрос с топологией
toroidal-cli query "MATCH (doc:Document) CONNECTEDTO(doc, 'TAGGED_WITH', 'quantum') WITHIN 2 HOPS RETURN doc.id LIMIT 20"
```

#### Управление узлами

```bash
# Получить узел
toroidal-cli nodes get 123

# Создать узел
toroidal-cli nodes create 123 --vector "[0.5, 0.3, 0.7]" --properties '{"name": "test_node", "type": "document"}'

# Создать узел из файла
toroidal-cli nodes create 123 --vector-file ./vector.json --properties-file ./properties.json

# Удалить узел
toroidal-cli nodes delete 123
```

#### Управление резервными копиями

```bash
# Создать резервную копию
toroidal-cli backup --description "Daily backup"

# Восстановить из резервной копии
toroidal-cli restore backup_1234567890

# Список резервных копий
toroidal-cli list-backups
```

#### Управление транзакциями

```bash
# Начать транзакцию
toroidal-cli transaction begin --file ./transaction_ops.json

# Завершить транзакцию
toroidal-cli transaction commit

# Откатить транзакцию
toroidal-cli transaction rollback
```

#### Системная информация

```bash
# Проверить состояние системы
toroidal-cli health

# Получить метрики
toroidal-cli metrics
```

### Опции командной строки

- `--host`: адрес сервера ToroidalDB (по умолчанию: https://localhost:8443)
- `--token`: токен аутентификации (по умолчанию берется из переменной окружения TOROIDAL_TOKEN)
- `--help`: показать справку по команде

## MCP (Model Configuration Protocol)

### Обзор

MCP предоставляет протокол для динамической настройки параметров системы и моделей. Он позволяет изменять конфигурацию во время работы без перезапуска сервера.

### Структура конфигурации

#### Системная конфигурация

```rust
pub struct SystemConfiguration {
    pub server_port: u16,                    // Порт сервера
    pub tls_cert_path: String,              // Путь к TLS сертификату
    pub tls_key_path: String,               // Путь к TLS ключу
    pub data_path: String,                  // Путь к данным
    pub backup_retention_days: u32,         // Дней хранения резервных копий
    pub cache_size: usize,                  // Размер кэша запросов
    pub cache_ttl_seconds: u64,             // Время жизни кэша в секундах
    pub shard_count: u32,                   // Количество шардов
    pub max_connections: u32,               // Максимальное количество соединений
    pub query_timeout_seconds: u64,         // Таймаут запросов в секундах
    pub enable_metrics: bool,               // Включить сбор метрик
    pub log_level: String,                  // Уровень логирования
    pub custom_parameters: HashMap<String, Value>, // Пользовательские параметры
}
```

#### Конфигурация моделей

```rust
pub struct ModelConfiguration {
    pub name: String,                       // Имя модели
    pub version: String,                    // Версия модели
    pub dimensions: Vec<u32>,               // Поддерживаемые размерности
    pub default_threshold: f32,             // Порог по умолчанию
    pub optimization_settings: OptimizationSettings, // Настройки оптимизации
    pub topology_settings: TopologySettings,         // Настройки топологии
    pub performance_settings: PerformanceSettings,   // Настройки производительности
}
```

### API MCP

#### Получение конфигурации

```bash
# Получить системную конфигурацию
curl -k -H "Authorization: Bearer TOKEN" https://localhost:8443/mcp/config/system

# Получить конфигурацию модели
curl -k -H "Authorization: Bearer TOKEN" https://localhost:8443/mcp/config/model/default
```

#### Обновление конфигурации

```bash
# Обновить системную конфигурацию
curl -k -X PUT -H "Authorization: Bearer TOKEN" -H "Content-Type: application/json" \
  https://localhost:8443/mcp/config/system \
  -d '{
    "cache_size": 2000,
    "cache_ttl_seconds": 7200,
    "max_connections": 200
  }'

# Обновить конфигурацию модели
curl -k -X PUT -H "Authorization: Bearer TOKEN" -H "Content-Type: application/json" \
  https://localhost:8443/mcp/config/model/my_model \
  -d '{
    "default_threshold": 0.25,
    "optimization_settings": {
      "enable_caching": true,
      "cache_strategy": "LRU"
    }
  }'
```

### CLI команды для MCP

```bash
# Получить системную конфигурацию
toroidal-cli mcp get system

# Получить конфигурацию модели
toroidal-cli mcp get model default

# Обновить системную конфигурацию
toroidal-cli mcp update system --param cache_size=2000 --param max_connections=200

# Обновить конфигурацию модели
toroidal-cli mcp update model my_model --param default_threshold=0.25

# Сбросить конфигурацию к значениям по умолчанию
toroidal-cli mcp reset
```

## Примеры использования

### Выполнение аналитического запроса

```bash
# Найти 10 самых похожих документов к вектору
toroidal-cli query "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, [0.5, 0.3, 0.7], 0.2) RETURN doc.id, doc.score LIMIT 10"
```

### Управление узлами

```bash
# Создать узел с вектором и свойствами
toroidal-cli nodes create 1 --vector "[0.1, 0.2, 0.3]" --properties '{"name": "example", "category": "test"}'

# Получить информацию об узле
toroidal-cli nodes get 1
```

### Работа с транзакциями

```bash
# Начать транзакцию для создания нескольких узлов
toroidal-cli transaction begin

# Выполнить несколько операций
toroidal-cli nodes create 100 --vector "[0.5, 0.5]" --properties '{"name": "user100"}'
toroidal-cli nodes create 101 --vector "[0.6, 0.4]" --properties '{"name": "user101"}'

# Создать связь между узлами
toroidal-cli query "MATCH (u1:User {id: 100}), (u2:User {id: 101}) CREATE (u1)-[:FRIENDS]->(u2)"

# Зафиксировать транзакцию
toroidal-cli transaction commit
```

### Изменение конфигурации во время работы

```bash
# Увеличить размер кэша
toroidal-cli mcp update system --param cache_size=5000

# Изменить порог по умолчанию для модели
toroidal-cli mcp update model default --param default_threshold=0.15
```

## Архитектура

### CLI архитектура

```
[Пользовательский ввод] -> [Clap Parser] -> [Команды] -> [HTTP API] -> [ToroidalDB]
```

### MCP архитектура

```
[Конфигурационные запросы] -> [MCP Handler] -> [Runtime Config] -> [System Components]
```

## Безопасность

### Аутентификация

CLI использует JWT токены для аутентификации:
- Токен может быть передан через флаг `--token`
- Токен может быть задан через переменную окружения `TOROIDAL_TOKEN`
- Токен автоматически добавляется к HTTP запросам

### Авторизация

- Различные уровни доступа к командам CLI
- Проверка прав доступа на уровне API
- Поддержка ролей и разрешений

## Производительность

### CLI оптимизации

- Пакетная обработка команд
- Кэширование соединений
- Асинхронные запросы

### MCP оптимизации

- Быстрое применение изменений без перезапуска
- Атомарное обновление конфигурации
- Валидация конфигурации перед применением

## Заключение

MCP и CLI обеспечивают мощный и удобный интерфейс для управления ToroidalDB. CLI позволяет выполнять все основные операции из командной строки, а MCP предоставляет гибкий протокол для динамической настройки системы. Вместе они обеспечивают полный контроль над базой данных и ее параметрами.