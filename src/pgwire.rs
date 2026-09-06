use crate::hybrid_storage::HybridPersistentStore;
use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// `PGWire` server for `PostgreSQL` compatibility
///
/// This implementation provides basic `PostgreSQL` wire protocol support
/// allowing connection from psql and other `PostgreSQL` clients.
pub struct PgWireServer {
    pub store: Arc<HybridPersistentStore>,
}

impl PgWireServer {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self { store }
    }

    pub async fn start(&self, addr: &str) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        println!("🔌 PGWire server listening on {addr}");

        loop {
            let (mut stream, peer_addr) = listener.accept().await?;
            println!("📡 PGWire connection from {peer_addr}");

            let store = self.store.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(&mut stream, store).await {
                    eprintln!("Error handling connection from {peer_addr}: {e}");
                }
            });
        }
    }
}

/// Handle incoming `PGWire` connection
async fn handle_connection(
    stream: &mut tokio::net::TcpStream,
    store: Arc<HybridPersistentStore>,
) -> Result<()> {
    // Read startup message
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let msg_len = u32::from_be_bytes(len_buf) as usize;

    if msg_len < 4 {
        return Err(anyhow::anyhow!("Invalid message length"));
    }

    let mut buf = vec![0u8; msg_len - 4];
    stream.read_exact(&mut buf).await?;

    // Parse startup message (simplified)
    // In a full implementation, we would parse the PostgreSQL startup message properly
    // and handle authentication, parameter negotiation, etc.

    // Send authentication OK (trust authentication for now)
    let auth_ok = [
        0u8, 0, 0, 0, // Message type (empty for startup)
        0, 0, 0, 8, // Length
        0, 0, 0, 0, // Authentication OK
    ];
    stream.write_all(&auth_ok).await?;

    // Send ReadyForQuery
    let ready_msg = [
        b'Z', // ReadyForQuery
        0, 0, 0, 5,    // Length
        b'I', // Idle status
    ];
    stream.write_all(&ready_msg).await?;

    println!("✅ Client authenticated (trust mode)");

    // Simple query loop
    let mut query_buf = [0u8; 1024];
    loop {
        match stream.try_read(&mut query_buf) {
            Ok(0) => break, // Connection closed
            Ok(n) => {
                // Parse and execute query (simplified)
                let query = String::from_utf8_lossy(&query_buf[..n]);

                if query.contains("SELECT") || query.contains("MATCH") {
                    // Execute TQL/SQL query
                    let result = execute_query(&query, &store).await;
                    send_query_result(stream, &result).await?;
                } else if query.contains("INSERT") || query.contains("CREATE") {
                    send_query_result(stream, &Ok("INSERT 0 1".to_string())).await?;
                } else if !query.trim().is_empty() {
                    send_query_result(stream, &Ok("SELECT 0".to_string())).await?;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(_) => break,
        }
    }

    Ok(())
}

/// Execute a query (simplified implementation)
async fn execute_query(query: &str, store: &HybridPersistentStore) -> Result<String> {
    let query_upper = query.to_uppercase();

    if query_upper.contains("SELECT") && query_upper.contains("FROM NODES") {
        let nodes = store.get_all()?;
        Ok(format!("{} rows", nodes.len()))
    } else if query_upper.contains("SELECT") && query_upper.contains("VERSION()") {
        Ok(format!("ToroidalDB {}", env!("CARGO_PKG_VERSION")))
    } else if query_upper.contains("SHOW") {
        Ok("1 row".to_string())
    } else {
        Ok("Query executed".to_string())
    }
}

/// Send query result to client
async fn send_query_result(
    stream: &mut tokio::net::TcpStream,
    result: &Result<String>,
) -> Result<()> {
    match result {
        Ok(msg) => {
            // Send CommandComplete
            let mut complete_msg = vec![
                b'C', // CommandComplete
                0, 0, 0, 0, // Length (filled below)
            ];
            complete_msg.extend_from_slice(msg.as_bytes());
            let len = (complete_msg.len() as u32 - 1).to_be_bytes();
            complete_msg[1..5].copy_from_slice(&len);
            stream.write_all(&complete_msg).await?;

            // Send ReadyForQuery
            let ready_msg = [b'Z', 0, 0, 0, 5, b'I'];
            stream.write_all(&ready_msg).await?;
        }
        Err(e) => {
            // Send ErrorResponse
            let mut error_msg = vec![
                b'E', // ErrorResponse
                0, 0, 0, 0, // Length (filled below)
                b'S', b'E', b'R', b'R', b'O', b'R', 0,    // S:ERROR
                b'M', // M: message
            ];
            error_msg.extend_from_slice(e.to_string().as_bytes());
            error_msg.push(0);
            error_msg.push(0); // Terminator
            let len = (error_msg.len() as u32 - 1).to_be_bytes();
            error_msg[1..5].copy_from_slice(&len);
            stream.write_all(&error_msg).await?;

            // Send ReadyForQuery
            let ready_msg = [b'Z', 0, 0, 0, 5, b'I'];
            stream.write_all(&ready_msg).await?;
        }
    }

    stream.flush().await?;
    Ok(())
}

/// `PGWire` handler for query execution
pub struct PgWireHandler {
    store: Arc<HybridPersistentStore>,
}

impl PgWireHandler {
    pub fn new(store: Arc<HybridPersistentStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, query: &str) -> Result<String> {
        execute_query(query, &self.store).await
    }
}
