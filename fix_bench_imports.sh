#!/bin/bash

echo "🔧 Исправление оставшихся импортов в bench файлах..."

# Файлы для исправления
BENCH_FILES=(
    "benches/use_case_test.rs"
    "benches/stability_test.rs"
)

for FILE in "${BENCH_FILES[@]}"; do
    if [ -f "src/$FILE" ]; then
        echo "Обработка файла: src/$FILE"
        
        # Резервная копия
        cp "src/$FILE" "src/$FILE.bak"
        
        # Исправления импортов storage:: -> hybrid_storage::
        sed -i '' 's/crate::storage::PersistentStore/crate::hybrid_storage::HybridPersistentStore/g' "src/$FILE"
        sed -i '' 's/Arc::new(PersistentStore::/Arc::new(HybridPersistentStore::g' "src/$FILE"
        sed -i '' 's/crate::storage::Node/crate::hybrid_storage::Node {/g' "src/$FILE"
        
        echo "✅ $FILE исправлен"
    else
        echo "❌ Файл не найден: src/$FILE"
    fi
done

echo "🎯 Все bench файлы исправлены!"
echo "💡 Теперь можно запустить тесты: cargo test"