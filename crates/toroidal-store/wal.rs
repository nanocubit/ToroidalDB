//! Write-Ahead Log (WAL).
//!
//! # Frame layout
//!
//! ```text
//! ┌─────────┬─────────┬────────┬──────────┬──────────┬───────┬─────────┬─────────┐
//! │ len:u32 │magic:u32│ver:u16 │ seq:u64  │batch:u64 │kind:u8│ payload │ crc:u32 │
//! └─────────┴─────────┴────────┴──────────┴──────────┴───────┴─────────┴─────────┘
//! ```
//!
//! All integers are little-endian. `len` covers the entire frame including itself.
//!
//! CRC32 covers: `magic || version || sequence || batch_id || kind || payload`.
//!
//! `batch_id == 0` means a standalone operation.
//!
//! # Batch protocol
//!
//! ```text
//! BEGIN(batch_id)
//!   PUT / DELETE ...
//! COMMIT(batch_id)
//! ```
//!
//! Recovery applies only standalone ops and fully committed batches.
//! An incomplete trailing batch is discarded silently.
//! The invalid suffix is truncated on open so future appends cannot corrupt replay.

use crate::{Result, TQLError};
use crc32fast::Hasher as Crc32Hasher;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAGIC: u32 = 0x544F_524F; // "TORO"
const VERSION: u16 = 1;

const KIND_BEGIN: u8 = 1;
const KIND_PUT: u8 = 2;
const KIND_DELETE: u8 = 3;
const KIND_COMMIT: u8 = 4;

/// Fixed header size (bytes):
/// len(4) + magic(4) + version(2) + sequence(8) + batch_id(8) + kind(1) = 27
const HEADER_SIZE: usize = 27;
const CRC_SIZE: usize = 4;
const MIN_FRAME_SIZE: usize = HEADER_SIZE + CRC_SIZE;

/// Caps allocation during recovery of a corrupt length field.
const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
const MAX_FRAME_SIZE: usize = HEADER_SIZE + MAX_PAYLOAD_SIZE + CRC_SIZE;

const MAX_KEY_SIZE: usize = 16 * 1024 * 1024;
const MAX_VALUE_SIZE: usize = 16 * 1024 * 1024;

/// Reserved: standalone operations carry this batch_id.
const STANDALONE: u64 = 0;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub type Sequence = u64;
pub type BatchId = u64;

/// Logical operation stored in a WAL frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalFrameKind {
    Begin { batch_id: BatchId },
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
    Commit { batch_id: BatchId },
}

impl WalFrameKind {
    fn kind_byte(&self) -> u8 {
        match self {
            Self::Begin { .. } => KIND_BEGIN,
            Self::Put { .. } => KIND_PUT,
            Self::Delete { .. } => KIND_DELETE,
            Self::Commit { .. } => KIND_COMMIT,
        }
    }

    /// Only PUT and DELETE may appear as user-supplied operations.
    pub fn is_operation(&self) -> bool {
        matches!(self, Self::Put { .. } | Self::Delete { .. })
    }
}

/// Logical frame returned to the store / recovery layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalFrame {
    pub sequence: Sequence,
    pub batch_id: BatchId,
    pub kind: WalFrameKind,
}

// ---------------------------------------------------------------------------
// Physical frame
// ---------------------------------------------------------------------------

pub(crate) struct PhysicalFrame {
    pub(crate) sequence: Sequence,
    batch_id: BatchId,
    kind: u8,
    payload: Vec<u8>,
}

impl PhysicalFrame {
    pub(crate) fn build(
        sequence: Sequence,
        batch_id: BatchId,
        kind: &WalFrameKind,
    ) -> Result<Self> {
        validate_batch_id_for_kind(kind, batch_id)?;
        let payload = encode_payload(kind)?;
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(storage("WAL payload exceeds maximum size"));
        }
        Ok(Self {
            sequence,
            batch_id,
            kind: kind.kind_byte(),
            payload,
        })
    }

    pub(crate) fn encoded_len(&self) -> Result<usize> {
        HEADER_SIZE
            .checked_add(self.payload.len())
            .and_then(|n| n.checked_add(CRC_SIZE))
            .filter(|&n| (MIN_FRAME_SIZE..=MAX_FRAME_SIZE).contains(&n))
            .ok_or_else(|| storage("WAL frame length out of bounds"))
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(self.encoded_len()?);
        self.encode_into(&mut buf)?;
        Ok(buf)
    }

    /// Append this frame's bytes to `buf`.  Used both by `encode()` and by
    /// group `append_many` (one preallocated buffer for many frames).
    pub(crate) fn encode_into(&self, buf: &mut Vec<u8>) -> Result<()> {
        let frame_len = self.encoded_len()?;
        let frame_len_u32 =
            u32::try_from(frame_len).map_err(|_| storage("WAL frame exceeds u32 length"))?;

        let base = buf.len();
        // `len` is excluded from CRC coverage.
        buf.extend_from_slice(&frame_len_u32.to_le_bytes());
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        buf.extend_from_slice(&VERSION.to_le_bytes());
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        buf.extend_from_slice(&self.batch_id.to_le_bytes());
        buf.push(self.kind);
        buf.extend_from_slice(&self.payload);

        let crc = crc32(&buf[base + 4..]); // excludes `len`
        buf.extend_from_slice(&crc.to_le_bytes());
        debug_assert_eq!(buf.len() - base, frame_len);
        Ok(())
    }

    fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < MIN_FRAME_SIZE || data.len() > MAX_FRAME_SIZE {
            return Err(storage("WAL frame size out of bounds"));
        }

        let declared_len = read_u32(data, 0)? as usize;
        if declared_len != data.len() {
            return Err(storage("WAL frame length mismatch"));
        }

        let magic = read_u32(data, 4)?;
        if magic != MAGIC {
            return Err(storage("invalid WAL magic"));
        }

        let version = read_u16(data, 8)?;
        if version != VERSION {
            return Err(storage(&format!("unsupported WAL version: {version}")));
        }

        let sequence = read_u64(data, 10)?;
        let batch_id = read_u64(data, 18)?;
        let kind_byte = data[26];

        if !matches!(kind_byte, KIND_BEGIN | KIND_PUT | KIND_DELETE | KIND_COMMIT) {
            return Err(storage("invalid WAL frame kind"));
        }

        let payload_end = data.len() - CRC_SIZE;
        let payload = &data[HEADER_SIZE..payload_end];
        if payload.len() > MAX_PAYLOAD_SIZE {
            return Err(storage("WAL payload exceeds maximum size"));
        }

        let stored_crc = read_u32(data, payload_end)?;
        let calculated_crc = crc32(&data[4..payload_end]); // excludes `len` and `crc`
        if stored_crc != calculated_crc {
            return Err(storage("WAL checksum mismatch"));
        }

        // Reconstruct the logical kind, then validate batch_id consistency.
        let kind = decode_payload_for_kind(kind_byte, batch_id, payload)?;
        validate_batch_id_for_kind(&kind, batch_id)?;

        Ok(Self {
            sequence,
            batch_id,
            kind: kind_byte,
            payload: payload.to_vec(),
        })
    }

    fn to_logical(&self) -> Result<WalFrame> {
        let kind = decode_payload_for_kind(self.kind, self.batch_id, &self.payload)?;
        Ok(WalFrame {
            sequence: self.sequence,
            batch_id: self.batch_id,
            kind,
        })
    }
}

// ---------------------------------------------------------------------------
// WAL
// ---------------------------------------------------------------------------

pub struct Wal {
    /// Append-only file handle, protected by the mutex.
    file: Mutex<File>,
    /// Path used to open a read handle for replay / scan.
    path: PathBuf,
    /// Monotonically increasing. Reads are allowed without the lock.
    next_sequence: AtomicU64,
}

impl Wal {
    /// Open or create a WAL file.
    ///
    /// On open the valid prefix is scanned, the corrupt suffix is truncated,
    /// and `next_sequence` is restored to `max_seen_sequence + 1`.
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(TQLError::Io)?;

        let wal = Self {
            file: Mutex::new(file),
            path: path.to_path_buf(),
            next_sequence: AtomicU64::new(0),
        };

        let (valid_end, max_seq) = wal.scan_valid_prefix()?;

        // Truncate any corrupt trailing bytes so future appends are clean.
        {
            let f = wal.file.lock();
            let actual_len = f.metadata().map_err(TQLError::Io)?.len();
            if valid_end < actual_len {
                f.set_len(valid_end).map_err(TQLError::Io)?;
                f.sync_all().map_err(TQLError::Io)?;
            }
        }

        let next = match max_seq {
            Some(s) => s
                .checked_add(1)
                .ok_or_else(|| storage("WAL sequence exhausted"))?,
            None => 0,
        };
        wal.next_sequence.store(next, Ordering::Release);
        Ok(wal)
    }

    /// Same as `open`, but enforces a minimum sequence floor.
    ///
    /// Use this after a WAL checkpoint/truncation when the manifest already
    /// records a higher sequence than what the (now empty) WAL would restore.
    pub fn open_with_sequence(path: &Path, floor: Sequence) -> Result<Self> {
        let wal = Self::open(path)?;
        let current = wal.next_sequence.load(Ordering::Acquire);
        if current < floor {
            wal.next_sequence.store(floor, Ordering::Release);
        }
        Ok(wal)
    }

    /// Returns the next sequence that will be allocated.
    pub fn next_sequence(&self) -> Sequence {
        self.next_sequence.load(Ordering::Acquire)
    }

    /// Reserve `n` consecutive sequences and return the first one.
    /// The caller is responsible for writing corresponding frames to the
    /// WAL before recovery expects them.
    pub fn reserve_sequences(&self, n: u64) -> Result<Sequence> {
        let first = self.next_sequence.fetch_add(n, Ordering::AcqRel);
        Ok(first)
    }

    // -----------------------------------------------------------------------
    // Standalone write (with fsync)
    // -----------------------------------------------------------------------

    /// Append a single PUT or DELETE, fsynced immediately.
    pub fn append(&self, kind: WalFrameKind) -> Result<Sequence> {
        let seq = self.append_nosync(kind)?;
        self.sync()?;
        Ok(seq)
    }

    /// Append without fsync. Caller must call `sync()` later for durability.
    pub fn append_nosync(&self, kind: WalFrameKind) -> Result<Sequence> {
        if !kind.is_operation() {
            return Err(storage("standalone WAL append requires PUT or DELETE"));
        }
        let mut file = self.file.lock();
        let seq = self.next_seq_locked()?;
        let frame = PhysicalFrame::build(seq, STANDALONE, &kind)?;
        write_frame(&mut file, &frame)?;
        Ok(seq)
    }

    /// fsync the WAL file to disk.
    pub fn sync(&self) -> Result<()> {
        let file = self.file.lock();
        file.sync_all().map_err(TQLError::Io)
    }

    /// Append multiple standalone operations atomically under the file lock,
    /// allocating all sequences and writing+fsyncing in one batch.
    /// Returns the first allocated sequence (caller can derive the rest).
    /// Race-free: concurrent callers cannot interleave frames.
    pub fn append_many_standalone_sync(&self, kinds: &[WalFrameKind]) -> Result<Sequence> {
        if kinds.is_empty() {
            return Err(storage("empty batch not allowed"));
        }
        let mut file = self.file.lock();
        let first = self.next_seq_locked()?;
        let mut buf = Vec::with_capacity(kinds.len() * 64);
        for (i, kind) in kinds.iter().enumerate() {
            if !kind.is_operation() {
                return Err(storage("standalone WAL append requires PUT or DELETE"));
            }
            let seq = first + i as u64;
            let frame = PhysicalFrame::build(seq, STANDALONE, kind)?;
            frame.encode_into(&mut buf)?;
        }
        file.write_all(&buf).map_err(TQLError::Io)?;
        file.sync_all().map_err(TQLError::Io)?;
        Ok(first)
    }

    // -----------------------------------------------------------------------
    // Group append (Phase 12.2) — N standalone ops, one buffered write + one fsync
    // -----------------------------------------------------------------------

    /// Append multiple standalone (batch_id=0) operations with a single
    /// `write_all` followed by a single `fsync`, using pre-allocated
    /// sequences.
    ///
    /// Each entry in `kinds` carries the sequence that the caller has already
    /// reserved (via `next_seq_locked` or `next_sequence`).  Sequences are
    /// written to the WAL in the order given; no extra allocation happens.
    /// Returns `Ok(())` only after the durability barrier succeeds.
    pub fn append_many_at(&self, kinds: &[(WalFrameKind, Sequence)]) -> Result<()> {
        if kinds.is_empty() {
            return Ok(());
        }
        let mut file = self.file.lock();
        let mut buf = Vec::with_capacity(kinds.len() * 64);
        for (kind, seq) in kinds {
            if !kind.is_operation() {
                return Err(storage("standalone WAL append requires PUT or DELETE"));
            }
            let frame = PhysicalFrame::build(*seq, STANDALONE, kind)?;
            frame.encode_into(&mut buf)?;
        }
        file.write_all(&buf).map_err(TQLError::Io)?;
        file.sync_all().map_err(TQLError::Io)?;
        Ok(())
    }

    /// Legacy `append_many` — allocates sequences internally (used by
    /// batcher unit tests).  Prefer `append_many_at` for production paths
    /// that need to know the assigned sequences.
    pub fn append_many(&self, kinds: &[WalFrameKind]) -> Result<()> {
        if kinds.is_empty() {
            return Ok(());
        }
        let mut file = self.file.lock();
        let mut buf = Vec::with_capacity(kinds.len() * 64);
        for kind in kinds {
            if !kind.is_operation() {
                return Err(storage("standalone WAL append requires PUT or DELETE"));
            }
            let seq = self.next_seq_locked()?;
            let frame = PhysicalFrame::build(seq, STANDALONE, kind)?;
            frame.encode_into(&mut buf)?;
        }
        file.write_all(&buf).map_err(TQLError::Io)?;
        file.sync_all().map_err(TQLError::Io)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Atomic batch write
    // -----------------------------------------------------------------------

    /// Append `BEGIN … ops … COMMIT` under a single lock + fsync.
    ///
    /// Returns the `batch_id` (the sequence allocated to BEGIN).
    pub fn append_batch(&self, ops: &[WalFrameKind]) -> Result<BatchId> {
        if ops.is_empty() {
            return Err(storage("empty WAL batch is not allowed"));
        }
        for op in ops {
            if !op.is_operation() {
                return Err(storage("WAL batch may only contain PUT or DELETE"));
            }
        }

        let mut file = self.file.lock();

        // Allocate batch_id (must not be STANDALONE=0).  Advance the sequence
        // past 0 if needed, so the first batch does not carry reserved id 0.
        let batch_id = loop {
            let s = self.next_seq_locked()?;
            if s != STANDALONE {
                break s;
            }
        };
        let begin_seq = batch_id;
        let begin = PhysicalFrame::build(begin_seq, batch_id, &WalFrameKind::Begin { batch_id })?;
        write_frame(&mut file, &begin)?;

        // Operations
        for op in ops {
            let seq = self.next_seq_locked()?;
            let frame = PhysicalFrame::build(seq, batch_id, op)?;
            write_frame(&mut file, &frame)?;
        }

        // COMMIT
        let commit_seq = self.next_seq_locked()?;
        let commit =
            PhysicalFrame::build(commit_seq, batch_id, &WalFrameKind::Commit { batch_id })?;
        write_frame(&mut file, &commit)?;

        // Single fsync for the entire batch.
        file.sync_all().map_err(TQLError::Io)?;
        Ok(batch_id)
    }

    // -----------------------------------------------------------------------
    // Replay
    // -----------------------------------------------------------------------

    /// Return all committed frames in WAL order.
    ///
    /// BEGIN / COMMIT markers are stripped; only PUT / DELETE are returned.
    /// An incomplete trailing batch is silently discarded.
    pub fn replay(&self) -> Result<Vec<WalFrame>> {
        let mut file = File::open(&self.path).map_err(TQLError::Io)?;
        let mut committed: Vec<WalFrame> = Vec::new();
        let mut pending: Option<PendingBatch> = None;

        loop {
            match read_next_frame(&mut file)? {
                FrameRead::Eof | FrameRead::Truncated => break,
                FrameRead::Corrupt(e) => return Err(e),
                FrameRead::Ok(phys) => {
                    let logical = phys.to_logical()?;
                    match &logical.kind {
                        WalFrameKind::Begin { batch_id } => {
                            if *batch_id == STANDALONE {
                                return Err(storage("batch_id zero is reserved"));
                            }
                            if pending.is_some() {
                                return Err(storage("nested WAL batch"));
                            }
                            pending = Some(PendingBatch {
                                batch_id: *batch_id,
                                ops: Vec::new(),
                            });
                        }
                        WalFrameKind::Put { .. } | WalFrameKind::Delete { .. } => {
                            match pending.as_mut() {
                                Some(batch) => {
                                    if logical.batch_id != batch.batch_id {
                                        return Err(storage("WAL op batch_id mismatch"));
                                    }
                                    batch.ops.push(logical);
                                }
                                None => {
                                    if logical.batch_id != STANDALONE {
                                        return Err(storage("standalone op has non-zero batch_id"));
                                    }
                                    committed.push(logical);
                                }
                            }
                        }
                        WalFrameKind::Commit { batch_id } => {
                            let batch = pending
                                .take()
                                .ok_or_else(|| storage("COMMIT without BEGIN"))?;
                            if batch.batch_id != *batch_id {
                                return Err(storage("COMMIT batch_id mismatch"));
                            }
                            // Batch becomes visible atomically here.
                            committed.extend(batch.ops);
                        }
                    }
                }
            }
        }
        // Any pending (uncommitted) batch is intentionally dropped.
        Ok(committed)
    }

    // -----------------------------------------------------------------------
    // Truncation
    // -----------------------------------------------------------------------

    /// Clear the WAL file while preserving the current sequence counter.
    pub fn truncate(&self) -> Result<()> {
        self.truncate_with_floor(self.next_sequence())
    }

    /// Clear the WAL file, ensuring the sequence counter never falls below `floor`.
    pub fn truncate_with_floor(&self, floor: Sequence) -> Result<()> {
        let current = self.next_sequence.load(Ordering::Acquire);
        let next = current.max(floor);
        let file = self.file.lock();
        file.set_len(0).map_err(TQLError::Io)?;
        file.sync_all().map_err(TQLError::Io)?;
        self.next_sequence.store(next, Ordering::Release);
        Ok(())
    }

    /// Truncate the WAL, keeping only frames with `sequence > boundary`.
    /// Frames at or below `boundary` are removed. The sequence counter is
    /// preserved so future appends continue from the correct next value.
    ///
    /// This is the safe truncation for checkpoint: the WAL tail after the
    /// checkpoint boundary (concurrent writes) is preserved, preventing
    /// data loss when a writer was acknowledged after the freeze but before
    /// the checkpoint captured its sequence.
    ///
    /// Batch atomicity: if any frame of a BEGIN/COMMIT batch has a sequence
    /// above the boundary, the ENTIRE batch is preserved so recovery never
    /// sees a dangling COMMIT or an op without its BEGIN.
    pub fn truncate_keep_above(&self, boundary: Sequence) -> Result<()> {
        let mut file = self.file.lock();
        // Read every physical frame in order.
        let mut frames: Vec<PhysicalFrame> = Vec::new();
        let mut read_file = File::open(&self.path).map_err(TQLError::Io)?;
        loop {
            match read_next_frame(&mut read_file)? {
                FrameRead::Eof | FrameRead::Truncated | FrameRead::Corrupt(_) => break,
                FrameRead::Ok(phys) => frames.push(phys),
            }
        }
        // Collect batch_ids that have ANY frame above the boundary.
        let mut kept_batches: std::collections::HashSet<BatchId> = Default::default();
        for f in &frames {
            if f.sequence > boundary && f.batch_id != STANDALONE {
                kept_batches.insert(f.batch_id);
            }
        }
        // Keep frames above the boundary, plus entire straddling batches.
        let mut keep: Vec<&PhysicalFrame> = Vec::new();
        for f in &frames {
            if f.sequence > boundary || kept_batches.contains(&f.batch_id) {
                keep.push(f);
            }
        }
        let next_seq = self.next_sequence.load(Ordering::Relaxed);
        file.set_len(0).map_err(TQLError::Io)?;
        for phys in &keep {
            let encoded = phys.encode()?;
            file.write_all(&encoded).map_err(TQLError::Io)?;
        }
        file.sync_all().map_err(TQLError::Io)?;
        self.next_sequence.store(next_seq, Ordering::Release);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Allocate the next sequence number. Must be called with the file lock held.
    fn next_seq_locked(&self) -> Result<Sequence> {
        let seq = self.next_sequence.load(Ordering::Relaxed);
        let next = seq
            .checked_add(1)
            .ok_or_else(|| storage("WAL sequence exhausted"))?;
        self.next_sequence.store(next, Ordering::Relaxed);
        Ok(seq)
    }

    /// Scan forward and return `(valid_end_offset, Option<max_sequence>)`.
    ///
    /// Stops at EOF, truncation, corruption, or a sequence gap.
    fn scan_valid_prefix(&self) -> Result<(u64, Option<Sequence>)> {
        let mut file = File::open(&self.path).map_err(TQLError::Io)?;
        let mut offset = 0u64;
        let mut prev_seq: Option<Sequence> = None;
        let mut max_seq: Option<Sequence> = None;

        loop {
            match read_next_frame(&mut file)? {
                FrameRead::Eof | FrameRead::Truncated | FrameRead::Corrupt(_) => break,
                FrameRead::Ok(frame) => {
                    // Enforce strict sequence monotonicity.
                    if let Some(prev) = prev_seq {
                        let expected = prev
                            .checked_add(1)
                            .ok_or_else(|| storage("WAL sequence exhausted"))?;
                        if frame.sequence != expected {
                            break;
                        }
                    }
                    prev_seq = Some(frame.sequence);
                    max_seq = Some(frame.sequence);
                    let frame_len = frame.encoded_len()? as u64;
                    offset = offset
                        .checked_add(frame_len)
                        .ok_or_else(|| storage("WAL offset overflow"))?;
                }
            }
        }

        Ok((offset, max_seq))
    }
}

// ---------------------------------------------------------------------------
// Pending batch (replay only)
// ---------------------------------------------------------------------------

struct PendingBatch {
    batch_id: BatchId,
    ops: Vec<WalFrame>,
}

// ---------------------------------------------------------------------------
// Frame reading
// ---------------------------------------------------------------------------

enum FrameRead {
    Eof,
    Truncated,
    Corrupt(TQLError),
    Ok(PhysicalFrame),
}

fn read_next_frame(file: &mut File) -> Result<FrameRead> {
    let mut len_buf = [0u8; 4];
    match file.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(FrameRead::Eof),
        Err(e) => return Err(TQLError::Io(e)),
    }

    let len = u32::from_le_bytes(len_buf) as usize;
    if !(MIN_FRAME_SIZE..=MAX_FRAME_SIZE).contains(&len) {
        return Ok(FrameRead::Corrupt(storage(
            "WAL frame length out of bounds",
        )));
    }

    let mut data = vec![0u8; len];
    data[..4].copy_from_slice(&len_buf);

    match file.read_exact(&mut data[4..]) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(FrameRead::Truncated),
        Err(e) => return Err(TQLError::Io(e)),
    }

    match PhysicalFrame::decode(&data) {
        Ok(frame) => Ok(FrameRead::Ok(frame)),
        Err(e) => Ok(FrameRead::Corrupt(e)),
    }
}

fn write_frame(file: &mut File, frame: &PhysicalFrame) -> Result<()> {
    let encoded = frame.encode()?;
    file.write_all(&encoded).map_err(TQLError::Io)
}

/// Exposed for out-of-module tests that need to craft raw frames.
#[allow(dead_code)]
pub(crate) fn write_frame_pub(file: &mut File, frame: &PhysicalFrame) -> Result<()> {
    write_frame(file, frame)
}

// ---------------------------------------------------------------------------
// Payload encoding / decoding
// ---------------------------------------------------------------------------
//
// BEGIN / COMMIT  → empty payload  (batch_id lives in the fixed header)
// PUT             → key_len:u32 || value_len:u32 || key || value
// DELETE          → key_len:u32 || key

fn encode_payload(kind: &WalFrameKind) -> Result<Vec<u8>> {
    match kind {
        WalFrameKind::Begin { .. } | WalFrameKind::Commit { .. } => Ok(Vec::new()),

        WalFrameKind::Put { key, value } => {
            validate_key(key)?;
            validate_value(value)?;
            let kl = u32::try_from(key.len()).map_err(|_| storage("key too large"))?;
            let vl = u32::try_from(value.len()).map_err(|_| storage("value too large"))?;
            let total = 8usize
                .checked_add(key.len())
                .and_then(|n| n.checked_add(value.len()))
                .filter(|&n| n <= MAX_PAYLOAD_SIZE)
                .ok_or_else(|| storage("WAL PUT payload too large"))?;
            let mut buf = Vec::with_capacity(total);
            buf.extend_from_slice(&kl.to_le_bytes());
            buf.extend_from_slice(&vl.to_le_bytes());
            buf.extend_from_slice(key);
            buf.extend_from_slice(value);
            Ok(buf)
        }

        WalFrameKind::Delete { key } => {
            validate_key(key)?;
            let kl = u32::try_from(key.len()).map_err(|_| storage("key too large"))?;
            let total = 4usize
                .checked_add(key.len())
                .filter(|&n| n <= MAX_PAYLOAD_SIZE)
                .ok_or_else(|| storage("WAL DELETE payload too large"))?;
            let mut buf = Vec::with_capacity(total);
            buf.extend_from_slice(&kl.to_le_bytes());
            buf.extend_from_slice(key);
            Ok(buf)
        }
    }
}

/// Decode payload, using `batch_id` from the frame header for BEGIN/COMMIT.
fn decode_payload_for_kind(
    kind_byte: u8,
    batch_id: BatchId,
    payload: &[u8],
) -> Result<WalFrameKind> {
    match kind_byte {
        KIND_BEGIN => {
            if !payload.is_empty() {
                return Err(storage("BEGIN payload must be empty"));
            }
            Ok(WalFrameKind::Begin { batch_id })
        }
        KIND_COMMIT => {
            if !payload.is_empty() {
                return Err(storage("COMMIT payload must be empty"));
            }
            Ok(WalFrameKind::Commit { batch_id })
        }
        KIND_PUT => {
            if payload.len() < 8 {
                return Err(storage("PUT payload too short"));
            }
            let key_len = read_u32(payload, 0)? as usize;
            let val_len = read_u32(payload, 4)? as usize;
            if key_len > MAX_KEY_SIZE || val_len > MAX_VALUE_SIZE {
                return Err(storage("PUT payload lengths out of bounds"));
            }
            let expected = 8usize
                .checked_add(key_len)
                .and_then(|n| n.checked_add(val_len))
                .ok_or_else(|| storage("PUT payload length overflow"))?;
            if expected != payload.len() {
                return Err(storage("PUT payload length mismatch"));
            }
            let key = payload[8..8 + key_len].to_vec();
            let value = payload[8 + key_len..].to_vec();
            Ok(WalFrameKind::Put { key, value })
        }
        KIND_DELETE => {
            if payload.len() < 4 {
                return Err(storage("DELETE payload too short"));
            }
            let key_len = read_u32(payload, 0)? as usize;
            if key_len > MAX_KEY_SIZE {
                return Err(storage("DELETE key too large"));
            }
            let expected = 4usize
                .checked_add(key_len)
                .ok_or_else(|| storage("DELETE payload length overflow"))?;
            if expected != payload.len() {
                return Err(storage("DELETE payload length mismatch"));
            }
            Ok(WalFrameKind::Delete {
                key: payload[4..].to_vec(),
            })
        }
        _ => Err(storage("invalid WAL kind byte")),
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_batch_id_for_kind(kind: &WalFrameKind, batch_id: BatchId) -> Result<()> {
    match kind {
        WalFrameKind::Begin { batch_id: bid } | WalFrameKind::Commit { batch_id: bid } => {
            if *bid == STANDALONE {
                return Err(storage("batch_id zero is reserved"));
            }
            if *bid != batch_id {
                return Err(storage("WAL batch_id mismatch between kind and header"));
            }
        }
        // PUT / DELETE: both zero (standalone) and non-zero (batched) are valid.
        WalFrameKind::Put { .. } | WalFrameKind::Delete { .. } => {}
    }
    Ok(())
}

fn validate_key(key: &[u8]) -> Result<()> {
    if key.len() > MAX_KEY_SIZE {
        Err(storage("key exceeds maximum size"))
    } else {
        Ok(())
    }
}

fn validate_value(value: &[u8]) -> Result<()> {
    if value.len() > MAX_VALUE_SIZE {
        Err(storage("value exceeds maximum size"))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Binary helpers
// ---------------------------------------------------------------------------

fn read_u16(data: &[u8], offset: usize) -> Result<u16> {
    data.get(offset..offset + 2)
        .and_then(|b| b.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| storage("unexpected EOF reading u16"))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32> {
    data.get(offset..offset + 4)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| storage("unexpected EOF reading u32"))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64> {
    data.get(offset..offset + 8)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| storage("unexpected EOF reading u64"))
}

fn crc32(data: &[u8]) -> u32 {
    let mut h = Crc32Hasher::new();
    h.update(data);
    h.finalize()
}

fn storage(msg: &str) -> TQLError {
    TQLError::Storage(msg.to_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_wal() -> (NamedTempFile, Wal) {
        let f = NamedTempFile::new().unwrap();
        let w = Wal::open(f.path()).unwrap();
        (f, w)
    }

    #[test]
    fn standalone_roundtrip() {
        let (_f, wal) = tmp_wal();
        let seq = wal
            .append(WalFrameKind::Put {
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            })
            .unwrap();
        assert_eq!(seq, 0);
        let frames = wal.replay().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0].kind,
            WalFrameKind::Put {
                key: b"k".to_vec(),
                value: b"v".to_vec()
            }
        );
    }

    #[test]
    fn batch_roundtrip() {
        let (_f, wal) = tmp_wal();
        wal.append_batch(&[
            WalFrameKind::Put {
                key: b"a".to_vec(),
                value: b"1".to_vec(),
            },
            WalFrameKind::Delete { key: b"b".to_vec() },
        ])
        .unwrap();
        let frames = wal.replay().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[0].kind,
            WalFrameKind::Put {
                key: b"a".to_vec(),
                value: b"1".to_vec()
            }
        );
        assert_eq!(frames[1].kind, WalFrameKind::Delete { key: b"b".to_vec() });
    }

    #[test]
    fn incomplete_batch_is_discarded() {
        let f = NamedTempFile::new().unwrap();
        {
            // Write BEGIN + PUT without COMMIT directly to the file.
            let wal = Wal::open(f.path()).unwrap();
            let batch_id = 99u64;
            let begin =
                PhysicalFrame::build(0, batch_id, &WalFrameKind::Begin { batch_id }).unwrap();
            let put = PhysicalFrame::build(
                1,
                batch_id,
                &WalFrameKind::Put {
                    key: b"orphan".to_vec(),
                    value: b"lost".to_vec(),
                },
            )
            .unwrap();
            // Bypass the public API to skip fsync and sequence tracking.
            let mut file = wal.file.lock();
            write_frame(&mut file, &begin).unwrap();
            write_frame(&mut file, &put).unwrap();
        }
        let wal2 = Wal::open(f.path()).unwrap();
        let frames = wal2.replay().unwrap();
        assert!(frames.is_empty(), "incomplete batch must be discarded");
    }

    #[test]
    fn sequence_survives_reopen() {
        let f = NamedTempFile::new().unwrap();
        {
            let wal = Wal::open(f.path()).unwrap();
            wal.append(WalFrameKind::Put {
                key: b"x".to_vec(),
                value: b"1".to_vec(),
            })
            .unwrap();
            wal.append(WalFrameKind::Put {
                key: b"y".to_vec(),
                value: b"2".to_vec(),
            })
            .unwrap();
        }
        let wal2 = Wal::open(f.path()).unwrap();
        assert_eq!(wal2.next_sequence(), 2);
    }

    #[test]
    fn truncate_preserves_sequence() {
        let (_f, wal) = tmp_wal();
        wal.append(WalFrameKind::Put {
            key: b"a".to_vec(),
            value: b"1".to_vec(),
        })
        .unwrap();
        assert_eq!(wal.next_sequence(), 1);
        wal.truncate().unwrap();
        assert_eq!(wal.next_sequence(), 1);
        assert!(wal.replay().unwrap().is_empty());
    }

    #[test]
    fn sequence_floor_on_open() {
        let f = NamedTempFile::new().unwrap();
        let wal = Wal::open_with_sequence(f.path(), 1000).unwrap();
        assert_eq!(wal.next_sequence(), 1000);
    }

    #[test]
    fn empty_wal_next_sequence_is_zero() {
        let (_f, wal) = tmp_wal();
        assert_eq!(wal.next_sequence(), 0);
    }
}
