# CI Current Status

**Last Reviewed**: 2026-02-11  
**Status Type**: Operational snapshot (not a persistent truth source)

## Purpose

This file records a point-in-time CI posture and how to re-verify it.
It must not be used as proof that CI is currently green without rerunning checks.

## Authoritative Sources

- CI definitions: `.github/workflows/*.yml`
- Required local command routing: `AGENTS.md`
- Rust workspace root: `icn/`

## Current Notes (as of 2026-02-11)

- Treat older "all green" statements as historical snapshots only.
- Deployment-related checks may depend on external infrastructure availability.
- Architecture/docs status must be validated against the current branch and workflows.

## Re-verify CI Locally

Run from repo root unless noted:

```bash
# Rust checks (run from icn/)
cd icn
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
cargo test --workspace --test '*' -- --test-threads=1
```

Subsystem checks by change scope (see `AGENTS.md`):

- Gateway: `cargo test -p icn-gateway --features sled-storage`
- TypeScript SDK: `cd sdk/typescript && npm ci && npm run build && npm test && npm run lint`
- React Native SDK: `cd sdk/react-native && npm test && npm run build`
- Pilot UI: `cd web/pilot-ui && npm ci && npm run test && npm run test:e2e && npm run test:a11y`

## Reporting Format

When updating this file, include:

1. Exact date/time reviewed
2. Branch/commit checked
3. Commands run
4. Pass/fail outcomes
5. Known external blockers (for example unavailable runners)

## History

For older narrative CI reports, see session/status documents under:

- `docs/status/`
- `docs/development/sessions/`
