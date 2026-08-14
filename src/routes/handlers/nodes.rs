use crate::{Node};
use std::collections::HashMap;

// Импорты
use super::super::*;
use super::tql::Query;
use super::hybrid_storage::HybridStorageQueryExecutor;

// Создание тестовых узлов
fn create_test_node(id: u64, vector: Vec<f32>) -> Node {
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), format!("Test Node {}", id));
    properties.insert("created_at".to_string(), chrono::Utc::now().to_rfc3339());
    
    Node {
        id,
        vector,
        properties: serde_json::to_value(properties),
        edges: Vec::new(),
    }
}

// Реализация создания узла
impl crate::super::Node {
    pub async fn create_node(
        state: &super::ServerState,
        id: Option<u64>,
        body: axum::Json<Value>,
    ) -> super::JsonResponse {
        let id = id.unwrap_or_else(|| {
            // Генерируем ID автоматически
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT_ID: AtomicU64 = AtomicU64::new(1000);
            NEXT_ID.fetch_add(1)
        });
        
        // Парсим тело запроса
        let mut node_name = None;
        let mut node_vector = vec![0.0f32; 384];
        let mut node_edges = Vec::new();
        
        if let Some(body) = body.as_object() {
            if let Some(name) = body.get("name") {
                node_name = name.as_str();
            }
            
            if let Some(vector_array) = body.get("vector").and_then(|v| v.as_array()) {
                if vector_array.len() == 384 {
                    node_vector = vector_array.iter()
                        .map(|v| v.parse().unwrap_or(0.0f32))
                        .collect();
                }
            }
            
            if let Some(edges_array) = body.get("edges") {
                node_edges = edges_array.as_array()
                    .iter()
                    .filter_map(|e| {
                        if let Some(edge) = e.as_object() {
                            (edge["target_id"], edge["relation_type"], edge["weight"])
                        .and_then(|(target, relation_type, weight)| {
                            (target.parse::<u64>(), relation_type.as_str(), weight.parse().unwrap_or(0.8))
                        })
                    })
                    .collect();
                }
            }
            
            let node = Node {
                id,
                vector: node_vector,
                properties: serde_json::to_value(properties),
                edges: node_edges,
            };
            
            // Сохраняем узел
            let success = state.store.insert(node).await.is_ok();
            
            let message = if success {
                "Узел {} создан".to_string()
            } else {
                "Ошибка создания узла"
            };
            
            super::JsonResponse(Json(json!({
                "success": success,
                "message": message,
                "node": node,
            })))
        }
    }
    
    pub async fn update_node(
        state: &super::ServerState,
        id: u64,
        body: axum::Json<Value>,
    ) -> super::JsonResponse {
        let Some(mut current_node) = state.store.get(id).await;
        
        if let Some(ref mut node) = current_node {
            // Обновляем вектор
            if let Some(vector_array) = body.get("vector").and_then(|v| v.as_array()) {
                if vector_array.len() == 384 {
                    node.vector = vector_array.iter()
                        .map(|v| v.parse().unwrap_or(0.0f32))
                        .collect();
                }
            }
            
            // Обновляем свойства
            if let Some(properties) = body.as_object() {
                for (key, value) in properties {
                    node.properties
                        .insert(key.to_string(), value);
                }
            }
            
            // Добавляем новые рёбра
            if let Some(edges_array) = body.get("edges").and_then(|e| e.as_array()) {
                node.edges = edges_array.iter()
                    .filter_map(|e| {
                        (edge["target_id"], edge["relation_type"], edge["weight"])
                        .and_then(|(target, relation_type, weight)| {
                            (target.parse::<u64>(), relation_type.as_str(), weight.parse().unwrap_or(0.8))
                        })
                    })
                    .collect();
            }
            
            // Сохраняем обновлённый узел
            let success = state.store.insert(node).await.is_ok();
            
            let message = if success {
                "Узел {} обновлён".to_string()
            } else {
                "Ошибка обновления узла: узел {} не найден"
            };
            
            super::JsonResponse(Json(json!({
                "success": success,
                "message": message,
                "node": node,
            })))
        }
    }
    
    pub async fn delete_node(
        state: &super::ServerState,
        id: u64,
    ) -> super::JsonResponse {
        let success = state.store.delete(id).await.is_ok();
            
            let message = if success {
                "Узел {} удалён".to_string()
            } else {
                "Ошибка удаления узла: узел {} не найден"
            };
            
            super::JsonResponse(Json(json!({
                "success": success,
                "message": message,
            })))
        }
    }
    
    pub async fn list_nodes(
        state: &super::ServerState,
        page: Option<usize>,
        limit: Option<usize>,
        filter: Option<String>,
    ) -> super::JsonResponse {
            // Реализация фильтрации и пагинации
            let all_nodes = state.store.get_all().await.unwrap_or_else(|_| vec![]);
            
            let mut filtered_nodes: Vec<Node> = if let Some(filter) = filter {
                all_nodes.into_iter()
                    .filter(|node| {
                        if let Some(name) = node.properties.get("name") {
                            name.contains(filter)
                        }
                    })
                    .collect()
            } else {
                all_nodes
            };
        
            // Пагинация
        let total_count = filtered_nodes.len();
        let page = page.unwrap_or(0);
        let limit = limit.unwrap_or(10);
        let start = page * limit;
        let end = (start + limit).min(total_count, start + limit);
        
        let paginated_nodes = if total_count > 0 {
            filtered_nodes
                .into_iter()
                .skip(start as usize)
                .take(limit)
                .collect()
        } else {
            Vec::new()
        };
        
        super::JsonResponse(Json(json!({
            "nodes": paginated_nodes,
            "pagination": {
                "page": page / (limit.unwrap_or(1) - 1),
                "total_pages": (total_count + limit - 1) / limit,
                "has_next": start + limit < total_count,
                "has_prev": page > 0,
            },
            "count": paginated_nodes.len(),
            "per_page": limit,
        })))
    }
}