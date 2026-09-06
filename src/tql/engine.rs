use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::{
    ast::{DdlStatement, Query},
    cache::{CacheBackend, HashMemory},
    context::ContextModulator,
    ddl_parser::parse_ddl_statement,
    executor::{QueryExecutor, QueryResult},
    planner::{ExplainResult, PlanBuilder},
    predictor::{AccessEvent, AccessPredictor},
    schema::{SchemaError, SchemaRegistry},
};
use std::sync::{Arc, Mutex};

/// Unified TQL engine with cache, context modulation, and access prediction.
pub struct TqlEngine {
    schema_registry: Arc<SchemaRegistry>,
    cache: Mutex<HashMemory>,
    context_modulator: ContextModulator,
    predictor: AccessPredictor,
    use_cache: bool,
    store: Option<Arc<HybridPersistentStore>>,
}

impl TqlEngine {
    pub fn new() -> Self {
        Self {
            schema_registry: Arc::new(SchemaRegistry::new()),
            cache: Mutex::new(HashMemory::new(384)),
            context_modulator: ContextModulator::default(),
            predictor: AccessPredictor::default(),
            use_cache: true,
            store: None,
        }
    }

    pub fn with_store(store: Arc<HybridPersistentStore>) -> Self {
        Self {
            schema_registry: Arc::new(SchemaRegistry::new()),
            cache: Mutex::new(HashMemory::new(384)),
            context_modulator: ContextModulator::default(),
            predictor: AccessPredictor::default(),
            use_cache: true,
            store: Some(store),
        }
    }

    pub fn with_registry(registry: Arc<SchemaRegistry>) -> Self {
        Self {
            schema_registry: registry,
            cache: Mutex::new(HashMemory::new(384)),
            context_modulator: ContextModulator::default(),
            predictor: AccessPredictor::default(),
            use_cache: true,
            store: None,
        }
    }

    /// Set a custom cache backend.
    pub fn with_cache(mut self, cache: HashMemory) -> Self {
        self.cache = Mutex::new(cache);
        self
    }

    /// Enable or disable cache.
    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.use_cache = enabled;
    }

    /// Access the context modulator.
    pub fn context_modulator(&self) -> &ContextModulator {
        &self.context_modulator
    }

    /// Access the access predictor.
    pub fn predictor(&self) -> &AccessPredictor {
        &self.predictor
    }

    /// Execute any TQL statement (DDL, Query, or EXPLAIN)
    pub async fn execute(&self, sql: &str) -> Result<TqlResult, TqlError> {
        if let Ok((_, ddl)) = parse_ddl_statement(sql) {
            return self.execute_ddl(ddl).await;
        }

        if sql.trim().to_uppercase().starts_with("EXPLAIN") {
            let query_sql = sql.trim()[7..].trim();
            if let Ok((_, query)) = crate::tql::parser::parse_query(query_sql) {
                return self.explain_query(&query).await;
            }
            return Err(TqlError::ParseError(
                "Failed to parse query for EXPLAIN".to_string(),
            ));
        }

        match crate::tql::parser::parse_query(sql) {
            Ok((_, query)) => {
                if let Some(ref match_clause) = query.match_clause {
                    let label = &match_clause.source.label;
                    if self.schema_registry.get_node_type(label)?.is_none() {
                        return Err(TqlError::ValidationError(format!(
                            "Unknown node type: {label}"
                        )));
                    }
                    if let Some(ref where_clause) = query.where_clause {
                        self.validate_where_clause(label, where_clause)?;
                    }
                }
                // If store is available, execute immediately
                if let Some(ref store) = self.store {
                    let results = self.execute_query(store, query, &[]).await?;
                    return Ok(results);
                }
                Ok(TqlResult::QueryReady(query))
            }
            Err(e) => Err(TqlError::ParseError(format!("{e:?}"))),
        }
    }

    /// Execute a parsed query with a given store.
    /// Uses cache, context modulation, and records access for prediction.
    pub async fn execute_query(
        &self,
        store: &HybridPersistentStore,
        query: Query,
        context: &[crate::tql::context::ContextKey],
    ) -> Result<TqlResult, TqlError> {
        // Validate
        if let Some(ref match_clause) = &query.match_clause {
            let label = &match_clause.source.label;
            if self.schema_registry.get_node_type(label)?.is_none() {
                return Err(TqlError::ValidationError(format!(
                    "Unknown node type: {label}"
                )));
            }
            if let Some(ref where_clause) = &query.where_clause {
                self.validate_where_clause(label, where_clause)?;
            }
        }

        // Try cache first
        let cache_key = query_to_cache_key(&query);
        if self.use_cache {
            if let Ok(Some(cached)) = self.cache.lock().unwrap().recall(&cache_key) {
                let results: Vec<QueryResult> = bincode::deserialize(&cached)
                    .map_err(|e| TqlError::ExecutionError(e.to_string()))?;
                // Record access for prediction
                for r in &results {
                    self.predictor.record(&AccessEvent {
                        node_id: r.id,
                        timestamp: chrono::Utc::now().timestamp(),
                        hit: true,
                    });
                }
                return Ok(TqlResult::Query(results));
            }
        }

        // Execute
        let results = QueryExecutor::execute_query(store, query)
            .await
            .map_err(TqlError::ExecutionError)?;

        // Apply context modulation to distances
        let modulated: Vec<QueryResult> = if context.is_empty() {
            results
        } else {
            results
                .into_iter()
                .map(|mut r| {
                    let base_dist = r.score;
                    r.score = self.context_modulator.modulate(base_dist, context);
                    r
                })
                .collect()
        };

        // Cache results
        if self.use_cache && !modulated.is_empty() {
            if let Ok(serialized) = bincode::serialize(&modulated) {
                let _ = self.cache.lock().unwrap().store(&cache_key, serialized);
            }
        }

        // Record access for prediction
        for r in &modulated {
            self.predictor.record(&AccessEvent {
                node_id: r.id,
                timestamp: chrono::Utc::now().timestamp(),
                hit: true,
            });
        }

        Ok(TqlResult::Query(modulated))
    }

    /// Get hot nodes from predictor for prefetching.
    pub fn get_hot_nodes(&self) -> Vec<crate::tql::predictor::AccessPrediction> {
        self.predictor.predict_hot()
    }

    /// Execute DDL statement
    async fn execute_ddl(&self, ddl: DdlStatement) -> Result<TqlResult, TqlError> {
        match ddl {
            DdlStatement::CreateNodeType(def) => {
                self.schema_registry.create_node_type(def)?;
                Ok(TqlResult::DdlSuccess(
                    "Node type created successfully".to_string(),
                ))
            }
            DdlStatement::CreateEdgeType(def) => {
                self.schema_registry.create_edge_type(def)?;
                Ok(TqlResult::DdlSuccess(
                    "Edge type created successfully".to_string(),
                ))
            }
            DdlStatement::DropNodeType(name) => {
                self.schema_registry.drop_node_type(&name)?;
                Ok(TqlResult::DdlSuccess(format!("Node type '{name}' dropped")))
            }
            DdlStatement::DropEdgeType(name) => {
                self.schema_registry.drop_edge_type(&name)?;
                Ok(TqlResult::DdlSuccess(format!("Edge type '{name}' dropped")))
            }
            DdlStatement::ShowSchema => {
                let node_types = self.schema_registry.list_node_types()?;
                let edge_types = self.schema_registry.list_edge_types()?;
                let mut output = String::from("SCHEMA:\n\nNode Types:\n");
                for node_type in &node_types {
                    output.push_str(&format!("  - {node_type}\n"));
                }
                output.push_str("\nEdge Types:\n");
                for edge_type in &edge_types {
                    output.push_str(&format!("  - {edge_type}\n"));
                }
                Ok(TqlResult::DdlSuccess(output))
            }
        }
    }

    /// Generate EXPLAIN plan for a query
    async fn explain_query(&self, query: &Query) -> Result<TqlResult, TqlError> {
        let plan = PlanBuilder::build(query);
        let explain = ExplainResult {
            plan,
            pretty_plan: String::new(),
        };
        let formatted = explain.format_plan();
        Ok(TqlResult::Explain(formatted))
    }

    fn validate_where_clause(
        &self,
        node_type: &str,
        where_clause: &crate::tql::ast::WhereCondition,
    ) -> Result<(), TqlError> {
        match where_clause {
            crate::tql::ast::WhereCondition::ToroidalDistance { field, .. } => {
                let data_type = self.schema_registry.validate_field(node_type, field)?;
                match data_type {
                    crate::tql::ast::DataType::Vector(_) => Ok(()),
                    _ => Err(TqlError::ValidationError(format!(
                        "Field '{field}' is not a vector type"
                    ))),
                }
            }
            crate::tql::ast::WhereCondition::SimilarTo { field, .. } => {
                self.schema_registry.validate_field(node_type, field)?;
                Ok(())
            }
            crate::tql::ast::WhereCondition::PropertyFilter { property, .. } => {
                self.schema_registry.validate_field(node_type, property)?;
                Ok(())
            }
        }
    }

    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }
}

impl Default for TqlEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a simple cache key from query structure.
fn query_to_cache_key(query: &Query) -> Vec<f32> {
    let mut key = Vec::new();
    if let Some(ref mc) = query.match_clause {
        for b in mc.source.label.bytes() {
            key.push(f32::from(b) / 255.0);
        }
    }
    if let Some(ref wc) = query.where_clause {
        if let crate::tql::ast::WhereCondition::ToroidalDistance { threshold, .. } = wc {
            key.push(*threshold);
        }
    }
    key.push(query.limit as f32);
    // Pad to at least 4 elements
    while key.len() < 4 {
        key.push(0.0);
    }
    key
}

/// Result of TQL execution
#[derive(Debug, Clone)]
pub enum TqlResult {
    Query(Vec<QueryResult>),
    QueryReady(Query),
    Explain(String),
    DdlSuccess(String),
}

/// TQL execution errors
#[derive(Debug, Clone)]
pub enum TqlError {
    ParseError(String),
    ValidationError(String),
    ExecutionError(String),
    SchemaError(SchemaError),
}

impl std::fmt::Display for TqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TqlError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            TqlError::ValidationError(msg) => write!(f, "Validation error: {msg}"),
            TqlError::ExecutionError(msg) => write!(f, "Execution error: {msg}"),
            TqlError::SchemaError(e) => write!(f, "Schema error: {e}"),
        }
    }
}

impl std::error::Error for TqlError {}

impl From<SchemaError> for TqlError {
    fn from(err: SchemaError) -> Self {
        TqlError::SchemaError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tql::ast::{DataType, FieldConstraint, FieldDef, NodeTypeDef};
    use crate::tql::context::ContextKey;

    fn create_test_engine() -> TqlEngine {
        TqlEngine::new()
    }

    #[tokio::test]
    async fn test_ddl_create_node_type() {
        let engine = create_test_engine();
        let sql = r#"CREATE NODE TYPE Document (
            id INT PRIMARY KEY,
            title TEXT NOT NULL,
            content_t3 VECTOR(1536) VECTOR_INDEX(phi = 5.71)
        )"#;
        let result = engine.execute(sql).await;
        assert!(result.is_ok());
        match result.unwrap() {
            TqlResult::DdlSuccess(msg) => assert!(msg.contains("created successfully")),
            _ => panic!("Expected DDL success"),
        }
        let doc_type = engine.schema_registry().get_node_type("Document").unwrap();
        assert!(doc_type.is_some());
        assert_eq!(doc_type.unwrap().fields.len(), 3);
    }

    #[tokio::test]
    async fn test_explain_query() {
        let engine = create_test_engine();
        let ddl = r#"CREATE NODE TYPE Document (id INT PRIMARY KEY, content_t3 VECTOR(1536))"#;
        engine.execute(ddl).await.unwrap();
        let explain_sql = r#"EXPLAIN MATCH (d:Document) 
            WHERE TOROIDALDISTANCE(content_t3, 0.3) 
            RETURN d.id 
            LIMIT 10"#;
        let result = engine.execute(explain_sql).await;
        assert!(result.is_ok());
        match result.unwrap() {
            TqlResult::Explain(plan) => {
                assert!(plan.contains("QUERY PLAN:"));
                assert!(plan.contains("ROUTE_SHARDS"));
            }
            _ => panic!("Expected EXPLAIN result"),
        }
    }

    #[tokio::test]
    async fn test_validation_unknown_type() {
        let engine = create_test_engine();
        let sql = r#"MATCH (d:UnknownType) RETURN d.id LIMIT 10"#;
        let result = engine.execute(sql).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TqlError::ValidationError(msg) => assert!(msg.contains("Unknown node type")),
            _ => panic!("Expected validation error"),
        }
    }

    #[test]
    fn test_context_modulator_integration() {
        let engine = create_test_engine();
        engine
            .context_modulator()
            .set_weight(ContextKey::user_role("admin"), 0.3);
        assert!(engine.context_modulator().len() > 0);
    }

    #[test]
    fn test_predictor_integration() {
        let engine = create_test_engine();
        engine.predictor().record(&AccessEvent {
            node_id: 1,
            timestamp: 0,
            hit: true,
        });
        assert!(engine.predictor().len() > 0);
    }

    #[test]
    fn test_cache_config() {
        let mut engine = create_test_engine();
        assert!(engine.use_cache);
        engine.set_cache_enabled(false);
        // use_cache is private, but we can test that execution still works
    }
}
