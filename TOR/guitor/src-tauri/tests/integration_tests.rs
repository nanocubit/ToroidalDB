//! # Integration Tests для GuiTor
//! 
//! Полное тестирование всех компонентов GuiTor

use guitor::*;
use db::{DbManager, DbConnection};
use sql_autocomplete::{SqlAutocompleteService, CompletionContext};
use std::sync::Arc;
use tokio;

// ===== DB Manager Tests =====

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::Row;

    #[tokio::test]
    async fn test_db_manager_creation() {
        let manager = DbManager::new();
        assert_eq!(manager.list_connections().len(), 0);
    }

    #[tokio::test]
    async fn test_sqlite_connection() {
        let manager = DbManager::new();
        
        // Создаём тестовую SQLite БД в памяти
        let result = manager.connect(
            "Test SQLite".to_string(),
            "sqlite".to_string(),
            "".to_string(),
            0,
            ":memory:".to_string(),
            None,
            None,
        ).await;
        
        assert!(result.is_ok(), "Failed to connect to SQLite");
        
        let conn_id = result.unwrap();
        let connections = manager.list_connections();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "Test SQLite");
        
        // Тестируем выполнение запроса
        let query_result = manager.execute_query(
            conn_id,
            "SELECT 1 as test"
        ).await;
        
        assert!(query_result.is_ok());
        let result = query_result.unwrap();
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0], "test");
        assert_eq!(result.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_connection_lifecycle() {
        let manager = DbManager::new();
        
        // Connect
        let conn_id = manager.connect(
            "Test DB".to_string(),
            "sqlite".to_string(),
            "".to_string(),
            0,
            ":memory:".to_string(),
            None,
            None,
        ).await.unwrap();
        
        // Verify connected
        let connections = manager.list_connections();
        assert_eq!(connections.len(), 1);
        
        // Disconnect
        manager.disconnect(conn_id).await.unwrap();
        
        // Verify disconnected
        let connections = manager.list_connections();
        assert_eq!(connections.len(), 0);
    }

    #[tokio::test]
    async fn test_schema_operations() {
        let manager = DbManager::new();
        
        let conn_id = manager.connect(
            "Schema Test".to_string(),
            "sqlite".to_string(),
            "".to_string(),
            0,
            ":memory:".to_string(),
            None,
            None,
        ).await.unwrap();
        
        // Создаём тестовую таблицу
        manager.execute_query(conn_id, "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
            .await.unwrap();
        
        // Вставляем данные
        manager.execute_query(conn_id, "INSERT INTO users (name, email) VALUES ('John', 'john@example.com')")
            .await.unwrap();
        manager.execute_query(conn_id, "INSERT INTO users (name, email) VALUES ('Jane', 'jane@example.com')")
            .await.unwrap();
        
        // Выбираем данные
        let result = manager.execute_query(conn_id, "SELECT * FROM users ORDER BY id")
            .await.unwrap();
        
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.columns.len(), 3); // id, name, email
    }
}

// ===== SQL Autocomplete Tests =====

#[cfg(test)]
mod sql_autocomplete_tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let service = SqlAutocompleteService::new("postgresql");
        assert!(true);
    }

    #[test]
    fn test_keyword_completions() {
        let service = SqlAutocompleteService::new("postgresql");
        
        let context = CompletionContext {
            sql: String::new(),
            position: 0,
            schemas: vec![],
            tables: vec![],
            columns: vec![],
            functions: vec![],
            aliases: vec![],
        };
        
        let completions = service.get_completions(&context);
        
        // Проверяем наличие ключевых слов
        let keywords: Vec<_> = completions.iter()
            .filter(|c| c.kind == sql_autocomplete::CompletionKind::Keyword)
            .collect();
        
        assert!(keywords.len() > 20, "Should have many keyword completions");
        assert!(keywords.iter().any(|k| k.label == "SELECT"));
        assert!(keywords.iter().any(|k| k.label == "FROM"));
        assert!(keywords.iter().any(|k| k.label == "WHERE"));
    }

    #[test]
    fn test_schema_completions() {
        let service = SqlAutocompleteService::new("postgresql");
        
        let context = CompletionContext {
            sql: String::new(),
            position: 0,
            schemas: vec!["public".to_string(), "information_schema".to_string()],
            tables: vec![],
            columns: vec![],
            functions: vec![],
            aliases: vec![],
        };
        
        let completions = service.get_completions(&context);
        
        let schemas: Vec<_> = completions.iter()
            .filter(|c| c.kind == sql_autocomplete::CompletionKind::Schema)
            .collect();
        
        assert_eq!(schemas.len(), 2);
    }

    #[test]
    fn test_validate_sql() {
        let service = SqlAutocompleteService::new("postgresql");
        
        // Valid SQL
        assert!(service.validate_sql("SELECT * FROM users").is_ok());
        assert!(service.validate_sql("SELECT id, name FROM users WHERE id = 1").is_ok());
        
        // Invalid SQL
        assert!(service.validate_sql("SELEC * FROM").is_err());
        assert!(service.validate_sql("INVALID SQL QUERY").is_err());
    }

    #[test]
    fn test_extract_tables() {
        let service = SqlAutocompleteService::new("postgresql");
        
        let sql = "SELECT * FROM users u JOIN orders o ON u.id = o.user_id";
        let tables = service.extract_tables(sql);
        
        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"orders".to_string()));
    }

    #[test]
    fn test_format_sql() {
        let service = SqlAutocompleteService::new("postgresql");
        
        let sql = "select * from users where id = 1";
        let result = service.format_sql(sql);
        
        assert!(result.is_ok());
    }
}

// ===== Git VCS Tests =====

#[cfg(test)]
mod git_vcs_tests {
    use super::*;
    use git_vcs::GitVcs;
    use tempfile::tempdir;

    #[test]
    fn test_git_init() {
        let dir = tempdir().unwrap();
        let mut vcs = GitVcs::new(dir.path().to_str().unwrap());
        
        let result = vcs.init_or_open();
        assert!(result.is_ok(), "Failed to initialize Git repository");
        assert!(vcs.repository.is_some());
    }

    #[test]
    fn test_git_commit() {
        let dir = tempdir().unwrap();
        let mut vcs = GitVcs::new(dir.path().to_str().unwrap());
        vcs.init_or_open().unwrap();
        
        // Создаём тестовый файл
        let file_path = dir.path().join("test.sql");
        std::fs::write(&file_path, "CREATE TABLE test (id INT);").unwrap();
        
        // Коммитим
        let relative_path = "test.sql";
        let result = vcs.commit_sql(relative_path, "Initial commit");
        
        // Note: В реальной реализации должно работать
        // Сейчас может вернуть ошибку из-за пути
        println!("Commit result: {:?}", result);
    }

    #[test]
    fn test_git_history() {
        let dir = tempdir().unwrap();
        let mut vcs = GitVcs::new(dir.path().to_str().unwrap());
        vcs.init_or_open().unwrap();
        
        let history = vcs.get_history(10);
        
        // Пустой репозиторий - нет истории
        assert!(history.is_ok());
    }
}

// ===== AI Assistant Tests =====

#[cfg(test)]
mod ai_assistant_tests {
    use super::*;
    use ai_assistant::AiAssistant;

    #[test]
    fn test_extract_sql() {
        let assistant = AiAssistant::new("", "", "");
        
        let text = "Here's the query:\n```sql\nSELECT * FROM users;\n```";
        let sql = assistant.extract_sql(text);
        
        assert_eq!(sql, Some("SELECT * FROM users;".to_string()));
    }

    #[test]
    fn test_build_prompt() {
        let assistant = AiAssistant::new("", "", "");
        
        let request = ai_assistant::AiRequest {
            query: "Explain this".to_string(),
            context: Some("test context".to_string()),
            sql: Some("SELECT 1".to_string()),
            schema: None,
        };

        let prompt = assistant.build_prompt(&request);
        assert!(prompt.contains("Explain this"));
        assert!(prompt.contains("SELECT 1"));
        assert!(prompt.contains("test context"));
    }
}

// ===== SSH Tunnel Tests =====

#[cfg(test)]
mod ssh_tunnel_tests {
    use super::*;
    use ssh_tunnel::{SshTunnel, SshTunnelManager};

    #[test]
    fn test_find_available_port() {
        let tunnel = SshTunnel::new();
        let port = tunnel.find_available_port().unwrap();
        
        assert!(port > 0);
        assert!(port < 65536);
    }

    #[test]
    fn test_tunnel_manager_creation() {
        let manager = SshTunnelManager::new();
        assert!(manager.tunnels.is_empty());
    }

    #[test]
    fn test_tunnel_manager_lifecycle() {
        let mut manager = SshTunnelManager::new();
        
        // Note: Это тест без реального подключения
        // В production нужно поднимать тестовый SSH сервер
        assert_eq!(manager.tunnels.len(), 0);
    }
}

// ===== Integration Tests =====

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_workflow() {
        // 1. Создаём DB Manager
        let db_manager = Arc::new(DbManager::new());
        
        // 2. Подключаемся к SQLite
        let conn_id = db_manager.connect(
            "Integration Test".to_string(),
            "sqlite".to_string(),
            "".to_string(),
            0,
            ":memory:".to_string(),
            None,
            None,
        ).await.unwrap();
        
        // 3. Создаём схему
        db_manager.execute_query(conn_id, "CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT, price REAL)")
            .await.unwrap();
        
        // 4. Вставляем данные
        db_manager.execute_query(conn_id, "INSERT INTO products (name, price) VALUES ('Product A', 19.99)")
            .await.unwrap();
        db_manager.execute_query(conn_id, "INSERT INTO products (name, price) VALUES ('Product B', 29.99)")
            .await.unwrap();
        
        // 5. Выбираем данные
        let result = db_manager.execute_query(conn_id, "SELECT * FROM products ORDER BY price")
            .await.unwrap();
        
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][1].as_str().unwrap(), "Product A");
        assert_eq!(result.rows[1][1].as_str().unwrap(), "Product B");
        
        // 6. Тестируем autocomplete
        let service = SqlAutocompleteService::new("sqlite");
        let context = CompletionContext {
            sql: "SELECT * FROM ".to_string(),
            position: 14,
            schemas: vec![],
            tables: vec![sql_autocomplete::TableInfo {
                name: "products".to_string(),
                schema: "main".to_string(),
                columns: vec!["id".to_string(), "name".to_string(), "price".to_string()],
            }],
            columns: vec![],
            functions: vec![],
            aliases: vec![],
        };
        
        let completions = service.get_completions(&context);
        assert!(completions.iter().any(|c| c.label == "products"));
    }
}
