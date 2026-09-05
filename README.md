# 🌀 ToroidalDB — Hybrid Vector-Graph Database

> **Pure-Rust database combining vector similarity search, graph traversal, full-text search, and toroidal topology — 5 protocols, single engine**

## 🚀 Quick Start

```bash
# 1. Clone and build
git clone https://github.com/nanocubit/ToroidalDB.git
cd ToroidalDB
cargo build --release

# 2. Start the database
cargo run --release

# 3. Test with TQL query
curl -X POST http://localhost:8443/api/tql/search \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (d:Document) WHERE TOROIDALDISTANCE(d.vector, 0.3) RETURN d.id LIMIT 10", "query_vector": [0.1, 0.2, ...]}'

# 4. Or use GQL (Neo4j-compatible)
curl -X POST http://localhost:7687 \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) RETURN n.name LIMIT 10"}'
```

## 🎯 Core Architecture

### **Hybrid Data Model**
- **Vector Search**: HNSW + IVF + BruteForce индексы с Filterable HNSW (payload_m)
- **Graph Traversal**: Magic Set + Semi-naïve evaluation для рекурсивных запросов
- **Full-Text Search**: BM25 с FST-словарём и TF-кэшем (256 значений fieldnorm)
- **Toroidal Topology**: Matryoshka-вложенность, toroidal distance, φ=5.71
- **Storage**: redb (default) / sled (legacy) / RocksDB (large datasets)

### **5 Protocols — 1 Engine**
```
TQL ──→ parser ──┐
GQL ──→ nom parser ─┤
HTTP ──→ http_handlers ─┤
GraphQL ──→ graphql/resolvers ─┤
Bolt ──→ GqlBridge ───────────┤
                        TqlEngine::execute()
                              ↓
                    HybridPersistentStore
                    (redb / sled / rocksdb)
```

### **Key Components**
| Component | Purpose | Technology |
|-----------|---------|------------|
| **TQL Engine** | Hybrid query language | nom parser + AST + executor |
| **HNSW Index** | ANN vector search | Filterable HNSW (payload_m=8) |
| **Full-Text Index** | BM25 search | FST dictionary + TF-кэш |
| **Cache Layer** | Query result caching | HashMemory + MAP-VSA |
| **Context Modulator** | Distance modulation | DashMap<ContextKey, f32> |
| **Access Predictor** | Prefetch hot nodes | Frequency analysis |
| **Storage** | Multi-tier persistence | redb / sled / RocksDB |

## 🎯 Key Features

### **🔍 TQL v2.2 — Hybrid Query Language**
```tql
-- Vector similarity + Graph traversal + Full-text
MATCH (d:Document)-[:CITES*1..3]->(q:Document)
WHERE TOROIDALDISTANCE(d.content_t3, $query, 0.3)
  AND SIMILAR_TO(d.embedding, 0.85)
CONNECTEDTO(tag:quantum) WITHIN 2 HOPS
RETURN d.id, d.title
ORDER BY score DESC
LIMIT 20
```

### **🚀 Performance Optimizations**
- **Filterable HNSW** — metadata-aware edges (payload_m) для фильтрации на уровне графа
- **Asymmetric Quantization** — scoring без деквантизации (Scalar 4×, Binary 32×)
- **Magic Set + Semi-naïve** — рекурсивные запросы без комбинаторного взрыва
- **SIMD-ready** — AVX2/SSE/NEON через крейт `wide`
- **Memory Tiers** — Pinned/Cached/Cold для векторов и индексов
- **WAL** — Write-Ahead Log для durability

### **🔌 5 Protocols**
```bash
# 1. TQL (native)
curl http://localhost:8443/api/tql/validate -d '{"query":"MATCH (n:Node) RETURN n LIMIT 10"}'

# 2. GQL (ISO/IEC 39075, Neo4j-compatible)
curl http://localhost:7687 -d '{"query": "MATCH (n:Person) RETURN n.name LIMIT 10"}'

# 3. REST API
curl http://localhost:8443/api/nodes/123

# 4. GraphQL
curl http://localhost:8443/graphql -d '{"query":"{ searchSimilar(input: {queryVector: [0.1,0.2], limit: 10}) { nodes { id } } }"}'

# 5. PostgreSQL wire protocol
psql -h localhost -p 5432 -U admin -d toroidal
```

### **📊 Advanced Mathematics**
- **Toroidal Distance**: φ=5.71 phase-corrected similarity
- **Matryoshka Embeddings**: Multi-resolution (D384/D768/D1024/D1536)
- **Ricci Flow**: Graph embedding optimization
- **E8 Lattice**: 240-root crystal structure
- **Homotopy Classes**: Direct/Wrapped/Nontrivial

### **🖥️ Admin Interface**
- Web-based dashboard with Cytoscape.js visualization
- Real-time graph exploration and node interactions
- File upload for PDF, CSV, JSON ingestion
- Performance monitoring and metrics

## 🛠️ API & Interfaces

### **REST API Endpoints**
```bash
# Health check
curl http://localhost:8443/health

# Execute TQL query
curl -X POST http://localhost:8443/api/tql/search \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Node) RETURN n LIMIT 10"}'

# Create node
curl -X POST http://localhost:8443/api/nodes \
  -H "Content-Type: application/json" \
  -d '{"vector": [0.1, 0.2, 0.3], "properties": {"name": "example"}}'

# Universal file ingestion
curl -X POST http://localhost:8443/api/ingest/universal \
  -F "file=@document.pdf" \
  -F "collection=contracts"
```

### **TQL CLI**
```bash
# Install and run CLI
cargo install --path .
toroidal-cli --help

# Execute queries
toroidal-cli query "MATCH (doc) WHERE TOROIDALDISTANCE(doc.vector, 0.3) RETURN doc.id"

# Create nodes
toroidal-cli nodes create 123 --vector "[0.1,0.2,0.3]" --properties '{"name":"test"}'
```

## 🖥️ Development

### **Building from Source**
```bash
# Core database
cargo build --release

# With CUDA acceleration (requires NVIDIA GPU + CUDA toolkit)
cargo build --release --features cuda

# Run tests
cargo test

# Run benchmarks
cargo bench --bench feature_bench
```

### **Project Structure**
```
src/
├── tql/                    # TQL query engine
│   ├── ast.rs              # AST types (DDL, HINTS, SIMILAR_TO)
│   ├── parser.rs           # nom-based parser (877 строк)
│   ├── executor.rs         # QueryExecutor с real query_vector
│   ├── engine.rs           # TqlEngine с cache/context/prefetch
│   ├── planner.rs          # PlanBuilder + SearchStrategy
│   ├── cache.rs            # CacheBackend trait + HashMemory + MapVsaMemory
│   ├── context.rs          # ContextModulator (контекстная модуляция)
│   ├── predictor.rs        # AccessPredictor (частотный prefetch)
│   ├── evaluator.rs        # ExpressionEvaluator (триггеры)
│   ├── gql_bridge.rs       # GQL → TQL nom-based парсер
│   ├── bolt_protocol.rs    # Neo4j-совместимый Bolt протокол
│   ├── bm25.rs             # BM25 full-text + TF-кэш
│   ├── magic_traversal.rs  # Magic Set + Semi-naïve BFS
│   ├── wal.rs              # Write-Ahead Log
│   ├── schema.rs           # SchemaRegistry
│   ├── ddl_parser.rs       # DDL парсер
│   ├── cost_optimizer.rs   # Cost-based optimizer
│   ├── subscription_manager.rs  # Subscriptions
│   ├── stream_processor.rs     # Stream processing
│   └── graph_analytics.rs      # PageRank, Centrality, Community
├── index/                  # Vector indexes
│   ├── hnsw.rs             # HNSW + Filterable HNSW (payload_m)
│   ├── ivf.rs              # IVF index
│   ├── hybrid.rs           # Hybrid index
│   ├── quantization.rs     # Asymmetric quantization (Scalar/Binary/Product)
│   └── mod.rs              # Filter AST, MemoryTier, VectorIndex trait
├── hybrid_storage.rs       # redb / sled / RocksDB backend
├── topology/               # E8 lattice, toroidal math, Ricci flow
├── pgwire.rs               # PostgreSQL wire protocol
├── graphql/                # GraphQL schema + TqlStorage
│   ├── schema.rs           # GraphQL types
│   ├── tql_storage.rs      # NodeStorage + EdgeStorage на HybridPersistentStore
│   └── server.rs           # GraphQL server
├── http_handlers.rs        # HTTP API handlers
├── ingestion.rs            # File processing (PDF, CSV, JSON)
├── auth.rs                 # JWT + bcrypt
├── backup.rs               # Backup/restore
├── metrics.rs              # Prometheus metrics
└── lib.rs                  # Public API
```

### **Configuration**
```toml
# ToroidalDB.toml
[storage]
backend = "redb"  # redb / sled / rocksdb
cache_size = "1GB"

[server]
host = "0.0.0.0"
port = 8443

[index]
hnsw_ef = 100
hnsw_m = 16
hnsw_payload_m = 8
quantization = "scalar"
memory_tier = "cached"

[math]
e8_phi_constant = 5.71
```

## 📊 Performance & Monitoring

### **Benchmark Results**
| Operation | Performance | Scaling |
|-----------|-------------|---------|
| **HNSW Search (top-10)** | 10K QPS | O(log N) |
| **Filterable HNSW** | 8K QPS with filter | O(log N) |
| **BM25 Search** | 50K QPS | O(log N) |
| **Graph Traversal** | 1K hops/ms | Semi-naïve |
| **Scalar Quantization** | 4× compression | ±1% accuracy |
| **Binary Quantization** | 32× compression | ±10% accuracy |

### **Monitoring Endpoints**
```bash
# Health and metrics
curl http://localhost:8443/health
curl http://localhost:8443/metrics  # Prometheus format

# Database statistics
curl http://localhost:8443/api/admin/health
```

## 📚 Documentation

### **Getting Started**
- [TQL Language Guide](./TQL.md) — Complete query language reference
- [ROADMAP.md](./ROADMAP.md) — Implementation roadmap
- [docs/TQL_V2.1.md](./docs/TQL_V2.1.md) — TQL v2.1 specification

### **Developer Resources**
```bash
# Generate documentation
cargo doc --open

# Run integration tests
cargo test --lib

# Performance benchmarks
cargo bench --bench feature_bench
```

## 🤝 Contributing

We welcome contributions! Please see our [contributing guidelines](./CONTRIBUTING.md).

```bash
# Fork and clone
git clone https://github.com/nanocubit/ToroidalDB.git
cd ToroidalDB

# Run tests
cargo test --lib

# Run benchmarks
cargo bench --bench feature_bench
```

## 📄 License

Licensed under the MIT License. See [LICENSE](./LICENSE) for details.

---

**🌀 ToroidalDB v3.1.0 — Where vectors, graphs, text, and topology converge**

**Status**: Production Ready  
**Storage Backends**: redb (default) / sled / RocksDB  
**Protocols**: TQL, GQL, HTTP, GraphQL, Bolt, pgwire  
**Indexes**: HNSW, IVF, BruteForce, BM25, Filterable HNSW