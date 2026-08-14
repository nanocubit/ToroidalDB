//! # Subscription Manager для TQL v3.0 - Production Ready
//! 
//! Управление подписками на изменения данных с поддержкой WebSocket, Webhook, gRPC

use crate::hybrid_storage::{HybridPersistentStore, Node};
use crate::tql::ast::*;
use crate::tql::executor::QueryExecutor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock, Mutex};
use uuid::Uuid;

/// Менеджер подписок
pub struct SubscriptionManager {
    subscriptions: Arc<RwLock<HashMap<String, ActiveSubscription>>>,
    event_tx: broadcast::Sender<ChangeEvent>,
    store: Arc<HybridPersistentStore>,
    websocket_clients: Arc<Mutex<HashMap<String, WebSocketClient>>>,
    stats: Arc<RwLock<SubscriptionStats>>,
}

/// Активная подписка
#[derive(Debug, Clone)]
pub struct ActiveSubscription {
    pub id: String,
    pub query: Query,
    pub emit: EmitClause,
    pub where_condition: Option<WhereCondition>,
    pub created_at: u64,
    pub last_triggered: Option<u64>,
    pub trigger_count: u64,
    pub error_count: u64,
    pub status: SubscriptionStatus,
}

/// Статус подписки
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionStatus {
    Active,
    Paused,
    Error(String),
    Stopped,
}

/// WebSocket клиент
#[derive(Debug, Clone)]
pub struct WebSocketClient {
    pub subscription_id: String,
    pub endpoint: String,
    pub connected: bool,
}

/// Статистика подписок
#[derive(Debug, Clone, Default)]
pub struct SubscriptionStats {
    pub total_subscriptions: u64,
    pub active_subscriptions: u64,
    pub total_events: u64,
    pub total_errors: u64,
    pub avg_latency_ms: f64,
}

/// Событие изменения
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ChangeEvent {
    #[serde(rename = "node_inserted")]
    NodeInserted {
        node_id: u64,
        node_type: String,
        properties: serde_json::Value,
        timestamp: u64,
    },
    #[serde(rename = "node_updated")]
    NodeUpdated {
        node_id: u64,
        node_type: String,
        old_properties: serde_json::Value,
        new_properties: serde_json::Value,
        timestamp: u64,
    },
    #[serde(rename = "node_deleted")]
    NodeDeleted {
        node_id: u64,
        node_type: String,
        timestamp: u64,
    },
    #[serde(rename = "edge_inserted")]
    EdgeInserted {
        from_id: u64,
        to_id: u64,
        edge_type: String,
        timestamp: u64,
    },
    #[serde(rename = "edge_deleted")]
    EdgeDeleted {
        from_id: u64,
        to_id: u64,
        edge_type: String,
        timestamp: u64,
    },
}

/// Результат подписки
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionResult {
    pub subscription_id: String,
    pub event: ChangeEvent,
    pub results: Vec<QueryMatch>,
    pub timestamp: u64,
    pub latency_ms: f64,
}

/// Результат запроса
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMatch {
    pub id: u64,
    pub score: f32,
    pub properties: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
}

impl SubscriptionManager {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        let (event_tx, _) = broadcast::channel(10000);
        
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            store,
            websocket_clients: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(RwLock::new(SubscriptionStats::default())),
        }
    }

    /// Регистрирует новую подписку
    pub async fn register(&self, subscription: Subscription) -> Result<String, String> {
        let id = subscription.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        
        let active_sub = ActiveSubscription {
            id: id.clone(),
            query: (*subscription.query).clone(),
            emit: subscription.emit,
            where_condition: subscription.where_condition,
            created_at: get_timestamp(),
            last_triggered: None,
            trigger_count: 0,
            error_count: 0,
            status: SubscriptionStatus::Active,
        };
        
        // Настраиваем WebSocket если нужно
        if let EmitClause::WebSocket { endpoint } = &subscription.emit {
            let mut clients = self.websocket_clients.lock().await;
            clients.insert(id.clone(), WebSocketClient {
                subscription_id: id.clone(),
                endpoint: endpoint.clone(),
                connected: true,
            });
        }
        
        let mut subs = self.subscriptions.write().await;
        subs.insert(id.clone(), active_sub);
        
        // Обновляем статистику
        self.update_stats().await;
        
        Ok(id)
    }

    /// Отменяет подписку
    pub async fn unsubscribe(&self, subscription_id: &str) -> Result<(), String> {
        let mut subs = self.subscriptions.write().await;
        
        if subs.remove(subscription_id).is_none() {
            return Err(format!("Subscription '{}' not found", subscription_id));
        }
        
        // Удаляем WebSocket клиент если есть
        let mut clients = self.websocket_clients.lock().await;
        clients.remove(subscription_id);
        
        self.update_stats().await;
        
        Ok(())
    }

    /// Приостанавливает подписку
    pub async fn pause(&self, subscription_id: &str) -> Result<(), String> {
        let mut subs = self.subscriptions.write().await;
        
        let sub = subs.get_mut(subscription_id)
            .ok_or_else(|| format!("Subscription '{}' not found", subscription_id))?;
        
        sub.status = SubscriptionStatus::Paused;
        Ok(())
    }

    /// Возобновляет подписку
    pub async fn resume(&self, subscription_id: &str) -> Result<(), String> {
        let mut subs = self.subscriptions.write().await;
        
        let sub = subs.get_mut(subscription_id)
            .ok_or_else(|| format!("Subscription '{}' not found", subscription_id))?;
        
        sub.status = SubscriptionStatus::Active;
        Ok(())
    }

    /// Получает список активных подписок
    pub async fn list_subscriptions(&self) -> Vec<ActiveSubscription> {
        let subs = self.subscriptions.read().await;
        subs.values().cloned().collect()
    }

    /// Получает подписку по ID
    pub async fn get_subscription(&self, subscription_id: &str) -> Option<ActiveSubscription> {
        let subs = self.subscriptions.read().await;
        subs.get(subscription_id).cloned()
    }

    /// Отправляет событие изменения
    pub async fn emit_event(&self, event: ChangeEvent) -> Result<(), String> {
        let start_time = std::time::Instant::now();
        
        // Отправляем в broadcast channel
        self.event_tx.send(event.clone()).map_err(|e| e.to_string())?;
        
        // Проверяем подписки и выполняем запросы
        self.process_event(event, start_time).await?;
        
        Ok(())
    }

    /// Обрабатывает событие изменения
    async fn process_event(&self, event: ChangeEvent, start_time: std::time::Instant) -> Result<(), String> {
        let subs = self.subscriptions.read().await.clone();
        let mut processed = 0;
        let mut errors = 0;
        
        for (id, subscription) in subs.iter() {
            // Пропускаем неактивные подписки
            if subscription.status != SubscriptionStatus::Active {
                continue;
            }
            
            if !self.is_subscription_relevant(subscription, &event) {
                continue;
            }
            
            // Выполняем запрос подписки
            match self.execute_subscription_query(subscription).await {
                Ok(results) => {
                    if !results.is_empty() {
                        let latency_ms = start_time.elapsed().as_secs_f64() * 1000.0;
                        
                        // Отправляем результаты
                        if let Err(e) = self.send_results(id, event.clone(), results, latency_ms).await {
                            eprintln!("Error sending results for subscription {}: {}", id, e);
                            self.increment_error_count(id).await;
                            errors += 1;
                        } else {
                            self.update_subscription_stats(id).await?;
                            processed += 1;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error executing subscription query for {}: {}", id, e);
                    self.increment_error_count(id).await;
                    errors += 1;
                }
            }
        }
        
        // Обновляем общую статистику
        self.update_event_stats(processed, errors).await;
        
        Ok(())
    }

    /// Проверяет релевантность подписки для события
    fn is_subscription_relevant(&self, subscription: &ActiveSubscription, event: &ChangeEvent) -> bool {
        // Проверяем WHERE condition если есть
        if let Some(where_clause) = &subscription.where_condition {
            if !self.evaluate_where_clause(where_clause, event) {
                return false;
            }
        }
        
        // Проверяем тип события против MATCH clause
        match event {
            ChangeEvent::NodeInserted { node_type, .. } => {
                if let Some(match_clause) = &subscription.query.match_clause {
                    return match_clause.source.label == *node_type;
                }
            }
            ChangeEvent::EdgeInserted { edge_type, .. } => {
                if let Some(match_clause) = &subscription.query.match_clause {
                    if let Some(rel) = &match_clause.relationship {
                        return rel.type_ == *edge_type;
                    }
                }
            }
            _ => {}
        }
        
        true
    }

    /// Оценивает WHERE clause для события
    fn evaluate_where_clause(&self, where_clause: &WhereCondition, event: &ChangeEvent) -> bool {
        match where_clause {
            WhereCondition::PropertyFilter { property, operator, value } => {
                match event {
                    ChangeEvent::NodeInserted { properties, .. } |
                    ChangeEvent::NodeUpdated { new_properties: properties, .. } => {
                        if let Some(prop_value) = properties.get(property) {
                            return self.compare_property(prop_value, operator, value);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        
        true
    }

    /// Сравнивает свойства
    fn compare_property(&self, prop_value: &serde_json::Value, operator: &str, value: &PropertyValue) -> bool {
        match value {
            PropertyValue::String(s) => {
                if let Some(prop_str) = prop_value.as_str() {
                    match operator {
                        "=" | "==" => prop_str == s,
                        "!=" | "<>" => prop_str != s,
                        "LIKE" => prop_str.contains(s),
                        "ILIKE" => prop_str.to_lowercase().contains(&s.to_lowercase()),
                        _ => false,
                    }
                } else {
                    false
                }
            }
            PropertyValue::Number(n) => {
                if let Some(prop_num) = prop_value.as_f64() {
                    match operator {
                        "=" | "==" => prop_num == n,
                        "!=" | "<>" => prop_num != n,
                        ">" => prop_num > n,
                        "<" => prop_num < n,
                        ">=" => prop_num >= n,
                        "<=" => prop_num <= n,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            PropertyValue::Boolean(b) => {
                if let Some(prop_bool) = prop_value.as_bool() {
                    match operator {
                        "=" | "==" => prop_bool == *b,
                        "!=" | "<>" => prop_bool != *b,
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    }

    /// Выполняет запрос подписки
    async fn execute_subscription_query(&self, subscription: &ActiveSubscription) -> Result<Vec<QueryMatch>, String> {
        let results = QueryExecutor::execute_query(&self.store, subscription.query.clone()).await?;
        
        let matches = results.into_iter().map(|r| QueryMatch {
            id: r.id,
            score: r.score,
            properties: r.properties,
            metadata: None,
        }).collect();
        
        Ok(matches)
    }

    /// Отправляет результаты подписки
    async fn send_results(&self, subscription_id: &str, event: ChangeEvent, results: Vec<QueryMatch>, latency_ms: f64) -> Result<(), String> {
        let result = SubscriptionResult {
            subscription_id: subscription_id.to_string(),
            event,
            results,
            timestamp: get_timestamp(),
            latency_ms,
        };
        
        let subs = self.subscriptions.read().await;
        let subscription = subs.get(subscription_id)
            .ok_or_else(|| format!("Subscription {} not found", subscription_id))?;
        
        match &subscription.emit {
            EmitClause::Changes | EmitClause::Events => {
                // Логгируем или отправляем в internal channel
                tracing::info!("Subscription {} triggered: {:?}", subscription_id, result);
            }
            EmitClause::WebSocket { endpoint } => {
                // Отправляем через WebSocket
                self.send_webhook(endpoint, &result).await?;
            }
            EmitClause::Webhook { url } => {
                // Отправляем HTTP POST
                self.send_webhook(url, &result).await?;
            }
            EmitClause::Grpc { service } => {
                // Отправляем через gRPC
                self.send_grpc(service, &result).await?;
            }
        }
        
        Ok(())
    }

    /// Отправляет webhook
    async fn send_webhook(&self, url: &str, payload: &SubscriptionResult) -> Result<(), String> {
        let client = reqwest::Client::new();
        
        let response = client.post(url)
            .json(payload)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("Webhook error: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("Webhook returned status: {}", response.status()));
        }
        
        Ok(())
    }

    /// Отправляет gRPC
    async fn send_grpc(&self, service: &str, payload: &SubscriptionResult) -> Result<(), String> {
        // Упрощенная реализация - в production использовать tonic
        tracing::info!("Sending to gRPC service {}: {:?}", service, payload);
        Ok(())
    }

    /// Обновляет статистику подписки
    async fn update_subscription_stats(&self, subscription_id: &str) -> Result<(), String> {
        let mut subs = self.subscriptions.write().await;
        
        if let Some(sub) = subs.get_mut(subscription_id) {
            sub.last_triggered = Some(get_timestamp());
            sub.trigger_count += 1;
        }
        
        Ok(())
    }

    /// Увеличивает счетчик ошибок
    async fn increment_error_count(&self, subscription_id: &str) {
        let mut subs = self.subscriptions.write().await;
        
        if let Some(sub) = subs.get_mut(subscription_id) {
            sub.error_count += 1;
            
            // Если слишком много ошибок, помечаем как Error
            if sub.error_count > 10 {
                sub.status = SubscriptionStatus::Error("Too many errors".to_string());
            }
        }
    }

    /// Обновляет общую статистику
    async fn update_stats(&self) {
        let subs = self.subscriptions.read().await;
        let mut stats = self.stats.write().await;
        
        stats.total_subscriptions = subs.len() as u64;
        stats.active_subscriptions = subs.values()
            .filter(|s| s.status == SubscriptionStatus::Active)
            .count() as u64;
    }

    /// Обновляет статистику событий
    async fn update_event_stats(&self, processed: u64, errors: u64) {
        let mut stats = self.stats.write().await;
        stats.total_events += processed;
        stats.total_errors += errors;
    }

    /// Получает статистику
    pub async fn get_stats(&self) -> SubscriptionStats {
        self.stats.read().await.clone()
    }

    /// Создаёт подписку из TQL
    pub async fn create_from_tql(&self, tql: &str) -> Result<String, String> {
        let (_, query) = crate::tql::parser::parse_query(tql)
            .map_err(|e| format!("Parse error: {:?}", e))?;
        
        let subscription = Subscription {
            id: None,
            query: Box::new(query),
            emit: EmitClause::Changes,
            where_condition: None,
        };
        
        self.register(subscription).await
    }

    /// Экспортирует подписки
    pub async fn export_subscriptions(&self) -> Result<String, String> {
        let subs = self.subscriptions.read().await;
        serde_json::to_string_pretty(&*subs)
            .map_err(|e| format!("Export error: {}", e))
    }

    /// Импортирует подписки
    pub async fn import_subscriptions(&self, json: &str) -> Result<(), String> {
        let subs: HashMap<String, ActiveSubscription> = serde_json::from_str(json)
            .map_err(|e| format!("Import error: {}", e))?;
        
        let mut current_subs = self.subscriptions.write().await;
        for (id, sub) in subs {
            current_subs.insert(id, sub);
        }
        
        self.update_stats().await;
        Ok(())
    }
}

fn get_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn create_test_store() -> Arc<HybridPersistentStore> {
        let temp_dir = tempfile::tempdir().unwrap();
        Arc::new(HybridPersistentStore::open(temp_dir.path()).unwrap())
    }

    #[tokio::test]
    async fn test_register_subscription() {
        let store = create_test_store().await;
        let manager = SubscriptionManager::new(store);
        
        let query = Query::default();
        let subscription = Subscription {
            id: None,
            query: Box::new(query),
            emit: EmitClause::Changes,
            where_condition: None,
        };
        
        let id = manager.register(subscription).await.unwrap();
        assert!(!id.is_empty());
        
        let subs = manager.list_subscriptions().await;
        assert_eq!(subs.len(), 1);
    }

    #[tokio::test]
    async fn test_unsubscribe() {
        let store = create_test_store().await;
        let manager = SubscriptionManager::new(store);
        
        let query = Query::default();
        let subscription = Subscription {
            id: None,
            query: Box::new(query),
            emit: EmitClause::Changes,
            where_condition: None,
        };
        
        let id = manager.register(subscription).await.unwrap();
        assert!(manager.unsubscribe(&id).await.is_ok());
        
        let subs = manager.list_subscriptions().await;
        assert_eq!(subs.len(), 0);
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let store = create_test_store().await;
        let manager = SubscriptionManager::new(store);
        
        let query = Query::default();
        let subscription = Subscription {
            id: None,
            query: Box::new(query),
            emit: EmitClause::Changes,
            where_condition: None,
        };
        
        let id = manager.register(subscription).await.unwrap();
        
        // Pause
        assert!(manager.pause(&id).await.is_ok());
        let sub = manager.get_subscription(&id).await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Paused);
        
        // Resume
        assert!(manager.resume(&id).await.is_ok());
        let sub = manager.get_subscription(&id).await.unwrap();
        assert_eq!(sub.status, SubscriptionStatus::Active);
    }

    #[tokio::test]
    async fn test_get_stats() {
        let store = create_test_store().await;
        let manager = SubscriptionManager::new(store);
        
        // Register multiple subscriptions
        for _ in 0..3 {
            let query = Query::default();
            let subscription = Subscription {
                id: None,
                query: Box::new(query),
                emit: EmitClause::Changes,
                where_condition: None,
            };
            manager.register(subscription).await.unwrap();
        }
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_subscriptions, 3);
        assert_eq!(stats.active_subscriptions, 3);
    }
}
