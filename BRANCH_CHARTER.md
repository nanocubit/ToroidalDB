# Branch Charter: ARCHI

## Purpose
`ARCHI` is ToroidalDB's architectural research and design-integration branch.

## Scope
Use this branch for:
- Architecture Decision Records (ADRs) and Requests for Comments (RFCs).
- Module boundaries, storage/index/query contracts, schemas, and migration strategies.
- Proofs of concept that validate a concrete architectural hypothesis.
- Reproducible design benchmarks and compatibility investigations.

## Required artifacts
Every non-trivial proposal should state:
1. Problem and constraints.
2. Alternatives considered and rejected.
3. Affected interfaces, invariants, persistence, and compatibility boundaries.
4. Validation plan, measurable acceptance criteria, and rollback/migration path.
5. Decision status: proposed, accepted, superseded, or rejected.

## Integration policy
Do not merge a whole architecture experiment into `main` as one opaque change.
Promote independently testable slices through focused pull requests.

## Suggested layout
- `docs/architecture/ADR/`
- `docs/architecture/RFC/`
- `docs/architecture/diagrams/`
- `benchmarks/architecture/`
