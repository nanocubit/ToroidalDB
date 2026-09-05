use crate::hybrid_storage::{Edge, HybridPersistentStore as PersistentStore, Node};
use crate::tql::ast::{NodePattern, PropertyValue, Transaction, TransactionOperation};
use std::collections::HashMap;
use std::sync::Arc;

pub struct TransactionManager;

impl TransactionManager {
    pub async fn execute_transaction(
        store: &PersistentStore,
        transaction: Transaction,
    ) -> Result<(), String> {
        let mut temp_store: Vec<Node> = Vec::new();

        for operation in transaction.operations {
            match operation {
                TransactionOperation::CreateNode(node_pattern) => {
                    let new_node = Node {
                        id: Self::generate_id(),
                        vector: vec![0.0; 384],
                        properties: Self::convert_properties(&node_pattern.properties),
                        edges: Vec::new(),
                    };
                    match store.insert(new_node.clone()) {
                        Ok(_) => {}
                        Err(e) => return Err(format!("Failed to create node: {}", e)),
                    }
                    temp_store.push(new_node);
                }
                TransactionOperation::UpdateNode(node_pattern, updates) => {
                    if let Some((_id_prop, PropertyValue::Number(id))) = node_pattern
                        .properties
                        .as_ref()
                        .and_then(|props| props.iter().find(|(k, _)| k == "id"))
                    {
                        let node_id = *id as u64;
                        if let Ok(Some(mut node)) = store.get(node_id) {
                            for (field, value) in updates {
                                match value {
                                    PropertyValue::String(s) => {
                                        node.properties[field] = serde_json::Value::String(s);
                                    }
                                    PropertyValue::Number(n) => {
                                        node.properties[field] = serde_json::Value::Number(
                                            serde_json::Number::from(n as i64),
                                        );
                                    }
                                    PropertyValue::Boolean(b) => {
                                        node.properties[field] = serde_json::Value::Bool(b);
                                    }
                                }
                            }
                            if store.insert(node).is_err() {
                                return Err("Failed to update node".to_string());
                            }
                        } else {
                            return Err(format!("Node with id {} not found", node_id));
                        }
                    }
                }
                TransactionOperation::DeleteNode(node_pattern) => {
                    if let Some((_id_prop, PropertyValue::Number(id))) = node_pattern
                        .properties
                        .as_ref()
                        .and_then(|props| props.iter().find(|(k, _)| k == "id"))
                    {
                        let node_id = *id as u64;
                        let all_nodes = store.get_all().map_err(|e| {
                            format!("Failed to get all nodes for edge cleanup: {}", e)
                        })?;
                        for other_node in all_nodes {
                            if other_node.edges.iter().any(|e| e.target_id == node_id) {
                                let mut updated_node = other_node.clone();
                                updated_node.edges.retain(|e| e.target_id != node_id);
                                store
                                    .update_node(other_node.id, updated_node)
                                    .map_err(|e| {
                                        format!("Failed to remove incoming edges: {}", e)
                                    })?;
                            }
                        }
                        let removed = store
                            .remove(node_id)
                            .map_err(|e| format!("Failed to delete node {}: {}", node_id, e))?;
                        if !removed {
                            return Err(format!("Node {} not found for deletion", node_id));
                        }
                    }
                }
                TransactionOperation::CreateEdge(source_id, target_id, relation_type) => {
                    let source_id_num =
                        source_id.parse::<u64>().map_err(|_| "Invalid source ID")?;
                    let target_id_num =
                        target_id.parse::<u64>().map_err(|_| "Invalid target ID")?;
                    match store.get(source_id_num) {
                        Ok(Some(mut node)) => {
                            node.edges.push(Edge {
                                target_id: target_id_num,
                                relation_type,
                                weight: 1.0,
                            });
                            if store.insert(node).is_err() {
                                return Err("Failed to add edge to node".to_string());
                            }
                        }
                        Ok(None) => {
                            return Err(format!("Source node {} not found", source_id_num));
                        }
                        Err(e) => {
                            return Err(format!("Error getting source node: {}", e));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Create a savepoint: snapshot current state of affected nodes.
    pub async fn create_savepoint(
        store: &PersistentStore,
        node_ids: &[u64],
    ) -> Result<HashMap<u64, Node>, String> {
        let mut snapshot = HashMap::new();
        for &id in node_ids {
            if let Ok(Some(node)) = store.get(id) {
                snapshot.insert(id, node);
            }
        }
        Ok(snapshot)
    }

    /// Restore to a savepoint: revert nodes to saved state.
    pub async fn restore_savepoint(
        store: &PersistentStore,
        savepoint: &HashMap<u64, Node>,
    ) -> Result<(), String> {
        for (_, node) in savepoint {
            store
                .insert(node.clone())
                .map_err(|e| format!("Failed to restore savepoint: {}", e))?;
        }
        Ok(())
    }

    fn generate_id() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos() as u64
    }

    fn convert_properties(properties: &Option<Vec<(String, PropertyValue)>>) -> serde_json::Value {
        match properties {
            Some(props) => {
                let mut obj = serde_json::Map::new();
                for (key, value) in props {
                    match value {
                        PropertyValue::String(s) => {
                            obj.insert(key.clone(), serde_json::Value::String(s.clone()));
                        }
                        PropertyValue::Number(n) => {
                            obj.insert(
                                key.clone(),
                                serde_json::Value::Number(serde_json::Number::from(*n as i64)),
                            );
                        }
                        PropertyValue::Boolean(b) => {
                            obj.insert(key.clone(), serde_json::Value::Bool(*b));
                        }
                    }
                }
                serde_json::Value::Object(obj)
            }
            None => serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}
