// Обработчик для универсальной загрузки файлов
use super::{Node};
use std::collections::HashMap;
use serde_json::Value;
use axum::{
    extract::{Multipart, Path},
    response::Json,
};

use super::{ingest::{IngestStats, IngestPreview},
    hybrid_storage::HybridStorageQueryExecutor};

// Типы для запросов загрузки
#[derive(Debug, serde::Deserialize)]
pub struct IngestRequest {
    pub collection: Option<String>,
    pub chunk_size: Option<usize>,
    pub dimension: Option<String>,
}

// Обработчик универсальной загрузки
pub struct IngestHandler;

impl IngestHandler {
    pub fn new() -> Self {
        Self {}
    }
    
    // Обработка универсального запроса
    pub async fn universal_ingest(
        state: super::ServerState,
        multipart: Multipart,
    ) -> super::JsonResponse {
        let start_time = std::time::Instant::now();
        
        let mut nodes_created = 0u64;
        let mut file_types_processed = std::collections::HashSet::new();
        let collection_name = multipart
            .field_name("collection")
            .and_then(|f| f.file_name())
            .unwrap_or_else(|| "default".to_string());
        
        let mut processing_results = Vec::new();
        
        while let Ok(Some(field)) = multipart.next_field().await {
            match field.name() {
                "file" => {
                    // Обработка файла
                    let data = field.bytes().await.unwrap_or_default();
                    let filename = field.file_name()
                        .unwrap_or("unknown");
                    
                    let content_type = field
                        .content_type()
                        .and_then(|ct| ct.to_string()).unwrap_or("application/octet-stream"));
                    
                    println!("📁 Обработка файла: {} ({})", filename);
                    
                    match content_type.as_str() {
                        "application/pdf" => {
                            match process_pdf(&data, filename, &collection_name, &mut nodes_created, &mut processing_results) {
                                nodes_created += result.len() as u64;
                                processing_results.push(format!("✅ Обработано {} страниц из PDF", result.len()));
                                file_types_processed.insert("PDF");
                            }
                        },
                        "application/csv" => {
                            match process_csv(&data, filename, &collection_name, &mut nodes_created, &mut processing_results) {
                                nodes_created += result.len() as u64;
                                processing_results.push(format!("✅ Обработано {} строк из CSV", result.len()));
                                file_types_processed.insert("CSV");
                            }
                        },
                        "application/json" => {
                            match process_json(&data, filename, &collection_name, &mut nodes_created, &mut processing_results) {
                                nodes_created += result.len() as u64;
                                processing_results.push(format!("✅ Обработано {} объектов из JSON", result.len()));
                                file_types_processed.insert("JSON");
                            }
                        },
                        "text/plain" => {
                            match process_text(&data, filename, &collection_name, &mut nodes_created, &mut processing_results) {
                                nodes_created += result.len() as u64;
                                processing_results.push(format!("✅ Обработано {} символов из текста", result.len()));
                                file_types_processed.insert("TEXT");
                            }
                        },
                        _ => {
                            processing_results.push(format!("⚠️ Неподдерживаемый тип файла: {}", content_type));
                        }
                    }
                }
                }
                "collection" => {
                    // Просто устанавливаем коллекцию
                    println!("📂 Установлена коллекция: {}", collection_name);
                }
                _ => {
                    processing_results.push("ℹ️ Коллекция не указана");
                }
            }
        }
        
        let end_time = start_time.elapsed().as_millis();
        
        // Статистика обработки
        let total_files = processing_results.len();
        let success_files = processing_results.iter().filter(|r| r.starts_with("✅"));
        let failed_files = total_files - success_files;
        
        let stats = IngestStats {
            nodes_created,
            chunks_processed: total_files,
            processing_time_ms: end_time.as_millis(),
            collection: collection_name,
            file_types: file_types_processed.into_iter().collect(),
            success_rate: if total_files > 0 { 
                ((total_files - failed_files) as f64 / total_files as f64) * 100.0
            } else {
                0.0
            },
        };
        
        super::JsonResponse(Json(json!({
            "results": processing_results,
            "stats": stats,
            "collection": stats.collection,
            "message": format!(
                "Загружено {} файлов за {}мс",
                total_files,
                end_time.as_millis()
            ),
        }))
    }
}

// Функции обработки различных типов файлов
async fn process_pdf(
    data: &[u8],
    filename: &str,
    collection: &str,
    nodes_created: &mut u64,
    processing_results: &mut Vec<String>,
) -> Vec<Node> {
    use pdf_extract::TextExtractor;
    
    let mut extractor = TextExtractor::new();
    let cursor = std::io::Cursor::new(data);
    
    let text = extractor.read(&mut cursor).unwrap_or_default();
    let words: Vec<&str> = text.split_whitespace().collect();
    
    // Разделение на чанки
    let chunk_size = 512; // Размер чанка в символах
    let mut chunks = Vec::new();
    
    for (i, word) in words.chunks(chunk_size) {
        if word.is_empty() { continue; }
        
        let vector = super::super::super::math::generate_embedding(&word, 384); // d384 для быстрого поиска
        let metadata = serde_json::json!({
            "source": filename,
            "chunk_index": i,
            "file_type": "pdf"
        });
        
        let node = super::Node {
            id: super::super::math::generate_node_id(),
            vector,
            properties: metadata,
            edges: Vec::new(),
        };
        
        chunks.push(node);
        nodes_created += 1;
    }
    
    Ok(chunks)
}

// Функция обработки CSV
async fn process_csv(
    data: &[u8],
    filename: &str,
    collection: &str,
    nodes_created: &mut u64,
    processing_results: &mut Vec<String>,
) -> Vec<Node> {
    use csv::Reader;
    
    let mut rdr = csv::Reader::from_reader(data);
    let mut records = Vec::new();
    
    for record in rdr.records() {
        let mut record_obj = serde_json::Map::new();
        
        for (i, field) in record.fields().iter().enumerate() {
            record_obj.insert(field.to_string(), serde_json::Value::from(field.clone()));
        }
        
        records.push(record_obj);
    }
    
    let nodes = records
        .into_iter()
        .map(|record_obj| {
            let vector = super::super::super::math::generate_embedding(&record_obj.get("text").unwrap_or_default(), 384);
            
            let metadata = serde_json::json!({
                "source": filename,
                "row_index": record_obj.get("row_index").unwrap_or(0),
                "file_type": "csv"
            });
            
            Node {
                id: super::super::super::math::generate_node_id(),
                vector,
                properties: metadata,
                edges: Vec::new(),
            }
        })
        .collect();
    
    nodes_created = records.len() as u64;
    
    Ok(nodes)
}

// Функция обработки JSON
async fn process_json(
    data: &[u8],
    filename: &str,
    collection: &str,
    nodes_created: &mut u64,
    processing_results: &mut Vec<String>,
) -> Vec<Node> {
    let data_str = String::from_utf8_lossy(data);
    
    match serde_json::from_str(&data_str) {
        Ok(json_value) => {
            let vector = super::super::super::math::generate_embedding(&json_value.get("text").unwrap_or_default(), 384);
            
            let metadata = serde_json::json!({
                "source": filename,
                "file_type": "json"
            });
            
            Node {
                id: super::super::super::math::generate_node_id(),
                vector,
                properties: metadata,
                edges: Vec::new(),
            }
            
            nodes_created += 1;
        }
        
        Err(err) => {
            Vec::new()
        }
    }
}

// Функция обработки текста
async fn process_text(
    data: &[u8],
    filename: &str,
    collection: &str,
    nodes_created: &mut u64,
    processing_results: &mut Vec<String>,
) -> Vec<Node> {
    let text = String::from_utf8_lossy(data);
    
    // Разделение на слова
    let words: Vec<&str> = text.split_whitespace().collect();
    
    let chunks = text_to_chunks(&words, 512);
    
    for (i, chunk) in chunks.iter().enumerate() {
        let vector = super::super::super::math::generate_embedding(&chunk, 384);
        let metadata = serde_json::json!({
            "source": filename,
            "chunk_index": i,
            "file_type": "text"
        });
        
        let node = Node {
            id: super::super::super::math::generate_node_id(),
            vector,
            properties: metadata,
            edges: Vec::new(),
        };
        
        chunks.push(node);
        nodes_created += 1;
    }
    
    Ok(chunks)
    }
}

// Вспомогательная функция разделения текста на чанки
fn text_to_chunks(words: &[&str], chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();
    let current_size = 0;
    
    for word in words {
        if current_size + word.len() + 1 >= chunk_size {
            chunks.push(current_chunk.clone());
            current_chunk.clear();
        }
        current_size += word.len() + 1;
        
        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }
    }
    
    chunks
}

// Создание тестового узла
fn create_test_node(id: u64, vector: Vec<f32>) -> super::Node {
    super::Node {
        id,
        vector,
        properties: serde_json::json!({
            "name": format!("Test Node {}", id),
            "type": "test",
        }),
        edges: Vec::new(),
    }
}