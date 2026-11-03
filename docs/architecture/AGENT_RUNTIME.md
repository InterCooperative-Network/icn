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
| Provider adapters (`.codex/`, `.cursor/`, `.opencode/`) | Configuration overlays for other providers. Only `.cursor/mcp.json` actually declares the ops MCP; `.codex/mcp/servers.example.json` is an example that omits it, and `.opencode/opencode.json` has no MCP block at all. | `.cursor` partially — MCP tools yes, hooks no. `.codex`/`.opencode`: manifest file only |
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
| `worktree_id` | `realpath(git rev-parse --absolute-git-dir)` | **one per worktree, unique within *and across* repositories — in SPACE. Not unique over TIME: see `worktree_generation`.** |
| `worktree_generation` | UUID minted into `<admin-dir>/icn-lane-generation` | **which generation of the lane this is** |
| `worktree_path` | `realpath(git rev-parse --show-toplevel)` | working directory |
| `worktree_name` | basename of the path | **display only — never a join key** |

### 2.2.1 The lane in TIME — `worktree_generation`

`worktree_id` names a *slot*, and git reclaims slots. `git worktree remove` deletes
`<repo>/.git/worktrees/<basename>`, and the next `git worktree add` with the same basename is
handed **the same admin directory** — so a new worktree, on a new branch, at a different path
*or the same one*, inherits the previous lane's `worktree_id`.

That matters because unreleased rows are joined to the lane by that id. Observed directly: a
freshly created worktree classified `REGISTERED-ACTIVE` holding the *previous* lane's session
row. Comparing `worktree_path` does not help — recreate at the same pathname and repo, admin
dir and path all match while the lane is genuinely a different one.

So identity carries a **generation**: a UUID minted into `<admin-dir>/icn-lane-generation`.

- **Where it lives is the whole design.** It sits inside the one container git itself deletes,
  so it cannot outlive its generation. Not the working tree (a user may keep that), not the
  repo root, not a global cache — each of those survives removal and reintroduces the aliasing.
- **Minted atomically**, by writing a complete temp file and `link(2)`-ing it into place: a
  single winner, and the target is complete the instant it is visible. (`O_CREAT|O_EXCL`
  followed by a separate write let a loser read a zero-length file: 6 of 240 concurrent minters
  got nothing.)
- **Minted only on the identity path.** `discoverWorktree()` mints; the classification path
  reads and never writes, because it receives a caller-supplied `worktree_id` and a read path
  must not create files at an arbitrary location.
- **What it survives:** commits, branch switches, branch renames, history-rewriting rebases,
  detached HEAD, `git worktree move`, and symlinked access. None of those make it a different
  worktree, and the admin directory is untouched by all of them.
- **NULL means UNKNOWN, never "different".** A row written before schema v5, or a lane whose
  token could not be minted, is *kept* by the filter. Rows are dropped only when both sides are
  known and differ, so this can only ever make a lane look **more** occupied.

A re-registering session always refreshes its row's generation, including when the lane id is
unchanged — otherwise a conversation resuming into a recreated lane would keep the dead
generation and be filtered out of the lane it is actually working in.

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
| **Authoritative stable identity** | `id`, `repo_id`, `worktree_id`, `worktree_generation`, `worktree_path` |
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

### 3.1 How a fresh worktree gets that CLI

`dist/` is gitignored, and no worktree-creation path — not `wt-new`, not Claude Code's nested
`.claude/worktrees/<name>` lane, not `git worktree add` by hand — builds it. Requiring an
in-tree build therefore made a new lane **permanently** unregistered rather than briefly
degraded. When this was found, 44 of 50 worktrees on the dev VM could not register, the
registry reported zero sessions while four Claude activations were live, and the readiness
audit that found it was itself running unregistered. `mcp-host` worked only because its build
happened to exist.

`ops/scripts/icn-runtime-build` closes that. It resolves a build of the session CLI from a
shared cache under `${XDG_CACHE_HOME:-~/.cache}/icn/agent-runtime`, keyed by a SHA-256 over the
exact bytes of the checkout's runtime sources (`ops/mcp/src/**` plus `package.json`,
`package-lock.json`, `tsconfig.json`), hashed with repo-relative paths so lanes holding
identical sources agree on one key.

- **Anti-stale — strengthened, not traded away.** The old rule was *this checkout's location
  wins*. The cache key is a content hash, so running `<cache>/build/<fingerprint>/dist` proves
  the executing bytes were compiled from **these** sources. A location rule only ever proved
  the build sat next to them; it never noticed an in-tree `dist` gone stale against its own
  `src`. An in-tree build still wins when present, so a developer's `npm run build` is
  unaffected.
- **Cost.** Measured on the dev VM: a cache hit is free and is the normal case, because a lane
  branched from `main` has not touched `ops/mcp`. A first-of-its-kind source state costs one
  `tsc` (~10s). The dependency tree (~157M, `better-sqlite3` compiled from source) is
  machine-level state keyed by the lockfile, installed once and shared by every lane.
- **Which events pay it.** `register` fires once per session and re-resolves unconditionally,
  so a session always starts on a runtime built from the sources currently in the lane.
  `progress`/`interaction` fire after every tool call and take the resolved path as-is; they
  record liveness into a shared registry rather than interpreting it, and re-fingerprinting 52
  files per tool call would cost ~116ms to re-answer a question they do not ask.
- **Never blocking on the expensive half.** A missing `dist` is worth ~10s of a human's time
  and is built inline. A missing dependency tree is a multi-minute native compile: the session
  hands it to a detached background bootstrap, says plainly that *this* session is unregistered
  and the next one will not be, and never fabricates a registration to hide it.
- **Concurrency.** Entries are staged in a private directory and published with `rename(2)`,
  with the completion marker written before the move, so a half-built tree is never observable.
  `flock` stops duplicate work; it is not the correctness argument, since two racing builds of
  one fingerprint produce the same bytes.

The bootstrap publishes its resolution into the lane as a symlink at `ops/mcp/dist` (ignored by
`.gitignore`), which keeps later hooks on the zero-cost path and makes provenance readable —
the link target names the fingerprint the runtime was compiled from.

Proven by `scripts/test-agent-runtime-bootstrap.sh`, whose standing rule is that no case may
start from a worktree that already has a `dist`.

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
three states, and only the SECOND can support retirement:

| Value | Means |
|---|---|
| omitted / `null` | **nobody looked** — protected |
| `[]` | an observation was performed and found nothing — **the only value that can support retirement**, and only alongside a dead or absent recorded pid |
| `[…]` | processes are holding the lane — protected |

The registry also **corroborates itself**: it records the session's own `agent_pid`, so a live
agent process protects the lane regardless of what any caller did or did not observe. Retirement
requires *both* an affirmative empty observation *and* a dead (or absent) recorded pid.

Classification is keyed on `worktree_id` **and `worktree_generation`** (§2.2, §2.2.1), and
**protection is a property of the lane, not
of any one row**. Heartbeat ages are reported from the freshest session, but liveness is
aggregated across *every* occupant: a live recorded `agent_pid` anywhere on the lane is
reported, and named in `reason`. Selecting a single "primary" by heartbeat freshness and
judging from that row produced `retireable: true` on a lane with a running agent, because a
crashed peer with a fresher heartbeat won the selection and only its dead pid was checked.
`contention` reports every occupant.

TTL is `ICN_SESSION_TTL_MINUTES` (default 30, matching the pre-existing `list_sessions`
window), clamped to a 5-minute floor — it is read from the ambient environment of whoever runs
classify, and a value of 1 turned a two-minute-old heartbeat into a retirement candidate. Stall window is `ICN_SESSION_STALL_MINUTES` (default 90, chosen to match the
`QUIESCENT_AFTER_MIN` proposed by icn#2653, which is not merged).

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
| `watchers_process.session_id` | **not touched** | That table belongs to the pre-existing `watch_process` feature and its background poller. Supervision, which also used it, left with `ops/agent-supervision-lifecycle`, and clearing watcher rows went with it. A row whose session is deleted survives as `running` until its pid exits — a pre-existing leak on `main`, not one this layer introduces or fixes. |
| `mailbox.to_session` (unread) | authority | invalidated → marked read |
| `mailbox.from_session` | history | kept |
| `events.scope = session:<id>` | history | kept |

The rule: **a released session retains history and surrenders every authority this layer
grants.** Because rows are deleted rather than soft-marked, that much is true by construction
rather than by discipline.

One honest exception, listed above: `watchers_process` rows are not cleared. That table and its
poller predate this work and are not part of the session runtime — a watcher outliving its
session is a pre-existing leak on `main`, and clearing it belongs with the supervision surface
that was split out. It is named here rather than quietly omitted, because an inventory that
claims to be complete has to be.

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
| MCP tools | a live `tools/list` from the built server — introspected, never grepped from source |
| Skills | `ops/state/truth/skills.json` (the registry that already owns them) |
| Hooks | `.claude/settings.json` hook declarations |
| Repo helper scripts | `ops/scripts/` executables with a `#:capability` header |
| Truth domains | `ops/state/truth/sources.json` |

It is surfaced three ways: the `icn_ops_agent_runtime` MCP tool, the startup context emitted
by `session-lifecycle.sh` (a pointer, not a dump), and the file itself.

`scripts/check-agent-capabilities.py` regenerates the manifest and fails if it differs from the
committed copy. It runs in `agent-drift-check.yml` on **every** PR.

**Therefore:** adding a capability in its canonical location and regenerating makes it
discoverable to every launcher that reads the manifest, and *forgetting* to regenerate fails
CI. No launcher, provider adapter, or prompt is edited to add a capability.

Limits, stated plainly: the manifest describes capabilities to a session that reads it. No
provider outside Claude Code gets the startup line or automatic registration, because none of
them run Claude Code hooks — and the MCP route is narrower than it looks. Only `.cursor/mcp.json`
declares the ops MCP server today, so only Cursor can call `icn_ops_agent_runtime` and the
session tools (explicitly — it still cannot auto-register). `.codex/mcp/servers.example.json` is
an *example* file that does not declare the ops MCP at all, and its entries point at the retired
`~/projects/icn` path; `.opencode/opencode.json` has no MCP block. Those two get the generated
manifest as a file on disk and nothing else. `check-agent-runtime-adoption.py` prints this same
breakdown, so the claim is verified on every run rather than asserted here. See §1.1.

---

## 8. Lifecycle consumers

`icn-lane-audit` (icn#2653 — not merged, so it does not exist in this tree yet) and any future consumer should join on **canonical lane identity**
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
`ops/scripts/README-icn-wait.md` and pinned by regression tests in `scripts/test-icn-wait.sh`.

A blocking Bash-seam guard was implemented and then removed. It failed four rewrites in four
review rounds; its last false negative was the most natural phrasing of the loop, it refused
the canonical `ps aux | grep -v grep` idiom, and it exceeded its own hook timeout on large
inputs — failing open. Static analysis of arbitrary shell is not a solvable problem at this
seam, so this layer relies on the positive path instead: a supported primitive, named in the
startup context. The attempt is preserved on `ops/agent-wait-guard`.

Every wait is bounded. Indefinite blocking is available only behind an explicit flag.

## 10. Registry snapshot export

`ops/mcp/src/state/snapshot-export.ts` (CLI: `ops/mcp/src/cli/snapshot-export.ts`,
`npm run snapshot:export`) produces a **consistent, offline-readable copy of an existing session
registry**, so an external observer (Project Conductor is the first) can read session state
without opening the live database. The disclosure policy lives in one file,
`ops/mcp/src/state/snapshot-profile.ts`.

```
snapshot-export --profile <full|observation> --source <registry.db> --dest <snapshot.db>
snapshot-export --verify <snapshot.db>          # consumer-side completeness/integrity check
```

**Authority.** ICN keeps session identity, authorization and lifecycle (§2, §5). A snapshot is an
*observation* of that state at an instant. It is not a second registry, never flows back, and
confers no authority on whoever holds it. A consumer that treats it as live state is wrong by
construction: a snapshot is stale the moment it is taken.

### 10.1 Two profiles, two disclosure decisions

`--profile` is **required**; there is no default, so an omitted flag cannot make the disclosure
decision for the caller.

| | `full` | `observation` |
|---|---|---|
| Purpose | recovery and diagnostics **inside the ICN trust boundary** | least-privilege input for an **external** read-only observer |
| How made | one `VACUUM INTO` (byte-faithful) | an allowlisted projection **built** from one read transaction |
| Contains | everything, including mailbox bodies, event payloads, free-form agent text, transcript/worktree paths, health cache | `schema_version`, `sessions` (see below), `file_claims` |
| Transfer | do not send to an external consumer | intended for one |

The observation file is **built, not redacted**: rows are streamed from the source's read
transaction into a fresh file, so withheld data never reaches disk — not in the output, not in
free pages, not in a `.partial` left by a hard kill. Only allowlisted columns are ever `SELECT`ed.
Anything not named in the policy is not exported, and a test fails until every new column or table
is classified, so a later migration cannot widen the disclosure by accident.

A column that *is* copied is not trusted either: every copied value is checked against the column's
rule (identifier alphabet, timestamp shape, or integer) **before** it is passed to the `INSERT`. A
value that fails aborts the export as `SNAPSHOT_VERIFY_FAILED` while nothing unsafe has been
written, so the guarantee above holds for a completed file and for a `.partial` alike. The same
rules are re-checked on the finished file by the producer and by `--verify`.

**Field classification (observation profile)**

| Data | Observation | Why |
|---|---|---|
| `sessions`: `id`, `repo`, `worktree_name`, `worktree_generation`, `provider_session_id`, `parent_session_id`, `provider`, `agent_pid`, `host`, `started_at`, `last_heartbeat`, `last_progress`, `progress_count` | verbatim | session identity, lane association, activation/lifecycle, liveness. `host` is a *claim*; `agent_pid` is correlation only (§4) |
| `sessions.repo_id`, `sessions.worktree_id` | `sha256:<hex>` of the original | **these columns are filesystem paths** (the absolute git common dir and git dir). The digest is a stable join key without the layout. It is pseudonymisation, not secrecy |
| `sessions`: `worktree`, `worktree_path`, `branch_at_registration`, `head_at_registration`, `task_description`, `task_ref`, `pr_ref`, `transcript_path`, `current_activity` | `NULL` (withheld) | free-form agent text, absolute paths, and branch names (an unpushed branch name can disclose unreleased work) |
| `file_claims.file_path` | shown only if every `/`-separated segment matches `[A-Za-z0-9._@+=,-]+` (and none is `..`); otherwise `withheld:<16 hex>` | claims are agent-supplied free text. This is a **positive allowlist**: any spelling that merely looks relative (`file:///x`, `a:b/c`, `~/x`, backslashes, spaces, non-ASCII) is tokenised. Safe direction: a legitimate path with a space is tokenised, not leaked. The row is kept (dropping it would turn "claimed" into "not claimed"); the token is salted per export, so equal paths still collide *within* a snapshot |
| `mailbox`, `events`, `watchers_process`, `health_cache`, `decision_index` | **table absent** | message bodies and event payloads are free-form; `watchers_process.label` and `health_cache` hold command labels and service endpoints. Absent, not empty: an empty `mailbox` would read as "looked, found no messages" |

Reader rules that follow, and are also written into the sidecar's `disclosure` block: **a `NULL`
in a withheld column means WITHHELD** (never absent, none, or unknown-to-ICN), and **an absent
table is not evidence that it holds no rows**.

What the observation file still discloses, stated plainly: session ids, the provider conversation
id, claimed host names, lane names, process ids, timestamps, and repo-relative file paths of
claims. Repo-relative paths *are* the occupancy fact an observer wants, so they are not hidden.

### 10.2 What it does

- **Names its source.** `--source` is required. There is no env var, no git-derived default and
  no "current registry" — §2.6's resolver would silently pick the live shared database.
- **Never creates or migrates the source.** It checks the SQLite header *before* opening anything
  (SQLite treats an empty file as a fresh database), opens read-only with `fileMustExist`, and
  never goes through `initDb`.
- **Checks the registry is what its stamp says.** Exactly schema v`LATEST_SCHEMA_VERSION`; every
  column the contract depends on present (a version row is not a schema); and **only tables and
  indexes** — a view, trigger or virtual table is code the consumer would inherit, and is refused.
  `trusted_schema` is turned off on every connection.
- **Takes the snapshot in one read transaction** (§ above). In WAL mode that includes
  committed-but-uncheckpointed frames and excludes anything uncommitted; concurrent writers are
  neither blocked nor seen half-way. The online-backup API was rejected (a write between its
  steps restarts it); `immutable=1` is never used (false for a live registry).
- **Verifies the copy** (`integrity_check`, `foreign_key_check`, schema, `journal_mode = delete`
  so reading it creates no sidecars) and, for `observation`, the profile invariants and a
  row-count cross-check against counts taken *in the same source transaction*. Any failure
  discards the copy.
- **Refuses a source that changed under it.** The path must still name the same device/inode
  before the open and after the copy (`SOURCE_CHANGED`).
- **Is bounded.** Source main file **plus WAL** over `--max-source-bytes` (default 512 MiB), or too
  little free space on the destination filesystem, is refused before anything is written.
- **Publishes without overwriting.** Temp files (`.<name>.<pid>.<rand>.partial`, `0600`, created
  `O_EXCL`) live beside the destination; both final names are created with `link(2)`, which fails
  with `EEXIST` instead of replacing. The destination directory must already exist and is never
  created. The destination may not be the source or its `-wal`/`-shm`/`-journal`.
- **Cleans up.** A failure removes this attempt's temp files, and a published db whose sidecar
  could not be published is withdrawn.

### 10.3 Output pair and completeness

`<dest>` (SQLite) and `<dest>.meta.json` (`icn-ops-registry-snapshot-meta/v1`, `profile`-tagged).
The database is published first and the sidecar last, so a **hard kill between the two leaves a
database with no sidecar** (reproduced with a real `SIGKILL`). That file is *incomplete*, not
corrupt. Run `snapshot-export --verify <dest>` (library: `verifySnapshotPair`) before reading: it
requires the sidecar, a known format and profile, `disclosure.classification` and `disclosure.policy` that agree with that profile, a matching `sha256`/byte length/schema version/row counts, a
clean integrity and foreign-key check, and — for `observation` — the profile invariants: the exact
table set; **the exact column set of all three tables** (`schema_version`, `sessions`,
`file_claims`; generated columns included, so neither an extra nor a missing column passes, whatever
the sidecar says); **the exact definition of every table and index** (compared with what the
production migration ladder creates, because `sqlite_master.sql` is free text — an index
expression, a partial-index predicate or a column `DEFAULT` can carry data no row check sees);
no `sqlite_*` object other than the kept tables' autoindexes (`sqlite_stat1` rows are free text);
withheld columns `NULL`; digests well-formed; every copied value inside its column's rule
(identifier text, timestamp shape, integer); and no raw absolute path.

The definition check compares with the schema **this build** creates, so it is coupled to the
build: if a later ICN build changes the DDL text of the kept tables or their indexes without
changing the policy name, an older archived pair is refused as "definition differs", and the
policy name (`observation-v1`) must be bumped with such a change. A retry never
"repairs" a lone database; it is refused as `DEST_EXISTS`.

`verifySnapshotPair` always returns a verdict. A directory, FIFO, link or unreadable file in place of
either file is a problem in the verdict; it is decided by `lstat` *before* anything is opened, so a
FIFO cannot make it block.

**A consumer must check the declared profile.** A valid `full` pair is a valid pair: it verifies
`ok: true` with `profile: "full"` and contains mailbox bodies and transcript paths. A consumer of an
external observation therefore calls `verifySnapshotPair(dest, { expectProfile: "observation" })` or
`snapshot-export --verify <dest> --expect-profile observation`, which refuses any other profile.
The option is additive: omitting it keeps the earlier behaviour.

Stale `*.partial` files from a killed run are left in place (the pid in the name is not proof of
death) and are owner-only. An `observation` partial contains only values that already passed the
disclosure rules (§10.2), because they are checked before they are written; a `full` partial holds a
full-fidelity copy. A hard kill still leaves a file that is not a snapshot: it has no final name and
no sidecar.

The sidecar keeps these apart, on purpose:

| Block | Meaning |
|---|---|
| `source` | which database was read. `full`: path, realpath, dev/ino. `observation`: no path, device, inode **or derived identity** — a hash of guessable filesystem metadata is a confirmable fingerprint, so none is provided |
| `acquisition` | method, start/finish time, busy timeout |
| `snapshot` | the published file: bytes, sha256, page size/count, schema version |
| `consistency` | checks run on the **copy**, with per-table row counts. Internal consistency only |
| `disclosure` | profile classification and, for `observation`, exactly what was included, digested and withheld |
| `host` | `claimed.hostname` is self-asserted; `authenticated` is `null` and `authenticity` is `"none"` |
| `producer` | exporter name/version, node, better-sqlite3, SQLite version, supported schema |

The hash is **integrity** evidence: anyone who can write both files can write a matching pair.
Nothing here is signed, and nothing authenticates the producing host.

**Why `source.size_bytes_at_start` and `source.mtime_ms_at_start` stay in the `observation`
sidecar.** They are recency evidence for the reader (how large the registry was and when it was last
written when it was read), cited by the staleness row of §10.4. They are not an identity: no path,
device or inode accompanies them, both change on every write, and §10.7 R1 still says sameness is
never derivable from an export. A reader who already holds the registry file can `stat` it, but then
they can read it; the values tell a reader without the file nothing it could confirm. They are
informational and not covered by the database hash. If a deployment does not want them, the
decision is to drop the fields in a new sidecar version, not to replace them with an identifier.

### 10.4 Consumer compatibility (Project Conductor)

Conductor's reader contract (`conductor-icn-observation/v1`, `icn_snapshot.py`, `icn_contract.py`,
`docs/integration/icn-read-only-v1.md`) lives in a separate repository that was **not available
to the author of this section**. Nothing in this tree references it; ICN contains no Conductor
fixtures. Every row below marked UNVERIFIED is therefore a statement about what ICN produces,
not about what Conductor accepts.

| Requirement | ICN side | Conductor side |
|---|---|---|
| Schema version | exactly v5; any other is refused, never migrated | UNVERIFIED (reported to use synthetic v5 fixtures; not inspected) |
| Table names | `observation`: `schema_version`, `sessions`, `file_claims`. `full`: also `events`, `mailbox`, `watchers_process`, `health_cache`, `decision_index` | UNVERIFIED — does the reader require any table the observation profile omits? |
| Required columns | all 24 `sessions` columns exist in both profiles; withheld ones are `NULL` in `observation` | UNVERIFIED — does it read a withheld column, and does it treat `NULL` as absent? |
| Identity values | `repo_id`/`worktree_id` are `sha256:<hex>` in `observation` | **DECISION NEEDED** — if Conductor correlates to ICN's live tooling by raw `worktree_id`, the digest breaks that join |
| Completeness marker | `<dest>.meta.json` published last; `--verify` | UNVERIFIED — marker name, and whether it expects one at all |
| Sidecar format / checksum | ICN-defined `icn-ops-registry-snapshot-meta/v1`; whole-file sha256 | UNVERIFIED — field names and hash scope are ICN choices pending the contract |
| Missing / unavailable source | typed refusal (`SOURCE_MISSING`, `SOURCE_LOCKED`, …), exit 1, nothing written | UNVERIFIED mapping to UNKNOWN |
| Stale evidence | `acquisition.finished_at`; `source.mtime_ms_at_start` (informational, not snapshot-bound); per-row `last_heartbeat` | UNVERIFIED — staleness threshold and which timestamp it trusts |
| Privacy boundary | **VERIFIED** ICN-side by raw-byte scans (`registry-snapshot-observation.test.ts`) | UNVERIFIED — is the allowlist sufficient, or too wide? |
| Authenticity vs claimed host | `host.authenticated = null`; no attestation | UNVERIFIED — must not present `host.claimed` as identity |
| Version / migration | `producer.supported_schema_version`; future schema refused | UNVERIFIED — behaviour on a v6 snapshot |
| Snapshot consistency under writers | **VERIFIED** ICN-side (§10.2 tests, separate-process writer) | UNVERIFIED — reported acquisition tests not inspected |

**Reported Conductor-side findings** (relayed by the Conductor team; *not* inspected here): its
legacy reader matches lanes on raw absolute git-dir paths, does not recognise the `.meta.json`
marker, and its envelope requires raw path identities, so against an `observation` snapshot it can
report `available` with zero sessions. That is a Conductor fail-closed defect, handled there. The
planned separate versioned reader (explicit profile, mandatory marker, exact policy match,
digest-only lane lookup, conversation correlation, UNKNOWN for unmapped relations, fail-closed
validation) asks for nothing ICN does not already provide: see the golden fixture below.

**Open questions for the Conductor owner** (answers are small and contain no registry data; do
not send real session ids or a real registry):

1. The full text of `docs/integration/icn-read-only-v1.md`.
2. The exact SQL (or column list) the reader runs against `sessions`, `file_claims` and
   `schema_version`; and every other table or `PRAGMA` it touches.
3. For each column it reads: required or optional, and what it does with `NULL`.
4. Which column is the ICN/Conductor session correlation key (`id`, `provider_session_id`, a
   lane id?) and whether it compares `worktree_id` to a raw git directory path.
5. How it recognises a *complete* snapshot: a sidecar? its name, format and hash scope?
6. The output of `.schema` and `PRAGMA user_version;` on one synthetic fixture database (schema
   only, no rows).
7. How it maps "source missing / locked / older schema / newer schema" to its UNKNOWN state.

### 10.5 Limits — stated, not hidden

1. **Reading a WAL source can leave `<src>-wal` / `<src>-shm` beside it.** A read-only connection
   still attaches the wal-index and cannot remove it on close. Locking metadata, not registry
   content (the main file's bytes and logical content are unchanged — pinned by a test), but not
   "touches nothing on disk". If the directory is not writable and no `-shm` exists the open fails
   as `SOURCE_UNREADABLE`.
2. **Source identity is checked by name, not by descriptor.** better-sqlite3 exposes no fd, so
   there is a window between the identity check and the open. It is narrowed (checked before the
   open and after the copy, fail-closed) but not eliminated; an attacker who can rotate the
   registry file can already do worse.
3. **`link(2)` needs a filesystem that supports hard links** in the destination directory; where
   it does not, publication fails closed (`PUBLISH_FAILED`) rather than falling back to a
   replacing rename.
4. **Hard kill leaves debris.** `*.partial` files (owner-only) and, between the two publishes, a
   lone database. Both are detected; neither is auto-cleaned.
5. **The export blocks the process** for its duration (SQLite's copy is synchronous). Fine for a
   CLI; do not call it from the MCP server's event loop on a large registry.
6. **Pseudonymised identity is guessable.** `sha256:` of a path someone can enumerate is
   confirmable by that someone. It removes layout from the file; it is not a secret.
7. **No authenticity.** See §10.3. The pair check also validates only what SQLite exposes (schema
   text and rows). It cannot attest the unreferenced bytes of a file forged by someone who writes
   both files (free pages, page slack). The producer builds a fresh file and so leaves none; a
   consumer that must not trust the producer cannot rely on this check alone.
8. **An unsafe value in a copied column refuses the whole `observation` export.** `host`, `repo`,
   `worktree_name` and the other verbatim text columns must match `[\p{L}\p{N}._:@+ -]{1,256}` — in
   particular contain no `/`, so they can never be a path. The time columns (`started_at`,
   `last_heartbeat`, `last_progress`, `file_claims.claimed_at`, `schema_version.applied_at`) must be
   timestamps (`YYYY-MM-DD HH:MM:SS`, or ISO-8601 with `T`, optional fraction and `Z`); `agent_pid`,
   `progress_count` and `schema_version.version` must be numbers. Otherwise the export fails as
   `SNAPSHOT_VERIFY_FAILED` (naming the column, never the value) — before the value is written —
   rather than disclosing it or silently altering it. The remedy is to correct the registry row, or
   to use `full` inside the trust boundary. The text rule is an *alphabet*, not a grammar: it keeps
   out paths, URLs and punctuation, not words, and is sound because these columns hold identifiers
   by production contract (UUIDs, hostnames, repo and lane names). `file_claims.file_path` is
   handled separately (§10.2): shown only if repo-relative, otherwise tokenised.
9. **Fixture-verified only.** Tested against throwaway databases, including a separate process
   committing multi-table transactions and a real `SIGKILL`. It has not been run against the live
   shared registry.

### 10.6 Golden observation fixture

`ops/mcp/src/tests/fixtures/observation-golden/` holds a **fully synthetic** observation snapshot
produced by the real exporter from a WAL-backed fictional registry, so a consumer can write
compatibility tests with no ICN data and no connection to ICN. Generate with
`node --experimental-strip-types --no-warnings --import ./src/tests/fixtures/ts-resolve-register.mjs src/tests/fixtures/observation-golden/generate.ts <empty-dir>`
(from `ops/mcp`, Node >= 22.6, no build, nothing written under `dist`):

| Path | Role |
|---|---|
| `reference/contract.json` | machine-readable producer contract, **derived** from source and from the generated fixture (types, header, field paths, limits, error codes, reader obligations) — committed |
| `reference/test-vectors.json` | digest, token-formula, path-disclosure and verbatim-text vectors, plus structural fixture facts — committed |
| `HANDOFF.md` | self-contained description for the consumer's session |
| `generate.ts`, `contract.ts` | the generator and the derivation |

The database and its sidecar are **generated, not committed**: the repository ignores `*.db`
(`.gitignore`), no database is tracked anywhere in the tree, and a committed sidecar whose
`sha256` named an absent file would mislead. `observation-golden-fixture.test.ts` generates the
pair in a temp directory on every run and compares it with `reference/`. Committing the database
would need an explicit decision to carve an exception to that rule; it was not made here.

The fixture exercises: several activations in one lane; a second lane that must not be selected; a
present, a `NULL` and a pre-identity conversation id; a present and a missing worktree generation;
a cross-lane conflict on a disclosable path and on a withheld path; a path that is shown and paths
that are withheld; and every withheld field, table and path carrying a synthetic canary that tests
prove is absent from every byte of the database and sidecar.

**Reproducibility, stated exactly.** `contract.json` and `test-vectors.json` regenerate
byte-identically. The database and sidecar do **not**: each export draws a fresh random salt for
withheld-path tokens (deliberately — tokens must not be recomputable) and the sidecar records
generation-time fields and tool versions. The pair is still internally consistent, and its logical
content, with token values replaced by first-appearance labels, is identical on every run; tests
regenerate in a child process and compare on exactly that basis. Regenerating is only needed when
the exporter's contract changes.

**What can be recomputed without private inputs:** the `sha256:` digest of any raw value you hold;
the token formula given a published test salt. **What cannot:** the fixture's own claim tokens, and
anything about a real ICN host. Digests are unkeyed — confirmable by anyone who can enumerate the
raw value — and are never to be described as secret, authenticated or unguessable.

**Note on `repo_id`.** It is nullable in the registry schema (added in v3 without `NOT NULL`);
`repo` is the non-null column. The export preserves `NULL` as `NULL` and otherwise always emits a
well-formed digest; it never invents a value and never emits the original path.

### 10.7 Continuity across exports (what ICN does over time)

`ops/mcp/src/tests/fixtures/observation-series/` holds a series of five **real consecutive exporter
outputs** (registry A x4, registry B x1) built by driving ICN's own `registerSession`,
`recordHeartbeat`, `recordProgress` and `releaseSession`, plus an offline verifier and a
deterministic package builder. Same producer contract as §10.6; only *time* is new. Nothing is
edited after export. What is controlled (a virtual `Date`, re-stamped touched rows, a counter for
`randomUUID`, fictional hosts, a mirrored claim rule) is listed in its `HANDOFF.md`, and the mirror
is pinned to the real `claim_files` handler by `observation-series-semantics.test.ts`.

Answers, from executable source and the tests that pin them:

| Question | Answer |
|---|---|
| May `provider`, `provider_session_id`, `host`, `parent_session_id`, `repo`, `started_at` change for an existing session id? | **No.** Set only in the `INSERT` (`session-runtime.ts` register). A resume leaves them untouched. |
| May the lane (`repo_id`, `worktree_id`, `worktree_name`) or `worktree_generation` change for an existing id? | **Yes, only** through `registerSession` re-registering a **known `provider_session_id`**: a *lane move* overwrites the lane columns **and** the generation together; a lane recreated at the same path overwrites **only** the generation (to `NULL` if it cannot be minted). A session with no conversation id never dedupes, so its lane never changes. |
| Does release/retire delete the row, change a state, or keep history? | `releaseSession` **deletes the row and its claims**. There is **no lifecycle-state column** and no "retire" in the registry. Stale rows are **never** reaped. History lives only in `ops/state/session-log.jsonl`, outside the registry and every export. A re-registered conversation becomes a **new** activation. |
| Is the acquisition start time monotonic? | **No.** `acquisition.started_at` is a sampled `new Date()`; nothing compares it with a prior export. Row stamps are SQLite `datetime('now')` at one-second resolution. Only `progress_count` is monotonic, and only within one row. |
| Is there a stable registry-instance identifier? | **No.** No registry id, `user_version` 0, `application_id` 0. The sidecar carries no source identity. The only registry-scoped value is `schema_version.applied_at`: differing stamps prove "different registries"; equal stamps prove nothing (a restored copy keeps them). |

Consequences a reader must honour (machine-readable as `rules` in `reference/series-contract.json`):
sameness of registry is asserted out of band, never derived; a later `started_at` is not
authenticated freshness and may even regress (the series includes an unchanged re-export taken
with the clock stepped back); `NULL` identities are unknown and never equal; claim tokens are
per-export and must never be compared across snapshots; **absence of a row is not evidence that a
session ended**, and a stale row is not an ended session; the TTL (default 30 min,
`ICN_SESSION_TTL_MINUTES`, minimum 5) is not in the snapshot, so staleness is UNKNOWN without an
agreed value. Two holders of one file can coexist when the first was stale at claim time.

`python3 -I verify_series.py` (standard library) recomputes every per-export fact and every
pairwise verdict from the databases and compares them with the contract, so the producer-side
model is not its own witness.

