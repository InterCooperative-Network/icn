# Agent-worktree lifecycle audit

Reports lifecycle debt on the `icn-dev` development VM: merged lanes whose build output cannot
be reclaimed because processes still hold the worktree.

```bash
ops/scripts/icn-lane-audit                 # human-readable
ops/scripts/icn-lane-audit --json          # machine-readable
ops/scripts/icn-lane-audit --plan-retire   # what retirement WOULD signal; acts on nothing
ops/scripts/icn-lane-audit --self-test     # 28 policy assertions
```

Read-only. It never signals a process, deletes anything, or mutates git.

## Why this exists

`icn-disk-guard` refuses to reclaim a Cargo target while any process holds the worktree. That
refusal is correct and it is also unbounded: a merged lane whose shells never exit pins its
build output forever. Two merged lanes currently pin **120 GB** this way.

## The finding that shaped the design

The three shells pinning the merged #2644 lane (aged 28 h, 39 h and 67 h) are not idle. Each
re-executes every 5–20 seconds and has a live parent. Each is also **incapable of ever
finishing**:

```
until ! pgrep -f "scratchpad/mutate.py"; do sleep 20; done
```

`pgrep -f` matches command lines, and the waiting shell's own command line *contains that
pattern*. The shell matches itself, the predicate is permanently false, the loop spins forever.
A third waits on `grep -q "^EXIT=" <file>` for a file whose originating session was cleaned up
and which no longer exists.

So the lanes look maximally alive by every naive metric — fresh child process, live parent,
recent process activity — and are maximally dead in fact.

**The model therefore measures progress, not activity**, and treats activity-without-progress
as the specific pathology it is. Both non-terminating shapes are detected structurally and
reported per process.

The `[b]racket` idiom (`pgrep -f "[m]utate.py"`) is the correct way to avoid self-matching and
is explicitly *not* flagged — a false positive here would propose a healthy process for
termination.

## The existing lifecycle machinery is unwired

The ICN ops MCP already defines exactly the model this needs:

```
sessions(id, repo, worktree, task_description, started_at, last_heartbeat)
register_session · heartbeat · release_session · list_sessions
```

with "active" defined as `last_heartbeat > now - 30 minutes`, and a session bound to a worktree
name. **Nothing populates it.** `list_sessions` returns `[]` and `recent_sessions` reports no
history: agents never call `register_session` or `heartbeat`.

That is the root cause of stale lanes. A designed, authoritative liveness signal exists and is
not wired up, so the only observable evidence is "a process has a cwd here", which never
expires. Wiring agent launch to `register_session`/`heartbeat` would make this tool's
heuristics largely unnecessary — that is the real fix, and it is an agent-harness change rather
than an ops-script one.

Until then, **absence of a heartbeat is treated as no signal, never as evidence of staleness.**

## States

| State | Meaning | Retireable |
|---|---|---|
| `UNPINNED` | no processes; the disk guard's business | n/a |
| `ACTIVE` | build/test child running, or progress within 90 min | no |
| `OPEN-PR` | the lane still has a job | no |
| `UNMERGED` | branch not merged | no |
| `PINNED-DIRTY` | uncommitted source — unique state | no |
| `PINNED-UNKNOWN` | unrecognised process, or unknown git state — fail safe | no |
| `QUIESCENT` | merged and quiet, but inside the grace window | no |
| `STALE-CANDIDATE` | merged, agent shells only, no work children, clean, quiet ≥ 8 h, oldest pin ≥ 4 h | **candidate only** |

Safety predicates dominate staleness: an unknown process outranks every other signal, and dirty
state outranks staleness. Thresholds live in one config block and are environment-overridable.

## Automatic retirement is NOT enabled, and should not be yet

`--plan-retire` prints what termination *would* target and stops. Nothing in this branch signals
a process. Three reasons, all of which would have to change:

1. **No authoritative activity signal.** The session registry is empty, so liveness is inferred
   from filesystem and process heuristics. That is good enough to *report*, not to kill.
2. **The parents are alive.** The shells pinning #2644 are children of running Claude CLI
   processes (aged 1.7 and 2.8 days) which are presumably blocked awaiting them. Terminating
   the child would likely *unblock* a stuck agent — plausibly a repair — but it reaches into
   another live session, and "plausibly" is not a safety argument.
3. **The detector is new.** Its non-terminating heuristics found the real pathology on the first
   run, and they have not yet been wrong in a way anyone has observed. That is not the same as
   being right.

### Operator-approved procedure

```bash
ops/scripts/icn-lane-audit --plan-retire      # review exactly which PIDs, and why
kill -TERM <pid>                               # SIGTERM only; never SIGKILL by default
ops/scripts/icn-lane-audit                     # confirm the lane is no longer pinned
icn-disk-guard                                 # the guard now classifies the target on its own rules
```

The handoff to the disk guard is deliberately implicit: once the pins are gone, the guard sees
a merged, unpinned, clean target and applies its own independent policy. Neither tool calls the
other, and the disk guard gains no process-control powers.

## Worktree retirement

Out of scope here, and not implemented. `wt-clean` already exists for finished worktrees and is
user-local; ICN policy does not currently authorise automatic worktree deletion. Evidence that
would make it safe: merged branch, clean tree, no processes, no unique untracked files, a grace
period, no tooling references, and history preserved remotely. Proposal only.

## Scheduling

None installed. If a periodic audit is wanted, hourly-to-4-hourly is ample — this is debt
reporting, not monitoring, and any automatic retirement threshold must stay far longer than the
audit cadence.
