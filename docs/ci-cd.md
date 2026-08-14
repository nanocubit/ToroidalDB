# CI/CD Процесс для ToroidalDB

## Обзор

CI/CD (Continuous Integration/Continuous Deployment) процесс для ToroidalDB обеспечивает автоматизированную проверку, тестирование и деплоймент приложения. Процесс включает в себя несколько этапов, начиная с проверки кода и заканчивая деплоем в production.

## Архитектура CI/CD

### Основные компоненты

1. **GitHub Actions** - основная система для выполнения CI/CD пайплайнов
2. **Docker** - контейнеризация приложения
3. **Cargo** - система сборки и тестирования Rust
4. **Make** - утилита для автоматизации задач

### Этапы CI/CD

#### 1. Проверка кода (Code Quality)
- Проверка форматирования с помощью `cargo fmt`
- Линтинг кода с помощью `cargo clippy`
- Проверка зависимостей с помощью `cargo deny` и `cargo audit`
- Статический анализ безопасности

#### 2. Тестирование
- **Модульные тесты**: `cargo test --lib`
- **Интеграционные тесты**: `cargo test --test '*'`
- **Бенчмарки**: `cargo bench`
- **Покрытие кода**: `cargo tarpaulin`

#### 3. Сборка
- Сборка релизной версии: `cargo build --release`
- Сборка Docker образа
- Тегирование образа

#### 4. Тестирование образа
- Запуск контейнера для тестирования
- Проверка работоспособности приложения
- Проверка API endpoints

#### 5. Деплоймент
- Деплой в staging среду
- Деплой в production (при наличии тега релиза)

## Конфигурация GitHub Actions

Файл `.github/workflows/ci-cd.yml` определяет пайплайн:

```yaml
name: ToroidalDB CI/CD Pipeline

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      uses: dtolnay/rust-toolchain@stable
      with:
        toolchain: stable
        components: rustfmt, clippy
    - name: Cache dependencies
      uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
          target
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    - name: Check formatting
      run: cargo fmt --all -- --check
    - name: Run clippy
      run: cargo clippy -- -D warnings
    - name: Run tests
      run: cargo test --verbose
    - name: Build release
      run: cargo build --release --verbose
```

## Makefile задачи

Makefile предоставляет удобные команды для автоматизации:

```makefile
# Сборка приложения
make build

# Запуск тестов
make test

# Запуск линтера
make lint

# Сборка Docker образа
make docker-build

# Запуск в Docker
make docker-run

# Деплой в staging
make deploy-staging

# Деплой в production
make deploy-prod

# Полная проверка
make check
```

## Docker контейнеризация

Dockerfile оптимизирован для минимального размера и безопасности:

```dockerfile
# Многоступенчатая сборка
FROM rust:1.78-slim-bookworm as builder
# ...

FROM debian:bookworm-slim
# Финальный образ с минимальными зависимостями
```

## Деплоймент стратегии

### Staging деплоймент
- Автоматический при пуше в main
- Использует тег `staging-{commit-sha}`
- Проверяет работоспособность перед деплоем

### Production деплоймент
- Ручной или по тегу релиза
- Требует ручного подтверждения
- Использует стратегию zero-downtime

## Безопасность

### Проверки безопасности
- `cargo audit` - проверка уязвимостей в зависимостях
- `cargo deny` - политики зависимостей
- Статический анализ кода
- Проверка секретов

### Защита
- Использование non-root пользователя в контейнере
- Минимальные разрешения
- TLS шифрование
- JWT аутентификация

## Мониторинг и метрики

### Сбор метрик
- Prometheus endpoint `/metrics`
- Логирование производительности
- Мониторинг ресурсов

### Алертинг
- Проверка работоспособности
- Мониторинг ошибок
- Уведомления о сбоях

## Автоматизация тестирования

### Unit тесты
- Тестирование отдельных компонентов
- Быстрое выполнение
- Покрытие логики

### Интеграционные тесты
- Тестирование взаимодействия компонентов
- Тестирование API
- Проверка сценариев использования

### Нагрузочное тестирование
- Проверка производительности
- Тестирование масштабируемости
- Проверка стабильности

## Процесс релиза

### Подготовка релиза
1. Обновление версии в `Cargo.toml`
2. Обновление CHANGELOG
3. Создание тега релиза

### Проверки перед релизом
- Все тесты должны проходить
- Покрытие кода не менее 80%
- Безопасность проверена
- Документация обновлена

### Деплой релиза
1. Автоматическая сборка
2. Тестирование в staging
3. Ручное подтверждение
4. Деплой в production

## Управление конфигурацией

Файл `config/ci-cd.toml` содержит настройки для различных сред:

```toml
[build]
release_profile = "release"
opt_level = 3
lto = true

[testing]
unit_tests = true
integration_tests = true
security_tests = true

[deployment]
default_environment = "development"
zero_downtime_deploy = true
```

## Скрипты автоматизации

### run_ci_cd.sh
Полный скрипт для запуска всех этапов CI/CD локально:

- Проверка зависимостей
- Запуск всех тестов
- Сборка и тестирование Docker образа
- Сбор метрик производительности
- Проверка покрытия кода

### deploy.sh
Скрипт для деплоя в различные среды:

- Поддержка development, staging, production
- Автоматическая генерация сертификатов
- Проверка работоспособности
- Обработка ошибок

## Best Practices

### Код
- Следование Rust best practices
- Использование clippy для стиля кода
- Проверка форматирования
- Покрытие тестами

### Тестирование
- Модульные тесты для каждой функции
- Интеграционные тесты для сценариев
- Тесты безопасности
- Проверка производительности

### Деплоймент
- Zero-downtime деплои
- Rollback возможности
- Мониторинг после деплоя
- Постепенное развертывание

## Заключение

CI/CD процесс для ToroidalDB обеспечивает:
- Автоматизированную проверку качества кода
- Комплексное тестирование
- Безопасную и надежную доставку
- Мониторинг и обратную связь
- Быструю идентификацию проблем
- Стабильность production среды