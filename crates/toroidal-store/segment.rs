//! Segment — sorted on-disk LSM block with index and bloom filter.
//!
//! # Format
//!
//! ```text
//! ┌──────────────────┐
//! │ Header (32 B)    │
//! ├──────────────────┤
//! │ Data Block 0     │ ← DEFAULT_BLOCK_SIZE (32 KiB)
//! ├──────────────────┤
//! │ Data Block 1     │
//! ├──────────────────┤
//! │ ...              │
//! ├──────────────────┤
//! │ Bloom Filter     │
//! ├──────────────────┤
//! │ Index Block      │
//! ├──────────────────┤
//! │ Footer (48 B)    │
//! └──────────────────┘
//! ```
//!
//! Header: magic(4) + version(4) + n_entries(8) + max_seq(8) + n_blocks(4) + reserved(4) = 32
//! Each data block: [entry_count:u32] [entries...] [crc32:u32]
//! Each entry: key_len:u32 || key || val_len:u32 || value (val_len=0 = tombstone)
//! Index entry: key_len:u32 || key || block_offset:u64
//! Bloom: k(4) + bits.len(4) + bits
//! Footer: bloom_offset(8) + bloom_len(8) + index_offset(8) + index_len(8) + n_entries(8) + crc(8) = 48

use crate::cache::{Block, BlockCache, BlockId, NoCache, SegmentReadMode};
use crate::{EntryValue, MemTable, Result, TQLError, VersionedEntry};
use crc32fast::Hasher;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// -----------------------------------------------------------------------
// Constants
// -----------------------------------------------------------------------

const MAGIC: u32 = 0x5345_4742; // "SEGB"
/// Current segment format version.
///
/// - v1: entries encoded as `key_len u32 | key | seq u64 | val_len u32 | value`,
///   with `val_len == 0` meaning Tombstone (empty values were
///   indistinguishable from tombstones).
/// - v2: entries encoded as `key_len u32 | key | seq u64 | kind u8 |
///   val_len u32 | value`, where `kind` unambiguously distinguishes
///   `Value(vec![])` from `Tombstone`.  Readers accept both v1 (legacy,
///   with the documented empty-value limitation) and v2.
const VERSION: u32 = 2;
/// Legacy format version — entries have no kind flag; `val_len == 0` is a
/// tombstone and empty values are therefore indistinguishable from
/// tombstones.  Kept for backward compatibility when reading pre-12.5B
/// segments.
const VERSION_LEGACY: u32 = 1;

/// Entry kind flags for v2 blocks.
const KIND_VALUE: u8 = 1;
const KIND_TOMBSTONE: u8 = 2;

pub const DEFAULT_BLOCK_SIZE: usize = 32 * 1024;

const HEADER_SIZE: usize = 32;
const FOOTER_SIZE: usize = 44;
const BLOOM_FP_RATE: f64 = 0.01;

// -----------------------------------------------------------------------
// Metadata
// -----------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SegmentMeta {
    pub id: u64,
    pub file: PathBuf,
    pub level: u32,
    pub min_key: Vec<u8>,
    pub max_key: Vec<u8>,
    pub min_sequence: u64,
    pub max_sequence: u64,
    pub n_entries: u64,
}

// -----------------------------------------------------------------------
// Index
// -----------------------------------------------------------------------

#[derive(Debug, Clone)]
struct IndexEntry {
    first_key: Vec<u8>,
    block_offset: u64,
    block_size: u32, // bytes of the block (including entry_count + CRC)
}

#[derive(Debug, Clone)]
struct Index {
    entries: Vec<IndexEntry>,
}

impl Index {
    fn locate(&self, key: &[u8]) -> Option<usize> {
        let idx = self
            .entries
            .binary_search_by(|e| {
                if key < e.first_key.as_slice() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            })
            .unwrap_or_else(|i| i.saturating_sub(1));
        if idx < self.entries.len() {
            Some(idx)
        } else {
            None
        }
    }
}

// -----------------------------------------------------------------------
// Bloom filter
// -----------------------------------------------------------------------

#[derive(Clone)]
struct BloomFilter {
    bits: Vec<u8>,
    k: u32,
    m: u64, // bits.len() * 8
}

impl BloomFilter {
    fn new(n_entries: usize) -> Self {
        if n_entries == 0 {
            return Self {
                bits: Vec::new(),
                k: 0,
                m: 0,
            };
        }
        // Optimal size: m = -n * ln(p) / ln(2)^2
        // Optimal k = m/n * ln(2)
        let m = (-(n_entries as f64) * BLOOM_FP_RATE.ln() / (2.0_f64.ln().powi(2))).ceil() as u64;
        let m = m.max(64); // minimum 64 bits
        let byte_len = m.div_ceil(8) as usize;
        let k = ((m as f64 / n_entries as f64) * 2.0_f64.ln()).round() as u32;
        let k = k.max(1);
        Self {
            bits: vec![0u8; byte_len],
            k,
            m: byte_len as u64 * 8,
        }
    }

    fn insert(&mut self, key: &[u8]) {
        if self.k == 0 {
            return;
        }
        let mut h = [0u64; 2];
        Self::hash(key, &mut h);
        let (h1, h2) = (h[0], h[1]);
        for i in 0..self.k {
            let idx = (h1.wrapping_add((i as u64).wrapping_mul(h2))) % self.m;
            let byte = (idx / 8) as usize;
            let bit = 1 << (idx % 8);
            if byte < self.bits.len() {
                self.bits[byte] |= bit;
            }
        }
    }

    fn contains(&self, key: &[u8]) -> bool {
        if self.k == 0 || self.bits.is_empty() {
            return false;
        }
        let mut h = [0u64; 2];
        Self::hash(key, &mut h);
        let (h1, h2) = (h[0], h[1]);
        for i in 0..self.k {
            let idx = (h1.wrapping_add((i as u64).wrapping_mul(h2))) % self.m;
            let byte = (idx / 8) as usize;
            let bit = 1 << (idx % 8);
            if byte < self.bits.len() && (self.bits[byte] & bit) == 0 {
                return false;
            }
        }
        true
    }

    fn hash(key: &[u8], out: &mut [u64; 2]) {
        // SipHash-like: use two independent hash values via a simple split.
        let h1 = sip_hash(key, 0);
        let h2 = sip_hash(key, 1);
        out[0] = h1;
        out[1] = h2;
    }
}

// Simple SipHash-style for bloom (not cryptographic, just good distribution).
fn sip_hash(data: &[u8], seed: u64) -> u64 {
    let mut v0 = seed;
    let mut v1 = seed.wrapping_add(0x736f6d6570736575);
    let mut v2 = seed.wrapping_add(0x646f72616e646f6d);
    let mut v3 = seed.wrapping_add(0x6c7967656e657261);
    let mut b = (data.len() as u64) << 56;
    for chunk in data.chunks(8) {
        let mut m = [0u8; 8];
        for (i, &b) in chunk.iter().enumerate() {
            m[i] = b;
        }
        let mi = u64::from_le_bytes(m);
        v3 ^= mi;
        for _ in 0..2 {
            v0 = v0.wrapping_add(v1);
            v1 = v1.rotate_left(13) ^ v0;
            v0 = v0.rotate_left(32);
            v2 = v2.wrapping_add(v3);
            v3 = v3.rotate_left(16) ^ v2;
            v2 = v2.rotate_left(32);
        }
        v0 ^= mi;
    }
    b = b.wrapping_add((data.len() as u64) << 56);
    v3 ^= b;
    for _ in 0..3 {
        v0 = v0.wrapping_add(v1);
        v1 = v1.rotate_left(13) ^ v0;
        v0 = v0.rotate_left(32);
        v2 = v2.wrapping_add(v3);
        v3 = v3.rotate_left(16) ^ v2;
        v2 = v2.rotate_left(32);
    }
    v0 ^ v1 ^ v2 ^ v3
}

// -----------------------------------------------------------------------
// SegmentReader
// -----------------------------------------------------------------------

#[derive(Clone)]
pub struct SegmentReader {
    _data: Arc<Vec<u8>>,
    map: Arc<BTreeMap<Vec<u8>, Vec<VersionedEntry>>>,
    index: Index,
    bloom: BloomFilter,
    meta: SegmentMeta,
    read_mode: SegmentReadMode,
    cache: Arc<dyn BlockCache>,
    /// True when the segment uses the legacy v1 block encoding
    /// (`val_len == 0` ⇒ tombstone, no kind flag).
    legacy_blocks: bool,
}

impl SegmentReader {
    /// Open a segment file in Predecoded mode (default).
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with(path, SegmentReadMode::Predecoded, Arc::new(NoCache))
    }

    /// Open with explicit read mode and block cache.
    pub fn open_with(
        path: &Path,
        read_mode: SegmentReadMode,
        cache: Arc<dyn BlockCache>,
    ) -> Result<Self> {
        let mut raw = Vec::new();
        File::open(path)
            .map_err(TQLError::Io)?
            .read_to_end(&mut raw)
            .map_err(TQLError::Io)?;

        if raw.len() < HEADER_SIZE + FOOTER_SIZE {
            return Err(TQLError::Storage("segment file too small".into()));
        }

        // Validate footer CRC (covers everything except last 4 bytes).
        let file_len = raw.len();
        let footer_crc_off = file_len
            .checked_sub(4)
            .ok_or_else(|| TQLError::Storage("segment file too small".into()))?;
        let stored_crc = read_u32_le(&raw, footer_crc_off);
        let calc_crc = crc32(&raw[..footer_crc_off]);
        if stored_crc != calc_crc {
            return Err(TQLError::Storage("segment CRC mismatch".into()));
        }

        let magic = read_u32_le(&raw, 0);
        if magic != MAGIC {
            return Err(TQLError::Storage("invalid segment magic".into()));
        }

        let version = read_u32_le(&raw, 4);
        let legacy = version == VERSION_LEGACY;
        if !legacy && version != VERSION {
            return Err(TQLError::Storage(format!(
                "unsupported segment version: {version}"
            )));
        }

        let n_entries = read_u64_le(&raw, 8);
        let max_sequence = read_u64_le(&raw, 16);
        let _n_blocks_hdr = read_u32_le(&raw, 24) as usize;

        let footer_start = raw
            .len()
            .checked_sub(FOOTER_SIZE)
            .ok_or_else(|| TQLError::Storage("segment file too small".into()))?;
        let bloom_offset = read_u64_le(&raw, footer_start);
        let bloom_len = read_u64_le(&raw, footer_start + 8);
        let index_offset = read_u64_le(&raw, footer_start + 16);
        let index_len = read_u64_le(&raw, footer_start + 24);

        // P1-3: structural validation — every offset/length must lie within
        // the file image, using checked arithmetic so malformed bytes yield a
        // controlled StorageError instead of a panic.
        let file_u64 = file_len as u64;
        let data_region_end = file_u64
            .checked_sub(FOOTER_SIZE as u64)
            .ok_or_else(|| TQLError::Storage("segment file too small".into()))?;

        let check_range = |off: u64, len: u64, what: &str| -> Result<()> {
            if off < HEADER_SIZE as u64 {
                return Err(TQLError::Storage(format!(
                    "{what} offset before header: {off}"
                )));
            }
            let end = off
                .checked_add(len)
                .ok_or_else(|| TQLError::Storage(format!("{what} length overflow")))?;
            if end > data_region_end {
                return Err(TQLError::Storage(format!(
                    "{what} out of file bounds: off={off} len={len} file={file_u64}"
                )));
            }
            Ok(())
        };

        // Bloom and index layout.
        check_range(bloom_offset, bloom_len, "bloom")?;
        let index_end = index_offset
            .checked_add(index_len)
            .ok_or_else(|| TQLError::Storage("index length overflow".into()))?;
        // Index begins after bloom (or coincides if bloom empty).
        if index_offset < bloom_offset
            || index_offset.checked_add(index_len).is_none()
            || index_end > data_region_end
        {
            return Err(TQLError::Storage("segment index out of file bounds".into()));
        }

        // Parse bloom filter (only used for metadata).
        let bloom = if bloom_len > 0 && bloom_offset >= HEADER_SIZE as u64 {
            let bo = bloom_offset as usize;
            let k = read_u32_le(&raw, bo);
            let bits_len = read_u32_le(&raw, bo + 4) as usize;
            // bits region: [bo+8, bo+8+bits_len) must lie within the bloom
            // region [bloom_offset, bloom_offset+bloom_len) and the file.
            let Some(bits_start) = bo.checked_add(8) else {
                return Err(TQLError::Storage("bloom offset overflow".into()));
            };
            let Some(bits_end) = bits_start.checked_add(bits_len) else {
                return Err(TQLError::Storage("bloom bits length overflow".into()));
            };
            let bloom_region_end = bloom_offset
                .checked_add(bloom_len)
                .ok_or_else(|| TQLError::Storage("bloom length overflow".into()))?;
            if bits_end as u64 > bloom_region_end || bits_end > file_len {
                return Err(TQLError::Storage("bloom out of file bounds".into()));
            }
            let bits = raw[bits_start..bits_end].to_vec();
            let m = bits.len() as u64 * 8;
            BloomFilter { bits, k, m }
        } else {
            BloomFilter {
                bits: Vec::new(),
                k: 0,
                m: 0,
            }
        };

        // Parse index.
        let io = index_offset as usize;
        let il = index_len as usize;
        let index_raw = &raw[io..io + il];
        let index_n = if index_len >= 4 {
            read_u32_le(index_raw, 0) as usize
        } else {
            0
        };
        let mut idx_entries = Vec::with_capacity(index_n.min(1 << 16));
        let mut off = 4usize;
        for _ in 0..index_n {
            let Some(kl) = try_read_u32(index_raw, off).map(|v| v as usize) else {
                return Err(TQLError::Storage(
                    "malformed index: key length out of bounds".into(),
                ));
            };
            off += 4;
            let Some(key_end) = off.checked_add(kl) else {
                return Err(TQLError::Storage("index key length overflow".into()));
            };
            let Some(key) = index_raw.get(off..key_end) else {
                return Err(TQLError::Storage("index key out of bounds".into()));
            };
            let key = key.to_vec();
            off = key_end;
            // Read block_offset (u64) and block_size (u32).
            let Some(block_offset) = try_read_u64(index_raw, off) else {
                return Err(TQLError::Storage(
                    "malformed index: block offset out of bounds".into(),
                ));
            };
            off += 8;
            let Some(block_size) = try_read_u32(index_raw, off) else {
                return Err(TQLError::Storage(
                    "malformed index: block size out of bounds".into(),
                ));
            };
            off += 4;

            // P1-3: validate each index entry's block range against the data
            // region before storing it.
            let bs = block_size as u64;
            check_range(block_offset, bs, "index block")?;

            idx_entries.push(IndexEntry {
                first_key: key,
                block_offset,
                block_size,
            });
        }

        // Parse all data blocks into a BTreeMap.
        let mut map: BTreeMap<Vec<u8>, Vec<VersionedEntry>> = BTreeMap::new();
        // Data blocks live between HEADER_SIZE and the start of the bloom
        // region.  We parse their entries in a bounds-checked way; any block
        // whose internal lengths exceed the region is treated as corruption
        // and aborts the parse with an error (never a panic).
        let mut cursor = HEADER_SIZE as u64;
        let data_end = bloom_offset.min(data_region_end);
        while cursor + 4 <= data_end {
            let c = cursor as usize;
            let entry_count = read_u32_le(&raw, c) as usize;
            cursor += 4;
            for _ in 0..entry_count {
                let Some(kl) = try_read_u32(&raw, cursor as usize).map(|v| v as usize) else {
                    return Err(TQLError::Storage(
                        "malformed block: key length out of bounds".into(),
                    ));
                };
                cursor += 4;
                let Some(key_end) = cursor.checked_add(kl as u64) else {
                    return Err(TQLError::Storage("block key length overflow".into()));
                };
                if key_end > data_end {
                    return Err(TQLError::Storage("block key out of bounds".into()));
                }
                let key = raw[cursor as usize..key_end as usize].to_vec();
                cursor = key_end;
                let Some(sequence) = try_read_u64(&raw, cursor as usize) else {
                    return Err(TQLError::Storage(
                        "malformed block: sequence out of bounds".into(),
                    ));
                };
                cursor += 8;

                let ve = if legacy {
                    let Some(vl) = try_read_u32(&raw, cursor as usize).map(|v| v as usize) else {
                        return Err(TQLError::Storage(
                            "malformed block: value length out of bounds".into(),
                        ));
                    };
                    cursor += 4;
                    if vl == 0 {
                        VersionedEntry::tombstone(sequence)
                    } else {
                        let Some(val_end) = cursor.checked_add(vl as u64) else {
                            return Err(TQLError::Storage("block value length overflow".into()));
                        };
                        if val_end > data_end {
                            return Err(TQLError::Storage("block value out of bounds".into()));
                        }
                        let v = raw[cursor as usize..val_end as usize].to_vec();
                        cursor = val_end;
                        VersionedEntry::value_version(sequence, v)
                    }
                } else {
                    let Some(kind) = try_read_u8(&raw, cursor as usize) else {
                        return Err(TQLError::Storage(
                            "malformed block: kind out of bounds".into(),
                        ));
                    };
                    cursor += 1;
                    if kind == KIND_TOMBSTONE {
                        VersionedEntry::tombstone(sequence)
                    } else if kind == KIND_VALUE {
                        let Some(vl) = try_read_u32(&raw, cursor as usize).map(|v| v as usize)
                        else {
                            return Err(TQLError::Storage(
                                "malformed block: value length out of bounds".into(),
                            ));
                        };
                        cursor += 4;
                        let Some(val_end) = cursor.checked_add(vl as u64) else {
                            return Err(TQLError::Storage("block value length overflow".into()));
                        };
                        if val_end > data_end {
                            return Err(TQLError::Storage("block value out of bounds".into()));
                        }
                        let v = raw[cursor as usize..val_end as usize].to_vec();
                        cursor = val_end;
                        VersionedEntry::value_version(sequence, v)
                    } else {
                        return Err(TQLError::Storage(format!(
                            "malformed block: unknown kind {kind}"
                        )));
                    }
                };
                map.entry(key).or_default().push(ve);
            }
            cursor += 4; // skip block CRC
            if cursor > data_end {
                break;
            }
        }

        let min_key = idx_entries
            .first()
            .map(|e| e.first_key.clone())
            .unwrap_or_default();
        let max_key = idx_entries
            .last()
            .map(|e| e.first_key.clone())
            .unwrap_or_default();

        let data = Arc::new(raw);

        // Derive segment id from filename `seg_{:016}.seg` if present.
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("seg_"))
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0);

        Ok(Self {
            _data: data,
            map: Arc::new(map),
            index: Index {
                entries: idx_entries,
            },
            bloom,
            read_mode,
            cache,
            legacy_blocks: legacy,
            meta: SegmentMeta {
                id,
                file: path.to_path_buf(),
                level: 0,
                min_key,
                max_key,
                min_sequence: 0,
                max_sequence,
                n_entries,
            },
        })
    }

    pub fn meta(&self) -> &SegmentMeta {
        &self.meta
    }

    /// Read a single key from the pre-decoded BTreeMap (hot path).
    pub fn get(&self, key: &[u8]) -> Option<VersionedEntry> {
        self.map.get(key).and_then(|chain| chain.first().cloned())
    }

    /// Block-cached single-key read: bloom → index → cache → decode.
    pub fn get_cached(&self, key: &[u8]) -> Result<Option<VersionedEntry>> {
        if !self.bloom.contains(key) {
            return Ok(None);
        }
        let block_idx = self
            .index
            .locate(key)
            .ok_or_else(|| TQLError::Storage("segment index locate returned no block".into()))?;
        let block = self.read_block_cached(block_idx)?;
        let entries = decode_block(block.data.as_slice(), self.legacy_blocks);
        Ok(entries
            .into_iter()
            .find(|(k, _)| k.as_slice() == key)
            .map(|(_, ve)| ve))
    }

    /// Returns the newest version visible at `seq`.
    pub fn get_visible(&self, key: &[u8], seq: u64) -> Option<VersionedEntry> {
        match self.read_mode {
            SegmentReadMode::Predecoded => self
                .map
                .get(key)
                .and_then(|chain| chain.iter().find(|ve| ve.sequence <= seq).cloned()),
            SegmentReadMode::BlockCached => self
                .get_cached(key)
                .ok()
                .flatten()
                .filter(|ve| ve.sequence <= seq),
        }
    }

    /// Scan all entries, one version per key is NOT enough — return each key's
    /// version chain flattened, sorted by key then sequence desc.
    pub fn scan_entries(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Vec<(Vec<u8>, VersionedEntry)> {
        match self.read_mode {
            SegmentReadMode::Predecoded => self.scan_entries_predecoded(start, end),
            SegmentReadMode::BlockCached => {
                self.scan_entries_cached(start, end).unwrap_or_default()
            }
        }
    }

    fn scan_entries_predecoded(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Vec<(Vec<u8>, VersionedEntry)> {
        self.map
            .iter()
            .filter(|(k, _)| {
                let ok_start = start.is_none_or(|s| k.as_slice() >= s);
                let ok_end = end.is_none_or(|e| k.as_slice() < e);
                ok_start && ok_end
            })
            .flat_map(|(k, chain)| chain.iter().map(move |ve| (k.clone(), ve.clone())))
            .collect()
    }

    /// Block-cached scan: iterates blocks in key range, decodes each.
    pub fn scan_entries_cached(
        &self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, VersionedEntry)>> {
        let start_key = start.unwrap_or(b"");
        let first_block = self.index.locate(start_key).unwrap_or(0);

        let mut out = Vec::new();
        for bi in first_block..self.index.entries.len() {
            let block = self.read_block_cached(bi)?;
            let entries = decode_block(block.data.as_slice(), self.legacy_blocks);
            for (k, ve) in entries {
                if k.as_slice() < start_key {
                    continue;
                }
                if let Some(e) = end {
                    if k.as_slice() >= e {
                        return Ok(out);
                    }
                }
                out.push((k, ve));
            }
        }
        Ok(out)
    }

    /// Fetch the `block_idx`-th data block through the cache, verifying CRC
    /// before insert.  On cache miss: slice from raw file data → verify block
    /// CRC → insert validated block → return.
    fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        let entry = self
            .index
            .entries
            .get(block_idx)
            .ok_or_else(|| TQLError::Storage("segment block index out of bounds".into()))?;

        let id = BlockId::new(self.meta.id, entry.block_offset, entry.block_size);
        if let Some(block) = self.cache.get(id) {
            return Ok(block);
        }

        // Miss: read raw bytes from the in-memory file image.
        let start = entry.block_offset as usize;
        let len = entry.block_size as usize;
        let raw = &self._data;
        let block_bytes = raw
            .get(start..start + len)
            .ok_or_else(|| TQLError::Storage("segment block out of file bounds".into()))?;

        // Verify block CRC before caching.
        if len < 4 {
            return Err(TQLError::Storage("segment block too small".into()));
        }
        let stored_crc = read_u32_le(block_bytes, len - 4);
        let calc_crc = crc32(&block_bytes[..len - 4]);
        if stored_crc != calc_crc {
            return Err(TQLError::Storage("segment block CRC mismatch".into()));
        }

        let block = Arc::new(Block::new(block_bytes.to_vec()));
        self.cache.insert(id, block.clone());
        Ok(block)
    }

    /// Number of entries (including tombstones).
    pub fn len(&self) -> usize {
        self.meta.n_entries as usize
    }

    pub fn is_empty(&self) -> bool {
        self.meta.n_entries == 0
    }

    pub fn path(&self) -> &Path {
        &self.meta.file
    }

    pub fn max_sequence(&self) -> u64 {
        self.meta.max_sequence
    }
}

// -----------------------------------------------------------------------
// Segment writing
// -----------------------------------------------------------------------

/// Build a segment file at `path` from an in-memory MemTable (no fsync).
pub fn write_segment(mem: &MemTable, path: &Path, max_sequence: u64) -> Result<()> {
    write_segment_nosync(mem, path, max_sequence)?;
    let file = File::open(path).map_err(TQLError::Io)?;
    file.sync_all().map_err(TQLError::Io)?;
    Ok(())
}

/// Write segment file without fsync. Caller must sync for durability.
pub fn write_segment_nosync(mem: &MemTable, path: &Path, max_sequence: u64) -> Result<()> {
    // Persist ALL versions, not just newest, so snapshots remain correct
    // after flush. scan_all_versions returns key-major, sequence-minor
    // (newest first) order.
    let entries = mem.scan_all_versions();
    if entries.is_empty() {
        return write_empty_segment(path, max_sequence);
    }

    let n_total = entries.len();
    let mut bloom = BloomFilter::new(n_total);
    for (key, _) in &entries {
        bloom.insert(key);
    }

    write_segment_impl(&entries, path, max_sequence, &bloom)
}

/// Write a segment with entries grouped into blocks.
fn write_segment_impl(
    entries: &[(Vec<u8>, VersionedEntry)],
    path: &Path,
    max_sequence: u64,
    bloom: &BloomFilter,
) -> Result<()> {
    if entries.is_empty() {
        return write_empty_segment(path, max_sequence);
    }

    let n_total = entries.len();
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut block_first_keys: Vec<Vec<u8>> = Vec::new();
    let mut block_offsets: Vec<u64> = Vec::new();
    let mut block_sizes: Vec<u32> = Vec::new();

    let mut current_block: Vec<(Vec<u8>, VersionedEntry)> = Vec::new();
    let mut current_size = 0usize;

    for (key, entry) in entries {
        let entry_size = 4 + key.len() + 8 + 4 + entry.value.byte_size(); // key_len + key + seq + val_len + value

        if current_size + entry_size > DEFAULT_BLOCK_SIZE && !current_block.is_empty() {
            block_first_keys.push(current_block[0].0.clone());
            block_offsets
                .push(HEADER_SIZE as u64 + blocks.iter().map(|b| b.len() as u64).sum::<u64>());
            let encoded = encode_block(&current_block);
            block_sizes.push(encoded.len() as u32);
            blocks.push(encoded);
            current_block.clear();
            current_size = 0;
        }
        current_block.push((key.clone(), entry.clone()));
        current_size += entry_size;
    }
    if !current_block.is_empty() {
        block_first_keys.push(current_block[0].0.clone());
        block_offsets.push(HEADER_SIZE as u64 + blocks.iter().map(|b| b.len() as u64).sum::<u64>());
        let encoded = encode_block(&current_block);
        block_sizes.push(encoded.len() as u32);
        blocks.push(encoded);
    }

    // Bloom filter serialization.
    let bloom_buf = if bloom.k > 0 {
        let mut buf = Vec::new();
        buf.extend_from_slice(&bloom.k.to_le_bytes());
        buf.extend_from_slice(&(bloom.bits.len() as u32).to_le_bytes());
        buf.extend_from_slice(&bloom.bits);
        buf
    } else {
        vec![0u8; 8] // k=0, bits_len=0
    };

    let bloom_offset = HEADER_SIZE as u64 + blocks.iter().map(|b| b.len() as u64).sum::<u64>();

    // Index serialization.
    let mut index_buf = Vec::new();
    let index_n = u32::try_from(blocks.len())
        .map_err(|_| TQLError::Storage("too many segment blocks".into()))?;
    index_buf.extend_from_slice(&index_n.to_le_bytes());
    for (i, fk) in block_first_keys.iter().enumerate() {
        let kl = u32::try_from(fk.len())
            .map_err(|_| TQLError::Storage("segment key too large".into()))?;
        index_buf.extend_from_slice(&kl.to_le_bytes());
        index_buf.extend_from_slice(fk);
        index_buf.extend_from_slice(&block_offsets[i].to_le_bytes());
        index_buf.extend_from_slice(&block_sizes[i].to_le_bytes());
    }

    let index_offset = bloom_offset + bloom_buf.len() as u64;

    // Final assembly: header + blocks + bloom + index + footer.
    let mut buf = Vec::new();

    // Header.
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(n_total as u64).to_le_bytes());
    buf.extend_from_slice(&max_sequence.to_le_bytes());
    buf.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]); // reserved

    // Data blocks.
    for block in &blocks {
        buf.extend_from_slice(block);
    }

    // Bloom filter.
    buf.extend_from_slice(&bloom_buf);

    // Index.
    buf.extend_from_slice(&index_buf);

    // Footer + CRC (CRC covers everything except the trailing 4 CRC bytes).
    buf.extend_from_slice(&bloom_offset.to_le_bytes());
    buf.extend_from_slice(&(bloom_buf.len() as u64).to_le_bytes());
    buf.extend_from_slice(&index_offset.to_le_bytes());
    buf.extend_from_slice(&(index_buf.len() as u64).to_le_bytes());
    buf.extend_from_slice(&(n_total as u64).to_le_bytes());

    let footer_crc = crc32(&buf);
    buf.extend_from_slice(&footer_crc.to_le_bytes());

    let mut file = File::create(path).map_err(TQLError::Io)?;
    file.write_all(&buf).map_err(TQLError::Io)?;
    // No fsync — caller groups durability via sync_all on all segment files.
    Ok(())
}

/// Write an empty segment (no data blocks, no bloom, empty index) — no fsync.
fn write_empty_segment(path: &Path, max_sequence: u64) -> Result<()> {
    let mut buf = Vec::new();

    // Header.
    buf.extend_from_slice(&MAGIC.to_le_bytes());
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes()); // n_entries
    buf.extend_from_slice(&max_sequence.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // n_blocks
    buf.extend_from_slice(&[0u8; 4]); // reserved

    let bloom_offset = buf.len() as u64;

    // Empty index (just 0).
    buf.extend_from_slice(&0u32.to_le_bytes());

    // Footer + CRC (CRC covers everything except the trailing 4 CRC bytes).
    let index_offset = bloom_offset; // bloom starts here but len=0, so index follows
    buf.extend_from_slice(&bloom_offset.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes()); // bloom_len
    buf.extend_from_slice(&index_offset.to_le_bytes());
    buf.extend_from_slice(&4u64.to_le_bytes()); // index_len (4 bytes: u32 zero)
    buf.extend_from_slice(&0u64.to_le_bytes()); // n_entries

    let footer_crc = crc32(&buf);
    buf.extend_from_slice(&footer_crc.to_le_bytes());

    let mut file = File::create(path).map_err(TQLError::Io)?;
    file.write_all(&buf).map_err(TQLError::Io)?;
    // No fsync — caller groups durability via sync_all on all segment files.
    Ok(())
}

fn encode_block(entries: &[(Vec<u8>, VersionedEntry)]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (key, entry) in entries {
        buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
        buf.extend_from_slice(key);
        buf.extend_from_slice(&entry.sequence.to_le_bytes());
        match &entry.value {
            // v2: explicit kind flag before value length, so ``Value(vec![])``
            // is distinguishable from ``Tombstone``.
            EntryValue::Value(v) => {
                buf.push(KIND_VALUE);
                buf.extend_from_slice(&(v.len() as u32).to_le_bytes());
                buf.extend_from_slice(v);
            }
            EntryValue::Tombstone => {
                buf.push(KIND_TOMBSTONE);
            }
        }
    }
    let crc = crc32(&buf);
    buf.extend_from_slice(&crc.to_le_bytes());
    buf
}

// -----------------------------------------------------------------------
// Binary helpers
// -----------------------------------------------------------------------

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    let bytes: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
    u32::from_le_bytes(bytes)
}

fn read_u64_le(data: &[u8], offset: usize) -> u64 {
    let bytes: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
    u64::from_le_bytes(bytes)
}

/// Fallible u32 read — returns `None` when the 4 bytes are not within bounds.
fn try_read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// Fallible u64 read — returns `None` when the 8 bytes are not within bounds.
fn try_read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

/// Fallible u8 read — returns `None` when the byte is not within bounds.
fn try_read_u8(data: &[u8], offset: usize) -> Option<u8> {
    Some(*data.get(offset)?)
}

fn crc32(data: &[u8]) -> u32 {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

/// Decode a raw data block into versioned entries.
///
/// P1-2: v2 blocks store an explicit `kind:u8` flag so `Value(vec![])` is
/// unambiguously distinguishable from `Tombstone`.  Legacy v1 blocks (no
/// kind flag, `val_len == 0` ⇒ tombstone) remain decodable when the reader
/// is opened in legacy mode.
///
/// P1-3: every length/offset is bounds-checked; malformed blocks yield an
/// empty parse (caller treats that as corruption) instead of panicking.
fn decode_block(block: &[u8], legacy: bool) -> Vec<(Vec<u8>, VersionedEntry)> {
    if block.len() < 4 {
        return Vec::new();
    }
    let Some(n32) = try_read_u32(block, 0) else {
        return Vec::new();
    };
    let n = n32 as usize;
    let mut off = 4usize;
    let mut entries = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        // key_len
        let Some(kl) = try_read_u32(block, off).map(|v| v as usize) else {
            return Vec::new();
        };
        off += 4;
        let Some(key_end) = off.checked_add(kl) else {
            return Vec::new();
        };
        let Some(key) = block.get(off..key_end) else {
            return Vec::new();
        };
        let key = key.to_vec();
        off = key_end;
        // sequence
        let Some(sequence) = try_read_u64(block, off) else {
            return Vec::new();
        };
        off += 8;

        let ve = if legacy {
            // v1: val_len == 0 ⇒ tombstone
            let Some(vl) = try_read_u32(block, off).map(|v| v as usize) else {
                return Vec::new();
            };
            off += 4;
            if vl == 0 {
                VersionedEntry::tombstone(sequence)
            } else {
                let Some(val_end) = off.checked_add(vl) else {
                    return Vec::new();
                };
                let Some(val) = block.get(off..val_end) else {
                    return Vec::new();
                };
                off = val_end;
                VersionedEntry::value_version(sequence, val.to_vec())
            }
        } else {
            // v2: explicit kind flag
            let Some(kind) = try_read_u8(block, off) else {
                return Vec::new();
            };
            off += 1;
            if kind == KIND_TOMBSTONE {
                // v2 tombstones do not carry a value length — the flag is
                // the discriminator. Push the tombstone entry.
                VersionedEntry::tombstone(sequence)
            } else if kind == KIND_VALUE {
                let Some(vl) = try_read_u32(block, off).map(|v| v as usize) else {
                    return Vec::new();
                };
                off += 4;
                let Some(val_end) = off.checked_add(vl) else {
                    return Vec::new();
                };
                let Some(val) = block.get(off..val_end) else {
                    return Vec::new();
                };
                off = val_end;
                VersionedEntry::value_version(sequence, val.to_vec())
            } else {
                // Unknown kind byte — malformed.
                return Vec::new();
            }
        };
        entries.push((key, ve));
    }
    entries
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use crate::EntryValue;
    use tempfile::NamedTempFile;

    fn roundtrip(entries: Vec<(&[u8], EntryValue)>) -> SegmentReader {
        let mem = MemTable::new();
        for (k, e) in &entries {
            match e {
                EntryValue::Value(v) => mem.insert_with_seq(k.to_vec(), v.clone(), 1).unwrap(),
                EntryValue::Tombstone => mem.delete_with_seq(k, 2).unwrap(),
            }
        }
        let f = NamedTempFile::new().unwrap();
        let max_seq = 42;
        write_segment(&mem, f.path(), max_seq).unwrap();
        SegmentReader::open(f.path()).unwrap()
    }

    #[test]
    fn write_and_read() {
        let r = roundtrip(vec![(b"a", EntryValue::Value(b"1".to_vec()))]);
        let got = r.get(b"a").unwrap();
        assert_eq!(got.value, EntryValue::Value(b"1".to_vec()));
        assert!(r.get(b"b").is_none());
    }

    #[test]
    fn tombstone_is_preserved() {
        let r = roundtrip(vec![
            (b"a", EntryValue::Value(b"1".to_vec())),
            (b"b", EntryValue::Tombstone),
        ]);
        assert_eq!(r.get(b"b").unwrap().value, EntryValue::Tombstone);
    }

    #[test]
    fn scan_respects_range() {
        let r = roundtrip(vec![
            (b"a", EntryValue::Value(b"1".to_vec())),
            (b"b", EntryValue::Value(b"2".to_vec())),
            (b"c", EntryValue::Value(b"3".to_vec())),
        ]);
        let results = r.scan_entries(Some(b"b"), Some(b"d"));
        assert_eq!(results.len(), 2);
        assert_eq!(&results[0].0, b"b");
        assert_eq!(&results[1].0, b"c");
    }

    #[test]
    fn corrupt_crc_detected() {
        let mem = MemTable::new();
        mem.insert_with_seq(b"x".to_vec(), b"y".to_vec(), 1)
            .unwrap();
        let f = NamedTempFile::new().unwrap();
        write_segment(&mem, f.path(), 0).unwrap();
        let mut raw = std::fs::read(f.path()).unwrap();
        raw[HEADER_SIZE + 10] ^= 0xFF;
        std::fs::write(f.path(), &raw).unwrap();
        assert!(SegmentReader::open(f.path()).is_err());
    }

    #[test]
    fn bloom_rejects_absent_key() {
        let r = roundtrip(vec![
            (b"a", EntryValue::Value(b"1".to_vec())),
            (b"c", EntryValue::Value(b"3".to_vec())),
        ]);
        assert!(r.get(b"b").is_none());
    }

    #[test]
    fn large_entries_span_blocks() {
        let mut entries = Vec::new();
        for i in 0..1000 {
            let key = format!("k-{:04}", i);
            let val = vec![b'x'; 64];
            entries.push((key.as_bytes().to_vec(), EntryValue::Value(val)));
        }
        let mem = MemTable::new();
        for (k, e) in &entries {
            match e {
                EntryValue::Value(v) => mem.insert_with_seq(k.clone(), v.clone(), 1).unwrap(),
                EntryValue::Tombstone => mem.delete_with_seq(k, 2).unwrap(),
            }
        }
        let f = NamedTempFile::new().unwrap();
        write_segment(&mem, f.path(), 99).unwrap();
        let r = SegmentReader::open(f.path()).unwrap();
        assert_eq!(r.len(), 1000);
        for i in 0..1000 {
            let key = format!("k-{:04}", i);
            assert!(r.get(key.as_bytes()).is_some());
        }
    }

    #[test]
    fn empty_segment() {
        let mem = MemTable::new();
        let f = NamedTempFile::new().unwrap();
        write_segment(&mem, f.path(), 0).unwrap();
        let r = SegmentReader::open(f.path()).unwrap();
        assert!(r.is_empty());
        assert!(r.scan_entries(None, None).is_empty());
    }

    #[test]
    fn segment_meta() {
        let r = roundtrip(vec![(b"x", EntryValue::Value(b"y".to_vec()))]);
        assert_eq!(r.max_sequence(), 42);
        assert_eq!(r.meta().n_entries, 1);
    }

    #[test]
    fn cached_predecoded_equality() {
        let mem = MemTable::new();
        mem.insert_with_seq(b"a".to_vec(), b"1".to_vec(), 10)
            .unwrap();
        mem.insert_with_seq(b"b".to_vec(), b"2".to_vec(), 20)
            .unwrap();
        mem.delete_with_seq(b"c", 30).unwrap();
        let f = NamedTempFile::new().unwrap();
        write_segment(&mem, f.path(), 99).unwrap();

        let cache = Arc::new(Cache::new(1_000_000, 2));
        let pre = SegmentReader::open(f.path()).unwrap();
        let cached =
            SegmentReader::open_with(f.path(), SegmentReadMode::BlockCached, cache.clone())
                .unwrap();

        // get — every key
        for key in [b"a", b"b", b"c", b"z"] {
            assert_eq!(pre.get(key), cached.get(key), "mismatch for key {:?}", key);
        }

        // get_visible
        for seq in [5, 15, 25, 99] {
            assert_eq!(
                pre.get_visible(b"a", seq),
                cached.get_visible(b"a", seq),
                "mismatch visible {:?}",
                seq
            );
        }

        // scan
        assert_eq!(
            pre.scan_entries(None, None),
            cached.scan_entries(None, None)
        );
        assert_eq!(
            pre.scan_entries(Some(b"b"), Some(b"z")),
            cached.scan_entries(Some(b"b"), Some(b"z"))
        );

        // cache hit ratio > 0 after warmup
        let stats = cache.stats();
        assert!(stats.hits > 0, "expected at least one cache hit");
    }

    #[test]
    fn cache_does_not_leak_across_segments() {
        let cache = Arc::new(Cache::new(1_000_000, 2));

        let id_a = BlockId::new(1, 100, 64);
        let id_b = BlockId::new(2, 100, 64);
        cache.insert(id_a, Arc::new(Block::new(vec![0; 64])));
        assert!(cache.get(id_a).is_some());
        assert!(
            cache.get(id_b).is_none(),
            "same offset in different segment must not collide"
        );
    }
}
