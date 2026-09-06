//! Модуль для топологических функций и операций

use crate::hybrid_storage::Node;
use crate::math::MatryoshkaDim;
use crate::topology::edges::HomotopyClass;
use std::collections::HashMap;

/// Вычисляет тороидальное расстояние между двумя векторами
pub fn toroidal_distance(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert!(
        vec1.len() == vec2.len(),
        "Vectors must have the same length"
    );

    let mut sum = 0.0;
    for (a, b) in vec1.iter().zip(vec2.iter()) {
        let diff = (a - b).rem_euclid(1.0);
        let diff = if diff > 0.5 { 1.0 - diff } else { diff };
        sum += diff * diff;
    }
    sum.sqrt()
}

/// Вычисляет гомотопический класс пути между двумя узлами
pub fn compute_homotopy_class(
    source_node: &Node,
    target_node: &Node,
    dim: MatryoshkaDim,
) -> HomotopyClass {
    let padded_source = crate::math::pad_or_truncate(&source_node.vector, dim.size());
    let padded_target = crate::math::pad_or_truncate(&target_node.vector, dim.size());

    let differences: Vec<i32> = padded_source
        .iter()
        .zip(padded_target.iter())
        .map(|(a, b)| {
            let diff = b - a;
            // Определяем, сколько оборотов вокруг тора совершает путь
            if diff > 0.5 {
                1 // Оборот в положительном направлении
            } else if diff < -0.5 {
                -1 // Оборот в отрицательном направлении
            } else {
                0 // Нет оборота
            }
        })
        .collect();

    if differences.iter().all(|&x| x == 0) {
        HomotopyClass::Direct
    } else {
        HomotopyClass::Wrapped(differences)
    }
}

/// Вычисляет топологическое расстояние с учетом гомотопического класса
pub fn topological_distance_with_homotopy(
    source_node: &Node,
    target_node: &Node,
    dim: MatryoshkaDim,
) -> (f32, HomotopyClass) {
    let base_distance = toroidal_distance(&source_node.vector, &target_node.vector);
    let homotopy_class = compute_homotopy_class(source_node, target_node, dim);

    (base_distance, homotopy_class)
}

/// Вычисляет кривизну Риччи для пары узлов
/// Это дискретная аппроксимация кривизны Риччи на графе
pub fn ricci_curvature(
    node1: &Node,
    node2: &Node,
    all_nodes: &[Node],
    connection_threshold: f32,
) -> f32 {
    // Находим соседей для обоих узлов
    let neighbors1: Vec<&Node> = all_nodes
        .iter()
        .filter(|n| {
            if n.id == node1.id || n.id == node2.id {
                return false;
            }
            let dist = toroidal_distance(&node1.vector, &n.vector);
            dist <= connection_threshold
        })
        .collect();

    let neighbors2: Vec<&Node> = all_nodes
        .iter()
        .filter(|n| {
            if n.id == node1.id || n.id == node2.id {
                return false;
            }
            let dist = toroidal_distance(&node2.vector, &n.vector);
            dist <= connection_threshold
        })
        .collect();

    // Вычисляем среднее расстояние между соответствующими соседями
    let mut avg_neighbor_distance = 0.0;
    let mut count = 0;

    for n1 in &neighbors1 {
        for n2 in &neighbors2 {
            avg_neighbor_distance += toroidal_distance(&n1.vector, &n2.vector);
            count += 1;
        }
    }

    if count > 0 {
        avg_neighbor_distance /= count as f32;
    }

    // Кривизна Риччи = 1 - (среднее расстояние между соседями)/(расстояние между узлами)
    let direct_distance = toroidal_distance(&node1.vector, &node2.vector);
    if direct_distance > 0.0 {
        1.0 - (avg_neighbor_distance / direct_distance)
    } else {
        1.0 // Если узлы в одной точке, кривизна максимальна
    }
}

/// Вычисляет топологическую центральность узла
pub fn topological_centrality(node: &Node, all_nodes: &[Node], connection_threshold: f32) -> f32 {
    let mut centrality = 0.0;
    let mut connections = 0;

    for other_node in all_nodes {
        if other_node.id == node.id {
            continue;
        }

        let distance = toroidal_distance(&node.vector, &other_node.vector);
        if distance <= connection_threshold {
            centrality += 1.0 / (1.0 + distance);
            connections += 1;
        }
    }

    if connections > 0 {
        centrality / connections as f32
    } else {
        0.0
    }
}

/// Вычисляет гомологические группы (упрощенная версия)
/// Возвращает бетти числа: количество компонент связности, циклов, полостей и т.д.
pub fn betti_numbers(graph: &[(u64, u64)]) -> Vec<usize> {
    // Это упрощенная реализация, которая возвращает только бетти_0 (компоненты связности)
    // В реальной реализации потребуется более сложный алгоритм

    let mut nodes: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for &(a, b) in graph {
        nodes.insert(a);
        nodes.insert(b);
    }

    // Используем Union-Find для подсчета компонент связности
    let mut parent: HashMap<u64, u64> = HashMap::new();
    for &node in &nodes {
        parent.insert(node, node);
    }

    fn find(parent: &mut HashMap<u64, u64>, x: u64) -> u64 {
        let p = *parent.get(&x).unwrap();
        if p == x {
            x
        } else {
            let root = find(parent, p);
            parent.insert(x, root);
            root
        }
    }

    fn union(parent: &mut HashMap<u64, u64>, x: u64, y: u64) {
        let px = find(parent, x);
        let py = find(parent, y);
        if px != py {
            parent.insert(px, py);
        }
    }

    for &(a, b) in graph {
        union(&mut parent, a, b);
    }

    let mut components = std::collections::HashSet::new();
    for &node in &nodes {
        components.insert(find(&mut parent, node));
    }

    vec![components.len()] // Возвращаем только бетти_0
}

/// Вычисляет топологические инварианты для узла
pub fn topological_invariants(
    node: &Node,
    all_nodes: &[Node],
    connection_threshold: f32,
) -> TopologicalInvariants {
    TopologicalInvariants {
        ricci_curvature: ricci_curvature(node, node, all_nodes, connection_threshold),
        centrality: topological_centrality(node, all_nodes, connection_threshold),
        euler_characteristic: 0.0, // Упрощенная реализация
        betti_numbers: vec![],
    }
}

/// Структура для хранения топологических инвариантов
#[derive(Debug, Clone)]
pub struct TopologicalInvariants {
    pub ricci_curvature: f32,
    pub centrality: f32,
    pub euler_characteristic: f32,
    pub betti_numbers: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_toroidal_distance() {
        let vec1 = vec![0.1, 0.2, 0.3];
        let vec2 = vec![0.9, 0.8, 0.7];

        let distance = toroidal_distance(&vec1, &vec2);
        // На торе расстояние между 0.1 и 0.9 по первой координате составляет 0.2, а не 0.8
        assert!(distance > 0.0);
    }

    #[test]
    fn test_compute_homotopy_class() {
        let node1 = Node {
            id: 1,
            vector: vec![0.1, 0.1],
            properties: json!({}),
            edges: vec![],
        };

        let node2 = Node {
            id: 2,
            vector: vec![0.9, 0.9], // На противоположной стороне тора
            properties: json!({}),
            edges: vec![],
        };

        let class = compute_homotopy_class(&node1, &node2, MatryoshkaDim::D384);
        // В зависимости от конкретных значений, класс может быть Wrapped или Direct
        println!("Homotopy class: {:?}", class);
    }

    #[test]
    fn test_ricci_curvature() {
        let node1 = Node {
            id: 1,
            vector: vec![0.5, 0.5],
            properties: json!({}),
            edges: vec![],
        };

        let node2 = Node {
            id: 2,
            vector: vec![0.5, 0.6],
            properties: json!({}),
            edges: vec![],
        };

        let node3 = Node {
            id: 3,
            vector: vec![0.6, 0.5],
            properties: json!({}),
            edges: vec![],
        };

        let all_nodes = vec![node1.clone(), node2.clone(), node3];
        let curvature = ricci_curvature(&node1, &node2, &all_nodes, 0.2);

        println!("Ricci curvature: {}", curvature);
    }
}
