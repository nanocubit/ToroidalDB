use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub enum ReplicationEvent {
    Write {
        key: String,
        value: Vec<u8>,
        timestamp: u64,
    },
    Delete {
        key: String,
        timestamp: u64,
    },
    Batch {
        operations: Vec<ReplicationEvent>,
    },
}

#[derive(Clone, Debug)]
pub struct ReplicationConfig {
    pub replication_factor: usize,
    pub ack_quorum: usize,
    pub sync_timeout: Duration,
    pub async_batch_size: usize,
    pub heartbeat_interval: Duration,
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        ReplicationConfig {
            replication_factor: 3,
            ack_quorum: 2,
            sync_timeout: Duration::from_secs(5),
            async_batch_size: 100,
            heartbeat_interval: Duration::from_secs(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeRole {
    Master,
    Slave,
}

#[derive(Clone, Debug)]
pub enum NodeStatus {
    Active,
    Lagging { lag_ms: u64 },
    Offline,
    Syncing,
}

pub struct ReplicaNode {
    pub id: String,
    pub endpoint: String,
    pub role: NodeRole,
    pub status: RwLock<NodeStatus>,
    pub last_heartbeat: RwLock<Instant>,
    pub lag_ms: RwLock<u64>,
}

impl ReplicaNode {
    pub fn new(id: String, endpoint: String, role: NodeRole) -> Self {
        ReplicaNode {
            id,
            endpoint,
            role,
            status: RwLock::new(NodeStatus::Active),
            last_heartbeat: RwLock::new(Instant::now()),
            lag_ms: RwLock::new(0),
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(*self.status.read().unwrap(), NodeStatus::Active)
    }

    pub fn update_heartbeat(&self) {
        let mut last = self.last_heartbeat.write().unwrap();
        *last = Instant::now();
    }

    pub fn get_lag(&self) -> u64 {
        *self.lag_ms.read().unwrap()
    }
}

pub struct ReplicationManager {
    config: ReplicationConfig,
    nodes: RwLock<HashMap<String, Arc<ReplicaNode>>>,
    master_id: RwLock<Option<String>>,
    write_queue: mpsc::Sender<ReplicationEvent>,
    replication_lag: RwLock<HashMap<String, u64>>,
}

impl ReplicationManager {
    pub fn new(config: ReplicationConfig) -> Self {
        let (tx, _rx) = mpsc::channel(1000);

        ReplicationManager {
            config,
            nodes: RwLock::new(HashMap::new()),
            master_id: RwLock::new(None),
            write_queue: tx,
            replication_lag: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_node(&self, id: String, endpoint: String, role: NodeRole) -> Arc<ReplicaNode> {
        let is_master = role == NodeRole::Master;
        let node = Arc::new(ReplicaNode::new(id.clone(), endpoint, role));

        let mut nodes = self.nodes.write().unwrap();
        nodes.insert(id.clone(), node.clone());

        if is_master {
            let mut master = self.master_id.write().unwrap();
            *master = Some(id);
        }

        node
    }

    pub fn remove_node(&self, id: &str) -> bool {
        let mut nodes = self.nodes.write().unwrap();
        nodes.remove(id).is_some()
    }

    pub fn get_master(&self) -> Option<Arc<ReplicaNode>> {
        let master_id = self.master_id.read().unwrap();
        if let Some(ref id) = *master_id {
            let nodes = self.nodes.read().unwrap();
            nodes.get(id).cloned()
        } else {
            None
        }
    }

    pub fn get_active_slaves(&self) -> Vec<Arc<ReplicaNode>> {
        let nodes = self.nodes.read().unwrap();

        nodes
            .values()
            .filter(|n| n.role == NodeRole::Slave && n.is_active())
            .cloned()
            .collect()
    }

    pub fn get_all_nodes(&self) -> Vec<Arc<ReplicaNode>> {
        let nodes = self.nodes.read().unwrap();
        nodes.values().cloned().collect()
    }

    pub async fn replicate_write(
        &self,
        key: &str,
        value: Vec<u8>,
    ) -> Result<ReplicationResult, ReplicationError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let event = ReplicationEvent::Write {
            key: key.to_string(),
            value,
            timestamp,
        };

        self.replicate_event(event).await
    }

    pub async fn replicate_delete(&self, key: &str) -> Result<ReplicationResult, ReplicationError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let event = ReplicationEvent::Delete {
            key: key.to_string(),
            timestamp,
        };

        self.replicate_event(event).await
    }

    async fn replicate_event(
        &self,
        event: ReplicationEvent,
    ) -> Result<ReplicationResult, ReplicationError> {
        let slaves = self.get_active_slaves();

        if slaves.len() < self.config.ack_quorum - 1 {
            return Err(ReplicationError::InsufficientReplicas {
                required: self.config.ack_quorum,
                available: slaves.len() + 1,
            });
        }

        let mut acks = 0;
        let mut errors = Vec::new();

        for slave in slaves.iter().take(self.config.replication_factor - 1) {
            match self.send_to_replica(slave, &event).await {
                Ok(_) => acks += 1,
                Err(e) => errors.push(format!("{}: {}", slave.id, e)),
            }
        }

        if acks >= self.config.ack_quorum - 1 {
            Ok(ReplicationResult {
                success: true,
                acks_received: acks + 1,
                errors,
            })
        } else {
            Err(ReplicationError::ReplicationFailed {
                acks_received: acks,
                required: self.config.ack_quorum,
            })
        }
    }

    async fn send_to_replica(
        &self,
        node: &ReplicaNode,
        _event: &ReplicationEvent,
    ) -> Result<(), ReplicationError> {
        node.update_heartbeat();
        Ok(())
    }

    pub fn check_node_health(&self) -> Vec<NodeHealthReport> {
        let nodes = self.nodes.read().unwrap();

        nodes
            .values()
            .map(|n| {
                let status = n.status.read().unwrap().clone();
                let last_heartbeat = *n.last_heartbeat.read().unwrap();
                let elapsed = last_heartbeat.elapsed();

                let health_status = if elapsed > Duration::from_secs(10) {
                    NodeHealthStatus::Unhealthy
                } else if elapsed > Duration::from_secs(3) {
                    NodeHealthStatus::Degraded
                } else {
                    NodeHealthStatus::Healthy
                };

                NodeHealthReport {
                    node_id: n.id.clone(),
                    role: n.role.clone(),
                    health: health_status,
                    lag_ms: n.get_lag(),
                    last_heartbeat_ms: elapsed.as_millis() as u64,
                }
            })
            .collect()
    }

    pub fn get_replication_status(&self) -> ReplicationStatus {
        let nodes = self.nodes.read().unwrap();
        let master = self.get_master();
        let slaves: Vec<_> = nodes
            .values()
            .filter(|n| n.role == NodeRole::Slave)
            .cloned()
            .collect();

        let active_slaves = slaves.iter().filter(|n| n.is_active()).count();
        let total_slaves = slaves.len();

        let quorum = (total_slaves + 1) / 2 + 1;
        let quorum_available = active_slaves >= quorum.saturating_sub(1);

        let master_active = master.as_ref().map(|m| m.is_active()).unwrap_or(false);

        ReplicationStatus {
            master_active,
            active_slaves,
            total_slaves,
            quorum_available,
            replication_factor: self.config.replication_factor,
            healthy: master_active && quorum_available,
        }
    }

    pub fn update_lag(&self, node_id: &str, lag_ms: u64) {
        let nodes = self.nodes.read().unwrap();
        if let Some(node) = nodes.get(node_id) {
            let mut lag = node.lag_ms.write().unwrap();
            *lag = lag_ms;

            let mut all_lag = self.replication_lag.write().unwrap();
            all_lag.insert(node_id.to_string(), lag_ms);
        }
    }

    pub fn get_max_lag(&self) -> u64 {
        let lag = self.replication_lag.read().unwrap();
        *lag.values().max().unwrap_or(&0)
    }
}

#[derive(Clone, Debug)]
pub struct ReplicationResult {
    pub success: bool,
    pub acks_received: usize,
    pub errors: Vec<String>,
}

#[derive(Debug)]
pub enum ReplicationError {
    InsufficientReplicas {
        required: usize,
        available: usize,
    },
    ReplicationFailed {
        acks_received: usize,
        required: usize,
    },
    NodeOffline(String),
    Timeout,
}

impl std::fmt::Display for ReplicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplicationError::InsufficientReplicas {
                required,
                available,
            } => {
                write!(
                    f,
                    "Insufficient replicas: required {}, available {}",
                    required, available
                )
            }
            ReplicationError::ReplicationFailed {
                acks_received,
                required,
            } => {
                write!(
                    f,
                    "Replication failed: acks_received {}, required {}",
                    acks_received, required
                )
            }
            ReplicationError::NodeOffline(node) => write!(f, "Node offline: {}", node),
            ReplicationError::Timeout => write!(f, "Replication timeout"),
        }
    }
}

impl std::error::Error for ReplicationError {}

#[derive(Clone, Debug)]
pub struct ReplicationStatus {
    pub master_active: bool,
    pub active_slaves: usize,
    pub total_slaves: usize,
    pub quorum_available: bool,
    pub replication_factor: usize,
    pub healthy: bool,
}

#[derive(Clone, Debug)]
pub enum NodeHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Clone, Debug)]
pub struct NodeHealthReport {
    pub node_id: String,
    pub role: NodeRole,
    pub health: NodeHealthStatus,
    pub lag_ms: u64,
    pub last_heartbeat_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_manager_new() {
        let manager = ReplicationManager::new(ReplicationConfig::default());
        let status = manager.get_replication_status();

        assert!(!status.healthy);
    }

    #[test]
    fn test_add_node() {
        let manager = ReplicationManager::new(ReplicationConfig::default());

        let master = manager.add_node(
            "master-1".to_string(),
            "http://localhost:8001".to_string(),
            NodeRole::Master,
        );

        assert_eq!(master.role, NodeRole::Master);

        let status = manager.get_replication_status();
        assert!(status.master_active);
    }

    #[test]
    fn test_add_slaves() {
        let manager = ReplicationManager::new(ReplicationConfig {
            replication_factor: 3,
            ..Default::default()
        });

        manager.add_node(
            "master-1".to_string(),
            "http://localhost:8001".to_string(),
            NodeRole::Master,
        );
        manager.add_node(
            "slave-1".to_string(),
            "http://localhost:8002".to_string(),
            NodeRole::Slave,
        );
        manager.add_node(
            "slave-2".to_string(),
            "http://localhost:8003".to_string(),
            NodeRole::Slave,
        );

        let slaves = manager.get_active_slaves();
        assert_eq!(slaves.len(), 2);
    }

    #[test]
    fn test_node_health() {
        let manager = ReplicationManager::new(ReplicationConfig::default());

        manager.add_node(
            "master-1".to_string(),
            "http://localhost:8001".to_string(),
            NodeRole::Master,
        );
        manager.add_node(
            "slave-1".to_string(),
            "http://localhost:8002".to_string(),
            NodeRole::Slave,
        );

        let health = manager.check_node_health();
        assert_eq!(health.len(), 2);
    }

    #[test]
    fn test_replication_status() {
        let manager = ReplicationManager::new(ReplicationConfig {
            replication_factor: 3,
            ack_quorum: 2,
            ..Default::default()
        });

        manager.add_node(
            "master-1".to_string(),
            "http://localhost:8001".to_string(),
            NodeRole::Master,
        );
        manager.add_node(
            "slave-1".to_string(),
            "http://localhost:8002".to_string(),
            NodeRole::Slave,
        );
        manager.add_node(
            "slave-2".to_string(),
            "http://localhost:8003".to_string(),
            NodeRole::Slave,
        );

        let status = manager.get_replication_status();

        assert_eq!(status.total_slaves, 2);
        assert!(status.quorum_available);
    }
}
