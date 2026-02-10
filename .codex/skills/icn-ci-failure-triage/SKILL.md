---
name: icn-ci-failure-triage
description: "Triage and fix failing ICN CI checks with minimal diff and deterministic verification. Use when GitHub Actions jobs fail on PRs or branches, especially for clippy/test/release/coverage, and when quick root-cause extraction from logs is required."
---

# ICN CI Failure Triage

Drive to first real root cause, then patch narrowly.

## Workflow

1. Identify exact failing run and jobs.
```bash
gh pr checks <PR_NUMBER>
gh run list --branch <BRANCH> --limit 10 --json databaseId,status,conclusion,workflowName,headSha
```

2. Pull only failed logs.
```bash
gh run view <RUN_ID> --log-failed
# If needed:
gh run view <RUN_ID> --job <JOB_ID> --log
```

3. Extract first actionable failure.
- Ignore downstream noise.
- Capture first failing test, lint, or build error.
- Confirm touched files and scope.

4. Reproduce locally with CI-equivalent commands.
```bash
cd icn
RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTC_WRAPPER= cargo test --workspace --lib
RUSTC_WRAPPER= cargo build --workspace --release
```
- For targeted debug, run the narrowest failing crate/test first.

5. Patch minimally.
- Do not weaken guards, validation, signatures, or encoding constraints.
- Preserve kernel/app boundaries.
- Keep diff scoped to root cause.

6. Re-verify narrow then broad.
- Re-run failing command.
- Re-run required checks for touched area from `AGENTS.md`.
- Push and re-check CI.

## Failure-Class Playbook

- `clippy`: fix warning cause directly; avoid broad allows.
- `test`: rerun exact test name/crate first; fix determinism/race/state leakage.
- `build release`: verify feature gates and release-only code paths.
- `coverage`: inspect tarpaulin-specific failures separately from unit tests.
- web/sdk jobs: run package-local test/build/lint commands.

## Output Contract

Return:
1. failing job + first root-cause line,
2. patch summary,
3. verification commands and outcomes,
4. remaining CI state.
