//! Group-commit batcher (Phase 12.2–12.4) — race-free bounded queue that
//! coalesces standalone WAL ops into one `append_many_standalone_sync`.
//!
//! Durability invariant:
//!   ACK is returned to the caller ONLY after the group's fsync completed.
//!
//! Two paths:
//!   Direct-sync (batch_size ≤ 1): seq allocation + write + fsync under one
//!     WAL file lock.  Race-free.  Used by store.put() / store.delete().
//!   Worker path (batch_size > 1): queue drain, seq allocation under file lock,
//!     one write + one fsync.  Race-free via per-batch seq allocation.

use crate::{Result, Sequence, TQLError, Wal, WalFrameKind};
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct BatcherOptions {
    pub max_batch_size: usize,
    pub max_wait: Duration,
}

impl Default for BatcherOptions {
    fn default() -> Self {
        Self {
            max_batch_size: 64,
            max_wait: Duration::from_millis(1),
        }
    }
}

struct PendingOp {
    kind: WalFrameKind,
    ack: Arc<AckSlot>,
}

struct AckSlot {
    done: Mutex<Option<Result<()>>>,
    cv: Condvar,
}

impl AckSlot {
    fn new() -> Self {
        Self {
            done: Mutex::new(None),
            cv: Condvar::new(),
        }
    }
    fn wait(&self) -> Result<()> {
        let mut guard = self.done.lock();
        while guard.is_none() {
            self.cv.wait(&mut guard);
        }
        guard.take().unwrap()
    }
    fn signal(&self, res: Result<()>) {
        *self.done.lock() = Some(res);
        self.cv.notify_all();
    }
}

struct Shared {
    wal: Arc<Wal>,
    options: BatcherOptions,
    queue: Mutex<VecDeque<PendingOp>>,
    cv: Condvar,
    shutting_down: Mutex<bool>,
    force_flush: Mutex<bool>,
}

pub struct WalBatcher {
    shared: Arc<Shared>,
    #[allow(dead_code)]
    worker: Option<JoinHandle<()>>,
}

impl WalBatcher {
    pub fn new(wal: Arc<Wal>, options: BatcherOptions) -> Self {
        let shared = Arc::new(Shared {
            wal,
            options,
            queue: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            shutting_down: Mutex::new(false),
            force_flush: Mutex::new(false),
        });
        let s = shared.clone();
        let worker = thread::spawn(move || worker_loop(s));
        Self {
            shared,
            worker: Some(worker),
        }
    }

    /// Race-free direct-sync put: allocates seq + write + fsync under ONE
    /// WAL file lock.  Returns the allocated sequence for MemTable insert.
    pub fn append_sync_direct(&self, kind: WalFrameKind) -> Result<Sequence> {
        self.shared.wal.append_many_standalone_sync(&[kind])
    }

    /// Group sync path: reserves sequences + writes + fsyncs under ONE lock.
    /// Returns the first sequence in the group.
    pub fn append_many_sync(&self, kinds: &[WalFrameKind]) -> Result<Sequence> {
        self.shared.wal.append_many_standalone_sync(kinds)
    }

    /// Background-worker path: enqueue for later coalescing.
    /// Sequences are assigned by the worker under the file lock (race-free).
    pub fn append_sync_background(&self, kind: WalFrameKind) -> Result<()> {
        let ack = Arc::new(AckSlot::new());
        {
            self.shared.queue.lock().push_back(PendingOp {
                kind,
                ack: ack.clone(),
            });
        }
        self.shared.cv.notify_one();
        ack.wait()
    }

    /// Force-drain all pending operations.
    pub fn flush(&self) -> Result<()> {
        {
            let q = self.shared.queue.lock();
            if q.is_empty() {
                return Ok(());
            }
        }
        *self.shared.force_flush.lock() = true;
        self.shared.cv.notify_all();
        let mut q = self.shared.queue.lock();
        while !q.is_empty() {
            self.shared.cv.wait(&mut q);
        }
        Ok(())
    }

    pub fn shutdown(&self) -> Result<()> {
        *self.shared.shutting_down.lock() = true;
        *self.shared.force_flush.lock() = true;
        self.shared.cv.notify_all();
        let mut q = self.shared.queue.lock();
        while !q.is_empty() {
            self.shared.cv.wait(&mut q);
        }
        Ok(())
    }
}

impl Drop for WalBatcher {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn worker_loop(shared: Arc<Shared>) {
    loop {
        let deadline = {
            let mut q = shared.queue.lock();
            loop {
                if !q.is_empty() {
                    break Instant::now() + shared.options.max_wait;
                }
                if *shared.shutting_down.lock() {
                    return;
                }
                shared.cv.wait_for(&mut q, shared.options.max_wait);
            }
        };
        let batch: Vec<PendingOp> = {
            let mut q = shared.queue.lock();
            while q.len() < shared.options.max_batch_size
                && !*shared.shutting_down.lock()
                && !*shared.force_flush.lock()
                && Instant::now() < deadline
            {
                shared.cv.wait_for(&mut q, deadline - Instant::now());
            }
            let mut out = Vec::with_capacity(q.len());
            while let Some(op) = q.pop_front() {
                out.push(op);
            }
            out
        };
        if batch.is_empty() {
            continue;
        }
        *shared.force_flush.lock() = false;

        let kinds: Vec<WalFrameKind> = batch.iter().map(|o| o.kind.clone()).collect();
        let result = shared
            .wal
            .append_many_standalone_sync(&kinds)
            .map_err(|e| TQLError::Storage(format!("group commit failed: {e}")));

        for op in batch {
            let res = result
                .as_ref()
                .map(|_| ())
                .map_err(|e| TQLError::Storage(format!("{e}")));
            op.ack.signal(res);
        }
        shared.cv.notify_all();
        if *shared.shutting_down.lock() && shared.queue.lock().is_empty() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Wal;
    use tempfile::tempdir;

    fn put(k: &[u8], v: &[u8]) -> WalFrameKind {
        WalFrameKind::Put {
            key: k.to_vec(),
            value: v.to_vec(),
        }
    }

    fn make_batcher(dir: &std::path::Path, bs: usize, mw: u64) -> WalBatcher {
        let wal = Arc::new(Wal::open(&dir.join("wal")).unwrap());
        WalBatcher::new(
            wal,
            BatcherOptions {
                max_batch_size: bs,
                max_wait: Duration::from_millis(mw),
            },
        )
    }

    #[test]
    fn single_put_acks_after_durability() {
        let dir = tempdir().unwrap();
        let b = make_batcher(dir.path(), 1, 100);
        b.append_sync_direct(put(b"k", b"v")).unwrap();
        b.shutdown().unwrap();
        let store = crate::ToroidalStore::open(dir.path()).unwrap();
        assert_eq!(store.get(b"k"), Some(b"v".to_vec()));
    }

    #[test]
    fn many_puts_survive_reopen() {
        let dir = tempdir().unwrap();
        let b = make_batcher(dir.path(), 16, 5);
        for i in 0..100 {
            let k = format!("k{:04}", i);
            b.append_sync_direct(put(k.as_bytes(), b"v")).unwrap();
        }
        b.shutdown().unwrap();
        let store = crate::ToroidalStore::open(dir.path()).unwrap();
        for i in 0..100 {
            let k = format!("k{:04}", i);
            assert!(store.get(k.as_bytes()).is_some(), "key {} lost", i);
        }
    }

    // ... (truncated to fit) ...
}
