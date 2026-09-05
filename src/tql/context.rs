//! Context-dependent distance modulation.
//!
//! Модифицирует базовое расстояние между векторами с учётом контекста
//! (роль пользователя, устройство, время суток и т.д.).
//!
//! ```
//! modulated_distance = base_distance * (1.0 + influence)
//! ```
//!
//! Где `influence` — сумма весов активных контекстов из `ContextModulator`.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Ключ контекста для модуляции расстояния.
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContextKey {
    /// Роль пользователя.
    UserRole(String),
    /// Класс устройства.
    DeviceClass(String),
    /// Временной контекст (час дня 0-23).
    HourOfDay(u8),
    /// Произвольный контекст.
    Custom(String),
}

impl ContextKey {
    pub fn user_role(role: &str) -> Self {
        ContextKey::UserRole(role.to_string())
    }
    pub fn device(class: &str) -> Self {
        ContextKey::DeviceClass(class.to_string())
    }
    pub fn hour(h: u8) -> Self {
        ContextKey::HourOfDay(h.min(23))
    }
    pub fn custom(key: &str) -> Self {
        ContextKey::Custom(key.to_string())
    }
}

/// Модулятор расстояния: контекст → вес влияния.
///
/// Вес может быть:
/// - Положительным (увеличивает расстояние → «отталкивает»)
/// - Отрицательным (уменьшает расстояние → «притягивает»)
/// - Нулевым (не влияет)
#[derive(Clone, Debug)]
pub struct ContextModulator {
    weights: DashMap<ContextKey, f32>,
    decay_rate: f32,
}

impl ContextModulator {
    pub fn new(decay_rate: f32) -> Self {
        Self {
            weights: DashMap::new(),
            decay_rate,
        }
    }

    /// Set weight for a context key.
    pub fn set_weight(&self, key: ContextKey, weight: f32) {
        self.weights.insert(key, weight.clamp(-1.0, 1.0));
    }

    /// Get weight for a context key.
    pub fn get_weight(&self, key: &ContextKey) -> Option<f32> {
        self.weights.get(key).as_ref().map(|r| *r.value())
    }

    /// Modulate base distance given active contexts.
    ///
    /// `influence` = sum of weights of active context keys.
    /// Result = `base_dist * (1.0 + influence.clamp(-0.5, 0.5))`.
    pub fn modulate(&self, base_dist: f32, context: &[ContextKey]) -> f32 {
        let influence: f32 = context
            .iter()
            .filter_map(|k| self.weights.get(k))
            .map(|r| *r.value())
            .sum();
        let clamped = influence.clamp(-0.5, 0.5);
        base_dist * (1.0 + clamped)
    }

    /// Apply decay to all weights (called periodically).
    pub fn decay(&self) {
        let rate = self.decay_rate;
        self.weights.retain(|_, w| {
            *w *= (1.0 - rate);
            w.abs() > 0.001
        });
    }

    /// Number of registered context keys.
    pub fn len(&self) -> usize {
        self.weights.len()
    }

    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }
}

impl Default for ContextModulator {
    fn default() -> Self {
        Self::new(0.05)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modulate_no_context() {
        let modul = ContextModulator::default();
        let dist = modul.modulate(0.5, &[]);
        assert!((dist - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_modulate_with_context() {
        let modul = ContextModulator::default();
        modul.set_weight(ContextKey::user_role("admin"), 0.3);
        let dist = modul.modulate(0.5, &[ContextKey::user_role("admin")]);
        assert!((dist - 0.5 * 1.3).abs() < 1e-6);
    }

    #[test]
    fn test_modulate_negative() {
        let modul = ContextModulator::default();
        modul.set_weight(ContextKey::device("mobile"), -0.2);
        let dist = modul.modulate(1.0, &[ContextKey::device("mobile")]);
        assert!((dist - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_decay() {
        let modul = ContextModulator::new(0.5);
        modul.set_weight(ContextKey::custom("test"), 1.0);
        modul.decay();
        let w = modul.get_weight(&ContextKey::custom("test")).unwrap();
        assert!((w - 0.5).abs() < 1e-6);
    }
}
