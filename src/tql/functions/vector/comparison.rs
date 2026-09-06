use std::iter::Sum;

#[cfg(feature = "simd")]
use std::arch::x86_64::*;

pub fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert!(
        vec1.len() == vec2.len(),
        "Vectors must have the same length"
    );
    if vec1.is_empty() {
        return 0.0;
    }

    dot_product(vec1, vec2) / (magnitude(vec1) * magnitude(vec2))
}

#[cfg(feature = "simd")]
pub fn cosine_similarity_simd(vec1: &[f32], vec2: &[f32]) -> f32 {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }
    if vec1.is_empty() {
        return 0.0;
    }

    let dot = dot_product_simd(vec1, vec2);
    let mag1 = magnitude_simd(vec1);
    let mag2 = magnitude_simd(vec2);

    dot / (mag1 * mag2)
}

pub fn euclidean_distance(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert!(
        vec1.len() == vec2.len(),
        "Vectors must have the same length"
    );

    squared_euclidean_distance(vec1, vec2).sqrt()
}

#[cfg(feature = "simd")]
pub fn euclidean_distance_simd(vec1: &[f32], vec2: &[f32]) -> f32 {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }

    squared_euclidean_distance_simd(vec1, vec2).sqrt()
}

pub fn squared_euclidean_distance(vec1: &[f32], vec2: &[f32]) -> f32 {
    vec1.iter()
        .zip(vec2.iter())
        .map(|(a, b)| (a - b) * (a - b))
        .sum()
}

#[cfg(feature = "simd")]
pub fn squared_euclidean_distance_simd(vec1: &[f32], vec2: &[f32]) -> f32 {
    let len = vec1.len();
    let mut sum = 0.0f32;
    let mut i = 0;

    let chunks = len / 8;
    for _ in 0..chunks {
        let a = unsafe { _mm256_loadu_ps(vec1.as_ptr().add(i)) };
        let b = unsafe { _mm256_loadu_ps(vec2.as_ptr().add(i)) };
        let diff = unsafe { _mm256_sub_ps(a, b) };
        let sq = unsafe { _mm256_mul_ps(diff, diff) };

        let mut result = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result.as_mut_ptr(), sq) };

        sum += result.iter().sum::<f32>();
        i += 8;
    }

    for j in i..len {
        let diff = vec1[j] - vec2[j];
        sum += diff * diff;
    }

    sum
}

pub fn manhattan_distance(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert!(
        vec1.len() == vec2.len(),
        "Vectors must have the same length"
    );

    vec1.iter()
        .zip(vec2.iter())
        .map(|(a, b)| (a - b).abs())
        .sum()
}

#[cfg(feature = "simd")]
pub fn manhattan_distance_simd(vec1: &[f32], vec2: &[f32]) -> f32 {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }

    let len = vec1.len();
    let mut sum = 0.0f32;
    let mut i = 0;

    let chunks = len / 8;
    for _ in 0..chunks {
        let a = unsafe { _mm256_loadu_ps(vec1.as_ptr().add(i)) };
        let b = unsafe { _mm256_loadu_ps(vec2.as_ptr().add(i)) };
        let diff = unsafe { _mm256_sub_ps(a, b) };
        let abs_diff = unsafe { _mm256_andnot_ps(_mm256_set1_ps(-0.0), diff) };

        let mut result = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result.as_mut_ptr(), abs_diff) };

        sum += result.iter().sum::<f32>();
        i += 8;
    }

    for j in i..len {
        sum += (vec1[j] - vec2[j]).abs();
    }

    sum
}

pub fn dot_product(vec1: &[f32], vec2: &[f32]) -> f32 {
    vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum()
}

#[cfg(feature = "simd")]
pub fn dot_product_simd(vec1: &[f32], vec2: &[f32]) -> f32 {
    let len = vec1.len();
    let mut sum = 0.0f32;
    let mut i = 0;

    let chunks = len / 8;
    for _ in 0..chunks {
        let a = unsafe { _mm256_loadu_ps(vec1.as_ptr().add(i)) };
        let b = unsafe { _mm256_loadu_ps(vec2.as_ptr().add(i)) };
        let prod = unsafe { _mm256_mul_ps(a, b) };

        let mut result = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result.as_mut_ptr(), prod) };

        sum += result.iter().sum::<f32>();
        i += 8;
    }

    for j in i..len {
        sum += vec1[j] * vec2[j];
    }

    sum
}

pub fn magnitude(vec: &[f32]) -> f32 {
    vec.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(feature = "simd")]
pub fn magnitude_simd(vec: &[f32]) -> f32 {
    let len = vec.len();
    let mut sum = 0.0f32;
    let mut i = 0;

    let chunks = len / 8;
    for _ in 0..chunks {
        let v = unsafe { _mm256_loadu_ps(vec.as_ptr().add(i)) };
        let sq = unsafe { _mm256_mul_ps(v, v) };

        let mut result = [0.0f32; 8];
        unsafe { _mm256_storeu_ps(result.as_mut_ptr(), sq) };

        sum += result.iter().sum::<f32>();
        i += 8;
    }

    for j in i..len {
        sum += vec[j] * vec[j];
    }

    sum.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < 1e-6);

        let v3 = vec![1.0, 0.0, 0.0];
        let v4 = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&v3, &v4) - 0.0).abs() < 1e-6);

        let v5 = vec![1.0, 1.0];
        let v6 = vec![1.0, 1.0];
        assert!((cosine_similarity(&v5, &v6) - 1.0).abs() < 1e-6);

        let v7 = vec![1.0, 1.0];
        let v8 = vec![-1.0, -1.0];
        assert!((cosine_similarity(&v7, &v8) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distance() {
        let v1 = vec![0.0, 0.0];
        let v2 = vec![3.0, 4.0];
        assert!((euclidean_distance(&v1, &v2) - 5.0).abs() < 1e-6);

        let v3 = vec![1.0, 2.0, 3.0];
        let v4 = vec![4.0, 5.0, 6.0];
        let dist = euclidean_distance(&v3, &v4);
        assert!((dist - 5.196152).abs() < 1e-4);
    }

    #[test]
    fn test_manhattan_distance() {
        let v1 = vec![0.0, 0.0];
        let v2 = vec![3.0, 4.0];
        assert!((manhattan_distance(&v1, &v2) - 7.0).abs() < 1e-6);

        let v3 = vec![1.0, 2.0, 3.0];
        let v4 = vec![4.0, 5.0, 6.0];
        assert!((manhattan_distance(&v3, &v4) - 9.0).abs() < 1e-6);
    }

    #[test]
    fn test_dot_product() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![4.0, 5.0, 6.0];
        assert!((dot_product(&v1, &v2) - 32.0).abs() < 1e-6);
    }

    #[test]
    fn test_magnitude() {
        let v = vec![3.0, 4.0];
        assert!((magnitude(&v) - 5.0).abs() < 1e-6);

        let v2 = vec![1.0, 1.0, 1.0, 1.0];
        assert!((magnitude(&v2) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_similarity_different_lengths() {
        let v1 = vec![1.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        let result = std::panic::catch_unwind(|| cosine_similarity(&v1, &v2));
        assert!(result.is_err());
    }
}
