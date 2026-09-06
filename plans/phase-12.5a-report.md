# Phase 12.5A — Storage Correctness Repair

## 1. Executive Summary

Два P0 бага durability/recovery в ToroidalStore были воспроизведены и исправлены:

- **P0-1 (checkpoint concurrent-writer data loss)**: писатель мог выполнить WAL append + fsync, затем быть приостановленным, и checkpoint в этом окне мог захватить его sequence как границу (`checkpoint_seq = wal.next_sequence()`), затем обрезать WAL — запись пропадала (segment: absent, WAL: truncated).
- **P0-2 (compaction crash publication)**: старые segment-файлы удалялись ДО durable manifest REMOVE; crash между удалением файла и fsync REMOVE оставлял manifest, ссылающийся на отсутствующий файл → reopen невозможен.

Оба исправлены минимальными изменениями internal synchronization/protocol — без изменения WAL format, без изменения public API.

## 2. Root Cause

### P0-1 — Checkpoint

Было:
```rust
self.freeze();                       // grab active memtable
// ... release state lock ...
let checkpoint_seq = self.wal.next_sequence();  // ← captures concurrent writer!
// ... manifest + WAL truncate ...
self.wal.truncate_with_floor(checkpoint_seq);   // ← deletes writer's WAL record
```

Writer flow:
```
WAL append + fsync → [window] → MemTable insert
```
Если checkpoint проходит freeze в этом окне и берёт `wal.next_sequence()`, граница включает неза- materialized запись.

**Fix (Variant A + B):**
1. Писатель (put/delete/batch) держит `state.write()` на протяжении ВСЕЙ операции: WAL append + MemTable insert. Freeze (который берёт `state.write()`) больше не может вклиниться между ними.
2. Checkpoint boundary = **max sequence фактически присутствующий в frozen immutables** (`frozen.max_sequence()`), а не WAL tail.
3. WAL truncation использует новый `Wal::truncate_keep_above(boundary)` — сохраняет кадры `> boundary` (concurrent writes) и целые BEGIN/COMMIT батчи, пересекающие границу.

Инвариант: `WAL truncate boundary <= durably materialized state boundary`, и ни один WAL record `> boundary` не усекается.

### P0-2 — Compaction

Было:
```rust
manifest.add_segment(new)?;      // ADD durable
// delete old files  ← crash HERE: manifest still references them
for p in &old_paths { fs::remove_file(p); }
for p in &old_paths { manifest.remove_segment(p)?; }  // REMOVE durable после удаления
```

**Fix:** новый `Manifest::append_compaction_batch(add, removes)` публикует ADD(new) + REMOVE(old) **атомарно в одном fsync batch**, и только ПОСЛЕ этого физически удаляются старые файлы.

Инвариант: после любого crash manifest и filesystem согласованы — каждый `SegmentMeta` в authoritative manifest существует и читается.

## 3. New Protocol

### Checkpoint (P0-1)

```
Writer                          Checkpoint
  │ put/delete/batch              │
  │ state.write() ← ────────────┐ │ freeze() waits for writers
  │   WAL append + fsync         │ │ (state.write() serializes)
  │   fault.check(AfterWalAppend)│ │
  │   MemTable insert            │ │
  │ state.write() release ───────┘ │
  │                               │ freeze() → immutable memtables
  │                               │ batch = drain immutables
  │                               │ checkpoint_seq = max(frozen.max_sequence())
  │                               │ write_segment_nosync (per frozen)
  │                               │ fsync segments
  │                               │ manifest.append_batch(adds, checkpoint_seq)
  │                               │ manifest.sync()
  │                               │ push segments (in-memory)
  │                               │ wal.truncate_keep_above(checkpoint_seq)
  │                               │   (keeps frames > boundary + straddling batches)
```

### Compaction (P0-2)

```
Compaction
  read old segments (clone, under read-lock)
  merge versions (floor-aware, no lock)
  write_segment(new, tmp) + fsync          [C1]
  fault.check(CompactionAfterOutput)
  SegmentReader::open(new)                 [C2 publish in-memory]
  fault.check(CompactionAfterPublish)
  manifest.append_compaction_batch(        [C3/C4 — ADD+REMOVE one batch, fsync]
      ADD(new), REMOVE(old1..oldN))
  fault.check(CompactionAfterManifestBatch)
  delete old segment files                 [C5 — after REMOVE durable]
  fault.check(CompactionAfterOldFileDelete)
```

## 4. Invariants

1. `WAL truncate boundary <= durably materialized boundary` — нет WAL record ниже/равно границе, который не был бы в segments.
2. Ни один WAL record `> checkpoint boundary` не усекается (tail после checkpoint остаётся в WAL).
3. Писатель никогда не наблюдаем checkpoint-ом в состоянии "WAL durably, MemTable not" — пара атомарна под `state.write()`.
4. После любого crash: manifest ссылается только на существующие и структурно читаемые segment-файлы.
5. Старые segment-файлы физически удаляются только после durable REMOVE в manifest.
6. Recovery идемпотентен: reopen → reopen не меняет логическое содержимое.
7. После recovery store остаётся writable.

## 5. Tests

| Test | Crash point | Result |
|---|---|---|
| checkpoint_concurrent_writer_preserved | writer paused at AfterWalAppend, checkpoint runs, crash, reopen | PASS |
| truncate_keep_above_preserves_tail | WAL tail unit | PASS |
| compaction_crash_after_output | AfterCompactionOutput | PASS |
| compaction_crash_after_manifest_batch | CompactionAfterManifestBatch | PASS |
| compaction_crash_after_old_file_delete | CompactionAfterOldFileDelete | PASS |
| checkpoint_crash_after_freeze | AfterMemtableFreeze | PASS |
| checkpoint_crash_after_manifest_sync | AfterManifestSync | PASS |
| checkpoint_crash_before_wal_truncate | BeforeWalTruncate | PASS |
| manifest_segment_existence_invariant | 4 compaction points | PASS |

Все тесты прошли 5+ прогонов — стабильно, без sleep-based race (детерминированные barrier/fault injection).

## 6. SIGKILL Evidence

In-process crash (drop without close) используется в checkpoint/compaction matrix — моделирует SIGKILL: WAL не закрыт, buffered data считается потерянной без replay.

| Process model | Crash point | Recovery | lost | corrupt |
|---|---|---|---|---|
| in-process drop | AfterCompactionOutput | reopen OK | 0 | 0 |
| in-process drop | AfterCompactionManifestBatch | reopen OK | 0 | 0 |
| in-process drop | AfterOldFileDelete | reopen OK | 0 | 0 |
| in-process drop | checkpoint faults x3 | reopen OK | 0 | 0 |
| Phase 12.4 runner (real SIGKILL) | group=64, 757 acked | PASS | 0 | 0 |

## 7. Regression

```
cargo test (crate, 146 tests incl. 9 new):  PASS
cargo test (integration 13):                 PASS (ToroidalDB root)
cargo clippy:                                 (PASS on changed files; pre-existing warnings unchanged)
cargo fmt --all -- --check:                  PASS
Phase 12.4 durability (SIGKILL runner):      PASS (lost=0 corrupt=0 batch_lost=0)
```

## 8. Deferred P1/P2

- snapshot linearization (retention/floor races) — Phase 13+
- empty value encoding (val_len=0 currently means tombstone)
- malformed segment parser hardening
- HybridPersistentStore add_edge atomicity
- concurrent insert / node_count consistency
- query cache invalidation
- auth middleware/secrets
- backup security/semantics
- manifest corrupt suffix full validation
- CI coverage for process-level crash matrix
- README drift

## 9. Verdict

```
PHASE 12.5A: PASS
```