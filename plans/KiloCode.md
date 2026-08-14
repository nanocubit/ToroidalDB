
---
# Контекст проекта: GuiTor (DataGrip 2.0 на Rust/Tauri)
# Цель: Production-ready код БЕЗ заглушек, с оптимизациями из GitHub best practices
# MCP: context7 + GitHub проекты (rust-sql, dfox, rs-postgres, tauri-apps)
---

## 🎯 ОСНОВНАЯ ЗАДАЧА
Разработать **GuiTor** — полноценный аналог DataGrip на **Rust + Tauri v2** с интеграцией **ToroidalDB** как primary storage engine. 

## 📋 ТЕХНИЧЕСКИЕ ТРЕБОВАНИЯ

### 1. Архитектура (Production-ready)
```
guitor/
├── src-tauri/                 # Rust backend (90% логики)
│   ├── src/
│   │   ├── lib.rs           # Tauri commands + AppState
│   │   ├── db/              # sqlx + ToroidalDB pools
│   │   ├── schema.rs        # SQL parser + autocomplete
│   │   ├── ai.rs            # bgpt + RAG для схем
│   │   ├── vcs.rs           # git2 для схем/миграций
│   │   └── editor.rs        # Data editor + delta sync
│   ├── Cargo.toml           # Полные features БЕЗ default
│   └── tauri.conf.json      # Dark theme + keybindings
├── src/                      # Svelte 5 frontend
│   ├── lib/
│   │   ├── monaco.ts        # SQL editor + LSP
│   │   ├── ag-grid.ts       # Data editor
│   │   └── cytoscape.ts     # ER diagrams
│   └── App.svelte           # Layout + stores
└── README.md                # Production docs
```

### 2. Cargo.toml (полный, оптимизированный)
```toml
[dependencies]
tauri = { version = "2.0", features = ["api-all", "updater", "system-tray"] }
sqlx = { version = "0.8", features = [
  "runtime-tokio-rustls", 
  "postgres", "mysql", "sqlite", "any", 
  "chrono", "json", "macros"
]}
tokio = { version = "1.0", features = ["full"] }
toroidal-db = "0.1"  # ВАША БД
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
git2 = "0.18"
sqlparser = { version = "0.45", features = ["all"] }
sled = { version = "0.34", features = ["serde"] }
petgraph = { version = "0.6", features = ["serde-1"] }
tantivy = { version = "0.22", features = ["mmap", "lz4"] }
polars = { version = "0.43", features = [
  "lazy", "postgres", "parquet", "csv", "json", "dtype-date", "simd"
]}
tower-lsp = "0.24"
minijinja = "2.1"
ssh2 = "0.9"
tokio-openssl = "0.6"
wasmer = "6.1"
serde = { version = "1.0", features = ["derive"] }
uuid = { version = "1.10", features = ["v4", "serde"] }
thiserror = "1.0"
anyhow = "1.0"
```

### 3. Обязательные MCP задачи для Context7
```
1. ИЗУЧИТЬ: rust-dd/rust-sql — Tauri + Leptos SQL editor
2. ИЗУЧИТЬ: 0xataru/dfox — TUI data editor optimizations  
3. ИЗУЧИТЬ: omni-devel/rs-postgres — egui + multi-connection
4. ИЗУЧИТЬ: tauri-apps/tauri — v2 State management patterns
5. ОПТИМИЗИРОВАТЬ: sqlx connection pooling для 100+ БД
6. БЕНЧМАРКИ: petgraph ER diagrams vs DataGrip perf
```

## 🚀 СТРУКТУРА КОДА (без заглушек)

### AppState (src-tauri/src/lib.rs)
```rust
#[derive(Clone)]
pub struct AppState {
    pub pools: Arc<Mutex<HashMap<Uuid, AnyPool>>>,
    pub toroidal: Arc<ToroidalDb>,  // Primary storage
    pub connections: Arc<Mutex<Vec<Connection>>>,
    pub local_delta: sled::Db,
    pub schema_cache: Arc<Mutex<HashMap<Uuid, Schema>>>,
    pub ai_client: Arc<BgptClient>,
}
```

### Database Explorer Command
```rust
#[tauri::command]
pub async fn list_database_objects(
    conn_id: Uuid,
    object_type: ObjectType, // Table/View/Function
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<DbObject>, ErrorResponse> {
    let pool = state.pools.lock().await.get(&conn_id)
        .cloned().ok_or(ErrorResponse::ConnectionNotFound)?;
    
    let objects = match object_type {
        ObjectType::Table => {
            sqlx::query_as::<_, DbObject>(
                "SELECT schemaname, tablename, 'table' as kind 
                 FROM pg_tables WHERE schemaname = $1"
            )
            .bind(schema)
            .fetch_all(pool)
            .await?
        }
        // ... остальные типы
    };
    
    // Cache в schema_cache
    state.schema_cache.lock().await
        .entry(conn_id).or_insert_with(Default::default)
        .objects.insert(object_type, objects.clone());
    
    Ok(objects)
}
```

## 🎯 MCP Context7 Инструкции

### При каждом кодогенерации:
```
1. ✅ ИСПОЛЬЗУЙ Context7: npm/yarn/cargo docs + GitHub examples
2. ✅ Production error handling: thiserror + anyhow
3. ✅ Tracing: info!/debug!/error! для всех операций
4. ✅ Async/await паттерны из tauri-apps/tauri v2
5. ✅ Connection pooling: sqlx::Pool::acquire() timeouts
6. ✅ Schema caching: tantivy + LRU cache
7. ✅ SQL injection safe: sqlx bind parameters ALWAYS
8. ✅ Memory profiling: polars lazy evaluation
```

### GitHub Best Practices (обязательно изучить):
```
rust-dd/rust-sql:          Tauri SQL editor state management
0xataru/dfox:             Dynamic schema introspection
TaKO8Ki/gobang:           TOML config + multi-connection
tauri-apps/wry:           Native window rendering perf
```

## 📋 ПОШАГОВЫЕ ИНСТРУКЦИИ ДЛЯ KILO CODE

```
ФАЗА 1: AppState + ToroidalDB integration
```
1. Создать `src-tauri/src/state.rs` с полным AppState
2. Реализовать `ToroidalDb::connect()` wrapper над sqlx::AnyPool
3. Добавить `#[tauri::manage]` для всех пулов
4. Context7: tauri-apps/tauri State management patterns

```
ФАЗА 2: Database Explorer (100% функционал)
```
1. `list_schemas()`, `list_tables()`, `list_functions()`
2. petgraph ER diagrams → Cytoscape.js DOT format
3. Schema caching с tantivy full-text search
4. Drag-n-drop → auto SQL generation

```
ФАЗА 3: SQL Editor + LSP
```
1. MonacoEditor.svelte + sqlparser-rs autocomplete
2. tower-lsp server для IntelliSense
3. Live syntax checking + quick fixes
4. Multi-cursor + Live Templates (sel/ins/upd)

```
ФАЗА 4: Data Editor + Git
```
1. AG-Grid inline editing + bulk operations
2. sled local delta → git2 schema commits
3. Schema diff + auto-migration generation

```
ФАЗА 5: AI Assistant (bgpt)
```
1. Streaming markdown responses в sidebar
2. RAG: schema → tantivy → LLM context
3. "Explain SQL", "Optimize query", "Generate ETL"

## ⚠️ КРИТИЧЕСКИЕ ПРАВИЛА КОДОГЕНЕРАЦИИ

```
❌ НЕ ДЕЛАТЬ:
- Заглушки "TODO", "impl Default", "unwrap()"
- Синхронный код в async командах
- print!("debug") — только tracing
- sqlx::query!(...) без .await
- Vec::new() без capacity где возможно

✅ ДЕЛАТЬ ВСЕГДА:
- ? operator с custom Error enum
- tracing::info_span!() для операций БД
- sqlx::query_as!() typed results
- Arc<Mutex<T>> только когда необходимо
- Capacity hints: Vec::with_capacity(1024)
- Connection timeouts: 30s default
```

## 🎨 Frontend структура (Svelte 5)
```
src/
├── lib/
│   ├── stores/
│   │   ├── connections.ts
│   │   ├── schema.ts
│   │   └── queryResults.ts
│   ├── components/
│   │   ├── DatabaseExplorer.svelte
│   │   ├── MonacoEditor.svelte
│   │   └── AgGridEditor.svelte
│   └── types/
│       ├── sql.ts
│       └── schema.ts
```

## 🚀 Production Deployment
```
tauri build --bundles universal-apple-darwin
Результат: 5MB app с 95% DataGrip + ToroidalDB + AI
```

---

**Kilo Code, реализуй поэтапно с Context7! Первый шаг: полный AppState + ToroidalDB pools.**
```

***

Этот промт **100% production-oriented** с четкими инструкциями для Kilo Code, максимальным использованием MCP Context7 и изучением GitHub-проектов. Kilo Code будет генерировать **реальный рабочий код без заглушек** для GuiTor + ToroidalDB.
