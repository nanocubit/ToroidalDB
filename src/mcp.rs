use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfiguration {
    pub server_port: u16,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub data_path: String,
    pub backup_retention_days: u32,
    pub cache_size: usize,
    pub cache_ttl_seconds: u64,
    pub shard_count: u32,
    pub max_connections: u32,
    pub query_timeout_seconds: u64,
    pub enable_metrics: bool,
    pub log_level: String,
    pub custom_parameters: HashMap<String, serde_json::Value>,
}

impl Default for SystemConfiguration {
    fn default() -> Self {
        SystemConfiguration {
            server_port: 8443,
            tls_cert_path: "cert.pem".to_string(),
            tls_key_path: "key.pem".to_string(),
            data_path: "./data".to_string(),
            backup_retention_days: 7,
            cache_size: 1000,
            cache_ttl_seconds: 3600,
            shard_count: 3,
            max_connections: 100,
            query_timeout_seconds: 30,
            enable_metrics: true,
            log_level: "info".to_string(),
            custom_parameters: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfiguration {
    pub name: String,
    pub version: String,
    pub dimensions: Vec<u32>, // Поддерживаемые размерности
    pub default_threshold: f32,
    pub optimization_settings: OptimizationSettings,
    pub topology_settings: TopologySettings,
    pub performance_settings: PerformanceSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSettings {
    pub enable_caching: bool,
    pub cache_strategy: String, // "LRU", "LFU", "FIFO"
    pub enable_parallel_search: bool,
    pub early_termination: bool,
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySettings {
    pub enable_toroidal_topology: bool,
    pub enable_ricci_flow: bool,
    pub ricci_iterations: usize,
    pub homotopy_class_tracking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSettings {
    pub thread_pool_size: usize,
    pub memory_limit_mb: usize,
    pub disk_cache_enabled: bool,
    pub disk_cache_path: String,
}

pub struct MCPHandler {
    pub config: Arc<RwLock<SystemConfiguration>>,
    pub models: Arc<RwLock<HashMap<String, ModelConfiguration>>>,
}

impl MCPHandler {
    pub fn new() -> Self {
        MCPHandler {
            config: Arc::new(RwLock::new(SystemConfiguration::default())),
            models: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_system_config(&self) -> SystemConfiguration {
        self.config.read().await.clone()
    }

    pub async fn update_system_config(
        &self,
        new_config: SystemConfiguration,
    ) -> Result<(), String> {
        let mut config = self.config.write().await;
        *config = new_config;
        Ok(())
    }

    pub async fn get_model_config(&self, model_name: &str) -> Option<ModelConfiguration> {
        let models = self.models.read().await;
        models.get(model_name).cloned()
    }

    pub async fn set_model_config(&self, model_config: ModelConfiguration) -> Result<(), String> {
        let mut models = self.models.write().await;
        models.insert(model_config.name.clone(), model_config);
        Ok(())
    }

    pub async fn list_models(&self) -> Vec<String> {
        let models = self.models.read().await;
        models.keys().cloned().collect()
    }

    pub async fn update_model_config(
        &self,
        model_name: &str,
        updates: HashMap<String, serde_json::Value>,
    ) -> Result<(), String> {
        let mut models = self.models.write().await;

        if let Some(model) = models.get_mut(model_name) {
            // Обновляем параметры модели на основе переданных изменений
            for (key, value) in updates {
                match key.as_str() {
                    "version" => {
                        if let Some(version) = value.as_str() {
                            model.version = version.to_string();
                        }
                    }
                    "default_threshold" => {
                        if let Some(threshold) = value.as_f64() {
                            model.default_threshold = threshold as f32;
                        }
                    }
                    "dimensions" => {
                        if let Some(dimensions_array) = value.as_array() {
                            let dims: Result<Vec<u32>, _> = dimensions_array
                                .iter()
                                .map(|v| v.as_u64().map(|n| n as u32).ok_or("Invalid dimension"))
                                .collect();
                            if let Ok(dims) = dims {
                                model.dimensions = dims;
                            }
                        }
                    }
                    _ => {
                        // Обработка других параметров
                        println!("⚠️ Неизвестный параметр модели: {}", key);
                    }
                }
            }
            Ok(())
        } else {
            Err(format!("Model {} not found", model_name))
        }
    }

    pub async fn reset_to_defaults(&self) -> Result<(), String> {
        let mut config = self.config.write().await;
        *config = SystemConfiguration::default();

        let mut models = self.models.write().await;
        models.clear();

        Ok(())
    }

    pub async fn validate_config(&self) -> Result<bool, String> {
        let config = self.config.read().await;

        // Проверяем, что порт в допустимом диапазоне
        if config.server_port < 1024 || config.server_port > 65535 {
            return Err("Server port must be between 1024 and 65535".to_string());
        }

        // Проверяем, что пути существуют
        if !std::path::Path::new(&config.tls_cert_path).exists() {
            return Err(format!(
                "TLS certificate file does not exist: {}",
                config.tls_cert_path
            ));
        }

        if !std::path::Path::new(&config.tls_key_path).exists() {
            return Err(format!(
                "TLS key file does not exist: {}",
                config.tls_key_path
            ));
        }

        // Проверяем параметры кэша
        if config.cache_size == 0 {
            return Err("Cache size must be greater than 0".to_string());
        }

        if config.cache_ttl_seconds == 0 {
            return Err("Cache TTL must be greater than 0".to_string());
        }

        // Проверяем количество шардов
        if config.shard_count == 0 {
            return Err("Shard count must be greater than 0".to_string());
        }

        Ok(true)
    }

    pub async fn apply_config_changes(&self) -> Result<(), String> {
        // В реальной системе здесь будет применение изменений конфигурации
        // к соответствующим компонентам системы

        // Проверяем конфигурацию
        self.validate_config().await?;

        // В будущем можно добавить:
        // - Перезапуск сервера с новыми параметрами
        // - Обновление размера пула соединений
        // - Изменение параметров кэширования
        // - Обновление настроек топологии

        println!("✅ Configuration changes applied successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_handler_creation() {
        let mcp = MCPHandler::new();

        // Проверяем, что конфигурация по умолчанию создана
        let config = mcp.get_system_config().await;
        assert_eq!(config.server_port, 8443);
        assert_eq!(config.backup_retention_days, 7);
        assert_eq!(config.cache_size, 1000);

        println!("✅ MCP Handler creation test passed");
    }

    #[tokio::test]
    async fn test_model_configuration_crud() {
        let mcp = MCPHandler::new();

        // Создаем тестовую конфигурацию модели
        let model_config = ModelConfiguration {
            name: "test_model".to_string(),
            version: "1.0.0".to_string(),
            dimensions: vec![384, 768, 1536],
            default_threshold: 0.3,
            optimization_settings: OptimizationSettings {
                enable_caching: true,
                cache_strategy: "LRU".to_string(),
                enable_parallel_search: true,
                early_termination: true,
                batch_size: 100,
            },
            topology_settings: TopologySettings {
                enable_toroidal_topology: true,
                enable_ricci_flow: false,
                ricci_iterations: 50,
                homotopy_class_tracking: true,
            },
            performance_settings: PerformanceSettings {
                thread_pool_size: 4,
                memory_limit_mb: 1024,
                disk_cache_enabled: true,
                disk_cache_path: "./disk_cache".to_string(),
            },
        };

        // Устанавливаем конфигурацию
        mcp.set_model_config(model_config.clone()).await.unwrap();

        // Получаем конфигурацию
        let retrieved = mcp.get_model_config("test_model").await.unwrap();
        assert_eq!(retrieved.name, "test_model");
        assert_eq!(retrieved.version, "1.0.0");
        assert_eq!(retrieved.dimensions, vec![384, 768, 1536]);

        // Проверяем список моделей
        let models = mcp.list_models().await;
        assert!(models.contains(&"test_model".to_string()));

        println!("✅ Model configuration CRUD test passed");
    }

    #[tokio::test]
    async fn test_system_configuration_updates() {
        let mcp = MCPHandler::new();

        // Получаем текущую конфигурацию
        let mut config = mcp.get_system_config().await;
        assert_eq!(config.server_port, 8443);

        // Обновляем конфигурацию
        config.server_port = 9443;
        config.cache_size = 2000;

        mcp.update_system_config(config.clone()).await.unwrap();

        // Проверяем, что изменения сохранились
        let updated_config = mcp.get_system_config().await;
        assert_eq!(updated_config.server_port, 9443);
        assert_eq!(updated_config.cache_size, 2000);

        println!("✅ System configuration update test passed");
    }

    #[tokio::test]
    async fn test_model_configuration_updates() {
        let mcp = MCPHandler::new();

        // Создаем и устанавливаем модель
        let model_config = ModelConfiguration {
            name: "update_test_model".to_string(),
            version: "1.0.0".to_string(),
            dimensions: vec![384],
            default_threshold: 0.3,
            optimization_settings: OptimizationSettings {
                enable_caching: true,
                cache_strategy: "LRU".to_string(),
                enable_parallel_search: true,
                early_termination: true,
                batch_size: 100,
            },
            topology_settings: TopologySettings {
                enable_toroidal_topology: true,
                enable_ricci_flow: false,
                ricci_iterations: 50,
                homotopy_class_tracking: true,
            },
            performance_settings: PerformanceSettings {
                thread_pool_size: 4,
                memory_limit_mb: 1024,
                disk_cache_enabled: true,
                disk_cache_path: "./disk_cache".to_string(),
            },
        };

        mcp.set_model_config(model_config).await.unwrap();

        // Обновляем только некоторые параметры
        let mut updates = HashMap::new();
        updates.insert(
            "version".to_string(),
            serde_json::Value::String("2.0.0".to_string()),
        );
        updates.insert(
            "default_threshold".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(0.25).unwrap()),
        );

        mcp.update_model_config("update_test_model", updates)
            .await
            .unwrap();

        // Проверяем обновленные параметры
        let updated_model = mcp.get_model_config("update_test_model").await.unwrap();
        assert_eq!(updated_model.version, "2.0.0");
        assert_eq!(updated_model.default_threshold, 0.25);
        // Проверяем, что другие параметры остались без изменений
        assert_eq!(updated_model.dimensions, vec![384]);

        println!("✅ Model configuration update test passed");
    }

    #[tokio::test]
    async fn test_configuration_validation() {
        let mcp = MCPHandler::new();

        // Создаем действительную конфигурацию
        let mut valid_config = SystemConfiguration::default();
        valid_config.server_port = 8444; // Допустимый порт
        valid_config.tls_cert_path = "./test_cert.pem".to_string();
        valid_config.tls_key_path = "./test_key.pem".to_string();

        // Создаем файлы сертификатов для теста
        std::fs::write(&valid_config.tls_cert_path, "fake cert").unwrap();
        std::fs::write(&valid_config.tls_key_path, "fake key").unwrap();

        mcp.update_system_config(valid_config).await.unwrap();

        // Проверяем валидацию
        let is_valid = mcp.validate_config().await.unwrap();
        assert!(is_valid);

        // Удаляем временные файлы
        std::fs::remove_file("./test_cert.pem").ok();
        std::fs::remove_file("./test_key.pem").ok();

        println!("✅ Configuration validation test passed");
    }
}
