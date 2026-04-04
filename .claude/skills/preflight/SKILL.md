---
name: preflight
description: Session preflight check - prints repo state, branch, PR info, and compile sanity. Run at session start.
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
truth_contract:
  canonical_sources:
    - ops/state/config/repo-map.json    # workspace root, cluster IPs
    - ops/state/truth/policy.json       # required checks
    - ops/state/sprint/current.json     # sprint state
  live_load_required:
    - "bash ops/scripts/what-matters-now.sh 2>/dev/null || true"   # canonical entrypoint
    - "git branch --show-current"
    - "gh auth status"
    - "rustc --version"
  examples_only: []
  never_hardcode:
    - sprint number
    - toolchain version (read from rust-toolchain.toml)
    - cluster IPs
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
