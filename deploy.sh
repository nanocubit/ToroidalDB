#!/bin/bash

set -e  # Выход при ошибке

echo "🚀 Начинаем процесс деплоя ToroidalDB..."

# Проверяем, что мы в правильной директории
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Cargo.toml не найден. Убедитесь, что вы в корневой директории проекта."
    exit 1
fi

# Определяем тип деплоя
DEPLOY_ENV=${1:-"development"}
echo "🌍 Целевая среда: $DEPLOY_ENV"

# Функция для проверки зависимостей
check_deps() {
    echo "🔍 Проверка зависимостей..."
    
    if ! command -v cargo &> /dev/null; then
        echo "❌ Cargo не установлен"
        exit 1
    fi
    
    if ! command -v docker &> /dev/null; then
        echo "⚠️ Docker не установлен, пропускаем контейнеризацию"
        return 1
    fi
    
    if ! command -v openssl &> /dev/null; then
        echo "❌ OpenSSL не установлен"
        exit 1
    fi
    
    echo "✅ Все зависимости установлены"
}

# Функция для сборки
build_app() {
    echo "🏗️ Сборка приложения..."
    
    # Проверяем форматирование
    echo "  - Проверка форматирования..."
    cargo fmt --all -- --check
    
    # Запускаем линтер
    echo "  - Запуск линтера..."
    cargo clippy -- -D warnings
    
    # Собираем релизную версию
    echo "  - Сборка релизной версии..."
    cargo build --release
    
    echo "✅ Приложение собрано"
}

# Функция для запуска тестов
run_tests() {
    echo "🧪 Запуск тестов..."
    
    # Запускаем все тесты
    cargo test --verbose
    
    echo "✅ Тесты пройдены"
}

# Функция для генерации TLS сертификатов
generate_certs() {
    echo "🔐 Генерация TLS сертификатов..."
    
    # Проверяем, существуют ли уже сертификаты
    if [ -f "cert.pem" ] && [ -f "key.pem" ]; then
        echo "  - Сертификаты уже существуют, пропускаем генерацию"
        return
    fi
    
    # Генерируем самоподписанные сертификаты
    openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
        -subj "/C=RU/ST=Moscow/L=ToroidalDB/O=Security/OU=Production/CN=localhost" \
        -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
    
    echo "✅ TLS сертификаты сгенерированы"
}

# Функция для сборки Docker образа
build_docker_image() {
    if ! command -v docker &> /dev/null; then
        echo "⚠️ Docker не установлен, пропускаем сборку образа"
        return
    fi
    
    echo "🐳 Сборка Docker образа..."
    
    # Собираем Docker образ
    docker build -t toroidaldb/toroidaldb:latest .
    
    echo "✅ Docker образ собран"
}

# Функция для запуска в зависимости от среды
run_deployment() {
    case $DEPLOY_ENV in
        "development")
            echo "💻 Запуск в режиме разработки..."
            # Генерируем сертификаты
            generate_certs
            
            # Запускаем приложение
            echo "  - Запуск ToroidalDB..."
            nohup cargo run > toroidal.log 2>&1 &
            PID=$!
            echo $PID > toroidal.pid
            echo "  - ToroidalDB запущен с PID: $PID"
            ;;
            
        "staging")
            echo "🧪 Запуск в staging среде..."
            build_docker_image
            
            # Запускаем контейнер
            docker run -d --name toroidaldb-staging \
                -p 8443:8443 \
                -v $(pwd)/data:/data \
                -e RUST_LOG=info \
                toroidaldb/toroidaldb:latest
            ;;
            
        "production")
            echo "🏭 Запуск в production среде..."
            build_docker_image
            
            # В реальной системе здесь будет более сложная логика деплоя
            # с использованием Kubernetes, Docker Swarm или других оркестраторов
            echo "  - Подготовка production деплоя..."
            
            # Запускаем контейнер с production параметрами
            docker run -d --name toroidaldb-prod \
                -p 8443:8443 \
                -v /production/data:/data \
                -e RUST_LOG=warn \
                -e JWT_SECRET="$PROD_JWT_SECRET" \
                toroidaldb/toroidaldb:latest
            ;;
            
        *)
            echo "❌ Неизвестная среда: $DEPLOY_ENV"
            echo "Доступные среды: development, staging, production"
            exit 1
            ;;
    esac
}

# Функция для проверки работоспособности
health_check() {
    echo "🏥 Проверка работоспособности..."
    
    # Ждем несколько секунд перед проверкой
    sleep 5
    
    # Проверяем, запущено ли приложение
    if [ "$DEPLOY_ENV" = "development" ]; then
        # Для development проверяем локальный процесс
        if [ -f "toroidal.pid" ]; then
            PID=$(cat toroidal.pid)
            if ps -p $PID > /dev/null; then
                echo "  - ToroidalDB запущен (PID: $PID)"
            else
                echo "  - ToroidalDB не запущен"
                exit 1
            fi
        else
            echo "  - PID файл не найден"
            exit 1
        fi
    else
        # Для Docker проверяем статус контейнера
        CONTAINER_NAME="toroidaldb-$DEPLOY_ENV"
        if docker ps | grep -q "$CONTAINER_NAME"; then
            echo "  - Контейнер $CONTAINER_NAME запущен"
        else
            echo "  - Контейнер $CONTAINER_NAME не запущен"
            exit 1
        fi
    fi
    
    # Проверяем API
    if command -v curl &> /dev/null; then
        echo "  - Проверка API..."
        # Ждем, пока API станет доступен
        for i in {1..30}; do
            if curl -k -f https://localhost:8443/health > /dev/null 2>&1; then
                echo "  - API доступен"
                return
            fi
            echo "  - Ожидание доступности API... ($i/30)"
            sleep 2
        done
        
        echo "❌ API не стал доступен в течение 60 секунд"
        exit 1
    else
        echo "⚠️ curl не установлен, пропускаем проверку API"
    fi
}

# Основной процесс деплоя
main() {
    echo "📋 Начинаем деплой ToroidalDB в среду: $DEPLOY_ENV"
    
    check_deps
    run_tests
    build_app
    generate_certs
    run_deployment
    health_check
    
    echo ""
    echo "🎉 Деплой успешно завершен!"
    echo ""
    echo "📡 ToroidalDB доступен по адресу: https://localhost:8443"
    echo "📊 Health check: https://localhost:8443/health"
    echo "📈 Метрики: https://localhost:8443/metrics"
    echo "🔐 Админка: https://localhost:8443/admin"
    
    if [ "$DEPLOY_ENV" = "development" ]; then
        echo ""
        echo "📝 Для остановки сервера выполните: kill \$(cat toroidal.pid)"
        echo " tail -f toroidal.log"
    fi
}

# Запускаем основной процесс
main "$@"