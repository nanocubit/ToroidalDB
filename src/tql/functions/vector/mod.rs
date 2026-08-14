pub mod comparison;
pub mod operations;

pub use comparison::*;
pub use operations::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
    pub data: Vec<f32>,
    pub dimension: usize,
}

impl Vector {
    pub fn new(data: Vec<f32>) -> Self {
        let dimension = data.len();
        Vector { data, dimension }
    }

    pub fn cosine_similarity(&self, other: &Vector) -> f32 {
        comparison::cosine_similarity(&self.data, &other.data)
    }

    pub fn euclidean_distance(&self, other: &Vector) -> f32 {
        comparison::euclidean_distance(&self.data, &other.data)
    }

    pub fn manhattan_distance(&self, other: &Vector) -> f32 {
        comparison::manhattan_distance(&self.data, &other.data)
    }

    pub fn add(&self, other: &Vector) -> Vector {
        Vector::new(operations::vector_add(&self.data, &other.data))
    }

    pub fn subtract(&self, other: &Vector) -> Vector {
        Vector::new(operations::vector_subtract(&self.data, &other.data))
    }

    pub fn normalize(&self) -> Vector {
        Vector::new(operations::vector_normalize(&self.data))
    }

    pub fn magnitude(&self) -> f32 {
        operations::vector_magnitude(&self.data)
    }

    pub fn scale(&self, scalar: f32) -> Vector {
        Vector::new(operations::vector_scale(&self.data, scalar))
    }
}

impl From<Vec<f32>> for Vector {
    fn from(data: Vec<f32>) -> Self {
        Vector::new(data)
    }
}

impl From<&[f32]> for Vector {
    fn from(data: &[f32]) -> Self {
        Vector::new(data.to_vec())
    }
}
