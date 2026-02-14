---
name: preflight
description: Session preflight check - prints repo state, branch, PR info, and compile sanity. Run at session start.
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
---

Run a quick preflight check to establish session context. Do not change anything.

## Steps

1. Print:
   - repo root (`pwd`)
   - `git remote -v`
   - current branch (`git branch --show-current`)
   - `git status --short`
2. If a PR is involved:
   - `gh pr view --json number,baseRefName,headRefName,state,title`
3. If running services/tests:
   - list running listeners/ports (`ss -tlnp | head -20`)
4. Run quick compile sanity:
   - `cd icn && cargo check --workspace`

## Output

Report results in 5 bullets. Do not change anything yet.
