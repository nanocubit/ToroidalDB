//! Модуль для топологических индексов и структур данных

use crate::hybrid_storage::Node;
use crate::topology::edges::{HomotopyClass, ToroidalLevel};
use crate::topology::functions::{ricci_curvature, topological_centrality, toroidal_distance};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Топологический индекс для эффективного поиска по гомотопическим классам
pub struct HomotopyClassIndex {
    /// Карта гомотопических классов к спискам узлов
    classes: HashMap<HomotopyClass, Vec<u64>>,
    /// Обратная карта: узел -> его гомотопический класс
    node_to_class: HashMap<u64, HomotopyClass>,
}

impl Default for HomotopyClassIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl HomotopyClassIndex {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
            node_to_class: HashMap::new(),
        }
    }

    /// Добавляет узел в индекс с указанным гомотопическим классом
    pub fn add_node(&mut self, node_id: u64, homotopy_class: HomotopyClass) {
        self.classes
            .entry(homotopy_class.clone())
            .or_default()
            .push(node_id);
        self.node_to_class.insert(node_id, homotopy_class);
    }

    /// Получает узлы с заданным гомотопическим классом
    pub fn get_nodes_by_class(&self, homotopy_class: &HomotopyClass) -> Option<&Vec<u64>> {
        self.classes.get(homotopy_class)
    }

    /// Получает узлы по строковому имени гомотопического класса
    /// (например, "Direct", "Nontrivial", "Wrapped([1, 0])")
    pub fn get_nodes_in_class(&self, class_name: &str) -> Vec<u64> {
        self.classes
            .iter()
            .find(|(class, _)| format!("{class:?}").eq_ignore_ascii_case(class_name))
            .map(|(_, nodes)| nodes.clone())
            .unwrap_or_default()
    }

    /// Обновляет гомотопический класс узла
    pub fn update_node_class(&mut self, node_id: u64, new_class: HomotopyClass) {
        // Удаляем узел из старого класса
        if let Some(old_class) = self.node_to_class.get(&node_id) {
            if let Some(nodes) = self.classes.get_mut(old_class) {
                nodes.retain(|&id| id != node_id);
            }
        }

        // Добавляем узел в новый класс
        self.classes
            .entry(new_class.clone())
            .or_default()
            .push(node_id);
        self.node_to_class.insert(node_id, new_class);
    }
}

/// Индекс для поиска по топологическим характеристикам
pub struct TopologicalFeatureIndex {
    /// Индекс по кривизне Риччи
    ricci_curvature_index: HashMap<u64, f32>,
    /// Индекс по топологической центральности
    centrality_index: HashMap<u64, f32>,
    /// Индекс по гомотопическим классам
    homotopy_index: HomotopyClassIndex,
}

impl Default for TopologicalFeatureIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl TopologicalFeatureIndex {
    pub fn new() -> Self {
        Self {
            ricci_curvature_index: HashMap::new(),
            centrality_index: HashMap::new(),
            homotopy_index: HomotopyClassIndex::new(),
        }
    }

    /// Обновляет индекс для всех узлов
    pub fn update_for_nodes(
        &mut self,
        nodes: &[Node],
        all_nodes: &[Node],
        connection_threshold: f32,
    ) {
        for node in nodes {
            // Вычисляем топологические характеристики
            let ricci = ricci_curvature(node, node, all_nodes, connection_threshold);
            let centrality = topological_centrality(node, all_nodes, connection_threshold);

            // Обновляем индексы
            self.ricci_curvature_index.insert(node.id, ricci);
            self.centrality_index.insert(node.id, centrality);

            // Для гомотопического класса используем упрощенную логику
            // В реальном приложении это будет зависеть от конкретной топологии
            let homotopy_class = HomotopyClass::Direct; // Заглушка
            self.homotopy_index.add_node(node.id, homotopy_class);
        }
    }

    /// Находит узлы с похожей топологической кривизной
    pub fn find_by_ricci_similarity(&self, target_curvature: f32, tolerance: f32) -> Vec<u64> {
        self.ricci_curvature_index
            .iter()
            .filter(|(_, &curvature)| (curvature - target_curvature).abs() <= tolerance)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Находит узлы с высокой топологической центральностью
    pub fn find_by_high_centrality(&self, threshold: f32) -> Vec<u64> {
        self.centrality_index
            .iter()
            .filter(|(_, &centrality)| centrality >= threshold)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Находит узлы с заданным гомотопическим классом
    pub fn find_by_homotopy_class(&self, homotopy_class: &HomotopyClass) -> Vec<u64> {
        self.homotopy_index
            .get_nodes_by_class(homotopy_class)
            .cloned()
            .unwrap_or_default()
    }

    /// Возвращает вектор топологических характеристик узла
    /// (кривизна Риччи, топологическая центральность)
    pub fn get_features(&self, node_id: u64) -> Option<Vec<f32>> {
        let &ricci = self.ricci_curvature_index.get(&node_id)?;
        let &centrality = self.centrality_index.get(&node_id)?;
        Some(vec![ricci, centrality])
    }
}

/// Индекс для тороидального поиска с учетом топологии
pub struct ToroidalTopologyIndex {
    /// Карта уровней (размерностей) к индексам
    levels: HashMap<ToroidalLevel, LevelIndex>,
    /// Общий индекс для быстрого доступа
    global_index: Arc<RwLock<TopologicalFeatureIndex>>,
}

struct LevelIndex {
    /// Хранилище векторов для каждого уровня
    vectors: HashMap<u64, Vec<f32>>,
    /// Индекс ближайших соседей (упрощенная версия)
    neighbors: HashMap<u64, Vec<u64>>,
}

impl Default for ToroidalTopologyIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl ToroidalTopologyIndex {
    pub fn new() -> Self {
        Self {
            levels: HashMap::new(),
            global_index: Arc::new(RwLock::new(TopologicalFeatureIndex::new())),
        }
    }

    /// Добавляет узел в индекс
    pub fn add_node(&mut self, node: &Node, level: ToroidalLevel) {
        let level_index = self.levels.entry(level).or_insert_with(|| LevelIndex {
            vectors: HashMap::new(),
            neighbors: HashMap::new(),
        });

        level_index.vectors.insert(node.id, node.vector.clone());
    }

    /// Обновляет индекс на основе всех узлов
    pub fn update_global_index(&self, all_nodes: &[Node], connection_threshold: f32) {
        let mut index = self.global_index.write().unwrap();
        index.update_for_nodes(all_nodes, all_nodes, connection_threshold);
    }

    /// Выполняет топологический поиск
    pub fn topological_search(
        &self,
        query_node: &Node,
        level: ToroidalLevel,
        threshold: f32,
        all_nodes: &[Node],
    ) -> Vec<(u64, f32)> {
        if let Some(level_index) = self.levels.get(&level) {
            let mut results = Vec::new();

            for (candidate_id, candidate_vector) in &level_index.vectors {
                let distance = toroidal_distance(&query_node.vector, candidate_vector);

                if distance <= threshold {
                    // Проверяем также топологические характеристики
                    if let Some(_candidate_node) = all_nodes.iter().find(|n| n.id == *candidate_id)
                    {
                        // Можно добавить дополнительные топологические проверки
                        results.push((*candidate_id, distance));
                    }
                }
            }

            // Сортируем по расстоянию
            results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            results
        } else {
            Vec::new()
        }
    }

    /// Находит узлы с похожими топологическими характеристиками
    pub fn find_by_topological_similarity(
        &self,
        _target_node_id: u64,
        similarity_metric: TopologicalSimilarity,
    ) -> Vec<u64> {
        let index = self.global_index.read().unwrap();

        match similarity_metric {
            TopologicalSimilarity::RicciCurvature {
                target_value,
                tolerance,
            } => index.find_by_ricci_similarity(target_value, tolerance),
            TopologicalSimilarity::Centrality { min_value } => {
                index.find_by_high_centrality(min_value)
            }
            TopologicalSimilarity::HomotopyClass { class } => index.find_by_homotopy_class(&class),
        }
    }
}

/// Типы топологических схожестей
pub enum TopologicalSimilarity {
    RicciCurvature { target_value: f32, tolerance: f32 },
    Centrality { min_value: f32 },
    HomotopyClass { class: HomotopyClass },
}

/// Структура для управления всеми топологическими индексами
pub struct TopologyManager {
    pub indices: HashMap<String, ToroidalTopologyIndex>,
}

impl Default for TopologyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TopologyManager {
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Получает или создает индекс для конкретной коллекции
    pub fn get_or_create_index(&mut self, collection: &str) -> &mut ToroidalTopologyIndex {
        self.indices
            .entry(collection.to_string())
            .or_insert_with(ToroidalTopologyIndex::new)
    }

    /// Обновляет все индексы
    pub fn update_all_indices(&self, all_nodes: &[Node], connection_threshold: f32) {
        for index in self.indices.values() {
            index.update_global_index(all_nodes, connection_threshold);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_homotopy_class_index() {
        let mut index = HomotopyClassIndex::new();

        let class1 = HomotopyClass::Direct;
        let class2 = HomotopyClass::Wrapped(vec![1, -1]);

        index.add_node(1, class1.clone());
        index.add_node(2, class2.clone());
        index.add_node(3, class1.clone());

        let nodes_class1 = index.get_nodes_by_class(&class1).unwrap();
        assert_eq!(nodes_class1.len(), 2);
        assert!(nodes_class1.contains(&1));
        assert!(nodes_class1.contains(&3));

        let nodes_class2 = index.get_nodes_by_class(&class2).unwrap();
        assert_eq!(nodes_class2.len(), 1);
        assert!(nodes_class2.contains(&2));
    }

    #[test]
    fn test_topological_feature_index() {
        let mut index = TopologicalFeatureIndex::new();

        let node1 = Node {
            id: 1,
            vector: vec![0.1, 0.2],
            properties: json!({}),
            edges: vec![],
        };

        let node2 = Node {
            id: 2,
            vector: vec![0.9, 0.8],
            properties: json!({}),
            edges: vec![],
        };

        let all_nodes = vec![node1.clone(), node2.clone()];
        index.update_for_nodes(&[node1], &all_nodes, 0.5);

        // Проверяем, что узлы были добавлены в индексы
        assert!(index.ricci_curvature_index.contains_key(&1));
        assert!(index.centrality_index.contains_key(&1));
    }

    #[test]
    fn test_toroidal_topology_index() {
        let mut index = ToroidalTopologyIndex::new();

        let node1 = Node {
            id: 1,
            vector: vec![0.1, 0.2],
            properties: json!({}),
            edges: vec![],
        };

        index.add_node(&node1, ToroidalLevel::D384);

        let all_nodes = vec![node1.clone()];
        let results = index.topological_search(&node1, ToroidalLevel::D384, 0.5, &all_nodes);

        assert!(!results.is_empty());
        assert_eq!(results[0].0, 1);
    }
}
