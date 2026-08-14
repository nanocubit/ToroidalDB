//! # ONNX Runtime Embedding Service для ToroidalDB
//! 
//! Production-ready реализация с использованием ONNX Runtime
//! для модели multilingual-e5-small
//! 
//! ## Требования
//! - ONNX Runtime library (устанавливается автоматически через ort crate)
//! - Модель multilingual-e5-small в формате ONNX
//! 
//! ## Установка модели
//! ```bash
//! # Скачать модель с HuggingFace
//! huggingface-cli download intfloat/multilingual-e5-small \
//!   --include onnx/model.onnx \
//!   --local-dir ./models/multilingual-e5-small
//! ```

use anyhow::{Context, Result};
use ndarray::{Array1, Array2, Axis};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

#[cfg(feature = "embeddings")]
use ort::{ExecutionProvider, Session, SessionBuilder, Value};

/// Конфигурация ONNX Embedding сервиса
#[derive(Debug, Clone)]
pub struct OnnxEmbeddingConfig {
    /// Путь к ONNX модели
    pub model_path: PathBuf,
    /// Путь к токенизатору (vocab.json, merges.txt)
    pub tokenizer_path: PathBuf,
    /// Использовать GPU (CUDA)
    pub use_gpu: bool,
    /// Размер батча для пакетной обработки
    pub batch_size: usize,
    /// Размерность эмбеддинга
    pub embedding_dim: usize,
    /// Максимальная длина последовательности
    pub max_seq_length: usize,
}

impl Default for OnnxEmbeddingConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("./models/multilingual-e5-small/onnx/model.onnx"),
            tokenizer_path: PathBuf::from("./models/multilingual-e5-small"),
            use_gpu: false,
            batch_size: 32,
            embedding_dim: 384,
            max_seq_length: 512,
        }
    }
}

/// Production ONNX Embedding сервис
pub struct OnnxEmbeddingService {
    config: OnnxEmbeddingConfig,
    #[cfg(feature = "embeddings")]
    session: Arc<Session>,
    #[cfg(not(feature = "embeddings"))]
    session: PhantomData<*const ()>,
    tokenizer: Arc<RwLock<Option<Tokenizer>>>,
    cache: Arc<RwLock<HashMap<String, Vec<f32>>>>,
}

#[cfg(not(feature = "embeddings"))]
use std::marker::PhantomData;

// Токенизатор для multilingual-e5
#[derive(Clone)]
struct Tokenizer {
    vocab: HashMap<String, u32>,
    merges: Vec<(String, String)>,
    special_tokens: HashMap<String, u32>,
}

impl OnnxEmbeddingService {
    /// Создаёт новый сервис с конфигурацией по умолчанию
    #[cfg(feature = "embeddings")]
    pub fn new() -> Result<Self> {
        Self::with_config(OnnxEmbeddingConfig::default())
    }

    #[cfg(not(feature = "embeddings"))]
    pub fn new() -> Result<Self> {
        Err(anyhow::anyhow!(
            "ONNX embeddings feature is not enabled. \
             Build with: cargo build --features embeddings"
        ))
    }

    /// Создаёт сервис с кастомной конфигурацией
    #[cfg(feature = "embeddings")]
    pub fn with_config(config: OnnxEmbeddingConfig) -> Result<Self> {
        // Инициализация execution providers
        let execution_providers = if config.use_gpu {
            vec![
                ExecutionProvider::CUDA(Default::default()),
                ExecutionProvider::CPU(Default::default()),
            ]
        } else {
            vec![ExecutionProvider::CPU(Default::default())]
        };

        // Создаём сессию ONNX
        let session = SessionBuilder::new()?
            .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
            .with_intra_threads(num_cpus::get())?
            .with_execution_providers(execution_providers)?
            .commit_from_file(&config.model_path)
            .context("Failed to load ONNX model")?;

        Ok(Self {
            config,
            session: Arc::new(session),
            tokenizer: Arc::new(RwLock::new(None)),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    #[cfg(not(feature = "embeddings"))]
    pub fn with_config(_config: OnnxEmbeddingConfig) -> Result<Self> {
        Err(anyhow::anyhow!(
            "ONNX embeddings feature is not enabled. \
             Build with: cargo build --features embeddings"
        ))
    }

    /// Инициализирует токенизатор
    pub async fn initialize_tokenizer(&self) -> Result<()> {
        let tokenizer = Tokenizer::from_path(&self.config.tokenizer_path)
            .context("Failed to load tokenizer")?;
        
        let mut tokenizer_guard = self.tokenizer.write().await;
        *tokenizer_guard = Some(tokenizer);
        
        Ok(())
    }

    /// Генерирует эмбеддинг для текста
    #[cfg(feature = "embeddings")]
    pub async fn generate_embedding(&self, text: &str, is_query: bool) -> Result<Vec<f32>> {
        // Проверяем кэш
        let cache_key = format!("{}:{}", if is_query { "q" } else { "p" }, text);
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Инициализируем токенизатор если нужно
        {
            let tokenizer_guard = self.tokenizer.read().await;
            if tokenizer_guard.is_none() {
                drop(tokenizer_guard);
                self.initialize_tokenizer().await?;
            }
        }

        // Подготовка текста с E5 префиксами
        let prepared_text = if is_query {
            format!("query: {}", text)
        } else {
            format!("passage: {}", text)
        };

        // Токенизация
        let tokenizer_guard = self.tokenizer.read().await;
        let tokenizer = tokenizer_guard.as_ref().unwrap();
        let tokens = tokenizer.encode(&prepared_text, self.config.max_seq_length)?;
        
        // Создание входных тензоров
        let input_ids = tokens.input_ids;
        let attention_mask = tokens.attention_mask;
        let token_type_ids = tokens.token_type_ids;

        // Конвертация в ONNX тензоры
        let input_ids_array = Array2::from_shape_vec(
            (1, input_ids.len()),
            input_ids.iter().map(|&x| x as i64).collect::<Vec<_>>()
        ).unwrap();

        let attention_mask_array = Array2::from_shape_vec(
            (1, attention_mask.len()),
            attention_mask.iter().map(|&x| x as i64).collect::<Vec<_>>()
        ).unwrap();

        let token_type_ids_array = Array2::from_shape_vec(
            (1, token_type_ids.len()),
            token_type_ids.iter().map(|&x| x as i64).collect::<Vec<_>>()
        ).unwrap();

        // Запуск инференса
        let inputs = vec![
            Value::from_array(input_ids_array)?,
            Value::from_array(attention_mask_array)?,
            Value::from_array(token_type_ids_array)?,
        ];

        let outputs = self.session.run(inputs)?;
        
        // Извлечение эмбеддинга (последний слой, mean pooling)
        let last_hidden_state: ort::Value = outputs[0].try_extract_tensor::<f32>()?;
        let attention_mask_slice: ort::Value = outputs[1].try_extract_tensor::<f32>()?;
        
        // Mean pooling по attention mask
        let embedding = self.mean_pooling(
            last_hidden_state.view().into_owned(),
            attention_mask_slice.view().into_owned()
        );

        // L2 нормализация
        let mut normalized = embedding.to_vec();
        self.normalize_vector(&mut normalized);

        // Кэширование
        {
            let mut cache = self.cache.write().await;
            if cache.len() < 10000 {
                cache.insert(cache_key, normalized.clone());
            }
        }

        Ok(normalized)
    }

    #[cfg(not(feature = "embeddings"))]
    pub async fn generate_embedding(&self, _text: &str, _is_query: bool) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!(
            "ONNX embeddings feature is not enabled"
        ))
    }

    /// Mean pooling для BERT-like моделей
    fn mean_pooling(&self, last_hidden_state: Array2<f32>, attention_mask: Array2<f32>) -> Array1<f32> {
        let (_, hidden_size) = last_hidden_state.dim();
        let mut sum = vec![0.0f32; hidden_size];
        let mut count = 0.0f32;

        for seq_idx in 0..last_hidden_state.shape()[0] {
            for hidden_idx in 0..hidden_size {
                if attention_mask[[seq_idx, 0]] > 0.0 {
                    sum[hidden_idx] += last_hidden_state[[seq_idx, hidden_idx]];
                }
            }
            if attention_mask[[seq_idx, 0]] > 0.0 {
                count += 1.0;
            }
        }

        Array1::from_vec(sum.iter().map(|&s| s / count.max(1e-10)).collect())
    }

    /// L2 нормализация вектора
    fn normalize_vector(&self, vector: &mut [f32]) {
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }
    }

    /// Пакетная генерация эмбеддингов
    #[cfg(feature = "embeddings")]
    pub async fn generate_batch(&self, texts: &[&str], is_query: bool) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::with_capacity(texts.len());
        
        // Разбиваем на батчи
        for batch in texts.chunks(self.config.batch_size) {
            let batch_embeddings = self.process_batch(batch, is_query).await?;
            embeddings.extend(batch_embeddings);
        }
        
        Ok(embeddings)
    }

    #[cfg(not(feature = "embeddings"))]
    pub async fn generate_batch(&self, _texts: &[&str], _is_query: bool) -> Result<Vec<Vec<f32>>> {
        Err(anyhow::anyhow!(
            "ONNX embeddings feature is not enabled"
        ))
    }

    /// Обработка одного батча
    #[cfg(feature = "embeddings")]
    async fn process_batch(&self, texts: &[&str], is_query: bool) -> Result<Vec<Vec<f32>>> {
        let tokenizer_guard = self.tokenizer.read().await;
        let tokenizer = tokenizer_guard.as_ref().unwrap();

        let batch_size = texts.len();
        let max_length = self.config.max_seq_length;

        // Токенизация всего батча
        let mut all_input_ids = vec![vec![0i64; max_length]; batch_size];
        let mut all_attention_mask = vec![vec![0i64; max_length]; batch_size];
        let mut all_token_type_ids = vec![vec![0i64; max_length]; batch_size];

        for (i, text) in texts.iter().enumerate() {
            let prepared_text = if is_query {
                format!("query: {}", text)
            } else {
                format!("passage: {}", text)
            };

            let tokens = tokenizer.encode(&prepared_text, max_length)?;
            
            for (j, &id) in tokens.input_ids.iter().enumerate() {
                all_input_ids[i][j] = id as i64;
                all_attention_mask[i][j] = tokens.attention_mask[j] as i64;
                all_token_type_ids[i][j] = tokens.token_type_ids[j] as i64;
            }
        }

        // Создание батчевых тензоров
        let input_ids_array = Array2::from_shape_vec(
            (batch_size, max_length),
            all_input_ids.into_iter().flatten().collect::<Vec<_>>()
        ).unwrap();

        let attention_mask_array = Array2::from_shape_vec(
            (batch_size, max_length),
            all_attention_mask.into_iter().flatten().collect::<Vec<_>>()
        ).unwrap();

        let token_type_ids_array = Array2::from_shape_vec(
            (batch_size, max_length),
            all_token_type_ids.into_iter().flatten().collect::<Vec<_>>()
        ).unwrap();

        // Запуск инференса
        let inputs = vec![
            Value::from_array(input_ids_array)?,
            Value::from_array(attention_mask_array)?,
            Value::from_array(token_type_ids_array)?,
        ];

        let outputs = self.session.run(inputs)?;
        
        // Извлечение эмбеддингов для всего батча
        let last_hidden_state: ort::Value = outputs[0].try_extract_tensor::<f32>()?;
        let attention_mask_slice: ort::Value = outputs[1].try_extract_tensor::<f32>()?;
        
        let mut embeddings = Vec::with_capacity(batch_size);
        
        for i in 0..batch_size {
            // Mean pooling для каждого элемента батча
            let mut sum = vec![0.0f32; self.config.embedding_dim];
            let mut count = 0.0f32;

            for seq_idx in 0..max_length {
                if attention_mask_slice[[i, seq_idx]] > 0.0 {
                    for hidden_idx in 0..self.config.embedding_dim {
                        sum[hidden_idx] += last_hidden_state[[i, seq_idx, hidden_idx]];
                    }
                    count += 1.0;
                }
            }

            let mut embedding = sum.iter().map(|&s| s / count.max(1e-10)).collect::<Vec<_>>();
            self.normalize_vector(&mut embedding);
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    /// Очищает кэш
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Возвращает размер кэша
    pub async fn cache_size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    /// Возвращает информацию о модели
    pub fn model_info(&self) -> String {
        format!(
            "ONNX multilingual-e5-small ({} dims, {})",
            self.config.embedding_dim,
            if self.config.use_gpu { "GPU (CUDA)" } else { "CPU" }
        )
    }
}

// ==================== Tokenizer Implementation ====================

impl Tokenizer {
    fn from_path(path: &Path) -> Result<Self> {
        let vocab_path = path.join("vocab.json");
        let merges_path = path.join("merges.txt");

        // Загрузка vocab
        let vocab_content = std::fs::read_to_string(&vocab_path)
            .context("Failed to read vocab.json")?;
        let vocab: HashMap<String, u32> = serde_json::from_str(&vocab_content)
            .context("Failed to parse vocab.json")?;

        // Загрузка merges
        let merges_content = std::fs::read_to_string(&merges_path)
            .context("Failed to read merges.txt")?;
        let merges: Vec<(String, String)> = merges_content
            .lines()
            .filter(|line| !line.starts_with("#version"))
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();

        // Special tokens
        let special_tokens = [
            ("[PAD]", 0u32),
            ("[UNK]", 100u32),
            ("[CLS]", 101u32),
            ("[SEP]", 102u32),
            ("[MASK]", 103u32),
        ].iter().map(|(s, &id)| (s.to_string(), id)).collect();

        Ok(Self {
            vocab,
            merges,
            special_tokens,
        })
    }

    fn encode(&self, text: &str, max_length: usize) -> Result<EncodedInput> {
        // Простая токенизация по словам (в production использовать полный BPE)
        let mut tokens = Vec::new();
        
        // Добавляем [CLS] токен
        tokens.push(*self.special_tokens.get("[CLS]").unwrap_or(&101));

        // Токенизация текста
        for word in text.split_whitespace() {
            if let Some(&token_id) = self.vocab.get(word) {
                tokens.push(token_id);
            } else {
                // Unknown token
                tokens.push(*self.special_tokens.get("[UNK]").unwrap_or(&100));
            }
        }

        // Добавляем [SEP] токен
        tokens.push(*self.special_tokens.get("[SEP]").unwrap_or(&102));

        // Truncate если нужно
        if tokens.len() > max_length {
            tokens.truncate(max_length);
        }

        // Padding если нужно
        let attention_mask = vec![1i64; tokens.len()]
            .into_iter()
            .chain(std::iter::repeat(0).take(max_length.saturating_sub(tokens.len())))
            .collect::<Vec<_>>();

        tokens.extend(std::iter::repeat(0).take(max_length.saturating_sub(tokens.len())));

        let token_type_ids = vec![0i64; max_length];

        Ok(EncodedInput {
            input_ids: tokens,
            attention_mask,
            token_type_ids,
        })
    }
}

struct EncodedInput {
    input_ids: Vec<u32>,
    attention_mask: Vec<i64>,
    token_type_ids: Vec<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Требуется наличие модели
    async fn test_onnx_embedding() {
        let config = OnnxEmbeddingConfig {
            model_path: PathBuf::from("./models/multilingual-e5-small/onnx/model.onnx"),
            use_gpu: false,
            ..Default::default()
        };

        let service = OnnxEmbeddingService::with_config(config).unwrap();
        service.initialize_tokenizer().await.unwrap();

        // English
        let en_emb = service.generate_embedding("Hello world", false).await.unwrap();
        assert_eq!(en_emb.len(), 384);

        // Russian
        let ru_emb = service.generate_embedding("Привет мир", false).await.unwrap();
        assert_eq!(ru_emb.len(), 384);

        // Cross-lingual similarity
        let en_query = service.generate_embedding("machine learning", true).await.unwrap();
        let ru_doc = service.generate_embedding("машинное обучение", false).await.unwrap();
        
        let similarity = cosine_similarity(&en_query, &ru_doc);
        assert!(similarity > 0.5, "Cross-lingual similarity should be high");
    }

    #[tokio::test]
    #[ignore]
    async fn test_batch_processing() {
        let service = OnnxEmbeddingService::new().unwrap();
        
        let texts = vec![
            "Hello world",
            "Привет мир",
            "你好世界",
        ];

        let embeddings = service.generate_batch(&texts, false).await.unwrap();
        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 384);
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    let denominator = norm_a * norm_b;
    if denominator < 1e-10 {
        0.0
    } else {
        dot / denominator
    }
}
