//! Векторный проектор для ToroidalDB
//! 
//! Предоставляет:
//! - TSNE проекции векторов
//! - UMAP проекции векторов
//! - PCA снижение размерности
//! - Визуализацию в 2D/3D пространстве

use crate::storage::Node;
use crate::math::MatryoshkaDim;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorProjection {
    pub nodes: Vec<ProjectedNode>,
    pub algorithm: ProjectionAlgorithm,
    pub dimensions: usize,
    pub metadata: ProjectionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedNode {
    pub id: u64,
    pub original_vector: Vec<f32>,
    pub projected_coords: Vec<f32>,
    pub label: String,
    pub color: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectionAlgorithm {
    TSNE { perplexity: f32, learning_rate: f32 },
    UMAP { n_neighbors: usize, min_dist: f32 },
    PCA { n_components: usize },
    RandomProjection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionMetadata {
    pub original_dimensions: usize,
    pub projected_dimensions: usize,
    pub node_count: usize,
    pub algorithm: String,
    pub variance_explained: Option<f32>,
    pub execution_time_ms: u64,
}

pub struct VectorProjector {
    store: Arc<crate::storage::PersistentStore>,
}

impl VectorProjector {
    pub fn new(store: Arc<crate::storage::PersistentStore>) -> Self {
        Self { store }
    }

    /// Создает проекцию векторов с использованием заданного алгоритма
    pub fn create_projection(
        &self,
        node_ids: &[u64],
        algorithm: ProjectionAlgorithm,
        target_dims: usize,
    ) -> Result<VectorProjection, String> {
        let start_time = std::time::Instant::now();
        
        // Получаем узлы для проекции
        let mut nodes = Vec::new();
        for &id in node_ids {
            if let Ok(Some(node)) = self.store.get(id) {
                nodes.push(node);
            }
        }

        if nodes.is_empty() {
            return Err("No nodes found for projection".to_string());
        }

        // Извлекаем векторы
        let vectors: Vec<Vec<f32>> = nodes.iter()
            .map(|node| node.vector.clone())
            .collect();

        // Приводим векторы к одинаковой размерности
        let dim = self.get_common_dimension(&vectors)?;
        let padded_vectors: Vec<Vec<f32>> = vectors
            .iter()
            .map(|v| crate::math::pad_or_truncate(v, dim))
            .collect();

        // Выполняем проекцию
        let projected_coords = match &algorithm {
            ProjectionAlgorithm::TSNE { perplexity, learning_rate } => {
                self.perform_tsne_projection(&padded_vectors, target_dims, *perplexity, *learning_rate)?
            },
            ProjectionAlgorithm::UMAP { n_neighbors, min_dist } => {
                self.perform_umap_projection(&padded_vectors, target_dims, *n_neighbors, *min_dist)?
            },
            ProjectionAlgorithm::PCA { n_components } => {
                self.perform_pca_projection(&padded_vectors, *n_components)?
            },
            ProjectionAlgorithm::RandomProjection => {
                self.perform_random_projection(&padded_vectors, target_dims)?
            },
        };

        // Создаем проецированные узлы
        let projected_nodes: Vec<ProjectedNode> = nodes
            .iter()
            .zip(projected_coords.iter())
            .map(|(node, coords)| ProjectedNode {
                id: node.id,
                original_vector: node.vector.clone(),
                projected_coords: coords.clone(),
                label: self.get_node_label(node),
                color: self.get_node_color(node),
                properties: node.properties.clone(),
            })
            .collect();

        let execution_time = start_time.elapsed().as_millis() as u64;

        Ok(VectorProjection {
            nodes: projected_nodes,
            algorithm,
            dimensions: target_dims,
            metadata: ProjectionMetadata {
                original_dimensions: dim,
                projected_dimensions: target_dims,
                node_count: nodes.len(),
                algorithm: format!("{:?}", algorithm),
                variance_explained: None, // В реальной системе будет вычислено для PCA
                execution_time_ms: execution_time,
            },
        })
    }

    /// Выполняет TSNE проекцию
    fn perform_tsne_projection(
        &self,
        vectors: &[Vec<f32>],
        target_dims: usize,
        perplexity: f32,
        learning_rate: f32,
    ) -> Result<Vec<Vec<f32>>, String> {
        // В реальной системе здесь будет вызов алгоритма t-SNE
        // Пока возвращаем простую проекцию (первые n координат)
        
        let mut projected = Vec::new();
        for vector in vectors {
            let mut coords = Vec::new();
            for i in 0..target_dims {
                if i < vector.len() {
                    coords.push(vector[i]);
                } else {
                    coords.push(0.0);
                }
            }
            projected.push(coords);
        }
        
        Ok(projected)
    }

    /// Выполняет UMAP проекцию
    fn perform_umap_projection(
        &self,
        vectors: &[Vec<f32>],
        target_dims: usize,
        n_neighbors: usize,
        min_dist: f32,
    ) -> Result<Vec<Vec<f32>>, String> {
        // В реальной системе здесь будет вызов UMAP алгоритма
        // Пока возвращаем простую проекцию
        
        let mut projected = Vec::new();
        for vector in vectors {
            let mut coords = Vec::new();
            for i in 0..target_dims {
                if i < vector.len() {
                    coords.push(vector[i] * (i as f32 + 1.0)); // Простое преобразование
                } else {
                    coords.push(0.0);
                }
            }
            projected.push(coords);
        }
        
        Ok(projected)
    }

    /// Выполняет PCA проекцию
    fn perform_pca_projection(
        &self,
        vectors: &[Vec<f32>],
        n_components: usize,
    ) -> Result<Vec<Vec<f32>>, String> {
        // В реальной системе здесь будет вызов PCA
        // Пока возвращаем простую проекцию
        
        let mut projected = Vec::new();
        for vector in vectors {
            let mut coords = Vec::new();
            for i in 0..n_components {
                if i < vector.len() {
                    coords.push(vector[i]);
                } else {
                    coords.push(0.0);
                }
            }
            projected.push(coords);
        }
        
        Ok(projected)
    }

    /// Выполняет случайную проекцию
    fn perform_random_projection(
        &self,
        vectors: &[Vec<f32>],
        target_dims: usize,
    ) -> Result<Vec<Vec<f32>>, String> {
        // В реальной системе здесь будет случайная проекция
        // Пока возвращаем проекцию на первые target_dims координат
        
        let mut projected = Vec::new();
        for vector in vectors {
            let mut coords = Vec::new();
            for i in 0..target_dims {
                if i < vector.len() {
                    coords.push(vector[i]);
                } else {
                    coords.push(0.0);
                }
            }
            projected.push(coords);
        }
        
        Ok(projected)
    }

    /// Находит общую размерность векторов
    fn get_common_dimension(&self, vectors: &[Vec<f32>]) -> Result<usize, String> {
        if vectors.is_empty() {
            return Err("No vectors provided".to_string());
        }

        let first_len = vectors[0].len();
        if vectors.iter().all(|v| v.len() == first_len) {
            Ok(first_len)
        } else {
            // Найдем максимальную размерность и будем использовать её
            let max_len = vectors.iter().map(|v| v.len()).max().unwrap_or(0);
            Ok(max_len)
        }
    }

    /// Получает метку узла для визуализации
    fn get_node_label(&self, node: &Node) -> String {
        if let Some(name) = node.properties.get("name").and_then(|v| v.as_str()) {
            name.to_string()
        } else if let Some(title) = node.properties.get("title").and_then(|v| v.as_str()) {
            title.to_string()
        } else {
            format!("Node_{}", node.id)
        }
    }

    /// Получает цвет узла для визуализации
    fn get_node_color(&self, node: &Node) -> String {
        // Определяем цвет на основе метки или других свойств
        if let Some(label) = node.properties.get("label").and_then(|v| v.as_str()) {
            match label.to_lowercase().as_str() {
                "user" => "#6366f1", // indigo
                "document" => "#10b981", // emerald
                "image" => "#f59e0b", // amber
                "video" => "#ef4444", // red
                _ => "#8b5cf6", // violet
            }
        } else {
            // Цвет на основе ID
            let colors = ["#6366f1", "#8b5cf6", "#ec4899", "#f59e0b", "#10b981"];
            let idx = (node.id as usize) % colors.len();
            colors[idx].to_string()
        }
    }

    /// Экспортирует проекцию в JSON
    pub fn export_to_json(&self, projection: &VectorProjection) -> Result<String, String> {
        serde_json::to_string(projection)
            .map_err(|e| format!("Failed to serialize projection: {}", e))
    }

    /// Создает HTML визуализацию проекции
    pub fn create_html_visualization(&self, projection: &VectorProjection) -> Result<String, String> {
        if projection.dimensions != 2 && projection.dimensions != 3 {
            return Err("HTML visualization only supports 2D and 3D projections".to_string());
        }

        let nodes_json = serde_json::to_string(&projection.nodes)
            .map_err(|e| format!("Failed to serialize nodes: {}", e))?;

        let html = if projection.dimensions == 2 {
            // 2D визуализация с D3.js
            format!(
                r#"<!DOCTYPE html>
<html>
<head>
    <title>ToroidalDB Vector Projection - 2D</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>
        body {{
            margin: 0;
            padding: 20px;
            font-family: Arial, sans-serif;
            background: linear-gradient(135deg, #0f172a, #1e293b);
            color: white;
        }}
        #chart {{
            width: 100vw;
            height: calc(100vh - 100px);
            border: 1px solid #334155;
            border-radius: 8px;
        }}
        .node {{
            stroke: #fff;
            stroke-width: 1.5px;
        }}
        .tooltip {{
            position: absolute;
            text-align: center;
            padding: 8px;
            font: 12px sans-serif;
            background: rgba(0, 0, 0, 0.8);
            color: white;
            border: 0px;
            border-radius: 8px;
            pointer-events: none;
            opacity: 0;
        }}
    </style>
</head>
<body>
    <h1>ToroidalDB Vector Projection - 2D ({})</h1>
    <div id="chart"></div>
    <div class="tooltip"></div>
    
    <script>
        const nodes = {};
        const width = document.getElementById('chart').clientWidth;
        const height = document.getElementById('chart').clientHeight;
        
        // Найдем мин и макс координаты для нормализации
        let minX = Infinity, maxX = -Infinity;
        let minY = Infinity, maxY = -Infinity;
        
        nodes.forEach(node => {{
            if (node.projected_coords.length >= 2) {{
                minX = Math.min(minX, node.projected_coords[0]);
                maxX = Math.max(maxX, node.projected_coords[0]);
                minY = Math.min(minY, node.projected_coords[1]);
                maxY = Math.max(maxY, node.projected_coords[1]);
            }}
        }});
        
        // Создаем SVG
        const svg = d3.select('#chart')
            .append('svg')
            .attr('width', width)
            .attr('height', height);
        
        // Создаем scales для нормализации координат
        const xScale = d3.scaleLinear()
            .domain([minX, maxX])
            .range([50, width - 50]);
        
        const yScale = d3.scaleLinear()
            .domain([minY, maxY])
            .range([height - 50, 50]); // Инвертируем Y для правильного отображения
        
        // Добавляем узлы
        const tooltip = d3.select('.tooltip');
        
        svg.selectAll('.node')
            .data(nodes)
            .enter()
            .append('circle')
            .attr('class', 'node')
            .attr('cx', d => xScale(d.projected_coords[0]))
            .attr('cy', d => yScale(d.projected_coords[1]))
            .attr('r', 8)
            .attr('fill', d => d.color)
            .on('mouseover', function(event, d) {{
                tooltip.transition()
                    .duration(200)
                    .style('opacity', .9);
                tooltip.html('<strong>' + d.label + '</strong><br/>ID: ' + d.id + '<br/>Coords: [' + 
                             d.projected_coords[0].toFixed(3) + ', ' + d.projected_coords[1].toFixed(3) + ']')
                    .style('left', (event.pageX + 10) + 'px')
                    .style('top', (event.pageY - 28) + 'px');
            }})
            .on('mouseout', function(d) {{
                tooltip.transition()
                    .duration(500)
                    .style('opacity', 0);
            }});
        
        // Добавляем подписи
        svg.selectAll('.label')
            .data(nodes)
            .enter()
            .append('text')
            .attr('class', 'label')
            .attr('x', d => xScale(d.projected_coords[0]))
            .attr('y', d => yScale(d.projected_coords[1]) - 10)
            .attr('text-anchor', 'middle')
            .attr('fill', 'white')
            .attr('font-size', '10px')
            .text(d => d.label.substring(0, 10) + (d.label.length > 10 ? '...' : ''));
    </script>
</body>
</html>"#,
                format!("{:?}", projection.algorithm),
                nodes_json
            )
        } else {
            // 3D визуализация с Three.js
            format!(
                r#"<!DOCTYPE html>
<html>
<head>
    <title>ToroidalDB Vector Projection - 3D</title>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/three@0.128.0/examples/js/controls/OrbitControls.js"></script>
    <style>
        body {{
            margin: 0;
            overflow: hidden;
            background: #000;
        }}
        #info {{
            position: absolute;
            top: 10px;
            width: 100%;
            text-align: center;
            color: white;
            font-family: Monospace;
            font-size: 13px;
            font-weight: bold;
            z-index: 100;
        }}
    </style>
</head>
<body>
    <div id="info">ToroidalDB 3D Vector Projection ({})</div>
    <div id="container"></div>
    
    <script>
        const nodes = {};
        const scene = new THREE.Scene();
        const camera = new THREE.PerspectiveCamera(75, window.innerWidth / window.innerHeight, 0.1, 1000);
        const renderer = new THREE.WebGLRenderer({{ antialias: true }});
        renderer.setSize(window.innerWidth, window.innerHeight);
        document.body.appendChild(renderer.domElement);
        
        // Найдем мин и макс координаты для нормализации
        let minX = Infinity, maxX = -Infinity;
        let minY = Infinity, maxY = -Infinity;
        let minZ = Infinity, maxZ = -Infinity;
        
        nodes.forEach(node => {{
            if (node.projected_coords.length >= 3) {{
                minX = Math.min(minX, node.projected_coords[0]);
                maxX = Math.max(maxX, node.projected_coords[0]);
                minY = Math.min(minY, node.projected_coords[1]);
                maxY = Math.max(maxY, node.projected_coords[1]);
                minZ = Math.min(minZ, node.projected_coords[2]);
                maxZ = Math.max(maxZ, node.projected_coords[2]);
            }}
        }});
        
        // Создаем узлы
        const geometry = new THREE.SphereGeometry(0.1, 32, 32);
        const material = new THREE.MeshBasicMaterial({{ color: 0x00ff00 }});
        
        nodes.forEach(node => {{
            if (node.projected_coords.length >= 3) {{
                const sphere = new THREE.Mesh(geometry, material.clone());
                
                // Нормализуем координаты в [-5, 5] диапазон
                sphere.position.x = 10 * (node.projected_coords[0] - minX) / (maxX - minX) - 5;
                sphere.position.y = 10 * (node.projected_coords[1] - minY) / (maxY - minY) - 5;
                sphere.position.z = 10 * (node.projected_coords[2] - minZ) / (maxZ - minZ) - 5;
                
                scene.add(sphere);
            }}
        }});
        
        camera.position.z = 5;
        
        // Добавляем управление камерой
        const controls = new THREE.OrbitControls(camera, renderer.domElement);
        controls.enableDamping = true;
        controls.dampingFactor = 0.25;
        
        function animate() {{
            requestAnimationFrame(animate);
            controls.update();
            renderer.render(scene, camera);
        }}
        
        animate();
        
        // Обработка изменения размера окна
        window.addEventListener('resize', () => {{
            camera.aspect = window.innerWidth / window.innerHeight;
            camera.updateProjectionMatrix();
            renderer.setSize(window.innerWidth, window.innerHeight);
        }});
    </script>
</body>
</html>"#,
                format!("{:?}", projection.algorithm),
                nodes_json
            )
        };

        Ok(html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_storage::HybridPersistentStore;
    use serde_json::json;

    #[test]
    fn test_vector_projector_creation() {
        let store = Arc::new(HybridPersistentStore::open("./test_projection_data").unwrap());
        let projector = VectorProjector::new(store);

        assert_eq!(projector.store.len().unwrap(), 0);
    }

    #[test]
    fn test_tsne_projection() {
        let store = Arc::new(HybridPersistentStore::open("./test_projection_data2").unwrap());
        let projector = VectorProjector::new(store);

        // Создаем тестовые узлы
        let test_nodes = vec![
            Node {
                id: 1,
                vector: vec![0.1, 0.2, 0.3, 0.4],
                properties: json!({"name": "Node1", "label": "user"}),
                edges: vec![],
            },
            Node {
                id: 2,
                vector: vec![0.9, 0.8, 0.7, 0.6],
                properties: json!({"name": "Node2", "label": "document"}),
                edges: vec![],
            },
        ];

        for node in test_nodes {
            projector.store.insert(node).unwrap();
        }

        let node_ids = vec![1, 2];
        let algorithm = ProjectionAlgorithm::TSNE { 
            perplexity: 5.0, 
            learning_rate: 100.0 
        };

        let result = projector.create_projection(&node_ids, algorithm, 2);
        assert!(result.is_ok());

        let projection = result.unwrap();
        assert_eq!(projection.nodes.len(), 2);
        assert_eq!(projection.dimensions, 2);
    }

    #[test]
    fn test_umap_projection() {
        let store = Arc::new(HybridPersistentStore::open("./test_projection_data3").unwrap());
        let projector = VectorProjector::new(store);

        // Создаем тестовые узлы
        let test_nodes = vec![
            Node {
                id: 1,
                vector: vec![0.5, 0.5, 0.5],
                properties: json!({"name": "Node1"}),
                edges: vec![],
            },
            Node {
                id: 2,
                vector: vec![0.3, 0.7, 0.2],
                properties: json!({"name": "Node2"}),
                edges: vec![],
            },
            Node {
                id: 3,
                vector: vec![0.8, 0.1, 0.9],
                properties: json!({"name": "Node3"}),
                edges: vec![],
            },
        ];

        for node in test_nodes {
            projector.store.insert(node).unwrap();
        }

        let node_ids = vec![1, 2, 3];
        let algorithm = ProjectionAlgorithm::UMAP { 
            n_neighbors: 2, 
            min_dist: 0.1 
        };

        let result = projector.create_projection(&node_ids, algorithm, 2);
        assert!(result.is_ok());

        let projection = result.unwrap();
        assert_eq!(projection.nodes.len(), 3);
        assert_eq!(projection.dimensions, 2);
    }

    #[test]
    fn test_pca_projection() {
        let store = Arc::new(HybridPersistentStore::open("./test_projection_data4").unwrap());
        let projector = VectorProjector::new(store);

        // Создаем тестовые узлы
        let test_nodes = vec![
            Node {
                id: 1,
                vector: vec![1.0, 2.0, 3.0, 4.0, 5.0],
                properties: json!({"name": "Node1"}),
                edges: vec![],
            },
            Node {
                id: 2,
                vector: vec![2.0, 3.0, 4.0, 5.0, 6.0],
                properties: json!({"name": "Node2"}),
                edges: vec![],
            },
        ];

        for node in test_nodes {
            projector.store.insert(node).unwrap();
        }

        let node_ids = vec![1, 2];
        let algorithm = ProjectionAlgorithm::PCA { n_components: 2 };

        let result = projector.create_projection(&node_ids, algorithm, 2);
        assert!(result.is_ok());

        let projection = result.unwrap();
        assert_eq!(projection.nodes.len(), 2);
        assert_eq!(projection.dimensions, 2);
    }

    #[test]
    fn test_export_to_json() {
        let store = Arc::new(HybridPersistentStore::open("./test_projection_data5").unwrap());
        let projector = VectorProjector::new(store);

        // Создаем минимальную проекцию для тестирования
        let projection = VectorProjection {
            nodes: vec![ProjectedNode {
                id: 1,
                original_vector: vec![0.5, 0.3],
                projected_coords: vec![0.2, 0.8],
                label: "Test".to_string(),
                color: "#ff0000".to_string(),
                properties: json!({}),
            }],
            algorithm: ProjectionAlgorithm::PCA { n_components: 2 },
            dimensions: 2,
            metadata: ProjectionMetadata {
                original_dimensions: 2,
                projected_dimensions: 2,
                node_count: 1,
                algorithm: "PCA".to_string(),
                variance_explained: Some(0.95),
                execution_time_ms: 10,
            },
        };

        let result = projector.export_to_json(&projection);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("nodes"));
    }
}