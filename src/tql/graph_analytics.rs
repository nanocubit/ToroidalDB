//! # Graph Analytics для TQL v3.0
//!
//! Графовые аналитические функции: CENTRALITY, PAGERANK, COMMUNITY DETECTION

use crate::hybrid_storage::{HybridPersistentStore, Node};
use std::collections::{HashMap, HashSet, VecDeque};

/// Графовый аналитический движок
pub struct GraphAnalytics;

impl GraphAnalytics {
    /// Вычисляет центральность узла
    pub fn centrality(
        store: &HybridPersistentStore,
        node_id: u64,
        algorithm: CentralityAlgorithm,
    ) -> Result<f32, String> {
        match algorithm {
            CentralityAlgorithm::Degree => Self::degree_centrality(store, node_id),
            CentralityAlgorithm::Betweenness => Self::betweenness_centrality(store, node_id),
            CentralityAlgorithm::Closeness => Self::closeness_centrality(store, node_id),
            CentralityAlgorithm::Eigenvector => Self::eigenvector_centrality(store, node_id),
        }
    }

    /// Degree Centrality - количество соседей
    fn degree_centrality(store: &HybridPersistentStore, node_id: u64) -> Result<f32, String> {
        let neighbors = store.get_neighbors(node_id).map_err(|e| e.to_string())?;
        let degree = neighbors.len() as f32;

        // Нормализуем относительно общего количества узлов
        let total_nodes = store.get_node_count() as f32;
        if total_nodes <= 1.0 {
            return Ok(0.0);
        }

        Ok(degree / (total_nodes - 1.0))
    }

    /// Betweenness Centrality - количество кратчайших путей через узел
    fn betweenness_centrality(store: &HybridPersistentStore, node_id: u64) -> Result<f32, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;
        let mut centrality = 0.0f32;

        // Для каждой пары узлов (s, t) считаем кратчайшие пути
        for s_node in &all_nodes {
            if s_node.id == node_id {
                continue;
            }

            for t_node in &all_nodes {
                if t_node.id == node_id || t_node.id == s_node.id {
                    continue;
                }

                // Находим все кратчайшие пути
                let paths = Self::find_all_shortest_paths(store, s_node.id, t_node.id)?;

                // Считаем сколько путей проходит через node_id
                let paths_through_node =
                    paths.iter().filter(|path| path.contains(&node_id)).count() as f32;

                if !paths.is_empty() {
                    centrality += paths_through_node / paths.len() as f32;
                }
            }
        }

        // Нормализуем
        let n = all_nodes.len() as f32;
        if n <= 2.0 {
            return Ok(0.0);
        }

        Ok(centrality / ((n - 1.0) * (n - 2.0)))
    }

    /// Closeness Centrality - обратная сумма расстояний до всех узлов
    fn closeness_centrality(store: &HybridPersistentStore, node_id: u64) -> Result<f32, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;
        let mut total_distance = 0.0f32;
        let mut reachable_count = 0;

        for node in &all_nodes {
            if node.id == node_id {
                continue;
            }

            // Находим расстояние через BFS
            let distance = Self::bfs_distance(store, node_id, node.id)?;

            if distance > 0 {
                total_distance += distance as f32;
                reachable_count += 1;
            }
        }

        if reachable_count == 0 || total_distance == 0.0 {
            return Ok(0.0);
        }

        Ok(reachable_count as f32 / total_distance)
    }

    /// Eigenvector Centrality - влияние узла в сети
    fn eigenvector_centrality(store: &HybridPersistentStore, node_id: u64) -> Result<f32, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;
        let n = all_nodes.len();

        if n == 0 {
            return Ok(0.0);
        }

        // Создаём матрицу смежности
        let mut adjacency = vec![vec![0.0f32; n]; n];
        let node_index: HashMap<u64, usize> = all_nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.id, i))
            .collect();

        for (i, node) in all_nodes.iter().enumerate() {
            for neighbor in store.get_neighbors(node.id).map_err(|e| e.to_string())? {
                if let Some(&j) = node_index.get(&neighbor.id) {
                    adjacency[i][j] = 1.0;
                }
            }
        }

        // Power iteration method
        let mut centrality = vec![1.0f32 / n as f32; n];

        for _ in 0..100 {
            let mut new_centrality = vec![0.0f32; n];

            for i in 0..n {
                for j in 0..n {
                    new_centrality[i] += adjacency[i][j] * centrality[j];
                }
            }

            // Normalization
            let norm: f32 = new_centrality.iter().map(|&x| x * x).sum::<f32>().sqrt();

            if norm > 0.0 {
                for x in new_centrality.iter_mut() {
                    *x /= norm;
                }
            }

            centrality = new_centrality;
        }

        // Возвращаем центральность для нужного узла
        if let Some(&idx) = node_index.get(&node_id) {
            Ok(centrality[idx])
        } else {
            Ok(0.0)
        }
    }

    /// PageRank алгоритм
    pub fn pagerank(
        store: &HybridPersistentStore,
        node_id: u64,
        damping: f32,
        iterations: u32,
    ) -> Result<f32, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;
        let n = all_nodes.len();

        if n == 0 {
            return Ok(0.0);
        }

        let node_index: HashMap<u64, usize> = all_nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.id, i))
            .collect();

        // Инициализация PageRank
        let mut pagerank = vec![1.0f32 / n as f32; n];

        // Создаём матрицу связей
        let mut outlinks: Vec<Vec<usize>> = vec![vec![]; n];

        for (i, node) in all_nodes.iter().enumerate() {
            for neighbor in store.get_neighbors(node.id).map_err(|e| e.to_string())? {
                if let Some(&j) = node_index.get(&neighbor.id) {
                    outlinks[i].push(j);
                }
            }
        }

        // Итерации PageRank
        for _ in 0..iterations {
            let mut new_pagerank = vec![0.0f32; n];

            for i in 0..n {
                for &j in &outlinks[i] {
                    let outdegree = outlinks[i].len() as f32;
                    if outdegree > 0.0 {
                        new_pagerank[j] += pagerank[i] / outdegree;
                    }
                }
            }

            // Применяем damping factor
            for i in 0..n {
                new_pagerank[i] = (1.0 - damping) / n as f32 + damping * new_pagerank[i];
            }

            pagerank = new_pagerank;
        }

        // Возвращаем PageRank для нужного узла
        if let Some(&idx) = node_index.get(&node_id) {
            Ok(pagerank[idx])
        } else {
            Ok(0.0)
        }
    }

    /// Community Detection - алгоритм Louvain
    pub fn community_detection(
        store: &HybridPersistentStore,
        algorithm: CommunityAlgorithm,
    ) -> Result<Vec<(u64, u32)>, String> {
        match algorithm {
            CommunityAlgorithm::Louvain => Self::louvain(store),
            CommunityAlgorithm::LabelPropagation => Self::label_propagation(store),
            CommunityAlgorithm::ConnectedComponents => Self::connected_components(store),
        }
    }

    /// Louvain algorithm для обнаружения сообществ
    fn louvain(store: &HybridPersistentStore) -> Result<Vec<(u64, u32)>, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;
        let n = all_nodes.len();

        if n == 0 {
            return Ok(vec![]);
        }

        // Инициализация: каждый узел в своём сообществе
        let mut communities: HashMap<u64, u32> = all_nodes
            .iter()
            .map(|node| (node.id, node.id as u32))
            .collect();

        // Упрощённая реализация Louvain
        for _ in 0..10 {
            for node in &all_nodes {
                let neighbors = store.get_neighbors(node.id).map_err(|e| e.to_string())?;

                // Считаем модулярность для каждого соседнего сообщества
                let mut community_gain: HashMap<u32, f32> = HashMap::new();

                for neighbor in &neighbors {
                    let &comm = communities.get(&neighbor.id).unwrap();
                    *community_gain.entry(comm).or_insert(0.0) += 1.0;
                }

                // Выбираем сообщество с максимальным gain
                if let Some((&best_comm, _)) = community_gain
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                {
                    communities.insert(node.id, best_comm);
                }
            }
        }

        Ok(communities.into_iter().collect())
    }

    /// Label Propagation algorithm
    fn label_propagation(store: &HybridPersistentStore) -> Result<Vec<(u64, u32)>, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;

        if all_nodes.is_empty() {
            return Ok(vec![]);
        }

        // Инициализация: каждый узел со своим label
        let mut labels: HashMap<u64, u32> = all_nodes
            .iter()
            .map(|node| (node.id, node.id as u32))
            .collect();

        // Итерации
        for _ in 0..20 {
            for node in &all_nodes {
                let neighbors = store.get_neighbors(node.id).map_err(|e| e.to_string())?;

                if neighbors.is_empty() {
                    continue;
                }

                // Считаем frequency каждого label среди соседей
                let mut label_freq: HashMap<u32, u32> = HashMap::new();

                for neighbor in &neighbors {
                    let &label = labels.get(&neighbor.id).unwrap();
                    *label_freq.entry(label).or_insert(0) += 1;
                }

                // Выбираем наиболее частый label
                if let Some((&best_label, _)) = label_freq.iter().max_by(|a, b| a.1.cmp(b.1)) {
                    labels.insert(node.id, best_label);
                }
            }
        }

        Ok(labels.into_iter().collect())
    }

    /// Connected Components algorithm
    fn connected_components(store: &HybridPersistentStore) -> Result<Vec<(u64, u32)>, String> {
        let all_nodes = store.get_all().map_err(|e| e.to_string())?;

        if all_nodes.is_empty() {
            return Ok(vec![]);
        }

        let mut visited = HashSet::new();
        let mut components: HashMap<u64, u32> = HashMap::new();
        let mut component_id = 0u32;

        for node in &all_nodes {
            if visited.contains(&node.id) {
                continue;
            }

            // BFS для нахождения всех узлов в компоненте
            let mut queue = VecDeque::new();
            queue.push_back(node.id);
            visited.insert(node.id);

            while let Some(current_id) = queue.pop_front() {
                components.insert(current_id, component_id);

                let neighbors = store.get_neighbors(current_id).map_err(|e| e.to_string())?;
                for neighbor in neighbors {
                    if !visited.contains(&neighbor.id) {
                        visited.insert(neighbor.id);
                        queue.push_back(neighbor.id);
                    }
                }
            }

            component_id += 1;
        }

        Ok(components.into_iter().collect())
    }

    /// Находит расстояние между узлами через BFS
    fn bfs_distance(store: &HybridPersistentStore, start: u64, end: u64) -> Result<u32, String> {
        if start == end {
            return Ok(0);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((start, 0u32));
        visited.insert(start);

        while let Some((current, distance)) = queue.pop_front() {
            if current == end {
                return Ok(distance);
            }

            let neighbors = store.get_neighbors(current).map_err(|e| e.to_string())?;
            for neighbor in neighbors {
                if !visited.contains(&neighbor.id) {
                    visited.insert(neighbor.id);
                    queue.push_back((neighbor.id, distance + 1));
                }
            }
        }

        Ok(u32::MAX) // Узлы не связаны
    }

    /// Находит все кратчайшие пути между узлами
    fn find_all_shortest_paths(
        store: &HybridPersistentStore,
        start: u64,
        end: u64,
    ) -> Result<Vec<Vec<u64>>, String> {
        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        let mut current_path = vec![start];

        Self::dfs_paths(
            store,
            start,
            end,
            &mut visited,
            &mut current_path,
            &mut paths,
        )?;

        // Находим минимальную длину
        if let Some(min_len) = paths.iter().map(|p| p.len()).min() {
            paths.retain(|p| p.len() == min_len);
        }

        Ok(paths)
    }

    fn dfs_paths(
        store: &HybridPersistentStore,
        current: u64,
        end: u64,
        visited: &mut HashSet<u64>,
        path: &mut Vec<u64>,
        all_paths: &mut Vec<Vec<u64>>,
    ) -> Result<(), String> {
        if current == end {
            all_paths.push(path.clone());
            return Ok(());
        }

        let neighbors = store.get_neighbors(current).map_err(|e| e.to_string())?;

        for neighbor in neighbors {
            if !visited.contains(&neighbor.id) {
                visited.insert(neighbor.id);
                path.push(neighbor.id);

                Self::dfs_paths(store, neighbor.id, end, visited, path, all_paths)?;

                path.pop();
                visited.remove(&neighbor.id);
            }
        }

        Ok(())
    }
}

/// Алгоритмы центральности
#[derive(Debug, Clone, Copy)]
pub enum CentralityAlgorithm {
    Degree,
    Betweenness,
    Closeness,
    Eigenvector,
}

/// Алгоритмы обнаружения сообществ
#[derive(Debug, Clone, Copy)]
pub enum CommunityAlgorithm {
    Louvain,
    LabelPropagation,
    ConnectedComponents,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_storage::Edge;
    use serde_json::json;

    fn create_test_graph() -> HybridPersistentStore {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = HybridPersistentStore::open(temp_dir.path()).unwrap();

        // Создаём простой граф: 1-2-3-4-5
        for i in 1..=5 {
            let node = Node {
                id: i,
                vector: vec![0.5; 384],
                properties: json!({"id": i}),
                edges: vec![],
            };
            store.insert(node).unwrap();
        }

        // Добавляем рёбра
        for i in 1..4 {
            store
                .add_edge(i, i + 1, "CONNECTS".to_string(), 1.0)
                .unwrap();
            store
                .add_edge(i + 1, i, "CONNECTS".to_string(), 1.0)
                .unwrap();
        }

        store
    }

    #[test]
    fn test_degree_centrality() {
        let store = create_test_graph();

        // Узел 3 имеет 2 соседей (2 и 4)
        let centrality =
            GraphAnalytics::centrality(&store, 3, CentralityAlgorithm::Degree).unwrap();
        assert!(centrality > 0.0);
    }

    #[test]
    fn test_pagerank() {
        let store = create_test_graph();

        let pr = GraphAnalytics::pagerank(&store, 3, 0.85, 20).unwrap();
        assert!(pr > 0.0);
    }

    #[test]
    fn test_connected_components() {
        let store = create_test_graph();

        let components =
            GraphAnalytics::community_detection(&store, CommunityAlgorithm::ConnectedComponents)
                .unwrap();

        // Все узлы в одном компоненте
        assert_eq!(components.len(), 5);
        let first_component = components[0].1;
        for (_, comm) in &components {
            assert_eq!(*comm, first_component);
        }
    }
}
