# Инструкции по запуску ToroidalDB

## Установка Rust

Для запуска ToroidalDB вам понадобится Rust. Установите его следующим образом:

```bash
# Установка Rust через rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Проверка установки
rustc --version
cargo --version
```

## Установка зависимостей

```bash
# Установка необходимых инструментов
cargo install cargo-watch
cargo install cargo-tarpaulin
cargo install cargo-deny
cargo install ripgrep

# Установка зависимостей проекта
cargo build
```

## Запуск тестов

```bash
# Запуск всех тестов
cargo test

# Запуск конкретных тестов
cargo test extended_functionality_tests
cargo test distributed_execution_tests
cargo test toroidal_topology_tests
cargo test performance_optimization_tests

# Запуск бенчмарков
cargo bench

# Проверка форматирования
cargo fmt --check

# Линтинг кода
cargo clippy -- -D warnings

# Проверка безопасности
cargo deny check
cargo audit
```

## Запуск сервера

```bash
# Генерация TLS сертификатов
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/C=RU/ST=Moscow/L=ToroidalDB/O=Production/OU=Server/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

# Запуск сервера
cargo run --release
```

## Использование CLI

```bash
# Выполнение TQL запроса
cargo run --bin toroidal-cli -- query "MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id LIMIT 10"

# Управление узлами
cargo run --bin toroidal-cli -- nodes create 1 --vector "[0.1, 0.2, 0.3]" --properties '{"name": "example"}'

# Резервное копирование
cargo run --bin toroidal-cli -- backup create --description "Daily backup"
```

## Docker

```bash
# Сборка Docker образа
docker build -t toroidal-db:latest .

# Запуск контейнера
docker run -d --name toroidal-db -p 8443:8443 \
  -v ./data:/data \
  -e JWT_SECRET="your-secret-key-change-in-production" \
  toroidal-db:latest
```

## Kubernetes

Для деплоя в Kubernetes используйте файлы из директории `k8s/`:

```bash
kubectl apply -f k8s/prod-values.yaml
```

## Тестирование производительности

```bash
# Запуск тестов производительности
cargo test performance_optimization_tests -- --nocapture

# Запуск нагрузочного тестирования
cargo bench -- --nocapture
```

## Запуск полной проверки

```bash
# Использование скрипта полной проверки
./run_full_verification.sh
```

## Архитектурные тесты

```bash
# Тесты агрегаций
cargo test extended_functionality_tests::test_aggregation_functions

# Тесты подзапросов
cargo test extended_functionality_tests::test_subquery_execution

# Тесты транзакций
cargo test extended_functionality_tests::test_transaction_operations

# Тесты распределённого выполнения
cargo test distributed_execution_tests::test_scatter_gather_mechanism

# Тесты топологических операций
cargo test toroidal_topology_tests::test_bidirectional_bfs_algorithm

# Тесты оптимизаций производительности
cargo test performance_optimization_tests::test_query_caching_performance
```

## CI/CD

Система включает полный CI/CD pipeline:

- Автоматическая проверка форматирования и линтинг
- Запуск всех тестов
- Проверка безопасности
- Сборка Docker образов
- Деплоймент в staging и production

Смотрите файл `.github/workflows/full_verification.yml` для деталей.

## Документация

Полная документация доступна в директории `docs/tql/`:

- `guide.md` - Руководство по TQL v2.0
- `architecture.md` - Архитектурные решения
- `performance.md` - Оптимизация производительности
- `distributed_execution.md` - Распределённое выполнение
- `extended_functionality.md` - Расширенная функциональность
- `toroidal_topology.md` - Топологические операции