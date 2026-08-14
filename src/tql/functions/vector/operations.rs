pub fn vector_add(vec1: &[f32], vec2: &[f32]) -> Vec<f32> {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }

    vec1.iter().zip(vec2.iter()).map(|(a, b)| a + b).collect()
}

pub fn vector_subtract(vec1: &[f32], vec2: &[f32]) -> Vec<f32> {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }

    vec1.iter().zip(vec2.iter()).map(|(a, b)| a - b).collect()
}

pub fn vector_normalize(vec: &[f32]) -> Vec<f32> {
    let mag = magnitude(vec);
    if mag == 0.0 {
        return vec.to_vec();
    }

    vec.iter().map(|x| x / mag).collect()
}

pub fn vector_magnitude(vec: &[f32]) -> f32 {
    vec.iter().map(|x| x * x).sum::<f32>().sqrt()
}

pub fn vector_scale(vec: &[f32], scalar: f32) -> Vec<f32> {
    vec.iter().map(|x| x * scalar).collect()
}

pub fn vector_multiply(vec1: &[f32], vec2: &[f32]) -> Vec<f32> {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }

    vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).collect()
}

pub fn vector_divide(vec1: &[f32], vec2: &[f32]) -> Vec<f32> {
    if vec1.len() != vec2.len() {
        panic!("Vectors must have the same length");
    }

    vec1.iter()
        .zip(vec2.iter())
        .map(|(a, b)| {
            if *b == 0.0 {
                panic!("Division by zero");
            }
            a / b
        })
        .collect()
}

pub fn vector_negate(vec: &[f32]) -> Vec<f32> {
    vec.iter().map(|x| -x).collect()
}

pub fn vector_abs(vec: &[f32]) -> Vec<f32> {
    vec.iter().map(|x| x.abs()).collect()
}

pub fn vector_clamp(vec: &[f32], min: f32, max: f32) -> Vec<f32> {
    vec.iter()
        .map(|x| {
            if *x < min {
                min
            } else if *x > max {
                max
            } else {
                *x
            }
        })
        .collect()
}

pub fn vector_sum(vecs: &[&[f32]]) -> Vec<f32> {
    if vecs.is_empty() {
        return vec![];
    }

    let len = vecs[0].len();
    for v in vecs.iter() {
        if v.len() != len {
            panic!("All vectors must have the same length");
        }
    }

    let mut result = vec![0.0f32; len];
    for vec in vecs {
        for (i, val) in vec.iter().enumerate() {
            result[i] += val;
        }
    }
    result
}

pub fn vector_mean(vecs: &[&[f32]]) -> Vec<f32> {
    if vecs.is_empty() {
        return vec![];
    }

    let sum = vector_sum(vecs);
    let count = vecs.len() as f32;
    sum.iter().map(|x| x / count).collect()
}

pub fn vector_center(vecs: &[&[f32]]) -> Vec<f32> {
    vector_mean(vecs)
}

fn magnitude(vec: &[f32]) -> f32 {
    vec.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_add() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![4.0, 5.0, 6.0];
        let result = vector_add(&v1, &v2);
        assert_eq!(result, vec![5.0, 7.0, 9.0]);
    }

    #[test]
    fn test_vector_subtract() {
        let v1 = vec![4.0, 5.0, 6.0];
        let v2 = vec![1.0, 2.0, 3.0];
        let result = vector_subtract(&v1, &v2);
        assert_eq!(result, vec![3.0, 3.0, 3.0]);
    }

    #[test]
    fn test_vector_normalize() {
        let v = vec![3.0, 4.0];
        let result = vector_normalize(&v);
        assert!((result[0] - 0.6).abs() < 1e-6);
        assert!((result[1] - 0.8).abs() < 1e-6);
        assert!((vector_magnitude(&result) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_vector_scale() {
        let v = vec![1.0, 2.0, 3.0];
        let result = vector_scale(&v, 2.0);
        assert_eq!(result, vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn test_vector_multiply() {
        let v1 = vec![1.0, 2.0, 3.0];
        let v2 = vec![4.0, 5.0, 6.0];
        let result = vector_multiply(&v1, &v2);
        assert_eq!(result, vec![4.0, 10.0, 18.0]);
    }

    #[test]
    fn test_vector_divide() {
        let v1 = vec![6.0, 10.0, 14.0];
        let v2 = vec![2.0, 5.0, 7.0];
        let result = vector_divide(&v1, &v2);
        assert_eq!(result, vec![3.0, 2.0, 2.0]);
    }

    #[test]
    fn test_vector_negate() {
        let v = vec![1.0, -2.0, 3.0];
        let result = vector_negate(&v);
        assert_eq!(result, vec![-1.0, 2.0, -3.0]);
    }

    #[test]
    fn test_vector_abs() {
        let v = vec![-1.0, 2.0, -3.0];
        let result = vector_abs(&v);
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_vector_clamp() {
        let v = vec![-1.0, 0.5, 2.0];
        let result = vector_clamp(&v, 0.0, 1.0);
        assert_eq!(result, vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_vector_sum() {
        let vecs = vec![&vec![1.0, 2.0], &vec![3.0, 4.0]];
        let result = vector_sum(&vecs);
        assert_eq!(result, vec![4.0, 6.0]);
    }

    #[test]
    fn test_vector_mean() {
        let vecs = vec![&vec![1.0, 3.0], &vec![3.0, 5.0]];
        let result = vector_mean(&vecs);
        assert_eq!(result, vec![2.0, 4.0]);
    }

    #[test]
    fn test_vector_magnitude() {
        let v = vec![3.0, 4.0];
        assert!((vector_magnitude(&v) - 5.0).abs() < 1e-6);
    }

    #[test]
    #[should_panic]
    fn test_add_different_lengths() {
        let v1 = vec![1.0, 2.0];
        let v2 = vec![1.0, 2.0, 3.0];
        vector_add(&v1, &v2);
    }
}
