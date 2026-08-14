// GuiTor - Tauri Backend for DataGrip-like Database IDE
use sqlx::AnyPool;
use sled::Db;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{
    plugin::{Plugin, Result as PluginResult},
    AppHandle, Invoke, Runtime, State, Window,
};
use uuid::Uuid;

// ============================================================================
// AppState - Глобальное состояние приложения
// ============================================================================

#[derive(Clone)]
pub struct AppState {
    /// Database connection pools (PostgreSQL, MySQL, SQLite, etc.)
    pub pools: Arc<Mutex<HashMap<Uuid, AnyPool>>>,
    /// ToroidalDB persistent storage
    pub toroidal_db: Arc<sled::Db>,
    /// Local delta changes (for undo/redo)
    pub local_delta: Arc<sled::Db>,
    /// Schema cache per connection
    pub schema_cache: Arc<Mutex<HashMap<Uuid, Schema>>>,
    /// Active connections metadata
    pub connections: Arc<Mutex<Vec<Connection>>>,
}

impl AppState {
    pub fn new(data_path: &str) -> Result<Self, String> {
        let toroidal_db = sled::open(format!("{}/toroidal", data_path))
            .map_err(|e| format!("Failed to open ToroidalDB: {}", e))?;
        
        let local_delta = sled::open(format!("{}/delta", data_path))
            .map_err(|e| format!("Failed to open delta store: {}", e))?;
        
        Ok(Self {
            pools: Arc::new(Mutex::new(HashMap::new())),
            toroidal_db: Arc::new(toroidal_db),
            local_delta: Arc::new(local_delta),
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            connections: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

/// Database connection metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub name: String,
    pub driver: DriverType,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Supported database drivers
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DriverType {
    PostgreSQL,
    MySQL,
    SQLite,
    ToroidalDB,
}

/// Cached schema information
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Schema {
    pub tables: Vec<Table>,
    pub views: Vec<View>,
    pub functions: Vec<Function>,
    pub procedures: Vec<Procedure>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Table {
    pub name: String,
    pub schema: String,
    pub columns: Vec<Column>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub is_primary_key: bool,
    pub default_value: Option<String>,
    pub ordinal_position: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct View {
    pub name: String,
    pub schema: String,
    pub definition: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Function {
    pub name: String,
    pub schema: String,
    pub arguments: Vec<Column>,
    pub return_type: String,
    pub definition: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Procedure {
    pub name: String,
    pub schema: String,
    pub arguments: Vec<Column>,
    pub definition: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForeignKey {
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_delete: String,
    pub on_update: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn list_connections(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<Connection>, String> {
    let connections = state.connections.lock().map_err(|e| e.to_string())?;
    Ok(connections.clone())
}

#[tauri::command]
pub async fn add_connection(
    state: State<'_, Arc<AppState>>,
    connection: ConnectionRequest,
) -> Result<Connection, String> {
    let id = Uuid::new_v4();
    
    // Создаем connection pool
    let connection_url = match connection.driver {
        DriverType::PostgreSQL => format!(
            "postgres://{}:{}@{}:{}/{}",
            connection.username, connection.password.unwrap_or_default(),
            connection.host, connection.port, connection.database
        ),
        DriverType::MySQL => format!(
            "mysql://{}:{}@{}:{}/{}",
            connection.username, connection.password.unwrap_or_default(),
            connection.host, connection.port, connection.database
        ),
        DriverType::SQLite => format!("sqlite:{}", connection.database),
        DriverType::ToroidalDB => format!("toroidal://{}:{}", connection.host, connection.port),
    };
    
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(10)
        .connect(&connection_url)
        .await
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    let conn = Connection {
        id,
        name: connection.name,
        driver: connection.driver,
        host: connection.host,
        port: connection.port,
        database: connection.database,
        username: connection.username,
        created_at: chrono::Utc::now(),
    };
    
    // Сохраняем pool и connection
    {
        let mut pools = state.pools.lock().map_err(|e| e.to_string())?;
        pools.insert(id, pool);
        
        let mut connections = state.connections.lock().map_err(|e| e.to_string())?;
        connections.push(conn.clone());
    }
    
    Ok(conn)
}

#[tauri::command]
pub async fn remove_connection(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
) -> Result<(), String> {
    // Закрываем pool
    {
        let mut pools = state.pools.lock().map_err(|e| e.to_string())?;
        pools.remove(&conn_id);
        
        let mut connections = state.connections.lock().map_err(|e| e.to_string())?;
        connections.retain(|c| c.id != conn_id);
    }
    
    Ok(())
}

#[tauri::command]
pub async fn list_schemas(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
) -> Result<Vec<String>, String> {
    let pools = state.pools.lock().map_err(|e| e.to_string())?;
    let pool = pools.get(&conn_id)
        .ok_or("Connection not found")?;
    
    // PostgreSQL schema query
    let schemas: Vec<String> = sqlx::query_scalar!(
        "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ('pg_catalog', 'information_schema')"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    
    Ok(schemas)
}

#[tauri::command]
pub async fn list_tables(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
    schema: String,
) -> Result<Vec<Table>, String> {
    let pools = state.pools.lock().map_err(|e| e.to_string())?;
    let pool = pools.get(&conn_id)
        .ok_or("Connection not found")?;
    
    // Get tables
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT tablename FROM pg_tables WHERE schemaname = $1"
    )
    .bind(&schema)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    
    // Get columns for each table
    let mut result = Vec::with_capacity(tables.len());
    for (table_name,) in tables {
        let columns: Vec<Column> = sqlx::query_as(
            "SELECT column_name, data_type, is_nullable::boolean, column_default, ordinal_position
             FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2
             ORDER BY ordinal_position"
        )
        .bind(&schema)
        .bind(&table_name)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        
        // Get primary key
        let pk_columns: Vec<String> = sqlx::query_scalar(
            "SELECT a.attname FROM pg_index i
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
             WHERE i.indrelid = $1::regclass AND i.indisprimary"
        )
        .bind(format!("{}.{}", schema, table_name))
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
        
        result.push(Table {
            name: table_name,
            schema: schema.clone(),
            columns,
            primary_key: pk_columns,
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
            row_count: None,
        });
    }
    
    Ok(result)
}

#[tauri::command]
pub async fn execute_query(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
    query: String,
) -> Result<QueryResult, String> {
    let pools = state.pools.lock().map_err(|e| e.to_string())?;
    let pool = pools.get(&conn_id)
        .ok_or("Connection not found")?;
    
    // Execute query and get results
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    
    let mut result = QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        row_count: rows.len() as u64,
        execution_time_ms: 0,
    };
    
    if let Some(row) = rows.first() {
        for i in 0..row.len() {
            result.columns.push(row.columns().get(i).unwrap_or("unknown").to_string());
        }
    }
    
    for row in rows {
        let mut row_data = Vec::new();
        for i in 0..row.len() {
            let value: Option<String> = row.try_get(i).unwrap_or(None);
            row_data.push(serde_json::json!(value));
        }
        result.rows.push(row_data);
    }
    
    Ok(result)
}

#[tauri::command]
pub async fn get_table_data(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
    schema: String,
    table: String,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<QueryResult, String> {
    let query = format!(
        "SELECT * FROM \"{}\".\"{}\" LIMIT {} OFFSET {}",
        schema, table,
        limit.unwrap_or(100),
        offset.unwrap_or(0)
    );
    execute_query(state, conn_id, query).await
}

#[tauri::command]
pub async fn insert_row(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
    schema: String,
    table: String,
    data: serde_json::Value,
) -> Result<u64, String> {
    let pools = state.pools.lock().map_err(|e| e.to_string())?;
    let pool = pools.get(&conn_id)
        .ok_or("Connection not found")?;
    
    let columns: Vec<String> = data.as_object()
        .ok_or("Invalid data format")?
        .keys()
        .cloned()
        .collect();
    
    let values: Vec<String> = data.as_object()
        .ok_or("Invalid data format")?
        .values()
        .map(|v| format!("'{}'", v.as_str().unwrap_or("")))
        .collect();
    
    let query = format!(
        "INSERT INTO \"{}\".\"{}\" ({}) VALUES ({}) RETURNING *",
        schema, table,
        columns.join(", "),
        values.join(", ")
    );
    
    sqlx::query(&query)
        .fetch(pool)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(1)
}

#[tauri::command]
pub async fn update_row(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
    schema: String,
    table: String,
    where_clause: String,
    data: serde_json::Value,
) -> Result<u64, String> {
    let sets: Vec<String> = data.as_object()
        .ok_or("Invalid data format")?
        .iter()
        .map(|(k, v)| format!("{} = '{}'", k, v.as_str().unwrap_or("")))
        .collect();
    
    let query = format!(
        "UPDATE \"{}\".\"{}\" SET {} WHERE {}",
        schema, table,
        sets.join(", "),
        where_clause
    );
    
    let pools = state.pools.lock().map_err(|e| e.to_string())?;
    let pool = pools.get(&conn_id)
        .ok_or("Connection not found")?;
    
    let result = sqlx::query(&query)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(result.rows_affected())
}

#[tauri::command]
pub async fn delete_row(
    state: State<'_, Arc<AppState>>,
    conn_id: Uuid,
    schema: String,
    table: String,
    where_clause: String,
) -> Result<u64, String> {
    let query = format!(
        "DELETE FROM \"{}\".\"{}\" WHERE {}",
        schema, table, where_clause
    );
    
    let pools = state.pools.lock().map_err(|e| e.to_string())?;
    let pool = pools.get(&conn_id)
        .ok_or("Connection not found")?;
    
    let result = sqlx::query(&query)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(result.rows_affected())
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ConnectionRequest {
    pub name: String,
    pub driver: DriverType,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: u64,
    pub execution_time_ms: u64,
}

// ============================================================================
// Tauri Plugin
// ============================================================================

pub struct GuitorPlugin<R: Runtime> {
    invoke_handler: Box<dyn Fn(Invoke<R>) + Send + Sync>,
}

impl<R: Runtime> GuitorPlugin<R> {
    pub fn new() -> Self {
        Self {
            invoke_handler: Box::new(tauri::generate_handler![
                list_connections,
                add_connection,
                remove_connection,
                list_schemas,
                list_tables,
                execute_query,
                get_table_data,
                insert_row,
                update_row,
                delete_row,
            ]),
        }
    }
}

impl<R: Runtime> Plugin<R> for GuitorPlugin<R> {
    fn name(&self) -> &'static str {
        "guitor"
    }

    fn invoke_handler(&self) -> &dyn Fn(Invoke<R>) {
        &self.invoke_handler
    }

    fn initialize(&mut self, app: &AppHandle<R>, _url: &[url::Url]) -> PluginResult<()> {
        // Инициализация состояния приложения
        let data_path = app.path_resolver().app_data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("./data"));
        
        let state = AppState::new(data_path.to_str().unwrap_or("./data"))
            .map_err(|e| tauri::Error::PluginInitialization(e))?;
        
        app.manage(state);
        Ok(())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

fn get_default_pools() -> HashMap<Uuid, AnyPool> {
    HashMap::new()
}

fn get_default_schema() -> Schema {
    Schema {
        tables: Vec::new(),
        views: Vec::new(),
        functions: Vec::new(),
        procedures: Vec::new(),
    }
}
