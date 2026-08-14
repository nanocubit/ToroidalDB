//! # Database Manager для GuiTor
//! 
//! Управление мульти-БД подключениями через sqlx

use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::{AnyPool, AnyRow, AnyConnection, Executor, Row, Column};
use std::sync::Arc;
use uuid::Uuid;

/// Менеджер подключений к БД
#[derive(Clone)]
pub struct DbManager {
    pools: Arc<DashMap<Uuid, AnyPool>>,
    connections: Arc<DashMap<Uuid, DbConnection>>,
}

/// Информация о подключении
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConnection {
    pub id: Uuid,
    pub name: String,
    pub driver: String,
    pub host: String,
    pub port: i32,
    pub database: String,
    pub username: Option<String>,
    pub status: ConnectionStatus,
}

/// Статус подключения
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Error(String),
}

/// Объект БД (таблица, view, функция)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbObject {
    pub name: String,
    pub kind: String, // table, view, function, procedure
    pub schema: String,
    pub columns: Vec<ColumnSchema>,
    pub children: Vec<DbObject>,
}

/// Схема столбца
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub is_primary_key: bool,
}

/// Результат запроса
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub rows_affected: u64,
    pub duration_ms: f64,
}

impl DbManager {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(DashMap::new()),
            connections: Arc::new(DashMap::new()),
        }
    }

    /// Создаёт новое подключение
    pub async fn connect(
        &self,
        name: String,
        driver: String,
        host: String,
        port: i32,
        database: String,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Uuid> {
        // Формируем connection URL
        let url = self.build_connection_url(&driver, &host, port, &database, &username, &password)?;
        
        // Создаём connection pool
        let pool = AnyPool::connect(&url)
            .await
            .context(format!("Failed to connect to {} database", driver))?;

        let id = Uuid::new_v4();
        
        let connection = DbConnection {
            id,
            name,
            driver,
            host,
            port,
            database,
            username,
            status: ConnectionStatus::Connected,
        };

        self.pools.insert(id, pool);
        self.connections.insert(id, connection);

        Ok(id)
    }

    /// Проверяет подключение
    pub async fn test_connection(
        &self,
        driver: String,
        host: String,
        port: i32,
        database: String,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<bool> {
        let url = self.build_connection_url(&driver, &host, port, &database, &username, &password)?;
        
        match AnyPool::connect(&url).await {
            Ok(pool) => {
                // Проверяем простой запрос
                match sqlx::query("SELECT 1").fetch_one(&pool).await {
                    Ok(_) => Ok(true),
                    Err(e) => {
                        tracing::error!("Connection test failed: {}", e);
                        Ok(false)
                    }
                }
            }
            Err(e) => {
                tracing::error!("Connection failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Отключается от БД
    pub async fn disconnect(&self, id: Uuid) -> Result<()> {
        self.pools.remove(&id);
        self.connections.remove(&id);
        Ok(())
    }

    /// Получает список всех подключений
    pub fn list_connections(&self) -> Vec<DbConnection> {
        self.connections.iter().map(|c| c.value().clone()).collect()
    }

    /// Получает подключение по ID
    pub fn get_connection(&self, id: Uuid) -> Option<DbConnection> {
        self.connections.get(&id).map(|c| c.value().clone())
    }

    /// Получает pool по ID
    pub fn get_pool(&self, id: Uuid) -> Option<AnyPool> {
        self.pools.get(&id).map(|p| p.value().clone())
    }

    /// Получает список схем
    pub async fn list_schemas(&self, conn_id: Uuid) -> Result<Vec<DbObject>> {
        let pool = self.get_pool(conn_id)
            .context("Connection not found")?;

        // Универсальный запрос для information_schema
        // Работает для PostgreSQL, MySQL, SQLite
        let schemas = sqlx::query(
            r#"
            SELECT schema_name as name, 'schema' as kind
            FROM information_schema.schemata
            WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast')
            ORDER BY schema_name
            "#
        )
        .fetch_all(&pool)
        .await?;

        let result: Vec<DbObject> = schemas.iter().map(|row| {
            let name: String = row.get("name");
            DbObject {
                name: name.clone(),
                kind: "schema".to_string(),
                schema: name,
                columns: vec![],
                children: vec![],
            }
        }).collect();

        Ok(result)
    }

    /// Получает список таблиц в схеме
    pub async fn list_tables(&self, conn_id: Uuid, schema: String) -> Result<Vec<DbObject>> {
        let pool = self.get_pool(conn_id)
            .context("Connection not found")?;

        let tables = sqlx::query(
            r#"
            SELECT table_name as name, 'table' as kind, table_schema as schema
            FROM information_schema.tables
            WHERE table_schema = $1
            ORDER BY table_name
            "#
        )
        .bind(&schema)
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in tables {
            let name: String = row.get("name");
            let table_schema: String = row.get("schema");
            
            // Получаем столбцы таблицы
            let columns = self.get_columns(conn_id, &table_schema, &name).await?;
            
            result.push(DbObject {
                name: name.clone(),
                kind: "table".to_string(),
                schema: table_schema,
                columns,
                children: vec![],
            });
        }

        Ok(result)
    }

    /// Получает список view в схеме
    pub async fn list_views(&self, conn_id: Uuid, schema: String) -> Result<Vec<DbObject>> {
        let pool = self.get_pool(conn_id)
            .context("Connection not found")?;

        let views = sqlx::query(
            r#"
            SELECT table_name as name, 'view' as kind, table_schema as schema
            FROM information_schema.views
            WHERE table_schema = $1
            ORDER BY table_name
            "#
        )
        .bind(&schema)
        .fetch_all(&pool)
        .await?;

        let result: Vec<DbObject> = views.iter().map(|row| {
            let name: String = row.get("name");
            let table_schema: String = row.get("schema");
            DbObject {
                name: name.clone(),
                kind: "view".to_string(),
                schema: table_schema,
                columns: vec![],
                children: vec![],
            }
        }).collect();

        Ok(result)
    }

    /// Получает список функций
    pub async fn list_functions(&self, conn_id: Uuid, schema: String) -> Result<Vec<DbObject>> {
        let pool = self.get_pool(conn_id)
            .context("Connection not found")?;

        // PostgreSQL-specific query for functions
        let functions = sqlx::query(
            r#"
            SELECT 
                routine_name as name,
                'function' as kind,
                routine_schema as schema
            FROM information_schema.routines
            WHERE routine_schema = $1
            AND routine_type = 'FUNCTION'
            ORDER BY routine_name
            "#
        )
        .bind(&schema)
        .fetch_all(&pool)
        .await?;

        let result: Vec<DbObject> = functions.iter().map(|row| {
            let name: String = row.get("name");
            let routine_schema: String = row.get("schema");
            DbObject {
                name: name.clone(),
                kind: "function".to_string(),
                schema: routine_schema,
                columns: vec![],
                children: vec![],
            }
        }).collect();

        Ok(result)
    }

    /// Получает столбцы таблицы
    async fn get_columns(&self, conn_id: Uuid, schema: &str, table: &str) -> Result<Vec<ColumnSchema>> {
        let pool = self.get_pool(conn_id)
            .context("Connection not found")?;

        let columns = sqlx::query(
            r#"
            SELECT 
                column_name,
                data_type,
                is_nullable,
                CASE 
                    WHEN constraint_type = 'PRIMARY KEY' THEN 'YES'
                    ELSE 'NO'
                END as is_primary_key
            FROM information_schema.columns
            LEFT JOIN (
                SELECT kcu.column_name, kcu.table_schema, kcu.table_name, tc.constraint_type
                FROM information_schema.table_constraints tc
                JOIN information_schema.key_column_usage kcu 
                    ON tc.constraint_name = kcu.constraint_name
                    AND tc.table_schema = kcu.table_schema
                    AND tc.table_name = kcu.table_name
                WHERE tc.constraint_type = 'PRIMARY KEY'
            ) pk ON columns.column_name = pk.column_name 
                AND columns.table_schema = pk.table_schema 
                AND columns.table_name = pk.table_name
            WHERE columns.table_schema = $1 
            AND columns.table_name = $2
            ORDER BY columns.ordinal_position
            "#
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&pool)
        .await?;

        let result: Vec<ColumnSchema> = columns.iter().map(|row| {
            let name: String = row.get("column_name");
            let data_type: String = row.get("data_type");
            let is_nullable: String = row.get("is_nullable");
            let is_primary_key: String = row.get("is_primary_key");
            
            ColumnSchema {
                name,
                data_type,
                is_nullable: is_nullable == "YES",
                is_primary_key: is_primary_key == "YES",
            }
        }).collect();

        Ok(result)
    }

    /// Выполняет SQL запрос
    pub async fn execute_query(&self, conn_id: Uuid, sql: &str) -> Result<QueryResult> {
        let pool = self.get_pool(conn_id)
            .context("Connection not found")?;

        let start = std::time::Instant::now();
        
        // Выполняем запрос
        let rows = sqlx::query(sql)
            .fetch_all(&pool)
            .await?;
        
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Извлекаем названия столбцов
        let columns: Vec<String> = if let Some(first_row) = rows.first() {
            first_row.columns().iter().map(|col| col.name().to_string()).collect()
        } else {
            vec![]
        };

        // Конвертируем строки в JSON значения
        let json_rows: Vec<Vec<serde_json::Value>> = rows.iter().map(|row| {
            columns.iter().enumerate().map(|(i, col)| {
                // Пытаемся получить значение разных типов
                if let Ok(val) = row.try_get::<String, _>(i) {
                    serde_json::Value::String(val)
                } else if let Ok(val) = row.try_get::<i64, _>(i) {
                    serde_json::Value::Number(val.into())
                } else if let Ok(val) = row.try_get::<f64, _>(i) {
                    serde_json::Number::from_f64(val)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                } else if let Ok(val) = row.try_get::<bool, _>(i) {
                    serde_json::Value::Bool(val)
                } else {
                    serde_json::Value::Null
                }
            }).collect()
        }).collect();

        Ok(QueryResult {
            columns,
            rows: json_rows,
            rows_affected: rows.len() as u64,
            duration_ms,
        })
    }

    /// Строит connection URL
    fn build_connection_url(
        &self,
        driver: &str,
        host: &str,
        port: i32,
        database: &str,
        username: &Option<String>,
        password: &Option<String>,
    ) -> Result<String> {
        match driver.to_lowercase().as_str() {
            "postgresql" | "postgres" => {
                let user = username.as_deref().unwrap_or("postgres");
                let pass = password.as_deref().unwrap_or("");
                Ok(format!("postgres://{}:{}@{}:{}/{}", user, pass, host, port, database))
            }
            "mysql" => {
                let user = username.as_deref().unwrap_or("root");
                let pass = password.as_deref().unwrap_or("");
                Ok(format!("mysql://{}:{}@{}:{}/{}", user, pass, host, port, database))
            }
            "sqlite" => {
                // Для SQLite database - это путь к файлу
                Ok(format!("sqlite:{}", database))
            }
            _ => Err(anyhow::anyhow!("Unsupported driver: {}", driver)),
        }
    }

    /// Получает статистику подключения
    pub fn get_stats(&self, conn_id: Uuid) -> Option<ConnectionStats> {
        let conn = self.connections.get(&conn_id)?;
        
        Some(ConnectionStats {
            name: conn.name.clone(),
            driver: conn.driver.clone(),
            host: conn.host.clone(),
            port: conn.port,
            database: conn.database.clone(),
            status: conn.status.clone(),
        })
    }
}

/// Статистика подключения
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub name: String,
    pub driver: String,
    pub host: String,
    pub port: i32,
    pub database: String,
    pub status: ConnectionStatus,
}

impl Default for DbManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_postgres_url() {
        let manager = DbManager::new();
        let url = manager.build_connection_url(
            "postgresql",
            "localhost",
            5432,
            "mydb",
            &Some("user".to_string()),
            &Some("pass".to_string()),
        ).unwrap();
        
        assert_eq!(url, "postgres://user:pass@localhost:5432/mydb");
    }

    #[tokio::test]
    async fn test_build_mysql_url() {
        let manager = DbManager::new();
        let url = manager.build_connection_url(
            "mysql",
            "localhost",
            3306,
            "mydb",
            &Some("root".to_string()),
            &Some("secret".to_string()),
        ).unwrap();
        
        assert_eq!(url, "mysql://root:secret@localhost:3306/mydb");
    }
}
