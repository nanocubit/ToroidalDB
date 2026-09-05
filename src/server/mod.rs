use std::sync::Arc;
use tokio::net::TcpListener;

use crate::http_handlers::AppState;
use crate::hybrid_storage::HybridPersistentStore;

pub mod routes;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub request_timeout_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8443,
            max_connections: 1000,
            request_timeout_secs: 30,
        }
    }
}

pub async fn run_server(config: AppConfig) -> Result<()> {
    println!("🚀 ToroidalDB v{}", env!("CARGO_PKG_VERSION"));

    let store = Arc::new(HybridPersistentStore::open("./data")?);
    let state = Arc::new(AppState::new(store));

    let app = routes::create_router(state.clone());

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;

    println!("🌐 Listening on {}", addr);

    axum::serve(listener, app)
        .await?;

    Ok(())
}