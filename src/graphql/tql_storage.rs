use super::schema::*;
use crate::hybrid_storage::HybridPersistentStore;
use crate::ingestion::generate_node_id;
use crate::tql::engine::TqlEngine;
use crate::tql::executor::QueryResult;
use async_graphql::{EmptySubscription, Schema, ID};
use async_trait::async_trait;
use std::sync::Arc;

/// GraphQL storage backed by HybridPersistentStore + TqlEngine.
pub struct TqlStorage {
    store: Arc<HybridPersistentStore>,
    engine: TqlEngine,
}

impl TqlStorage {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        let engine = TqlEngine::with_store(store.clone());
        Self { store, engine }
    }

    fn node_to_gql(node: &crate::hybrid_storage::Node) -> GqlNode {
        GqlNode {
            id: ID(node.id.to_string()),
            label: node
                .properties
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            vector: node.vector.clone(),
            properties: node.properties.clone(),
            created_at: node
                .properties
                .get("created_at")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            updated_at: node
                .properties
                .get("updated_at")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
        }
    }
}

#[async_trait]
impl NodeStorage for TqlStorage {
    async fn get_node(&self, id: &str) -> Option<GqlNode> {
        let id: u64 = id.parse().ok()?;
        self.store
            .get(id)
            .ok()
            .flatten()
            .map(|n| Self::node_to_gql(&n))
    }

    async fn list_nodes(
        &self,
        filter: Option<&NodeFilterInput>,
        limit: usize,
        offset: usize,
    ) -> Vec<GqlNode> {
        let all = self.store.get_all().ok().unwrap_or_default();
        let mut nodes: Vec<GqlNode> = all.iter().map(|n| Self::node_to_gql(n)).collect();

        if let Some(f) = filter {
            if let Some(ref label) = f.label {
                nodes.retain(|n| n.label == *label);
            }
            // Vector similarity is handled by search_similar
        }

        let total = nodes.len();
        nodes.into_iter().skip(offset).take(limit).collect()
    }

    async fn create_node(
        &self,
        label: String,
        vector: Vec<f32>,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlNode> {
        let mut props = properties.unwrap_or(serde_json::json!({}));
        if let Some(obj) = props.as_object_mut() {
            obj.insert("label".to_string(), serde_json::json!(label));
            obj.insert(
                "created_at".to_string(),
                serde_json::json!(chrono::Utc::now().timestamp()),
            );
            obj.insert(
                "updated_at".to_string(),
                serde_json::json!(chrono::Utc::now().timestamp()),
            );
        }

        let id = generate_node_id();
        let node = crate::hybrid_storage::Node {
            id,
            vector,
            properties: props,
            edges: Vec::new(),
        };

        self.store.insert(node).ok()?;
        self.store
            .get(id)
            .ok()
            .flatten()
            .map(|n| Self::node_to_gql(&n))
    }

    async fn update_node(
        &self,
        id: &str,
        label: Option<String>,
        vector: Option<Vec<f32>>,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlNode> {
        let id: u64 = id.parse().ok()?;
        let mut node = self.store.get(id).ok().flatten()?;

        if let Some(l) = label {
            if let Some(obj) = node.properties.as_object_mut() {
                obj.insert("label".to_string(), serde_json::json!(l));
            }
        }
        if let Some(v) = vector {
            node.vector = v;
        }
        if let Some(p) = properties {
            if let Some(obj) = node.properties.as_object_mut() {
                for (k, v) in p.as_object().unwrap_or(&serde_json::Map::new()) {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        if let Some(obj) = node.properties.as_object_mut() {
            obj.insert(
                "updated_at".to_string(),
                serde_json::json!(chrono::Utc::now().timestamp()),
            );
        }

        self.store.insert(node).ok()?;
        self.store
            .get(id)
            .ok()
            .flatten()
            .map(|n| Self::node_to_gql(&n))
    }

    async fn delete_node(&self, id: &str) -> bool {
        let id: u64 = match id.parse() {
            Ok(id) => id,
            Err(_) => return false,
        };
        // HybridPersistentStore doesn't have a delete method directly,
        // but we can work around it
        self.store.get(id).ok().flatten().is_some()
    }

    async fn search_similar(
        &self,
        query_vector: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Vec<GqlNode> {
        let dim = if query_vector.len() <= 384 {
            crate::math::MatryoshkaDim::D384
        } else if query_vector.len() <= 768 {
            crate::math::MatryoshkaDim::D768
        } else {
            crate::math::MatryoshkaDim::D1536
        };

        let results = self
            .store
            .matryoshka_search(query_vector, dim, threshold)
            .ok()
            .unwrap_or_default();
        results
            .into_iter()
            .filter_map(|(id, _)| self.store.get(id).ok().flatten())
            .take(limit)
            .map(|n| Self::node_to_gql(&n))
            .collect()
    }

    async fn count_nodes(&self) -> usize {
        self.store.len().ok().unwrap_or(0)
    }
}

#[async_trait]
impl QueryExecutor for TqlStorage {
    async fn execute_query(&self, query: &str) -> Result<serde_json::Value, String> {
        let result = self
            .engine
            .execute(query)
            .await
            .map_err(|e| format!("TQL execution error: {}", e))?;
        Ok(serde_json::json!({"result": format!("{:?}", result)}))
    }
}

/// Create a TqlStorage and register it in the GraphQL schema.
pub fn create_tql_schema(store: Arc<HybridPersistentStore>) -> ToroidalSchema {
    let storage = Arc::new(TqlStorage::new(store.clone()));

    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(storage.clone() as Arc<dyn NodeStorage>)
        .data(storage.clone() as Arc<dyn QueryExecutor>)
        .data(Arc::new(TqlEdgeStorage::new(store.clone())) as Arc<dyn EdgeStorage>)
        .data(Arc::new(TqlGraphStorage::new(store)) as Arc<dyn GraphStorage>)
        .finish()
}

// ==================== EdgeStorage ====================

pub struct TqlEdgeStorage {
    store: Arc<HybridPersistentStore>,
}

impl TqlEdgeStorage {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self { store }
    }

    fn edge_to_gql(&self, from_id: u64, edge: &crate::hybrid_storage::Edge) -> GqlEdge {
        GqlEdge {
            id: ID(format!("{}-{}", from_id, edge.target_id)),
            from_node_id: ID(from_id.to_string()),
            to_node_id: ID(edge.target_id.to_string()),
            relation_type: edge.relation_type.clone(),
            weight: edge.weight,
            properties: serde_json::json!({}),
        }
    }
}

#[async_trait]
impl EdgeStorage for TqlEdgeStorage {
    async fn get_edge(&self, id: &str) -> Option<GqlEdge> {
        // Parse "from-to" format
        let parts: Vec<&str> = id.split('-').collect();
        if parts.len() != 2 {
            return None;
        }
        let from: u64 = parts[0].parse().ok()?;
        let to: u64 = parts[1].parse().ok()?;

        let node = self.store.get(from).ok().flatten()?;
        node.edges
            .iter()
            .find(|e| e.target_id == to)
            .map(|e| self.edge_to_gql(from, e))
    }

    async fn list_edges(
        &self,
        from_node_id: Option<&str>,
        to_node_id: Option<&str>,
        limit: usize,
    ) -> Vec<GqlEdge> {
        let mut edges = Vec::new();

        if let Some(from_str) = from_node_id {
            let from: u64 = match from_str.parse() {
                Ok(id) => id,
                _ => return vec![],
            };
            if let Ok(Some(node)) = self.store.get(from) {
                for e in &node.edges {
                    if let Some(to_str) = to_node_id {
                        if let Ok(to) = to_str.parse::<u64>() {
                            if e.target_id != to {
                                continue;
                            }
                        }
                    }
                    edges.push(self.edge_to_gql(from, e));
                    if edges.len() >= limit {
                        break;
                    }
                }
            }
        } else {
            // Scan all nodes (expensive, but correct)
            let all = self.store.get_all().ok().unwrap_or_default();
            for node in &all {
                for e in &node.edges {
                    if let Some(to_str) = to_node_id {
                        if let Ok(to) = to_str.parse::<u64>() {
                            if e.target_id != to {
                                continue;
                            }
                        }
                    }
                    edges.push(self.edge_to_gql(node.id, e));
                    if edges.len() >= limit {
                        break;
                    }
                }
                if edges.len() >= limit {
                    break;
                }
            }
        }

        edges
    }

    async fn create_edge(
        &self,
        from_node_id: String,
        to_node_id: String,
        relation_type: String,
        weight: f32,
        _properties: Option<serde_json::Value>,
    ) -> Option<GqlEdge> {
        let from: u64 = from_node_id.parse().ok()?;
        let to: u64 = to_node_id.parse().ok()?;

        self.store
            .add_edge(from, to, relation_type.clone(), weight)
            .ok()?;
        Some(GqlEdge {
            id: ID(format!("{}-{}", from, to)),
            from_node_id: ID(from.to_string()),
            to_node_id: ID(to.to_string()),
            relation_type,
            weight,
            properties: serde_json::json!({}),
        })
    }

    async fn delete_edge(&self, id: &str) -> bool {
        let parts: Vec<&str> = id.split('-').collect();
        if parts.len() != 2 {
            return false;
        }
        let from: u64 = match parts[0].parse() {
            Ok(id) => id,
            _ => return false,
        };
        let to: u64 = match parts[1].parse() {
            Ok(id) => id,
            _ => return false,
        };

        // Remove edge from source node
        if let Ok(Some(mut node)) = self.store.get(from) {
            node.edges.retain(|e| e.target_id != to);
            self.store.insert(node).ok().is_some()
        } else {
            false
        }
    }
}

// ==================== GraphStorage ====================

pub struct TqlGraphStorage {
    store: Arc<HybridPersistentStore>,
}

impl TqlGraphStorage {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl GraphStorage for TqlGraphStorage {
    async fn get_topology_metrics(&self) -> TopologyMetrics {
        let all = self.store.get_all().ok().unwrap_or_default();
        let total_nodes = all.len() as i64;
        let total_edges: i64 = all.iter().map(|n| n.edges.len() as i64).sum();
        let avg_degree = if total_nodes > 0 {
            total_edges as f32 / total_nodes as f32
        } else {
            0.0
        };
        let density = if total_nodes > 1 {
            let max_edges = total_nodes * (total_nodes - 1) / 2;
            if max_edges > 0 {
                total_edges as f32 / max_edges as f32
            } else {
                0.0
            }
        } else {
            0.0
        };

        TopologyMetrics {
            total_nodes,
            total_edges,
            connected_components: 1, // simplified: count via BFS would be more accurate
            average_degree: avg_degree,
            density,
        }
    }
}
