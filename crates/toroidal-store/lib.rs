//! ToroidalStore — Correctness Core
//!
//! Foundational storage engine for ToroidalDB:
//! - Write-Ahead Log (WAL) with CRC32 checksums and atomic batches
//! - Concurrent MemTable with Value/Tombstone semantics
//! - Crash recovery
//! - Atomic freeze for flush preparation

pub mod batcher;
pub mod cache;
pub mod fault;
pub mod invariants;
pub mod manifest;
pub mod memtable;
pub mod recovery;
pub mod segment;
pub mod snapshot;
pub mod store;
pub mod wal;

pub use batcher::{BatcherOptions, WalBatcher};
pub use cache::*;
pub use fault::{FailAt, FailAtSet, FailureInjector, FailurePoint, NoFault};
pub use manifest::Manifest;
pub use memtable::MemTable;
pub use recovery::recover;
pub use segment::{write_segment, write_segment_nosync, SegmentReader};
pub use snapshot::{
    EntryValue, RetentionHorizon, Snapshot, SnapshotManager, VersionedEntry, LATEST,
};
pub use store::{ToroidalStore, ToroidalStoreOptions};
pub use wal::{BatchId, Sequence, Wal, WalFrameKind};

/// Core error type.
#[derive(thiserror::Error, Debug)]
pub enum TQLError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Snapshot expired: requested seq {requested}, oldest available {oldest_available}")]
    SnapshotExpired {
        requested: u64,
        oldest_available: u64,
    },
}

pub type Result<T> = std::result::Result<T, TQLError>;
