pub mod schema;
pub mod server;

pub use schema::*;
pub use server::*;

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MockNodeStorage {
    nodes: RwLock<Vec<GqlNode>>,
}

impl MockNodeStorage {
    pub fn new() -> Self {
        MockNodeStorage {
            nodes: RwLock::new(Vec::new()),
        }
    }

    pub fn into_arc(self) -> Arc<dyn NodeStorage> {
        Arc::new(self)
    }
}

#[async_trait]
impl NodeStorage for MockNodeStorage {
    async fn get_node(&self, id: &str) -> Option<GqlNode> {
        let nodes = self.nodes.read().await;
        nodes.iter().find(|n| n.id.to_string() == id).cloned()
    }

    async fn list_nodes(
        &self,
        _filter: Option<&NodeFilterInput>,
        limit: usize,
        offset: usize,
    ) -> Vec<GqlNode> {
        let nodes = self.nodes.read().await;
        nodes.iter().skip(offset).take(limit).cloned().collect()
    }

    async fn create_node(
        &self,
        label: String,
        vector: Vec<f32>,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlNode> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();

        let node = GqlNode {
            id: async_graphql::ID(id),
            label,
            vector,
            properties: properties.unwrap_or(serde_json::json!({})),
            created_at: now,
            updated_at: now,
        };

        let mut nodes = self.nodes.write().await;
        nodes.push(node.clone());

        Some(node)
    }

    async fn update_node(
        &self,
        id: &str,
        label: Option<String>,
        vector: Option<Vec<f32>>,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlNode> {
        let mut nodes = self.nodes.write().await;

        if let Some(node) = nodes.iter_mut().find(|n| n.id.to_string() == id) {
            if let Some(l) = label {
                node.label = l;
            }
            if let Some(v) = vector {
                node.vector = v;
            }
            if let Some(p) = properties {
                node.properties = p;
            }
            node.updated_at = chrono::Utc::now().timestamp();
            return Some(node.clone());
        }

        None
    }

    async fn delete_node(&self, id: &str) -> bool {
        let mut nodes = self.nodes.write().await;
        let len_before = nodes.len();
        nodes.retain(|n| n.id.to_string() != id);
        nodes.len() < len_before
    }

    async fn search_similar(
        &self,
        query_vector: &[f32],
        _threshold: f32,
        limit: usize,
    ) -> Vec<GqlNode> {
        let nodes = self.nodes.read().await;

        let mut scored: Vec<(GqlNode, f32)> = nodes
            .iter()
            .map(|n| {
                let similarity = cosine_similarity(query_vector, &n.vector);
                (n.clone(), similarity)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored.into_iter().take(limit).map(|(n, _)| n).collect()
    }

    async fn count_nodes(&self) -> usize {
        self.nodes.read().await.len()
    }
}

pub struct MockEdgeStorage {
    edges: RwLock<Vec<GqlEdge>>,
}

impl MockEdgeStorage {
    pub fn new() -> Self {
        MockEdgeStorage {
            edges: RwLock::new(Vec::new()),
        }
    }

    pub fn into_arc(self) -> Arc<dyn EdgeStorage> {
        Arc::new(self)
    }
}

#[async_trait]
impl EdgeStorage for MockEdgeStorage {
    async fn get_edge(&self, id: &str) -> Option<GqlEdge> {
        let edges = self.edges.read().await;
        edges.iter().find(|e| e.id.to_string() == id).cloned()
    }

    async fn list_edges(
        &self,
        from_node_id: Option<&str>,
        to_node_id: Option<&str>,
        limit: usize,
    ) -> Vec<GqlEdge> {
        let edges = self.edges.read().await;

        edges
            .iter()
            .filter(|e| {
                let from_match = from_node_id
                    .map(|f| e.from_node_id.to_string() == f)
                    .unwrap_or(true);
                let to_match = to_node_id
                    .map(|t| e.to_node_id.to_string() == t)
                    .unwrap_or(true);
                from_match && to_match
            })
            .take(limit)
            .cloned()
            .collect()
    }

    async fn create_edge(
        &self,
        from_node_id: String,
        to_node_id: String,
        relation_type: String,
        weight: f32,
        _properties: Option<serde_json::Value>,
    ) -> Option<GqlEdge> {
        let id = uuid::Uuid::new_v4().to_string();

        let edge = GqlEdge {
            id: async_graphql::ID(id),
            from_node_id: async_graphql::ID(from_node_id),
            to_node_id: async_graphql::ID(to_node_id),
            relation_type,
            weight,
            properties: serde_json::json!({}),
        };

        let mut edges = self.edges.write().await;
        edges.push(edge.clone());

        Some(edge)
    }

    async fn delete_edge(&self, id: &str) -> bool {
        let mut edges = self.edges.write().await;
        let len_before = edges.len();
        edges.retain(|e| e.id.to_string() != id);
        edges.len() < len_before
    }
}

pub struct MockGraphStorage {
    metrics: RwLock<TopologyMetrics>,
}

impl MockGraphStorage {
    pub fn new() -> Self {
        MockGraphStorage {
            metrics: RwLock::new(TopologyMetrics {
                total_nodes: 0,
                total_edges: 0,
                connected_components: 0,
                average_degree: 0.0,
                density: 0.0,
            }),
        }
    }

    pub fn into_arc(self) -> Arc<dyn GraphStorage> {
        Arc::new(self)
    }

    pub fn set_metrics(&self, metrics: TopologyMetrics) {
        let mut m = self.metrics.write().await;
        *m = metrics;
    }
}

#[async_trait]
impl GraphStorage for MockGraphStorage {
    async fn get_topology_metrics(&self) -> TopologyMetrics {
        self.metrics.read().await.clone()
    }
}

fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    if v1.len() != v2.len() || v1.is_empty() {
        return 0.0;
    }

    let dot: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
    let mag1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag2: f32 = v2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag1 == 0.0 || mag2 == 0.0 {
        return 0.0;
    }

    dot / (mag1 * mag2)
}
