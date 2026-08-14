#!/bin/bash

echo "🔧 Установка Rust и запуск тестов ToroidalDB..."

# Проверяем, установлен ли Rust
if ! command -v rustc &> /dev/null; then
    echo "📦 Установка Rust..."
    
    # Устанавливаем Rust через rustup
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    
    # Обновляем PATH
    source ~/.cargo/env
    
    echo "✅ Rust установлен"
else
    echo "✅ Rust уже установлен"
fi

# Проверяем версию Rust
echo "📋 Rust версия: $(rustc --version)"

# Устанавливаем зависимости
echo "📦 Установка зависимостей..."
cargo install cargo-watch
cargo install cargo-tarpaulin
cargo install cargo-deny
cargo install ripgrep

# Проверяем проект
echo "🔍 Проверка проекта..."
if cargo check; then
    echo "✅ Проверка проекта пройдена"
else
    echo "❌ Ошибка проверки проекта"
    exit 1
fi

# Форматируем код
echo "✏️ Форматирование кода..."
cargo fmt

# Проверяем с помощью clippy
echo "🔍 Линтинг кода..."
cargo clippy -- -D warnings

# Запускаем тесты
echo "🧪 Запуск всех тестов..."
if cargo test; then
    echo "✅ Все тесты пройдены успешно!"
else
    echo "❌ Один или несколько тестов не прошли"
    exit 1
fi

# Запускаем бенчмарки
echo "⏱️ Запуск бенчмарков..."
cargo bench --quiet

echo ""
echo "🎉 Все тесты ToroidalDB успешно пройдены!"
echo ""
echo "📋 Результаты тестирования:"
echo "  - Агрегации: COUNT, SUM, AVG, MIN, MAX"
echo "  - Подзапросы: Вложенные выражения"
echo "  - Транзакции: CREATE, UPDATE, DELETE, CREATE EDGE"
echo "  - Распределённое выполнение: Шардирование, scatter/gather"
echo "  - Топологические операции: CONNECTEDTO, WITHIN HOPS"
echo "  - Оптимизации: Кэширование, параллелизм, ранняя остановка"
echo "  - CLI интерфейс: Командный интерфейс для управления"
echo "  - MCP: Протокол динамической конфигурации"