# ICN Agent Runtime

**Status:** active · **Owner domain:** `agent_runtime_state` (see `ops/state/truth/sources.json`)

This document describes how an ICN development agent session comes into existence, what
authoritative identity it carries, how its liveness is judged, and how it discovers the
capabilities the repository provides for it.

It is deliberately short. It says **where authoritative information lives**; it does not
restate that information.

---

## 1. Current agent-launch architecture (established truth, not history)

There is no single agent "framework". There are several entrypoints that converge on one
process shape, and they were verified by inspecting the live `icn-dev` VM and this repository —
not by reading prior design docs.

### 1.1 Entrypoints

| Entrypoint | What it is | Shares the lifecycle? |
|---|---|---|
| `icn-start [worktree]` | Human front door on `icn-dev`. Resolves a worktree under `~/icn-dev/worktrees/icn/`, pins `ICN_ROOT`, `exec claude`. | Yes — via Claude Code hooks |
| `icn-claude` | Same, from a remote host over SSH; always lands in the `mcp-host` worktree. | Yes — same hooks |
| Claude Code interactive / remote (`ccd-cli`) | The actual agent process. One OS process per session. | Yes — same hooks |
| Claude Code subagents (`Agent` tool) | In-process children of a session. They do **not** get their own OS process, hook events, or MCP client. | **No** — see §6 |
| Provider adapters (`.codex/`, `.cursor/`, `.opencode/`) | Configuration overlays that point a different provider at the same MCP server and skills. | Partially — MCP tools yes, hooks no |
| CI agents (`.github/workflows/`) | Non-interactive, ephemeral, no worktree, no registry. | No — out of scope by design |

**The real front door is not a script.** `icn-start` only chooses a directory and execs
`claude`; every meaningful startup behaviour is driven by **Claude Code hooks** declared in
`.claude/settings.json`. That is the narrowest seam every interactive session passes through,
so it is where this runtime attaches.

### 1.2 Pre-existing pieces this runtime wires together (it invents little)

| Concern | Authoritative owner | Where |
|---|---|---|
| Truth-domain ownership | `sources.json` | `ops/state/truth/sources.json` |
| Skill ownership + mirror/adapter policy | `skills.json` | `ops/state/truth/skills.json` |
| Skill drift enforcement | `check-skill-registry.py` | `scripts/`, run by `.github/workflows/agent-drift-check.yml` |
| Canonical skills | `.agents/skills/` (mirrored to `.claude/skills/`) | repo |
| MCP tools | `ops/mcp/src/tools/*.ts` | repo |
| Session registry | `sessions` table | `ops/mcp` SQLite (`ICN_OPS_DB`) |
| Process supervision | `watchers_process` table + `watch_process` tool | `ops/mcp` |
| Startup orientation | `session-orient.sh` | `.claude/hooks/` |
| Agent operating contract | `AGENTS.md` | repo root |

### 1.3 Why the session registry was unused

The registry was complete and correct, and nothing was obliged to call it:

1. **No launch seam called it.** `register_session` is an MCP *tool*. Tools are invoked by a
   model deciding to invoke them. Nothing in the launch path — `icn-start`, hooks, or provider
   config — called it, so registration depended entirely on an agent remembering.
2. **It owned no truth domain.** `ops/state/truth/sources.json` registers an owner for every
   domain that matters. There was no `agent_runtime_state` domain, so no consumer was directed
   to the registry and no drift check noticed its absence.
3. **Its identity was too thin to be worth adopting.** The row held `repo`, `worktree`,
   `task_description`, `started_at`, `last_heartbeat` — no branch, PR, task ref, parent, pid,
   or provider. A consumer could not join a row to a process or a lane, so observation of
   `/proc` was strictly more useful than the registry.
4. **`last_heartbeat` could not distinguish liveness from progress.** A single timestamp
   advanced by an explicit `heartbeat` call means "something called heartbeat", which a
   deadlocked-but-looping agent can do forever. See §4.

The fix is therefore not "write a registry" — it is to give the existing registry a launch
seam (§3), a richer identity (§2), a progress signal (§4), and a declared owner (§7).

---

## 2. The agent session contract

A session's **authoritative** identity lives in one row of the `sessions` table. Fields are
classified so consumers know how much to trust each one.

### Authoritative — written by the runtime, owned here
| Field | Meaning |
|---|---|
| `id` | Session UUID. Stable for the session's life. |
| `repo` | Repo key (`icn`, …). |
| `worktree` | Worktree directory name, not a path. |
| `branch` | Branch at registration. Advisory after that — branches move. |
| `state` | `active` \| `released`. Lifecycle state. |
| `started_at` / `last_heartbeat` / `last_progress` | Timestamps; see §4. |
| `progress_count` | Monotonic counter. Only advances on evidence of *work*. |
| `agent_pid` / `host` | The OS process to correlate against `/proc`. |
| `parent_session_id` | Set for child sessions; NULL for roots. |

### Advisory — supplied at launch, may be stale or absent
`task_description`, `task_ref` (e.g. `icn#2653`), `pr_ref`, `provider`, `current_activity`.

### Derived — never stored, always recomputed
Everything the auditor computes: expiry, stall, pin state, and the lane classification in §5.
Storing a derived verdict would let it go stale silently.

### Not duplicated here
Branch/PR/issue *state* is a live query (`live_branch_state`, `live_pr_state`,
`live_issue_state` in `sources.json`). The registry stores the **reference**, never the state.

---

## 3. Registration seam and failure policy

**Seam:** the `SessionStart` hook, via `ops/scripts/icn-agent-session register`.

Claude Code hooks run as ordinary subprocesses and cannot call MCP tools, so the runtime
exposes the same core through a CLI (`ops/mcp/dist/cli/session.js`). MCP tools and the CLI call
**one shared module** (`ops/mcp/src/runtime/session-runtime.ts`); there is no second
implementation to drift.

Idempotency: registration is keyed on the harness session identifier
(`CLAUDE_SESSION_ID`, else `pid@host`). Re-running `register` for the same key returns the
existing row instead of creating a second one, so a hook that fires twice cannot double-register.

**Failure policy: degrade loudly, never block, never overclaim.**

- Registration failure does **not** fail launch. The registry is observability, and ICN policy
  does not gate development on it.
- Failure prints an explicit `agent-runtime: DEGRADED (unregistered)` line into session context.
  The runtime never reports lifecycle tracking as active when registration failed.
- Operations that *require* authoritative lifecycle state fail closed: with no row, the
  classifier returns `REGISTRY-UNAVAILABLE`, and consumers must treat that as **protected**,
  never as "safe to retire" (§5).

---

## 4. Heartbeat and progress: three distinct claims

The `#2644` lanes proved that a process can emit activity forever without progressing. The
runtime therefore separates three claims that were previously conflated:

```
process alive   !=   session healthy   !=   meaningful progress occurring
   /proc              last_heartbeat          last_progress + progress_count
```

- **`last_heartbeat`** — "the harness is still running". Advanced by any hook firing.
  Cheap, and deliberately weak evidence.
- **`last_progress` + `progress_count`** — "work happened". Advanced **only** by hook events
  that correspond to real runtime effects: a file edit (`PostToolUse` on `Edit|Write`), a
  completed non-trivial command (`PostToolUse` on `Bash`), or an agent turn boundary (`Stop`).
  `progress_count` is monotonic, so a consumer can sample it twice and prove motion without
  trusting a clock.
- **`current_activity`** — advisory free text for humans reading `list_sessions`.

A polling wait loop advances `last_heartbeat` and never `progress_count`; that is exactly the
`PROGRESS-STALLED` signature in §5. No periodic self-heartbeat timer is installed, because a
timer is precisely the mechanism that makes a deadlocked session look healthy.

---

## 5. Lifecycle classification (fail-safe by construction)

`icn-agent-session classify --worktree <name>` returns one state. The registry is the
**first** source of evidence; process observation is corroborating, never overriding.

| State | Means | Retireable? |
|---|---|---|
| `REGISTERED-ACTIVE` | Row present, heartbeat inside TTL, progress recent. | No |
| `PROGRESS-STALLED` | Row present, heartbeat fresh, `progress_count` unchanged past the stall window. | Only with operator approval |
| `REGISTERED-EXPIRED` | Row present, heartbeat older than TTL, no live pid. | Candidate |
| `UNREGISTERED-OBSERVED` | No row, but processes hold the worktree. | **No** — pre-integration or unsupported launcher |
| `REGISTRY-UNAVAILABLE` | Registry unreadable. | **No** |

**Absence of a row never means "safe to terminate."** A missing row is indistinguishable from a
pre-integration session, an unsupported launcher, or a registry failure, so it resolves to
protected. This is asserted by tests, not by convention.

TTL is `ICN_SESSION_TTL_MINUTES` (default 30, matching the pre-existing `list_sessions`
window). Stall window is `ICN_SESSION_STALL_MINUTES` (default 90, matching `icn-lane-audit`'s
`QUIESCENT_AFTER_MIN`).

---

## 6. Release guarantees — stated exactly

| Exit path | Released? | Mechanism |
|---|---|---|
| Normal completion / user exit | Yes | `SessionEnd` hook |
| Controlled shutdown (SIGTERM/SIGINT to the harness) | Yes, if the harness runs `SessionEnd` | same |
| Agent error with harness still alive | Yes | same |
| Parent cancellation of a child session | Yes | parent releases children it registered |
| `SIGKILL`, VM death, OOM, power loss | **No** | not representable; heartbeat expiry (§5) covers it |

The runtime does not pretend otherwise. A row that stops heartbeating becomes
`REGISTERED-EXPIRED` after TTL; that is the only mechanism for abrupt death, by design.

### Subagents and child sessions
Claude Code subagents (`Agent` tool) run **in-process**. They receive no `SessionStart` event,
no separate pid, and no MCP client of their own, so they cannot be auto-registered — this is a
provider limitation, stated honestly rather than papered over. What *is* supported:

- A session spawned as its own process (a second `icn-start`, a review lane) registers
  normally and may declare `--parent-session <id>`.
- Children are released independently; releasing a parent does not release children, and an
  orphaned child does **not** keep its parent classified as active, because classification
  reads the parent's own `progress_count`.

---

## 7. Capability discovery — the mechanism, not a list

The failure mode this replaces is a hand-maintained prompt listing every tool, which drifts the
day someone adds a skill.

`docs/reference/project-index/generated/agent-capabilities.json` is **generated** by
`scripts/generate-agent-capabilities.py`, which derives capabilities from their canonical
locations:

| Capability kind | Derived from |
|---|---|
| MCP tools | `server.tool("…")` registrations in `ops/mcp/src/tools/*.ts` |
| Skills | `ops/state/truth/skills.json` (the registry that already owns them) |
| Hooks | `.claude/settings.json` hook declarations |
| Repo helper scripts | `ops/scripts/` executables with a `#:capability` header |
| Truth domains | `ops/state/truth/sources.json` |

It is surfaced three ways: the `icn_ops_agent_runtime` MCP tool, the `session-orient.sh`
startup line (a pointer, not a dump), and the file itself.

`scripts/check-agent-capabilities.py` regenerates the manifest and fails if it differs from the
committed copy. It runs in `agent-drift-check.yml` on **every** PR.

**Therefore:** adding a capability in its canonical location and regenerating makes it
discoverable to every launcher that reads the manifest, and *forgetting* to regenerate fails
CI. No launcher, provider adapter, or prompt is edited to add a capability.

Limits, stated plainly: the manifest describes capabilities to a session that reads it. Provider
adapters that do not run Claude Code hooks (`.codex/`, `.cursor/`, `.opencode/`) get the MCP
tool and the file, but not the startup line or automatic registration. See §1.1.

---

## 8. Safe waiting

`ops/scripts/icn-wait` is the supported way to wait for something. It exists because the
alternative agents invent is defective in two reproducible ways — a self-matching
`pgrep -f` predicate, and a sentinel wait whose file cannot appear. Both are documented in
`ops/scripts/README-icn-wait.md`, rejected by `.claude/hooks/pre-bash-guard.py`, and pinned by
regression tests.

Every wait is bounded. Indefinite blocking is available only behind an explicit flag.
