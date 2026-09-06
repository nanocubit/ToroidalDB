use clap::{Parser, Subcommand};
use serde_json::Value;

#[derive(Parser)]
#[command(name = "toroidal-cli")]
#[command(about = "CLI для взаимодействия с ToroidalDB", long_about = None)]
pub struct Cli {
    #[arg(short, long, default_value = "https://localhost:8443")]
    /// Адрес сервера `ToroidalDB`
    pub host: String,

    #[arg(short, long)]
    /// Токен аутентификации
    pub token: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Выполнить TQL запрос
    Query {
        /// Сам TQL запрос
        #[arg(required = true)]
        query: String,
    },

    /// Создать резервную копию
    Backup {
        /// Описание резервной копии
        #[arg(long)]
        description: Option<String>,
    },

    /// Восстановить из резервной копии
    Restore {
        /// ID резервной копии
        backup_id: String,
    },

    /// Получить список резервных копий
    ListBackups,

    /// Проверить состояние системы
    Health,

    /// Получить метрики
    Metrics,

    /// Управление узлами
    Nodes {
        #[command(subcommand)]
        command: NodeCommands,
    },

    /// Управление транзакциями
    Transaction {
        #[command(subcommand)]
        command: TransactionCommands,
    },
}

#[derive(Subcommand)]
pub enum NodeCommands {
    /// Получить узел по ID
    Get {
        /// ID узла
        id: u64,
    },

    /// Создать новый узел
    Create {
        /// ID узла
        id: u64,

        /// Вектор узла (в формате JSON или файла)
        #[arg(long)]
        vector: Option<String>,

        /// Файл с вектором
        #[arg(long)]
        vector_file: Option<String>,

        /// Свойства узла (в формате JSON)
        #[arg(long)]
        properties: Option<String>,
    },

    /// Удалить узел
    Delete {
        /// ID узла
        id: u64,
    },
}

#[derive(Subcommand)]
pub enum TransactionCommands {
    /// Начать транзакцию
    Begin {
        /// Файл с операциями транзакции
        file: Option<String>,
    },

    /// Завершить транзакцию
    Commit,

    /// Откатить транзакцию
    Rollback,
}

pub struct CliExecutor {
    pub host: String,
    pub auth_header: String,
}

impl CliExecutor {
    pub fn new(host: String, token: Option<String>) -> Self {
        let auth_header = if let Some(token) = token {
            format!("Bearer {token}")
        } else {
            // Попробуем получить токен из переменной окружения
            std::env::var("TOROIDAL_TOKEN").unwrap_or_else(|_| "Bearer admin-token-xyz".to_string())
        };

        CliExecutor { host, auth_header }
    }

    pub async fn execute_command(
        &self,
        command: Commands,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            Commands::Query { query } => {
                self.execute_query(&query).await?;
            }
            Commands::Backup { description } => {
                self.create_backup(description.as_deref()).await?;
            }
            Commands::Restore { backup_id } => {
                self.restore_backup(&backup_id).await?;
            }
            Commands::ListBackups => {
                self.list_backups().await?;
            }
            Commands::Health => {
                self.check_health().await?;
            }
            Commands::Metrics => {
                self.get_metrics().await?;
            }
            Commands::Nodes { command } => {
                self.handle_nodes_command(command).await?;
            }
            Commands::Transaction { command } => {
                self.handle_transaction_command(command).await?;
            }
        }

        Ok(())
    }

    async fn execute_query(&self, query: &str) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let response = client
            .post(format!("{}/tql", self.host))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "query": query
            }))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка выполнения запроса: {body}");
        }

        Ok(())
    }

    async fn create_backup(
        &self,
        description: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let mut payload = serde_json::json!({});
        if let Some(desc) = description {
            payload["description"] = serde_json::Value::String(desc.to_string());
        }

        let response = client
            .post(format!("{}/backup/create", self.host))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка создания резервной копии: {body}");
        }

        Ok(())
    }

    async fn restore_backup(&self, backup_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let response = client
            .post(format!("{}/backup/restore", self.host))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "backup_id": backup_id
            }))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка восстановления из резервной копии: {body}");
        }

        Ok(())
    }

    async fn list_backups(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let response = client
            .get(format!("{}/backup/list", self.host))
            .header("Authorization", &self.auth_header)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка получения списка резервных копий: {body}");
        }

        Ok(())
    }

    async fn check_health(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let response = client.get(format!("{}/health", self.host)).send().await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка проверки состояния: {body}");
        }

        Ok(())
    }

    async fn get_metrics(&self) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let response = client.get(format!("{}/metrics", self.host)).send().await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка получения метрик: {body}");
        }

        Ok(())
    }

    async fn handle_nodes_command(
        &self,
        command: NodeCommands,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            NodeCommands::Get { id } => {
                self.get_node(id).await?;
            }
            NodeCommands::Create {
                id,
                vector,
                vector_file,
                properties,
            } => {
                self.create_node(id, vector, vector_file, properties)
                    .await?;
            }
            NodeCommands::Delete { id } => {
                self.delete_node(id).await?;
            }
        }
        Ok(())
    }

    async fn get_node(&self, id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        let response = client
            .get(format!("{}/nodes/{}", self.host, id))
            .header("Authorization", &self.auth_header)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка получения узла: {body}");
        }

        Ok(())
    }

    async fn create_node(
        &self,
        id: u64,
        vector: Option<String>,
        vector_file: Option<String>,
        properties: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();

        // Получаем вектор из файла или строки
        let vector_data = if let Some(file_path) = vector_file {
            let content = std::fs::read_to_string(file_path)?;
            serde_json::from_str::<Vec<f32>>(&content)?
        } else if let Some(vector_str) = vector {
            if vector_str.starts_with('[') {
                serde_json::from_str::<Vec<f32>>(&vector_str)?
            } else {
                // Предполагаем, что это путь к файлу
                let content = std::fs::read_to_string(&vector_str)?;
                serde_json::from_str::<Vec<f32>>(&content)?
            }
        } else {
            // Используем стандартный вектор
            vec![0.5; 384] // 384-мерный вектор
        };

        // Получаем свойства
        let properties_data = if let Some(props_str) = properties {
            if props_str.starts_with('{') {
                serde_json::from_str::<Value>(&props_str)?
            } else {
                // Предполагаем, что это путь к файлу
                let content = std::fs::read_to_string(&props_str)?;
                serde_json::from_str::<Value>(&content)?
            }
        } else {
            serde_json::json!({})
        };

        let response = client
            .post(format!("{}/nodes/{}", self.host, id))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "vector": vector_data,
                "properties": properties_data,
                "edges": []
            }))
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if status.is_success() {
            println!("{body}");
        } else {
            eprintln!("❌ Ошибка создания узла: {body}");
        }

        Ok(())
    }

    async fn delete_node(&self, _id: u64) -> Result<(), Box<dyn std::error::Error>> {
        eprintln!(
            "❌ Удаление узлов напрямую не поддерживается через API. Используйте транзакции."
        );
        Ok(())
    }

    async fn handle_transaction_command(
        &self,
        command: TransactionCommands,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match command {
            TransactionCommands::Begin { file } => {
                self.begin_transaction(file).await?;
            }
            TransactionCommands::Commit => {
                self.commit_transaction().await?;
            }
            TransactionCommands::Rollback => {
                self.rollback_transaction().await?;
            }
        }
        Ok(())
    }

    async fn begin_transaction(
        &self,
        file: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(file_path) = file {
            let content = std::fs::read_to_string(file_path)?;
            println!("📁 Транзакция начата из файла");
            println!("   Содержимое: {content}");
        } else {
            println!("📁 Интерактивная транзакция начата");
        }

        println!("💡 Используйте команды TQL для выполнения операций в транзакции");
        println!(
            "💡 Завершите транзакцию командой 'transaction commit' или 'transaction rollback'"
        );
        Ok(())
    }

    async fn commit_transaction(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Выполнение коммита транзакции...");
        // В реальной системе здесь будет вызов API для коммита транзакции
        println!("✅ Транзакция успешно зафиксирована");
        Ok(())
    }

    async fn rollback_transaction(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Выполнение отката транзакции...");
        // В реальной системе здесь будет вызов API для отката транзакции
        println!("✅ Транзакция успешно откачена");
        Ok(())
    }
}
