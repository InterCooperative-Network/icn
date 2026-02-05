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

### Change Routing Matrix

| Changed files | Required checks |
|---|---|
| `icn/crates/**/*.rs` | `cd icn && cargo fmt --all --check && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test -p <touched-crates>` |
| `icn/crates/icn-gateway/**` | Also: `cargo test -p icn-gateway --features sled-storage` |
| `icn/crates/icn-kernel-api/**` | Also: `cargo test -p icn-kernel-api` |
| `sdk/typescript/**` | `cd sdk/typescript && npm ci && npm run build && npm test && npm run lint` |
| `web/pilot-ui/**` | `cd web/pilot-ui && npm run test && npm run test:e2e` |
| `deploy/**` | Check for committed secrets: `grep -rn 'password\|secret\|token' deploy/ --include='*.yml' --include='*.yaml'` |
| `docs/**` | Verify links exist, terminology consistency |

3. If `$ARGUMENTS` specifies a crate name, only verify that crate
4. If `$ARGUMENTS` is `--all`, run the full suite
5. Report results with pass/fail for each category

## Important

- Always run checks from the correct directory (`icn/` for Rust, `sdk/typescript/` for TS)
- If gateway API behavior changed, remind about OpenAPI + TS type regeneration
- Never weaken checks to make them pass
