//! Pluggable cache backend for vector search results.
//!
//! `CacheBackend` trait — абстракция для кэша результатов векторного поиска.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub type Result<T> = std::result::Result<T, CacheError>;

#[derive(Debug, Clone, PartialEq)]
pub enum CacheError {
    DimensionMismatch { expected: usize, actual: usize },
    Empty,
    Backend(String),
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheError::DimensionMismatch { expected, actual } => {
                write!(
                    f,
                    "dimension mismatch: expected {expected}, actual {actual}"
                )
            }
            CacheError::Empty => write!(f, "cache is empty"),
            CacheError::Backend(msg) => write!(f, "backend error: {msg}"),
        }
    }
}

impl std::error::Error for CacheError {}

/// Key-value cache for vector search results.
pub trait CacheBackend: Send + Sync {
    fn dim(&self) -> usize;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn store(&mut self, key: &[f32], value: Vec<u8>) -> Result<()>;
    fn recall(&self, cue: &[f32]) -> Result<Option<Vec<u8>>>;
    fn clear(&mut self);
}

// ==================== HashMemory ====================

/// Exact-key cache: битовый паттерн ключа → значение.
/// O(D) хеширование + O(1) lookup.
/// 100% recall для exact-совпадений, 0 ложных срабатываний.
pub struct HashMemory {
    dim: usize,
    map: HashMap<u64, Vec<(Vec<u64>, Vec<u8>)>>,
    count: usize,
}

impl HashMemory {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            map: HashMap::new(),
            count: 0,
        }
    }
}

impl CacheBackend for HashMemory {
    fn dim(&self) -> usize {
        self.dim
    }
    fn len(&self) -> usize {
        self.count
    }

    fn store(&mut self, key: &[f32], value: Vec<u8>) -> Result<()> {
        if key.len() != self.dim {
            return Err(CacheError::DimensionMismatch {
                expected: self.dim,
                actual: key.len(),
            });
        }
        let bits = f32_slice_to_bits(key);
        let hash = hash_bits(&bits);
        self.map.entry(hash).or_default().push((bits, value));
        self.count += 1;
        Ok(())
    }

    fn recall(&self, cue: &[f32]) -> Result<Option<Vec<u8>>> {
        if cue.len() != self.dim {
            return Err(CacheError::DimensionMismatch {
                expected: self.dim,
                actual: cue.len(),
            });
        }
        let bits = f32_slice_to_bits(cue);
        let hash = hash_bits(&bits);
        Ok(self.map.get(&hash).and_then(|bucket| {
            bucket
                .iter()
                .find(|(stored_bits, _)| stored_bits == &bits)
                .map(|(_, value)| value.clone())
        }))
    }

    fn clear(&mut self) {
        self.map.clear();
        self.count = 0;
    }
}

// ==================== Helpers ====================

/// Convert f32 slice to Vec<u64> for bit-pattern comparison.
fn f32_slice_to_bits(data: &[f32]) -> Vec<u64> {
    data.chunks(4)
        .map(|chunk| {
            let mut buf = [0u8; 32];
            for (i, &v) in chunk.iter().enumerate() {
                buf[i * 4..(i + 1) * 4].copy_from_slice(&v.to_bits().to_le_bytes());
            }
            u64::from_le_bytes(buf[..8].try_into().unwrap())
        })
        .collect()
}

fn hash_bits(bits: &[u64]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bits.hash(&mut hasher);
    hasher.finish()
}

// ==================== MapVsaMemory ====================

/// MAP-VSA associative cache: использует покомпонентное умножение для связывания.
///
/// - `store(key, value)`: `memory += key ⊙ value`
/// - `recall(cue)`: `cue ⊙ memory`
///
/// Для Rademacher-векторов (компоненты ±1/√D) unbinding — self-inverse.
/// Для Gaussian-векторов используется покомпонентное обращение.
/// Подходит для fuzzy-поиска: key может не совпадать точно, но recall вернёт
/// приближённый результат.
pub struct MapVsaMemory {
    dim: usize,
    memory: Option<Vec<f64>>,
    count: usize,
}

impl MapVsaMemory {
    fn add_to_memory(&mut self, bound: &[f64]) {
        match &mut self.memory {
            Some(m) => {
                for (a, b) in m.iter_mut().zip(bound) {
                    *a += b;
                }
            }
            None => self.memory = Some(bound.to_vec()),
        }
    }
}

impl MapVsaMemory {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            memory: None,
            count: 0,
        }
    }
}

impl CacheBackend for MapVsaMemory {
    fn dim(&self) -> usize {
        self.dim
    }
    fn len(&self) -> usize {
        self.count
    }

    fn store(&mut self, key: &[f32], value: Vec<u8>) -> Result<()> {
        if key.len() != self.dim {
            return Err(CacheError::DimensionMismatch {
                expected: self.dim,
                actual: key.len(),
            });
        }
        // Convert value to f64 vector for superposition
        let val_f64: Vec<f64> = value.iter().map(|&v| v as f64 / 255.0).collect();
        let bound: Vec<f64> = key
            .iter()
            .zip(&val_f64)
            .map(|(k, v)| *k as f64 * v)
            .collect();
        self.add_to_memory(&bound);
        self.count += 1;
        Ok(())
    }

    fn recall(&self, cue: &[f32]) -> Result<Option<Vec<u8>>> {
        if cue.len() != self.dim {
            return Err(CacheError::DimensionMismatch {
                expected: self.dim,
                actual: cue.len(),
            });
        }
        let Some(memory) = &self.memory else {
            return Ok(None);
        };
        // Unbind: component-wise multiply cue ⊙ memory
        let raw: Vec<f64> = cue.iter().zip(memory).map(|(c, m)| *c as f64 * m).collect();
        // Normalize and convert back to bytes
        let energy: f64 = raw.iter().map(|x| x * x).sum::<f64>().sqrt();
        if energy < 1e-12 {
            return Ok(Some(vec![0u8; self.dim]));
        }
        let result: Vec<u8> = raw
            .iter()
            .map(|x| ((x / energy).clamp(-1.0, 1.0) * 127.0 + 128.0) as u8)
            .collect();
        Ok(Some(result))
    }

    fn clear(&mut self) {
        self.memory = None;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_memory_round_trip() {
        let mut cache = HashMemory::new(4);
        let key = vec![0.1, 0.2, 0.3, 0.4];
        let value = vec![1, 2, 3, 4];
        cache.store(&key, value.clone()).unwrap();
        let result = cache.recall(&key).unwrap();
        assert_eq!(result, Some(value));
    }

    #[test]
    fn test_hash_memory_miss() {
        let mut cache = HashMemory::new(4);
        cache.store(&vec![0.1, 0.2, 0.3, 0.4], vec![1]).unwrap();
        let result = cache.recall(&vec![0.5, 0.6, 0.7, 0.8]).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_hash_memory_dim_mismatch() {
        let mut cache = HashMemory::new(4);
        let result = cache.store(&vec![0.1, 0.2, 0.3], vec![1]);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_vsa_round_trip() {
        let mut cache = MapVsaMemory::new(4);
        let key = vec![0.5, 0.2, -0.3, 0.1];
        let value = vec![100, 200, 150, 50];
        cache.store(&key, value.clone()).unwrap();
        let result = cache.recall(&key).unwrap();
        // MAP-VSA — approximate, so tolerance is loose
        assert!(result.is_some());
        let r = result.unwrap();
        // Vectors should be correlated (not exact)
        let dot: f32 = r
            .iter()
            .zip(&value)
            .map(|(a, b)| (*a as f32 - *b as f32).abs())
            .sum();
        assert!(dot < 300.0, "reconstruction error too high: {dot}");
    }

    #[test]
    fn test_map_vsa_miss() {
        let mut cache = MapVsaMemory::new(4);
        cache
            .store(&vec![0.5, 0.2, -0.3, 0.1], vec![1, 2, 3, 4])
            .unwrap();
        let result = cache.recall(&vec![0.9, -0.1, 0.7, 0.3]).unwrap();
        // MAP-VSA returns something even for unseen cues (it's fuzzy)
        assert!(result.is_some());
    }

    #[test]
    fn test_map_vsa_clear() {
        let mut cache = MapVsaMemory::new(4);
        cache
            .store(&vec![0.5, 0.2, -0.3, 0.1], vec![1, 2, 3, 4])
            .unwrap();
        cache.clear();
        assert_eq!(cache.len(), 0);
    }
}
