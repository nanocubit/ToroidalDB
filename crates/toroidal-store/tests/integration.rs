use tempfile::tempdir;
use toroidal_store::{ToroidalStore, WalFrameKind};

// ---------------------------------------------------------------------------
// Basic operations
// ---------------------------------------------------------------------------

#[test]
fn put_get_delete() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    s.put(b"key".to_vec(), b"value".to_vec())?;
    assert_eq!(s.get(b"key"), Some(b"value".to_vec()));
    s.delete(b"key")?;
    assert_eq!(s.get(b"key"), None);
    Ok(())
}

// ---------------------------------------------------------------------------
// Freeze + scan across generations
// ---------------------------------------------------------------------------

#[test]
fn freeze_and_scan() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    s.put(b"a".to_vec(), b"1".to_vec())?;
    s.put(b"b".to_vec(), b"2".to_vec())?;
    s.freeze();
    s.put(b"c".to_vec(), b"3".to_vec())?;
    let r = s.scan(None, None);
    assert_eq!(r.len(), 3);
    assert_eq!(r[0], (b"a".to_vec(), b"1".to_vec()));
    assert_eq!(r[1], (b"b".to_vec(), b"2".to_vec()));
    assert_eq!(r[2], (b"c".to_vec(), b"3".to_vec()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Tombstone suppression across freeze boundary
// ---------------------------------------------------------------------------

#[test]
fn tombstone_suppression() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    s.put(b"a".to_vec(), b"1".to_vec())?;
    s.freeze();
    s.delete(b"a")?;
    assert_eq!(s.get(b"a"), None);
    assert!(s.scan(None, None).is_empty());
    Ok(())
}

// ---------------------------------------------------------------------------
// Atomic batch
// ---------------------------------------------------------------------------

#[test]
fn batch_all_or_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    s.batch(&[
        WalFrameKind::Put {
            key: b"x".to_vec(),
            value: b"1".to_vec(),
        },
        WalFrameKind::Delete { key: b"y".to_vec() },
        WalFrameKind::Put {
            key: b"z".to_vec(),
            value: b"3".to_vec(),
        },
    ])?;
    assert_eq!(s.get(b"x"), Some(b"1".to_vec()));
    assert_eq!(s.get(b"y"), None);
    assert_eq!(s.get(b"z"), Some(b"3".to_vec()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Crash recovery (WAL only, no flush)
// ---------------------------------------------------------------------------

#[test]
fn crash_recovery_no_flush() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    {
        let s = ToroidalStore::open(dir.path())?;
        s.put(b"persist".to_vec(), b"yes".to_vec())?;
        s.delete(b"gone")?;
    }
    let s = ToroidalStore::open(dir.path())?;
    assert_eq!(s.get(b"persist"), Some(b"yes".to_vec()));
    assert_eq!(s.get(b"gone"), None);
    Ok(())
}

// ---------------------------------------------------------------------------
// Multiple freeze generations
// ---------------------------------------------------------------------------

#[test]
fn multiple_freeze_generations() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    s.put(b"k".to_vec(), b"gen0".to_vec())?;
    s.freeze();
    s.put(b"k".to_vec(), b"gen1".to_vec())?;
    s.freeze();
    s.put(b"k".to_vec(), b"gen2".to_vec())?;
    assert_eq!(s.get(b"k"), Some(b"gen2".to_vec()));
    assert_eq!(s.immutable_count(), 2);
    Ok(())
}

// ---------------------------------------------------------------------------
// Scan range
// ---------------------------------------------------------------------------

#[test]
fn scan_range() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    for (k, v) in [("a", "1"), ("b", "2"), ("c", "3"), ("d", "4")] {
        s.put(k.as_bytes().to_vec(), v.as_bytes().to_vec())?;
    }
    let r = s.scan(Some(b"b"), Some(b"d"));
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].0, b"b");
    assert_eq!(r[1].0, b"c");
    Ok(())
}

// ---------------------------------------------------------------------------
// Flush + read
// ---------------------------------------------------------------------------

#[test]
fn flush_and_read() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let s = ToroidalStore::open(dir.path())?;
    s.put(b"a".to_vec(), b"1".to_vec())?;
    s.freeze();
    let n = s.flush()?;
    assert_eq!(n, Some(1));
    assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
    Ok(())
}

// ---------------------------------------------------------------------------
// Flush then recovery
// ---------------------------------------------------------------------------

#[test]
fn flush_then_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    {
        let s = ToroidalStore::open(dir.path())?;
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.flush()?;
        s.put(b"b".to_vec(), b"2".to_vec())?;
    }
    let s = ToroidalStore::open(dir.path())?;
    assert_eq!(s.get(b"a"), Some(b"1".to_vec()));
    assert_eq!(s.get(b"b"), Some(b"2".to_vec()));
    assert_eq!(s.segment_count(), 1);
    Ok(())
}

// ---------------------------------------------------------------------------
// Flush tombstone survives recovery
// ---------------------------------------------------------------------------

#[test]
fn flush_tombstone_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    {
        let s = ToroidalStore::open(dir.path())?;
        s.put(b"a".to_vec(), b"1".to_vec())?;
        s.freeze();
        s.delete(b"a")?;
        s.freeze();
        s.flush()?;
    }
    let s = ToroidalStore::open(dir.path())?;
    assert_eq!(s.get(b"a"), None);
    Ok(())
}
