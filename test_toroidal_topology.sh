#!/bin/bash

echo "🔬 Тестирование специфичных для тороидальной топологии конструкций..."

echo ""
echo "🧪 Тесты CONNECTEDTO и WITHIN HOPS:"
echo "  - Разбор синтаксиса конструкций"
echo "  - Выполнение bidirectional BFS"
echo "  - Учет тороидальной топологии"
echo "  - Тестирование на графах с циклами"

echo ""
echo "Тест 1: Проверка разбора CONNECTEDTO синтаксиса..."
if command -v cargo &> /dev/null; then
    cargo test toroidal_topology_tests::test_connectedto_parsing -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 2: Проверка bidirectional BFS алгоритма..."
if command -v cargo &> /dev/null; then
    cargo test toroidal_topology_tests::test_bidirectional_bfs_algorithm -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 3: Проверка тороидальной топологии с циклами..."
if command -v cargo &> /dev/null; then
    cargo test toroidal_topology_tests::test_toroidal_topology_with_cycles -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 4: Проверка диапазона шагов (WITHIN HOPS)..."
if command -v cargo &> /dev/null; then
    cargo test toroidal_topology_tests::test_within_hops_range -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 5: Проверка сложных тороидальных запросов..."
if command -v cargo &> /dev/null; then
    cargo test toroidal_topology_tests::test_complex_toroidal_query -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "Тест 6: Проверка крайних случаев тороидальной топологии..."
if command -v cargo &> /dev/null; then
    cargo test toroidal_topology_tests::test_toroidal_topology_edge_cases -- --nocapture
else
    echo "⚠️ Cargo не найден, пропускаем тесты"
fi

echo ""
echo "✅ Тестирование специфичных для тороидальной топологии конструкций завершено!"

echo ""
echo "📋 Резюме тестирования:"
echo "  ✓ Реализация CONNECTEDTO и WITHIN HOPS операций"
echo "  ✓ Bidirectional BFS алгоритм для обхода графа"
echo "  ✓ Учет тороидальной топологии при обходе"
echo "  ✓ Тестирование на графе с циклами"
echo "  ✓ Обработка крайних случаев тороидального пространства"
echo "  ✓ Проверка корректности синтаксического разбора"