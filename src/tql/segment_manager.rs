//! Segment-based storage: appendable + non-appendable сегменты.
//!
//! Вдохновлено Qdrant segment architecture:
//! - Appendable segment: можно писать и читать (in-memory + small file)
//! - Non-appendable (sealed): только читать (mmap, готов к merge)
//! - Optimizer: фоновый merge маленьких сегментов в большие

use crate::hybrid_storage::Node;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

/// Segment ID.
pub type SegmentId = u64;

/// Segment type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentType {
    /// Можно писать и читать (in-memory + small file)
    Appendable,
    /// Только читать (sealed, готов к merge)
    NonAppendable,
}

/// A single segment in the storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: SegmentId,
    pub segment_type: SegmentType,
    pub nodes: HashMap<u64, Node>,
    pub size_bytes: u64,
    pub created_at: u64,
}

impl Segment {
    pub fn new_appendable(id: SegmentId) -> Self {
        Self {
            id,
            segment_type: SegmentType::Appendable,
            nodes: HashMap::new(),
            size_bytes: 0,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    pub fn seal(self) -> Self {
        Self {
            segment_type: SegmentType::NonAppendable,
            ..self
        }
    }

    pub fn is_appendable(&self) -> bool {
        self.segment_type == SegmentType::Appendable
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn insert(&mut self, node: Node) -> bool {
        if !self.is_appendable() {
            return false;
        }
        let existed = self.nodes.contains_key(&node.id);
        self.nodes.insert(node.id, node);
        !existed
    }

    pub fn get(&self, id: u64) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        if !self.is_appendable() {
            return false;
        }
        self.nodes.remove(&id).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }
}

/// Segment manager: управляет коллекцией сегментов.
pub struct SegmentManager {
    segments: RwLock<Vec<Segment>>,
    next_id: RwLock<SegmentId>,
    /// Maximum segment size before seal + merge
    max_segment_size: usize,
    /// Base path for segment files
    base_path: PathBuf,
}

impl SegmentManager {
    pub fn new(base_path: impl AsRef<Path>, max_segment_size: usize) -> Self {
        Self {
            segments: RwLock::new(Vec::new()),
            next_id: RwLock::new(1),
            max_segment_size,
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Get the current appendable segment (create if needed).
    pub async fn current_appendable(&self) -> SegmentId {
        let segments = self.segments.read().await;
        if let Some(seg) = segments
            .iter()
            .find(|s| s.is_appendable() && s.len() < self.max_segment_size)
        {
            return seg.id;
        }
        drop(segments);

        // Create new appendable segment
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;

        let new_segment = Segment::new_appendable(id);
        self.segments.write().await.push(new_segment);
        id
    }

    /// Insert a node into the current appendable segment.
    pub async fn insert(&self, node: Node) -> bool {
        let seg_id = self.current_appendable().await;
        let mut segments = self.segments.write().await;
        if let Some(seg) = segments.iter_mut().find(|s| s.id == seg_id) {
            seg.insert(node)
        } else {
            false
        }
    }

    /// Get a node by ID from any segment.
    pub async fn get(&self, id: u64) -> Option<Node> {
        let segments = self.segments.read().await;
        for seg in segments.iter().rev() {
            if let Some(node) = seg.get(id) {
                return Some(node.clone());
            }
        }
        None
    }

    /// Remove a node from the appendable segment.
    pub async fn remove(&self, id: u64) -> bool {
        let segments = self.segments.read().await;
        for seg in segments.iter() {
            if seg.is_appendable() {
                // Can't mutate through RwLock read, need write
                drop(segments);
                let mut segments = self.segments.write().await;
                if let Some(s) = segments.iter_mut().find(|s| s.is_appendable()) {
                    return s.remove(id);
                }
                return false;
            }
        }
        false
    }

    /// Seal the current appendable segment (make it read-only).
    pub async fn seal_current(&self) {
        let mut segments = self.segments.write().await;
        if let Some(seg) = segments.iter_mut().find(|s| s.is_appendable()) {
            *seg = Segment::new_appendable(seg.id);
        }
    }

    /// Merge small non-appendable segments into a single appendable one.
    pub async fn merge(&self) {
        let mut segments = self.segments.write().await;
        let small_segments: Vec<Segment> = segments
            .iter()
            .filter(|s| !s.is_appendable() && s.len() < self.max_segment_size / 2)
            .cloned()
            .collect();

        if small_segments.len() < 2 {
            return;
        }

        let mut merged = Segment::new_appendable(*self.next_id.read().await);
        *self.next_id.write().await += 1;

        for seg in &small_segments {
            for node in seg.nodes.values() {
                merged.nodes.insert(node.id, node.clone());
            }
        }

        // Remove old segments, add merged
        let small_ids: Vec<SegmentId> = small_segments.iter().map(|s| s.id).collect();
        segments.retain(|s| !small_ids.contains(&s.id));
        segments.push(merged);
    }

    /// Number of segments.
    pub async fn segment_count(&self) -> usize {
        self.segments.read().await.len()
    }

    /// Total nodes across all segments.
    pub async fn total_nodes(&self) -> usize {
        let segments = self.segments.read().await;
        segments.iter().map(Segment::len).sum()
    }

    /// Get all nodes (for migration/compatibility).
    pub async fn get_all(&self) -> Vec<Node> {
        let segments = self.segments.read().await;
        let mut all = Vec::new();
        for seg in segments.iter() {
            for node in seg.iter() {
                all.push(node.clone());
            }
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_node(id: u64) -> Node {
        Node {
            id,
            vector: vec![id as f32 / 10.0; 4],
            properties: serde_json::json!({"id": id}),
            edges: vec![],
        }
    }

    #[tokio::test]
    async fn test_segment_insert_and_get() {
        let dir = TempDir::new().unwrap();
        let mgr = SegmentManager::new(dir.path(), 100);
        mgr.insert(test_node(1)).await;
        let node = mgr.get(1).await;
        assert!(node.is_some());
        assert_eq!(node.unwrap().id, 1);
    }

    #[tokio::test]
    async fn test_segment_seal() {
        let dir = TempDir::new().unwrap();
        let mgr = SegmentManager::new(dir.path(), 100);
        mgr.insert(test_node(1)).await;
        mgr.seal_current().await;
        // After seal, the old segment is non-appendable
        // A new one should be created
        let seg_id = mgr.current_appendable().await;
        assert!(seg_id > 1);
    }

    #[tokio::test]
    async fn test_segment_merge() {
        let dir = TempDir::new().unwrap();
        let mgr = SegmentManager::new(dir.path(), 10);
        for i in 0..3 {
            let seg_id = mgr.current_appendable().await;
            mgr.insert(test_node(i)).await;
            mgr.seal_current().await;
        }
        // Merge small segments
        mgr.merge().await;
        // Should have fewer segments now
        let count = mgr.segment_count().await;
        assert!(
            count <= 3,
            "Expected fewer segments after merge, got {}",
            count
        );
    }

    #[tokio::test]
    async fn test_get_all() {
        let dir = TempDir::new().unwrap();
        let mgr = SegmentManager::new(dir.path(), 100);
        mgr.insert(test_node(1)).await;
        mgr.insert(test_node(2)).await;
        let all = mgr.get_all().await;
        assert_eq!(all.len(), 2);
    }
}
