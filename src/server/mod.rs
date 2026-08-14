use std::sync::Arc;
use tokio::net::TcpListener;
use serde_json::Value;

// Импорты новых модулей
use crate::hybrid_storage::{HybridPersistentStore, Node};
use crate::tql::{executor::QueryExecutor, parser};
use crate::http_handlers::{
    search::SearchHandler,
    nodes::NodeHandler,
    admin::AdminHandler,
    ingest::IngestHandler,
    health::HealthHandler,
};
use crate::auth::{AuthService, Credentials};
use crate::backup::BackupManager;
use crate::metrics::MetricsCollector;

// Модули
pub mod routes;
pub mod handlers;
pub mod core;
pub mod services;

// Error типы
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub request_timeout_secs: u64,
}

#[derive(Clone)]
pub struct ServerState {
    pub store: Arc<HybridPersistentStore>,
    pub auth: Arc<AuthService>,
    pub metrics: Arc<MetricsCollector>,
    pub backup: Arc<BackupManager>,
}

impl ServerState {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self {
            store,
            auth: Arc::new(AuthService::new()),
            metrics: Arc::new(MetricsCollector::new()),
            backup: Arc::new(BackupManager::new()),
        }
    }
}

// Основная функция сервера
pub async fn run_server() -> Result<()> {
    let config = AppConfig {
        host: "0.0.0.0".to_string(),
        port: 8443,
        max_connections: 1000,
        request_timeout_secs: 30,
    };

    println!("🚀 Запуск ToroidalDB v{}...", env!("CARGO_PKG_VERSION"));
    println!("📊 WebSocket: ws://localhost:8444");
    println!("🔌 PGWire: postgresql://admin:admin@localhost:5432/toroidal");
    
    // Инициализация состояния
    let store = Arc::new(HybridPersistentStore::open("./data")?);
    let state = ServerState::new(store);
    
    // Создание маршрутизатора
    let app = routes::create_router().await?;
    
    // Запуск сервера
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| format!("Ошибка привязки: {}", e))?;
    
    println!("🌐 Сервер запущен на {}:{}", addr);
    
    // Обработка соединений
    axum::serve(listener)
        .with_state(state)
        .serve(app)
        .await
        .map_err(|e| format!("Ошибка запуска сервера: {}", e))?;
    
    Ok(())
}

// Функция для graceful shutdown
pub async fn shutdown_signal() -> Result<()> {
    println!("🛑️ Завершение работы ToroidalDB...");
    // Здесь будет логика graceful shutdown
    
    Ok(())
}