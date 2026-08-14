use crate::state::ServerState;
use crate::hybrid_storage::{HybridStorageQueryExecutor, HybridSearchResult};
use crate::tql::{Query, AggregationFunction};

// Типы для HTTP ответов
pub type JsonResponse = Result<axum::response::Json<Value>>;

// Обработчик запросов
pub struct SearchRequest {
    pub query: String,
    pub threshold: f32,
    pub dimension: Option<String>,
    pub limit: Option<usize>,
}

pub struct HybridSearchRequest {
    pub query: String,
    pub dimension: String,
    pub threshold: f32,
}

impl Clone for SearchRequest {}

// RESTful эндпоинты
pub mod search;
pub mod nodes;
pub mod admin;
pub mod health;
pub mod ingest;

pub use super::tql::{Query, AggregationFunction};