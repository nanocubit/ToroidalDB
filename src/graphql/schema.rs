use async_graphql::{Context, EmptySubscription, InputObject, Object, Schema, SimpleObject, ID};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(SimpleObject, Clone, Serialize, Deserialize)]
pub struct GqlNode {
    pub id: ID,
    pub label: String,
    pub vector: Vec<f32>,
    pub properties: serde_json::Value,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(SimpleObject, Clone)]
pub struct GqlEdge {
    pub id: ID,
    pub from_node_id: ID,
    pub to_node_id: ID,
    pub relation_type: String,
    pub weight: f32,
    pub properties: serde_json::Value,
}

#[derive(InputObject)]
pub struct CreateNodeInput {
    pub label: String,
    pub vector: Vec<f32>,
    pub properties: Option<serde_json::Value>,
}

#[derive(InputObject)]
pub struct UpdateNodeInput {
    pub label: Option<String>,
    pub vector: Option<Vec<f32>>,
    pub properties: Option<serde_json::Value>,
}

#[derive(InputObject)]
pub struct CreateEdgeInput {
    pub from_node_id: ID,
    pub to_node_id: ID,
    pub relation_type: String,
    pub weight: Option<f32>,
    pub properties: Option<serde_json::Value>,
}

#[derive(InputObject)]
pub struct NodeFilterInput {
    pub label: Option<String>,
    pub vector_similarity: Option<VectorSimilarityInput>,
}

#[derive(InputObject)]
pub struct VectorSimilarityInput {
    pub query_vector: Vec<f32>,
    pub threshold: Option<f32>,
    pub limit: Option<usize>,
}

#[derive(SimpleObject, Clone)]
pub struct QueryResult {
    pub nodes: Vec<GqlNode>,
    pub total_count: usize,
    pub took_ms: i64,
}

#[derive(SimpleObject, Clone)]
pub struct SearchResult {
    pub nodes: Vec<GqlNode>,
    pub edges: Vec<GqlEdge>,
    pub took_ms: i64,
}

#[derive(SimpleObject, Clone)]
pub struct TopologyMetrics {
    pub total_nodes: i64,
    pub total_edges: i64,
    pub connected_components: i64,
    pub average_degree: f32,
    pub density: f32,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn node(&self, ctx: &Context<'_>, id: ID) -> Option<GqlNode> {
        let storage = ctx.data::<Arc<dyn NodeStorage>>().ok()?;
        storage.get_node(&id.to_string()).await
    }

    async fn nodes(
        &self,
        ctx: &Context<'_>,
        filter: Option<NodeFilterInput>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> QueryResult {
        let storage = match ctx.data::<Arc<dyn NodeStorage>>() {
            Ok(s) => s,
            Err(_) => {
                return QueryResult {
                    nodes: vec![],
                    total_count: 0,
                    took_ms: 0,
                }
            }
        };

        let start = std::time::Instant::now();
        let limit = limit.unwrap_or(10);
        let offset = offset.unwrap_or(0);

        let nodes = storage.list_nodes(filter.as_ref(), limit, offset).await;
        let total_count = storage.count_nodes().await;

        let took = start.elapsed().as_millis() as i64;

        QueryResult {
            nodes,
            total_count,
            took_ms: took,
        }
    }

    async fn search_similar(&self, ctx: &Context<'_>, input: VectorSimilarityInput) -> QueryResult {
        let storage = match ctx.data::<Arc<dyn NodeStorage>>() {
            Ok(s) => s,
            Err(_) => {
                return QueryResult {
                    nodes: vec![],
                    total_count: 0,
                    took_ms: 0,
                }
            }
        };

        let start = std::time::Instant::now();
        let limit = input.limit.unwrap_or(10);
        let threshold = input.threshold.unwrap_or(0.0);

        let nodes = storage
            .search_similar(&input.query_vector, threshold, limit)
            .await;
        let took = start.elapsed().as_millis() as i64;

        QueryResult {
            nodes,
            total_count: nodes.len(),
            took_ms: took,
        }
    }

    async fn edge(&self, ctx: &Context<'_>, id: ID) -> Option<GqlEdge> {
        let storage = ctx.data::<Arc<dyn EdgeStorage>>().ok()?;
        storage.get_edge(&id.to_string()).await
    }

    async fn edges(
        &self,
        ctx: &Context<'_>,
        from_node_id: Option<ID>,
        to_node_id: Option<ID>,
        limit: Option<usize>,
    ) -> Vec<GqlEdge> {
        let storage = match ctx.data::<Arc<dyn EdgeStorage>>() {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let limit = limit.unwrap_or(50);
        storage
            .list_edges(
                from_node_id.as_ref().map(|i| i.to_string()).as_ref(),
                to_node_id.as_ref().map(|i| i.to_string()).as_ref(),
                limit,
            )
            .await
    }

    async fn topology_metrics(&self, ctx: &Context<'_>) -> TopologyMetrics {
        let storage = match ctx.data::<Arc<dyn GraphStorage>>() {
            Ok(s) => s,
            Err(_) => {
                return TopologyMetrics {
                    total_nodes: 0,
                    total_edges: 0,
                    connected_components: 0,
                    average_degree: 0.0,
                    density: 0.0,
                }
            }
        };

        storage.get_topology_metrics().await
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn create_node(&self, ctx: &Context<'_>, input: CreateNodeInput) -> Option<GqlNode> {
        let storage = ctx.data::<Arc<dyn NodeStorage>>().ok()?;
        storage
            .create_node(input.label, input.vector, input.properties)
            .await
    }

    async fn update_node(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: UpdateNodeInput,
    ) -> Option<GqlNode> {
        let storage = ctx.data::<Arc<dyn NodeStorage>>().ok()?;
        storage
            .update_node(&id.to_string(), input.label, input.vector, input.properties)
            .await
    }

    async fn delete_node(&self, ctx: &Context<'_>, id: ID) -> bool {
        let storage = ctx.data::<Arc<dyn NodeStorage>>().ok()?;
        storage.delete_node(&id.to_string()).await
    }

    async fn create_edge(&self, ctx: &Context<'_>, input: CreateEdgeInput) -> Option<GqlEdge> {
        let storage = ctx.data::<Arc<dyn EdgeStorage>>().ok()?;
        storage
            .create_edge(
                input.from_node_id.to_string(),
                input.to_node_id.to_string(),
                input.relation_type,
                input.weight.unwrap_or(1.0),
                input.properties,
            )
            .await
    }

    async fn delete_edge(&self, ctx: &Context<'_>, id: ID) -> bool {
        let storage = ctx.data::<Arc<dyn EdgeStorage>>().ok()?;
        storage.delete_edge(&id.to_string()).await
    }

    async fn execute_query(&self, ctx: &Context<'_>, query: String) -> String {
        let executor = ctx.data::<Arc<dyn QueryExecutor>>().ok();
        if let Some(exec) = executor {
            match exec.execute_query(&query).await {
                Ok(result) => serde_json::to_string(&result).unwrap_or_default(),
                Err(e) => format!("{{\"error\": \"{}\"}}", e),
            }
        } else {
            r#"{"error": "Query executor not available"}"#.to_string()
        }
    }
}

pub type ToroidalSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn create_schema() -> ToroidalSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription).finish()
}

#[async_trait::async_trait]
pub trait NodeStorage: Send + Sync {
    async fn get_node(&self, id: &str) -> Option<GqlNode>;
    async fn list_nodes(
        &self,
        filter: Option<&NodeFilterInput>,
        limit: usize,
        offset: usize,
    ) -> Vec<GqlNode>;
    async fn create_node(
        &self,
        label: String,
        vector: Vec<f32>,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlNode>;
    async fn update_node(
        &self,
        id: &str,
        label: Option<String>,
        vector: Option<Vec<f32>>,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlNode>;
    async fn delete_node(&self, id: &str) -> bool;
    async fn search_similar(
        &self,
        query_vector: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Vec<GqlNode>;
    async fn count_nodes(&self) -> usize;
}

#[async_trait::async_trait]
pub trait EdgeStorage: Send + Sync {
    async fn get_edge(&self, id: &str) -> Option<GqlEdge>;
    async fn list_edges(
        &self,
        from_node_id: Option<&str>,
        to_node_id: Option<&str>,
        limit: usize,
    ) -> Vec<GqlEdge>;
    async fn create_edge(
        &self,
        from_node_id: String,
        to_node_id: String,
        relation_type: String,
        weight: f32,
        properties: Option<serde_json::Value>,
    ) -> Option<GqlEdge>;
    async fn delete_edge(&self, id: &str) -> bool;
}

#[async_trait::async_trait]
pub trait GraphStorage: Send + Sync {
    async fn get_topology_metrics(&self) -> TopologyMetrics;
}

#[async_trait::async_trait]
pub trait QueryExecutor: Send + Sync {
    async fn execute_query(&self, query: &str) -> Result<serde_json::Value, String>;
}
