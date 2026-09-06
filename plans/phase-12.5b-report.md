# Phase 12.5B — Storage Correctness Completion

## 1. Executive Summary

Четыре storage correctness issues исправлены и доказаны тестами:

- **P1-1 Snapshot linearization**: snapshot sequence теперь = published visibility boundary (max sequence в active+immutable+segments под state lock), а НЕ WAL allocation tail. Запись, которая WAL-durable но ещё не опубликована в MemTable, больше не становится snapshot-visible.
- **P1-2 Empty value vs tombstone**: segment format v2 вводит явный `kind:u8` флаг (KIND_VALUE=1, KIND_TOMBSTONE=2), позволяющий отличить `put("k", [])` от `delete("k")`. Legacy v1 сегменты читаются с сохранением совместимости (с задокументированным ограничением).
- **P1-3 Segment structural validation**: SegmentReader теперь использует checked arithmetic и validaцию всех offset/length; malformed bytes → StorageError, никогда panic.
- **P2 Manifest corrupt suffix**: реплей отслеживает watermark последней валидной записи; corrupt/truncated suffix обрезается на open (set_len + fsync), последующие append'ы стартуют с чистого состояния.

## 2. Snapshot Linearization (P1-1)

### Root cause

```rust
// было:
let seq = self.wal.next_sequence().saturating_sub(1);
```

WAL tail может включать писателя, который зааппендил WAL (durable) но ещё не опубликовал в MemTable. Snapshot по такому seq "видел" запись, которая не была logical state на момент создания snapshot.

### New invariant

> Snapshot sequence = published visibility boundary of the logical store.
> For every snapshot S: all writes with sequence <= S are visible iff committed before the snapshot boundary. A write that is WAL-durable but not MemTable-published is never visible.

### Implementation

```rust
pub fn snapshot(&self) -> Snapshot {
    let seq = {
        let state = self.state.read();  // то же состояние, что держат писатели
        max(active.max_sequence(), immutables..., segments...)
    };
    Snapshot::new_managed(seq, self.snapshot_manager.clone())
}
```

Writer держит `state.write()` от WAL append до MemTable insert (фикс 12.5A), поэтому `state.read()` в snapshot — точка линейциализации: писатель либо полностью опубликован, либо вовсе не виден.

### Tests

| Test | Результат |
|---|---|
| snapshot_does_not_see_unpublished_wal_tail | PASS — snapshot блокируется пока writer не опубликован, не видит запись |
| snapshot_boundary_is_published_state | PASS — граница точно отражает опубликованное состояние |
| snapshot_survives_flush_checkpoint_compaction | PASS — snapshot semantics корректны через весь lifecycle |

## 3. Empty Values (P1-2)

### Previous encoding (v1)

`val_len == 0` означал Tombstone → `put("k", [])` после flush/reopen превращалось в `delete("k")`.

### New encoding (v2)

```
entry = key_len:u32 | key | seq:u64 | kind:u8 | [val_len:u32 | value]
kind: 1 = VALUE, 2 = TOMBSTONE
```

- `Value(vec![])` → kind=VALUE, val_len=0
- `Tombstone` → kind=TOMBSTONE (без val_len)

### Compatibility

- `VERSION_LEGACY = 1` читается: `val_len == 0` ⇒ tombstone (задокументированное ограничение старых сегментов).
- `VERSION = 2` пишется всеми новыми сегментами.
- Reader выбирает режим по header version.

### Tests

| Test | Результат |
|---|---|
| empty_value_basic | PASS |
| empty_value_after_flush | PASS |
| empty_value_after_reopen | PASS |
| empty_value_then_delete | PASS |
| empty_value_delete_recreate | PASS |
| empty_value_mvcc_chain | PASS |

## 4. Segment Validation (P1-3)

### Attack/corruption surface

- `slice[offset..end]` с corrupt offset → panic (устранено)
- `unwrap()` в read helper'ах на persisted data (устранено)
- Index/bloom/footer offsets за пределами файла (устранено)

### Validation rules

Каждый offset/length проверяется:
- `offset >= HEADER_SIZE`
- `offset + length <= data_region_end` (checked_add, без overflow)
- bloom bits в пределах bloom region
- index в пределах [bloom_end, data_region_end]
- каждый index block range валиден
- каждый entry: key_len/val_len bounds-checked
- block_size >= 4 (CRC)

### Error behavior

Все malformed-входы → `Result::Err(StorageError)`. Никаких panic/unwrap на persisted bytes.

### Tests (коррупционная матрица)

| Тест | Результат |
|---|---|
| corrupt_truncated_header | PASS (Err) |
| corrupt_invalid_magic | PASS (Err) |
| corrupt_unsupported_version | PASS (Err) |
| corrupt_truncated_footer | PASS (Err) |
| corrupt_footer_crc | PASS (Err) |
| corrupt_block_offset_beyond_eof | PASS (Err) |
| corrupt_index_length_overflow | PASS (Err) |
| corrupt_bloom_offset | PASS (Err) |
| corrupt_middle_entry_length | PASS (Err, no panic) |
| segment_arith_extremes | PASS (Err, no panic) |
| recovery_rejects_corrupt_segment | PASS (controlled error) |

## 5. Manifest Recovery (P2)

### Root cause

Manifest replay останавливался на corrupt/truncated записи, но corrupt bytes оставались в файле → последующий append мог создавать ambiguous state.

### Recovery protocol

```
replay():
  читать запись → validate → apply
  при corrupt/truncation: stop at first invalid record
  valid_len = offset последней валидной записи

open():
  replay()
  set_len(valid_len) + fsync  → файл содержит только валидные записи
```

### Tests

| Тест | Результат |
|---|---|
| manifest_truncated_suffix_recovery | PASS — partial record discarded, append после recovery видим |
| manifest_crc_corruption_suffix | PASS — corrupt record discarded, max_flushed_sequence корректен, двойной reopen стабилен |

## 6. Cross-component Recovery

```
WAL → MemTable → Segment → Manifest → Snapshot → Recovery
```

**`cross_component_recovery`** (PASS):
write → snapshot → overwrite → flush → compaction → checkpoint (WAL truncate) → close → reopen:
- k1=v2 (новейшая версия)
- empty value сохраняется
- snapshot semantics корректны
- новые writes работают после recovery
- второй reopen стабилен

## 7. Test Matrix

| Area | Tests | Runs | Result |
|---|---|---|---|
| Snapshot linearization | 3 | 5 | PASS |
| Empty values | 6 | 5 | PASS |
| Segment corruption | 11 | 5 | PASS |
| Manifest recovery | 2 | 5 | PASS |
| WAL batch atomicity (truncate_keep_above) | 1 | 5 | PASS |
| Checkpoint/compaction fault points (12.5A preserved) | 1 | 5 | PASS |
| Cross-component recovery | 1 | 5 | PASS |
| Phase 12.4 durability | 1 | 1 | PASS (lost=0, corrupt=0) |
| Phase 12.5A (существующие) | 9 | 5 | PASS |

## 8. Regression

```
cargo fmt --all -- --check:  PASS
cargo clippy --all-targets -- -D warnings: PASS
cargo test (crate, 171 tests):              PASS
Phase 12.4 durability:                      PASS (lost=0, corrupt=0, batch_lost=0)
Phase 12.5A (checkpoint+compaction tests):  PASS
ToroidalDB root integration (14 tests):     PASS (v2 segments)
Phase 11B frozen:                           PASS (30/30, 0 mismatches)
```

## 9. Deferred Issues

Остаются за пределами этой фазы (все известны, не являются storage-correctness P0/P1):
- HybridPersistentStore::add_edge
- concurrent insert / node_count
- query cache invalidation
- auth middleware / JWT secret / admin password
- backup path traversal / overwrite / ID collision
- CI coverage / README / runtime drift

## 10. Final Verdict

```
PHASE 12.5B: PASS
```

Release gate:
```
DURABILITY   (WAL/crash/checkpoint)   — PASS (12.4 + 12.5A)
MVCC         (snapshot/visibility)    — PASS (P1-1)
CORRUPTION   (segment validation)     — PASS (P1-3)
MANIFEST     (recovery)               — PASS (P2)
TOMBSTONE    (empty value)            — PASS (P1-2)

CRASH → RECOVERY → NO DATA LOSS
                 + NO FALSE VISIBILITY
                 + NO TOMBSTONE CONFUSION
                 + NO PARSER PANIC
                 + NO CORRUPT MANIFEST SUFFIX
                 → STORE CONSISTENT AND WRITABLE
```

Phase 13 (DatabaseContext) не начинается автоматически — требуется отдельный storage audit/review по Phase 12.5A + 12.5B.