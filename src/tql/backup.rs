// src/tql/backup.rs - Simplified backup and restore functionality for ToroidalDB
use crate::hybrid_storage::{HybridPersistentStore as PersistentStore, Node};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: String,
    pub created_at: u64,
    pub size_bytes: u64,
    pub node_count: usize,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBackupRequest {
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreBackupRequest {
    pub backup_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupResponse {
    pub success: bool,
    pub message: String,
    pub backup_id: Option<String>,
    pub data: Option<serde_json::Value>,
}

pub struct BackupManager {
    backup_dir: String,
    retention_days: usize,
}

impl BackupManager {
    pub fn new(backup_dir: &str, retention_days: usize) -> Self {
        Self {
            backup_dir: backup_dir.to_string(),
            retention_days,
        }
    }

    pub async fn create_backup(&self, store: &PersistentStore) -> Result<String, String> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Time error: {e}"))?
            .as_secs();

        let backup_id = format!("backup_{now}");
        let backup_path = format!("{}/{}.jsonl", self.backup_dir, backup_id);

        let all_nodes = store
            .get_all()
            .map_err(|e| format!("Failed to get all nodes: {e}"))?;

        let node_count = all_nodes.len();

        // Simple JSONL format - one node per line
        let mut file = std::fs::File::create(&backup_path)
            .map_err(|e| format!("Failed to create backup file: {e}"))?;

        // Write header
        let header = serde_json::json!({
            "version": "1.0",
            "created_at": now,
            "node_count": node_count,
        });
        writeln!(
            &mut file,
            "HEADER:{}",
            serde_json::to_string(&header).map_err(|e| e.to_string())?
        )
        .map_err(|e| format!("Failed to write header: {e}"))?;

        // Write each node as JSONL
        for node in all_nodes {
            let line = serde_json::to_string(&node)
                .map_err(|e| format!("Failed to serialize node: {e}"))?;
            writeln!(&mut file, "NODE:{line}").map_err(|e| format!("Failed to write node: {e}"))?;
        }

        file.sync_all()
            .map_err(|e| format!("Failed to sync file: {e}"))?;

        let size = std::fs::metadata(&backup_path)
            .map_err(|e| format!("Failed to get metadata: {e}"))?
            .len();

        println!("✅ Created backup {backup_id} with {node_count} nodes ({size} bytes)");

        Ok(backup_id)
    }

    pub async fn schedule_regular_backups(
        &self,
        store: Arc<PersistentStore>,
        interval_hours: u32,
    ) -> Result<(), String> {
        use tokio::time::{sleep, Duration};

        loop {
            sleep(Duration::from_secs(u64::from(interval_hours) * 3600)).await;

            match self.create_backup(&store).await {
                Ok(backup_id) => {
                    println!("🔄 Scheduled backup completed: {backup_id}");
                }
                Err(e) => {
                    eprintln!("❌ Failed to create scheduled backup: {e}");
                }
            }
        }
    }

    pub async fn restore_from_backup(
        &self,
        backup_id: &str,
        store: &PersistentStore,
    ) -> Result<(), String> {
        let backup_path = format!("{}/{}.jsonl", self.backup_dir, backup_id);

        if !Path::new(&backup_path).exists() {
            return Err(format!("Backup {backup_path} not found"));
        }

        let file =
            std::fs::File::open(&backup_path).map_err(|e| format!("Failed to open backup: {e}"))?;

        let reader = std::io::BufReader::new(file);

        let mut restored_count = 0;

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Failed to read line: {e}"))?;

            if line.trim().is_empty() {
                continue;
            }

            if let Some(header_str) = line.strip_prefix("HEADER:") {
                let header: serde_json::Value = serde_json::from_str(header_str)
                    .map_err(|e| format!("Failed to parse header: {e}"))?;
                println!("📋 Restoring from backup: {header:?}");
            } else if let Some(node_str) = line.strip_prefix("NODE:") {
                let node: Node = serde_json::from_str(node_str)
                    .map_err(|e| format!("Failed to parse node: {e}"))?;

                store
                    .insert(node)
                    .map_err(|e| format!("Failed to insert node: {e}"))?;

                restored_count += 1;
            }
        }

        println!("✅ Restored {restored_count} nodes from backup");
        Ok(())
    }

    pub fn list_backups(&self) -> Result<Vec<BackupInfo>, String> {
        let mut backups: Vec<BackupInfo> = Vec::new();

        if !Path::new(&self.backup_dir).exists() {
            return Ok(backups);
        }

        let entries = std::fs::read_dir(&self.backup_dir)
            .map_err(|e| format!("Failed to read backup directory: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                let file = std::fs::File::open(&path)
                    .map_err(|e| format!("Failed to open backup: {e}"))?;
                let reader = std::io::BufReader::new(file);

                for line in reader.lines() {
                    let line = line.map_err(|e| format!("Failed to read: {e}"))?;

                    if let Some(header_str) = line.strip_prefix("HEADER:") {
                        let header: serde_json::Value = serde_json::from_str(header_str)
                            .map_err(|e| format!("Failed to parse: {e}"))?;

                        let info = BackupInfo {
                            id: path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            created_at: header
                                .get("created_at")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0),
                            size_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                            node_count: header
                                .get("node_count")
                                .and_then(serde_json::Value::as_u64)
                                .map_or(0, |u| u as usize),
                            metadata: header.clone(),
                        };
                        backups.push(info);
                        break;
                    }
                }
            }
        }

        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }

    pub fn delete_backup(&self, backup_id: &str) -> Result<(), String> {
        let backup_path = format!("{}/{}.jsonl", self.backup_dir, backup_id);

        if !Path::new(&backup_path).exists() {
            return Err(format!("Backup {backup_id} not found"));
        }

        std::fs::remove_file(&backup_path).map_err(|e| format!("Failed to delete backup: {e}"))?;

        Ok(())
    }

    pub fn cleanup_old_backups(&self) -> Result<(), String> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Time error: {e}"))?
            .as_secs();

        let cutoff = now - (self.retention_days as u64 * 24 * 60 * 60);

        let backups = self.list_backups()?;

        for backup in backups {
            if backup.created_at < cutoff {
                if let Err(e) = self.delete_backup(&backup.id) {
                    eprintln!("Failed to delete old backup {}: {}", backup.id, e);
                }
            }
        }

        Ok(())
    }
}
