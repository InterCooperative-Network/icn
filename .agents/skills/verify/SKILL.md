---
name: verify
description: Run the correct verification commands for files changed in the current branch. Maps to ICN change routing matrix.
argument-hint: "[crate-name or --all]"
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # validation_ladder
    - ops/state/config/repo-map.json    # workspace root (rust commands run from icn/icn/)
  live_load_required:
    - "git diff --name-only origin/main...HEAD"
  examples_only: []
---

Run verification checks appropriate to the files changed in the current branch.

## Step 0 — Preflight

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
bash "${REPO_ROOT}/ops/scripts/drift-check.sh" 2>/dev/null | tail -3 || true
```

If drift-check reports FAIL → note it before running verification checks.

## Steps

1. Determine what changed by running `git diff --name-only origin/main...HEAD` (or `git diff --name-only HEAD` for uncommitted changes)
2. Classify the changed files into categories and run the matching checks:

### Repo topology reminder

Resolve both roots at runtime — never hardcode a machine path:

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"   # monorepo root: SDK, docs, deploy
CARGO_ROOT="${REPO_ROOT}/icn"                  # Cargo workspace root: all cargo commands
```

### Change Routing Matrix

| Changed files | Required checks |
|---|---|
| `icn/crates/**/*.rs` or `icn/apps/**/*.rs` | `cd icn && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p <touched-crates>` |
| `icn/crates/icn-gateway/**` | Also run: `cargo test -p icn-gateway --features sled-storage` (matches the CI gateway job) |
| `icn/crates/icn-kernel-api/**` | Also run: `cargo test -p icn-kernel-api` |
| `sdk/typescript/**` | `cd sdk/typescript && npm ci && npm run build && npm test && npm run lint` |
| `web/pilot-ui/**` | `cd web/pilot-ui && npm run test && npm run test:e2e` |
| `deploy/**` | Check for committed secrets: `grep -rn 'password\|secret\|token' deploy/ --include='*.yml' --include='*.yaml'` |
| `docs/**` | Verify links exist, terminology consistency |

3. If `$ARGUMENTS` specifies a crate name, only verify that crate
4. If `$ARGUMENTS` is `--all`, run the full suite
5. Report results with pass/fail for each category

## Important

- **Rust commands run from `${CARGO_ROOT}`** (the Cargo workspace root, contains `Cargo.toml`), not from the repo root.
- The command ladder above mirrors `ops/state/truth/policy.json#validation_ladder`. If the two disagree, `policy.json` wins — reread it rather than editing this list from memory.
- `sdk/typescript/` and `web/pilot-ui/` commands run from the monorepo root's subdirectories.
- If gateway API behavior changed, remind about OpenAPI + TS type regeneration (`cd sdk/typescript && npm run generate-types`).
- Never weaken checks to make them pass.
