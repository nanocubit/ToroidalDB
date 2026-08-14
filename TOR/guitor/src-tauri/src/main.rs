mod db;
mod sql_autocomplete;
mod git_vcs;
mod ai_assistant;
mod ssh_tunnel;

use db::{DbManager, DbConnection, DbObject, QueryResult, ConnectionStatus};
use sql_autocomplete::{SqlAutocompleteService, CompletionContext, CompletionItem};
use serde_json::Value;
use std::sync::Arc;
use tauri::{command, Manager, State};
use toroidal_db::{HybridPersistentStore, Node};
use uuid::Uuid;

// ===== DB Manager Commands =====

#[command]
async fn connect_db(
    name: String,
    driver: String,
    host: String,
    port: i32,
    database: String,
    username: Option<String>,
    password: Option<String>,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Uuid, String> {
    db_manager.connect(name, driver, host, port, database, username, password)
        .await
        .map_err(|e| format!("Connection failed: {}", e))
}

#[command]
async fn test_connection(
    driver: String,
    host: String,
    port: i32,
    database: String,
    username: Option<String>,
    password: Option<String>,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<bool, String> {
    db_manager.test_connection(driver, host, port, database, username, password)
        .await
        .map_err(|e| format!("Connection test failed: {}", e))
}

#[command]
async fn disconnect_db(
    conn_id: Uuid,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<(), String> {
    db_manager.disconnect(conn_id)
        .await
        .map_err(|e| format!("Disconnect failed: {}", e))
}

#[command]
async fn list_connections(
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbConnection>, String> {
    Ok(db_manager.list_connections())
}

#[command]
async fn list_schemas(
    conn_id: Uuid,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbObject>, String> {
    db_manager.list_schemas(conn_id)
        .await
        .map_err(|e| format!("Failed to list schemas: {}", e))
}

#[command]
async fn list_tables(
    conn_id: Uuid,
    schema: String,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbObject>, String> {
    db_manager.list_tables(conn_id, schema)
        .await
        .map_err(|e| format!("Failed to list tables: {}", e))
}

#[command]
async fn list_views(
    conn_id: Uuid,
    schema: String,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbObject>, String> {
    db_manager.list_views(conn_id, schema)
        .await
        .map_err(|e| format!("Failed to list views: {}", e))
}

#[command]
async fn list_functions(
    conn_id: Uuid,
    schema: String,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Vec<DbObject>, String> {
    db_manager.list_functions(conn_id, schema)
        .await
        .map_err(|e| format!("Failed to list functions: {}", e))
}

#[command]
async fn execute_query(
    conn_id: Uuid,
    sql: String,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<QueryResult, String> {
    db_manager.execute_query(conn_id, &sql)
        .await
        .map_err(|e| format!("Query execution failed: {}", e))
}

#[command]
async fn get_connection_stats(
    conn_id: Uuid,
    db_manager: State<'_, Arc<DbManager>>,
) -> Result<Option<db::ConnectionStats>, String> {
    Ok(db_manager.get_stats(conn_id))
}

#[command]
async fn search_nodes(
    query: String,
    threshold: f32,
    state: State<'_, Arc<HybridPersistentStore>>,
) -> Result<Vec<SearchResult>, String> {
    use toroidal_db::math::MatryoshkaDim;

    let query_vector = generate_embedding(&query, 384);

    match state.matryoshka_search(&query_vector, MatryoshkaDim::D384, threshold) {
        Ok(results) => {
            let search_results: Vec<SearchResult> = results
                .into_iter()
                .map(|(id, distance)| SearchResult {
                    id,
                    distance,
                    similarity: 1.0 - distance,
                })
                .collect();
            Ok(search_results)
        }
        Err(e) => Err(format!("Search failed: {}", e)),
    }
}

#[command]
async fn get_node(
    id: u64,
    state: State<'_, Arc<HybridPersistentStore>>,
) -> Result<Option<Node>, String> {
    match state.get(id) {
        Ok(node) => Ok(node),
        Err(e) => Err(format!("Failed to get node: {}", e)),
    }
}

#[command]
async fn get_all_nodes(
    limit: Option<usize>,
    state: State<'_, Arc<HybridPersistentStore>>,
) -> Result<Vec<Node>, String> {
    match state.get_all() {
        Ok(mut nodes) => {
            if let Some(limit) = limit {
                nodes.truncate(limit);
            }
            Ok(nodes)
        }
        Err(e) => Err(format!("Failed to get nodes: {}", e)),
    }
}

#[command]
async fn get_stats(state: State<'_, Arc<HybridPersistentStore>>) -> Result<DatabaseStats, String> {
    let node_count = state
        .len()
        .map_err(|e| format!("Failed to get stats: {}", e))?;

    Ok(DatabaseStats {
        total_nodes: node_count as u64,
        storage_type: state.get_storage_type().to_string(),
        last_updated: chrono::Utc::now().to_rfc3339(),
    })
}

#[command]
async fn add_edge(
    from_id: u64,
    to_id: u64,
    relation_type: String,
    weight: f32,
    state: State<'_, Arc<HybridPersistentStore>>,
) -> Result<bool, String> {
    match state.add_edge(from_id, to_id, relation_type, weight) {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Failed to add edge: {}", e)),
    }
}

#[command]
async fn clear_cache(state: State<'_, Arc<HybridPersistentStore>>) -> Result<(), String> {
    state.clear_cache();
    Ok(())
}

// Open admin HTML in browser
#[command]
async fn open_admin_window(app: tauri::AppHandle) -> Result<(), String> {
    let url = "https://localhost:8443/admin";
    webbrowser::open(url).map_err(|e| format!("Failed to open browser: {}", e))?;
    Ok(())
}

// Helper functions
fn process_pdf_bytes(data: &[u8], filename: &str) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    // For now, create dummy text-based nodes from PDF
    // In production, use pdf-extract crate
    let text = format!("PDF content from {}", filename);
    let chunks = text_to_chunks(&text, 512);

    let mut nodes = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let vector = generate_embedding(chunk, 384);
        let node = Node {
            id: generate_node_id(),
            vector,
            properties: serde_json::json!({
                "text": chunk,
                "source": filename,
                "chunk_index": i,
                "file_type": "pdf"
            }),
            edges: Vec::new(),
        };
        nodes.push(node);
    }

    Ok(nodes)
}

fn text_to_chunks(text: &str, chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for word in text.split_whitespace() {
        if current_chunk.len() + word.len() + 1 > chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }

        if !current_chunk.is_empty() {
            current_chunk.push(' ');
        }
        current_chunk.push_str(word);
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

fn generate_embedding(text: &str, dimension: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Simple hash-based embedding for now
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();

    let mut vector = vec![0.0f32; dimension];
    for i in 0..dimension {
        vector[i] = ((hash >> (i % 64)) & 0xFF) as f32 / 255.0 - 0.5;
    }

    // Normalize
    let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vector.iter_mut() {
            *v /= norm;
        }
    }

    vector
}

fn generate_node_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[derive(serde::Serialize)]
pub struct IngestStats {
    pub nodes_processed: u64,
    pub processing_time_ms: u64,
    pub collection: String,
    pub file_path: String,
    pub success: bool,
}

#[derive(serde::Serialize)]
pub struct SearchResult {
    pub id: u64,
    pub distance: f32,
    pub similarity: f32,
}

#[derive(serde::Serialize)]
pub struct DatabaseStats {
    pub total_nodes: u64,
    pub storage_type: String,
    pub last_updated: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_prevent_default::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            // DB Manager commands
            connect_db,
            test_connection,
            disconnect_db,
            list_connections,
            list_schemas,
            list_tables,
            list_views,
            list_functions,
            execute_query,
            get_connection_stats,
            // SQL Autocomplete commands
            get_sql_completions,
            format_sql,
            validate_sql,
            // Git VCS commands
            git_init,
            git_commit,
            git_history,
            git_list_branches,
            git_diff,
            // AI Assistant commands
            ai_query,
            ai_explain_sql,
            ai_optimize_sql,
            ai_generate_sql,
            // SSH Tunnel commands
            ssh_create_tunnel,
            ssh_close_tunnel,
            // ToroidalDB commands
            ingest_pdf,
            search_nodes,
            get_node,
            get_all_nodes,
            get_stats,
            add_edge,
            clear_cache,
            open_admin_window
        ])
        .setup(|app| {
            // Initialize DbManager
            let db_manager = Arc::new(DbManager::new());
            app.manage(db_manager);

            // Initialize ToroidalDB storage
            let store = Arc::new(
                HybridPersistentStore::open("./tauri_data").expect("Failed to initialize storage"),
            );

            app.manage(store);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
