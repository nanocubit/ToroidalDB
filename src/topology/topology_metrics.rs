use crate::topology::edges::HomotopyClass;
use crate::tql::functions::graph::Graph;
use std::collections::{HashMap, HashSet};

pub struct TopologyMetricsCollector {
    graph: Graph,
    homotopy_classes: HashMap<String, HashSet<usize>>,
}

#[derive(Clone, Debug, Default)]
pub struct TopologyMetricsSnapshot {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub connected_components: usize,
    pub average_degree: f64,
    pub density: f64,
    pub clustering_coefficient: f64,
    pub homotopy_class_count: usize,
    pub homotopy_class_distribution: HashMap<String, usize>,
    pub diameter: Option<usize>,
    pub average_path_length: f64,
}

impl TopologyMetricsCollector {
    pub fn new(graph: Graph) -> Self {
        TopologyMetricsCollector {
            graph,
            homotopy_classes: HashMap::new(),
        }
    }

    pub fn collect(&self) -> TopologyMetricsSnapshot {
        let nodes = self.graph.nodes().len();
        let edges = self.count_edges();

        TopologyMetricsSnapshot {
            total_nodes: nodes,
            total_edges: edges,
            connected_components: self.count_connected_components(),
            average_degree: self.calculate_average_degree(),
            density: self.calculate_density(),
            clustering_coefficient: self.calculate_clustering_coefficient(),
            homotopy_class_count: self.homotopy_classes.len(),
            homotopy_class_distribution: self.get_homotopy_distribution(),
            diameter: self.estimate_diameter(),
            average_path_length: self.estimate_average_path_length(),
        }
    }

    fn count_edges(&self) -> usize {
        let mut count = 0;
        for node in self.graph.nodes() {
            count += self.graph.get_neighbors(*node).len();
        }
        count / 2
    }

    fn count_connected_components(&self) -> usize {
        let mut visited = HashSet::new();
        let mut components = 0;

        for node in self.graph.nodes() {
            if !visited.contains(node) {
                components += 1;
                self.dfs_component(*node, &mut visited);
            }
        }

        components
    }

    fn dfs_component(&self, node: usize, visited: &mut HashSet<usize>) {
        if visited.contains(&node) {
            return;
        }

        visited.insert(node);

        for (neighbor, _) in self.graph.get_neighbors(node) {
            self.dfs_component(neighbor, visited);
        }
    }

    fn calculate_average_degree(&self) -> f64 {
        let total_nodes = self.graph.nodes().len();
        if total_nodes == 0 {
            return 0.0;
        }

        let total_degree: usize = self
            .graph
            .nodes()
            .iter()
            .map(|n| self.graph.get_neighbors(*n).len())
            .sum();

        total_degree as f64 / total_nodes as f64
    }

    fn calculate_density(&self) -> f64 {
        let n = self.graph.nodes().len();
        if n <= 1 {
            return 0.0;
        }

        let m = self.count_edges();
        let max_edges = n * (n - 1) / 2;

        m as f64 / max_edges as f64
    }

    fn calculate_clustering_coefficient(&self) -> f64 {
        let mut total_cc = 0.0;
        let mut count = 0;

        for node in self.graph.nodes() {
            let neighbors: Vec<usize> = self
                .graph
                .get_neighbors(*node)
                .into_iter()
                .map(|(n, _)| n)
                .collect();

            if neighbors.len() < 2 {
                continue;
            }

            let mut triangles = 0;

            for (i, &n1) in neighbors.iter().enumerate() {
                for &n2 in &neighbors[i + 1..] {
                    let n1_neighbors: HashSet<usize> = self
                        .graph
                        .get_neighbors(n1)
                        .into_iter()
                        .map(|(n, _)| n)
                        .collect();

                    if n1_neighbors.contains(&n2) {
                        triangles += 1;
                    }
                }
            }

            let possible_edges = neighbors.len() * (neighbors.len() - 1) / 2;
            if possible_edges > 0 {
                total_cc += triangles as f64 / possible_edges as f64;
                count += 1;
            }
        }

        if count > 0 {
            total_cc / count as f64
        } else {
            0.0
        }
    }

    fn get_homotopy_distribution(&self) -> HashMap<String, usize> {
        self.homotopy_classes
            .iter()
            .map(|(class, nodes)| (class.clone(), nodes.len()))
            .collect()
    }

    fn estimate_diameter(&self) -> Option<usize> {
        let nodes: Vec<usize> = self.graph.nodes().iter().cloned().collect();
        if nodes.len() < 2 {
            return None;
        }

        let mut max_distance = 0;

        for i in 0..nodes.len().min(100) {
            let distances = self.bfs_distances(nodes[i]);
            if let Some(&max_dist) = distances.values().max() {
                max_distance = max_distance.max(max_dist);
            }
        }

        if max_distance > 0 {
            Some(max_distance)
        } else {
            None
        }
    }

    fn estimate_average_path_length(&self) -> f64 {
        let nodes: Vec<usize> = self.graph.nodes().iter().cloned().collect();
        if nodes.len() < 2 {
            return 0.0;
        }

        let sample_size = nodes.len().min(50);
        let mut total_distance = 0.0;
        let mut count = 0;

        for i in 0..sample_size {
            let distances = self.bfs_distances(nodes[i]);
            for (j, &dist) in distances.iter() {
                if *j != nodes[i] {
                    total_distance += dist as f64;
                    count += 1;
                }
            }
        }

        if count > 0 {
            total_distance / count as f64
        } else {
            0.0
        }
    }

    fn bfs_distances(&self, start: usize) -> HashMap<usize, usize> {
        let mut distances = HashMap::new();
        let mut queue = vec![start];
        let mut visited = HashSet::new();

        distances.insert(start, 0);
        visited.insert(start);

        while let Some(node) = queue.pop() {
            let current_dist = *distances.get(&node).unwrap();

            for (neighbor, _) in self.graph.get_neighbors(node) {
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor);
                    distances.insert(neighbor, current_dist + 1);
                    queue.push(neighbor);
                }
            }
        }

        distances
    }

    pub fn register_homotopy_class(&mut self, node_id: usize, class: &HomotopyClass) {
        let class_str = format!("{:?}", class);
        self.homotopy_classes
            .entry(class_str)
            .or_insert_with(HashSet::new)
            .insert(node_id);
    }

    pub fn get_homotopy_class_count(&self, class: &str) -> usize {
        self.homotopy_classes
            .get(class)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    pub fn track_topology_changes(&self, previous: &TopologyMetricsSnapshot) -> TopologyChanges {
        let current = self.collect();

        TopologyChanges {
            nodes_added: current.total_nodes.saturating_sub(previous.total_nodes),
            nodes_removed: previous.total_nodes.saturating_sub(current.total_nodes),
            edges_added: current.total_edges.saturating_sub(previous.total_edges),
            edges_removed: previous.total_edges.saturating_sub(current.total_edges),
            density_change: current.density - previous.density,
            new_homotopy_classes: self.find_new_homotopy_classes(previous),
        }
    }

    fn find_new_homotopy_classes(&self, previous: &TopologyMetricsSnapshot) -> Vec<String> {
        let current_classes: HashSet<String> = self.homotopy_classes.keys().cloned().collect();
        let previous_classes: HashSet<String> = previous
            .homotopy_class_distribution
            .keys()
            .cloned()
            .collect();

        current_classes
            .difference(&previous_classes)
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct TopologyChanges {
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
    pub density_change: f64,
    pub new_homotopy_classes: Vec<String>,
}
