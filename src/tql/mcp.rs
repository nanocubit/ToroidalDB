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

impl Default for MCPHandler {
    fn default() -> Self {
        Self::new()
    }
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
                    "enable_caching" => {
                        if let Some(enable) = value.as_bool() {
                            model.optimization_settings.enable_caching = enable;
                        }
                    }
                    "cache_strategy" => {
                        if let Some(strategy) = value.as_str() {
                            model.optimization_settings.cache_strategy = strategy.to_string();
                        }
                    }
                    "enable_parallel_search" => {
                        if let Some(enable) = value.as_bool() {
                            model.optimization_settings.enable_parallel_search = enable;
                        }
                    }
                    "early_termination" => {
                        if let Some(enable) = value.as_bool() {
                            model.optimization_settings.early_termination = enable;
                        }
                    }
                    "batch_size" => {
                        if let Some(batch_size) = value.as_u64() {
                            model.optimization_settings.batch_size = batch_size as usize;
                        }
                    }
                    "enable_toroidal_topology" => {
                        if let Some(enable) = value.as_bool() {
                            model.topology_settings.enable_toroidal_topology = enable;
                        }
                    }
                    "enable_ricci_flow" => {
                        if let Some(enable) = value.as_bool() {
                            model.topology_settings.enable_ricci_flow = enable;
                        }
                    }
                    "ricci_iterations" => {
                        if let Some(iterations) = value.as_u64() {
                            model.topology_settings.ricci_iterations = iterations as usize;
                        }
                    }
                    "thread_pool_size" => {
                        if let Some(pool_size) = value.as_u64() {
                            model.performance_settings.thread_pool_size = pool_size as usize;
                        }
                    }
                    "memory_limit_mb" => {
                        if let Some(memory_limit) = value.as_u64() {
                            model.performance_settings.memory_limit_mb = memory_limit as usize;
                        }
                    }
                    "disk_cache_enabled" => {
                        if let Some(enabled) = value.as_bool() {
                            model.performance_settings.disk_cache_enabled = enabled;
                        }
                    }
                    "disk_cache_path" => {
                        if let Some(path) = value.as_str() {
                            model.performance_settings.disk_cache_path = path.to_string();
                        }
                    }
                    _ => {
                        // Обработка других параметров
                        println!("⚠️ Неизвестный параметр модели: {key}");
                    }
                }
            }
            Ok(())
        } else {
            Err(format!("Model {model_name} not found"))
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

        // Проверяем, что порт в допустимом диапазоне (верхняя граница гарантирована u16)
        if config.server_port < 1024 {
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
