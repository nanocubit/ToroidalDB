# Архитектурные решения ToroidalDB

## Обзор архитектуры

ToroidalDB - это гибридная база данных, объединяющая векторный, графовый и топологический подходы к хранению и поиску данных. Архитектура построена на модульном принципе с четким разделением ответственности.

## Компоненты системы

### 1. Ядро системы (Core)

#### Storage Layer
- **PersistentStore**: Основной интерфейс к хранилищу
- **QueryCache**: LRU кэш с TTL для результатов запросов
- **Sled Integration**: Использует sled как встроенный движок хранения
- **Graph Storage**: Хранение узлов и рёбер в графовой структуре

#### Math Layer
- **Matryoshka Embeddings**: Поддержка вложенных векторных пространств
- **Toroidal Distance**: Уникальная метрика расстояния для тороидального пространства
- **Dimension Handling**: Поддержка разных размерностей (384, 768, 1024, 1536)

#### Topology Layer
- **Toroidal Topology**: Модель тороидального пространства
- **Homotopy Classes**: Классификация путей по топологическим свойствам
- **Ricci Flow**: Алгоритм оптимизации вложений через поток Риччи
- **Inter-Toroidal Edges**: Связи между разными уровнями иерархии

### 2. Язык запросов (TQL v2.0)

#### Parser Module
- **Nom-based Parser**: Использует nom для эффективного разбора
- **AST Generation**: Создает абстрактное синтаксическое дерево
- **Error Recovery**: Обработка синтаксических ошибок

#### Executor Module
- **Query Execution**: Выполнение запросов на уровне хранилища
- **Aggregation Support**: Обработка агрегационных функций
- **Transaction Support**: Выполнение транзакционных операций
- **Distributed Execution**: Поддержка распределённых запросов

#### Coordinator Module
- **Consistent Hashing**: Алгоритм для равномерного распределения данных
- **Shard Routing**: Маршрутизация запросов по шардам
- **Scatter/Gather**: Механизм распределённого выполнения

### 3. Сетевой уровень (Network)

#### HTTP API
- **Axum Framework**: Использует axum для асинхронного HTTP
- **TLS Encryption**: Все соединения зашифрованы
- **JWT Authentication**: Токены для аутентификации
- **Rate Limiting**: Ограничение частоты запросов

#### Middleware
- **Authentication**: Проверка JWT токенов
- **Authorization**: Проверка прав доступа
- **Logging**: Подробное логирование запросов
- **Metrics**: Сбор метрик производительности

## Архитектурные паттерны

### 1. Consistent Hashing

Для распределения данных по шардам используется consistent hashing:

```
Node ID -> Hash Function -> Hash Ring -> Shard ID
```

- **Virtual Nodes**: Каждый физический шард представлен несколькими виртуальными узлами
- **Load Balancing**: Равномерное распределение нагрузки
- **Scalability**: Легкое добавление/удаление шардов

### 2. Scatter/Gather

Для распределённых запросов используется паттерн scatter/gather:

```
[Coordinator] -> [Scatter] -> [Shard 1], [Shard 2], [Shard 3]
[Shard 1], [Shard 2], [Shard 3] -> [Gather] -> [Coordinator]
[Coordinator] -> [Merge Top-K] -> [Final Results]
```

### 3. Actor Model

Каждый компонент системы реализует actor-подобный паттерн:

- **State Encapsulation**: Каждый модуль инкапсулирует свое состояние
- **Message Passing**: Взаимодействие через вызовы функций
- **Concurrency**: Безопасная многопоточность через Arc/RwLock

### 4. Functional Reactive Programming

Для обработки запросов используется FRP подход:

- **Immutable Data**: AST и другие структуры неизменяемы
- **Pure Functions**: Функции обработки не имеют побочных эффектов
- **Event Streams**: Запросы обрабатываются как поток событий

## Оптимизации производительности

### 1. Query Caching

- **LRU Strategy**: Least Recently Used для управления кэшем
- **TTL Expiration**: Время жизни кэшированных результатов
- **Hash-based Keys**: Уникальные ключи на основе содержимого запроса

### 2. Parallel Processing

- **Rayon Integration**: Использование rayon для параллельных вычислений
- **Work Stealing**: Алгоритм распределения задач между потоками
- **Embarrassingly Parallel**: Поиск может быть легко распараллелен

### 3. Early Termination

- **Limit-Based**: Прекращение поиска при достижении лимита
- **Threshold-Based**: Остановка при достижении порога расстояния
- **Resource-Based**: Ограничение по времени или памяти

### 4. Memory Management

- **Pool Allocation**: Пулы для часто используемых структур
- **Zero-Copy Operations**: Минимизация копирования данных
- **Lazy Evaluation**: Вычисления только при необходимости

## Безопасность

### 1. Authentication

- **JWT Tokens**: JSON Web Tokens с настраиваемым сроком действия
- **Secret Rotation**: Возможность смены секретных ключей
- **Token Validation**: Проверка подписи и срока действия

### 2. Authorization

- **Role-Based Access Control**: Разные уровни доступа
- **Permission Scopes**: Ограничение по операциям и данным
- **Audit Logging**: Журнал всех операций

### 3. Data Protection

- **TLS Encryption**: Шифрование всех соединений
- **Input Validation**: Проверка всех входных данных
- **SQL Injection Prevention**: Защита от инъекций

## Мониторинг и метрики

### 1. Prometheus Integration

- **Custom Metrics**: Специфичные для ToroidalDB метрики
- **Standard Metrics**: HTTP запросы, время ответа, ошибки
- **Performance Counters**: Операции поиска, транзакции, кэш

### 2. Tracing

- **Structured Logging**: Подробные логи с контекстом
- **Request IDs**: Отслеживание запросов через всю систему
- **Performance Profiling**: Измерение времени выполнения операций

## Масштабируемость

### 1. Horizontal Scaling

- **Sharding**: Распределение данных по нескольким узлам
- **Load Balancing**: Равномерное распределение запросов
- **Replication**: Репликация для отказоустойчивости

### 2. Vertical Scaling

- **Multi-threading**: Использование всех ядер процессора
- **SIMD Operations**: Векторные инструкции для вычислений
- **Memory Optimization**: Эффективное использование памяти

## Тестирование

### 1. Unit Tests

- **Property-Based Testing**: Тестирование свойств структур
- **Edge Cases**: Проверка граничных условий
- **Error Handling**: Тестирование обработки ошибок

### 2. Integration Tests

- **Component Interaction**: Взаимодействие между модулями
- **End-to-End**: Полные сценарии использования
- **Performance**: Тестирование производительности

### 3. Chaos Testing

- **Fault Injection**: Искусственные сбои компонентов
- **Network Partitions**: Тестирование при разделении сети
- **Resource Exhaustion**: Тестирование при нехватке ресурсов

## Деплоймент

### 1. Containerization

- **Docker Support**: Готовые образы для контейнеризации
- **Multi-stage Builds**: Оптимизированные образы
- **Environment Variables**: Настройка через переменные окружения

### 2. Orchestration

- **Kubernetes Manifests**: Готовые манифесты для K8s
- **Service Discovery**: Автоматическое обнаружение сервисов
- **Auto-scaling**: Автоматическое масштабирование по нагрузке

## Заключение

Архитектура ToroidalDB спроектирована для:
- **High Performance**: Быстрое выполнение запросов
- **Scalability**: Масштабирование с ростом данных
- **Reliability**: Отказоустойчивость и надежность
- **Flexibility**: Поддержка различных сценариев использования
- **Security**: Защита данных и аутентификация
- **Maintainability**: Четкая архитектура для легкого сопровождения