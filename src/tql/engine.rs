use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::{
    ast::{DdlStatement, Query},
    ddl_parser::parse_ddl_statement,
    executor::QueryExecutor,
    planner::{ExplainResult, PlanBuilder},
    schema::{SchemaError, SchemaRegistry},
};
use std::sync::Arc;

/// Unified TQL v2.1 Engine
///
/// Combines DDL operations, query execution, and EXPLAIN functionality
pub struct TqlEngine {
    schema_registry: Arc<SchemaRegistry>,
}

impl TqlEngine {
    pub fn new() -> Self {
        Self {
            schema_registry: Arc::new(SchemaRegistry::new()),
        }
    }

    pub fn with_registry(registry: Arc<SchemaRegistry>) -> Self {
        Self {
            schema_registry: registry,
        }
    }

    /// Execute any TQL statement (DDL, Query, or EXPLAIN)
    pub async fn execute(&self, sql: &str) -> Result<TqlResult, TqlError> {
        // Try to parse as DDL first
        if let Ok((_, ddl)) = parse_ddl_statement(sql) {
            return self.execute_ddl(ddl).await;
        }

        // Try to parse as EXPLAIN
        if sql.trim().to_uppercase().starts_with("EXPLAIN") {
            let query_sql = sql.trim()[7..].trim(); // Remove "EXPLAIN" prefix
            if let Ok((_, query)) = crate::tql::parser::parse_query(query_sql) {
                return self.explain_query(&query).await;
            } else {
                return Err(TqlError::ParseError(
                    "Failed to parse query for EXPLAIN".to_string(),
                ));
            }
        }

        // Try to parse as regular query
        match crate::tql::parser::parse_query(sql) {
            Ok((_, query)) => {
                // Validate query against schema before execution
                if let Some(ref match_clause) = query.match_clause {
                    let label = &match_clause.source.label;
                    if self.schema_registry.get_node_type(label)?.is_none() {
                        return Err(TqlError::ValidationError(format!(
                            "Unknown node type: {}",
                            label
                        )));
                    }

                    // Validate vector fields in WHERE clause
                    if let Some(ref where_clause) = query.where_clause {
                        self.validate_where_clause(label, where_clause)?;
                    }
                }

                // Query parsed and validated - ready for execution with store
                Ok(TqlResult::QueryReady(query))
            }
            Err(e) => Err(TqlError::ParseError(format!("{:?}", e))),
        }
    }

    /// Execute a parsed query with a given store
    pub async fn execute_query(
        &self,
        store: &HybridPersistentStore,
        query: Query,
    ) -> Result<TqlResult, TqlError> {
        // Validate query against schema before execution
        if let Some(ref match_clause) = &query.match_clause {
            let label = &match_clause.source.label;
            if self.schema_registry.get_node_type(label)?.is_none() {
                return Err(TqlError::ValidationError(format!(
                    "Unknown node type: {}",
                    label
                )));
            }

            // Validate vector fields in WHERE clause
            if let Some(ref where_clause) = &query.where_clause {
                self.validate_where_clause(label, where_clause)?;
            }
        }

        // Execute using QueryExecutor
        let results = QueryExecutor::execute_query(store, query)
            .await
            .map_err(|e| TqlError::ExecutionError(e))?;

        Ok(TqlResult::Query(results))
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
                Ok(TqlResult::DdlSuccess(format!(
                    "Node type '{}' dropped",
                    name
                )))
            }
            DdlStatement::DropEdgeType(name) => {
                self.schema_registry.drop_edge_type(&name)?;
                Ok(TqlResult::DdlSuccess(format!(
                    "Edge type '{}' dropped",
                    name
                )))
            }
            DdlStatement::ShowSchema => {
                let node_types = self.schema_registry.list_node_types()?;
                let edge_types = self.schema_registry.list_edge_types()?;

                let mut output = String::from("SCHEMA:\n\nNode Types:\n");
                for node_type in &node_types {
                    output.push_str(&format!("  - {}\n", node_type));
                }

                output.push_str("\nEdge Types:\n");
                for edge_type in &edge_types {
                    output.push_str(&format!("  - {}\n", edge_type));
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

    /// Validate WHERE clause against schema
    fn validate_where_clause(
        &self,
        node_type: &str,
        where_clause: &crate::tql::ast::WhereCondition,
    ) -> Result<(), TqlError> {
        match where_clause {
            crate::tql::ast::WhereCondition::ToroidalDistance { field, .. } => {
                // Check that the field exists and is a vector type
                let data_type = self.schema_registry.validate_field(node_type, field)?;
                match data_type {
                    crate::tql::ast::DataType::Vector(_) => Ok(()),
                    _ => Err(TqlError::ValidationError(format!(
                        "Field '{}' is not a vector type",
                        field
                    ))),
                }
            }
            crate::tql::ast::WhereCondition::PropertyFilter { property, .. } => {
                // Check that the property exists
                self.schema_registry.validate_field(node_type, property)?;
                Ok(())
            }
        }
    }

    /// Get schema registry (for testing and introspection)
    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    /// Validate that a node conforms to its type schema
    pub fn validate_node(
        &self,
        node_type: &str,
        properties: &serde_json::Value,
    ) -> Result<(), TqlError> {
        let type_def = self
            .schema_registry
            .get_node_type(node_type)?
            .ok_or_else(|| {
                TqlError::ValidationError(format!("Unknown node type: {}", node_type))
            })?;

        // Check required fields
        for field in &type_def.fields {
            if field
                .constraints
                .iter()
                .any(|c| matches!(c, crate::tql::ast::FieldConstraint::NotNull))
            {
                if properties.get(&field.name).is_none() {
                    return Err(TqlError::ValidationError(format!(
                        "Required field '{}' is missing",
                        field.name
                    )));
                }
            }
        }

        Ok(())
    }
}

/// Result of TQL execution
#[derive(Debug, Clone)]
pub enum TqlResult {
    Query(Vec<QueryResult>),
    QueryReady(Query), // Parsed and validated query ready for execution with store
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
            TqlError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            TqlError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            TqlError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            TqlError::SchemaError(e) => write!(f, "Schema error: {}", e),
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
            TqlResult::DdlSuccess(msg) => {
                assert!(msg.contains("created successfully"));
            }
            _ => panic!("Expected DDL success"),
        }

        // Verify the type was created
        let doc_type = engine.schema_registry().get_node_type("Document").unwrap();
        assert!(doc_type.is_some());
        assert_eq!(doc_type.unwrap().fields.len(), 3);
    }

    #[tokio::test]
    async fn test_explain_query() {
        let engine = create_test_engine();

        // First create a node type
        let ddl = r#"CREATE NODE TYPE Document (
            id INT PRIMARY KEY,
            content_t3 VECTOR(1536)
        )"#;
        engine.execute(ddl).await.unwrap();

        // Now EXPLAIN a query
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
                assert!(plan.contains("LOCAL_VECTOR_SEARCH"));
            }
            _ => panic!("Expected EXPLAIN result"),
        }
    }

    #[tokio::test]
    async fn test_validation_unknown_type() {
        let engine = create_test_engine();

        // Try to query a non-existent type
        let sql = r#"MATCH (d:UnknownType) RETURN d.id LIMIT 10"#;

        let result = engine.execute(sql).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            TqlError::ValidationError(msg) => {
                assert!(msg.contains("Unknown node type"));
            }
            _ => panic!("Expected validation error"),
        }
    }

    #[tokio::test]
    async fn test_show_schema() {
        let engine = create_test_engine();

        // Create some types
        let node_ddl = r#"CREATE NODE TYPE Document (id INT PRIMARY KEY)"#;
        engine.execute(node_ddl).await.unwrap();

        let edge_ddl = r#"CREATE EDGE TYPE LINKS (from Document, to Document)"#;
        engine.execute(edge_ddl).await.unwrap();

        // Show schema
        let result = engine.execute("SHOW SCHEMA").await;
        assert!(result.is_ok());

        match result.unwrap() {
            TqlResult::DdlSuccess(schema) => {
                assert!(schema.contains("Document"));
                assert!(schema.contains("LINKS"));
            }
            _ => panic!("Expected schema output"),
        }
    }
}
