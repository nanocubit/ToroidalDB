# Branch Charter: TAHI

## Purpose
`TAHI` is an isolated research and integration branch for evaluating TAHI concepts,
components, and workflows alongside ToroidalDB.

## Scope
Use this branch for:
- TAHI adapters, interoperability layers, and data-model mappings.
- Experiments involving archival, research-corpus, schema, or query workflows.
- Contract tests, integration benchmarks, and reproducible evaluation datasets.
- Incremental ports that preserve ToroidalDB's core boundaries.

## Non-goal
This branch must not replace the entire ToroidalDB tree with the external
`nanocubit/TAHI` repository without an explicit, separately reviewed migration decision.

## Required artifacts
Each experiment should record:
1. The source component or hypothesis.
2. The integration boundary and ownership of data/contracts.
3. Expected benefit and measurable success criteria.
4. Benchmark/test procedure and observed results.
5. Compatibility, cleanup, and rollback plan.

## Integration policy
Keep experiments narrowly scoped. Promote validated adapters or contracts to `main`
through focused pull requests rather than merging the branch wholesale.
