//! TQL gRPC service — tonic-based server.
//!
//! ## Proto definition
//!
//! Proto-файл: `proto/tql.proto`
//! ```protobuf
//! service TqlService {
//!     rpc ExecuteQuery(ExecuteQueryRequest) returns (ExecuteQueryResponse);
//!     rpc Subscribe(SubscribeRequest) returns (stream SubscribeResponse);
//!     rpc HealthCheck(HealthCheckRequest) returns (HealthCheckResponse);
//! }
//! ```
//!
//! Для сборки требуется:
//! ```bash
//! cargo add tonic prost --features prost
//! # или добавьте build.rs с tonic-build
//! ```

use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::engine::TqlEngine;
use std::sync::Arc;
use tokio::sync::broadcast;

/// gRPC server configuration.
pub struct GrpcConfig {
    pub host: String,
    pub port: u16,
    pub proto_path: String,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 50051,
            proto_path: "proto/tql.proto".to_string(),
        }
    }
}

/// TQL gRPC server.
pub struct TqlGrpcServer {
    config: GrpcConfig,
    engine: TqlEngine,
    event_tx: broadcast::Sender<String>,
}

impl TqlGrpcServer {
    pub fn new(config: GrpcConfig, store: Arc<HybridPersistentStore>) -> Self {
        let (event_tx, _) = broadcast::channel(1024);
        Self {
            config,
            engine: TqlEngine::with_store(store),
            event_tx,
        }
    }

    pub fn event_tx(&self) -> broadcast::Sender<String> {
        self.event_tx.clone()
    }

    /// Start the gRPC server.
    ///
    /// Для полноценной работы требуется:
    /// 1. Добавить tonic и prost в Cargo.toml
    /// 2. Создать build.rs с `tonic_build::compile_protos("proto/tql.proto`")
    /// 3. Сгенерировать код: `tonic_build::compile_protos("proto/tql.proto`")?
    ///
    /// После этого можно использовать сгенерированный код:
    /// ```rust,ignore
    /// use tonic::transport::Server;
    /// use tql::tql_service_server::{TqlService, TqlServiceServer};
    ///
    /// Server::builder()
    ///     .add_service(TqlServiceServer::new(impl))
    ///     .serve(addr)
    ///     .await?;
    /// ```
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!(
            "📡 gRPC server ready on {}:{}",
            self.config.host, self.config.port
        );
        println!("   Proto: {}", self.config.proto_path);
        println!("   Service: toroidal.v1.TqlService");
        println!("   Methods:");
        println!("     - ExecuteQuery(TQL) → results");
        println!("     - Subscribe → stream of changes");
        println!("     - HealthCheck() → status");
        println!();
        println!("   To enable: add to Cargo.toml:");
        println!("     tonic = {{ version = \"0.11\", features = [\"prost\"] }}");
        println!("     prost = \"0.12\"");
        println!("   And create build.rs with:");
        println!("     tonic_build::compile_protos(\"proto/tql.proto\")?;");
        Ok(())
    }

    /// Execute a TQL query.
    pub async fn execute_query(
        &self,
        query: &str,
        query_vector: Option<Vec<f32>>,
    ) -> Result<String, String> {
        let mut tql = query.to_string();
        if let Some(vec) = query_vector {
            tql = format!("-- query_vector: {vec:?}\n{tql}");
        }
        let result = self
            .engine
            .execute(&tql)
            .await
            .map_err(|e| format!("{e}"))?;
        Ok(format!("{result:?}"))
    }

    /// Publish an event to all gRPC subscribers.
    pub fn publish_event(&self, event: String) {
        let _ = self.event_tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_grpc_config_default() {
        let config = GrpcConfig::default();
        assert_eq!(config.port, 50051);
        assert!(config.proto_path.contains("tql.proto"));
    }

    #[tokio::test]
    async fn test_grpc_server_stub() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());
        let server = TqlGrpcServer::new(GrpcConfig::default(), store);
        let result = server.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_publish() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(HybridPersistentStore::open(dir.path().join("test")).unwrap());
        let server = TqlGrpcServer::new(GrpcConfig::default(), store);
        let mut rx = server.event_tx.subscribe();
        server.publish_event("test".to_string());
        let received = rx.recv().await.unwrap();
        assert_eq!(received, "test");
    }
}
