# ToroidalDB — Roadmap внедрения

## ✅ Фаза 1: Ядро (v2.1) — РЕАЛИЗОВАНО
- [x] DDL (CREATE NODE/EDGE TYPE)
- [x] SchemaRegistry
- [x] EXPLAIN / LogicalPlan
- [x] TqlEngine (unified execute)
- [x] Expression Evaluator (триггеры, условия)
- [x] CacheBackend trait + HashMemory (exact-key lookup)
- [x] MapVsaMemory (MAP-VSA fuzzy-кэш)
- [x] ContextModulator (контекстная модуляция расстояния)
- [x] AccessPredictor (частотный prefetch)

## ✅ Фаза 2: Оптимизация (v2.2) — РЕАЛИЗОВАНО
- [x] HINTS (USING GPU, SCATTER, PREFER LOCAL_SHARD)
- [x] CostBasedOptimizer
- [x] Parallel execution
- [x] Predicate pushdown
- [x] Filter AST (must/should/must_not)
- [x] Query Planner (filter-first vs vector-first)
- [x] Filterable HNSW (payload_m)
- [x] Asymmetric quantization (Scalar/Binary/Product)
- [x] BM25 full-text search + TF-кэш
- [x] Magic Set + Semi-naïve evaluation
- [x] SIMILAR_TO оператор в TQL
- [x] MemoryTier (Pinned/Cached/Cold)
- [x] WAL для durability

## ✅ Фаза 3: Протоколы — РЕАЛИЗОВАНО
- [x] GQL → TQL nom-based bridge
- [x] Bolt Wire Protocol (Neo4j-совместимый)
- [x] gRPC service stub
- [x] GraphQL → TQL bridge
- [x] GraphQL EdgeStorage через HybridPersistentStore
- [x] GraphQL GraphStorage через HybridPersistentStore

## ✅ Фаза 4: Инфраструктура — РЕАЛИЗОВАНО
- [x] redb storage backend (default)
- [x] CI/CD (GitHub Actions)
- [x] Бенчмарки (feature_bench)
- [x] WAL integration

## 📝 Фаза 5: События (v2.3) — ЧАСТИЧНО
- [x] Subscriptions (SUBSCRIBE ... EMIT CHANGES) — AST готов
- [x] TriggerManager — AST готов
- [x] Event Bus — базовая архитектура
- [ ] Полноценный WebSocket runtime для subscriptions
- [ ] Полноценный gRPC stream для subscriptions

## 🧪 Фаза 6: Исследования (research/)
- [ ] FFT-корреляция как look-aside cache
- [ ] Федеративное обучение контекстов
- [ ] HNSW с адаптивной топологией
- [ ] TurboQuant / Product Quantization (полная реализация)
- [ ] Segment storage (appendable + non-appendable)

## 🏗️ Что не реализовано из дорожных карт

### Из TQL V3.0 vision
- [ ] PIPELINE (конвейеры обработки данных)
- [ ] Time-travel запросы (`MATCH (n:N@2024-01-01)`)
- [ ] Политики хранения (`MOVE TO COLD_STORAGE`)
- [ ] Multi-tenant SPACE изоляция

### Из GrafeoDB паттернов
- [ ] Полноценный GQL-парсер (ISO/IEC 39075) — сейчас nom-based bridge
- [ ] Полноценный Bolt packstream — сейчас stub
- [ ] HNSW quantization (SIMD) — сейчас скалярный fallback

### Из Qdrant паттернов
- [ ] Segment storage (appendable/non-appendable) — сейчас единый backend
- [ ] Auto-optimizer (merge, build, quantization) — сейчас ручной
- [ ] Multi-tenant tiered isolation

### Инфраструктура
- [ ] Docker образ
- [ ] Helm chart для Kubernetes
- [ ] Prometheus exporter (есть базовый, нужен полный)
- [ ] OpenTelemetry tracing