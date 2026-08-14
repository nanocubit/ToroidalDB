#!/bin/bash

echo "🔧 Исправление оставшихся импортов в stability_test.rs..."

# Файл для исправления
FILE="src/benches/stability_test.rs"

# Резервная копия
cp "$FILE" "${FILE}.bak"

# Критические исправления
sed -i '' 's/Arc::new(PersistentStore::/Arc::new(HybridPersistentStore::/g' "$FILE"
sed -i '' 's/PersistentStore::open(/HybridPersistentStore::open(/g' "$FILE"
sed -i '' 's/let node = Node {/let node = crate::hybrid_storage::Node {/g' "$FILE"

echo "✅ Импорты в stability_test.rs исправлены"

# Проверка
if [ -f "$FILE" ]; then
    echo "✅ Файл успешно исправлен"
else
    echo "❌ Ошибка при исправлении файла"
fi