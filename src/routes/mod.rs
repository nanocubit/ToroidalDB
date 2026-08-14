// Работа с сетевыми конфигурациями
pub mod network;

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::sleep;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub max_connections: usize,
    pub request_timeout_secs: u64,
    pub connection_timeout_secs: u64,
    pub health_check_interval: Duration,
    pub retry_attempts: u32,
    pub tcp_keep_alive: bool,
    pub allow_origins: Vec<String>,
}

// Типы соединений
#[derive(Debug, Clone)]
pub enum ConnectionType {
    Http,
    PgWire,
    Internal,
    Unknown,
}

// Структура соединения
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: u64,
    pub connection_type: ConnectionType,
    pub created_at: std::time::Instant,
    pub last_activity: Option<std::time::Instant>,
    pub is_active: bool,
    pub metrics: ConnectionMetrics,
}

// Метрики соединения
#[derive(Debug, Clone)]
pub struct ConnectionMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub queries_processed: u64,
    pub errors: u64,
    pub avg_response_time_ms: f64,
    pub packets_dropped: u64,
    pub throughput_mbps: f64,
}

impl NetworkConfig {
    pub fn default() -> Self {
        Self {
            max_connections: 100,
            request_timeout_secs: 30,
            connection_timeout_secs: 600,
            health_check_interval: Duration::from_secs(30),
            retry_attempts: 3,
            tcp_keep_alive: true,
            allow_origins: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
            "0.0.0.0".to_string(),
                "::1".to_string(),
            "10.0.0.1".to_string(),
                "192.168.1.1".to_string(),
            "172.16.254.1".to_string()
            ],
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            max_connections: 50,
            request_timeout_secs: 60,
            connection_timeout_secs: 300,
            health_check_interval: Duration::from_secs(60),
            retry_attempts: 5,
            tcp_keep_alive: false,
            allow_origins: vec![
                "localhost".to_string(),
                "::1".to_string(),
                "10.0.0.1".to_string(),
                "172.16.254.1".to_string()
            ],
        }
    }
}

// Управление соединениями
pub struct ConnectionManager {
    config: NetworkConfig,
    connections: Arc<RwLock<std::collections::HashMap<u64, Connection>>,
    next_id: Arc<AtomicU64>,
    connection_pool: Vec<Connection>,
}

impl ConnectionManager {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            connection_pool: Vec::new(),
        }
    }
    
    // Получение соединения из пула
    pub async fn get_connection(
        &self,
        connection_type: ConnectionType,
    ) -> Option<Arc<Connection>> {
        // Проверяем, есть ли свободное соединение нужного типа
        let active_connections = self.connections
            .iter()
            .filter(|c| c.connection_type == connection_type && c.is_active)
            .collect();
        
        if !active_connections.is_empty() {
            // Создаём новое соединение
            let new_connection = Arc::new(Connection::new(
                connection_type,
                self.config.connection_timeout_secs,
            ));
            let connection_id = self.next_id.fetch_add(1);
            
            let mut conn = new_connection.as_ref();
            conn.id = connection_id;
            let created_at = std::time::Instant::now();
            
            // Добавляем в пул
            self.connections.insert(connection_id, conn.clone());
            
            println!("🔗 Новое соединение {}: {:?}", connection_id);
            
            return Some(conn)
        } else {
            // Ищем свободное соединение из пула
            match connection_type {
                ConnectionType::Http => {
                    // Для HTTP создаем новое соединение на каждый запрос
                    let http_connection = Arc::new(Connection::new(
                        self.config.request_timeout_secs,
                        ));
                    let connection_id = self.next_id.fetch_add(1);
                    let mut conn = http_connection.as_ref();
                    conn.id = connection_id;
                    let created_at = std::time::now();
                    
                    self.connections.insert(connection_id, conn.clone());
                    
                    println!("🌐 Новое HTTP соединение {}: {}", connection_id);
                    Some(conn)
                }
                },
                ConnectionType::PgWire => {
                    // Для PGWire переиспользуем существующее соединение
                    if let Some(pg_connection) = self.connections
                        .iter_mut()
                        .find(|c| c.connection_type == ConnectionType::PgWire && c.is_active)
                    {
                        // Возвращаем существующее соединение
                        Some(pg_connection.clone())
                    } else {
                        // Создаём новое PGWire соединение
                        let pg_connection = Arc::new(Connection::new(
                            self.config.connection_timeout_secs,
                            ));
                        let connection_id = self.next_id.fetch_add(1);
                        let mut conn = pg_connection.as_ref();
                        conn.id = connection_id;
                        let created_at = std::time::now();
                        
                        println!("🔌 Новое PGWire соединение {}: {}", connection_id);
                        
                        self.connections.insert(connection_id, conn.clone());
                        
                        return Some(conn)
                    }
                },
                ConnectionType::Internal => {
                    let internal_connection = Arc::new(Connection::new(
                        self.config.connection_timeout_secs,
                        ));
                    let connection_id = self.next_id.fetch_add(1);
                    let mut conn = internal_connection.as_ref();
                    conn.id = connection_id;
                    let created_at = std::time::now();
                    
                    self.connections.insert(connection_id, conn.clone());
                    
                    println!("🔗 Новое внутреннее соединение {}: {}", connection_id);
                    Some(conn)
                },
                ConnectionType::Unknown => {
                    // Создаём новое соединение
                    let connection = Arc::new(Connection::new(
                        self.config.connection_timeout_secs,
                        ));
                    let connection_id = self.next_id.fetch_add(1);
                    let mut conn = connection.as_ref();
                    conn.id = connection_id;
                    let created_at = std::time::now();
                    
                    self.connections.insert(connection_id, conn.clone());
                    
                    println!("🔗 Новое соединение {}: {}", connection_id);
                    Some(conn)
                }
            }
        }
    }
    
    // Возвращаем соединение в пул
    pub fn return_connection(
        &self,
        connection_id: u64,
    ) -> Option<Arc<Connection>> {
        if let Some(conn) = self.connections.get(&connection_id) {
            // Помечаем активность соединения
            conn.last_activity = Some(std::time::Instant::now());
            
            // Проверяем, не истекло ли время соединения
            let timeout = Duration::from_millis(
                (std::time::Instant::now() - conn.created_at).as_millis()
            );
            
            if conn.last_activity.is_none() || timeout.elapsed() > Duration::from_millis(self.config.connection_timeout_secs) {
                println!("⚠️ Соединение {} истекло по таймауту", connection_id);
                return None;
            }
            
            // Возвращаем в пул для реиспользования
            conn.is_active = false;
            self.connection_pool.push(conn.clone());
            None
        } else {
            println!("❌ Соединение {} не найдено", connection_id);
            None
        }
    }
    
    // Очистка неактивных соединений
    pub fn cleanup_inactive_connections(&self) {
        let mut inactive_connections = Vec::new();
        
        for (id, conn) in self.connections.iter() {
            if !conn.is_active {
                inactive_connections.push(conn);
            }
        }
        
        // Удаляем неактивные соединения из пула
        for conn in inactive_connections {
            self.connections.remove(&conn.id);
        }
        
        println!("🧹 Очищено {} неактивных соединений", inactive_connections.len());
    }
}

    // Получение метрик соединений
    pub fn get_metrics(&self) -> ConnectionMetrics {
        let active_connections = self.connections
            .iter()
            .filter(|c| c.is_active)
            .collect();
        
        let total_bytes_sent: u64;
        let total_bytes_received: u64;
        let queries_processed: u64;
        let errors: u64;
        let total_requests: u64;
        let total_response_time_ms: f64;
        
        for conn in active_connections {
            let metrics = conn.metrics.clone();
            total_bytes_sent += metrics.bytes_sent;
            total_bytes_received += metrics.bytes_received;
            queries_processed += metrics.queries_processed;
            errors += metrics.errors;
            total_requests += metrics.total_requests;
            total_response_time_ms += metrics.avg_response_time_ms;
        }
        
        let avg_response_time_ms = if total_requests > 0 {
            total_response_time_ms / total_requests as f64
        } else {
            0.0
        };
        
        ConnectionMetrics {
            active_connections: active_connections.len(),
            total_bytes_sent,
            total_bytes_received,
            queries_processed,
            errors,
            total_requests,
            avg_response_time_ms,
            throughput_mbps: if total_bytes_sent > 0 { 
                (total_bytes_sent * 8) / (avg_response_time_ms / 1000.0)
            } else {
                0.0
            },
        }
        }
    }
}

// Мониторинг состояния
pub fn get_health_status(&self) -> Json<Value> {
    let active_connections = self.connections
        .iter()
            .filter(|c| c.is_active)
        .count();
    
    let system_uptime = match sysinfo::uptime() {
        Ok(uptime) => uptime.as_secs(),
        Err(_) => "0.0".to_string(),
    };
    
    let system_load = match sysinfo::load_average().ok() {
        Ok(load) => load.average() * 100.0,
        Err(_) => "50.0".to_string(),
    };
    
    let memory_usage = match sysinfo::memory().ok() {
        Ok(memory) => {
            let used = memory.used();
            let total = memory.total();
            let percentage = (used as f64) / total * 100.0;
            Json(json!({
                "system_uptime": system_uptime,
                "memory_usage": {
                    "used_percentage": percentage,
                    "used": format!("{} GB", used as f64 / 1e9),
                    "total": format!("{} GB", total as f64 / 1e9),
                }
            }),
        })
    } else {
            Json(json!({
                "error": "Не удалось получить информацию о системе",
                "system_uptime": system_uptime,
                "memory_usage": {"used_percentage": "N/A", "used": "N/A", "total": "N/A"}
            }))
        }
    };
    
    let health_status = if active_connections.is_empty() {
        "unhealthy"
    } else {
            "healthy"
    };
    
    let storage_metrics = match self.store.get_storage_metrics() {
        Ok(metrics) => metrics,
        Err(_) => json!({
            "error": "Не удалось получить метрики хранилища",
        }),
    };
    
    Json(json!({
        "connections": active_connections,
        "load_average": system_load.average(),
        "memory_usage": memory_usage,
        "storage": storage_metrics,
        "health": health_status
    }))
}

// Очистка неиспользуемых ресурсов при завершении
impl Drop for ServerState {
    fn drop(self) {
        // Очищаем соединения
        self.cleanup_inactive_connections();
        
        // Очищаем Arc ссылки
        drop(self.store);
        
        // Завершаем воркеры
        println!("🔑 Завершение работы сервера");
    }
}