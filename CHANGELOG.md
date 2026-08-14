# Changelog

All significant changes to the ToroidalDB project will be documented in this file.

## [3.1.0] - 2026-02-12

### Added
- **TQL v2.0 Engine** - Complete hybrid query language with vector, graph, and topological search
- **E8 Lattice Mathematics** - 240-root crystal structure with toroidal distance metrics (φ=5.71)
- **Hybrid Storage Layer** - Auto-migration between sled and RocksDB at 100K nodes threshold
- **PGWire Protocol** - Full PostgreSQL compatibility for BI tool integration
- **Distributed Query Execution** - Scatter/gather across shards with consistent hashing
- **Admin Web Dashboard** - Cytoscape.js-based visualization with file upload
- **CUDA Acceleration** - Optional GPU acceleration for vector operations
- **AVX2 Optimization** - SIMD-accelerated E8 distance calculations
- **Transaction Support** - ACID-compliant operations with BEGIN/COMMIT/ROLLBACK
- **Query Caching** - LRU cache with TTL for query result acceleration
- **Backup & Recovery** - Point-in-time backup creation and restoration
- **JWT Authentication** - Secure API access with token-based auth
- **Metrics & Monitoring** - Prometheus-compatible metrics endpoint
- **Universal Ingestion** - PDF, CSV, JSON file processing with auto-embedding
- **Bidirectional Graph Traversal** - Efficient 2-hop neighbor search
- **Ricci Flow Optimization** - Graph embedding optimization using differential geometry
- **Matryoshka Embeddings** - Multi-resolution vector spaces (d384, d768, d1536)
- **CLI Interface** - Command-line tools for database management
- **Docker Support** - Containerized deployment with multi-stage builds

### Changed
- **Storage Architecture** - Migrated to hybrid model combining vector + graph + topology
- **Query Parser** - Complete rewrite to support Cypher-over-TQL syntax
- **Executor Engine** - New implementation with parallel processing and early termination
- **Math Module** - Enhanced toroidal metrics with E8 lattice corrections
- **Topology Module** - Expanded functionality for advanced topological operations
- **Performance Model** - Optimized for 12K QPS vector search with <20ms latency
- **Memory Management** - Improved allocation patterns and reduced fragmentation

### Fixed
- **Memory Leaks** - Resolved potential leaks in parallel operations
- **Data Races** - Enhanced synchronization in multi-threaded environment
- **Query Performance** - Optimized search and traversal algorithms
- **Error Handling** - Improved error propagation and logging
- **Resource Cleanup** - Fixed file descriptor and connection leaks
- **Edge Cases** - Better handling of empty queries and malformed data

## [3.0.0] - 2026-01-15

### Added
- **Toroidal Topology** - Introduction of topological concepts to database
- **Matryoshka Vector Embeddings** - Support for nested vector spaces
- **Graph Operations** - Basic functions for graph operations
- **Admin Web Interface** - For monitoring and management
- **Performance Metrics** - Collection and export of metrics

### Changed
- **Architecture** - Moved to modular structure with clear separation of concerns
- **Storage System** - Integration of sled with graph structures
- **API** - Moved to HTTPS with TLS encryption

## [2.4.0] - 2025-12-01

### Added
- **Vector Search** - Support for vector similarity search
- **Topological Metrics** - Distances considering toroidal topology
- **API Endpoints** - Basic functions for database interaction

### Changed
- **Storage System** - Moved to sled for embedded persistence

## [2.0.0] - 2025-11-01

### Added
- **Graph Data Model** - Support for nodes and edges
- **Basic Operations** - Create, read, update, and delete operations
- **Simple API** - Basic endpoints for interaction

## [1.0.0] - 2025-10-01

### Added
- **Basic Architecture** - Core system components
- **Storage** - Simple data storage implementation
- **Server** - HTTP server for API

---

## Version Summary

| Version | Release Date | Key Features |
|---------|--------------|--------------|
| 3.1.0 | 2026-02-12 | Production-ready hybrid database with TQL v2.0 |
| 3.0.0 | 2026-01-15 | Toroidal topology and Matryoshka embeddings |
| 2.4.0 | 2025-12-01 | Vector search and topological metrics |
| 2.0.0 | 2025-11-01 | Graph data model introduction |
| 1.0.0 | 2025-10-01 | Initial basic architecture |