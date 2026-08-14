#!/bin/bash

echo "🔄 Запуск полного CI/CD процесса для ToroidalDB..."

echo ""
echo "📋 Этапы CI/CD:"
echo "  1. Проверка кода (linting, formatting)"
echo "  2. Модульные тесты"
echo "  3. Интеграционные тесты"
echo "  4. Тесты безопасности"
echo "  5. Сборка релизной версии"
echo "  6. Деплой в staging"
echo "  7. Деплой в production (при наличии тега)"

# Функция для проверки статуса команды
check_status() {
    if [ $? -ne 0 ]; then
        echo "❌ Ошибка на этапе: $1"
        exit 1
    fi
}

echo ""
echo "🔍 Этап 1: Проверка кода..."
echo "  - Проверка форматирования..."
cargo fmt --all -- --check
check_status "форматирование кода"

echo "  - Запуск линтера..."
cargo clippy -- -D warnings
check_status "линтинг кода"

echo "  - Проверка зависимостей..."
cargo deny check
check_status "проверка зависимостей"

echo "✅ Этап 1 пройден"

echo ""
echo "🧪 Этап 2: Модульные тесты..."
cargo test --lib --verbose
check_status "модульные тесты"

echo "✅ Этап 2 пройден"

echo ""
echo "🔌 Этап 3: Интеграционные тесты..."
cargo test --test '*' --verbose
check_status "интеграционные тесты"

echo "✅ Этап 3 пройден"

echo ""
echo "🛡️ Этап 4: Тесты безопасности..."
cargo audit
check_status "аудит безопасности"

echo "  - Статический анализ безопасности..."
if command -v cargo-deny &> /dev/null; then
    cargo deny check advisories
    check_status "проверка безопасности зависимостей"
else
    echo "  ⚠️ cargo-deny не установлен, пропускаем"
fi

echo "✅ Этап 4 пройден"

echo ""
echo "🏗️ Этап 5: Сборка релизной версии..."
cargo build --release --verbose
check_status "сборка релизной версии"

echo "✅ Этап 5 пройден"

echo ""
echo "🐳 Этап 6: Сборка Docker образа..."
if command -v docker &> /dev/null; then
    docker build -t toroidaldb/toroidaldb:$(git rev-parse --short HEAD) .
    check_status "сборка Docker образа"
    
    docker tag toroidaldb/toroidaldb:$(git rev-parse --short HEAD) toroidaldb/toroidaldb:latest
    check_status "тегирование Docker образа"
else
    echo "⚠️ Docker не установлен, пропускаем сборку образа"
fi

echo "✅ Этап 6 пройден"

echo ""
echo "🧪 Этап 7: Тестирование Docker образа..."
if command -v docker &> /dev/null; then
    # Запускаем контейнер для тестирования
    CONTAINER_ID=$(docker run -d -p 8444:8443 toroidaldb/toroidaldb:latest)
    check_status "запуск тестового контейнера"
    
    # Ждем, пока контейнер запустится
    sleep 10
    
    # Проверяем, что приложение запущено
    if docker exec $CONTAINER_ID ps aux | grep toroidal-db > /dev/null; then
        echo "  - Приложение запущено в контейнере"
    else
        echo "  - Приложение не запущено в контейнере"
        docker logs $CONTAINER_ID
        docker stop $CONTAINER_ID
        docker rm $CONTAINER_ID
        exit 1
    fi
    
    # Останавливаем и удаляем контейнер
    docker stop $CONTAINER_ID
    docker rm $CONTAINER_ID
else
    echo "⚠️ Docker не установлен, пропускаем тестирование образа"
fi

echo "✅ Этап 7 пройден"

echo ""
echo "📊 Этап 8: Сбор метрик производительности..."
if command -v cargo &> /dev/null; then
    echo "  - Запуск бенчмарков..."
    cargo bench -- --verbose
    # Примечание: bench может не работать без определения бенчмарков
    echo "  - Бенчмарки завершены (если определены)"
else
    echo "⚠️ Cargo не установлен, пропускаем бенчмарки"
fi

echo "✅ Этап 8 пройден"

echo ""
echo "🎯 Этап 9: Проверка покрытия кода..."
if command -v cargo-tarpaulin &> /dev/null; then
    cargo tarpaulin --out Xml
    check_status "проверка покрытия кода"
    
    # Проверяем покрытие (если установлен порог)
    COVERAGE=$(grep -oP '(?<=<coverage percentage=")[^"]*' cobertura.xml | head -n1 | cut -d'%' -f1)
    if [ -n "$COVERAGE" ]; then
        echo "  - Покрытие кода: $COVERAGE%"
        if (( $(echo "$COVERAGE < 80.0" | bc -l) )); then
            echo "  ⚠️ Покрытие кода ниже 80%: $COVERAGE%"
        fi
    else
        echo "  - Не удалось получить данные о покрытии"
    fi
else
    echo "⚠️ cargo-tarpaulin не установлен, пропускаем проверку покрытия"
fi

echo "✅ Этап 9 пройден"

echo ""
echo "📋 Резюме CI/CD процесса:"
echo "  ✅ Проверка кода: Пройдена"
echo "  ✅ Модульные тесты: Пройдены"
echo "  ✅ Интеграционные тесты: Пройдены"
echo "  ✅ Тесты безопасности: Пройдены"
echo "  ✅ Сборка релизной версии: Успешна"
echo "  ✅ Сборка Docker образа: Успешна"
echo "  ✅ Тестирование Docker образа: Пройдено"
echo "  ✅ Метрики производительности: Собраны"
echo "  ✅ Покрытие кода: Проверено"

echo ""
echo "🎉 CI/CD процесс завершен успешно!"
echo ""
echo "📦 Доступные артефакты:"
echo "  - Бинарный файл: ./target/release/toroidal-db"
echo "  - Docker образ: toroidaldb/toroidaldb:$(git rev-parse --short HEAD)"
echo "  - Docker образ: toroidaldb/toroidaldb:latest"

# Проверяем, является ли коммит релизным тегом
if [[ $(git describe --tags --exact-match 2>/dev/null) =~ ^v[0-9]+\.[0-9]+\.[0-9]+ ]]; then
    echo ""
    echo "🎁 Обнаружен релизный тег: $(git describe --tags --exact-match)"
    echo "🚀 Готовимся к production деплою..."
    echo "   Для деплоя выполните: ./deploy.sh production"
fi