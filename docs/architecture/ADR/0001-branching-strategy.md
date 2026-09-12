# ADR 0001: Branching Strategy for Stable and Experimental Work

- Status: Accepted
- Date: 2026-09-12
- Decision owners: ToroidalDB maintainers

## Context

ToroidalDB requires a stable integration line while architectural research and TAHI
experiments evolve at different rates and can temporarily introduce incompatible
assumptions. Mixing them directly into the stable line increases regression and
integration risk.

## Decision

Maintain three primary branches:

- `main`: stable integration and release line.
- `ARCHI`: architectural research, ADR/RFC development, and architecture-validation work.
- `TAHI`: isolated evaluation and incremental integration of TAHI concepts and components.

Use short-lived topic branches for individual hypotheses or implementations:

- `archi/<topic>`
- `experiment/tahi-<topic>`
- `fix/<topic>`

Promote validated, bounded changes to `main` through focused pull requests. Keep
`ARCHI` and `TAHI` periodically synchronized with `main` to control divergence.

## Consequences

### Positive

- Stable work remains isolated from exploratory changes.
- Architectural decisions receive durable, reviewable records.
- TAHI can be evaluated without replacing the ToroidalDB codebase.
- Independent experiments can be benchmarked and discarded safely.

### Negative

- Branch synchronization and conflict resolution require ongoing discipline.
- A decision and its implementation can temporarily live in different branches.
- Long-lived experiments can accumulate integration cost if not kept current.

## Validation

- Confirm that `main`, `ARCHI`, and `TAHI` have explicit branch charters.
- Require test and benchmark evidence before promotion to `main`.
- Review divergence from `main` at least once per active development cycle.

## Rollback

If the branch model becomes disproportionately costly, archive the experiment branch,
record the reason in an ADR, and retain only focused topic branches from `main`.
