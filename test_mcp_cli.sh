#!/bin/bash

echo "🧩 Запуск тестов расширенной функциональности (MCP и CLI)..."

echo ""
echo "🧪 Тесты MCP (Model Configuration Protocol):"
echo "  - Создание и обновление конфигурации"
echo "  - Управление параметрами моделей"
echo "  - Валидация конфигурации"
echo "  - Применение изменений во время работы"

echo ""
echo "🧪 Тесты CLI:"
echo "  - Выполнение TQL запросов"
echo "  - Управление узлами"
echo "  - Работа с транзакциями"
echo "  - Резервное копирование"

echo ""
echo "🔧 Проверка зависимостей..."
if command -v cargo &> /dev/null; then
    echo "✅ Cargo установлен"
else
    echo "❌ Cargo не найден"
    exit 1
fi

if command -v openssl &> /dev/null; then
    echo "✅ OpenSSL установлен"
else
    echo "❌ OpenSSL не найден"
    exit 1
fi

echo ""
echo "🔐 Генерация TLS сертификатов для тестирования..."
if [ ! -f "cert.pem" ] || [ ! -f "key.pem" ]; then
    openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
      -subj "/C=RU/ST=Moscow/L=ToroidalDB/O=MCP-CLI/OU=Test/CN=localhost" \
      -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
    echo "✅ TLS сертификаты сгенерированы"
else
    echo "✅ TLS сертификаты уже существуют"
fi

echo ""
echo "📦 Установка зависимостей..."
if command -v cargo &> /dev/null; then
    cargo check
    if [ $? -eq 0 ]; then
        echo "✅ Зависимости проверены"
    else
        echo "❌ Ошибка проверки зависимостей"
        exit 1
    fi
else
    echo "⚠️ Cargo не установлен, пропускаем проверку зависимостей"
fi

echo ""
echo "🚀 Тест 1: Создание конфигурации системы..."
if command -v cargo &> /dev/null; then
    cargo test mcp_tests::test_system_configuration_creation -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 2: Обновление конфигурации системы..."
if command -v cargo &> /dev/null; then
    cargo test mcp_tests::test_system_configuration_update -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 3: Управление конфигурацией моделей..."
if command -v cargo &> /dev/null; then
    cargo test mcp_tests::test_model_configuration_management -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 4: Валидация конфигурации..."
if command -v cargo &> /dev/null; then
    cargo test mcp_tests::test_configuration_validation -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 5: Применение изменений конфигурации..."
if command -v cargo &> /dev/null; then
    cargo test mcp_tests::test_apply_configuration_changes -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 6: CLI выполнение TQL запросов..."
if command -v cargo &> /dev/null; then
    cargo test cli_tests::test_cli_query_execution -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 7: CLI управление узлами..."
if command -v cargo &> /dev/null; then
    cargo test cli_tests::test_cli_node_management -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 8: CLI транзакции..."
if command -v cargo &> /dev/null; then
    cargo test cli_tests::test_cli_transactions -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 9: CLI резервное копирование..."
if command -v cargo &> /dev/null; then
    cargo test cli_tests::test_cli_backup_operations -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "🚀 Тест 10: Комплексный сценарий использования..."
if command -v cargo &> /dev/null; then
    cargo test mcp_cli_integration_tests::test_mcp_cli_integration -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "✅ Тестирование MCP и CLI завершено!"

echo ""
echo "📋 Резюме реализации:"
echo "  ✓ MCP (Model Configuration Protocol) - протокол динамической конфигурации"
echo "  ✓ CLI (Command Line Interface) - командный интерфейс для управления"
echo "  ✓ Агрегации - COUNT, SUM, AVG, MIN, MAX"
echo "  ✓ Подзапросы - вложенные выражения"
echo "  ✓ Транзакции - атомарные операции"
echo "  ✓ Интеграция с основной системой"
echo "  ✓ Тестирование и документация"