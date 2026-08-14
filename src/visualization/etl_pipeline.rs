//! ETL (Extract, Transform, Load) модуль для ToroidalDB
//! 
//! Предоставляет:
//! - Потоковую обработку данных
//! - Преобразование форматов
//! - Интеграцию с внешними источниками данных
//! - Пайплайны обработки

use crate::storage::Node;
use crate::math::MatryoshkaDim;
use crate::topology::edges::{HomotopyClass, InterToroidalEdge, ToroidalLevel};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum DataSource {
    Csv { path: String, delimiter: char },
    Json { path: String },
    Database { connection_string: String },
    Api { url: String, headers: HashMap<String, String> },
    Stream { stream_id: String },
    Memory { data: Vec<Node> },
}

#[derive(Debug, Clone)]
pub struct EtlPipelineConfig {
    pub source: DataSource,
    pub transformations: Vec<Transformation>,
    pub destination: String, // collection name
    pub batch_size: usize,
    pub parallelism: usize,
    pub enable_topology: bool,
    pub topology_level: ToroidalLevel,
}

#[derive(Debug, Clone)]
pub enum Transformation {
    Filter { field: String, operator: FilterOperator, value: Value },
    Map { field: String, function: MapFunction },
    Reduce { field: String, function: ReduceFunction },
    Join { collection: String, join_on: String },
    GroupBy { field: String },
    Sort { field: String, ascending: bool },
    Limit { count: usize },
    AddField { field: String, value: Value },
    RemoveField { field: String },
    RenameField { old_name: String, new_name: String },
}

#[derive(Debug, Clone)]
pub enum FilterOperator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
}

#[derive(Debug, Clone)]
pub enum MapFunction {
    NormalizeVector,
    PadOrTruncate { target_size: usize },
    ComputeHomotopyClass,
    ComputeRicciCurvature,
    ApplyHomotopy,
}

#[derive(Debug, Clone)]
pub enum ReduceFunction {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    Concat,
}

pub struct EtlPipeline {
    store: Arc<crate::storage::PersistentStore>,
    config: EtlPipelineConfig,
    stats: Arc<RwLock<EtlStats>>,
}

#[derive(Debug, Clone)]
pub struct EtlStats {
    pub records_processed: usize,
    pub records_loaded: usize,
    pub records_filtered: usize,
    pub processing_time_ms: u64,
    pub errors: Vec<String>,
}

impl EtlPipeline {
    pub fn new(store: Arc<crate::storage::PersistentStore>, config: EtlPipelineConfig) -> Self {
        Self {
            store,
            config,
            stats: Arc::new(RwLock::new(EtlStats {
                records_processed: 0,
                records_loaded: 0,
                records_filtered: 0,
                processing_time_ms: 0,
                errors: Vec::new(),
            })),
        }
    }

    /// Запускает ETL пайплайн
    pub async fn execute(&self) -> Result<EtlStats, String> {
        let start_time = std::time::Instant::now();
        
        // Извлечение данных
        let mut nodes = self.extract_data().await?;
        
        // Обновляем статистику
        {
            let mut stats = self.stats.write().unwrap();
            stats.records_processed = nodes.len();
        }
        
        // Применение трансформаций
        for transformation in &self.config.transformations {
            nodes = self.apply_transformation(nodes, transformation).await?;
        }
        
        // Загрузка данных
        self.load_data(nodes).await?;
        
        // Обновляем статистику
        {
            let mut stats = self.stats.write().unwrap();
            stats.processing_time_ms = start_time.elapsed().as_millis() as u64;
        }
        
        let final_stats = self.stats.read().unwrap().clone();
        Ok(final_stats)
    }

    /// Извлекает данные из источника
    async fn extract_data(&self) -> Result<Vec<Node>, String> {
        match &self.config.source {
            DataSource::Csv { path, delimiter } => {
                self.extract_from_csv(path, *delimiter).await
            },
            DataSource::Json { path } => {
                self.extract_from_json(path).await
            },
            DataSource::Database { connection_string } => {
                self.extract_from_database(connection_string).await
            },
            DataSource::Api { url, headers } => {
                self.extract_from_api(url, headers).await
            },
            DataSource::Stream { stream_id } => {
                self.extract_from_stream(stream_id).await
            },
            DataSource::Memory { data } => {
                Ok(data.clone())
            },
        }
    }

    /// Извлекает данные из CSV файла
    async fn extract_from_csv(&self, path: &str, delimiter: char) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет чтение CSV
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }

    /// Извлекает данные из JSON файла
    async fn extract_from_json(&self, path: &str) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет чтение JSON
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }

    /// Извлекает данные из базы данных
    async fn extract_from_database(&self, connection_string: &str) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет подключение к базе данных
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }

    /// Извлекает данные из API
    async fn extract_from_api(&self, url: &str, headers: &HashMap<String, String>) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет HTTP-запрос
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }

    /// Извлекает данные из потока
    async fn extract_from_stream(&self, stream_id: &str) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет подключение к потоковому источнику
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }

    /// Применяет трансформацию к данным
    async fn apply_transformation(&self, mut nodes: Vec<Node>, transformation: &Transformation) -> Result<Vec<Node>, String> {
        match transformation {
            Transformation::Filter { field, operator, value } => {
                nodes = self.apply_filter(nodes, field, operator, value).await?;
            },
            Transformation::Map { field, function } => {
                nodes = self.apply_map(nodes, field, function).await?;
            },
            Transformation::Reduce { field, function } => {
                nodes = self.apply_reduce(nodes, field, function).await?;
            },
            Transformation::Join { collection, join_on } => {
                nodes = self.apply_join(nodes, collection, join_on).await?;
            },
            Transformation::GroupBy { field } => {
                nodes = self.apply_group_by(nodes, field).await?;
            },
            Transformation::Sort { field, ascending } => {
                nodes = self.apply_sort(nodes, field, *ascending).await?;
            },
            Transformation::Limit { count } => {
                nodes.truncate(*count);
            },
            Transformation::AddField { field, value } => {
                nodes = self.apply_add_field(nodes, field, value).await?;
            },
            Transformation::RemoveField { field } => {
                nodes = self.apply_remove_field(nodes, field).await?;
            },
            Transformation::RenameField { old_name, new_name } => {
                nodes = self.apply_rename_field(nodes, old_name, new_name).await?;
            },
        }
        
        Ok(nodes)
    }

    /// Применяет фильтрацию к данным
    async fn apply_filter(&self, nodes: Vec<Node>, field: &str, operator: &FilterOperator, value: &Value) -> Result<Vec<Node>, String> {
        let mut filtered_nodes = Vec::new();
        
        for node in nodes {
            let should_include = match operator {
                FilterOperator::Equal => {
                    if let Some(node_value) = node.properties.get(field) {
                        node_value == value
                    } else {
                        false
                    }
                },
                FilterOperator::NotEqual => {
                    if let Some(node_value) = node.properties.get(field) {
                        node_value != value
                    } else {
                        true
                    }
                },
                FilterOperator::GreaterThan => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_f64()), value.as_f64()) {
                        node_val > filter_val
                    } else {
                        false
                    }
                },
                FilterOperator::LessThan => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_f64()), value.as_f64()) {
                        node_val < filter_val
                    } else {
                        false
                    }
                },
                FilterOperator::GreaterThanOrEqual => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_f64()), value.as_f64()) {
                        node_val >= filter_val
                    } else {
                        false
                    }
                },
                FilterOperator::LessThanOrEqual => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_f64()), value.as_f64()) {
                        node_val <= filter_val
                    } else {
                        false
                    }
                },
                FilterOperator::Contains => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_str()), value.as_str()) {
                        node_val.contains(filter_val)
                    } else {
                        false
                    }
                },
                FilterOperator::StartsWith => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_str()), value.as_str()) {
                        node_val.starts_with(filter_val)
                    } else {
                        false
                    }
                },
                FilterOperator::EndsWith => {
                    if let (Some(node_val), Some(filter_val)) = (node.properties.get(field).and_then(|v| v.as_str()), value.as_str()) {
                        node_val.ends_with(filter_val)
                    } else {
                        false
                    }
                },
            };
            
            if should_include {
                filtered_nodes.push(node);
            } else {
                // Обновляем статистику
                let mut stats = self.stats.write().unwrap();
                stats.records_filtered += 1;
            }
        }
        
        Ok(filtered_nodes)
    }

    /// Применяет маппинг к данным
    async fn apply_map(&self, nodes: Vec<Node>, field: &str, function: &MapFunction) -> Result<Vec<Node>, String> {
        let mut mapped_nodes = Vec::new();
        
        for mut node in nodes {
            match function {
                MapFunction::NormalizeVector => {
                    // Нормализуем вектор узла
                    let norm = node.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if norm > 0.0 {
                        node.vector = node.vector.iter().map(|x| x / norm).collect();
                    }
                },
                MapFunction::PadOrTruncate { target_size } => {
                    // Подгоняем размер вектора
                    node.vector = crate::math::pad_or_truncate(&node.vector, *target_size);
                },
                MapFunction::ComputeHomotopyClass => {
                    // Вычисляем гомотопический класс (для топологических операций)
                    // В реальной системе это будет более сложное вычисление
                    if let Some(ref mut props) = node.properties.as_object_mut() {
                        props.insert("homotopy_class".to_string(), Value::String("Direct".to_string()));
                    }
                },
                MapFunction::ComputeRicciCurvature => {
                    // Вычисляем кривизну Риччи (для топологических операций)
                    // В реальной системе это будет более сложное вычисление
                    if let Some(ref mut props) = node.properties.as_object_mut() {
                        props.insert("ricci_curvature".to_string(), Value::Number(0.0.into()));
                    }
                },
                MapFunction::ApplyHomotopy => {
                    // Применяем гомотопическое преобразование
                    // В реальной системе это будет более сложное вычисление
                    if let Some(ref mut props) = node.properties.as_object_mut() {
                        props.insert("transformed".to_string(), Value::Bool(true));
                    }
                },
            }
            
            mapped_nodes.push(node);
        }
        
        Ok(mapped_nodes)
    }

    /// Применяет редьюс к данным
    async fn apply_reduce(&self, nodes: Vec<Node>, field: &str, function: &ReduceFunction) -> Result<Vec<Node>, String> {
        match function {
            ReduceFunction::Sum => {
                // Суммируем значения поля
                let sum: f64 = nodes.iter()
                    .filter_map(|node| node.properties.get(field).and_then(|v| v.as_f64()))
                    .sum();
                
                // Возвращаем один узел с результатом
                Ok(vec![Node {
                    id: 0,
                    vector: vec![],
                    properties: serde_json::json!({ field: sum }),
                    edges: vec![],
                }])
            },
            ReduceFunction::Avg => {
                // Вычисляем среднее значение поля
                let values: Vec<f64> = nodes.iter()
                    .filter_map(|node| node.properties.get(field).and_then(|v| v.as_f64()))
                    .collect();
                
                if !values.is_empty() {
                    let avg = values.iter().sum::<f64>() / values.len() as f64;
                    
                    Ok(vec![Node {
                        id: 0,
                        vector: vec![],
                        properties: serde_json::json!({ field: avg }),
                        edges: vec![],
                    }])
                } else {
                    Ok(vec![])
                }
            },
            ReduceFunction::Min => {
                // Находим минимальное значение поля
                let min_val = nodes.iter()
                    .filter_map(|node| node.properties.get(field).and_then(|v| v.as_f64()))
                    .fold(f64::INFINITY, |a, b| a.min(b));
                
                if min_val.is_finite() {
                    Ok(vec![Node {
                        id: 0,
                        vector: vec![],
                        properties: serde_json::json!({ field: min_val }),
                        edges: vec![],
                    }])
                } else {
                    Ok(vec![])
                }
            },
            ReduceFunction::Max => {
                // Находим максимальное значение поля
                let max_val = nodes.iter()
                    .filter_map(|node| node.properties.get(field).and_then(|v| v.as_f64()))
                    .fold(f64::NEG_INFINITY, |a, b| a.max(b));
                
                if max_val.is_finite() {
                    Ok(vec![Node {
                        id: 0,
                        vector: vec![],
                        properties: serde_json::json!({ field: max_val }),
                        edges: vec![],
                    }])
                } else {
                    Ok(vec![])
                }
            },
            ReduceFunction::Count => {
                // Подсчитываем количество узлов
                let count = nodes.len() as f64;
                
                Ok(vec![Node {
                    id: 0,
                    vector: vec![],
                    properties: serde_json::json!({ "count": count }),
                    edges: vec![],
                }])
            },
            ReduceFunction::Concat => {
                // Конкатенируем значения поля
                let concatenated: String = nodes.iter()
                    .filter_map(|node| node.properties.get(field).and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(",");
                
                Ok(vec![Node {
                    id: 0,
                    vector: vec![],
                    properties: serde_json::json!({ field: concatenated }),
                    edges: vec![],
                }])
            },
        }
    }

    /// Применяет джойн к данным
    async fn apply_join(&self, nodes: Vec<Node>, collection: &str, join_on: &str) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет джойн с другой коллекцией
        // Пока возвращаем исходные узлы без изменений
        Ok(nodes)
    }

    /// Применяет группировку к данным
    async fn apply_group_by(&self, nodes: Vec<Node>, field: &str) -> Result<Vec<Node>, String> {
        // Группируем узлы по значению поля
        let mut groups: HashMap<String, Vec<Node>> = HashMap::new();
        
        for node in nodes {
            let key = node.properties.get(field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            
            groups.entry(key).or_insert_with(Vec::new).push(node);
        }
        
        // Преобразуем группы в узлы
        let mut grouped_nodes = Vec::new();
        for (key, group) in groups {
            let mut properties = serde_json::json!({});
            if let Some(obj) = properties.as_object_mut() {
                obj.insert("group_key".to_string(), Value::String(key));
                obj.insert("group_size".to_string(), Value::Number((group.len() as u64).into()));
            }
            
            grouped_nodes.push(Node {
                id: 0, // ID не применим для групп
                vector: vec![], // Вектор не применим для групп
                properties,
                edges: vec![],
            });
        }
        
        Ok(grouped_nodes)
    }

    /// Применяет сортировку к данным
    async fn apply_sort(&self, mut nodes: Vec<Node>, field: &str, ascending: bool) -> Result<Vec<Node>, String> {
        nodes.sort_by(|a, b| {
            let val_a = a.properties.get(field).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let val_b = b.properties.get(field).and_then(|v| v.as_f64()).unwrap_or(0.0);
            
            if ascending {
                val_a.partial_cmp(&val_b).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                val_b.partial_cmp(&val_a).unwrap_or(std::cmp::Ordering::Equal)
            }
        });
        
        Ok(nodes)
    }

    /// Добавляет поле к узлам
    async fn apply_add_field(&self, mut nodes: Vec<Node>, field: &str, value: &Value) -> Result<Vec<Node>, String> {
        for node in &mut nodes {
            if let Some(ref mut props) = node.properties.as_object_mut() {
                props.insert(field.to_string(), value.clone());
            }
        }
        
        Ok(nodes)
    }

    /// Удаляет поле из узлов
    async fn apply_remove_field(&self, mut nodes: Vec<Node>, field: &str) -> Result<Vec<Node>, String> {
        for node in &mut nodes {
            if let Some(ref mut props) = node.properties.as_object_mut() {
                props.remove(field);
            }
        }
        
        Ok(nodes)
    }

    /// Переименовывает поле в узлах
    async fn apply_rename_field(&self, mut nodes: Vec<Node>, old_name: &str, new_name: &str) -> Result<Vec<Node>, String> {
        for node in &mut nodes {
            if let Some(ref mut props) = node.properties.as_object_mut() {
                if let Some(value) = props.remove(old_name) {
                    props.insert(new_name.to_string(), value);
                }
            }
        }
        
        Ok(nodes)
    }

    /// Загружает данные в хранилище
    async fn load_data(&self, nodes: Vec<Node>) -> Result<(), String> {
        for node in nodes {
            self.store.insert(node).map_err(|e| e.to_string())?;
        }
        
        // Обновляем статистику
        {
            let mut stats = self.stats.write().unwrap();
            stats.records_loaded = stats.records_processed - stats.records_filtered;
        }
        
        Ok(())
    }

    /// Создает потоковую обработку данных
    pub fn create_stream_processor(&self) -> StreamProcessor {
        StreamProcessor::new(self.store.clone())
    }

    /// Получает статистику выполнения
    pub fn get_stats(&self) -> EtlStats {
        self.stats.read().unwrap().clone()
    }
}

/// Обработчик потоковых данных
pub struct StreamProcessor {
    store: Arc<crate::storage::PersistentStore>,
    tx: Option<mpsc::UnboundedSender<Node>>,
    rx: Option<mpsc::UnboundedReceiver<Node>>,
}

impl StreamProcessor {
    pub fn new(store: Arc<crate::storage::PersistentStore>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        
        Self {
            store,
            tx: Some(tx),
            rx: Some(rx),
        }
    }

    /// Запускает обработку потока данных
    pub async fn start_processing(&mut self) -> Result<(), String> {
        let store = self.store.clone();
        let mut rx = self.rx.take().unwrap();
        
        // Запускаем фоновую задачу для обработки потока
        tokio::spawn(async move {
            while let Some(node) = rx.recv().await {
                // Применяем топологические преобразования к потоковым данным
                let processed_node = Self::process_node_topology(&node);
                
                // Сохраняем в хранилище
                if let Err(e) = store.insert(processed_node) {
                    eprintln!("Error inserting node: {}", e);
                }
            }
        });
        
        Ok(())
    }

    /// Обрабатывает узел с топологическими преобразованиями
    fn process_node_topology(node: &Node) -> Node {
        let mut processed_node = node.clone();
        
        // Применяем топологические преобразования
        if processed_node.vector.len() > 0 {
            // Нормализуем вектор
            let norm = processed_node.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                processed_node.vector = processed_node.vector.iter().map(|x| x / norm).collect();
            }
            
            // Добавляем топологические метаданные
            if let Some(ref mut props) = processed_node.properties.as_object_mut() {
                props.insert("processed_by_topology".to_string(), Value::Bool(true));
                props.insert("vector_norm".to_string(), Value::Number(norm.into()));
            }
        }
        
        processed_node
    }

    /// Отправляет узел в поток
    pub fn send_node(&self, node: Node) -> Result<(), String> {
        if let Some(ref tx) = self.tx {
            tx.send(node).map_err(|e| e.to_string())
        } else {
            Err("Stream processor not initialized".to_string())
        }
    }

    /// Закрывает поток
    pub fn close_stream(&mut self) {
        if let Some(tx) = self.tx.take() {
            drop(tx); // Закрываем отправитель
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_storage::HybridPersistentStore;
    use serde_json::json;

    #[tokio::test]
    async fn test_etl_pipeline_creation() {
        let store = Arc::new(HybridPersistentStore::open("./test_etl_data").unwrap());
        let config = EtlPipelineConfig {
            source: DataSource::Memory { data: vec![] },
            transformations: vec![],
            destination: "test_collection".to_string(),
            batch_size: 100,
            parallelism: 4,
            enable_topology: true,
            topology_level: ToroidalLevel::D384,
        };

        let pipeline = EtlPipeline::new(store, config);
        let stats = pipeline.get_stats();
        
        assert_eq!(stats.records_processed, 0);
        assert_eq!(stats.records_loaded, 0);
        assert_eq!(stats.records_filtered, 0);
        assert_eq!(stats.processing_time_ms, 0);
        assert!(stats.errors.is_empty());
    }

    #[tokio::test]
    async fn test_filter_transformation() {
        let store = Arc::new(HybridPersistentStore::open("./test_etl_data2").unwrap());
        let config = EtlPipelineConfig {
            source: DataSource::Memory { 
                data: vec![
                    Node {
                        id: 1,
                        vector: vec![0.1, 0.2],
                        properties: json!({"score": 0.8, "active": true}),
                        edges: vec![],
                    },
                    Node {
                        id: 2,
                        vector: vec![0.3, 0.4],
                        properties: json!({"score": 0.5, "active": false}),
                        edges: vec![],
                    },
                    Node {
                        id: 3,
                        vector: vec![0.5, 0.6],
                        properties: json!({"score": 0.9, "active": true}),
                        edges: vec![],
                    },
                ]
            },
            transformations: vec![
                Transformation::Filter { 
                    field: "score".to_string(), 
                    operator: FilterOperator::GreaterThan, 
                    value: Value::Number(0.6.into()) 
                }
            ],
            destination: "filtered_data".to_string(),
            batch_size: 100,
            parallelism: 1,
            enable_topology: false,
            topology_level: ToroidalLevel::D384,
        };

        let pipeline = EtlPipeline::new(store, config);
        let result = pipeline.execute().await;
        
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.records_loaded, 2); // Только узлы с ID 1 и 3 должны пройти фильтр
        assert_eq!(stats.records_filtered, 1); // Один узел должен быть отфильтрован
    }

    #[tokio::test]
    async fn test_map_transformation() {
        let store = Arc::new(HybridPersistentStore::open("./test_etl_data3").unwrap());
        let config = EtlPipelineConfig {
            source: DataSource::Memory { 
                data: vec![
                    Node {
                        id: 1,
                        vector: vec![3.0, 4.0], // норма = 5
                        properties: json!({"name": "Node1"}),
                        edges: vec![],
                    },
                ]
            },
            transformations: vec![
                Transformation::Map { 
                    field: "vector".to_string(), 
                    function: MapFunction::NormalizeVector 
                }
            ],
            destination: "normalized_data".to_string(),
            batch_size: 100,
            parallelism: 1,
            enable_topology: false,
            topology_level: ToroidalLevel::D384,
        };

        let pipeline = EtlPipeline::new(store, config);
        let result = pipeline.execute().await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_reduce_transformation() {
        let store = Arc::new(HybridPersistentStore::open("./test_etl_data4").unwrap());
        let config = EtlPipelineConfig {
            source: DataSource::Memory { 
                data: vec![
                    Node {
                        id: 1,
                        vector: vec![0.1, 0.1],
                        properties: json!({"value": 10.0}),
                        edges: vec![],
                    },
                    Node {
                        id: 2,
                        vector: vec![0.2, 0.2],
                        properties: json!({"value": 20.0}),
                        edges: vec![],
                    },
                    Node {
                        id: 3,
                        vector: vec![0.3, 0.3],
                        properties: json!({"value": 30.0}),
                        edges: vec![],
                    },
                ]
            },
            transformations: vec![
                Transformation::Reduce { 
                    field: "value".to_string(), 
                    function: ReduceFunction::Sum 
                }
            ],
            destination: "reduced_data".to_string(),
            batch_size: 100,
            parallelism: 1,
            enable_topology: false,
            topology_level: ToroidalLevel::D384,
        };

        let pipeline = EtlPipeline::new(store, config);
        let result = pipeline.execute().await;
        
        assert!(result.is_ok());
        // В результате должен быть один узел с суммой значений (60.0)
    }

    #[test]
    fn test_stream_processor() {
        let store = Arc::new(HybridPersistentStore::open("./test_etl_data5").unwrap());
        let mut processor = StreamProcessor::new(store);
        
        assert!(processor.start_processing().await.is_ok());
        
        // Отправляем тестовый узел
        let test_node = Node {
            id: 1,
            vector: vec![0.5, 0.5],
            properties: json!({"name": "Stream Test"}),
            edges: vec![],
        };
        
        assert!(processor.send_node(test_node).is_ok());
        
        processor.close_stream();
    }
}