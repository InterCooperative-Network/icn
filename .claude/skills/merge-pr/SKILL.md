---
name: merge-pr
description: Merge a PR after confirming CI is green. Fast, correct, no ceremony.
argument-hint: "[PR number] [--admin]"
user-invocable: true
allowed-tools: "Bash"
---

Merge a PR. Fast, correct, no ceremony.

## Steps

1. Confirm current PR + branch:
   - `gh pr view --json number,title,headRefName,baseRefName,state`
   - If `$ARGUMENTS` specifies a PR number, use that
2. Check CI status:
   - `gh pr checks`
   - If checks are still running, report and ask whether to wait
3. If all checks are green, merge:
   - `gh pr merge --merge`
4. If branch protection blocks merge:
   - If `$ARGUMENTS` includes `--admin`: proceed with `gh pr merge --merge --admin`
   - Otherwise: ask the user once. If they say yes, use `--admin` for this merge.
5. After merge:
   - `git checkout main && git pull`

## Output

Report: merged PR #, commit SHA, any follow-ups needed.
