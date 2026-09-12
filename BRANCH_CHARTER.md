# Branch Charter: main

## Purpose
`main` is the stable integration and release line of ToroidalDB.

## Allowed changes
- Production-ready features and bug fixes.
- Documentation that describes verified behavior.
- Dependency upgrades with reproducible validation.
- Changes merged through reviewed pull requests.

## Entry criteria
A change entering `main` must:
1. Build successfully.
2. Pass formatting, linting, unit, integration, and relevant regression tests.
3. Preserve or deliberately version public APIs and persisted-data contracts.
4. Include documentation and migration notes when behavior changes.
5. Have a clear rollback path for operationally significant changes.

## Integration policy
Experimental work belongs in dedicated branches. Integrate only a bounded, reviewed,
and measurable slice through a pull request.

## Branch hygiene
Do not commit credentials, generated build artifacts, local configuration, or unverified
benchmark claims.
