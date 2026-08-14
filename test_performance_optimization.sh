#!/bin/bash

echo "🚀 Запуск тестирования оптимизации производительности TQL v2.0..."

echo ""
echo "🧪 Тесты оптимизации производительности:"
echo "  - Кэширование запросов"
echo "  - Параллельный поиск через rayon"
echo "  - Ранняя остановка при достижении лимита"
echo "  - Замеры производительности"

echo ""
echo "Тест 1: Производительность кэширования запросов..."
if command -v cargo &> /dev/null; then
    cargo test performance_optimization_tests::test_query_caching_performance -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 2: Производительность параллельного поиска..."
if command -v cargo &> /dev/null; then
    cargo test performance_optimization_tests::test_parallel_search_performance -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 3: Оптимизация ранней остановки..."
if command -v cargo &> /dev/null; then
    cargo test performance_optimization_tests::test_early_stopping_optimization -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 4: Производительность на больших наборах данных..."
if command -v cargo &> /dev/null; then
    cargo test performance_optimization_tests::test_large_scale_performance -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 5: Оптимизация использования памяти..."
if command -v cargo &> /dev/null; then
    cargo test performance_optimization_tests::test_memory_usage_optimization -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "✅ Тестирование оптимизации производительности завершено!"

echo ""
echo "📋 Резюме оптимизаций:"
echo "  ✓ Кэширование запросов - ускорение повторных запросов"
echo "  ✓ Параллельный поиск через rayon - ускорение обработки"
echo "  ✓ Ранняя остановка при достижении лимита - экономия ресурсов"
echo "  ✓ Замеры и оптимизация производительности - мониторинг эффективности"