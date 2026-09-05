//! Trigger runtime for TQL — выполняет условия при вставке/обновлении узлов.
//!
//! Использует `ExpressionEvaluator` для проверки условий и `SubscriptionManager` для уведомлений.

use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::evaluator::{EvalContext, Expression, ExpressionEvaluator};
use crate::tql::subscription_manager::SubscriptionManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Триггер: условие + действие.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDef {
    pub name: String,
    pub on_table: String,
    pub on_event: TriggerEvent,
    pub condition: Expression,
    pub action: TriggerAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerEvent {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerAction {
    EmitEvent { topic: String, payload: Expression },
    ExecuteQuery(String),
}

/// Менеджер триггеров с полноценным evaluation.
pub struct TriggerManager {
    triggers: RwLock<Vec<TriggerDef>>,
    store: Arc<HybridPersistentStore>,
    evaluator: ExpressionEvaluator,
    globals: RwLock<HashMap<String, crate::tql::ast::PropertyValue>>,
}

impl TriggerManager {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self {
            triggers: RwLock::new(Vec::new()),
            store,
            evaluator: ExpressionEvaluator::new(),
            globals: RwLock::new(HashMap::new()),
        }
    }

    /// Register a trigger.
    pub async fn register(&self, trigger: TriggerDef) {
        self.triggers.write().await.push(trigger);
    }

    /// List all triggers.
    pub async fn list(&self) -> Vec<TriggerDef> {
        self.triggers.read().await.clone()
    }

    /// Remove a trigger by name.
    pub async fn remove(&self, name: &str) {
        self.triggers.write().await.retain(|t| t.name != name);
    }

    /// Set a global variable (e.g. $breakthrough_centroid).
    pub async fn set_global(&self, key: &str, value: crate::tql::ast::PropertyValue) {
        self.globals.write().await.insert(key.to_string(), value);
    }

    /// Called when a node is inserted — evaluates all applicable triggers.
    pub async fn on_insert(
        &self,
        table: &str,
        new_row: &HashMap<String, crate::tql::ast::PropertyValue>,
    ) {
        let triggers = self.triggers.read().await.clone();
        let globals = self.globals.read().await.clone();

        for trigger in &triggers {
            if trigger.on_table != table || trigger.on_event != TriggerEvent::Insert {
                continue;
            }

            let ctx = EvalContext::with_globals(new_row.clone(), globals.clone());
            let result = self.evaluator.eval(&trigger.condition, &ctx);

            match result {
                Ok(crate::tql::ast::PropertyValue::Boolean(true)) => {
                    self.execute_action(&trigger.action, new_row).await;
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "Trigger '{}' condition evaluation error: {}",
                        trigger.name, e
                    );
                }
            }
        }
    }

    /// Called when a node is updated.
    pub async fn on_update(
        &self,
        table: &str,
        new_row: &HashMap<String, crate::tql::ast::PropertyValue>,
    ) {
        // Same as on_insert for now
        self.on_insert(table, new_row).await;
    }

    async fn execute_action(
        &self,
        action: &TriggerAction,
        row: &HashMap<String, crate::tql::ast::PropertyValue>,
    ) {
        match action {
            TriggerAction::EmitEvent { topic, payload } => {
                let ctx = EvalContext::new(row.clone());
                let payload_val = self.evaluator.eval(payload, &ctx);
                match payload_val {
                    Ok(val) => {
                        println!("🔔 Trigger EMIT EVENT '{}': {:?}", topic, val);
                        // TODO: send to event bus / subscription manager
                    }
                    Err(e) => {
                        eprintln!("Trigger emit payload error: {}", e);
                    }
                }
            }
            TriggerAction::ExecuteQuery(query) => {
                println!("🔔 Trigger EXECUTE QUERY: {}", query);
                // TODO: execute via TqlEngine
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tql::ast::PropertyValue;
    use crate::tql::evaluator::Expression;

    #[tokio::test]
    async fn test_trigger_register_list() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());
        let mgr = TriggerManager::new(store);

        let trigger = TriggerDef {
            name: "test_trigger".to_string(),
            on_table: "Document".to_string(),
            on_event: TriggerEvent::Insert,
            condition: Expression::Literal(PropertyValue::Boolean(true)),
            action: TriggerAction::EmitEvent {
                topic: "test".to_string(),
                payload: Expression::Literal(PropertyValue::String("hello".to_string())),
            },
        };

        mgr.register(trigger).await;
        let list = mgr.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "test_trigger");
    }

    #[tokio::test]
    async fn test_trigger_on_insert() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());
        let mgr = TriggerManager::new(store);

        mgr.register(TriggerDef {
            name: "score_check".to_string(),
            on_table: "Document".to_string(),
            on_event: TriggerEvent::Insert,
            condition: Expression::BinaryOp {
                left: Box::new(Expression::FieldAccess {
                    field: "score".to_string(),
                }),
                op: crate::tql::evaluator::BinaryOperator::Gt,
                right: Box::new(Expression::Literal(PropertyValue::Number(0.9))),
            },
            action: TriggerAction::EmitEvent {
                topic: "high_score".to_string(),
                payload: Expression::Literal(PropertyValue::String("high_score".to_string())),
            },
        })
        .await;

        // Insert row with low score — should NOT trigger
        let mut row = HashMap::new();
        row.insert("score".to_string(), PropertyValue::Number(0.5));
        mgr.on_insert("Document", &row).await;

        // Insert row with high score — should trigger
        let mut high_row = HashMap::new();
        high_row.insert("score".to_string(), PropertyValue::Number(0.95));
        mgr.on_insert("Document", &high_row).await;
    }
}
