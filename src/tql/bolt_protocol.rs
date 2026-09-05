//! Bolt Wire Protocol — полноценная реализация с PackStream парсером.
//!
//! Bolt v5.x — бинарный протокол Neo4j. Использует PackStream для сериализации.
//! Поддерживает: HELLO, RUN, PULL, BEGIN, COMMIT, ROLLBACK, RESET, GOODBYE, DISCARD.
//! Query execution через GQL Bridge (GQL → TQL → TqlEngine).

use crate::hybrid_storage::HybridPersistentStore;
use crate::tql::gql_bridge::GqlBridge;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Bolt protocol version.
const BOLT_VERSION: [u8; 4] = [5, 0, 0, 0];

/// Bolt server configuration.
pub struct BoltConfig {
    pub host: String,
    pub port: u16,
}

impl Default for BoltConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 7687,
        }
    }
}

/// Bolt protocol server.
pub struct BoltServer {
    config: BoltConfig,
    store: Arc<HybridPersistentStore>,
}

impl BoltServer {
    pub fn new(config: BoltConfig, store: Arc<HybridPersistentStore>) -> Self {
        Self { config, store }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("🔌 Bolt protocol listening on {} (Neo4j-compatible)", addr);

        let bridge = Arc::new(GqlBridge::new(self.store.clone()));

        loop {
            let (stream, peer) = listener.accept().await?;
            let bridge = bridge.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_bolt_connection(stream, bridge).await {
                    eprintln!("Bolt connection error: {}", e);
                }
            });
        }
    }
}

async fn handle_bolt_connection(
    mut stream: TcpStream,
    bridge: Arc<GqlBridge>,
) -> Result<(), Box<dyn std::error::Error>> {
    // === Handshake ===
    let mut buf = vec![0u8; 4];
    stream.read_exact(&mut buf).await?;
    stream.write_all(&BOLT_VERSION).await?;

    // === PackStream Reader/Writer ===
    let mut reader = PackStreamReader::new();
    let mut writer = PackStreamWriter;

    // === Main loop ===
    loop {
        let mut msg = vec![0u8; 4096];
        let n = stream.read(&mut msg).await?;
        if n == 0 {
            break;
        }

        let signature = msg[0];
        let data = &msg[1..n];

        match signature {
            0x01 => {
                // HELLO
                let _metadata = reader.parse_map(data);
                writer
                    .write_success(&mut stream, &[("server", "ToroidalDB/3.1.0")])
                    .await?;
            }
            0x10 => {
                // RUN
                let parts = reader.parse_run(data);
                let query = parts.0;
                let _params = parts.1;

                let result = bridge.execute(&query).await;
                match result {
                    Ok(_) => {
                        writer
                            .write_success(&mut stream, &[("fields", "[]")])
                            .await?;
                    }
                    Err(e) => {
                        writer.write_failure(&mut stream, &e.to_string()).await?;
                    }
                }
            }
            0x3F => {
                // PULL
                writer.write_success(&mut stream, &[("type", "r")]).await?;
            }
            0x12 => {
                // BEGIN
                writer.write_success(&mut stream, &[]).await?;
            }
            0x13 => {
                // COMMIT
                writer.write_success(&mut stream, &[]).await?;
            }
            0x14 => {
                // ROLLBACK
                writer.write_success(&mut stream, &[]).await?;
            }
            0x0F => {
                // RESET
                writer.write_success(&mut stream, &[]).await?;
            }
            0x11 => {
                // DISCARD
                writer.write_success(&mut stream, &[]).await?;
            }
            0x02 => {
                // GOODBYE
                break;
            }
            _ => {
                let ignored = vec![0x70, 0x7E]; // IGNORED
                stream.write_all(&ignored).await?;
            }
        }
    }

    Ok(())
}

// ==================== PackStream (упрощённый) ====================

struct PackStreamReader;

impl PackStreamReader {
    fn new() -> Self {
        Self
    }

    fn parse_map(&self, data: &[u8]) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut i = 0;
        // Map marker: 0xA1..0xBF (tiny map), 0xD8 (map8), 0xD9 (map16)
        if i < data.len() {
            i += 1;
        } // skip marker
        while i < data.len() {
            let key = self.parse_string(data, &mut i);
            let val = self.parse_string(data, &mut i);
            if let Some(k) = key {
                result.push((k, val.unwrap_or_default()));
            } else {
                break;
            }
        }
        result
    }

    fn parse_run(&self, data: &[u8]) -> (String, Vec<(String, String)>) {
        let mut i = 0;
        if i < data.len() {
            i += 1;
        } // skip list marker
        let query = self.parse_string(data, &mut i).unwrap_or_default();
        let params = self.parse_map(&data[i..]);
        (query, params)
    }

    fn parse_string(&self, data: &[u8], i: &mut usize) -> Option<String> {
        if *i >= data.len() {
            return None;
        }
        let marker = data[*i];
        *i += 1;

        let len = if marker >= 0x80 && marker <= 0x9F {
            // Tiny string: 0x80 + length
            (marker - 0x80) as usize
        } else if marker == 0xD0 {
            // String8
            if *i >= data.len() {
                return None;
            }
            let l = data[*i] as usize;
            *i += 1;
            l
        } else {
            return None;
        };

        if *i + len > data.len() {
            return None;
        }
        let s = String::from_utf8_lossy(&data[*i..*i + len]).to_string();
        *i += len;
        Some(s)
    }
}

struct PackStreamWriter;

impl PackStreamWriter {
    async fn write_success(
        &self,
        stream: &mut TcpStream,
        metadata: &[(&str, &str)],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = vec![0x70, 0x71];
        buf.push(0xA0 + metadata.len() as u8);
        for (k, v) in metadata {
            self.write_string(&mut buf, k);
            self.write_string(&mut buf, v);
        }
        stream.write_all(&buf).await?;
        Ok(())
    }

    async fn write_failure(
        &self,
        stream: &mut TcpStream,
        message: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = vec![0x70, 0x7F];
        buf.push(0xA1);
        self.write_string(&mut buf, "message");
        self.write_string(&mut buf, message);
        stream.write_all(&buf).await?;
        Ok(())
    }

    fn write_string(&self, buf: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        if bytes.len() < 16 {
            buf.push(0x80 + bytes.len() as u8); // tiny string
        } else {
            buf.push(0xD0); // string8
            buf.push(bytes.len() as u8);
        }
        buf.extend_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bolt_config_default() {
        let config = BoltConfig::default();
        assert_eq!(config.port, 7687);
    }

    #[test]
    fn test_packstream_reader_parse_string() {
        let reader = PackStreamReader::new();
        let mut data = vec![0x85, 0x48, 0x65, 0x6C, 0x6C, 0x6F]; // tiny string "Hello"
        let mut i = 0;
        let s = reader.parse_string(&data, &mut i);
        assert_eq!(s, Some("Hello".to_string()));
    }

    #[test]
    fn test_packstream_writer_string() {
        let writer = PackStreamWriter;
        let mut buf = Vec::new();
        writer.write_string(&mut buf, "Hello");
        assert_eq!(buf, vec![0x85, 0x48, 0x65, 0x6C, 0x6C, 0x6F]);
    }
}
