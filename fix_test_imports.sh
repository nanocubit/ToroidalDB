#!/bin/bash

# Скрипт для исправления импортов в тестовых файлах ToroidalDB
# Заменяет старый storage::PersistentStore на новый hybrid_storage::HybridPersistentStore

echo "🔧 Исправление импортов в тестах ToroidalDB..."

# Файлы для исправления
TEST_FILES=(
    "src/tql/tests.rs"
    "src/tql/basic_syntax_tests.rs"
    "src/tql/performance_optimization_tests.rs"
    "src/tql/extended_functionality_tests.rs"
    "src/tql/distributed_execution_tests.rs"
    "src/tql/integration_tests.rs"
    "src/tql/toroidal_topology_tests.rs"
    "src/tql/hybrid_search_tests.rs"
    "src/tql/graph_traversal_tests.rs"
    "src/tql/comprehensive_hybrid_tests.rs"
    "benches/use_case_test.rs"
    "benches/stability_test.rs"
)

# Паттерны для замены
PATTERNS=(
    "s/use crate::storage::PersistentStore;/use crate::hybrid_storage::HybridPersistentStore;/g"
    "s/PersistentStore::open(/HybridPersistentStore::open(/g"
    "s/crate::storage::Node {/crate::hybrid_storage::Node {/g"
    "s/crate::storage::Edge {/crate::hybrid_storage::Edge {/g"
    "s/Arc::new(PersistentStore::/Arc::new(HybridPersistentStore::/g"
)

# Применяем исправления
for file in "${TEST_FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "Обработка файла: $file"
        
        for pattern in "${PATTERNS[@]}"; do
            if [[ "$OSTYPE" == "Darwin"* ]]; then
                # macOS
                sed -i '' "$pattern" "$file"
            else
                # Linux
                sed -i "$pattern" "$file"
            fi
        done
        
        echo "✅ $file исправлен"
    else
        echo "❌ Файл не найден: $file"
    fi
done

echo "🎯 Все тестовые файлы исправлены!"
echo "💡 Теперь можно запустить тесты: cargo test"