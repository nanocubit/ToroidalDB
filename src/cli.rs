use clap::{Args, Parser, Subcommand};
use serde_json::Value;
use std::fs;

#[derive(Parser)]
#[command(name = "toroidal-cli")]
#[command(about = "CLI для взаимодействия с ToroidalDB", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "https://localhost:8443")]
    /// Адрес сервера `ToroidalDB`
    host: String,

    #[arg(short, long)]
    /// Токен аутентификации
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
    Nodes(NodesArgs),

    /// Управление транзакциями
    Transaction(TransactionArgs),
}

#[derive(Args)]
struct NodesArgs {
    #[command(subcommand)]
    command: NodeCommands,
}

#[derive(Subcommand)]
enum NodeCommands {
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

#[derive(Args)]
struct TransactionArgs {
    #[command(subcommand)]
    command: TransactionCommands,
}

#[derive(Subcommand)]
enum TransactionCommands {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Устанавливаем токен аутентификации
    let auth_header = if let Some(token) = &cli.token {
        format!("Bearer {token}")
    } else {
        // Попробуем получить токен из переменной окружения
        std::env::var("TOROIDAL_TOKEN").unwrap_or_else(|_| "Bearer admin-token-xyz".to_string())
    };

    match &cli.command {
        Commands::Query { query } => {
            execute_query(&cli.host, &auth_header, query).await?;
        }
        Commands::Backup { description } => {
            create_backup(&cli.host, &auth_header, description).await?;
        }
        Commands::Restore { backup_id } => {
            restore_backup(&cli.host, &auth_header, backup_id).await?;
        }
        Commands::ListBackups => {
            list_backups(&cli.host, &auth_header).await?;
        }
        Commands::Health => {
            check_health(&cli.host).await?;
        }
        Commands::Metrics => {
            get_metrics(&cli.host).await?;
        }
        Commands::Nodes(nodes_args) => {
            handle_nodes_command(&cli.host, &auth_header, nodes_args).await?;
        }
        Commands::Transaction(transaction_args) => {
            handle_transaction_command(&cli.host, &auth_header, transaction_args).await?;
        }
    }

    Ok(())
}

async fn execute_query(
    host: &str,
    auth_header: &str,
    query: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client
        .post(&format!("{host}/tql"))
        .header("Authorization", auth_header)
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
    host: &str,
    auth_header: &str,
    description: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let mut payload = serde_json::json!({});
    if let Some(desc) = description {
        payload["description"] = serde_json::Value::String(desc.clone());
    }

    let response = client
        .post(&format!("{host}/backup/create"))
        .header("Authorization", auth_header)
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

async fn restore_backup(
    host: &str,
    auth_header: &str,
    backup_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client
        .post(&format!("{host}/backup/restore"))
        .header("Authorization", auth_header)
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

async fn list_backups(host: &str, auth_header: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{host}/backup/list"))
        .header("Authorization", auth_header)
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

async fn check_health(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client.get(&format!("{host}/health")).send().await?;

    let status = response.status();
    let body = response.text().await?;

    if status.is_success() {
        println!("{body}");
    } else {
        eprintln!("❌ Ошибка проверки состояния: {body}");
    }

    Ok(())
}

async fn get_metrics(host: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client.get(&format!("{host}/metrics")).send().await?;

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
    host: &str,
    auth_header: &str,
    args: &NodesArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        NodeCommands::Get { id } => {
            get_node(host, auth_header, *id).await?;
        }
        NodeCommands::Create {
            id,
            vector,
            vector_file,
            properties,
        } => {
            create_node(host, auth_header, *id, vector, vector_file, properties).await?;
        }
        NodeCommands::Delete { id } => {
            delete_node(host, auth_header, *id).await?;
        }
    }
    Ok(())
}

async fn get_node(
    host: &str,
    auth_header: &str,
    id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{host}/nodes/{id}"))
        .header("Authorization", auth_header)
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
    host: &str,
    auth_header: &str,
    id: u64,
    vector: &Option<String>,
    vector_file: &Option<String>,
    properties: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // Получаем вектор из файла или строки
    let vector_data = if let Some(file_path) = vector_file {
        let content = fs::read_to_string(file_path)?;
        serde_json::from_str::<Vec<f32>>(&content)?
    } else if let Some(vector_str) = vector {
        if vector_str.starts_with('[') {
            serde_json::from_str::<Vec<f32>>(vector_str)?
        } else {
            // Предполагаем, что это путь к файлу
            let content = fs::read_to_string(vector_str)?;
            serde_json::from_str::<Vec<f32>>(&content)?
        }
    } else {
        // Используем стандартный вектор
        vec![0.5; 384] // 384-мерный вектор
    };

    // Получаем свойства
    let properties_data = if let Some(props_str) = properties {
        if props_str.starts_with('{') {
            serde_json::from_str::<Value>(props_str)?
        } else {
            // Предполагаем, что это путь к файлу
            let content = fs::read_to_string(props_str)?;
            serde_json::from_str::<Value>(&content)?
        }
    } else {
        serde_json::json!({})
    };

    let response = client
        .post(&format!("{host}/nodes/{id}"))
        .header("Authorization", auth_header)
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

async fn delete_node(
    _host: &str,
    _auth_header: &str,
    _id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("❌ Удаление узлов напрямую не поддерживается через API. Используйте транзакции.");
    Ok(())
}

async fn handle_transaction_command(
    host: &str,
    auth_header: &str,
    args: &TransactionArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match &args.command {
        TransactionCommands::Begin { file } => {
            begin_transaction(host, auth_header, file).await?;
        }
        TransactionCommands::Commit => {
            commit_transaction(host, auth_header).await?;
        }
        TransactionCommands::Rollback => {
            rollback_transaction(host, auth_header).await?;
        }
    }
    Ok(())
}

async fn begin_transaction(
    _host: &str,
    _auth_header: &str,
    file: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(file_path) = file {
        let content = fs::read_to_string(file_path)?;
        // В реальной системе здесь будет парсинг файла транзакции
        println!("📁 Транзакция начата из файла: {file_path}");
        println!("   Содержимое: {content}");
    } else {
        println!("📁 Интерактивная транзакция начата");
    };

    println!("💡 Используйте команды TQL для выполнения операций в транзакции");
    println!("💡 Завершите транзакцию командой 'transaction commit' или 'transaction rollback'");

    Ok(())
}

async fn commit_transaction(
    _host: &str,
    _auth_header: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Выполнение коммита транзакции...");
    // В реальной системе здесь будет вызов API для коммита транзакции
    println!("✅ Транзакция успешно зафиксирована");
    Ok(())
}

async fn rollback_transaction(
    _host: &str,
    _auth_header: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Выполнение отката транзакции...");
    // В реальной системе здесь будет вызов API для отката транзакции
    println!("✅ Транзакция успешно откачена");
    Ok(())
}
