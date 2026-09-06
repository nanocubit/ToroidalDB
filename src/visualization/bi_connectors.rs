//! Коннекторы для BI-инструментов
//!
//! Предоставляет интерфейсы для подключения к популярным BI-инструментам:
//! - Tableau
//! - `PowerBI`
//! - Grafana
//! - Metabase
//! - Apache Superset

use crate::storage::Node;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum BiConnectorType {
    Tableau,
    PowerBI,
    Grafana,
    Metabase,
    ApacheSuperset,
    Custom,
}

#[derive(Debug, Clone)]
pub struct BiConnectorConfig {
    pub connector_type: BiConnectorType,
    pub connection_string: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
    pub custom_headers: HashMap<String, String>,
}

pub struct BiConnector {
    store: Arc<crate::storage::PersistentStore>,
    config: BiConnectorConfig,
}

impl BiConnector {
    pub fn new(store: Arc<crate::storage::PersistentStore>, config: BiConnectorConfig) -> Self {
        Self { store, config }
    }

    /// Подключается к BI-инструменту
    pub fn connect(&self) -> Result<BiConnection, String> {
        match self.config.connector_type {
            BiConnectorType::Tableau => self.connect_to_tableau(),
            BiConnectorType::PowerBI => self.connect_to_powerbi(),
            BiConnectorType::Grafana => self.connect_to_grafana(),
            BiConnectorType::Metabase => self.connect_to_metabase(),
            BiConnectorType::ApacheSuperset => self.connect_to_superset(),
            BiConnectorType::Custom => self.connect_custom(),
        }
    }

    /// Подключение к Tableau
    fn connect_to_tableau(&self) -> Result<BiConnection, String> {
        // В реальной системе здесь будет подключение к Tableau
        // Пока возвращаем заглушку
        Ok(BiConnection {
            connector_type: BiConnectorType::Tableau,
            connected: true,
            connection_info: format!("Connected to Tableau at: {}", self.config.connection_string),
        })
    }

    /// Подключение к `PowerBI`
    fn connect_to_powerbi(&self) -> Result<BiConnection, String> {
        // В реальной системе здесь будет подключение к PowerBI
        Ok(BiConnection {
            connector_type: BiConnectorType::PowerBI,
            connected: true,
            connection_info: format!("Connected to PowerBI at: {}", self.config.connection_string),
        })
    }

    /// Подключение к Grafana
    fn connect_to_grafana(&self) -> Result<BiConnection, String> {
        // В реальной системе здесь будет подключение к Grafana
        Ok(BiConnection {
            connector_type: BiConnectorType::Grafana,
            connected: true,
            connection_info: format!("Connected to Grafana at: {}", self.config.connection_string),
        })
    }

    /// Подключение к Metabase
    fn connect_to_metabase(&self) -> Result<BiConnection, String> {
        // В реальной системе здесь будет подключение к Metabase
        Ok(BiConnection {
            connector_type: BiConnectorType::Metabase,
            connected: true,
            connection_info: format!(
                "Connected to Metabase at: {}",
                self.config.connection_string
            ),
        })
    }

    /// Подключение к Apache Superset
    fn connect_to_superset(&self) -> Result<BiConnection, String> {
        // В реальной системе здесь будет подключение к Superset
        Ok(BiConnection {
            connector_type: BiConnectorType::ApacheSuperset,
            connected: true,
            connection_info: format!(
                "Connected to Superset at: {}",
                self.config.connection_string
            ),
        })
    }

    /// Подключение к кастомному BI-инструменту
    fn connect_custom(&self) -> Result<BiConnection, String> {
        // В реальной системе здесь будет подключение к кастомному инструменту
        Ok(BiConnection {
            connector_type: BiConnectorType::Custom,
            connected: true,
            connection_info: format!(
                "Connected to custom BI at: {}",
                self.config.connection_string
            ),
        })
    }

    /// Экспортирует данные в формате, подходящем для BI-инструментов
    pub fn export_for_bi(&self, query: &str) -> Result<BiExportData, String> {
        // Выполняем запрос к базе данных
        let nodes = self.execute_query(query)?;

        // Преобразуем в формат, подходящий для BI-инструментов
        let bi_data = self.convert_to_bi_format(nodes)?;

        Ok(bi_data)
    }

    /// Выполняет запрос и возвращает результаты
    fn execute_query(&self, _query: &str) -> Result<Vec<Node>, String> {
        // В реальной системе здесь будет выполнение TQL-запроса
        // Пока возвращаем все узлы
        self.store.get_all().map_err(|e| e.to_string())
    }

    /// Преобразует узлы в формат, подходящий для BI-инструментов
    fn convert_to_bi_format(&self, nodes: Vec<Node>) -> Result<BiExportData, String> {
        let mut columns = Vec::new();
        let mut rows = Vec::new();

        if !nodes.is_empty() {
            // Определяем колонки на основе свойств первого узла
            if let Some(first_node_props) = nodes[0].properties.as_object() {
                for key in first_node_props.keys() {
                    columns.push(key.clone());
                }
            }

            // Добавляем стандартные колонки
            columns.push("id".to_string());
            columns.push("vector_length".to_string());
            columns.push("edges_count".to_string());
        }

        // Преобразуем узлы в строки данных
        for node in nodes {
            let mut row = Vec::new();

            // Добавляем значения свойств
            if let Some(props) = node.properties.as_object() {
                for col in &columns {
                    if col == "id" {
                        row.push(Value::Number(node.id.into()));
                    } else if col == "vector_length" {
                        row.push(Value::Number((node.vector.len() as u64).into()));
                    } else if col == "edges_count" {
                        row.push(Value::Number((node.edges.len() as u64).into()));
                    } else if let Some(val) = props.get(col) {
                        row.push(val.clone());
                    } else {
                        row.push(Value::Null);
                    }
                }
            }

            rows.push(row);
        }

        let node_count = rows.len();

        Ok(BiExportData {
            columns,
            rows,
            metadata: BiMetadata {
                node_count,
                export_timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                format_version: "1.0".to_string(),
            },
        })
    }

    /// Экспортирует данные в формат CSV
    pub fn export_csv(&self, query: &str) -> Result<String, String> {
        let bi_data = self.export_for_bi(query)?;

        let mut csv = String::new();

        // Добавляем заголовки
        for (i, col) in bi_data.columns.iter().enumerate() {
            if i > 0 {
                csv.push(',');
            }
            csv.push_str(col);
        }
        csv.push('\n');

        // Добавляем строки данных
        for row in bi_data.rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    csv.push(',');
                }

                match cell {
                    Value::String(s) => {
                        // Экранируем кавычки и добавляем кавычки
                        let escaped = s.replace('"', "\"\"");
                        csv.push_str(&format!("\"{escaped}\""));
                    }
                    Value::Number(n) => {
                        csv.push_str(&n.to_string());
                    }
                    Value::Bool(b) => {
                        csv.push_str(if *b { "true" } else { "false" });
                    }
                    Value::Null => {
                        csv.push_str("");
                    }
                    _ => {
                        // Для других типов конвертируем в строку
                        csv.push_str(&cell.to_string());
                    }
                }
            }
            csv.push('\n');
        }

        Ok(csv)
    }

    /// Экспортирует данные в формат JSON
    pub fn export_json(&self, query: &str) -> Result<String, String> {
        let bi_data = self.export_for_bi(query)?;
        serde_json::to_string(&bi_data).map_err(|e| format!("Failed to serialize BI data: {e}"))
    }

    /// Экспортирует данные в формат Parquet (для больших объемов)
    pub fn export_parquet(&self, query: &str) -> Result<Vec<u8>, String> {
        // В реальной системе здесь будет экспорт в Parquet
        // Пока возвращаем JSON как байты
        let json_data = self.export_json(query)?;
        Ok(json_data.into_bytes())
    }

    /// Получает метаданные для BI-инструментов
    pub fn get_schema_info(&self) -> Result<BiSchemaInfo, String> {
        let all_nodes = self.store.get_all().map_err(|e| e.to_string())?;

        let mut property_types = HashMap::new();
        let mut node_labels = std::collections::HashSet::new();

        for node in &all_nodes {
            // Собираем типы свойств
            if let Some(props) = node.properties.as_object() {
                for (key, value) in props {
                    let value_type = match value {
                        Value::String(_) => "string".to_string(),
                        Value::Number(_) => "number".to_string(),
                        Value::Bool(_) => "boolean".to_string(),
                        Value::Array(_) => "array".to_string(),
                        Value::Object(_) => "object".to_string(),
                        Value::Null => "null".to_string(),
                    };

                    property_types
                        .entry(key.clone())
                        .or_insert_with(Vec::new)
                        .push(value_type);
                }
            }

            // Собираем метки узлов (если они есть в свойствах)
            if let Some(label) = node.properties.get("label").and_then(|v| v.as_str()) {
                node_labels.insert(label.to_string());
            }
        }

        // Упрощаем типы свойств (берем наиболее распространенный)
        let simplified_property_types: HashMap<String, String> = property_types
            .into_iter()
            .map(|(key, types)| {
                // Находим наиболее частый тип
                let mut type_counts = HashMap::new();
                for t in types {
                    *type_counts.entry(t).or_insert(0) += 1;
                }

                let most_common_type = type_counts
                    .into_iter()
                    .max_by_key(|(_, count)| *count)
                    .map_or_else(|| "unknown".to_string(), |(type_name, _)| type_name);

                (key, most_common_type)
            })
            .collect();

        Ok(BiSchemaInfo {
            node_count: all_nodes.len(),
            property_types: simplified_property_types,
            node_labels: node_labels.into_iter().collect(),
            vector_dimension: all_nodes.first().map_or(0, |n| n.vector.len()),
            has_edges: all_nodes.iter().any(|n| !n.edges.is_empty()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct BiConnection {
    pub connector_type: BiConnectorType,
    pub connected: bool,
    pub connection_info: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BiExportData {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub metadata: BiMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct BiMetadata {
    pub node_count: usize,
    pub export_timestamp: u64,
    pub format_version: String,
}

#[derive(Debug, Clone)]
pub struct BiSchemaInfo {
    pub node_count: usize,
    pub property_types: HashMap<String, String>,
    pub node_labels: Vec<String>,
    pub vector_dimension: usize,
    pub has_edges: bool,
}

// Реализация для различных BI-инструментов
pub struct TableauConnector {
    base_url: String,
    api_token: String,
}

impl TableauConnector {
    pub fn new(base_url: &str, api_token: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            api_token: api_token.to_string(),
        }
    }

    /// Подготовка данных для Tableau
    pub fn prepare_tableau_data(&self, nodes: &[Node]) -> Result<String, String> {
        // Tableau предпочитает формат Hyper
        // Пока возвращаем CSV
        let mut csv = "id,vector_length,edges_count,label,score\n".to_string();

        for node in nodes {
            let label = node
                .properties
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let score = node
                .properties
                .get("score")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);

            csv.push_str(&format!(
                "{},{},{},{},{}\n",
                node.id,
                node.vector.len(),
                node.edges.len(),
                label,
                score
            ));
        }

        Ok(csv)
    }
}

pub struct GrafanaConnector {
    datasource_url: String,
    api_key: String,
}

impl GrafanaConnector {
    pub fn new(datasource_url: &str, api_key: &str) -> Self {
        Self {
            datasource_url: datasource_url.to_string(),
            api_key: api_key.to_string(),
        }
    }

    /// Подготовка данных для Grafana
    pub fn prepare_grafana_data(&self, nodes: &[Node]) -> Result<Value, String> {
        // Grafana ожидает специфический формат JSON
        let mut series = Vec::new();

        // Создаем временную серию для каждого узла (если у него есть временные метки)
        for node in nodes {
            if let Some(timestamp) = node
                .properties
                .get("timestamp")
                .and_then(serde_json::Value::as_f64)
            {
                if let Some(value) = node
                    .properties
                    .get("value")
                    .and_then(serde_json::Value::as_f64)
                {
                    series.push(serde_json::json!([
                        [value, (timestamp * 1000.0) as u64] // Grafana ожидает миллисекунды
                    ]));
                }
            }
        }

        Ok(serde_json::json!({
            "results": {
                "data": series
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::PersistentStore;
    use serde_json::json;

    #[test]
    fn test_bi_connector_creation() {
        let store = Arc::new(PersistentStore::open("./test_bi_data").unwrap());
        let config = BiConnectorConfig {
            connector_type: BiConnectorType::Grafana,
            connection_string: "http://localhost:3000".to_string(),
            username: None,
            password: None,
            api_key: Some("test-api-key".to_string()),
            custom_headers: HashMap::new(),
        };

        let connector = BiConnector::new(store, config);
        assert!(connector.config.api_key.is_some());
    }

    #[test]
    fn test_schema_info() {
        let store = Arc::new(PersistentStore::open("./test_bi_data2").unwrap());
        let config = BiConnectorConfig {
            connector_type: BiConnectorType::Tableau,
            connection_string: "http://localhost:8080".to_string(),
            username: Some("admin".to_string()),
            password: Some("password".to_string()),
            api_key: None,
            custom_headers: HashMap::new(),
        };

        let connector = BiConnector::new(store.clone(), config);

        // Добавляем тестовые узлы
        let test_nodes = vec![
            Node {
                id: 1,
                vector: vec![0.1, 0.2, 0.3],
                properties: json!({"label": "user", "score": 0.8, "active": true}),
                edges: vec![],
            },
            Node {
                id: 2,
                vector: vec![0.4, 0.5, 0.6],
                properties: json!({"label": "document", "score": 0.9, "published": false}),
                edges: vec![],
            },
        ];

        for node in test_nodes {
            store.insert(node).unwrap();
        }

        let schema_info = connector.get_schema_info().unwrap();
        assert_eq!(schema_info.node_count, 2);
        assert!(schema_info.property_types.contains_key("label"));
        assert!(schema_info.property_types.contains_key("score"));
        assert!(schema_info.node_labels.contains(&"user".to_string()));
        assert!(schema_info.node_labels.contains(&"document".to_string()));
    }

    #[test]
    fn test_csv_export() {
        let store = Arc::new(PersistentStore::open("./test_bi_data3").unwrap());
        let config = BiConnectorConfig {
            connector_type: BiConnectorType::Metabase,
            connection_string: "http://localhost:3001".to_string(),
            username: None,
            password: None,
            api_key: None,
            custom_headers: HashMap::new(),
        };

        let connector = BiConnector::new(store.clone(), config);

        // Добавляем тестовый узел
        let test_node = Node {
            id: 1,
            vector: vec![0.5, 0.3],
            properties: json!({"name": "Test Node", "value": 42, "active": true}),
            edges: vec![],
        };

        store.insert(test_node).unwrap();

        let csv_result = connector.export_csv("SELECT * FROM nodes LIMIT 1");
        assert!(csv_result.is_ok());

        let csv = csv_result.unwrap();
        assert!(csv.contains("id"));
        assert!(csv.contains("vector_length"));
        assert!(csv.contains("name"));
        assert!(csv.contains("value"));
        assert!(csv.contains("active"));
    }

    #[test]
    fn test_connection_types() {
        let store = Arc::new(PersistentStore::open("./test_bi_data4").unwrap());

        let configs = vec![
            BiConnectorConfig {
                connector_type: BiConnectorType::Tableau,
                connection_string: "http://tableau.local".to_string(),
                username: Some("user".to_string()),
                password: Some("pass".to_string()),
                api_key: None,
                custom_headers: HashMap::new(),
            },
            BiConnectorConfig {
                connector_type: BiConnectorType::PowerBI,
                connection_string: "http://powerbi.local".to_string(),
                username: None,
                password: None,
                api_key: Some("api-key".to_string()),
                custom_headers: HashMap::new(),
            },
            BiConnectorConfig {
                connector_type: BiConnectorType::Grafana,
                connection_string: "http://grafana.local".to_string(),
                username: None,
                password: None,
                api_key: Some("api-key".to_string()),
                custom_headers: HashMap::new(),
            },
        ];

        for config in configs {
            let connector = BiConnector::new(store.clone(), config);
            let connection = connector.connect();
            assert!(connection.is_ok());
            assert!(connection.unwrap().connected);
        }
    }
}
