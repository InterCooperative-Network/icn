# Reality Sources

Use this map to decide what is authoritative for each doc claim.

## Workspace and topology

- Rust workspace root and members: `icn/Cargo.toml`
- Runtime app location rule: `AGENTS.md` and `docs/adr/ADR-0010-app-topology.md`
- Actual app wiring in daemon: `icn/bins/icnd/Cargo.toml`

## Build/test commands

- Required command routing by path: `AGENTS.md`
- Convenience wrappers: `justfile`
- Package-local scripts: `sdk/*/package.json`, `web/*/package.json`

## CI behavior

- Active checks and blocking behavior: `.github/workflows/*.yml`
- OpenAPI/type drift automation: `.github/workflows/api-types.yml`

## API and SDK sync

- Exported OpenAPI target: `docs/api/openapi.generated.yaml`
- TS generated types location: `sdk/typescript/src/generated/`
- TS generation command: `sdk/typescript/package.json` (`generate-types`)

## Status and recency

- Canonical current state file: `docs/STATE.md`
- Point-in-time snapshots: `docs/status/*.md`, `docs/demo/*.md`, session reports
- For volatile docs, require explicit date in heading/body and avoid present-tense claims without evidence.
