//! Crash recovery: replay the WAL into a fresh MemTable.

use crate::{MemTable, Result, TQLError, Wal, WalFrameKind};
use std::path::Path;

/// Replay the WAL at `path` and return a recovered `(MemTable, next_sequence)`.
///
/// `Wal::open` has already truncated any invalid suffix, so every frame
/// returned by `replay()` is guaranteed to be structurally valid and
/// checksummed.  Only committed operations (standalone or fully batched)
/// appear in the replay output.
pub fn recover(path: &Path) -> Result<(MemTable, u64)> {
    let wal = Wal::open(path)?;
    let mem = MemTable::new();

    for frame in wal.replay()? {
        match frame.kind {
            WalFrameKind::Put { key, value } => mem.insert_with_seq(key, value, frame.sequence)?,
            WalFrameKind::Delete { key } => mem.delete_with_seq(&key, frame.sequence)?,
            other => {
                return Err(TQLError::Storage(format!(
                    "unexpected WAL frame kind during recovery: {other:?}"
                )))
            }
        }
    }

    Ok((mem, wal.next_sequence()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VersionedEntry, WalFrameKind};
    use tempfile::NamedTempFile;

    #[test]
    fn empty_wal() -> Result<()> {
        let f = NamedTempFile::new()?;
        let (mem, next_seq) = recover(f.path())?;
        assert!(mem.is_empty());
        assert_eq!(next_seq, 0);
        Ok(())
    }

    #[test]
    fn standalone_operations() -> Result<()> {
        let f = NamedTempFile::new()?;
        {
            let wal = Wal::open(f.path())?;
            wal.append(WalFrameKind::Put {
                key: b"a".to_vec(),
                value: b"1".to_vec(),
            })?;
            wal.append(WalFrameKind::Delete { key: b"b".to_vec() })?;
        }
        let (mem, next_seq) = recover(f.path())?;
        assert_eq!(next_seq, 2);
        assert_eq!(mem.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(mem.get(b"b"), None);
        assert_eq!(mem.get_entry(b"b"), Some(VersionedEntry::tombstone(1)));
        Ok(())
    }

    #[test]
    fn committed_batch() -> Result<()> {
        let f = NamedTempFile::new()?;
        {
            let wal = Wal::open(f.path())?;
            wal.append_batch(&[
                WalFrameKind::Put {
                    key: b"x".to_vec(),
                    value: b"1".to_vec(),
                },
                WalFrameKind::Put {
                    key: b"y".to_vec(),
                    value: b"2".to_vec(),
                },
            ])?;
        }
        let (mem, _) = recover(f.path())?;
        assert_eq!(mem.get(b"x"), Some(b"1".to_vec()));
        assert_eq!(mem.get(b"y"), Some(b"2".to_vec()));
        Ok(())
    }

    #[test]
    fn incomplete_batch_discarded() -> Result<()> {
        // Craft a WAL with BEGIN + PUT but no COMMIT using the WAL's own
        // internal test helper (visible inside the crate via pub(crate)).
        use crate::wal::{write_frame_pub, PhysicalFrame};
        let f = NamedTempFile::new()?;
        {
            let batch_id = 77u64;
            let begin = PhysicalFrame::build(0, batch_id, &WalFrameKind::Begin { batch_id })?;
            let put = PhysicalFrame::build(
                1,
                batch_id,
                &WalFrameKind::Put {
                    key: b"orphan".to_vec(),
                    value: b"lost".to_vec(),
                },
            )?;
            let mut file = std::fs::OpenOptions::new().append(true).open(f.path())?;
            write_frame_pub(&mut file, &begin)?;
            write_frame_pub(&mut file, &put)?;
        }
        let (mem, _) = recover(f.path())?;
        assert!(mem.get(b"orphan").is_none());
        Ok(())
    }

    #[test]
    fn sequence_survives_reopen() -> Result<()> {
        let f = NamedTempFile::new()?;
        {
            let wal = Wal::open(f.path())?;
            wal.append(WalFrameKind::Put {
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            })?;
        }
        let (_, next_seq) = recover(f.path())?;
        assert_eq!(next_seq, 1);
        Ok(())
    }
}
