use axum::{
    routing::{get, post},
    Json, Router,
};
use axum_server::Handle;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use toroidal_db::{
    admin_ui::admin_ui_handler,
    hybrid_storage::HybridPersistentStore,
    ingestion::{preview_ingestion, search_ingested, universal_ingest},
    pgwire::PgWireServer,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    println!("🚀 ToroidalDB v3.1.0 starting on https://localhost:8443");
    println!("✅ Test with: curl https://localhost:8443/health");
    println!("🌐 Admin UI: https://localhost:8443/admin");
    println!("🔌 PGWire: psql -h localhost -p 5432 -U admin -d toroidal");

    // Initialize hybrid storage
    let store = Arc::new(HybridPersistentStore::open("./data").expect("Failed to open storage"));

    // Check if migration to RocksDB is recommended
    if store.should_migrate_to_rocksdb() {
        println!("⚠️  Consider migrating to RocksDB for better performance with large datasets");
    }

    // Start PGWire server in background
    let pgwire_store = store.clone();
    let pgwire_handle = tokio::spawn(async move {
        let pg_server = PgWireServer::new(pgwire_store);
        if let Err(e) = pg_server.start("127.0.0.1:5432").await {
            eprintln!("Failed to start PGWire server: {}", e);
        }
    });

    let health_store = store.clone();
    let app = Router::new()
        .route(
            "/health",
            get(move || async move {
                Json(json!({
                    "status": "healthy",
                    "version": env!("CARGO_PKG_VERSION"),
                    "storage_type": health_store.get_storage_type(),
                    "node_count": health_store.len().unwrap_or(0)
                }))
            }),
        )
        .route("/admin", get(admin_ui_handler))
        .route("/ingest/universal", post(universal_ingest))
        .route("/ingest/preview", post(preview_ingestion))
        .route("/ingest/search", post(search_ingested))
        .with_state(store);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8443));
    let handle = Handle::new();

    // Load TLS certificates
    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file("cert.pem", "key.pem")
        .await
        .expect("Failed to load TLS certificates");

    let http_handle = tokio::spawn(async move {
        axum_server::bind_rustls(addr, rustls_config)
            .handle(handle)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    // Wait for both tasks
    tokio::try_join!(http_handle, pgwire_handle).expect("Server error");
}
