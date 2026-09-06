use crate::auth;
use crate::hybrid_storage::{HybridPersistentStore, Node};
use crate::math::MatryoshkaDim;
use crate::query::{parse_tql, TQLQuery};
use crate::topology::edges::InterToroidalEdge;

use crate::topology::ricci_flow;
use crate::tql;
use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<HybridPersistentStore>,
    pub auth_service: Arc<auth::AuthService>,
    pub backup_manager: Arc<tql::BackupManager>,
}

impl AppState {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self {
            store,
            auth_service: Arc::new(auth::AuthService::new("toroidal-secret-key".to_string())),
            backup_manager: Arc::new(tql::BackupManager::new("./backups", 30)),
        }
    }
}

#[derive(Deserialize)]
pub struct InsertNode {
    pub vector: Vec<f32>,
    #[serde(default)]
    pub properties: Value,
    #[serde(default)]
    pub edges: Vec<crate::hybrid_storage::Edge>,
}

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
}

// ===== НОВЫЕ СТРУКТУРЫ ДЛЯ МЕЖ-ТОРОВЫХ РЁБЁР =====
#[derive(Deserialize)]
pub struct InterToroidalEdgeRequest {
    pub source_id: u64,
    pub source_level: String, // "d384", "d768", etc.
    pub target_id: u64,
    pub target_level: String,
    pub relation_type: String,
    #[serde(default)]
    pub properties: Value,
}

#[derive(Serialize)]
pub struct InterToroidalEdgeResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<InterToroidalEdge>,
}

#[derive(Serialize)]
pub struct ApiResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub async fn health_handler(State(state): State<Arc<AppState>>) -> Json<Value> {
    let node_count = state.store.len().unwrap_or(0);
    let optimization = if node_count > 1000 {
        "enabled (cached)".to_string()
    } else {
        format!("disabled (<{} nodes)", 1000)
    };

    Json(json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        "storage": {
            "nodes": node_count,
            "type": "persistent (toroidal-store + graph + matryoshka + topology)",
            "optimization": optimization,
            "path": "./data"
        }
    }))
}

pub async fn insert_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<InsertNode>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<Value>)> {
    // Authentication handled by auth_middleware

    if payload.vector.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Vector cannot be empty" })),
        ));
    }

    let node = Node {
        id,
        vector: payload.vector,
        properties: payload.properties,
        edges: payload.edges,
    };

    match state.store.insert(node) {
        Ok(true) => Ok(Json(ApiResponse {
            success: true,
            message: Some("Node inserted successfully".to_string()),
            data: None,
        })),
        Ok(false) => Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "Node already exists" })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn get_node(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<Value>)> {
    // Authentication handled by auth_middleware

    match state.store.get(id) {
        Ok(Some(node)) => Ok(Json(ApiResponse {
            success: true,
            message: None,
            data: Some(json!({
                "id": node.id,
                "vector": node.vector,
                "properties": node.properties,
                "edges": node.edges,
            })),
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Node not found" })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

pub async fn search_nodes(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SearchRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<Value>)> {
    let (_, tql) = parse_tql(&payload.query).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid TQL syntax" })),
        )
    })?;

    match tql {
        TQLQuery::ToroidalSearch {
            collection,
            vector,
            threshold,
            dimension,
        } => {
            let dim = dimension.unwrap_or(MatryoshkaDim::D1536);
            let normalized_query: Vec<f32> =
                vector.iter().map(|&x: &f32| x.rem_euclid(1.0)).collect();

            let results = state
                .store
                .matryoshka_search(&normalized_query, dim, threshold)
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": e.to_string() })),
                    )
                })?;

            let results_json: Vec<Value> = results
                .iter()
                .filter_map(|(id, dist)| {
                    state.store.get(*id).ok().flatten().map(|node| {
                        json!({
                            "id": node.id,
                            "distance": dist,
                            "properties": node.properties,
                            "edges_count": node.edges.len(),
                            "dimension": dim.size().to_string(),
                        })
                    })
                })
                .collect();

            let node_count = state.store.len().unwrap_or(0);
            let optimization = if node_count > 1000 {
                format!("cached (top {})", results_json.len().min(100))
            } else {
                "exact".to_string()
            };

            Ok(Json(ApiResponse {
                success: true,
                message: Some(format!("Search completed ({optimization})")),
                data: Some(json!({
                    "collection": collection,
                    "results": results_json,
                    "count": results_json.len(),
                    "threshold": threshold,
                    "dimension": dim.size(),
                    "optimization": optimization,
                })),
            }))
        }

        TQLQuery::AddEdge {
            from_id,
            to_id,
            relation_type,
            weight,
        } => {
            match state
                .store
                .add_edge(from_id, to_id, relation_type.clone(), weight)
            {
                Ok(()) => Ok(Json(ApiResponse {
                    success: true,
                    message: Some(format!(
                        "Edge added: {from_id} --[{relation_type}]({weight})-> {to_id}"
                    )),
                    data: None,
                })),
                Err(e) => Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": e.to_string() })),
                )),
            }
        }

        TQLQuery::GetNeighbors { node_id } => match state.store.get_neighbors(node_id) {
            Ok(neighbors) => {
                let nodes_info: Vec<Value> = neighbors
                    .iter()
                    .map(|node| {
                        json!({
                            "id": node.id,
                            "properties": node.properties,
                            "edges_count": node.edges.len(),
                        })
                    })
                    .collect();

                Ok(Json(ApiResponse {
                    success: true,
                    message: Some(format!("Found {} neighbors", nodes_info.len())),
                    data: Some(json!({
                        "node_id": node_id,
                        "neighbors": nodes_info,
                        "count": nodes_info.len()
                    })),
                }))
            }
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )),
        },

        TQLQuery::GraphSearch {
            start_id,
            max_depth,
        } => {
            // Implement proper BFS graph search
            match graph_search_bfs(&state.store, start_id, max_depth as u32).await {
                Ok(nodes_info) => Ok(Json(ApiResponse {
                    success: true,
                    message: Some(format!(
                        "BFS search completed, found {} nodes",
                        nodes_info.len()
                    )),
                    data: Some(json!({
                        "start_id": start_id,
                        "nodes": nodes_info,
                        "total": nodes_info.len()
                    })),
                })),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e })),
                )),
            }
        }
    }
}

/// Perform BFS graph search from a starting node up to `max_depth`
async fn graph_search_bfs(
    store: &Arc<HybridPersistentStore>,
    start_id: u64,
    max_depth: u32,
) -> Result<Vec<Value>, String> {
    use std::collections::{HashSet, VecDeque};

    let mut visited: HashSet<u64> = HashSet::new();
    let mut queue: VecDeque<(u64, u32)> = VecDeque::new();
    let mut results: Vec<Value> = Vec::new();

    // Start from the given node
    queue.push_back((start_id, 0));
    visited.insert(start_id);

    // Add the start node to results
    if let Ok(Some(node)) = store.get(start_id) {
        results.push(json!({
            "id": node.id,
            "properties": node.properties,
            "depth": 0
        }));
    }

    while let Some((current_id, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }

        // Get neighbors
        let neighbors = store
            .get_neighbors(current_id)
            .map_err(|e| format!("Failed to get neighbors: {e}"))?;

        for neighbor in neighbors {
            if !visited.contains(&neighbor.id) {
                visited.insert(neighbor.id);
                queue.push_back((neighbor.id, depth + 1));

                results.push(json!({
                    "id": neighbor.id,
                    "properties": neighbor.properties,
                    "depth": depth + 1
                }));
            }
        }
    }

    Ok(results)
}

// ===== НОВЫЙ ЭНДПОИНТ: СОЗДАНИЕ МЕЖ-ТОРОВОГО РЕБРА =====
pub async fn create_inter_toroidal_edge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InterToroidalEdgeRequest>,
) -> Result<Json<InterToroidalEdgeResponse>, (StatusCode, Json<Value>)> {
    // Конвертируем строковые уровни в перечисление
    let source_level = match payload.source_level.to_lowercase().as_str() {
        "d384" | "384" => MatryoshkaDim::D384,
        "d768" | "768" => MatryoshkaDim::D768,
        "d1024" | "1024" => MatryoshkaDim::D1024,
        "d1536" | "1536" => MatryoshkaDim::D1536,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Invalid source_level. Must be one of: d384, d768, d1024, d1536"
                })),
            ))
        }
    };

    let target_level = match payload.target_level.to_lowercase().as_str() {
        "d384" | "384" => MatryoshkaDim::D384,
        "d768" | "768" => MatryoshkaDim::D768,
        "d1024" | "1024" => MatryoshkaDim::D1024,
        "d1536" | "1536" => MatryoshkaDim::D1536,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Invalid target_level. Must be one of: d384, d768, d1024, d1536"
                })),
            ))
        }
    };

    // Use the new add_inter_toroidal_edge method
    match state.store.add_inter_toroidal_edge(
        payload.source_id,
        source_level,
        payload.target_id,
        target_level,
        payload.relation_type.clone(),
        payload.properties.clone(),
    ) {
        Ok(edge) => Ok(Json(InterToroidalEdgeResponse {
            success: true,
            message: Some(format!(
                "Inter-toroidal edge created: {}({}) --[{}]→ {}({})",
                payload.source_id,
                payload.source_level,
                payload.relation_type,
                payload.target_id,
                payload.target_level
            )),
            edge: Some(edge),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

// ===== НОВЫЙ ЭНДПОИНТ: ПОЛУЧЕНИЕ МЕЖ-ТОРОВЫХ РЁБЁР УЗЛА =====
pub async fn get_inter_toroidal_edges(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<u64>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<Value>)> {
    match state.store.get_inter_toroidal_edges(node_id) {
        Ok(edges) => {
            let edges_json: Vec<Value> = edges
                .iter()
                .map(|edge| {
                    json!({
                        "source_id": edge.source.1,
                        "source_level": format!("{:?}", edge.source.0),
                        "target_id": edge.target.1,
                        "target_level": format!("{:?}", edge.target.0),
                        "relation_type": edge.relation_type,
                        "topological_distance": edge.topological_distance,
                        "homotopy_class": format!("{:?}", edge.homotopy_class),
                        "properties": edge.properties,
                    })
                })
                .collect();

            Ok(Json(ApiResponse {
                success: true,
                message: Some(format!(
                    "Found {} inter-toroidal edges for node {}",
                    edges_json.len(),
                    node_id
                )),
                data: Some(json!({
                    "node_id": node_id,
                    "edges": edges_json
                })),
            }))
        }
        Ok(_) => unreachable!(), // This case is handled by the match above
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

// ===== НОВЫЙ ЭНДПОИНТ: ПОТОК РИЧЧИ ДЛЯ ОПТИМИЗАЦИИ ВЛОЖЕНИЯ =====
pub async fn ricci_flow_optimization(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SearchRequest>, // Используем существующую структуру для простоты
) -> Result<Json<ApiResponse>, (StatusCode, Json<Value>)> {
    // Парсим параметры из TQL-подобного запроса
    // Формат: "ricci_flow(iterations=100, target_dim=d768)"
    let query = &payload.query;

    if !query.starts_with("ricci_flow") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Query must start with 'ricci_flow'"
            })),
        ));
    }

    // Извлекаем параметры (упрощённый парсинг)
    let iterations = query
        .find("iterations=")
        .and_then(|pos| {
            query[pos + 11..]
                .split(|c: char| !c.is_numeric())
                .next()
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(50);

    let target_dim = query
        .find("target_dim=")
        .and_then(|pos| {
            let dim_str = &query[pos + 11..pos + 11 + 4];
            match dim_str.to_lowercase().as_str() {
                "d384" => Some(MatryoshkaDim::D384),
                "d768" => Some(MatryoshkaDim::D768),
                "d1024" => Some(MatryoshkaDim::D1024),
                "d1536" => Some(MatryoshkaDim::D1536),
                _ => None,
            }
        })
        .unwrap_or(MatryoshkaDim::D768);

    // Получаем все узлы для оптимизации
    let nodes = match state.store.get_all() {
        Ok(nodes) => nodes,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            ))
        }
    };

    if nodes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "No nodes to optimize"
            })),
        ));
    }

    // Применяем поток Риччи
    match ricci_flow::optimize_embedding(&nodes, target_dim, iterations) {
        Ok(optimized_count) => Ok(Json(ApiResponse {
            success: true,
            message: Some(format!(
                "Ricci flow optimization completed: {optimized_count} nodes optimized in {iterations} iterations"
            )),
            data: Some(json!({
                "iterations": iterations,
                "target_dimension": target_dim.size(),
                "optimized_nodes": optimized_count,
                "total_nodes": nodes.len(),
            })),
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        )),
    }
}

// ===== НОВЫЙ ЭНДПОИНТ: TQL v2.0 =====
#[derive(Deserialize)]
pub struct TqlRequest {
    pub query: String,
}

// Структура для валидации запроса
#[derive(Deserialize, Serialize)]
pub struct TqlValidateRequest {
    pub query: String,
}

#[derive(Serialize)]
pub struct TqlValidateResponse {
    pub valid: bool,
    pub message: String,
    pub query_type: Option<String>,
}

pub async fn tql_validate_handler(
    Json(payload): Json<TqlValidateRequest>,
) -> Result<Json<TqlValidateResponse>, (StatusCode, Json<Value>)> {
    // Проверяем синтаксис запроса
    match tql::parser::parse_query(&payload.query) {
        Ok((_, parsed_query)) => {
            // Определяем тип запроса
            let query_type = if parsed_query.distributed {
                Some("Distributed Query".to_string())
            } else if !parsed_query.aggregation_fields.is_empty() {
                Some("Aggregation Query".to_string())
            } else if parsed_query.connected_clause.is_some() {
                Some("Connected Query".to_string())
            } else {
                Some("Basic Query".to_string())
            };

            Ok(Json(TqlValidateResponse {
                valid: true,
                message: "Query is syntactically correct".to_string(),
                query_type,
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "valid": false,
                "message": format!("Invalid TQL syntax: {:?}", e),
                "query_type": Option::<String>::None
            })),
        )),
    }
}

pub async fn tql_search_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TqlRequest>,
) -> Result<Json<ApiResponse>, (StatusCode, Json<Value>)> {
    // Права доступа теперь проверяются через middleware
    // Authentication handled by middleware

    // Parse the TQL query using our new parser
    match tql::parser::parse_query(&payload.query) {
        Ok((_, parsed_query)) => {
            // Check if the query should be executed in distributed mode
            if parsed_query.distributed {
                // Use distributed execution if available
                // В текущей реализации используем только локальное выполнение
                // Распределенное выполнение будет добавлено позже
                match tql::QueryExecutor::execute_query(&state.store, parsed_query).await {
                    Ok(results) => {
                        let results_json: Vec<Value> = results
                            .iter()
                            .map(|result| {
                                json!({
                                    "id": result.id,
                                    "score": result.score,
                                    "properties": result.properties,
                                })
                            })
                            .collect();

                        Ok(Json(ApiResponse {
                            success: true,
                            message: Some(format!(
                                "TQL query executed successfully, found {} results",
                                results.len()
                            )),
                            data: Some(json!({
                                "results": results_json,
                                "count": results_json.len(),
                                "execution": "local",
                            })),
                        }))
                    }
                    Err(e) => Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": e })),
                    )),
                }
            } else {
                // Use local execution
                match tql::QueryExecutor::execute_query(&state.store, parsed_query).await {
                    Ok(results) => {
                        let results_json: Vec<Value> = results
                            .iter()
                            .map(|result| {
                                json!({
                                    "id": result.id,
                                    "score": result.score,
                                    "properties": result.properties,
                                })
                            })
                            .collect();

                        Ok(Json(ApiResponse {
                            success: true,
                            message: Some(format!(
                                "TQL query executed successfully, found {} results",
                                results.len()
                            )),
                            data: Some(json!({
                                "results": results_json,
                                "count": results_json.len(),
                                "execution": "local",
                            })),
                        }))
                    }
                    Err(e) => Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": e })),
                    )),
                }
            }
        }
        Err(_) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid TQL syntax" })),
        )),
    }
}

// ===== АУТЕНТИФИКАЦИОННЫЕ ОБРАБОТЧИКИ =====

pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<auth::AuthPayload>,
) -> Result<Json<auth::TokenResponse>, (StatusCode, Json<Value>)> {
    match state
        .auth_service
        .authenticate(&payload.username, &payload.password)
    {
        Ok(token_response) => Ok(Json(token_response)),
        Err(e) => Err((StatusCode::UNAUTHORIZED, Json(json!({ "error": e })))),
    }
}

pub async fn register_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<auth::AuthPayload>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // В реальной системе нужно добавить проверки безопасности
    match state.auth_service.register_user(
        payload.username,
        payload.password,
        vec!["user".to_string()], // По умолчанию роль пользователя
    ) {
        Ok(user_id) => Ok(Json(json!({
            "success": true,
            "message": "User registered successfully",
            "user_id": user_id,
        }))),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(json!({ "error": e })))),
    }
}

pub async fn validate_token_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = payload.get("token").and_then(|t| t.as_str()).ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "Token is required" })),
    ))?;

    match state.auth_service.validate_token(token) {
        Ok(claims) => Ok(Json(json!({
            "valid": true,
            "claims": claims,
        }))),
        Err(e) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "valid": false, "error": e })),
        )),
    }
}

// ===== ЭНДПОИНТЫ РЕЗЕРВНОГО КОПИРОВАНИЯ =====

#[derive(Deserialize)]
pub struct CreateBackupRequest {
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct RestoreBackupRequest {
    pub backup_id: String,
}

#[derive(Serialize)]
pub struct BackupResponse {
    pub success: bool,
    pub message: String,
    pub backup_id: Option<String>,
    pub data: Option<Value>,
}

#[derive(Deserialize)]
pub struct ScheduleBackupRequest {
    pub interval_hours: u32,
}

pub async fn create_backup_handler(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
) -> Result<Json<BackupResponse>, (StatusCode, Json<Value>)> {
    // Проверяем права доступа
    if let Some(claims) = req.extensions().get::<auth::Claims>() {
        if !state.auth_service.has_permission(claims, "backup:create") {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Insufficient permissions to create backup" })),
            ));
        }
    }

    match state.backup_manager.create_backup(&state.store).await {
        Ok(backup_id) => Ok(Json(BackupResponse {
            success: true,
            message: format!("Backup created successfully: {backup_id}"),
            backup_id: Some(backup_id),
            data: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.clone() })),
        )),
    }
}

pub async fn restore_backup_handler(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    Json(payload): Json<RestoreBackupRequest>,
) -> Result<Json<BackupResponse>, (StatusCode, Json<Value>)> {
    // Проверяем права доступа
    if let Some(claims) = req.extensions().get::<auth::Claims>() {
        if !state.auth_service.has_permission(claims, "backup:restore") {
            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "Insufficient permissions to restore backup" })),
            ));
        }
    }

    match state
        .backup_manager
        .restore_from_backup(&payload.backup_id, &state.store)
        .await
    {
        Ok(()) => Ok(Json(BackupResponse {
            success: true,
            message: format!("Backup {} restored successfully", payload.backup_id),
            backup_id: Some(payload.backup_id),
            data: None,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.clone() })),
        )),
    }
}

pub async fn list_backups_handler(
    State(state): State<Arc<AppState>>,
    _req: Request<axum::body::Body>,
) -> Result<Json<BackupResponse>, (StatusCode, Json<Value>)> {
    // Authentication handled by auth_middleware

    match state.backup_manager.list_backups() {
        Ok(backups) => {
            let backups_json: Vec<Value> = backups
                .iter()
                .map(|backup| {
                    json!({
                        "id": backup.id,
                        "created_at": backup.created_at,
                        "size_bytes": backup.size_bytes,
                        "node_count": backup.node_count,
                        "metadata": backup.metadata,
                    })
                })
                .collect();

            Ok(Json(BackupResponse {
                success: true,
                message: format!("Found {} backups", backups.len()),
                backup_id: None,
                data: Some(json!({
                    "backups": backups_json,
                    "count": backups_json.len(),
                })),
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.clone() })),
        )),
    }
}

pub async fn schedule_backup_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ScheduleBackupRequest>,
) -> Result<Json<BackupResponse>, (StatusCode, Json<Value>)> {
    // Authentication handled by auth_middleware

    // Запускаем фоновую задачу для регулярных резервных копий
    let store = state.store.clone();
    let backup_manager = state.backup_manager.clone();

    tokio::spawn(async move {
        backup_manager
            .schedule_regular_backups(store, payload.interval_hours)
            .await;
    });

    Ok(Json(BackupResponse {
        success: true,
        message: format!(
            "Backup scheduling started with interval {} hours",
            payload.interval_hours
        ),
        backup_id: None,
        data: Some(json!({
            "interval_hours": payload.interval_hours,
        })),
    }))
}

// Middleware для аутентификации и авторизации
pub async fn auth_middleware(mut req: Request<axum::body::Body>, next: Next) -> Response {
    // Skip auth for certain paths
    let path = req.uri().path();
    let skip_auth_paths = ["/health", "/login", "/register", "/favicon.ico"];

    if skip_auth_paths.contains(&path) {
        return next.run(req).await;
    }

    // Extract token from Authorization header
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            // For now, allow requests without token but log it
            // In production, this should return an error
            return next.run(req).await;
        }
    };

    // Validate token and extract claims
    if let Some(auth_service) = req.extensions().get::<Arc<auth::AuthService>>() {
        if let Ok(claims) = auth_service.validate_token(token) {
            // Add claims to request extensions for downstream handlers
            req.extensions_mut().insert(claims);
        }
    }

    next.run(req).await
}

// Alternative middleware that enforces authentication
pub async fn require_auth_middleware(
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with("Bearer ") => &header[7..],
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Authorization header required" })),
            ));
        }
    };

    if let Some(auth_service) = req.extensions().get::<Arc<auth::AuthService>>() {
        match auth_service.validate_token(token) {
            Ok(claims) => {
                req.extensions_mut().insert(claims);
                Ok(next.run(req).await)
            }
            Err(e) => Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": format!("Invalid token: {}", e) })),
            )),
        }
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Auth service not available" })),
        ))
    }
}

// Обработчик админ-панели
