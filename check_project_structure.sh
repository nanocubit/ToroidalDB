#!/bin/bash

echo "📋 Проверка структуры проекта ToroidalDB v3.1.0..."

echo ""
echo "📁 Структура директорий:"
tree /Users/Vladimir/ToroidalDB 2>/dev/null || find /Users/Vladimir/ToroidalDB -type d | sort

echo ""
echo "📄 Основные файлы проекта:"
find /Users/Vladimir/ToroidalDB -name "*.rs" -not -path "*/target/*" -not -path "*/.git/*" -exec ls -la {} \; | head -20

echo ""
echo "📦 Зависимости в Cargo.toml:"
cat /Users/Vladimir/ToroidalDB/Cargo.toml | grep -A 20 "\[dependencies\]"

echo ""
echo "🔧 Модули TQL v2.0:"
find /Users/Vladimir/ToroidalDB/src/tql -name "*.rs" -exec basename {} \;

echo ""
echo "🧪 Тесты и бенчмарки:"
find /Users/Vladimir/ToroidalDB/src -name "*test*" -o -name "*bench*" | head -15

echo ""
echo "📚 Документация:"
find /Users/Vladimir/ToroidalDB/docs -name "*.md" | head -10

echo ""
echo "✅ Структура проекта проверена!"
echo ""
echo "Архитектура TQL v2.0:"
echo "  - /src/tql/ast.rs: Абстрактное синтаксическое дерево"
echo "  - /src/tql/parser.rs: Парсер TQL запросов"
echo "  - /src/tql/executor.rs: Исполнитель запросов"
echo "  - /src/tql/graph.rs: Графовые операции"
echo "  - /src/tql/coordinator.rs: Координатор распределённых запросов"
echo "  - /src/tql/transaction.rs: Управление транзакциями"
echo "  - /src/tql/mod.rs: Модуль экспорта"
echo "  - /src/storage.rs: Модуль хранения с кэшированием"
echo "  - /src/main.rs: Главный файл с интеграцией всех компонентов"