# ADR 0002: TAHI Integration Boundary

- Status: Accepted
- Date: 2026-09-12
- Decision owners: ToroidalDB maintainers

## Context

`nanocubit/TAHI` is an independent project with its own Python-oriented structure,
including archival, schema, research-corpus, benchmark, and test components.
ToroidalDB is an existing Rust-oriented system with its own storage, query, runtime,
and operational contracts.

Replacing the ToroidalDB tree with the TAHI repository would destroy the ability to
evaluate compatibility, isolate regressions, and incrementally integrate useful parts.

## Decision

Treat TAHI as an external source of concepts and components, not as a wholesale
replacement for ToroidalDB.

Use the `TAHI` branch to evaluate integration through explicit boundaries:

- adapters and interoperability layers;
- data-model and schema mappings;
- import/export or service-level contracts;
- contract tests and reproducible benchmarks;
- narrow, independently reversible ports.

No full-tree replacement of `ToroidalDB` by `nanocubit/TAHI` is allowed without a
separate ADR that specifies ownership transfer, migration, compatibility policy,
rollback, and operational validation.

## Consequences

### Positive

- ToroidalDB remains independently buildable and testable.
- TAHI capabilities can be measured against explicit contracts.
- Failed experiments do not erase the existing system.
- Successful components can be promoted incrementally.

### Negative

- Adapters and contract tests add temporary engineering work.
- Some cross-language or cross-runtime integration paths may introduce overhead.
- The two project structures remain intentionally distinct during evaluation.

## Acceptance criteria for an integration

An integration candidate must define:

1. Input/output contracts and ownership of persisted data.
2. Compatibility and migration behavior.
3. Correctness tests, including negative and rollback cases.
4. Performance baseline and target measurements.
5. Cleanup or deprecation plan if the experiment is rejected.

## Rollback

Disable or remove the adapter/feature behind the integration boundary. Preserve
benchmark evidence and record the outcome in an ADR or RFC.
