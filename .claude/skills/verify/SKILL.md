---
name: verify
description: Run the correct verification commands for files changed in the current branch. Maps to ICN change routing matrix.
argument-hint: "[crate-name or --all]"
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
---

Run verification checks appropriate to the files changed in the current branch.

## Steps

1. Determine what changed by running `git diff --name-only origin/main...HEAD` (or `git diff --name-only HEAD` for uncommitted changes)
2. Classify the changed files into categories and run the matching checks:

### Repo topology reminder

| Root | Path | Cargo commands |
|------|------|----------------|
| Monorepo root | `/home/ubuntu/projects/icn` | SDK, docs, deploy |
| **Rust workspace** | `/home/ubuntu/projects/icn/icn` | **All `cargo` commands run here** |

### Change Routing Matrix

| Changed files | Required checks |
|---|---|
| `icn/crates/**/*.rs` or `icn/apps/**/*.rs` | `cd icn/icn && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p <touched-crates>` |
| `icn/crates/icn-gateway/**` | Also run: `cargo test -p icn-gateway` |
| `icn/crates/icn-kernel-api/**` | Also run: `cargo test -p icn-kernel-api` |
| `sdk/typescript/**` | `cd sdk/typescript && npm ci && npm run build && npm test && npm run lint` |
| `web/pilot-ui/**` | `cd web/pilot-ui && npm run test && npm run test:e2e` |
| `deploy/**` | Check for committed secrets: `grep -rn 'password\|secret\|token' deploy/ --include='*.yml' --include='*.yaml'` |
| `docs/**` | Verify links exist, terminology consistency |

3. If `$ARGUMENTS` specifies a crate name, only verify that crate
4. If `$ARGUMENTS` is `--all`, run the full suite
5. Report results with pass/fail for each category

## Important

- **Rust commands run from `icn/icn/`** (the Cargo workspace root), not from `icn/`.
- `sdk/typescript/` and `web/pilot-ui/` commands run from the monorepo root's subdirectories.
- If gateway API behavior changed, remind about OpenAPI + TS type regeneration (`cd sdk/typescript && npm run generate-types`).
- Never weaken checks to make them pass.
