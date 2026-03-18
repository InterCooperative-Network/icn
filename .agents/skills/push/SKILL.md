---
name: push
description: Run fmt + clippy gates, then push. The only sanctioned path for pushing Rust changes.
argument-hint: "[--skip-test] [--force-with-lease]"
user-invocable: true
allowed-tools: "Bash"
---

Gated push: runs fmt and clippy before allowing push. This is the sanctioned path for pushing Rust workspace changes.

## Steps

1. Confirm current branch and remote tracking:
   - `git branch --show-current`
   - `git status --short`
2. Run gates (all must pass):
   - `cd icn && cargo fmt --all --check`
   - `cd icn && cargo clippy --workspace --all-targets -- -D warnings`
3. If `$ARGUMENTS` does NOT include `--skip-test`:
   - `cd icn && cargo test --workspace`
4. If any gate fails:
   - Report which gate failed and the output
   - Do NOT push
   - Suggest fix commands
5. If all gates pass:
   - `git push` (or `git push --force-with-lease` if `$ARGUMENTS` includes `--force-with-lease`)
6. Report: pushed branch, commit SHA, gate results.

## Important

- Never push if fmt or clippy fails.
- `--skip-test` skips the test suite (for speed when tests were already run). fmt + clippy always run.
- If the branch has no upstream, use `git push -u origin <branch>`.
