use crate::embedding::{EmbeddingModel, EmbeddingService};
use crate::hybrid_storage::{HybridPersistentStore, Node};
use crate::math::MatryoshkaDim;
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;

/// Глобальный сервис эмбеддингов (ленивая инициализация)
static mut EMBEDDING_SERVICE: Option<Arc<EmbeddingService>> = None;

/// Получает или создаёт сервис эмбеддингов
fn get_embedding_service() -> Arc<EmbeddingService> {
    unsafe {
        if EMBEDDING_SERVICE.is_none() {
            EMBEDDING_SERVICE = Some(Arc::new(EmbeddingService::new(
                EmbeddingModel::MultilingualE5Small,
            )));
        }
        EMBEDDING_SERVICE.clone().unwrap()
    }
}

#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    pub collection: Option<String>,
    pub chunk_size: Option<usize>,
    pub dimension: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IngestStats {
    pub nodes_created: u64,
    pub chunks_processed: u64,
    pub processing_time_ms: u64,
    pub collection: String,
    pub file_types: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct NodePreview {
    pub id: u64,
    pub text_preview: String,
    pub vector_dimension: usize,
    pub metadata: serde_json::Value,
}

pub async fn universal_ingest(
    State(store): State<Arc<HybridPersistentStore>>,
    mut multipart: Multipart,
) -> Result<Json<IngestStats>, StatusCode> {
    let start_time = std::time::Instant::now();
    let mut stats = IngestStats {
        nodes_created: 0,
        chunks_processed: 0,
        processing_time_ms: 0,
        collection: "default".to_string(),
        file_types: Vec::new(),
    };

    let mut nodes_to_insert = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().unwrap_or("unknown").to_string();
        let content_type = field.content_type().unwrap_or("").to_string();

        // Update collection if specified
        if name == "collection" {
            if let Ok(collection_text) = field.text().await {
                stats.collection = collection_text;
            }
            continue;
        }

        // Process file fields
        if name == "file" || name == "files" {
            let file_name = field.file_name().unwrap_or("unknown").to_string();
            let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;

            stats.file_types.push(content_type.clone());

            let nodes = match content_type.as_str() {
                ct if ct.contains("pdf") => process_pdf(&data, &file_name).await,
                ct if ct.contains("csv") => process_csv(&data, &file_name).await,
                ct if ct.contains("json") => process_json(&data, &file_name).await,
                ct if ct.starts_with("text") => process_text(&data, &file_name).await,
                ct if ct.starts_with("image") => process_image(&data, &file_name).await,
                _ => process_generic(&data, &file_name).await,
            };

            match nodes {
                Ok(mut new_nodes) => {
                    stats.chunks_processed += new_nodes.len() as u64;
                    nodes_to_insert.append(&mut new_nodes);
                }
                Err(e) => {
                    eprintln!("Error processing file {file_name}: {e}");
                    return Err(StatusCode::UNPROCESSABLE_ENTITY);
                }
            }
        }
    }

    // Insert all nodes into the database
    for node in nodes_to_insert {
        if let Err(e) = store.insert(node) {
            eprintln!("Error inserting node: {e}");
        } else {
            stats.nodes_created += 1;
        }
    }

    stats.processing_time_ms = start_time.elapsed().as_millis() as u64;

    Ok(Json(stats))
}

async fn process_pdf(data: &[u8], filename: &str) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    let text = pdf_extract::extract_text_from_mem(data)?;

    let chunks = text_to_chunks(&text, 512);
    let mut nodes = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let vector = generate_embedding(chunk, 384).await?;
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

async fn process_csv(data: &[u8], filename: &str) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    let csv_text = String::from_utf8(data.to_vec())?;
    let mut rdr = csv::Reader::from_reader(csv_text.as_bytes());
    let mut nodes = Vec::new();
    let mut headers = Vec::new();

    for (i, result) in rdr.records().enumerate() {
        match result {
            Ok(record) => {
                if i == 0 {
                    headers = record
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect();
                    continue;
                }

                let mut record_obj = serde_json::Map::new();
                for (j, field) in record.iter().enumerate() {
                    if let Some(header) = headers.get(j) {
                        record_obj
                            .insert(header.clone(), serde_json::Value::String(field.to_string()));
                    }
                }

                let text = format!("{record_obj:?}");
                let vector = generate_embedding(&text, 384).await?;

                let node = Node {
                    id: generate_node_id(),
                    vector,
                    properties: serde_json::json!({
                        "data": record_obj,
                        "source": filename,
                        "row_index": i,
                        "file_type": "csv"
                    }),
                    edges: Vec::new(),
                };
                nodes.push(node);
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(nodes)
}

async fn process_json(
    data: &[u8],
    filename: &str,
) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    let json_str = String::from_utf8(data.to_vec())?;

    // Try to parse as JSON array first
    if let Ok(json_array) = serde_json::from_str::<serde_json::Value>(&json_str) {
        if let Some(arr) = json_array.as_array() {
            let mut nodes = Vec::new();

            for (i, item) in arr.iter().enumerate() {
                let text = item.to_string();
                let vector = generate_embedding(&text, 384).await?;

                let node = Node {
                    id: generate_node_id(),
                    vector,
                    properties: serde_json::json!({
                        "data": item,
                        "source": filename,
                        "index": i,
                        "file_type": "json"
                    }),
                    edges: Vec::new(),
                };
                nodes.push(node);
            }

            return Ok(nodes);
        }
    }

    // If not array, treat as single object
    let vector = generate_embedding(&json_str, 384).await?;
    let node = Node {
        id: generate_node_id(),
        vector,
        properties: serde_json::json!({
            "data": serde_json::from_str::<serde_json::Value>(&json_str)?,
            "source": filename,
            "file_type": "json"
        }),
        edges: Vec::new(),
    };

    Ok(vec![node])
}

async fn process_text(
    data: &[u8],
    filename: &str,
) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    let text = String::from_utf8(data.to_vec())?;
    let chunks = text_to_chunks(&text, 512);
    let mut nodes = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let vector = generate_embedding(chunk, 384).await?;
        let node = Node {
            id: generate_node_id(),
            vector,
            properties: serde_json::json!({
                "text": chunk,
                "source": filename,
                "chunk_index": i,
                "file_type": "text"
            }),
            edges: Vec::new(),
        };
        nodes.push(node);
    }

    Ok(nodes)
}

async fn process_image(
    data: &[u8],
    filename: &str,
) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    // For now, just extract basic metadata and create a simple text representation
    let image_info = serde_json::json!({
        "size": data.len(),
        "filename": filename,
        "file_type": "image"
    });

    let text = format!("Image: {} ({} bytes)", filename, data.len());
    let vector = generate_embedding(&text, 384).await?;

    let node = Node {
        id: generate_node_id(),
        vector,
        properties: serde_json::json!({
            "text": text,
            "metadata": image_info,
            "file_type": "image"
        }),
        edges: Vec::new(),
    };

    Ok(vec![node])
}

async fn process_generic(
    data: &[u8],
    filename: &str,
) -> Result<Vec<Node>, Box<dyn std::error::Error>> {
    let text = format!("Binary file: {} ({} bytes)", filename, data.len());
    let vector = generate_embedding(&text, 384).await?;

    let node = Node {
        id: generate_node_id(),
        vector,
        properties: serde_json::json!({
            "text": text,
            "filename": filename,
            "file_type": "binary"
        }),
        edges: Vec::new(),
    };

    Ok(vec![node])
}

fn text_to_chunks(text: &str, chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let _word_buffer = String::new();

    for word in text.split_whitespace() {
        if current_chunk.len() + word.len() + 1 > chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }

        if current_chunk.is_empty() {
            current_chunk.push(' ');
            current_chunk.push_str(word);
        } else {
            current_chunk.push_str(word);
        }
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk);
    }

    chunks
}

async fn generate_embedding(text: &str, dimension: usize) -> Result<Vec<f32>, String> {
    // Используем multilingual-e5-small модель для генерации эмбеддингов
    let service = get_embedding_service();

    // Генерируем эмбеддинг как passage (не query)
    let mut embedding = service
        .generate_embedding(text, false)
        .await
        .map_err(|e| format!("Embedding generation failed: {e}"))?;

    // Конвертируем в нужную размерность Matryoshka если нужно
    if embedding.len() != dimension {
        embedding = crate::embedding::convert_to_matryoshka(&embedding, dimension);
    }

    Ok(embedding)
}

pub fn generate_node_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub async fn preview_ingestion(
    State(store): State<Arc<HybridPersistentStore>>,
    Json(request): Json<IngestRequest>,
) -> Result<Json<Vec<NodePreview>>, StatusCode> {
    let collection = request.collection.unwrap_or_else(|| "default".to_string());
    let dimension = request.dimension.unwrap_or_else(|| "d384".to_string());

    // Get some sample nodes for preview
    let all_nodes = match store.get_all() {
        Ok(nodes) => nodes,
        Err(_) => return Ok(Json(Vec::new())),
    };

    let previews: Vec<NodePreview> = all_nodes
        .into_iter()
        .take(10)
        .map(|node| {
            let text_preview = node
                .properties
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(100)
                .collect();

            NodePreview {
                id: node.id,
                text_preview,
                vector_dimension: node.vector.len(),
                metadata: serde_json::json!({
                    "collection": collection,
                    "dimension": dimension
                }),
            }
        })
        .collect();

    Ok(Json(previews))
}

pub async fn search_ingested(
    State(store): State<Arc<HybridPersistentStore>>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let query = request.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let dimension_str = request
        .get("dimension")
        .and_then(|v| v.as_str())
        .unwrap_or("d384");
    let threshold = request
        .get("threshold")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.3) as f32;

    let dimension = match dimension_str {
        "d384" => MatryoshkaDim::D384,
        "d768" => MatryoshkaDim::D768,
        "d1536" => MatryoshkaDim::D1536,
        _ => MatryoshkaDim::D384,
    };

    // Convert query string to vector using multilingual-e5-small
    let query_vector = generate_embedding(query, dimension.size())
        .await
        .map_err(|e| {
            eprintln!("Embedding generation failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match store.matryoshka_search(&query_vector, dimension, threshold) {
        Ok(results) => {
            let response = serde_json::json!({
                "results": results.iter().map(|(id, distance)| {
                    serde_json::json!({
                        "id": id,
                        "distance": distance,
                        "similarity": 1.0 - distance
                    })
                }).collect::<Vec<_>>(),
                "count": results.len(),
                "dimension": dimension_str,
                "threshold": threshold
            });
            Ok(Json(response))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
