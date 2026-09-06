//! Графовый визуализатор для `ToroidalDB`
//!
//! Предоставляет встроенный графовый визуализатор с поддержкой:
//! - Интерактивных графов
//! - Топологических свойств
//! - Векторных представлений
//! - Цветовой схемы по меткам

use crate::storage::Node;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphVisualization {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub layout: LayoutAlgorithm,
    pub metadata: VisualizationMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u64,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: String,
    pub properties: Value,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: u64,
    pub target: u64,
    pub label: String,
    pub weight: f32,
    pub color: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutAlgorithm {
    ForceDirected,
    Circular,
    Grid,
    Hierarchical,
    Topological,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationMetadata {
    pub node_count: usize,
    pub edge_count: usize,
    pub density: f32,
    pub clustering_coefficient: f32,
    pub diameter: Option<u32>,
    pub connected_components: usize,
}

pub struct GraphVisualizer {
    store: Arc<crate::storage::PersistentStore>,
}

impl GraphVisualizer {
    pub fn new(store: Arc<crate::storage::PersistentStore>) -> Self {
        Self { store }
    }

    /// Создает визуализацию графа из узлов и ребер
    pub fn create_graph_visualization(
        &self,
        node_ids: &[u64],
        layout: LayoutAlgorithm,
    ) -> Result<GraphVisualization, String> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut all_visited = HashSet::new();

        // Собираем узлы
        for &node_id in node_ids {
            if let Ok(Some(node)) = self.store.get(node_id) {
                let graph_node = self.node_to_graph_node(&node, &layout)?;
                nodes.push(graph_node);
                all_visited.insert(node_id);
            }
        }

        // Собираем ребра
        for node in &nodes {
            if let Ok(Some(original_node)) = self.store.get(node.id) {
                for edge in &original_node.edges {
                    if all_visited.contains(&edge.target_id) {
                        edges.push(GraphEdge {
                            source: node.id,
                            target: edge.target_id,
                            label: edge.relation_type.clone(),
                            weight: edge.weight,
                            color: self.get_edge_color(&edge.relation_type),
                            properties: serde_json::json!({
                                "weight": edge.weight,
                                "type": &edge.relation_type,
                            }),
                        });
                    }
                }
            }
        }

        // Вычисляем метаданные
        let metadata = self.compute_metadata(&nodes, &edges)?;

        Ok(GraphVisualization {
            nodes,
            edges,
            layout,
            metadata,
        })
    }

    /// Преобразует внутренний узел в узел визуализации
    fn node_to_graph_node(
        &self,
        node: &Node,
        layout: &LayoutAlgorithm,
    ) -> Result<GraphNode, String> {
        let (x, y) = self.position_for_layout(node, layout)?;

        Ok(GraphNode {
            id: node.id,
            label: self.get_node_label(node),
            x,
            y,
            size: self.get_node_size(node),
            color: self.get_node_color(node),
            properties: node.properties.clone(),
            vector: node.vector.clone(),
        })
    }

    /// Вычисляет позицию узла в зависимости от выбранного алгоритма
    fn position_for_layout(
        &self,
        node: &Node,
        layout: &LayoutAlgorithm,
    ) -> Result<(f32, f32), String> {
        match layout {
            LayoutAlgorithm::ForceDirected => {
                // Простая реализация - случайное распределение
                // В реальной системе это будет результат силового алгоритма
                let hash = self.hash_node_position(node.id);
                let x = (hash % 1000) as f32 / 1000.0;
                let y = ((hash / 1000) % 1000) as f32 / 1000.0;
                Ok((x, y))
            }
            LayoutAlgorithm::Circular => {
                // Распределение по кругу
                let angle = 2.0 * std::f32::consts::PI * node.id as f32 / 100.0; // Предполагаем 100 узлов
                let x = angle.cos();
                let y = angle.sin();
                Ok((x, y))
            }
            LayoutAlgorithm::Grid => {
                // Распределение по сетке
                let size = (self.store.len().unwrap_or(100) as f32).sqrt() as u64;
                let x = (node.id % size) as f32;
                let y = (node.id / size) as f32;
                Ok((x, y))
            }
            LayoutAlgorithm::Hierarchical => {
                // Иерархическое расположение (простая реализация)
                let level = node.id % 5; // 5 уровней
                let pos_in_level = node.id / 5;
                let x = pos_in_level as f32 * 0.2;
                let y = level as f32 * 0.2;
                Ok((x, y))
            }
            LayoutAlgorithm::Topological => {
                // Топологическое расположение на основе векторных данных
                if node.vector.len() >= 2 {
                    let x = node.vector[0];
                    let y = node.vector[1];
                    Ok((x, y))
                } else {
                    // Если вектор недостаточной размерности, используем хэш
                    let hash = self.hash_node_position(node.id);
                    let x = (hash % 1000) as f32 / 1000.0;
                    let y = ((hash / 1000) % 1000) as f32 / 1000.0;
                    Ok((x, y))
                }
            }
        }
    }

    /// Вычисляет цвет узла на основе его свойств
    fn get_node_color(&self, node: &Node) -> String {
        // Определяем цвет на основе метки или других свойств
        if let Some(label) = node.properties.get("label").and_then(|v| v.as_str()) {
            match label.to_lowercase().as_str() {
                "user" => "#6366f1".to_string(),     // indigo
                "document" => "#10b981".to_string(), // emerald
                "image" => "#f59e0b".to_string(),    // amber
                "video" => "#ef4444".to_string(),    // red
                _ => "#8b5cf6".to_string(),          // violet
            }
        } else {
            // Цвет на основе ID
            let colors = ["#6366f1", "#8b5cf6", "#ec4899", "#f59e0b", "#10b981"];
            let idx = (node.id as usize) % colors.len();
            colors[idx].to_string()
        }
    }

    /// Вычисляет цвет ребра на основе типа отношения
    fn get_edge_color(&self, relation_type: &str) -> String {
        match relation_type.to_lowercase().as_str() {
            "friend" | "follows" => "#6366f1".to_string(), // indigo
            "likes" | "rates" => "#ec4899".to_string(),    // pink
            "connects" | "related" => "#8b5cf6".to_string(), // violet
            "contains" | "has" => "#10b981".to_string(),   // emerald
            _ => "#94a3b8".to_string(),                    // slate
        }
    }

    /// Вычисляет размер узла на основе его свойств
    fn get_node_size(&self, node: &Node) -> f32 {
        // Размер на основе количества ребер или других факторов
        let base_size = 10.0;
        let edge_factor = node.edges.len() as f32 * 0.5;
        let property_factor = if node.properties.is_null() { 0.0 } else { 1.0 };

        base_size + edge_factor + property_factor
    }

    /// Получает метку узла
    fn get_node_label(&self, node: &Node) -> String {
        // Пытаемся получить имя или ID из свойств
        if let Some(name) = node.properties.get("name").and_then(|v| v.as_str()) {
            name.to_string()
        } else if let Some(title) = node.properties.get("title").and_then(|v| v.as_str()) {
            title.to_string()
        } else {
            format!("Node_{}", node.id)
        }
    }

    /// Вычисляет хэш для позиционирования узла
    fn hash_node_position(&self, node_id: u64) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        node_id.hash(&mut hasher);
        hasher.finish()
    }

    /// Вычисляет метаданные визуализации
    fn compute_metadata(
        &self,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
    ) -> Result<VisualizationMetadata, String> {
        let node_count = nodes.len();
        let edge_count = edges.len();

        // Вычисляем плотность графа
        let density = if node_count > 1 {
            (2.0 * edge_count as f32) / ((node_count * (node_count - 1)) as f32)
        } else {
            0.0
        };

        // Вычисляем коэффициент кластеризации (упрощённо)
        let clustering_coefficient = self.approximate_clustering_coefficient(nodes, edges);

        // Вычисляем количество компонентов связности
        let connected_components = self.count_connected_components(nodes, edges);

        Ok(VisualizationMetadata {
            node_count,
            edge_count,
            density,
            clustering_coefficient,
            diameter: None, // Требует сложных вычислений
            connected_components,
        })
    }

    /// Приближённо вычисляет коэффициент кластеризации
    fn approximate_clustering_coefficient(&self, nodes: &[GraphNode], edges: &[GraphEdge]) -> f32 {
        if nodes.is_empty() {
            return 0.0;
        }

        // Простая эвристика: отношение фактических треугольников к возможным
        // В реальной системе это будет более сложное вычисление
        let mut triangles = 0;
        let mut triples = 0;

        for node in nodes {
            let neighbors: Vec<u64> = edges
                .iter()
                .filter(|e| e.source == node.id || e.target == node.id)
                .map(|e| {
                    if e.source == node.id {
                        e.target
                    } else {
                        e.source
                    }
                })
                .collect();

            for i in 0..neighbors.len() {
                for j in (i + 1)..neighbors.len() {
                    triples += 1;
                    // Проверяем, связаны ли соседи напрямую
                    if edges.iter().any(|e| {
                        (e.source == neighbors[i] && e.target == neighbors[j])
                            || (e.source == neighbors[j] && e.target == neighbors[i])
                    }) {
                        triangles += 1;
                    }
                }
            }
        }

        if triples == 0 {
            0.0
        } else {
            triangles as f32 / triples as f32
        }
    }

    /// Считает количество компонентов связности
    fn count_connected_components(&self, nodes: &[GraphNode], edges: &[GraphEdge]) -> usize {
        if nodes.is_empty() {
            return 0;
        }

        let mut visited = HashSet::new();
        let mut components = 0;

        for node in nodes {
            if !visited.contains(&node.id) {
                self.dfs_mark_component(node.id, edges, &mut visited);
                components += 1;
            }
        }

        components
    }

    /// Выполняет DFS для маркировки компонента связности
    fn dfs_mark_component(&self, start_node: u64, edges: &[GraphEdge], visited: &mut HashSet<u64>) {
        let mut stack = vec![start_node];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }

            visited.insert(current);

            // Добавляем соседей в стек
            for edge in edges {
                if edge.source == current && !visited.contains(&edge.target) {
                    stack.push(edge.target);
                } else if edge.target == current && !visited.contains(&edge.source) {
                    stack.push(edge.source);
                }
            }
        }
    }

    /// Экспортирует визуализацию в формате JSON
    pub fn export_to_json(&self, visualization: &GraphVisualization) -> Result<String, String> {
        serde_json::to_string(visualization)
            .map_err(|e| format!("Failed to serialize visualization: {e}"))
    }

    /// Экспортирует визуализацию в формате `GraphML`
    pub fn export_to_graphml(&self, visualization: &GraphVisualization) -> Result<String, String> {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
        xml.push_str("  <graph id=\"G\" edgedefault=\"directed\">\n");

        // Добавляем узлы
        for node in &visualization.nodes {
            xml.push_str(&format!(
                "    <node id=\"{}\">\n      <data key=\"label\">{}</data>\n      <data key=\"x\">{}</data>\n      <data key=\"y\">{}</data>\n      <data key=\"size\">{}</data>\n      <data key=\"color\">{}</data>\n    </node>\n",
                node.id,
                node.label,
                node.x,
                node.y,
                node.size,
                node.color
            ));
        }

        // Добавляем ребра
        for edge in &visualization.edges {
            xml.push_str(&format!(
                "    <edge source=\"{}\" target=\"{}\">\n      <data key=\"label\">{}</data>\n      <data key=\"weight\">{}</data>\n      <data key=\"color\">{}</data>\n    </edge>\n",
                edge.source,
                edge.target,
                edge.label,
                edge.weight,
                edge.color
            ));
        }

        xml.push_str("  </graph>\n</graphml>");
        Ok(xml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::PersistentStore;
    use serde_json::json;

    #[test]
    fn test_graph_visualizer_creation() {
        let store = Arc::new(PersistentStore::open("./test_data_vis").unwrap());
        let visualizer = GraphVisualizer::new(store);

        assert_eq!(visualizer.store.len().unwrap(), 0);
    }

    #[test]
    fn test_node_to_graph_node() {
        let store = Arc::new(PersistentStore::open("./test_data_vis2").unwrap());
        let visualizer = GraphVisualizer::new(store);

        let node = Node {
            id: 1,
            vector: vec![0.5, 0.3],
            properties: json!({"name": "Test Node", "label": "user"}),
            edges: vec![],
        };

        let graph_node = visualizer
            .node_to_graph_node(&node, &LayoutAlgorithm::ForceDirected)
            .unwrap();
        assert_eq!(graph_node.id, 1);
        assert_eq!(graph_node.label, "Test Node");
        assert_eq!(graph_node.color, "#6366f1"); // indigo для user
    }

    #[test]
    fn test_layout_algorithms() {
        let store = Arc::new(PersistentStore::open("./test_data_vis3").unwrap());
        let visualizer = GraphVisualizer::new(store);

        let node = Node {
            id: 1,
            vector: vec![0.5, 0.3],
            properties: json!({}),
            edges: vec![],
        };

        // Тестируем каждый алгоритм
        let layouts = [
            LayoutAlgorithm::ForceDirected,
            LayoutAlgorithm::Circular,
            LayoutAlgorithm::Grid,
            LayoutAlgorithm::Hierarchical,
            LayoutAlgorithm::Topological,
        ];

        for layout in &layouts {
            let result = visualizer.position_for_layout(&node, layout);
            assert!(result.is_ok());
            let (x, y) = result.unwrap();
            assert!(x.is_finite());
            assert!(y.is_finite());
        }
    }

    #[test]
    fn test_visualization_metadata() {
        let store = Arc::new(PersistentStore::open("./test_data_vis4").unwrap());
        let visualizer = GraphVisualizer::new(store);

        let nodes = vec![
            GraphNode {
                id: 1,
                label: "Node 1".to_string(),
                x: 0.0,
                y: 0.0,
                size: 10.0,
                color: "#6366f1".to_string(),
                properties: json!({}),
                vector: vec![0.1, 0.2],
            },
            GraphNode {
                id: 2,
                label: "Node 2".to_string(),
                x: 1.0,
                y: 1.0,
                size: 10.0,
                color: "#6366f1".to_string(),
                properties: json!({}),
                vector: vec![0.9, 0.8],
            },
        ];

        let edges = vec![GraphEdge {
            source: 1,
            target: 2,
            label: "connected".to_string(),
            weight: 0.8,
            color: "#94a3b8".to_string(),
            properties: json!({}),
        }];

        let metadata = visualizer.compute_metadata(&nodes, &edges).unwrap();
        assert_eq!(metadata.node_count, 2);
        assert_eq!(metadata.edge_count, 1);
        assert_eq!(metadata.connected_components, 1);
    }
}
