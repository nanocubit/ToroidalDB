use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatryoshkaDim {
    D384,
    D768,
    D1024,
    D1536,
}

impl MatryoshkaDim {
    pub fn size(&self) -> usize {
        match self {
            MatryoshkaDim::D384 => 384,
            MatryoshkaDim::D768 => 768,
            MatryoshkaDim::D1024 => 1024,
            MatryoshkaDim::D1536 => 1536,
        }
    }

    pub fn from_size(size: usize) -> Self {
        match size {
            384 => MatryoshkaDim::D384,
            768 => MatryoshkaDim::D768,
            1024 => MatryoshkaDim::D1024,
            1536 => MatryoshkaDim::D1536,
            _ => {
                // Find closest dimension
                let dimensions = [384, 768, 1024, 1536];
                let closest = dimensions
                    .iter()
                    .min_by_key(|&&d| (d as i32 - size as i32).abs())
                    .unwrap_or(&384);
                MatryoshkaDim::from_size(*closest)
            }
        }
    }
}

pub fn toroidal_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = (x - y).abs();
            let toroidal = diff.min(1.0 - diff);
            toroidal * toroidal
        })
        .sum::<f32>()
        .sqrt()
}

pub fn pad_or_truncate(vec: &[f32], target_len: usize) -> Vec<f32> {
    if vec.len() >= target_len {
        vec[..target_len].to_vec()
    } else {
        let mut padded = vec.to_vec();
        padded.resize(target_len, 0.0);
        padded
    }
}
