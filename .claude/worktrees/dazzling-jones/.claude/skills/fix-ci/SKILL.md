---
name: fix-ci
description: Fix CI failures. Scope-locked to branch changes only. No toolchain upgrades. No unrelated clippy.
argument-hint: "[PR number or branch]"
user-invocable: true
allowed-tools: "Bash, Read, Edit, Grep, Glob"
---

Fix CI failures for the current branch. Scope-locked. Output: cause, fix, proof.

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
