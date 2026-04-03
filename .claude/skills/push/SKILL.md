---
name: push
description: Resolve targets, run scoped local gates, then push. The only sanctioned path for pushing Rust changes.
argument-hint: "[--skip-test] [--force-with-lease]"
user-invocable: true
allowed-tools: "Bash"
truth_contract:
  canonical_sources:
    - ops/state/config/repo-map.json    # workspace root (cargo commands run from icn/)
    - ops/state/truth/policy.json       # validation_ladder, required gate sequence
  live_load_required:
    - "git branch --show-current"
    - "git diff --name-only $(git merge-base HEAD origin/main)..HEAD"
    - "cargo metadata --no-deps --format-version 1"   # package name resolution
  examples_only: []
  never_hardcode:
    - package names (always resolve via cargo metadata)
    - branch name
---

Gated push with scoped verification. Resolves affected packages first, runs the minimum sufficient
gate set, classifies any lint failures by known remediation class, then pushes.

## Step 0 — Preflight

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
bash "${REPO_ROOT}/ops/scripts/drift-check.sh" 2>/dev/null | tail -3 || true
```

If drift-check reports FAIL → warn loudly. Pushing while agent tooling has drift means
CI may evaluate your PR with stale policy. Fix drift or acknowledge explicitly before continuing.

## Steps

1. **Confirm branch and status**:
   ```bash
   git branch --show-current
   git status --short
   ```

2. **Resolve touched packages** to determine verification scope:
   ```bash
   # Files changed on this branch
   git diff --name-only $(git merge-base HEAD origin/main)..HEAD

   # Map to workspace package names (never guess)
   cargo metadata --no-deps --format-version 1 \
     | python3 -c "
   import sys, json
   md = json.load(sys.stdin)
   for p in md['packages']:
       root = p['manifest_path'].replace('/Cargo.toml','')
       print(f\"{p['name']:40s}  {root}\")
   " | sort
   ```

   Scope decision:
   - Changes in 1–3 packages → scoped commands (faster)
   - Changes include `icnd/src/main.rs`, `lifecycle.rs`, or cross multiple crate boundaries → workspace-wide

3. **Run format check** (always workspace-wide, fast):
   ```bash
   cargo fmt --all --check
   ```

4. **Run clippy** (scoped if possible):
   ```bash
   # Scoped (preferred — must use --all-targets to match CI):
   cargo clippy -p <pkg1> -p <pkg2> --all-targets -- -D warnings

   # Workspace (when cross-cutting):
   cargo clippy --workspace --all-targets -- -D warnings
   ```

   On failure, classify before fixing (see `fix-rust-lints` for canonical patterns):
   - `field_reassign_with_default` → struct update syntax
   - `use of deprecated` in tests → replace with recommended API
   - Overflow/underflow in time math → use `checked_sub` / `checked_mul`

5. **Run tests** (unless `$ARGUMENTS` includes `--skip-test`):
   ```bash
   cargo test -p <pkg1> -p <pkg2>   # scoped
   cargo test --workspace            # workspace
   ```

6. **If any gate fails**: report lint class and canonical fix, do NOT push.

7. **If all gates pass**:
   ```bash
   git push                         # normal push
   git push -u origin <branch>      # first push (no upstream)
   git push --force-with-lease      # after rebase (if $ARGUMENTS includes --force-with-lease)
   ```

8. **Report**: pushed branch, commit SHA, scope used, gate results.

## Important

- Never push if fmt or clippy fails.
- `--skip-test` skips the test suite. fmt + clippy always run.
- A local `cargo clippy -p <crate>` pass does NOT guarantee `--workspace --all-targets` passes.
  Use `--all-targets` to match CI behavior.
- After a rebase, always re-run at minimum scoped clippy before force-pushing.
- Never guess package IDs. Use `cargo metadata` to confirm. See known confusions below.

## Known ICN package name confusions

| Human label | Correct `-p` argument |
|-------------|----------------------|
| "ledger" (crate) | `icn-ledger` |
| "ledger app" / "icn-ledger-app" | `icn-ledger-actor` (apps/ledger) |
| "obs" | `icn-obs` |
| "security" | `icn-security` |
| "compute" | `icn-compute` |
| "core" | `icn-core` |
| "daemon" / "icnd" | `icnd` (bin — triggers workspace-wide) |
| "governance app" | `icn-governance-actor` |
| "membership app" | `icn-membership-app` |
