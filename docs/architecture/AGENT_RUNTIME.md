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

---

## 10. Session checkpoints — state ICN owns

### 10.1 The failure that motivated this

A session finished successfully and its harness's own export malfunctioned. Recovery meant
locating that harness's private transcript file by hand and reconstructing a handoff separately.
Nothing was lost, but the incident named a real property of the current arrangement: a session's
reconstructable state existed **only** inside a specific vendor's tooling, in a format that
vendor owns, reachable only through a command that vendor implements.

That is a portability failure rather than a harness defect. It would have looked the same in any
other agent product.

### 10.2 The principle

> ICN's canonical development state, authority, evidence and workflow must not depend on the
> continued availability or proprietary behaviour of a specific model provider or agent harness.

Operational corollary:

> A proprietary harness feature may accelerate development. State produced through it that the
> project depends on must also have a representation ICN can read without that harness.

Stated as a direction, not a status. **ICN is not harness-independent today.** §7 already records
how narrow the non-Claude surface is: only Cursor declares the ops MCP server, no other provider
gets automatic registration, and the Claude Code hook seam is the only launcher that registers a
session at all. The rule this section adds is about *how the gap closes*: when a proprietary
harness owns a strategically important function, or causes an actual portability failure, that
responsibility is extracted below the vendor boundary **one bounded feature at a time** — not by
designing a replacement for the harness.

Checkpointing is the first such extraction. It is one tooth of a ratchet, not a plan.

### 10.3 `ops/scripts/icn-session-checkpoint`

```
icn-session-checkpoint create --out DIR [--handoff F] [--transcript F]
                              [--provider NAME] [--ref R]... [--note TEXT]...
icn-session-checkpoint verify DIR
```

It writes `manifest.json` plus an `artifacts/` directory, and is a helper capability like any
other — declared with a `#: capability:` header, discovered by §7's generator, so no launcher or
prompt is edited to make it reachable.

**The manifest keeps three kinds of fact apart**, because a consumer that cannot tell a
re-derivable fact from a stale one will act on the stale one:

| Block | What it holds | How a consumer must treat it |
|---|---|---|
| `observed` | repository, worktree, branch, HEAD, base, ahead/behind, dirty state, changed files, recent commits — derived here from Git | re-derivable, and cheap to re-derive |
| `captured` | the branch's pull request and its check buckets, as GitHub reported them at `created_at` | historical the moment it was written; requery before acting |
| `generated` | a handoff file and notes supplied by a person or agent | not derived from anything here, and not verified by anything here |

This is the §2.5 field classification applied to a different artefact, and for the same reason:
the runtime stores the **reference**, never the state.

Three properties are deliberate:

* **It works with no transcript.** A provider transcript is optional. When one is supplied it is
  copied into `artifacts/` and hashed, and is **never parsed** — vendor evidence, not a
  dependency. A Codex or local-model adapter attaches its own stream on identical terms.
* **Artifacts are copied, not referenced.** A checkpoint pointing into a harness's private
  directory would reproduce the dependency the format removes.
* **It invents nothing.** Where a fact cannot be resolved — no PR for the branch, several PRs for
  one branch, `gh` absent — the manifest records `resolved: null` with the reason. Narrative is
  carried through from `--handoff`/`--note` and is never synthesised.

`verify` re-hashes every artifact and **fails** on a mismatch or an unknown schema, so a
checkpoint stays checkable after being copied off this machine.

**Integrity, not attestation.** SHA-256 content hashes only. ICN has signing primitives, and
reaching for them here would produce a weak ceremony rather than a real guarantee — the signer
would be an agent process with no standing to attest anything. Stronger attestation is a later
question, to be answered when there is a principal whose signature would mean something.

**What a checkpoint is not.** It is not authority. Branch, PR, issue and CI state have owners
named in `ops/state/truth/sources.json`, and those owners are authoritative. A checkpoint is
memory and evidence, on exactly the terms `.agents/skills/handoff` already states for a handoff.

### 10.4 Deliberately not built

A normalized session **event stream** — `session.started`, `context.loaded`, `tool.called`,
`file.changed`, `decision.recorded`, `test.completed`, `git.commit`, `pr.created`,
`checkpoint.created` — is the obvious next primitive, and the ops MCP already has an events table
(`ops/mcp/src/state/events.ts`) that a normalized stream would extend rather than replace. It is
recorded here as a design note and nothing more. Building it now would be designing the whole
future instead of adding the next tooth.

Also not built, and not implied: an agent orchestrator, a model router, a UI, or any change to
MCP. Model adapters remain vendor-specific; the checkpoint format and its exporter do not.
