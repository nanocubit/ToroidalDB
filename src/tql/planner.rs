use crate::index::{Condition, Filter};
use crate::tql::ast::{BackendHint, Query, QueryHints, WhereCondition};
use crate::tql::cost_optimizer::CostBasedOptimizer;
use serde::{Deserialize, Serialize};

/// Search strategy selected by the query planner
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SearchStrategy {
    /// Vector search first, then filter results
    VectorFirst { ef: usize, oversampling: f32 },
    /// Filter first (if highly selective), then vector search on subset
    FilterFirst { estimated_selectivity: f32 },
    /// Filter-aware HNSW (payload_m) handles both
    FilterAwareHnsw { payload_m: usize },
}

/// Logical execution plan for TQL queries
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicalPlan {
    pub steps: Vec<PlanStep>,
    pub estimated_cost: f64,
    pub estimated_rows: u64,
    pub backend: ExecutionBackend,
    pub search_strategy: SearchStrategy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlanStep {
    /// Route query to appropriate shards
    RouteShards {
        shard_ids: Vec<u32>,
        strategy: RoutingStrategy,
    },
    /// Perform vector search on local shard
    LocalVectorSearch {
        field: String,
        threshold: f32,
        limit: u32,
        backend: ExecutionBackend,
    },
    /// Traverse graph edges
    GraphTraversal {
        edge_type: Option<String>,
        max_hops: u32,
        min_hops: u32,
        direction: TraversalDirection,
    },
    /// Merge top-K results from shards
    TopKMerge { k: u32, sort_field: Option<String> },
    /// Apply filter conditions
    Filter { conditions: Vec<FilterCondition> },
    /// Apply aggregation functions
    Aggregate { functions: Vec<AggregationStep> },
    /// Sort results
    Sort { field: String, ascending: bool },
    /// Project final fields
    Project { fields: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RoutingStrategy {
    HashRing { radius: u32 },
    Broadcast,
    SingleShard(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionBackend {
    Auto,
    Gpu,
    Avx512,
    Avx2,
    Scalar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterCondition {
    PropertyEquals { field: String, value: String },
    PropertyRange { field: String, min: f64, max: f64 },
    ToroidalDistance { field: String, threshold: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationStep {
    pub function: String,
    pub field: String,
    pub alias: Option<String>,
}

/// Query plan builder
pub struct PlanBuilder;

impl PlanBuilder {
    /// Determine search strategy: filter-first vs vector-first vs filter-aware HNSW
    fn select_strategy(query: &Query) -> SearchStrategy {
        // If payload_m is configured, use filter-aware HNSW
        let has_property_filter = match &query.where_clause {
            Some(WhereCondition::PropertyFilter { .. }) => true,
            _ => false,
        };
        let has_vector_search = match &query.where_clause {
            Some(WhereCondition::ToroidalDistance { .. }) => true,
            _ => false,
        };

        if has_property_filter && !has_vector_search {
            // Only property filter, no vector search — filter-first
            return SearchStrategy::FilterFirst {
                estimated_selectivity: 0.1,
            };
        }

        if has_property_filter && has_vector_search {
            // Both filter and vector — estimate selectivity
            // For now, use a heuristic: if filter is likely selective, use filter-first
            // In production, use histogram statistics from CostBasedOptimizer
            return SearchStrategy::FilterAwareHnsw { payload_m: 8 };
        }

        // Default: vector-first
        SearchStrategy::VectorFirst {
            ef: 100,
            oversampling: 2.0,
        }
    }

    /// Build a logical plan from a TQL query
    pub fn build(query: &Query) -> LogicalPlan {
        let mut steps = Vec::new();
        let optimizer = CostBasedOptimizer::new();

        // Use CostBasedOptimizer for accurate cost estimation
        let cost = optimizer.estimate_cost(query);
        let estimated_rows = optimizer.estimate_result_cardinality(query);

        // Determine backend from hints or auto-select
        let backend = Self::select_backend(query);

        // Step 1: Route to shards
        let routing = if let Some(ref hints) = query.hints {
            if let Some(n) = hints.scatter_shards {
                RoutingStrategy::HashRing { radius: n }
            } else if hints.prefer_local_shard {
                RoutingStrategy::SingleShard(0)
            } else {
                RoutingStrategy::HashRing { radius: 5 }
            }
        } else {
            RoutingStrategy::HashRing { radius: 5 }
        };

        steps.push(PlanStep::RouteShards {
            shard_ids: vec![],
            strategy: routing,
        });

        // Step 2: Vector search if TOROIDALDISTANCE present
        if let Some(where_clause) = &query.where_clause {
            match where_clause {
                WhereCondition::ToroidalDistance { field, threshold } => {
                    steps.push(PlanStep::LocalVectorSearch {
                        field: field.clone(),
                        threshold: *threshold,
                        limit: query.limit,
                        backend: backend.clone(),
                    });
                }
                _ => {}
            }
        }

        // Step 3: Graph traversal if CONNECTEDTO/WITHIN present
        if let Some(connected) = &query.connected_clause {
            let max_hops = query
                .within_clause
                .as_ref()
                .map(|w| w.max_hops)
                .unwrap_or(2);
            let min_hops = query
                .within_clause
                .as_ref()
                .map(|w| w.min_hops)
                .unwrap_or(1);

            steps.push(PlanStep::GraphTraversal {
                edge_type: Some(connected.relationship_type.clone()),
                max_hops,
                min_hops,
                direction: TraversalDirection::Both,
            });
        }

        // Step 4: Merge top-K from shards
        steps.push(PlanStep::TopKMerge {
            k: query.limit,
            sort_field: query.order_by.as_ref().map(|o| o.field.clone()),
        });

        // Step 5: Sort if ORDER BY present
        if let Some(order_by) = &query.order_by {
            steps.push(PlanStep::Sort {
                field: order_by.field.clone(),
                ascending: order_by.ascending,
            });
        }

        // Step 6: Aggregations if present
        if !query.aggregation_fields.is_empty() {
            let agg_steps: Vec<AggregationStep> = query
                .aggregation_fields
                .iter()
                .map(|agg| AggregationStep {
                    function: format!("{:?}", agg.function),
                    field: format!("{:?}", agg.function),
                    alias: agg.alias.clone(),
                })
                .collect();

            steps.push(PlanStep::Aggregate {
                functions: agg_steps,
            });
        }

        // Step 7: Project final fields
        if !query.return_fields.is_empty() {
            steps.push(PlanStep::Project {
                fields: query.return_fields.clone(),
            });
        }

        LogicalPlan {
            steps,
            estimated_cost: cost.total_cost,
            estimated_rows: estimated_rows as u64,
            backend,
            search_strategy: Self::select_strategy(query),
        }
    }

    /// Select execution backend based on hints or auto-detection
    fn select_backend(query: &Query) -> ExecutionBackend {
        if let Some(ref hints) = query.hints {
            if let Some(ref backend_hint) = hints.backend {
                return match backend_hint {
                    BackendHint::Gpu => ExecutionBackend::Gpu,
                    BackendHint::Avx512 => ExecutionBackend::Avx512,
                    BackendHint::Avx2 => ExecutionBackend::Avx2,
                    BackendHint::Scalar => ExecutionBackend::Scalar,
                    BackendHint::Auto => ExecutionBackend::Auto,
                };
            }
            if hints.force_gpu {
                return ExecutionBackend::Gpu;
            }
        }
        ExecutionBackend::Auto
    }
}

/// EXPLAIN query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainResult {
    pub plan: LogicalPlan,
    pub pretty_plan: String,
}

impl ExplainResult {
    /// Format plan as human-readable text
    pub fn format_plan(&self) -> String {
        let mut output = String::new();
        output.push_str("QUERY PLAN:\n");
        output.push_str(&format!(
            "Estimated cost: {:.2}\n",
            self.plan.estimated_cost
        ));
        output.push_str(&format!("Estimated rows: {}\n", self.plan.estimated_rows));
        output.push_str(&format!("Backend: {:?}\n", self.plan.backend));
        output.push_str(&format!("Strategy: {:?}\n\n", self.plan.search_strategy));

        for (i, step) in self.plan.steps.iter().enumerate() {
            output.push_str(&format!("{}. ", i + 1));
            match step {
                PlanStep::RouteShards { strategy, .. } => {
                    output.push_str(&format!("ROUTE_SHARDS strategy={:?}\n", strategy));
                }
                PlanStep::LocalVectorSearch {
                    field,
                    threshold,
                    limit,
                    backend,
                } => {
                    output.push_str(&format!(
                        "LOCAL_VECTOR_SEARCH field={} threshold={} limit={} backend={:?}\n",
                        field, threshold, limit, backend
                    ));
                }
                PlanStep::GraphTraversal {
                    edge_type,
                    max_hops,
                    min_hops,
                    direction,
                } => {
                    output.push_str(&format!(
                        "GRAPH_TRAVERSAL edge={:?} hops={}..{} direction={:?}\n",
                        edge_type, min_hops, max_hops, direction
                    ));
                }
                PlanStep::TopKMerge { k, sort_field } => {
                    output.push_str(&format!("TOP_K_MERGE k={} sort={:?}\n", k, sort_field));
                }
                PlanStep::Filter { conditions } => {
                    output.push_str(&format!("FILTER conditions={}\n", conditions.len()));
                }
                PlanStep::Aggregate { functions } => {
                    output.push_str(&format!("AGGREGATE functions={}\n", functions.len()));
                }
                PlanStep::Sort { field, ascending } => {
                    output.push_str(&format!(
                        "SORT field={} order={}\n",
                        field,
                        if *ascending { "ASC" } else { "DESC" }
                    ));
                }
                PlanStep::Project { fields } => {
                    output.push_str(&format!("PROJECT fields=[{}]\n", fields.join(", ")));
                }
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tql::ast::{MatchClause, NodePattern, Query, WhereCondition};

    #[test]
    fn test_build_simple_plan() {
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "d".to_string(),
                    label: "Document".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: Some(WhereCondition::ToroidalDistance {
                field: "content_t3".to_string(),
                threshold: 0.3,
            }),
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["d.id".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 10,
            distributed: false,
            ..Default::default()
        };

        let plan = PlanBuilder::build(&query);

        assert!(!plan.steps.is_empty());
        assert!(matches!(plan.steps[0], PlanStep::RouteShards { .. }));
    }

    #[test]
    fn test_explain_format() {
        let query = Query {
            match_clause: Some(MatchClause {
                source: NodePattern {
                    alias: "d".to_string(),
                    label: "Document".to_string(),
                    properties: None,
                },
                relationship: None,
                target: None,
            }),
            where_clause: Some(WhereCondition::ToroidalDistance {
                field: "content_t3".to_string(),
                threshold: 0.3,
            }),
            connected_clause: None,
            within_clause: None,
            return_fields: vec!["d.id".to_string()],
            aggregation_fields: vec![],
            order_by: None,
            subqueries: vec![],
            transaction: None,
            limit: 10,
            distributed: false,
            ..Default::default()
        };

        let plan = PlanBuilder::build(&query);
        let explain = ExplainResult {
            plan,
            pretty_plan: String::new(),
        };

        let formatted = explain.format_plan();
        assert!(formatted.contains("QUERY PLAN:"));
        assert!(formatted.contains("ROUTE_SHARDS"));
        assert!(formatted.contains("LOCAL_VECTOR_SEARCH"));
    }
}
