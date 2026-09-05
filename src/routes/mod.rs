//! HTTP API routes for ToroidalDB.
//!
//! Delegates to `http_handlers` for the actual implementations.

use axum::{
    routing::{get, post, put},
    Router,
};
use std::sync::Arc;

use crate::http_handlers::AppState;

/// Create the router with all API routes.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Node CRUD
        .route("/api/nodes", post(crate::http_handlers::insert_node))
        .route("/api/nodes/{id}", get(crate::http_handlers::get_node))
        // Search
        .route("/api/search", post(crate::http_handlers::search_nodes))
        // Inter-toroidal edges
        .route("/api/graph/edges", post(crate::http_handlers::create_inter_toroidal_edge))
        .route("/api/graph/edges/{id}", get(crate::http_handlers::get_inter_toroidal_edges))
        // Admin
        .route("/api/admin/backup", post(crate::http_handlers::create_backup_handler))
        .route("/api/admin/backup/restore", post(crate::http_handlers::restore_backup_handler))
        .route("/api/admin/backups", get(crate::http_handlers::list_backups_handler))
        .route("/api/admin/health", get(crate::http_handlers::health_handler))
        .route("/api/admin/ricci-flow", post(crate::http_handlers::ricci_flow_optimization))
        // TQL
        .route("/api/tql/search", post(crate::http_handlers::tql_search_handler))
        .route("/api/tql/validate", post(crate::http_handlers::tql_validate_handler))
        // Auth
        .route("/api/auth/login", post(crate::http_handlers::login_handler))
        .route("/api/auth/register", post(crate::http_handlers::register_handler))
        .route("/api/auth/validate", post(crate::http_handlers::validate_token_handler))
        .with_state(state)
}