use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub weight: f64,
}

#[derive(Clone, Debug)]
pub struct Graph {
    adjacency: HashMap<usize, Vec<(usize, f64)>>,
    nodes: HashSet<usize>,
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            adjacency: HashMap::new(),
            nodes: HashSet::new(),
        }
    }

    pub fn add_edge(&mut self, from: usize, to: usize, weight: f64) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.adjacency.entry(from).or_default().push((to, weight));
    }

    pub fn add_undirected_edge(&mut self, from: usize, to: usize, weight: f64) {
        self.add_edge(from, to, weight);
        self.add_edge(to, from, weight);
    }

    pub fn get_neighbors(&self, node: usize) -> Vec<(usize, f64)> {
        self.adjacency.get(&node).cloned().unwrap_or_default()
    }

    pub fn nodes(&self) -> &HashSet<usize> {
        &self.nodes
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

pub fn dijkstra(graph: &Graph, start: usize, end: usize) -> Option<(Vec<usize>, f64)> {
    let mut distances: HashMap<usize, f64> = HashMap::new();
    let mut previous: HashMap<usize, usize> = HashMap::new();
    let mut visited: HashSet<usize> = HashSet::new();
    let mut heap = BinaryHeap::new();

    distances.insert(start, 0.0);
    heap.push(State {
        node: start,
        distance: 0.0,
    });

    while let Some(State { node, distance }) = heap.pop() {
        if visited.contains(&node) {
            continue;
        }
        visited.insert(node);

        if node == end {
            break;
        }

        for (neighbor, weight) in graph.get_neighbors(node) {
            if visited.contains(&neighbor) {
                continue;
            }

            let new_distance = distance + weight;
            let old_distance = distances.get(&neighbor).copied().unwrap_or(f64::MAX);

            if new_distance < old_distance {
                distances.insert(neighbor, new_distance);
                previous.insert(neighbor, node);
                heap.push(State {
                    node: neighbor,
                    distance: new_distance,
                });
            }
        }
    }

    if !distances.contains_key(&end) {
        return None;
    }

    let mut path = Vec::new();
    let mut current = end;

    while let Some(&prev) = previous.get(&current) {
        path.push(current);
        current = prev;
    }
    path.push(start);
    path.reverse();

    let total_distance = distances.get(&end).copied().unwrap_or(f64::MAX);

    Some((path, total_distance))
}

#[derive(Clone, Debug)]
struct State {
    node: usize,
    distance: f64,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for State {}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .partial_cmp(&self.distance)
            .unwrap_or(Ordering::Equal)
    }
}

pub fn bfs_paths(graph: &Graph, start: usize, end: usize, max_depth: usize) -> Vec<Vec<usize>> {
    let mut paths = Vec::new();
    let mut queue: Vec<(usize, Vec<usize>)> = Vec::new();

    queue.push((start, vec![start]));

    while let Some((current, path)) = queue.pop() {
        if path.len() > max_depth {
            continue;
        }

        if current == end {
            paths.push(path);
            continue;
        }

        for (neighbor, _) in graph.get_neighbors(current) {
            if !path.contains(&neighbor) {
                let mut new_path = path.clone();
                new_path.push(neighbor);
                queue.push((neighbor, new_path));
            }
        }
    }

    paths
}

pub fn all_simple_paths(
    graph: &Graph,
    start: usize,
    end: usize,
    max_depth: usize,
) -> Vec<Vec<usize>> {
    let mut all_paths = Vec::new();
    let mut visited = HashSet::new();
    let mut current_path = vec![start];

    dfs_paths(
        graph,
        start,
        end,
        max_depth,
        &mut visited,
        &mut current_path,
        &mut all_paths,
    );

    all_paths
}

fn dfs_paths(
    graph: &Graph,
    current: usize,
    end: usize,
    max_depth: usize,
    visited: &mut HashSet<usize>,
    path: &mut Vec<usize>,
    all_paths: &mut Vec<Vec<usize>>,
) {
    if path.len() > max_depth {
        return;
    }

    if current == end {
        all_paths.push(path.clone());
        return;
    }

    visited.insert(current);

    for (neighbor, _) in graph.get_neighbors(current) {
        if !visited.contains(&neighbor) {
            path.push(neighbor);
            dfs_paths(graph, neighbor, end, max_depth, visited, path, all_paths);
            path.pop();
        }
    }

    visited.remove(&current);
}

pub fn path_length(path: &[usize], graph: &Graph) -> f64 {
    let mut length = 0.0;

    for i in 0..path.len().saturating_sub(1) {
        let from = path[i];
        let to = path[i + 1];

        for (neighbor, weight) in graph.get_neighbors(from) {
            if neighbor == to {
                length += weight;
                break;
            }
        }
    }

    length
}

pub fn degree_centrality(graph: &Graph, node: usize) -> f64 {
    let neighbors = graph.get_neighbors(node).len();
    let total_nodes = graph.nodes().len();

    if total_nodes <= 1 {
        return 0.0;
    }

    neighbors as f64 / (total_nodes - 1) as f64
}

pub fn betweenness_centrality(graph: &Graph, node: usize) -> f64 {
    let mut betweenness = 0.0;
    let total_nodes = graph.nodes().len();

    for start in graph.nodes() {
        for end in graph.nodes() {
            if start == end || *end == node || *start == node {
                continue;
            }

            if let Some((path, _)) = dijkstra(graph, *start, *end) {
                if path.contains(&node) {
                    betweenness += 1.0;
                }
            }
        }
    }

    let normalization = 2.0 / ((total_nodes - 1) * (total_nodes - 2)) as f64;
    betweenness * normalization
}

pub fn eigenvector_centrality(
    graph: &Graph,
    iterations: usize,
    tolerance: f64,
) -> HashMap<usize, f64> {
    let nodes: Vec<usize> = graph.nodes().iter().copied().collect();
    let n = nodes.len();

    if n == 0 {
        return HashMap::new();
    }

    let mut centrality: HashMap<usize, f64> = nodes.iter().map(|&n| (n, 1.0)).collect();

    for _ in 0..iterations {
        let mut new_centrality: HashMap<usize, f64> = HashMap::new();
        let mut max_diff: f64 = 0.0;

        for &node in &nodes {
            let mut sum = 0.0;

            for (neighbor, _) in graph.get_neighbors(node) {
                sum += centrality.get(&neighbor).copied().unwrap_or(0.0);
            }

            new_centrality.insert(node, sum);
        }

        let norm: f64 = new_centrality.values().map(|v| v * v).sum::<f64>().sqrt();

        if norm > 0.0 {
            for (_, v) in &mut new_centrality {
                *v /= norm;
            }
        }

        for &node in &nodes {
            let diff: f64 = (new_centrality.get(&node).copied().unwrap_or(0.0)
                - centrality.get(&node).copied().unwrap_or(0.0))
            .abs();
            max_diff = max_diff.max(diff);
        }

        centrality = new_centrality;

        if max_diff < tolerance {
            break;
        }
    }

    centrality
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dijkstra() {
        let mut graph = Graph::new();
        graph.add_edge(1, 2, 1.0);
        graph.add_edge(2, 3, 1.0);
        graph.add_edge(1, 3, 3.0);

        let result = dijkstra(&graph, 1, 3);
        assert!(result.is_some());

        let (path, distance) = result.unwrap();
        assert_eq!(path, vec![1, 2, 3]);
        assert!((distance - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_bfs_paths() {
        let mut graph = Graph::new();
        graph.add_edge(1, 2, 1.0);
        graph.add_edge(2, 3, 1.0);
        graph.add_edge(1, 3, 1.0);

        let paths = bfs_paths(&graph, 1, 3, 3);
        assert!(!paths.is_empty());
    }

    #[test]
    fn test_degree_centrality() {
        let mut graph = Graph::new();
        graph.add_edge(1, 2, 1.0);
        graph.add_edge(1, 3, 1.0);
        graph.add_edge(1, 4, 1.0);

        let centrality = degree_centrality(&graph, 1);
        assert!((centrality - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_path_length() {
        let mut graph = Graph::new();
        graph.add_edge(1, 2, 1.0);
        graph.add_edge(2, 3, 2.0);

        let length = path_length(&[1, 2, 3], &graph);
        assert!((length - 3.0).abs() < 0.001);
    }
}
