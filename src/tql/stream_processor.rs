//! # Stream Processing для TQL v3.0 - Production Ready
//!
//! Обработка потоков данных с оконными функциями, watermark и CDC

use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::ast::*;
use crate::tql::executor::QueryExecutor;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};

/// Stream Processor
pub struct StreamProcessor {
    streams: Arc<RwLock<HashMap<String, StreamState>>>,
    event_tx: broadcast::Sender<StreamEvent>,
    store: Arc<HybridPersistentStore>,
}

/// Состояние стрима
#[derive(Debug, Clone)]
pub struct StreamState {
    pub name: String,
    pub topic: String,
    pub schema: StreamSchema,
    pub buffer: VecDeque<StreamRecord>,
    pub windows: HashMap<String, WindowState>,
    pub watermarks: HashMap<String, u64>,
    pub config: StreamConfig,
}

/// Конфигурация стрима
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub max_buffer_size: usize,
    pub retention_ms: u64,
    pub enable_watermark: bool,
    pub allowed_lateness_ms: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 10000,
            retention_ms: 3600000, // 1 hour
            enable_watermark: true,
            allowed_lateness_ms: 60000, // 1 minute
        }
    }
}

/// Запись в стриме
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    pub id: String,
    pub data: serde_json::Value,
    pub timestamp: u64,
    pub event_time: Option<u64>,
    pub watermark: Option<u64>,
}

/// Событие стрима
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "record")]
    Record {
        stream_name: String,
        record: StreamRecord,
    },
    #[serde(rename = "window_trigger")]
    WindowTrigger {
        stream_name: String,
        window_id: String,
        results: Vec<serde_json::Value>,
    },
    #[serde(rename = "watermark")]
    Watermark { stream_name: String, watermark: u64 },
    #[serde(rename = "cdc")]
    ChangeDataCapture {
        table: String,
        operation: String,
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    },
}

/// Состояние окна
#[derive(Debug, Clone)]
pub struct WindowState {
    pub window_type: WindowType,
    pub records: Vec<StreamRecord>,
    pub start_time: u64,
    pub end_time: u64,
    pub is_triggered: bool,
    pub last_trigger_time: Option<u64>,
}

/// Тип окна
#[derive(Debug, Clone)]
pub enum WindowType {
    Tumbling { duration_ms: u64 },
    Sliding { duration_ms: u64, slide_ms: u64 },
    Session { gap_ms: u64 },
}

impl StreamProcessor {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        let (event_tx, _) = broadcast::channel(10000);

        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            store,
        }
    }

    /// Регистрирует стрим
    pub async fn register_stream(&self, stream_def: StreamDef) -> Result<(), String> {
        let state = StreamState {
            name: stream_def.name.clone(),
            topic: stream_def.topic,
            schema: stream_def.schema,
            buffer: VecDeque::with_capacity(1000),
            windows: HashMap::new(),
            watermarks: HashMap::new(),
            config: StreamConfig::default(),
        };

        let mut streams = self.streams.write().await;
        streams.insert(stream_def.name, state);

        Ok(())
    }

    /// Добавляет запись в стрим
    pub async fn ingest(&self, stream_name: &str, record: StreamRecord) -> Result<(), String> {
        let mut streams = self.streams.write().await;

        let stream = streams
            .get_mut(stream_name)
            .ok_or_else(|| format!("Stream '{}' not found", stream_name))?;

        // Добавляем в буфер
        stream.buffer.push_back(record.clone());

        // Очищаем старые записи
        self.cleanup_old_records(stream).await;

        // Обновляем watermark
        if let Some(event_time) = record.event_time {
            self.update_watermark(stream_name, event_time).await?;
        }

        // Отправляем событие
        let event = StreamEvent::Record {
            stream_name: stream_name.to_string(),
            record,
        };
        self.event_tx.send(event).map_err(|e| e.to_string())?;

        // Проверяем окна
        self.check_windows(stream_name, stream).await?;

        Ok(())
    }

    /// Обрабатывает запрос к стриму
    pub async fn process_stream_query(
        &self,
        query: &Query,
    ) -> Result<Vec<serde_json::Value>, String> {
        let stream_query = query
            .from_stream
            .as_ref()
            .ok_or("No FROM STREAM clause in query")?;

        let streams = self.streams.read().await;
        let stream = streams
            .get(&stream_query.stream_name)
            .ok_or_else(|| format!("Stream '{}' not found", stream_query.stream_name))?;

        // Получаем записи из окна
        let records = if let Some(window) = &stream_query.window {
            self.get_window_records(stream, window).await?
        } else {
            stream.buffer.iter().cloned().collect()
        };

        // Применяем фильтры
        let filtered = self.apply_filters(&records, &stream_query.filter)?;

        // Применяем GROUP BY
        let grouped = if !query.group_by.is_empty() {
            self.apply_group_by(&filtered, &query.group_by, &query.aggregation_fields)?
        } else {
            self.records_to_values(&filtered)
        };

        // Применяем HAVING
        let having_filtered = if let Some(having) = &query.having {
            self.apply_having(&grouped, having)?
        } else {
            grouped
        };

        // Сортируем
        let mut sorted = having_filtered;
        if let Some(order_by) = &query.order_by {
            self.sort_results(&mut sorted, order_by);
        }

        // Применяем LIMIT
        sorted.truncate(query.limit as usize);

        Ok(sorted)
    }

    /// Получает записи из окна
    async fn get_window_records(
        &self,
        stream: &StreamState,
        window_spec: &StreamWindowSpec,
    ) -> Result<Vec<StreamRecord>, String> {
        let now = get_timestamp_ms();
        let duration_ms =
            window_spec.duration.value * self.duration_multiplier(&window_spec.duration.unit);

        let window_start = now - duration_ms;

        let records: Vec<StreamRecord> = stream
            .buffer
            .iter()
            .filter(|r| {
                let record_time = r.event_time.unwrap_or(r.timestamp);
                record_time >= window_start && record_time <= now
            })
            .cloned()
            .collect();

        Ok(records)
    }

    /// Применяет фильтры
    fn apply_filters(
        &self,
        records: &[StreamRecord],
        filter: &Option<WhereCondition>,
    ) -> Result<Vec<StreamRecord>, String> {
        let Some(where_clause) = filter else {
            return Ok(records.to_vec());
        };

        let filtered = records
            .iter()
            .filter(|record| self.evaluate_filter(where_clause, record))
            .cloned()
            .collect();

        Ok(filtered)
    }

    /// Оценивает фильтр для записи
    fn evaluate_filter(&self, filter: &WhereCondition, record: &StreamRecord) -> bool {
        match filter {
            WhereCondition::PropertyFilter {
                property,
                operator,
                value,
            } => {
                if let Some(prop_value) = record.data.get(property) {
                    return self.compare_property(prop_value, operator, value);
                }
                false
            }
            WhereCondition::ToroidalDistance { .. } => {
                // Vector search not supported in streams
                false
            }
            WhereCondition::SimilarTo { .. } => {
                // SimilarTo search not supported in streams
                false
            }
        }
    }

    /// Сравнивает свойства
    fn compare_property(
        &self,
        prop_value: &serde_json::Value,
        operator: &str,
        value: &PropertyValue,
    ) -> bool {
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
                        "=" | "==" => prop_num == *n,
                        "!=" | "<>" => prop_num != *n,
                        ">" => prop_num > *n,
                        "<" => prop_num < *n,
                        ">=" => prop_num >= *n,
                        "<=" => prop_num <= *n,
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

    /// Применяет GROUP BY
    fn apply_group_by(
        &self,
        records: &[StreamRecord],
        group_by: &[String],
        aggregations: &[AggregationField],
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut groups: HashMap<String, Vec<&StreamRecord>> = HashMap::new();

        // Группируем записи
        for record in records {
            let key = self.create_group_key(record, group_by);
            groups.entry(key).or_insert_with(Vec::new).push(record);
        }

        // Вычисляем агрегации для каждой группы
        let mut results = Vec::new();
        for (key, group_records) in groups {
            let mut row = serde_json::Map::new();

            // Добавляем ключи группы
            for (i, field) in group_by.iter().enumerate() {
                if let Some(first) = group_records.first() {
                    if let Some(value) = first.data.get(field) {
                        row.insert(field.clone(), value.clone());
                    }
                }
            }

            // Вычисляем агрегации
            for agg in aggregations {
                let value = self.compute_aggregation(&group_records, &agg.function);
                let alias = agg
                    .alias
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", agg.function));
                row.insert(alias, value);
            }

            results.push(serde_json::Value::Object(row));
        }

        Ok(results)
    }

    /// Создаёт ключ группы
    fn create_group_key(&self, record: &StreamRecord, fields: &[String]) -> String {
        let values: Vec<String> = fields
            .iter()
            .filter_map(|field| record.data.get(field).map(|v| v.to_string()))
            .collect();
        values.join("|")
    }

    /// Вычисляет агрегацию
    fn compute_aggregation(
        &self,
        records: &[&StreamRecord],
        function: &AggregationFunction,
    ) -> serde_json::Value {
        match function {
            AggregationFunction::Count => {
                serde_json::json!(records.len())
            }
            AggregationFunction::Sum(field) => {
                let sum: f64 = records
                    .iter()
                    .filter_map(|r| r.data.get(field).and_then(|v| v.as_f64()))
                    .sum();
                serde_json::json!(sum)
            }
            AggregationFunction::Avg(field) => {
                let values: Vec<f64> = records
                    .iter()
                    .filter_map(|r| r.data.get(field).and_then(|v| v.as_f64()))
                    .collect();
                if values.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(values.iter().sum::<f64>() / values.len() as f64)
                }
            }
            AggregationFunction::Min(field) => {
                let min = records
                    .iter()
                    .filter_map(|r| r.data.get(field).and_then(|v| v.as_f64()))
                    .fold(f64::INFINITY, |a, b| a.min(b));
                if min == f64::INFINITY {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(min)
                }
            }
            AggregationFunction::Max(field) => {
                let max = records
                    .iter()
                    .filter_map(|r| r.data.get(field).and_then(|v| v.as_f64()))
                    .fold(f64::NEG_INFINITY, |a, b| a.max(b));
                if max == f64::NEG_INFINITY {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(max)
                }
            }
        }
    }

    /// Применяет HAVING
    fn apply_having(
        &self,
        rows: &[serde_json::Value],
        having: &WhereCondition,
    ) -> Result<Vec<serde_json::Value>, String> {
        let filtered = rows
            .iter()
            .filter(|row| self.evaluate_having(row, having))
            .cloned()
            .collect();

        Ok(filtered)
    }

    /// Оценивает HAVING для строки
    fn evaluate_having(&self, row: &serde_json::Value, having: &WhereCondition) -> bool {
        match having {
            WhereCondition::PropertyFilter {
                property,
                operator,
                value,
            } => {
                if let Some(prop_value) = row.get(property) {
                    return self.compare_property(prop_value, operator, value);
                }
                false
            }
            _ => false,
        }
    }

    /// Сортирует результаты
    fn sort_results(&self, results: &mut Vec<serde_json::Value>, order_by: &OrderByClause) {
        results.sort_by(|a, b| {
            let val_a = a
                .get(&order_by.field)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let val_b = b
                .get(&order_by.field)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            if order_by.ascending {
                val_a
                    .partial_cmp(&val_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                val_b
                    .partial_cmp(&val_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        });
    }

    /// Конвертирует записи в значения
    fn records_to_values(&self, records: &[StreamRecord]) -> Vec<serde_json::Value> {
        records.iter().map(|r| r.data.clone()).collect()
    }

    /// Проверяет окна
    async fn check_windows(
        &self,
        stream_name: &str,
        stream: &mut StreamState,
    ) -> Result<(), String> {
        // Проверяем tumbling windows
        for (window_id, window) in stream.windows.iter_mut() {
            if !window.is_triggered && self.should_trigger_window(window) {
                // Trigger window
                let results = self.trigger_window(window).await?;

                // Отправляем событие
                let event = StreamEvent::WindowTrigger {
                    stream_name: stream_name.to_string(),
                    window_id: window_id.clone(),
                    results,
                };
                self.event_tx.send(event).map_err(|e| e.to_string())?;

                window.is_triggered = true;
                window.last_trigger_time = Some(get_timestamp_ms());
            }
        }

        Ok(())
    }

    /// Проверяет, должно ли окно сработать
    fn should_trigger_window(&self, window: &WindowState) -> bool {
        let now = get_timestamp_ms();
        now >= window.end_time
    }

    /// Срабатывание окна
    async fn trigger_window(&self, window: &WindowState) -> Result<Vec<serde_json::Value>, String> {
        let results = window.records.iter().map(|r| r.data.clone()).collect();

        Ok(results)
    }

    /// Обновляет watermark
    async fn update_watermark(&self, stream_name: &str, event_time: u64) -> Result<(), String> {
        let mut streams = self.streams.write().await;

        if let Some(stream) = streams.get_mut(stream_name) {
            let current_watermark = stream.watermarks.entry("default".to_string()).or_insert(0);

            if event_time > *current_watermark {
                *current_watermark = event_time;

                // Отправляем событие watermark
                let event = StreamEvent::Watermark {
                    stream_name: stream_name.to_string(),
                    watermark: event_time,
                };
                self.event_tx.send(event).map_err(|e| e.to_string())?;
            }
        }

        Ok(())
    }

    /// Очищает старые записи
    async fn cleanup_old_records(&self, stream: &mut StreamState) {
        let now = get_timestamp_ms();
        let retention_ms = stream.config.retention_ms;

        while let Some(front) = stream.buffer.front() {
            let record_time = front.event_time.unwrap_or(front.timestamp);
            if now - record_time > retention_ms {
                stream.buffer.pop_front();
            } else {
                break;
            }
        }
    }

    /// Умножитель длительности
    fn duration_multiplier(&self, unit: &str) -> u64 {
        match unit {
            "s" => 1000,
            "m" => 60000,
            "h" => 3600000,
            "d" => 86400000,
            _ => 1000,
        }
    }

    /// Создаёт CDC событие
    pub async fn emit_cdc(
        &self,
        table: &str,
        operation: &str,
        before: Option<serde_json::Value>,
        after: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let event = StreamEvent::ChangeDataCapture {
            table: table.to_string(),
            operation: operation.to_string(),
            before,
            after,
        };

        self.event_tx.send(event).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Подписывается на события стрима
    pub async fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_tx.subscribe()
    }

    /// Получает статистику стрима
    pub async fn get_stream_stats(&self, stream_name: &str) -> Option<StreamStats> {
        let streams = self.streams.read().await;
        let stream = streams.get(stream_name)?;

        Some(StreamStats {
            name: stream.name.clone(),
            record_count: stream.buffer.len(),
            window_count: stream.windows.len(),
            watermark: stream.watermarks.get("default").copied().unwrap_or(0),
        })
    }
}

/// Статистика стрима
#[derive(Debug, Clone)]
pub struct StreamStats {
    pub name: String,
    pub record_count: usize,
    pub window_count: usize,
    pub watermark: u64,
}

fn get_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
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
    async fn test_register_stream() {
        let store = create_test_store().await;
        let processor = StreamProcessor::new(store);

        let stream_def = StreamDef {
            name: "test_stream".to_string(),
            topic: "test.topic".to_string(),
            schema: StreamSchema { fields: vec![] },
            retention: None,
        };

        assert!(processor.register_stream(stream_def).await.is_ok());
    }

    #[tokio::test]
    async fn test_ingest_record() {
        let store = create_test_store().await;
        let processor = StreamProcessor::new(store);

        // Register stream
        let stream_def = StreamDef {
            name: "test_stream".to_string(),
            topic: "test.topic".to_string(),
            schema: StreamSchema { fields: vec![] },
            retention: None,
        };
        processor.register_stream(stream_def).await.unwrap();

        // Ingest record
        let record = StreamRecord {
            id: "1".to_string(),
            data: json!({"value": 42}),
            timestamp: get_timestamp_ms(),
            event_time: None,
            watermark: None,
        };

        assert!(processor.ingest("test_stream", record).await.is_ok());
    }

    #[tokio::test]
    async fn test_stream_query() {
        let store = create_test_store().await;
        let processor = StreamProcessor::new(store);

        // Register and ingest
        let stream_def = StreamDef {
            name: "clicks".to_string(),
            topic: "clicks.topic".to_string(),
            schema: StreamSchema { fields: vec![] },
            retention: None,
        };
        processor.register_stream(stream_def).await.unwrap();

        for i in 0..5 {
            let record = StreamRecord {
                id: format!("{}", i),
                data: json!({"user_id": 1, "value": i * 10}),
                timestamp: get_timestamp_ms(),
                event_time: None,
                watermark: None,
            };
            processor.ingest("clicks", record).await.unwrap();
        }

        // Query
        let mut query = Query::default();
        query.from_stream = Some(StreamQuery {
            stream_name: "clicks".to_string(),
            window: None,
            filter: None,
        });
        query.limit = 10;

        let results = processor.process_stream_query(&query).await.unwrap();
        assert_eq!(results.len(), 5);
    }
}
