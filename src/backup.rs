use crate::hybrid_storage::{HybridPersistentStore, Node};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupInfo {
    pub backup_id: String,
    pub created_at: SystemTime,
    pub node_count: usize,
    pub size_bytes: u64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateBackupRequest {
    pub compression: Option<String>,
    pub include_nodes: Option<bool>,
    pub metadata: Option<serde_json::Value>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoreBackupRequest {
    pub backup_id: String,
    pub force_overwrite: Option<bool>,
    pub selected_collections: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScheduleBackupRequest {
    pub interval_hours: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupResponse {
    pub success: bool,
    pub message: String,
    pub backup_id: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupListResponse {
    pub success: bool,
    pub message: String,
    pub backups: Vec<BackupInfo>,
    pub data: Option<serde_json::Value>,
}

pub struct BackupManager {
    backup_dir: String,
    retention_days: u64,
}

impl BackupManager {
    pub fn new(backup_dir: &str, retention_days: u64) -> Self {
        Self {
            backup_dir: backup_dir.to_string(),
            retention_days,
        }
    }

    pub async fn create_backup(
        &self,
        store: &HybridPersistentStore,
        description: Option<String>,
    ) -> Result<String, String> {
        let backup_id = format!(
            "backup_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        let backup_path = format!("{}/{}", self.backup_dir, backup_id);
        std::fs::create_dir_all(&backup_path)
            .map_err(|e| format!("Failed to create backup directory: {}", e))?;

        // Get all nodes
        let nodes = store
            .get_all()
            .map_err(|e| format!("Failed to get nodes for backup: {}", e))?;

        let node_count = nodes.len();

        // Serialize nodes to JSON
        let nodes_json = serde_json::to_string_pretty(&nodes)
            .map_err(|e| format!("Failed to serialize nodes: {}", e))?;

        // Write nodes to backup file
        let nodes_file = format!("{}/nodes.json", backup_path);
        std::fs::write(&nodes_file, &nodes_json)
            .map_err(|e| format!("Failed to write nodes file: {}", e))?;

        // Create backup metadata
        let metadata = BackupMetadata {
            backup_id: backup_id.clone(),
            created_at: SystemTime::now(),
            node_count,
            size_bytes: nodes_json.len() as u64,
            description,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

        let metadata_file = format!("{}/metadata.json", backup_path);
        std::fs::write(&metadata_file, &metadata_json)
            .map_err(|e| format!("Failed to write metadata file: {}", e))?;

        println!(
            "✅ Backup created: {} ({} nodes, {} bytes)",
            backup_id,
            node_count,
            nodes_json.len()
        );

        Ok(backup_id)
    }

    pub async fn list_backups(&self) -> Result<Vec<BackupInfo>, String> {
        let mut backups = Vec::new();

        if !Path::new(&self.backup_dir).exists() {
            return Ok(backups);
        }

        let entries = std::fs::read_dir(&self.backup_dir)
            .map_err(|e| format!("Failed to read backup directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                let metadata_file = path.join("metadata.json");
                if metadata_file.exists() {
                    let metadata_content = std::fs::read_to_string(&metadata_file)
                        .map_err(|e| format!("Failed to read metadata: {}", e))?;

                    if let Ok(metadata) = serde_json::from_str::<BackupMetadata>(&metadata_content)
                    {
                        backups.push(BackupInfo {
                            backup_id: metadata.backup_id,
                            created_at: metadata.created_at,
                            node_count: metadata.node_count,
                            size_bytes: metadata.size_bytes,
                            description: metadata.description,
                        });
                    }
                }
            }
        }

        // Sort by creation time (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }

    pub async fn restore_from_backup(
        &self,
        backup_id: &str,
        store: &HybridPersistentStore,
        force_overwrite: bool,
    ) -> Result<(), String> {
        let backup_path = format!("{}/{}", self.backup_dir, backup_id);

        if !Path::new(&backup_path).exists() {
            return Err(format!("Backup {} not found", backup_id));
        }

        let nodes_file = format!("{}/nodes.json", backup_path);
        if !Path::new(&nodes_file).exists() {
            return Err(format!("Nodes file not found in backup {}", backup_id));
        }

        // Read nodes from backup
        let nodes_content = std::fs::read_to_string(&nodes_file)
            .map_err(|e| format!("Failed to read nodes file: {}", e))?;

        let nodes: Vec<Node> = serde_json::from_str(&nodes_content)
            .map_err(|e| format!("Failed to deserialize nodes: {}", e))?;

        // Restore nodes
        let mut restored_count = 0;
        for node in nodes {
            if force_overwrite {
                store
                    .insert(node)
                    .map_err(|e| format!("Failed to insert node: {}", e))?;
            } else {
                // Only insert if node doesn't exist
                if let Ok(None) = store.get(node.id) {
                    store
                        .insert(node)
                        .map_err(|e| format!("Failed to insert node: {}", e))?;
                }
            }
            restored_count += 1;
        }

        println!(
            "✅ Restored {} nodes from backup: {}",
            restored_count, backup_id
        );
        Ok(())
    }

    pub async fn get_backup_info(&self, backup_id: &str) -> Result<BackupInfo, String> {
        let backup_path = format!("{}/{}", self.backup_dir, backup_id);

        if !Path::new(&backup_path).exists() {
            return Err(format!("Backup {} not found", backup_id));
        }

        let metadata_file = format!("{}/metadata.json", backup_path);
        if !Path::new(&metadata_file).exists() {
            return Err(format!("Metadata not found for backup {}", backup_id));
        }

        let metadata_content = std::fs::read_to_string(&metadata_file)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;

        let metadata = serde_json::from_str::<BackupMetadata>(&metadata_content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;

        Ok(BackupInfo {
            backup_id: metadata.backup_id,
            created_at: metadata.created_at,
            node_count: metadata.node_count,
            size_bytes: metadata.size_bytes,
            description: metadata.description,
        })
    }

    pub async fn cleanup_old_backups(&self) -> Result<usize, String> {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - (self.retention_days * 24 * 3600);

        let mut removed_count = 0;

        if !Path::new(&self.backup_dir).exists() {
            return Ok(removed_count);
        }

        let entries = std::fs::read_dir(&self.backup_dir)
            .map_err(|e| format!("Failed to read backup directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.is_dir() {
                let metadata_file = path.join("metadata.json");
                if let Ok(metadata_content) = std::fs::read_to_string(&metadata_file) {
                    if let Ok(metadata) = serde_json::from_str::<BackupMetadata>(&metadata_content)
                    {
                        if let Ok(duration) = metadata.created_at.duration_since(UNIX_EPOCH) {
                            if duration.as_secs() < cutoff_time {
                                println!("🗑️ Removing old backup: {}", metadata.backup_id);
                                std::fs::remove_dir_all(&path)
                                    .map_err(|e| format!("Failed to remove old backup: {}", e))?;
                                removed_count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(removed_count)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupMetadata {
    backup_id: String,
    created_at: SystemTime,
    node_count: usize,
    size_bytes: u64,
    description: Option<String>,
    version: String,
}
