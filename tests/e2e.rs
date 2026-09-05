//! End-to-end TQL tests: parse -> execute -> verify against a real store.

use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use toroidal_db::hybrid_storage::{HybridPersistentStore, Node};
use toroidal_db::tql::executor::QueryExecutor;
use toroidal_db::tql::parser;

fn store_with_nodes() -> (TempDir, Arc<HybridPersistentStore>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(HybridPersistentStore::open(dir.path()).unwrap());
    store
        .insert(Node {
            id: 1,
            vector: vec![0.1, 0.2, 0.3, 0.4],
            properties: json!({"name": "node1", "type": "test"}),
            edges: vec![],
        })
        .unwrap();
    store
        .insert(Node {
            id: 2,
            vector: vec![0.15, 0.25, 0.35, 0.45],
            properties: json!({"name": "node2", "type": "test"}),
            edges: vec![],
        })
        .unwrap();
    store
        .insert(Node {
            id: 3,
            vector: vec![0.9, 0.9, 0.9, 0.9],
            properties: json!({"name": "far", "type": "test"}),
            edges: vec![],
        })
        .unwrap();
    (dir, store)
}

#[tokio::test]
async fn parse_and_execute_match_with_limit() {
    let (_dir, store) = store_with_nodes();
    let query = parser::parse_query("MATCH (node:Document) RETURN node.id LIMIT 10")
        .unwrap()
        .1;
    let results = QueryExecutor::execute_query(&store, query).await.unwrap();
    assert!(!results.is_empty(), "expected non-empty result set");
}

#[tokio::test]
async fn parse_toroidal_distance_query() {
    let query = parser::parse_query(
        r#"MATCH (doc:Document) WHERE TOROIDALDISTANCE(doc.vector, 0.3)
           CONNECTEDTO(doc, "TAGGED_WITH", "quantum") WITHIN 2 HOPS
           RETURN doc.id LIMIT 10"#,
    )
    .unwrap()
    .1;
    assert!(query.match_clause.is_some());
    assert!(query.connected_clause.is_some());
    assert!(query.within_clause.is_some());
    assert_eq!(query.limit, 10);
}

#[tokio::test]
async fn graph_traversal_returns_connected_nodes() {
    let (_dir, store) = store_with_nodes();
    store.add_edge(1, 2, "SIMILAR".to_string(), 0.9).unwrap();
    let query = parser::parse_query(r#"MATCH (n)-[:SIMILAR]->(m) RETURN n.id, m.id LIMIT 10"#)
        .unwrap()
        .1;
    let results = QueryExecutor::execute_query(&store, query).await.unwrap();
    assert!(!results.is_empty(), "traversal should find the edge 1->2");
}

#[tokio::test]
async fn invalid_query_fails_to_parse() {
    assert!(parser::parse_query("THIS IS NOT TQL").is_err());
}
