---
name: fix-ci
description: Fix CI failures. Scope-locked to branch changes only. No toolchain upgrades. No unrelated clippy.
argument-hint: "[PR number or branch]"
user-invocable: true
allowed-tools: "Bash, Read, Edit, Grep, Glob"
truth_contract:
  canonical_sources:
    - ops/state/truth/policy.json       # required_checks, validation_ladder
    - ops/state/config/repo-map.json    # workspace root (cargo commands run from icn/)
  live_load_required:
    - "git diff --name-only $(git merge-base HEAD origin/main)..HEAD"
    - "gh pr checks <N> --json name,state,conclusion"
  examples_only: []
  never_hardcode:
    - toolchain version (read from rust-toolchain.toml)
    - branch name (read from git branch --show-current)
---

Fix CI failures for the current branch. Scope-locked. Output: cause, fix, proof.

## Step 0 — Preflight

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
bash "${REPO_ROOT}/ops/scripts/drift-check.sh" 2>/dev/null | tail -3 || true
```

If drift-check reports FAIL → note it. Agent tooling drift may itself be causing CI failures
(e.g., stale required-check set in a skill). Fix drift before fixing CI if related.

## Scope Rules (non-negotiable)

- **Only touch files changed on this branch** vs its base. Get the list: `git diff --name-only $(git merge-base HEAD origin/main)..HEAD`
- **No toolchain upgrades.** Do not touch `rust-toolchain.toml`.
- **No unrelated clippy fixes.** Only fix lints in changed files.
- **No new infrastructure files.** No justfiles, no CI config changes, no new scripts.
- **SIGSEGV = `cargo clean` + retry once.** If it fails again, report and stop.

## Steps

1. **Get context**:
   - `git branch --show-current`
   - If `$ARGUMENTS` has a PR number: `gh pr checks <N> --json name,state,conclusion` and `gh run view <RUN_ID> --log-failed`
   - Otherwise: ask what failed.

2. **Identify cause**: Read the failed log. Categorize:
   - `fmt` → run `cargo fmt` on changed files
   - `clippy` → fix only lints in changed files
   - `test` → read failing test, fix in changed code
   - `SIGSEGV` → `cargo clean && cargo check`
   - `compile error` → fix in changed files

3. **Fix**: Apply minimal fix. Stay in scope.

4. **Prove**: Run the gate that failed:
   - `cargo fmt --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test` (or the specific failing test)

## Output

Three sections, each 1-3 lines:
```
Cause: <what failed and why>
Fix: <what was changed>
Proof: <gate command output — pass/fail>
```
