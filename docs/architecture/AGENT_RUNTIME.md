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

## 2. The identity model: repository → lane → activation

The single most important thing this runtime gets right is *what is stable*. Everything else
follows from it.

```
repository                    repo_id        Git: realpath(--git-common-dir)
  └── worktree / lane         worktree_id    Git: realpath(--absolute-git-dir)
        ├── runtime activation(s)   id       one per live activation (0..N per lane)
        └── branch / HEAD / PR / task        live or advisory, never identity
```

### 2.1 The worktree is the lane; the branch is not its identity

A development session belongs to a concrete Git worktree. During that session the branch moves
constantly and legitimately: HEAD advances, the branch is rebased, it is renamed, the worktree
is deliberately detached, occasionally a different branch is checked out. **None of those events
creates a different lane.** A model that keys on branch would fork a lane's history every time
someone rebased.

Branch and HEAD are therefore *live state about* a lane, re-read on every query. The branch
captured at registration is kept only as `branch_at_registration`, a historical launch fact. It
is never used as current branch state. When the live branch differs from it, the runtime reports
`branch_changed: true` — a warning about state, not evidence that the session moved.

### 2.2 The lane key is Git-derived, never a basename

`worktree = "task-review"` is not an identity: two repositories on this VM can each contain a
worktree by that name, and the disk/lifecycle tooling has already been bitten by basename
collisions once. Rather than invent an identifier, the runtime uses what Git already
guarantees:

| Field | Source | Property |
|---|---|---|
| `repo_id` | `realpath(git rev-parse --git-common-dir)` | one per repository |
| `worktree_id` | `realpath(git rev-parse --absolute-git-dir)` | **one per worktree, unique within *and across* repositories** |
| `worktree_path` | `realpath(git rev-parse --show-toplevel)` | working directory |
| `worktree_name` | basename of the path | **display only — never a join key** |

For a linked worktree the admin dir is `<repo>.git/worktrees/<name>`; for a main worktree it is
the repo dir. Either way `task-review` under `icn.git` and under `nycn.git` resolve to different
ids, so the collision is impossible rather than unlikely.

Resolution goes through `discoverWorktree()`, which asks Git — it never parses a path. Two
consequences that are enforced, not assumed:

- **Hook `cwd` is a starting point, not truth.** Tool execution happens from subdirectories and
  scratch paths; Git resolves the owner from anywhere inside the worktree.
- **`ICN_ROOT` is a hint that loses to `cwd`.** Validating that an env root is a real worktree
  is not sufficient — a valid answer to the wrong question is still wrong. On icn-dev `ICN_ROOT`
  is pinned to the `mcp-host` worktree by the shell profile, so an env-root-first resolution
  filed *every* session under `mcp-host`. It is consulted only when `cwd` resolves to nothing.

### 2.3 Provider conversation vs runtime activation

Captured from this installation's real hook payloads:

- `session_id` is present on every hook event and identical across all of them;
- it is **unchanged across `claude --resume`** (`SessionStart` fires again with `source: resume`);
- there is **no `CLAUDE_SESSION_ID` environment variable**.

So the provider id identifies a **conversation**, which may outlive several runtime activations:

```
conversation X ──▶ activation A ──▶ SessionEnd (all authority surrendered)
               └─▶ (later --resume) ──▶ activation B     A ≠ B, same provider id
```

| Concept | Field | Lifetime |
|---|---|---|
| Runtime activation | `id` (UUID) | one live activation |
| Provider conversation | `provider_session_id` | may span resume cycles |
| Process | `agent_pid`, `host` | correlation only |

Registration idempotency is scoped to the *live* activation: a duplicate hook within one
activation returns the existing row; a resume after release creates a new activation that
inherits **no** claims, watchers or mailbox authority. The runtime therefore does **not** assert
"a provider session id is never reused after release" — the live probe disproved that.

`pid@host` is rejected outright as identity. PIDs are recycled, so a future process could
inherit a dead session's claims. It is correlation metadata at best.

### 2.4 Several sessions may occupy one lane

`one worktree == one session` is **not** a database invariant. Real situations include an
interactive agent plus a review session, an old activation briefly overlapping a resumed one,
operator inspection, and accidental concurrent sessions — the last of which is precisely what
we want the runtime to *detect*. Each session keeps its own activation id while pointing at the
same `worktree_id`; `classify()` reports `contention: {count, session_ids}` and registration
returns `co_occupants`.

File claims, watchers and every other authority stay **session-scoped**, never worktree-scoped.
Releasing one session leaves every other occupant's authority untouched.

### 2.5 Field classification

| Class | Fields |
|---|---|
| **Authoritative stable identity** | `id`, `repo_id`, `worktree_id`, `worktree_path` |
| **Authoritative live state** | live branch/HEAD (read from Git), `last_heartbeat`, `last_progress`, `progress_count`, contention |
| **Advisory launch metadata** | `provider_session_id`, `branch_at_registration`, `head_at_registration`, `task_ref`, `pr_ref`, `task_description`, `provider`, `transcript_path`, `worktree_name`, `worktree` |
| **Correlation only** | `agent_pid`, `host` |
| **Derived, never stored** | lifecycle state, expiry, stall, `branch_changed` |

Branch/PR/issue *state* is a live query (`live_branch_state`, `live_pr_state`,
`live_issue_state` in `sources.json`). The registry stores the **reference**, never the state.

## 2.6 One registry per repository

The registry is a single SQLite database resolved from `git rev-parse --git-common-dir`, so
every worktree of a repo shares it: `<repo>.git/icn-ops.db`. `ICN_OPS_DB` still overrides.

This is load-bearing, not cosmetic. The previous default resolved *beside the executing JS*,
which was harmless while the MCP server was the only writer. This runtime adds a second writer —
the hook CLI, which runs out of the agent's **own** worktree — while `~/.claude.json` pins the
MCP server to an absolute path in the `mcp-host` worktree. The observed result was two live
databases with different schemas, hook-registered sessions invisible to every `mcp__icn-ops__*`
tool, and `claim_files` failing a foreign-key check because the session id existed only in the
other file.

**Operational note:** after this lands, the `mcp-host` MCP server must be rebuilt and restarted
for running sessions to pick up the shared registry. Rows in the old per-worktree databases are
not migrated; they are stale session state and expire by TTL anyway.

## 3. Registration seam and failure policy

**Seam:** the `SessionStart` hook, via `ops/scripts/icn-agent-session register`.

Claude Code hooks run as ordinary subprocesses and cannot call MCP tools, so the runtime
exposes the same core through a CLI (`ops/mcp/dist/cli/session.js`). MCP tools and the CLI call
**one shared module** (`ops/mcp/src/runtime/session-runtime.ts`); there is no second
implementation to drift.

Idempotency: registration is keyed on the provider conversation id taken from the hook
payload's `session_id` field (§2.3). Re-running `register` for the same live activation returns
the existing row, so a hook that fires twice cannot double-register. Launchers with no provider
id pass `--identity-file`, which mints a UUID once and persists it for the harness's lifetime;
`pid@host` is never synthesised.

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
  that correspond to real runtime effects: a file edit (`PostToolUse` on `Edit|Write`) or a
  completed command (`PostToolUse` on `Bash`). `progress_count` is monotonic, so a consumer can
  sample it twice and prove motion without trusting a clock.

  **A completed turn is deliberately NOT progress.** `Stop` and `UserPromptSubmit` are
  *interaction*: they prove the harness produced a response or a human is driving, not that
  task state moved. An agent stuck in a retry cycle completes turns indefinitely while nothing
  advances; counting those would let it defeat `PROGRESS-STALLED`, the one signal that catches
  it. There is no `turn` progress kind, and a test asserts 200 consecutive turn boundaries
  leave `progress_count` at 1.
- **`current_activity`** — advisory free text for humans reading `list_sessions`.

A polling wait loop advances `last_heartbeat` and never `progress_count`; that is exactly the
`PROGRESS-STALLED` signature in §5. No periodic self-heartbeat timer is installed, because a
timer is precisely the mechanism that makes a deadlocked session look healthy.

---

## 5. Lifecycle classification (fail-safe by construction)

`icn-agent-session classify --worktree <name>` returns one state. The registry is the
**first** source of evidence; process observation is corroborating, never overriding.

| State | Means |
|---|---|
| `REGISTERED-ACTIVE` | Row present, heartbeat inside TTL, progress recent. |
| `PROGRESS-STALLED` | Row present, heartbeat fresh, `progress_count` unchanged past the stall window. |
| `REGISTERED-EXPIRED` | Row present, heartbeat older than TTL. |
| `UNREGISTERED-OBSERVED` | No row for this lane. |
| `REGISTRY-UNAVAILABLE` | Registry unreadable, or no lane could be resolved. |

**These are observations, not verdicts.** `classify` returns no `retireable` field: deciding a
lane may be reclaimed can mean killing a live agent or destroying an in-flight build, and every
attempt to answer it inside this module produced a defect that passed its own tests. Consumers
apply their own policy to the facts, and retirement stays read-only and operator-approved.

**Absence of a row is not evidence of absence of a session.** A missing row is indistinguishable
from a pre-integration session, an unsupported launcher, or a registry failure.

**Absence of an observation is not evidence of absence either.** `observed_pids` distinguishes
three states, and only the third can support retirement:

| Value | Means |
|---|---|
| omitted / `null` | **nobody looked** — protected |
| `[]` | an observation was performed and found nothing |
| `[…]` | processes are holding the lane — protected |

The registry also **corroborates itself**: it records the session's own `agent_pid`, so a live
agent process protects the lane regardless of what any caller did or did not observe. Retirement
requires *both* an affirmative empty observation *and* a dead (or absent) recorded pid.

Classification is keyed on `worktree_id` (§2.2), and **protection is a property of the lane, not
of any one row**. Heartbeat ages are reported from the freshest session, but liveness is
aggregated across *every* occupant: a live recorded `agent_pid` anywhere on the lane is
reported, and named in `reason`. Selecting a single "primary" by heartbeat freshness and
judging from that row produced `retireable: true` on a lane with a running agent, because a
crashed peer with a fresher heartbeat won the selection and only its dead pid was checked.
`contention` reports every occupant.

TTL is `ICN_SESSION_TTL_MINUTES` (default 30, matching the pre-existing `list_sessions`
window), clamped to a 5-minute floor — it is read from the ambient environment of whoever runs
classify, and a value of 1 turned a two-minute-old heartbeat into a retirement candidate. Stall window is `ICN_SESSION_STALL_MINUTES` (default 90, matching `icn-lane-audit`'s
`QUIESCENT_AFTER_MIN`).

### 5.1 Supervision of long-running operations — NOT IN THIS LAYER

A legitimately long build emits no hook events, so its heartbeat ages past the TTL and the lane
looks idle. Declaring the operation is the right answer, and it was implemented here — then
removed.

Five of the six P0 defects found across three independent review rounds were in that surface.
Each repair satisfied its own test and its own comment while breaking the invariant one layer
out, because a lane's protection had three competing sources of truth: the supervision row's
lane, the owning session's lane, and the live pid. That is a design problem, not a diligence
one, so it is being redesigned rather than patched again.

The work is preserved on `ops/agent-supervision-lifecycle` for a separate PR. Until it lands,
a long build shows as `PROGRESS-STALLED` or `REGISTERED-EXPIRED` — which is honest, because
this layer genuinely does not know the difference between a long build and an abandoned lane.
No consumer may act on that without operator approval.

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

### What release surrenders

Release keeps the **pre-existing DELETE semantics** — the session row is removed and
`file_claims` cascade — so there is no "released but still holding claims" state to get wrong.
History was already owned by `ops/state/session-log.jsonl` and stays there. The complete
inventory of session-scoped resources, kept beside the code that clears it:

| Resource | Class | On release |
|---|---|---|
| `file_claims.session_id` | authority | deleted (FK cascade) |
| `watchers_process.session_id` | authority | invalidated → `status='released'` (no FK; would otherwise stay `running` and keep supervising for a dead session) |
| `mailbox.to_session` (unread) | authority | invalidated → marked read |
| `mailbox.from_session` | history | kept |
| `events.scope = session:<id>` | history | kept |

The rule: **a released session retains history and surrenders every authority.** Because rows
are deleted rather than soft-marked, this is true by construction rather than by discipline.

Deliberately **not** introduced: a `state='released'` column. It would create rows that read as
inactive while still owning claims — the precise failure this table avoids.

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

## 8. Lifecycle consumers

`icn-lane-audit` (icn#2653) and any future consumer should join on **canonical lane identity**
first and corroborate with observation second:

```
icn-agent-session classify --path <worktree>     # or --worktree-id <id>
```

It returns JSON facts — `state`, `reason`, `contention`, `branch_changed`, `live_branch`,
`live_agent_pids`, heartbeat/progress ages and `progress_count`. The exit code reports whether
facts could be produced (`0`) or not (`3`); there is deliberately **no** "retirement candidate"
code.

Binding rules for every consumer:

- **A missing row is not permission.** A lane with no row may be a pre-integration session, an
  unsupported launcher, or a registry failure.
- **This layer issues no retirement verdict at all.** A consumer that wants one owns that
  policy, and owns the consequences.
- **A merged PR is not permission either.** Merge state is one input; it says nothing about who
  is currently working in the lane.
- **Process observation may only make a verdict safer**, never more permissive: an expired
  heartbeat plus a live process downgrades to protected, not the reverse.
- **The disk guard gets no process-control authority.** `icn-disk-guard` classifies and
  reclaims build output under its own policy; it does not signal processes, and retirement
  stays a separate, operator-approved step.

The hierarchy consumers should model is the one in §2: repository → lane → activation(s), with
branch/HEAD/PR hanging off the lane as live or advisory metadata, never as identity.

## 9. Safe waiting

`ops/scripts/icn-wait` is the supported way to wait for something. It exists because the
alternative agents invent is defective in two reproducible — and materially different — ways.
A self-matching `pgrep -f` predicate is **logically non-terminating**: the observer matches
itself, so no future event can satisfy it. An unbounded sentinel wait is **not** impossible —
the file may legitimately arrive later — but with stderr swallowed and no bound or producer
identity it cannot distinguish a working producer from a dead one, a deleted scratch directory,
or a sentinel that will never come, so it can spin indefinitely while appearing active. Both are
documented in
`ops/scripts/README-icn-wait.md`, **blocked** at the Bash tool seam by
`.claude/hooks/pre-bash-guard.py` (which explicitly does not flag the safe `[m]utate.py`
bracket idiom), and pinned by regression tests in `scripts/test-icn-wait.sh` and
`scripts/tests/test_pre_bash_guard.py`.

Every wait is bounded. Indefinite blocking is available only behind an explicit flag.
