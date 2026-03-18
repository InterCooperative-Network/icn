---
name: merge-prs
description: Merge one or more PRs. No polling loops. Uses gh --json. Prefer --auto, use --admin when told.
argument-hint: "[PR numbers...] [--admin]"
user-invocable: true
allowed-tools: "Bash"
---

Merge PRs. No polling. No tabular parsing. One line per PR.

## Steps

1. List target PRs:
   - If `$ARGUMENTS` specifies PR numbers, use those.
   - Otherwise: `gh pr list --json number,title,headRefName,statusCheckRollup --limit 20`
   - If zero open PRs, print "0 open PRs" and stop.

2. For each PR, in order:
   a. Get status: `gh pr view <N> --json number,title,mergeable,statusCheckRollup`
   b. If checks are green → `gh pr merge <N> --merge`
   c. If checks are pending/running → `gh pr merge <N> --auto --merge` (let GitHub merge when green)
   d. If `$ARGUMENTS` includes `--admin` → use `gh pr merge <N> --merge --admin`
   e. If merge fails, print one-line error and continue to next PR.

3. After all PRs: `git checkout main && git pull`

## Rules

- **No polling loops.** Never `while true; sleep; done`. Use `--auto` for pending checks.
- **No tabular parsing.** Always `--json` flag, parse with `jq` or python3.
- **One line per PR**: `#123 merged` or `#123 failed: <reason>` or `#123 --auto set`.
- Max 20 lines total output.
