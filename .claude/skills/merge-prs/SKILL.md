---
name: merge-prs
description: Merge one or more PRs with full stack-integration pipeline. Resolves branches, rebases, gates, pulls main.
argument-hint: "[PR numbers...] [--admin] [--dry-run]"
user-invocable: true
allowed-tools: "Bash"
---

Stack-integration merge pipeline. Not a thin `gh pr merge` wrapper. Owns branch resolution,
merge ordering, post-merge rebases, local verification, and main sync.

For single quick merges when you already know the state is clean, this still works. For sprint
batch closes, use `/integrate-pr-stack` instead (same logic, more verbose output).

## Steps

1. **Resolve PRs to merge**:
   - If `$ARGUMENTS` specifies numbers, use those.
   - Otherwise list open PRs: `gh pr list --json number,title,headRefName,mergeable,mergeStateStatus`
   - If zero open PRs, stop.

2. **For each PR, resolve head branch from GitHub** (never trust plan names):
   ```bash
   gh pr view <N> --json number,title,headRefName,baseRefName,mergeable,mergeStateStatus \
     --jq '{pr:.number, head:.headRefName, base:.baseRefName, mergeable:.mergeable, state:.mergeStateStatus}'
   ```

3. **Classify check state** (required gates only):
   ```bash
   gh pr view <N> --json statusCheckRollup \
     --jq '.statusCheckRollup[] | select(.name | IN(
       "Build Release","Test","Clippy","Format Check",
       "Meaning Firewall Check","Kernel Forbidden Dependencies",
       "Firewall Contract Enforcement","TypeScript SDK",
       "Accessibility Tests","Regulatory Compliance Linter"
     )) | {name:.name, result:.conclusion}' 2>/dev/null
   ```

   - `UNSTABLE` mergeStateStatus + all required conclusions `SUCCESS` → treat as GREEN, safe to merge
   - Any required conclusion empty/pending → use `--auto` or wait
   - `claude-review`, `Compare Against Base`, `Test Coverage` failures → **never block**, ignore

4. **Merge** (in order provided; smallest/safest first for batch). This repo uses **squash merge**:
   ```bash
   gh pr merge <N> --squash             # green required checks (default)
   gh pr merge <N> --squash --admin     # if $ARGUMENTS includes --admin (queue-stalled)
   gh pr merge <N> --auto --squash      # pending required checks
   ```
   Exception: subtree merge commits require `--merge` — note the reason explicitly.

5. **After each merge**:
   ```bash
   git checkout main && git pull
   ```

6. **Rebase remaining PR branches** after each merge to keep them current:
   ```bash
   gh pr view <remaining-N> --json headRefName --jq '.headRefName'   # resolve branch
   git checkout <branch> && git rebase origin/main
   # Run scoped clippy before force-pushing
   cargo clippy -p <affected-packages> --all-targets -- -D warnings
   git push --force-with-lease
   ```

7. **Final**: confirm main state, print one-line summary per PR.

## Output format

```
#1389 merged · rebased [feat/s22-compute-policy-config, feat/s22-obs-attestation-config]
#1391 merged · rebased [feat/s22-obs-attestation-config]
#1390 merged · rebased []
```

## Rules

- **No polling loops.** Use `--auto` for pending checks; use `--admin` for queue-stalled (not failing).
- **No tabular parsing.** Always `--json` with `jq` or `python3`.
- **UNSTABLE ≠ blocked.** Required checks green + `mergeable=MERGEABLE` → merge.
- **Always resolve head branch from GitHub.** Never use plan-doc branch names directly.
- **After each merge, rebase remaining branches.** Verify locally before force-push.

## Required checks (all 10 — verified via branch protection API)

`Build Release`, `Test`, `Clippy`, `Format Check`, `Meaning Firewall Check`,
`Kernel Forbidden Dependencies`, `Firewall Contract Enforcement`, `TypeScript SDK`,
`Accessibility Tests`, `Regulatory Compliance Linter`

**Non-blocking** (never block on these): `claude-review`, `Compare Against Base`,
`Test Coverage`, `Benchmarks`, `Backup Validation`, `Security Audit`, `Web UI`,
`Save Benchmark Baseline`, `Pilot Provenance Invariant`, `Check API Types Drift`

Verify anytime: `gh api repos/InterCooperative-Network/icn/branches/main/protection --jq '.required_status_checks.contexts'`
