//! Manifest — append-only log tracking which segment files are live.
//!
//! # Format
//!
//! Each entry:
//! ```text
//! ┌─────────┬──────┬────────┬──────────────────┬──────────────┬──────┐
//! │magic:u32│type:1│path_len│     path bytes   │ max_seq:u64  │crc:32│
//! │  0x4D46 │      │ :u16   │                  │ (ADD only)    │      │
//! └─────────┴──────┴────────┴──────────────────┴──────────────┴──────┘
//! ```
//!
//! Types: 1 = ADD_SEGMENT, 2 = REMOVE_SEGMENT.
//!
//! On replay, the manifest is scanned and the current set of live segments is
//! reconstructed. REMOVE_SEGMENT entries cancel prior ADD_SEGMENT entries.

use crate::{Result, TQLError};
use crc32fast::Hasher;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MANIFEST_MAGIC: u32 = 0x4D46_0001; // "MF\0\1"
const CRC_SIZE: usize = 4;
const HEADER_SIZE: usize = 7; // magic(4) + type(1) + path_len(2)

const TYPE_ADD: u8 = 1;
const TYPE_REMOVE: u8 = 2;
const TYPE_WAL_CHECKPOINT: u8 = 3;

// ---------------------------------------------------------------------------
// Manifest entry types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestEntry {
    AddSegment { path: PathBuf, max_sequence: u64 },
    RemoveSegment { path: PathBuf },
    SetWalCheckpoint { sequence: u64 },
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

pub struct Manifest {
    file: Mutex<File>,
    path: PathBuf,
    /// Set of live segment paths (deduplicated from replay).
    live: Mutex<HashSet<PathBuf>>,
    /// Highest max_sequence seen across all ADD_SEGMENT entries.
    max_flushed_sequence: Mutex<u64>,
    /// Byte offset of the last fully-valid record (replay watermark).
    /// Corrupt/truncated data after this offset is discarded on open.
    valid_len: Mutex<u64>,
}

impl Manifest {
    /// Open or create the manifest file at `path`.
    ///
    /// On open, replays all entries to reconstruct `live` segments and
    /// truncates any corrupt/truncated suffix, leaving the file with only
    /// valid records (P2).  Subsequent appends start from a clean state.
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(TQLError::Io)?;

        let mf = Self {
            file: Mutex::new(file),
            path: path.to_path_buf(),
            live: Mutex::new(HashSet::new()),
            max_flushed_sequence: Mutex::new(0),
            valid_len: Mutex::new(0),
        };
        mf.replay()?;

        // P2: truncate any bytes after the last valid record so the manifest
        // contains only valid, self-consistent entries.  A stale corrupt
        // suffix would otherwise make a future append ambiguous.
        {
            let file = mf.file.lock().unwrap();
            let actual = file.metadata().map_err(TQLError::Io)?.len();
            let valid = *mf.valid_len.lock().unwrap();
            if valid < actual {
                file.set_len(valid).map_err(TQLError::Io)?;
                file.sync_all().map_err(TQLError::Io)?;
            }
        }
        Ok(mf)
    }

    /// Return the set of live segment paths.
    pub fn live_segments(&self) -> HashSet<PathBuf> {
        self.live.lock().unwrap().clone()
    }

    /// Return the highest flushed sequence.
    pub fn max_flushed_sequence(&self) -> u64 {
        *self.max_flushed_sequence.lock().unwrap()
    }

    /// Append an ADD_SEGMENT entry.
    pub fn add_segment(&self, seg_path: &Path, max_sequence: u64) -> Result<()> {
        self.append_nosync(&ManifestEntry::AddSegment {
            path: seg_path.to_path_buf(),
            max_sequence,
        })?;
        self.sync()?;
        self.live.lock().unwrap().insert(seg_path.to_path_buf());
        let mut ms = self.max_flushed_sequence.lock().unwrap();
        if max_sequence > *ms {
            *ms = max_sequence;
        }
        Ok(())
    }

    /// Append a REMOVE_SEGMENT entry.
    pub fn remove_segment(&self, seg_path: &Path) -> Result<()> {
        self.append_nosync(&ManifestEntry::RemoveSegment {
            path: seg_path.to_path_buf(),
        })?;
        self.sync()?;
        self.live.lock().unwrap().remove(seg_path);
        Ok(())
    }

    /// Append a WAL checkpoint entry.
    pub fn set_wal_checkpoint(&self, sequence: u64) -> Result<()> {
        self.append_nosync(&ManifestEntry::SetWalCheckpoint { sequence })?;
        self.sync()?;
        let mut ms = self.max_flushed_sequence.lock().unwrap();
        if sequence > *ms {
            *ms = sequence;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal: encode + append (fsync managed by caller for grouping)
    // -----------------------------------------------------------------------

    /// Append a single record to the manifest file (no fsync).
    fn append_nosync(&self, entry: &ManifestEntry) -> Result<()> {
        let buf = encode_entry(entry)?;
        let mut file = self.file.lock().unwrap();
        file.write_all(&buf).map_err(TQLError::Io)
    }

    /// Append ADD_SEGMENT + REMOVE_SEGMENT entries in one atomic batch,
    /// followed by a single fsync.  This is the safe publication protocol for
    /// compaction: the manifest transitions to the new state before any old
    /// segment files are physically deleted.
    pub fn append_compaction_batch(&self, add: (&Path, u64), removes: &[&Path]) -> Result<()> {
        let add_entry = ManifestEntry::AddSegment {
            path: add.0.to_path_buf(),
            max_sequence: add.1,
        };
        let add_buf = encode_entry(&add_entry)?;
        let mut remove_bufs = Vec::with_capacity(removes.len());
        for old in removes {
            let rm_entry = ManifestEntry::RemoveSegment {
                path: old.to_path_buf(),
            };
            remove_bufs.push(encode_entry(&rm_entry)?);
        }
        {
            let mut file = self.file.lock().unwrap();
            file.write_all(&add_buf).map_err(TQLError::Io)?;
            for buf in &remove_bufs {
                file.write_all(buf).map_err(TQLError::Io)?;
            }
            file.sync_all().map_err(TQLError::Io)?;
        }
        self.live.lock().unwrap().insert(add.0.to_path_buf());
        for old in removes {
            self.live.lock().unwrap().remove(*old);
        }
        let mut ms = self.max_flushed_sequence.lock().unwrap();
        if add.1 > *ms {
            *ms = add.1;
        }
        Ok(())
    }

    /// Append multiple ADD_SEGMENT entries then a WAL checkpoint, all without
    /// individual fsync. After this, caller calls `sync()` once.
    pub fn append_batch(&self, adds: &[(&Path, u64)], checkpoint_seq: u64) -> Result<()> {
        for (path, max_seq) in adds {
            self.append_nosync(&ManifestEntry::AddSegment {
                path: path.to_path_buf(),
                max_sequence: *max_seq,
            })?;
            self.live.lock().unwrap().insert(path.to_path_buf());
        }
        self.append_nosync(&ManifestEntry::SetWalCheckpoint {
            sequence: checkpoint_seq,
        })?;
        {
            let mut ms = self.max_flushed_sequence.lock().unwrap();
            if checkpoint_seq > *ms {
                *ms = checkpoint_seq;
            }
        }
        Ok(())
    }

    /// fsync the manifest file to disk.
    pub fn sync(&self) -> Result<()> {
        let file = self.file.lock().unwrap();
        file.sync_all().map_err(TQLError::Io)
    }

    /// Replay all entries from the file to reconstruct `live` segments.
    fn replay(&self) -> Result<()> {
        let mut file = File::open(&self.path).map_err(TQLError::Io)?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw).map_err(TQLError::Io)?;

        let mut off = 0usize;
        let mut live = self.live.lock().unwrap();
        let mut max_seq = self.max_flushed_sequence.lock().unwrap();
        let mut valid = self.valid_len.lock().unwrap();

        while off + HEADER_SIZE + CRC_SIZE <= raw.len() {
            // Read header
            if raw.len() < off + HEADER_SIZE + CRC_SIZE {
                break;
            }
            let magic = read_u32_le(&raw, off);
            if magic != MANIFEST_MAGIC {
                // Corrupt or truncated — stop.
                break;
            }
            let entry_type = raw[off + 4];
            if entry_type != TYPE_ADD
                && entry_type != TYPE_REMOVE
                && entry_type != TYPE_WAL_CHECKPOINT
            {
                break;
            }
            let path_len = read_u16_le(&raw, off + 5) as usize;
            let extra = if entry_type == TYPE_ADD || entry_type == TYPE_WAL_CHECKPOINT {
                8
            } else {
                0
            };
            let total_len = HEADER_SIZE + path_len + extra + CRC_SIZE;

            if off + total_len > raw.len() {
                break;
            }

            // Verify CRC
            let stored_crc = read_u32_le(&raw, off + total_len - CRC_SIZE);
            let calc_crc = crc32(&raw[off..off + total_len - CRC_SIZE]);
            if stored_crc != calc_crc {
                break;
            }

            let path_start = off + HEADER_SIZE;
            let path_bytes = &raw[path_start..path_start + path_len];
            let path = PathBuf::from(std::str::from_utf8(path_bytes).unwrap_or(""));

            match entry_type {
                TYPE_ADD => {
                    let max_sequence = read_u64_le(&raw, path_start + path_len);
                    live.insert(path);
                    if max_sequence > *max_seq {
                        *max_seq = max_sequence;
                    }
                }
                TYPE_REMOVE => {
                    live.remove(&path);
                }
                TYPE_WAL_CHECKPOINT => {
                    let sequence = read_u64_le(&raw, path_start + path_len);
                    if sequence > *max_seq {
                        *max_seq = sequence;
                    }
                }
                _ => {}
            }

            off += total_len;
            // Record the watermark of the last fully-valid record.
            *valid = off as u64;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn encode_entry(entry: &ManifestEntry) -> Result<Vec<u8>> {
    let (entry_type, path, extra_u64): (u8, Option<&Path>, Option<u64>) = match entry {
        ManifestEntry::AddSegment { path, max_sequence } => {
            (TYPE_ADD, Some(path), Some(*max_sequence))
        }
        ManifestEntry::RemoveSegment { path } => (TYPE_REMOVE, Some(path), None),
        ManifestEntry::SetWalCheckpoint { sequence } => {
            (TYPE_WAL_CHECKPOINT, None, Some(*sequence))
        }
    };

    let path_bytes: &[u8] = match path {
        Some(p) => p
            .to_str()
            .ok_or_else(|| TQLError::Storage("non-UTF-8 segment path".into()))?
            .as_bytes(),
        None => b"",
    };
    let path_len = u16::try_from(path_bytes.len())
        .map_err(|_| TQLError::Storage("segment path too long".into()))?;

    let payload_len = HEADER_SIZE + path_bytes.len() + if extra_u64.is_some() { 8 } else { 0 };
    let mut buf = Vec::with_capacity(payload_len + CRC_SIZE);

    buf.extend_from_slice(&MANIFEST_MAGIC.to_le_bytes());
    buf.push(entry_type);
    buf.extend_from_slice(&path_len.to_le_bytes());
    buf.extend_from_slice(path_bytes);
    if let Some(v) = extra_u64 {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    let crc = crc32(&buf);
    buf.extend_from_slice(&crc.to_le_bytes());
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Binary helpers
// ---------------------------------------------------------------------------

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    let bytes: [u8; 2] = data[offset..offset + 2].try_into().unwrap();
    u16::from_le_bytes(bytes)
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    let bytes: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
    u32::from_le_bytes(bytes)
}

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    let bytes: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
    u64::from_le_bytes(bytes)
}

fn crc32(data: &[u8]) -> u32 {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn open_manifest() -> (NamedTempFile, Manifest) {
        let f = NamedTempFile::new().unwrap();
        let m = Manifest::open(f.path()).unwrap();
        (f, m)
    }

    #[test]
    fn empty_manifest() {
        let (_f, m) = open_manifest();
        assert!(m.live_segments().is_empty());
        assert_eq!(m.max_flushed_sequence(), 0);
    }

    #[test]
    fn add_and_remove_roundtrip() -> Result<()> {
        let (_f, m) = open_manifest();
        let p = Path::new("/tmp/seg_001.seg");
        m.add_segment(p, 100)?;
        assert!(m.live_segments().contains(p));
        assert_eq!(m.max_flushed_sequence(), 100);

        m.remove_segment(p)?;
        assert!(!m.live_segments().contains(p));
        Ok(())
    }

    #[test]
    fn replay_reconstructs_live_set() -> Result<()> {
        let f = NamedTempFile::new()?;
        {
            let m = Manifest::open(f.path())?;
            m.add_segment(Path::new("/tmp/a.seg"), 10)?;
            m.add_segment(Path::new("/tmp/b.seg"), 20)?;
            m.remove_segment(Path::new("/tmp/a.seg"))?;
        }
        // Reopen — replay
        let m = Manifest::open(f.path())?;
        let live = m.live_segments();
        assert!(!live.contains(Path::new("/tmp/a.seg")));
        assert!(live.contains(Path::new("/tmp/b.seg")));
        assert_eq!(m.max_flushed_sequence(), 20);
        Ok(())
    }

    #[test]
    fn max_seq_on_reopen() -> Result<()> {
        let f = NamedTempFile::new()?;
        {
            let m = Manifest::open(f.path())?;
            m.add_segment(Path::new("/tmp/x.seg"), 42)?;
        }
        let m = Manifest::open(f.path())?;
        assert_eq!(m.max_flushed_sequence(), 42);
        Ok(())
    }
}
