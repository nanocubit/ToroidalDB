//! Write-Ahead Log for durability.
//!
//! Все операции (insert/update/delete) проходят через WAL перед выполнением.
//! При старте WAL воспроизводится, гарантируя сохранность данных.

use crate::hybrid_storage::Node;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// WAL entry types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    Insert {
        seq: u64,
        node: Node,
    },
    Update {
        seq: u64,
        node: Node,
    },
    Delete {
        seq: u64,
        node_id: u64,
    },
    AddEdge {
        seq: u64,
        from: u64,
        to: u64,
        edge_type: String,
        weight: f32,
    },
    Checkpoint {
        seq: u64,
    },
}

/// Simple append-only WAL.
pub struct Wal {
    path: PathBuf,
    writer: Mutex<BufWriter<std::fs::File>>,
    seq: Mutex<u64>,
}

impl Wal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().join("wal.log");
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;

        // Determine next seq from existing WAL
        let existing = if path.exists() {
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let reader = BufReader::new(file);
            let mut last_seq = 0u64;
            for line_result in reader.lines() {
                let line = line_result.map_err(|e| e.to_string())?;
                if let Ok(entry) = serde_json::from_str::<WalEntry>(&line) {
                    match entry {
                        WalEntry::Insert { seq, .. }
                        | WalEntry::Update { seq, .. }
                        | WalEntry::Delete { seq, .. }
                        | WalEntry::AddEdge { seq, .. }
                        | WalEntry::Checkpoint { seq } => last_seq = seq,
                    }
                }
            }
            last_seq + 1
        } else {
            0
        };

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;

        let writer = BufWriter::new(file);
        Ok(Self {
            path,
            writer: Mutex::new(writer),
            seq: Mutex::new(existing),
        })
    }

    pub fn next_seq(&self) -> u64 {
        let mut seq = self.seq.lock().unwrap();
        let current = *seq;
        *seq += 1;
        current
    }

    pub fn append(&self, entry: &WalEntry) -> Result<(), String> {
        let mut writer = self.writer.lock().unwrap();
        let line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
        writeln!(writer, "{line}").map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Replay all entries, calling the callback for each.
    pub fn replay<F>(&self, mut callback: F) -> Result<(), String>
    where
        F: FnMut(WalEntry) -> Result<(), String>,
    {
        let file = std::fs::File::open(&self.path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        for line_result in reader.lines() {
            let line = line_result.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: WalEntry = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            callback(entry)?;
        }
        Ok(())
    }

    /// Checkpoint: truncate WAL (data is safely persisted in storage).
    pub fn checkpoint(&self) -> Result<(), String> {
        let seq = *self.seq.lock().unwrap();
        self.append(&WalEntry::Checkpoint { seq })?;

        // Rotate WAL: rename current, start fresh
        let new_path = self.path.with_extension("wal.old");
        std::fs::rename(&self.path, &new_path).map_err(|e| e.to_string())?;
        // Remove old WAL
        let _ = std::fs::remove_file(&new_path);

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        let mut writer = self.writer.lock().unwrap();
        *writer = BufWriter::new(file);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_wal_append_and_replay() {
        let dir = TempDir::new().unwrap();
        let wal = Wal::open(dir.path()).unwrap();

        let node = Node {
            id: 1,
            vector: vec![0.1, 0.2],
            properties: serde_json::json!({"name": "test"}),
            edges: vec![],
        };

        wal.append(&WalEntry::Insert {
            seq: wal.next_seq(),
            node: node.clone(),
        })
        .unwrap();
        wal.append(&WalEntry::Checkpoint {
            seq: wal.next_seq(),
        })
        .unwrap();

        let mut count = 0;
        wal.replay(|entry| {
            count += 1;
            match entry {
                WalEntry::Insert { node: n, .. } => assert_eq!(n.id, 1),
                _ => {}
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 2);
    }
}
