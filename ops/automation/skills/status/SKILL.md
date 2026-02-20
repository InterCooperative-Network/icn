---
name: status
description: Show full ICN development status dashboard — active sessions, sprint tasks, worktree freshness, CI state, and cluster health
---

You are the ICN development status dashboard. Show the current state of the entire development environment in a single scannable view.

## Data sources (use in order)

**1. Active sessions** — call MCP tool `list_sessions`
If empty, skip this section. Otherwise show each session as:
`Agent→repo:worktree — task_description`

**2. Sprint state** — read the sprint file:
```bash
cat /home/ubuntu/projects/icn-ops/state/sprint/current.json
```
Show: sprint number + name, goals, task counts by status (pending/in-progress/in-review/done).

**3. Repository status** — call MCP tool `repo_status`
Show branch, dirty/clean, ahead/behind for icn, icn-website, icn-ops.

**4. Worktree status** — call MCP tool `worktree_status`
Sort by staleness (most behind first). Flag stale (>10 commits behind) with ⚠️.

**5. CI status** — call MCP tool `ci_status` with repo="icn"
Show last 3 runs with status symbols.

**6. Cluster health** — call MCP tool `cluster_health`
One-line summary: N pods healthy, services reachable/unreachable.

**7. Build cache** — call MCP tool `build_cache_status`
One-line summary: cache hit rate if available.

## Output format

Use this structure with ✅ ⏳ ❌ ⚠️ symbols:

```
**Active Sessions** (N)
  Agent1 → icn:1084-names-gateway-a — implementing naming service
  Agent2 → icn-website — updating docs

**Sprint 7 — Naming & Pilot Hardening**
  Goals: [1] Land names API, [2] Pilot vertical slice, [3] icn-ops
  Tasks: 2 done · 1 in-review · 3 pending

**Repos**
  icn          feat/sprint7-naming-1083  ✅ clean  (0↑ 0↓)
  icn-website  main                      ✅ clean
  icn-ops      main                      ✅ clean

**Worktrees** (4)
  1084-names-gateway-a   feat/1084  ✅  2 behind main
  main                   main       ✅  current
  1120-auth-semantics-b  feat/1120  ⚠️  15 behind main
  1051-timestamp-resp-c  feat/1051  ⚠️  18 behind main

**CI** (icn · feat/sprint7-naming-1083)
  ✅ ci · 3 min ago
  ✅ ci · 2 hrs ago

**Cluster** 10.8.10.40
  ✅ 3/3 pods healthy · gateway ✅ · pilot-ui ✅ · metrics ✅

**Build Cache** sccache hit rate: 74%
```

Total output should be scannable in 30 seconds. If any MCP tool fails (server not running or tool error), show the section header and `⚠️ unavailable (MCP error)` — do not stop, continue with remaining sections.
