use crate::core::memory::pool::{ObjectPool as CoreObjectPool, PoolConfig};
use crate::hybrid_storage::{Edge, Node};
use serde_json::json;

pub struct NodePool {
    pool: CoreObjectPool<Node>,
}

impl NodePool {
    pub fn new(config: PoolConfig) -> Self {
        let pool = CoreObjectPool::new(
            || Node {
                id: 0,
                vector: vec![],
                properties: json!({}),
                edges: vec![],
            },
            config.max_size,
        );
        pool.prefill(config.initial_size);

        NodePool { pool }
    }

    pub fn acquire(&self) -> Node {
        let mut node = self.pool.acquire();
        node.id = 0;
        node.vector.clear();
        node.properties = json!({});
        node.edges.clear();
        node
    }

    pub fn release(&self, node: Node) {
        self.pool.release(node);
    }

    pub fn create(&self, id: u64, vector: Vec<f32>, properties: serde_json::Value) -> Node {
        let mut node = self.acquire();
        node.id = id;
        node.vector = vector;
        node.properties = properties;
        node
    }

    pub fn size(&self) -> usize {
        self.pool.size()
    }
}

pub struct EdgePool {
    pool: CoreObjectPool<Edge>,
}

impl EdgePool {
    pub fn new(config: PoolConfig) -> Self {
        let pool = CoreObjectPool::new(
            || Edge {
                target_id: 0,
                relation_type: String::new(),
                weight: 1.0,
            },
            config.max_size,
        );
        pool.prefill(config.initial_size);

        EdgePool { pool }
    }

    pub fn acquire(&self) -> Edge {
        let mut edge = self.pool.acquire();
        edge.target_id = 0;
        edge.relation_type.clear();
        edge.weight = 1.0;
        edge
    }

    pub fn release(&self, edge: Edge) {
        self.pool.release(edge);
    }

    pub fn create(&self, target_id: u64, relation_type: String, weight: f32) -> Edge {
        let mut edge = self.acquire();
        edge.target_id = target_id;
        edge.relation_type = relation_type;
        edge.weight = weight;
        edge
    }

    pub fn size(&self) -> usize {
        self.pool.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_pool() {
        let pool = NodePool::new(PoolConfig::with_initial(2, 10));

        let node = pool.create(1, vec![1.0, 2.0], json!({"name": "test"}));
        assert_eq!(node.id, 1);
        assert_eq!(node.vector, vec![1.0, 2.0]);

        pool.release(node);

        assert!(pool.size() > 0);
    }

    #[test]
    fn test_edge_pool() {
        let pool = EdgePool::new(PoolConfig::with_initial(2, 10));

        let edge = pool.create(2, "KNOWS".to_string(), 1.5);
        assert_eq!(edge.target_id, 2);
        assert_eq!(edge.relation_type, "KNOWS");
        assert_eq!(edge.weight, 1.5);

        pool.release(edge);

        assert!(pool.size() > 0);
    }
}
