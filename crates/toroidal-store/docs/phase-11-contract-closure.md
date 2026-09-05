# Priority 1.1 — Contract Closure Report

## Статус

```
Priority 1.1      CLOSED
Phase 11          UNBLOCKED
```

## 1. TQL E2E failure classification

Два failing теста — **pre-existing, не связанные со storage**:

| Тест | Failure | Слой | Storage-related | Воспроизводится с MemoryFactory |
|---|---|---|---|---|
| `parse_and_execute_match_with_limit` | `"No TOROIDALDISTANCE condition in query"` | TQL Executor | Нет | ✅ |
| `graph_traversal_returns_connected_nodes` | `nom::Err::Error` на парсинге `)-[:SIMILAR]->` | TQL Parser | Нет | ✅ |

Оба теста падают с **MemoryFactory** (без toroidal-store), что доказывает отсутствие связи с storage stack. Вынесены в отдельный pre-existing issue.

## 2. Sequence monotonicity после reopen

Формула восстановления: `next_sequence = max(manifest_max_flushed_sequence, wal_max_sequence) + 1`.

Реализация: `Wal::open_with_sequence(&wal_path, manifest.max_flushed_sequence + 1)`.

Подтверждено тестами:
- `sequence_monotonicity_across_checkpoint_cycles` — 3 цикла write→checkpoint→reopen, sequence строго возрастает
- `sequence_after_reopen_never_reuses` — reopen после checkpoint, sequence не откатывается к 0
- `snapshot_sees_checkpointed_data_after_reopen` — snapshot после reopen видит все checkpointed данные
- `snapshot_sees_mixed_after_reopen` — checkpointed + unflushed данные после reopen

## 3. Batch sequence property

- `batch_no_sequence_gaps` — все sequences в batch уникальны, без gaps, reopen не уменьшает sequence
- `batch_visible_after_reopen` — 100 batch-put'ов полностью видимы после reopen

## 4. Тестовый итог

| Уровень | Тестов | Статус |
|---|---|---|
| Engine (toroidal-store) | 137 | ✅ |
| Adapter (toroidal-storage-v0.2) | 7 | ✅ |
| ToroidalDB integration | 13 | ✅ |
| Contract closure | 6 | ✅ |
| TQL E2E (storage-relevant) | 2 | ✅ |
| TQL E2E (pre-existing) | 2 | ⚠️ classified |
| **Всего storage gate** | **165** | **✅** |

## 5. Phase 11 — UNBLOCKED

Рекомендуемый порядок:
1. **Phase 11A** — common workload schema + ToroidalStore/ToroidalDB adapter
2. **Phase 11B** — RocksDB/redb/fjall adapters
3. **Phase 11C** — raw + equivalent durability benchmarks + report