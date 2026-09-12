# ToroidalDB Architecture

This directory is the canonical home for architecture decisions, design proposals,
diagrams, interface contracts, and reproducible architecture benchmarks.

## Structure

- `ADR/` — accepted, proposed, superseded, and rejected Architecture Decision Records.
- `RFC/` — design proposals that require discussion or validation before a decision.
- `diagrams/` — architecture diagrams and their editable source files.
- `benchmarks/` — reproducible performance, correctness, and compatibility evidence.

## Decision lifecycle

1. Create an RFC when a design problem needs exploration.
2. Record the resulting decision in an ADR.
3. Implement the decision in focused, independently testable changes.
4. Attach validation evidence, compatibility analysis, and rollback notes.
5. Mark replaced decisions as superseded; do not silently rewrite history.

## Naming

- ADRs: `NNNN-short-kebab-case-title.md`
- RFCs: `NNNN-short-kebab-case-title.md`
- Diagrams: use an editable source plus an exported rendering when applicable.
