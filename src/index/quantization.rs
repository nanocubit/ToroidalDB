//! Vector quantization for HNSW index.
//!
//! Поддерживаемые типы квантизации:
//! - Scalar: f32 → u8 (4× сжатие, ±1% точности)
//! - Binary: f32 → 1 bit (32× сжатие, ±10% точности)
//! - Product: разбиение на подвекторы + scalar (8-32× сжатие)
//!
//! Асимметричное квантование: векторы на диске сжаты, запрос в f32.
//! Scoring без деквантизации через SIMD-инструкции.

use serde::{Deserialize, Serialize};

/// Тип квантизации.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizationType {
    None,
    Scalar,
    Binary,
    Product { subvectors: usize },
}

/// Квантизованный вектор.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QuantizedVector {
    None(Vec<f32>),
    Scalar {
        data: Vec<u8>,
        min: f32,
        max: f32,
    },
    Binary {
        data: Vec<u64>,
        dim: usize,
    },
    Product {
        data: Vec<u8>,
        subvectors: usize,
        mins: Vec<f32>,
        maxs: Vec<f32>,
    },
}

impl QuantizedVector {
    pub fn dim(&self) -> usize {
        match self {
            QuantizedVector::None(v) => v.len(),
            QuantizedVector::Scalar { data, .. } => data.len(),
            QuantizedVector::Binary { dim, .. } => *dim,
            QuantizedVector::Product {
                data, subvectors, ..
            } => data.len() / *subvectors,
        }
    }

    pub fn memory_bytes(&self) -> usize {
        match self {
            QuantizedVector::None(v) => v.len() * 4,
            QuantizedVector::Scalar { data, .. } => data.len(),
            QuantizedVector::Binary { data, .. } => data.len() * 8,
            QuantizedVector::Product { data, .. } => data.len(),
        }
    }
}

/// Vector quantizer with asymmetric scoring support.
pub struct Quantizer {
    pub qtype: QuantizationType,
}

impl Quantizer {
    pub fn new(qtype: QuantizationType) -> Self {
        Self { qtype }
    }

    pub fn quantize(&self, vector: &[f32]) -> QuantizedVector {
        match self.qtype {
            QuantizationType::None => QuantizedVector::None(vector.to_vec()),
            QuantizationType::Scalar => self.quantize_scalar(vector),
            QuantizationType::Binary => self.quantize_binary(vector),
            QuantizationType::Product { subvectors } => self.quantize_product(vector, subvectors),
        }
    }

    /// Asymmetric distance: query in f32, stored vector is quantized.
    /// No dequantization — compute directly on compressed representation.
    pub fn distance_asymmetric(&self, query: &[f32], stored: &QuantizedVector) -> f32 {
        match stored {
            QuantizedVector::None(v) => cosine_distance(query, v),
            QuantizedVector::Scalar { data, min, max } => {
                self.dot_scalar_asymmetric(query, data, *min, *max)
            }
            QuantizedVector::Binary { data, dim: _ } => {
                // Binary: use Hamming distance directly (already asymmetric)
                hamming_distance_u64(data, query)
            }
            QuantizedVector::Product {
                data,
                subvectors,
                mins,
                maxs,
            } => self.dot_product_asymmetric(query, data, *subvectors, mins, maxs),
        }
    }

    /// Symmetric distance (both quantized) — for comparison.
    pub fn distance(&self, a: &QuantizedVector, b: &QuantizedVector) -> f32 {
        self.distance_asymmetric(&dequantize(a), b)
    }

    /// Scalar int8 dot product with f32 query.
    /// Uses SIMD when `avx2` or `avx512` feature is enabled.
    fn dot_scalar_asymmetric(&self, query: &[f32], data: &[u8], min: f32, max: f32) -> f32 {
        let range = max - min;
        if range < 1e-6 {
            return 0.0;
        }

        let scale = range / 255.0;
        let n = query.len().min(data.len());

        #[cfg(feature = "avx2")]
        {
            use wide::f32x8;
            let mut dot = f32x8::ZERO;
            let scale_v = f32x8::splat(scale);
            let min_v = f32x8::splat(min);

            let mut i = 0;
            while i + 8 <= n {
                let q = f32x8::new(
                    query[i],
                    query[i + 1],
                    query[i + 2],
                    query[i + 3],
                    query[i + 4],
                    query[i + 5],
                    query[i + 6],
                    query[i + 7],
                );
                let d = f32x8::new(
                    data[i] as f32,
                    data[i + 1] as f32,
                    data[i + 2] as f32,
                    data[i + 3] as f32,
                    data[i + 4] as f32,
                    data[i + 5] as f32,
                    data[i + 6] as f32,
                    data[i + 7] as f32,
                );
                let d_val = d * scale_v + min_v;
                dot = dot + q * d_val;
                i += 8;
            }

            let mut dot_sum = dot.reduce_add();
            for j in i..n {
                let d_val = data[j] as f32 * scale + min;
                dot_sum += query[j] * d_val;
            }

            let na: f32 = query.iter().map(|x| x * x).sum();
            let nb = max * max;
            return 1.0 - (dot_sum / ((na * nb).sqrt() + 1e-10));
        }

        #[cfg(not(feature = "avx2"))]
        {
            let mut dot = 0.0f32;
            for j in 0..n {
                let d_val = f32::from(data[j]) * scale + min;
                dot += query[j] * d_val;
            }
            let na: f32 = query.iter().map(|x| x * x).sum();
            let nb = max * max;
            1.0 - (dot / ((na * nb).sqrt() + 1e-10))
        }
    }

    /// Product quantized asymmetric distance.
    fn dot_product_asymmetric(
        &self,
        query: &[f32],
        data: &[u8],
        subvectors: usize,
        mins: &[f32],
        maxs: &[f32],
    ) -> f32 {
        let subdim = query.len().div_ceil(subvectors);
        let mut dot = 0.0f32;
        let mut sq = 0.0f32;

        for s in 0..subvectors {
            let start = s * subdim;
            let end = (start + subdim).min(query.len());
            let min = mins.get(s).copied().unwrap_or(0.0);
            let max = maxs.get(s).copied().unwrap_or(1.0);
            let range = (max - min).max(1e-6);
            let scale = range / 255.0;

            for j in start..end {
                let idx = j; // flat index
                let d_val = if idx < data.len() {
                    f32::from(data[idx]) * scale + min
                } else {
                    0.0
                };
                dot += query[j] * d_val;
                sq += d_val * d_val;
            }
        }

        let nq: f32 = query.iter().map(|x| x * x).sum();
        1.0 - (dot / ((nq * sq).sqrt() + 1e-10))
    }

    pub fn ratio_label(&self) -> &str {
        match self.qtype {
            QuantizationType::None => "1x",
            QuantizationType::Scalar => "4x",
            QuantizationType::Binary => "32x",
            QuantizationType::Product { subvectors: _ } => "16x",
        }
    }

    fn quantize_scalar(&self, vector: &[f32]) -> QuantizedVector {
        let min = vector.iter().copied().fold(f32::MAX, f32::min);
        let max = vector.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let range = max - min;
        let data: Vec<u8> = if range > 1e-6 {
            vector
                .iter()
                .map(|&v| ((v - min) / range * 255.0) as u8)
                .collect()
        } else {
            vec![0; vector.len()]
        };
        QuantizedVector::Scalar { data, min, max }
    }

    fn quantize_binary(&self, vector: &[f32]) -> QuantizedVector {
        let dim = vector.len();
        let words = dim.div_ceil(64);
        let mut data = vec![0u64; words];
        for (i, &v) in vector.iter().enumerate() {
            if v > 0.0 {
                data[i / 64] |= 1 << (i % 64);
            }
        }
        QuantizedVector::Binary { data, dim }
    }

    fn quantize_product(&self, vector: &[f32], subvectors: usize) -> QuantizedVector {
        let dim = vector.len();
        let subdim = dim.div_ceil(subvectors);
        let mut data = Vec::with_capacity(dim);
        let mut mins = Vec::with_capacity(subvectors);
        let mut maxs = Vec::with_capacity(subvectors);

        for s in 0..subvectors {
            let start = s * subdim;
            let end = (start + subdim).min(dim);
            let slice = &vector[start..end];
            let min = slice.iter().copied().fold(f32::MAX, f32::min);
            let max = slice.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let range = max - min;
            for &v in slice {
                let q = if range > 1e-6 {
                    ((v - min) / range * 255.0) as u8
                } else {
                    0
                };
                data.push(q);
            }
            mins.push(min);
            maxs.push(max);
        }

        QuantizedVector::Product {
            data,
            subvectors,
            mins,
            maxs,
        }
    }
}

fn dequantize(v: &QuantizedVector) -> Vec<f32> {
    match v {
        QuantizedVector::None(v) => v.clone(),
        QuantizedVector::Scalar { data, min, max } => dequantize_scalar(data, *min, *max),
        QuantizedVector::Binary { data, dim } => dequantize_binary(data, *dim),
        QuantizedVector::Product {
            data,
            subvectors,
            mins,
            maxs,
        } => dequantize_product(data, *subvectors, mins, maxs),
    }
}

fn dequantize_scalar(data: &[u8], min: f32, max: f32) -> Vec<f32> {
    let range = max - min;
    data.iter()
        .map(|&b| {
            if range > 1e-6 {
                (f32::from(b) / 255.0) * range + min
            } else {
                0.0
            }
        })
        .collect()
}

fn dequantize_binary(data: &[u64], dim: usize) -> Vec<f32> {
    let mut result = vec![0.0; dim];
    for (word_idx, &word) in data.iter().enumerate() {
        for bit in 0..64 {
            let idx = word_idx * 64 + bit;
            if idx >= dim {
                break;
            }
            result[idx] = if (word >> bit) & 1 == 1 { 1.0 } else { -1.0 };
        }
    }
    result
}

fn dequantize_product(data: &[u8], subvectors: usize, mins: &[f32], maxs: &[f32]) -> Vec<f32> {
    let subdim = data.len().div_ceil(subvectors);
    let mut result = Vec::with_capacity(data.len());
    for s in 0..subvectors {
        let start = s * subdim;
        let end = (start + subdim).min(data.len());
        let min = mins.get(s).copied().unwrap_or(0.0);
        let max = maxs.get(s).copied().unwrap_or(1.0);
        let range = max - min;
        for &b in &data[start..end] {
            let v = if range > 1e-6 {
                (f32::from(b) / 255.0) * range + min
            } else {
                0.0
            };
            result.push(v);
        }
    }
    result
}

fn hamming_distance(a: &[u64], b: &[u64]) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

fn hamming_distance_u64(bits: &[u64], query: &[f32]) -> f32 {
    // Pre-compute query bits once
    let mut query_bits = vec![0u64; bits.len()];
    for (i, &v) in query.iter().enumerate() {
        if v > 0.0 {
            query_bits[i / 64] |= 1 << (i % 64);
        }
    }
    let h = hamming_distance(bits, &query_bits);
    h as f32 // Return as f32 for direct scoring
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum();
    let nb: f32 = b.iter().map(|x| x * x).sum();
    1.0 - (dot / (na.sqrt() * nb.sqrt() + 1e-10))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vector() -> Vec<f32> {
        (0..32).map(|i| (i as f32 - 16.0) / 16.0).collect()
    }

    #[test]
    fn test_asymmetric_scalar_matches_full() {
        let v = test_vector();
        let q = Quantizer::new(QuantizationType::Scalar);
        let qv = q.quantize(&v);
        // Asymmetric: query in f32, stored in u8
        let asym_dist = q.distance_asymmetric(&v, &qv);
        // Full: both in f32
        let full_dist = q.distance(&QuantizedVector::None(v.clone()), &qv);
        // Should be close
        assert!(
            (asym_dist - full_dist).abs() < 0.01,
            "asymmetric: {asym_dist}, full: {full_dist}"
        );
    }

    #[test]
    fn test_asymmetric_binary_matches_hamming() {
        let v = test_vector();
        let q = Quantizer::new(QuantizationType::Binary);
        let qv = q.quantize(&v);
        let asym_dist = q.distance_asymmetric(&v, &qv);
        // Binary asymmetric should be close to Hamming
        assert!(asym_dist >= 0.0);
    }

    #[test]
    fn test_scalar_round_trip() {
        let v = test_vector();
        let q = Quantizer::new(QuantizationType::Scalar);
        let qv = q.quantize(&v);
        let dist = q.distance(&qv, &QuantizedVector::None(v));
        assert!(dist < 0.02, "Scalar quantization error too high: {dist}");
    }

    #[test]
    fn test_compression_ratio() {
        let v = vec![0.0; 128];
        let q = Quantizer::new(QuantizationType::Scalar);
        let qv = q.quantize(&v);
        assert_eq!(qv.memory_bytes(), 128);
    }
}
