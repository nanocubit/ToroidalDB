// ToroidalDB Library - Core types and modules re-exported for external usage
pub mod admin_ui;
pub mod audit;
pub mod auth;
pub mod backup;
pub mod cli;
pub mod core;
pub mod embedding;
pub mod graphql;
pub mod http_handlers;
pub mod hybrid_storage;
pub mod index;
pub mod ingestion;
pub mod math;
pub mod mcp;
pub mod metrics;
pub mod middleware;
pub mod onnx_embedding;
pub mod pgwire;
pub mod query;
pub mod replication;
pub mod storage;
pub mod topology;
pub mod tql;
pub mod visualization;

pub use core::{Cache, EdgePool, LruKCache, NodePool, ObjectPool, PoolConfig};
pub use embedding::{convert_to_matryoshka, EmbeddingModel, EmbeddingService};
pub use hybrid_storage::{HybridPersistentStore, Node};
pub use index::{
    create_index, HnswIndex, HybridIndex, HybridIndexConfig, HybridSearchResult, IndexConfig,
    IndexRecommendation, IndexType, IvfIndex, MetricType, QueryCharacteristics, SearchFilters,
    VectorIndex,
};
#[cfg(feature = "embeddings")]
pub use onnx_embedding::{OnnxEmbeddingConfig, OnnxEmbeddingService};
pub use tql::{AggregationFunction, Query};
