#!/usr/bin/env python3

import os
import re
import sys

def fix_imports_in_file(file_path):
    """Исправляет импорты storage:: на hybrid_storage:: в указанном файле"""
    try:
        with open(file_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # Заменяем импорты
        content = re.sub(r'use crate::storage::PersistentStore', 'use crate::hybrid_storage::HybridPersistentStore', content)
        content = re.sub(r'Arc::new\(\s*PersistentStore::', 'Arc::new(HybridPersistentStore::', content)
        content = re.sub(r'crate::storage::Node {', 'crate::hybrid_storage::Node {', content)
        content = re.sub(r'crate::storage::Edge {', 'crate::hybrid_storage::Edge {', content)
        content = re.sub(r'crate::storage::\{', 'crate::hybrid_storage::{', content)
        
        with open(file_path, 'w', encoding='utf-8') as f:
            f.write(content)
        
        print(f"✅ {file_path}")
        return True
    except Exception as e:
        print(f"❌ {file_path}: {e}")
        return False

def main():
    # Список файлов для исправления
    files_to_fix = [
        "src/tql/extended_functionality_tests.rs",
        "src/tql/executor.rs", 
        "src/tql/distributed_execution_tests.rs",
        "src/tql/backup.rs",
        "src/visualization/etl_pipeline.rs",
        "src/visualization/bi_connectors.rs", 
        "src/visualization/vector_projector.rs",
        "src/visualization/topology_mapper.rs",
        "src/visualization/graph_visualizer.rs",
        "src/tql/integration_tests.rs",
        "src/tql/complete_extended_functionality_test.rs",
        "src/tql/coordinator.rs",
        "src/tql/transaction.rs",
        "src/tql/toroidal_topology_tests.rs",
        "src/tql/mcp_cli_tests.rs",
        "src/tql/hybrid_search_tests.rs",
        "src/tql/graph_traversal_tests.rs",
        "src/tql/comprehensive_hybrid_tests.rs",
        "src/tql/graph.rs",
        "src/benches/use_case_test.rs",
        "src/benches/tql_benchmark.rs",
        "src/benches/stability_test.rs"
    ]
    
    print("🔧 Исправление импортов в ToroidalDB...")
    
    success_count = 0
    total_count = len(files_to_fix)
    
    for file_path in files_to_fix:
        if os.path.exists(file_path):
            if fix_imports_in_file(file_path):
                success_count += 1
        else:
            print(f"⚠️  Файл не найден: {file_path}")
    
    print(f"\n🎯 Готово! Исправлено {success_count}/{total_count} файлов")
    print("💡 Теперь можно запустить тесты: cargo test")

if __name__ == "__main__":
    main()