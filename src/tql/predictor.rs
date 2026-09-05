//! Frequency-based access predictor for prefetch hints.
//!
//! Анализирует паттерны доступа к узлам и предсказывает «горячие» узлы
//! для префетчинга в горячий тир хранения.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// Событие доступа к узлу.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessEvent {
    pub node_id: u64,
    pub timestamp: i64,
    pub hit: bool,
}

/// Предсказание будущего доступа.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessPrediction {
    pub node_id: u64,
    /// Вероятность доступа (0..1).
    pub probability: f32,
}

/// Частотный анализатор доступа.
///
/// Хранит для каждого узла скользящую оценку частоты доступа
/// с экспоненциальным затуханием: `score = (score + 1) * (1 - decay)`.
#[derive(Clone, Debug)]
pub struct AccessPredictor {
    freq: DashMap<u64, (f32, i64)>,
    decay: f32,
    threshold: f32,
}

impl AccessPredictor {
    pub fn new(decay: f32, threshold: f32) -> Self {
        Self {
            freq: DashMap::new(),
            decay: decay.clamp(0.0, 1.0),
            threshold,
        }
    }

    /// Record an access event and update frequency score.
    pub fn record(&self, event: &AccessEvent) {
        let mut entry = self.freq.entry(event.node_id).or_insert((0.0, 0));
        let (score, _) = entry.value_mut();
        *score = (*score + 1.0) * (1.0 - self.decay);
        *score = score.min(100.0); // cap to prevent overflow
    }

    /// Get current frequency score for a node.
    pub fn score(&self, node_id: u64) -> f32 {
        self.freq.get(&node_id).map(|e| e.0).unwrap_or(0.0)
    }

    /// Predict hot nodes above threshold.
    pub fn predict_hot(&self) -> Vec<AccessPrediction> {
        let mut predictions: Vec<AccessPrediction> = self
            .freq
            .iter()
            .filter(|entry| entry.0 > self.threshold)
            .map(|entry| AccessPrediction {
                node_id: entry.key().clone(),
                probability: (entry.0 / 100.0).min(1.0),
            })
            .collect();
        predictions.sort_by(|a, b| b.probability.partial_cmp(&a.probability).unwrap());
        predictions
    }

    /// Number of tracked nodes.
    pub fn len(&self) -> usize {
        self.freq.len()
    }

    pub fn is_empty(&self) -> bool {
        self.freq.is_empty()
    }

    /// Clear all frequency data.
    pub fn clear(&self) {
        self.freq.clear();
    }
}

impl Default for AccessPredictor {
    fn default() -> Self {
        Self::new(0.05, 3.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_score() {
        let pred = AccessPredictor::default();
        pred.record(&AccessEvent {
            node_id: 1,
            timestamp: 0,
            hit: true,
        });
        pred.record(&AccessEvent {
            node_id: 1,
            timestamp: 1,
            hit: true,
        });
        let score = pred.score(1);
        assert!(score > 0.0);
    }

    #[test]
    fn test_predict_hot() {
        let pred = AccessPredictor::new(0.1, 1.0);
        for _ in 0..10 {
            pred.record(&AccessEvent {
                node_id: 1,
                timestamp: 0,
                hit: true,
            });
        }
        let hot = pred.predict_hot();
        assert!(!hot.is_empty());
        assert_eq!(hot[0].node_id, 1);
    }

    #[test]
    fn test_decay_over_time() {
        let pred = AccessPredictor::new(0.5, 0.1);
        pred.record(&AccessEvent {
            node_id: 1,
            timestamp: 0,
            hit: true,
        });
        let score_after = pred.score(1);
        assert!(score_after < 1.0); // decay applied on each record
    }
}
