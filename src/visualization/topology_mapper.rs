//! Топологический маппер для ToroidalDB
//! 
//! Предоставляет визуализацию топологических структур:
//! - Тороидальные пространства
//! - Гомотопические классы
//! - Поток Риччи
//! - Топологические инварианты

use crate::storage::Node;
use crate::topology::edges::{HomotopyClass, InterToroidalEdge, ToroidalLevel};
use crate::topology::functions::{toroidal_distance as topological_distance, compute_homotopy_class, ricci_curvature, topological_centrality};
use crate::math::MatryoshkaDim;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologicalMap {
    pub nodes: Vec<TopologicalNode>,
    pub edges: Vec<TopologicalEdge>,
    pub dimensions: Vec<ToroidalLevel>,
    pub homotopy_classes: Vec<HomotopyClass>,
    pub metadata: TopologicalMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologicalNode {
    pub id: u64,
    pub vector: Vec<f32>,
    pub level: ToroidalLevel,
    pub homotopy_class: HomotopyClass,
    pub x: f32,
    pub y: f32,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologicalEdge {
    pub source: u64,
    pub target: u64,
    pub source_level: ToroidalLevel,
    pub target_level: ToroidalLevel,
    pub relation_type: String,
    pub topological_distance: f32,
    pub homotopy_class: HomotopyClass,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologicalMetadata {
    pub euler_characteristic: i32,
    pub betti_numbers: Vec<usize>,
    pub ricci_curvature_avg: f32,
    pub topological_density: f32,
    pub connected_components: usize,
    pub homotopy_class_count: usize,
}

pub struct TopologyMapper {
    store: Arc<crate::storage::PersistentStore>,
}

impl TopologyMapper {
    pub fn new(store: Arc<crate::storage::PersistentStore>) -> Self {
        Self { store }
    }

    /// Создает топологическую карту для заданного уровня
    pub fn create_topological_map(
        &self,
        level: ToroidalLevel,
        include_inter_level: bool,
    ) -> Result<TopologicalMap, String> {
        let all_nodes = self.store.get_all().map_err(|e| e.to_string())?;
        
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut homotopy_classes = HashSet::new();

        // Фильтруем узлы по уровню
        for node in all_nodes {
            if self.belongs_to_level(&node, level) {
                let topo_node = self.node_to_topological_node(&node, level)?;
                nodes.push(topo_node);
            }
        }

        // Добавляем ребра
        for node in &nodes {
            if let Ok(Some(original_node)) = self.store.get(node.id) {
                for edge in &original_node.edges {
                    if let Some(target_node) = nodes.iter().find(|n| n.id == edge.target_id) {
                        let topo_edge = self.edge_to_topological_edge(
                            node.id,
                            target_node.id,
                            level,
                            level,
                            edge,
                        )?;
                        edges.push(topo_edge);
                    }
                }
            }
        }

        // Если нужно, добавляем меж-уровневые ребра
        if include_inter_level {
            let inter_level_edges = self.get_inter_toroidal_edges()?;
            for edge in inter_level_edges {
                if self.belongs_to_level_by_id(edge.source.1, level) || 
                   self.belongs_to_level_by_id(edge.target.1, level) {
                    edges.push(TopologicalEdge {
                        source: edge.source.1,
                        target: edge.target.1,
                        source_level: edge.source.0,
                        target_level: edge.target.0,
                        relation_type: edge.relation_type,
                        topological_distance: edge.topological_distance,
                        homotopy_class: edge.homotopy_class,
                        properties: edge.properties,
                    });
                }
            }
        }

        // Вычисляем топологические метаданные
        let metadata = self.compute_topological_metadata(&nodes, &edges)?;

        Ok(TopologicalMap {
            nodes,
            edges,
            dimensions: vec![level], // В реальной системе может быть несколько размерностей
            homotopy_classes: homotopy_classes.into_iter().collect(),
            metadata,
        })
    }

    /// Преобразует обычный узел в топологический узел
    fn node_to_topological_node(
        &self,
        node: &Node,
        level: ToroidalLevel,
    ) -> Result<TopologicalNode, String> {
        // Вычисляем гомотопический класс узла
        let homotopy_class = self.compute_node_homotopy_class(node, level)?;
        
        // Определяем позицию в топологическом пространстве
        let (x, y) = self.position_in_torus_space(&node.vector, level)?;
        
        Ok(TopologicalNode {
            id: node.id,
            vector: node.vector.clone(),
            level,
            homotopy_class,
            x,
            y,
            properties: node.properties.clone(),
        })
    }

    /// Преобразует обычное ребро в топологическое ребро
    fn edge_to_topological_edge(
        &self,
        source_id: u64,
        target_id: u64,
        source_level: ToroidalLevel,
        target_level: ToroidalLevel,
        edge: &crate::storage::Edge,
    ) -> Result<TopologicalEdge, String> {
        // Получаем узлы для вычисления топологического расстояния
        let source_node = self.store.get(source_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Source node {} not found", source_id))?;
        let target_node = self.store.get(target_id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Target node {} not found", target_id))?;
        
        // Вычисляем топологическое расстояние
        let topological_distance = topological_distance(&source_node.vector, &target_node.vector);
        
        // Вычисляем гомотопический класс пути
        let homotopy_class = compute_homotopy_class(&source_node, &target_node, MatryoshkaDim::from_size(source_node.vector.len()));
        
        Ok(TopologicalEdge {
            source: source_id,
            target: target_id,
            source_level,
            target_level,
            relation_type: edge.relation_type.clone(),
            topological_distance,
            homotopy_class,
            properties: serde_json::json!({
                "weight": edge.weight,
                "original_relation": &edge.relation_type,
            }),
        })
    }

    /// Проверяет, принадлежит ли узел к определенному уровню
    fn belongs_to_level(&self, node: &Node, level: ToroidalLevel) -> bool {
        // В реальной системе это будет более сложная логика
        // Пока просто проверяем размер вектора
        let expected_size = match level {
            ToroidalLevel::D384 => 384,
            ToroidalLevel::D768 => 768,
            ToroidalLevel::D1024 => 1024,
            ToroidalLevel::D1536 => 1536,
        };
        
        node.vector.len() <= expected_size
    }

    /// Проверяет принадлежность узла к уровню по ID
    fn belongs_to_level_by_id(&self, node_id: u64, level: ToroidalLevel) -> bool {
        if let Ok(Some(node)) = self.store.get(node_id) {
            self.belongs_to_level(&node, level)
        } else {
            false
        }
    }

    /// Вычисляет гомотопический класс узла
    fn compute_node_homotopy_class(&self, node: &Node, level: ToroidalLevel) -> Result<HomotopyClass, String> {
        // В реальной системе это будет более сложное вычисление
        // Пока возвращаем Direct для простоты
        Ok(HomotopyClass::Direct)
    }

    /// Определяет позицию узла в тороидальном пространстве
    fn position_in_torus_space(&self, vector: &[f32], level: ToroidalLevel) -> Result<(f32, f32), String> {
        if vector.is_empty() {
            return Ok((0.0, 0.0));
        }

        // Используем первые два элемента вектора как координаты
        // Если вектор короче, используем циклическое дополнение
        let x = if vector.len() > 0 { vector[0].rem_euclid(1.0) } else { 0.0 };
        let y = if vector.len() > 1 { vector[1].rem_euclid(1.0) } else { 
            if vector.len() > 0 { vector[0].rem_euclid(1.0) } else { 0.0 } 
        };

        Ok((x, y))
    }

    /// Вычисляет топологические метаданные
    fn compute_topological_metadata(
        &self,
        nodes: &[TopologicalNode],
        edges: &[TopologicalEdge],
    ) -> Result<TopologicalMetadata, String> {
        let node_count = nodes.len();
        let edge_count = edges.len();
        
        // Вычисляем эйлерову характеристику (для тора = 0)
        let euler_characteristic = if node_count > 0 { 0 } else { 0 }; // Для тора всегда 0
        
        // Вычисляем числа Бетти (для тора: b0=1, b1=2, b2=1)
        let betti_numbers = vec![1, 2, 1]; // Пример для 2D тора
        
        // Вычисляем среднюю кривизну Риччи
        let ricci_curvature_avg = self.compute_average_ricci_curvature(nodes)?;
        
        // Вычисляем топологическую плотность
        let topological_density = if node_count > 1 {
            (2.0 * edge_count as f32) / (node_count * (node_count - 1)) as f32
        } else {
            0.0
        };
        
        // Вычисляем количество компонентов связности
        let connected_components = self.count_connected_components(nodes, edges);
        
        // Вычисляем количество уникальных гомотопических классов
        let homotopy_class_count = edges.iter()
            .map(|e| format!("{:?}", e.homotopy_class))
            .collect::<HashSet<_>>()
            .len();

        Ok(TopologicalMetadata {
            euler_characteristic,
            betti_numbers,
            ricci_curvature_avg,
            topological_density,
            connected_components,
            homotopy_class_count,
        })
    }

    /// Вычисляет среднюю кривизну Риччи
    fn compute_average_ricci_curvature(&self, nodes: &[TopologicalNode]) -> Result<f32, String> {
        if nodes.is_empty() {
            return Ok(0.0);
        }

        let mut total_curvature = 0.0;
        let mut count = 0;

        for node in nodes {
            if let Ok(Some(original_node)) = self.store.get(node.id) {
                // Вычисляем кривизну Риччи для узла
                let all_nodes = self.store.get_all().map_err(|e| e.to_string())?;
                let curvature = ricci_curvature(&original_node, &all_nodes, 0.3);
                total_curvature += curvature;
                count += 1;
            }
        }

        if count > 0 {
            Ok(total_curvature / count as f32)
        } else {
            Ok(0.0)
        }
    }

    /// Считает количество компонентов связности
    fn count_connected_components(&self, nodes: &[TopologicalNode], edges: &[TopologicalEdge]) -> usize {
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
    fn dfs_mark_component(&self, start_node: u64, edges: &[TopologicalEdge], visited: &mut HashSet<u64>) {
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

    /// Получает меж-торовые ребра
    fn get_inter_toroidal_edges(&self) -> Result<Vec<InterToroidalEdge>, String> {
        // В реальной системе это будет получение из хранилища
        // Пока возвращаем пустой вектор
        Ok(Vec::new())
    }

    /// Экспортирует топологическую карту в JSON
    pub fn export_to_json(&self, topo_map: &TopologicalMap) -> Result<String, String> {
        serde_json::to_string(topo_map)
            .map_err(|e| format!("Failed to serialize topological map: {}", e))
    }

    /// Создает 3D визуализацию топологии
    pub fn create_3d_visualization(&self, level: ToroidalLevel) -> Result<String, String> {
        let topo_map = self.create_topological_map(level, true)?;
        
        // Создаем HTML с 3D визуализацией (упрощённо)
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>ToroidalDB 3D Topology Visualization</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <script src="https://unpkg.com/three@0.144.0/build/three.min.js"></script>
    <style>
        body {{ margin: 0; overflow: hidden; }}
        #info {{
            position: absolute;
            top: 10px;
            width: 100%;
            text-align: center;
            color: white;
            font-family: Monospace;
            font-size: 13px;
            font-weight: bold;
        }}
    </style>
</head>
<body>
    <div id="info">ToroidalDB Topology Visualization - Level: {:?}</div>
    <div id="container"></div>
    
    <script>
        // THREE.js код для 3D визуализации топологии
        // В реальной системе здесь будет сложная визуализация
        console.log("3D topology visualization for level {:?}", {});
    </script>
</body>
</html>"#, 
            level, 
            format!("{:?}", level)
        );

        Ok(html)
    }

    /// Создает 2D визуализацию топологии
    pub fn create_2d_visualization(&self, level: ToroidalLevel) -> Result<String, String> {
        let topo_map = self.create_topological_map(level, true)?;
        
        // Создаем HTML с 2D визуализацией
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>ToroidalDB 2D Topology Visualization</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>
        .node {{ stroke: #fff; stroke-width: 1.5px; }}
        .link {{ stroke: #999; stroke-opacity: 0.6; }}
        body {{ font-family: Arial, sans-serif; }}
        #chart {{ width: 100vw; height: 100vh; }}
    </style>
</head>
<body>
    <h1>ToroidalDB Topology Visualization - Level: {:?}</h1>
    <div id="chart"></div>
    
    <script>
        // D3.js код для визуализации топологии
        const width = window.innerWidth;
        const height = window.innerHeight;
        
        const svg = d3.select("#chart")
            .append("svg")
            .attr("width", width)
            .attr("height", height);
        
        // Данные для визуализации
        const nodes = {};
        const links = {};
        
        // Создаем силовой граф
        const simulation = d3.forceSimulation(nodes)
            .force("link", d3.forceLink(links).id(d => d.id).distance(50))
            .force("charge", d3.forceManyBody().strength(-100))
            .force("center", d3.forceCenter(width / 2, height / 2));
        
        // Рисуем связи
        const link = svg.append("g")
            .attr("class", "links")
            .selectAll("line")
            .data(links)
            .enter().append("line")
            .attr("stroke-width", d => Math.sqrt(d.value));
        
        // Рисуем узлы
        const node = svg.append("g")
            .attr("class", "nodes")
            .selectAll("circle")
            .data(nodes)
            .enter().append("circle")
            .attr("r", d => d.radius)
            .attr("fill", d => d.color)
            .call(d3.drag()
                .on("start", dragstarted)
                .on("drag", dragged)
                .on("end", dragended));
        
        // Добавляем подписи
        node.append("title")
            .text(d => d.id);
        
        simulation.on("tick", () => {
            link
                .attr("x1", d => d.source.x)
                .attr("y1", d => d.source.y)
                .attr("x2", d => d.target.x)
                .attr("y2", d => d.target.y);
        
            node
                .attr("cx", d => d.x)
                .attr("cy", d => d.y);
        });
        
        function dragstarted(event, d) {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            d.fx = d.x;
            d.fy = d.y;
        }
        
        function dragged(event, d) {
            d.fx = event.x;
            d.fy = event.y;
        }
        
        function dragended(event, d) {
            if (!event.active) simulation.alphaTarget(0);
            d.fx = null;
            d.fy = null;
        }
    </script>
</body>
</html>"#,
            level,
            serde_json::to_string(&topo_map.nodes).map_err(|e| e.to_string())?,
            serde_json::to_string(&topo_map.edges).map_err(|e| e.to_string())?
        );

        Ok(html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_storage::HybridPersistentStore;
    use serde_json::json;

    #[test]
    fn test_topology_mapper_creation() {
        let store = Arc::new(HybridPersistentStore::open("./test_topology_data").unwrap());
        let mapper = TopologyMapper::new(store);

        assert_eq!(mapper.store.len().unwrap(), 0);
    }

    #[test]
    fn test_topological_map_creation() {
        let store = Arc::new(HybridPersistentStore::open("./test_topology_data2").unwrap());
        let mapper = TopologyMapper::new(store);

        // Создаем тестовый узел
        let test_node = Node {
            id: 1,
            vector: vec![0.5, 0.3, 0.7],
            properties: json!({"name": "Test Node"}),
            edges: vec![],
        };

        // Добавляем узел в хранилище
        mapper.store.insert(test_node).unwrap();

        // Создаем топологическую карту
        let topo_map = mapper.create_topological_map(ToroidalLevel::D384, false);
        assert!(topo_map.is_ok());
    }

    #[test]
    fn test_2d_visualization() {
        let store = Arc::new(HybridPersistentStore::open("./test_topology_data3").unwrap());
        let mapper = TopologyMapper::new(store);

        let result = mapper.create_2d_visualization(ToroidalLevel::D384);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("ToroidalDB"));
    }

    #[test]
    fn test_3d_visualization() {
        let store = Arc::new(HybridPersistentStore::open("./test_topology_data4").unwrap());
        let mapper = TopologyMapper::new(store);

        let result = mapper.create_3d_visualization(ToroidalLevel::D768);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("THREE.js"));
    }
}