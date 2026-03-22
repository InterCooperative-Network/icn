---
name: watch-ci-and-advance
description: Monitor CI as a queue system. Advance other ready work while waiting. Report gate deltas, not polls.
argument-hint: "[PR number | run ID]"
user-invocable: true
allowed-tools: "Bash"
---

Treat CI as a queue system. While the runner is busy, do useful work. Report only meaningful
state changes, not repetitive status dumps.

## The Problem This Solves

The transcript spent significant time in a passive poll cycle: check → still pending → wait → check.
Meanwhile, other PRs could have been rebased, verified, and readied. This skill turns waiting time
into advance work.

## Steps

### Phase 1: Assess current queue state

```bash
# Check if runner is actively processing or just queued
gh run list --limit 5 --json databaseId,status,name,conclusion,createdAt \
  --jq '.[] | "\(.databaseId) \(.name) \(.status) \(.conclusion // "running")"'
```

Classify:
- `in_progress`: runner is actively working — productive wait
- `queued` + age > 5 min: runner contention likely
- `queued` + age > 30 min: definitely queue-stalled, likely safe to `--admin` merge after manual local verify

```bash
# Check runner health
gh api repos/InterCooperative-Network/icn/actions/runners \
  --jq '.runners[] | {name:.name, status:.status, busy:.busy}'
```

### Phase 2: Identify useful advance work

While CI runs, prioritize in this order:

1. **Rebase other PR branches onto latest main** (no runner needed)
2. **Run scoped local verification on rebased branches**
3. **Check main for workflow failures** (may reveal pre-existing breakage)
4. **Inspect non-running PRs for review comments that can be addressed**

```bash
# Find PRs not currently in CI queue
gh pr list --json number,headRefName,statusCheckRollup \
  --jq '.[] | select(.statusCheckRollup | length == 0 or all(.[]; .state != "IN_PROGRESS")) | .number'
```

### Phase 3: Report gate deltas, not full dumps

Instead of printing the full check table every poll, track which gates changed:

```bash
# Snapshot required gates only
gh pr checks <N> --json name,state,conclusion \
  | python3 -c "
import sys, json
required = {'Build Release','Test','Clippy','Format Check'}
checks = {c['name']:c for c in json.load(sys.stdin) if c['name'] in required}
for name, c in sorted(checks.items()):
    status = c['conclusion'] if c['state'] == 'COMPLETED' else c['state']
    print(f'{name:20s} {status}')
"
```

Output format — only report changes, not repeats:
```
[CI delta] Clippy: pass (was pending)
[CI delta] Build Release: pass (was pending)
[CI delta] Test: still running
```

### Phase 4: Merge decision

When required gates resolve:
- All pass → proceed with merge (use `integrate-pr-stack` for sequencing)
- A required gate fails → run `fix-ci` or `fix-rust-lints` to address
- Non-blocking fails → explicitly note "non-blocking only, safe to merge"

## Distinguishing queue stall from genuine failure

| Signal | Meaning |
|--------|---------|
| `pending / 0s` for < 5 min | Normal startup delay |
| `pending / 0s` for 5–30 min | Likely queue contention |
| `pending / 0s` for > 30 min | Queue-stalled; local verify + `--admin` is valid |
| `in_progress` for > 20 min | Likely running (full workspace test suite takes ~14 min) |
| Check disappears after push | Normal: new commit resets CI identity |

## ICN-specific notes

- ICN has one self-hosted runner (`ci-runner`, 10.8.30.46). Parallel PR merges cause queue.
- Full workspace test suite runs ~14 min on the self-hosted runner.
- Build Release + Clippy + Test tend to run sequentially on the same runner job.
- `Test Coverage` and `Compare Against Base` often tail the required gates — ignore them for merge decisions.
- After rebasing a PR branch and force-pushing, GitHub resets its CI run. The old pass result is
  gone. New run must complete before the PR regains green status. Use `--admin` after local verify
  if the queue is stalled.

## Guardrails

- Do NOT poll blindly in a loop. Check, do advance work, check again.
- Do NOT conflate non-blocking failures with merge blockers.
- Do NOT report the full check table unchanged on every check — that adds noise without signal.
- When the runner has been busy for > 30 min and local gates pass, inform the user that `--admin`
  is viable rather than waiting indefinitely.
