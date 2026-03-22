---
name: resolve-pr-branch
description: Resolve PR↔branch identity from GitHub. Never trust plan docs or memory for branch names.
argument-hint: "[PR number | branch name]"
user-invocable: true
allowed-tools: "Bash"
---

Resolve PR and branch identity from GitHub as the authoritative source. Use this before any
checkout, rebase, force-push, or when a plan references a branch by name.

## The Problem This Solves

Branch names in plans, notes, and memory drift. The transcript showed a `git checkout feat/s22-compute-config`
failing because the true head ref was `feat/s22-compute-policy-config`. That lookup took an extra round-trip
to recover. This skill makes the lookup the first step, not a recovery.

## Steps

1. **Determine lookup direction from `$ARGUMENTS`**:
   - If argument looks like a number → PR lookup
   - If argument looks like a branch name → branch lookup
   - If empty → show all open PRs with their head branches

2. **PR → head branch**:
   ```bash
   gh pr view <N> --json number,title,headRefName,baseRefName,state \
     --jq '{pr: .number, title: .title, head: .headRefName, base: .baseRefName, state: .state}'
   ```

3. **Branch → PR**:
   ```bash
   gh pr list --json number,title,headRefName,state \
     --jq '.[] | select(.headRefName == "<branch>") | {pr: .number, title: .title, state: .state}'
   ```

4. **Check local branch existence**:
   ```bash
   git branch --list <branch>          # local
   git ls-remote --heads origin <branch> | grep -q . && echo "exists remotely"
   ```

5. **Fetch if missing locally but present remotely**:
   ```bash
   git fetch origin <branch>
   ```

6. **Detect stacked PRs** (base ≠ main):
   ```bash
   gh pr list --json number,headRefName,baseRefName \
     --jq '.[] | select(.baseRefName != "main") | "PR #\(.number) \(.headRefName) → \(.baseRefName)"'
   ```

## Output format

```
PR #1391 · feat/s22-compute-policy-config → main  [OPEN]
  local branch: present
  remote:       present
  stacked:      no
```

## Guardrails

- Never proceed with a branch name that hasn't been confirmed against GitHub.
- When a local checkout fails with "pathspec did not match", immediately run this skill instead of
  trying other branch name guesses.
- Treat stacked PRs (base ≠ main) as requiring ordered integration.

## ICN-specific notes

- Sprint plan docs (`ops/state/sprint/current.json`) store branch names that can drift between
  plan-time and execution-time. Always verify against `gh pr view`.
- Worktrees in `icn-wt/` hold their own branch refs; a branch that appears absent may just be
  checked out in a worktree.
