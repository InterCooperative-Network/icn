# ICN Development Control Center (`icn-control`)

A keyboard-driven terminal dashboard for the ICN development host. It answers one question:
**what are all the agents and development lanes doing right now, and what needs me?**

It is an *observer and navigator*. It reads existing owners and reconciles them. It does not
become a project-status database, an agent registry or a supervisor, and it never stops, deletes
or merges anything. It acts in only two ways, both explicit:

- **Join**: switch your own terminal to an existing tmux session (nothing is started or typed).
- **Resume / Open**: after you confirm, ask the host launcher `icn-session` to start Claude Code
  for an exact recorded conversation, or in an existing worktree (never a new branch or worktree).

## Open it

On the development host, from any directory:

```bash
~/icn-dev/bin/icn-control            # opens (or re-attaches to) the persistent 'icn-control' tmux session
~/icn-dev/bin/icn-control --report   # plain-text report, no UI
~/icn-dev/bin/icn-control --here     # run in this terminal without tmux
~/icn-dev/bin/icn-control --status   # is it running?
~/icn-dev/bin/icn-control --restart  # stop only the control center's own session and reopen (after an upgrade)
```

From a workstation over SSH (the `-t` is required for an interactive screen):

```bash
ssh -t <dev-host> ~/icn-dev/bin/icn-control
```

Because the UI lives in a tmux session, closing the terminal or losing SSH leaves it running.
Running the same command again re-attaches; it never starts a second copy. After a reboot the
tmux session is gone, and the same command recreates it. The first screen then reads live state
and marks anything loaded from the local cache as **CACHED/STALE** until it is re-observed.

## Work: one row per piece of work (the default screen)

Screen **1 Work** groups everything that belongs to one piece of work into one **activity**: its open
PR, its worktrees (author and review checkouts), its Claude/Codex sessions, launcher and registry
records, review-evidence directories, handoffs and follow-up ledger. Links are made from evidence
only, and each activity lists them ("How these were linked"):

- a worktree whose branch is the head of an open PR in the same repository (fork PRs never match);
- a session whose working directory is inside a worktree;
- a session tied to a PR by a registered `pr_ref`, a branch match, or an explicit PR number;
- a review checkout that names an open PR (`review-pr-2905`, `review/pr-2905`).

Similar-looking names alone never merge two activities.

| Status | Meaning (derived on every refresh, never stored) |
|---|---|
| **Active** | a live agent, a maintainer decision or block on its PR, or PR movement in the last 24 h |
| **Paused** | unfinished and preserved: an open PR without recent movement, uncommitted work, a resumable conversation, or a fresh worktree |
| **Completed** | its branch's PR was merged (and nothing uncommitted is left behind) |
| **Archived** | superseded (PR closed unmerged) or a clean checkout with nothing unique and no PR |
| **Unresolved** | not enough evidence: e.g. commits on no remote-tracking ref and no PR, merged but still dirty, PR history unavailable, or the open-PR list of the worktree's repository not observed |

Views (`v` cycles): **NOW** — Active, anything that needs you (`!`), and Paused work touched in the
last 30 days (the default) · **ALL OPEN** — every Active and Paused activity · **ARCHIVE** —
Completed, Archived and Unresolved, searchable with `/`.

`Enter` on an activity offers every action, and says why any is unavailable: join its live Claude
session, resume its saved conversation, open Claude Code in its worktree, read its handoff, show
details, mark it paused/archived, request retirement approval.

- **Marks** (`m`) are a display-only overlay in `~/.local/state/icn-control/activity-marks.json`
  (0600, `canonical: false`). They move an activity between views. They never delete, stop or clean
  anything, never remove a protection, and a live agent always keeps its activity Active.
- **Retirement requests** (`x`) are offered only for Completed/Archived activities with no protection
  (no uncommitted changes, no commits on no remote ref, no open PR, no live agent, no file locks, no
  resumable launcher conversation, and the repository's open-PR list observed live and completely: a
  failed, truncated, missing or only-cached list is "open-PR state unknown", never "no PR").
  They append a reviewable entry with the `wt-clean` dry-run and `--apply --only <name>@<sha>`
  commands to `~/icn-dev/state/dev-control-consolidation-<date>/retirement-requests.md`. Nothing runs.

The PR history needed to tell finished from unfinished worktrees (merged and closed PRs by branch) is
fetched hourly and cached; until it loads, those worktrees show as Unresolved, never as Completed.

## Moving between Claude sessions

| You want to… | Do this |
|---|---|
| see the Claude sessions of current work and pick one | press **`g`** anywhere; type part of a name, task, program, repo or PR to filter; `↑/↓`; `Enter`. `Tab` shows every session record. |
| enter a running session | `g` → select it → **Join session** (or Work → `Enter` on the task → Join, or `a` on screen 4) |
| come back to the dashboard | **tmux prefix, then capital `L`** (the prefix on icn-dev is Ctrl-a). Claude keeps running. |
| continue a stopped conversation | `g` → a row marked `STOPPED · resumable` → **Resume saved conversation** → read the plan → `y` |
| start Claude in an existing worktree | Work (1), Lanes (3) or Worktrees (5) → **`c`** → read the plan → `y` |
| a dedicated "back" key | opt in once: `icn-control --enable-return-key` makes **F12** jump back (see below) |

What each action guarantees:

- **Join** re-checks, at the moment you press it, that the tmux session still exists, that the agent
  process is the same one (pid *and* start time), that it is still inside that tmux session and still
  on the same conversation. It moves only your terminal (`switch-client -c <your client>`): no
  process is started, no keys are sent to Claude, nobody else is disconnected. The dashboard keeps
  running, with its selection and scroll position, while you are away.
- **Resume saved conversation** is offered only for an `icn-session` launcher record whose exact
  conversation id has a transcript *for that worktree*. It is refused when that conversation, or any
  agent in that worktree, is already running. Registry-only conversations are not started from here;
  the detail screen shows the manual command instead. The conversation id is never guessed.
- **Open Claude Code in this worktree** is refused for missing, protected (`icn-session check-path`),
  review / read-only / detached checkouts, and worktrees that already have a live agent. If a launcher
  record exists for that worktree, the plan says that its conversation will be resumed.
- Both run `icn-session open <purpose> --repo … --worktree … --name … --no-create --no-attach`
  (argv only, no shell; every value validated), show you the exact command first, re-check
  everything again after you press `y`, and only then switch you to the new session.

The switcher's STATE column says exactly what each entry is and what can be done with it:
`RUNNING · busy|waiting` (joinable), `RUNNING · … · outside tmux (/dev/pts/N)` (use that terminal),
`STOPPED · resumable`, `STOPPED · no launcher record` (registry-only: a quoted manual
`claude --resume` command is shown, nothing is started), `STOPPED · transcript unavailable`,
`OUTDATED record · agent live here` (folded into the live agent by default), `STALE registry entry`.
By default it lists every live agent plus the session records of Active and Paused work.

### Optional F12 "back" key

`tmux prefix + L` works out of the box. If you prefer one key:

```bash
~/icn-dev/bin/icn-control --return-key-status    # free / already ours / taken by something else
~/icn-dev/bin/icn-control --enable-return-key    # opt in (refuses if F12 is already bound)
~/icn-dev/bin/icn-control --disable-return-key   # removes it (only if it is ours)
```

It binds F12 in the running tmux server only (not your `~/.tmux.conf`), so it lasts until tmux
restarts. While enabled, programs inside tmux, Claude Code included, no longer receive F12.

## Screens and keys

| Key | Screen | What it shows |
|---|---|---|
| `1` | Work | activities of current work (default), all open work, or the searchable archive |
| `2` | Overview | live agent sessions, what needs attention, a rule-based "what next", PRs by state, blocked work, merges in the last 7 days, infrastructure warnings |
| `3` | Lanes | the seven long-lived programs, with their open PRs, sessions, worktrees, handoffs and next action |
| `4` | Agents & Sessions | every agent process, tmux session, launcher record and registry entry, reconciled into one row per session |
| `5` | Worktrees | every git worktree with dirty/unpushed state, locks, PRs and why it must be kept (`k`: only must-keep) |
| `6` | Pull Requests | open PRs across org repos: draft, lifecycle, required CI, reviews, ledger, readiness |
| `7` | Handoffs | recent handoff documents with their completed / open / decision / next sections |
| `8` | System Health | disks, memory, load, disk-guard report, build reservations, running builds, MCP updater |

**`g` session switcher** (anywhere) · `↑/↓` select · `Enter` details, or the action menu on screen 3 ·
`a` join (screen 4) · `c` open Claude in a worktree (screens 1, 3 and 5) · `v` Work view · `m` mark · `x` retirement request · `o` open the record (handoff,
design packet, review body) · `/` filter · `r` refresh now · `s` data sources and ages · `?` help ·
`q` quit (inside tmux it asks first; `prefix d` leaves it running instead). The help screen shows
*your* tmux prefix key (read from tmux, not assumed).

## Status words

`VERIFIED LIVE` (process observed now, pid **and** start time checked) · `IN PROGRESS` ·
`IDLE` · `AWAITING MAINTAINER` · `BLOCKED` · `REGISTERED, NOT RUNNING` · `STALE` ·
`WARNING` (host thresholds) · `UNKNOWN`. A source that cannot be read shows **UNKNOWN**,
never an empty success. A session is never called active because its worktree still exists.

## Where the data comes from

| Data | Owner read (read-only) |
|---|---|
| Running agents | `/proc` (pid + start time), Claude Code's `~/.claude/sessions/<pid>.json` (JSON files only), transcripts held open by the process |
| tmux | `tmux list-sessions` / `list-panes` |
| Session registry | `<repo>.git/icn-ops.db`, opened `mode=ro` (no migration, no writes) |
| Launcher records | `icn-session` state records |
| Worktrees | `git worktree list` + `git --no-optional-locks status` per worktree (no fetch, never takes `index.lock`) |
| Task text, file locks | `~/icn-dev/state/active-worktrees.yaml`, `file-locks.yaml` (shown as historical claims) |
| PRs, reviews, CI | `gh api graphql` (authenticated `gh`), required checks from live branch protection, else `ops/state/truth/policy.json` |
| Merge readiness gates | `merge.ready_when` in `ops/state/truth/policy.json` (allowed merge states and review decisions, applied as given; unreadable → NOT READY) |
| Delivery lifecycle | the `ICN-DELIVERY-LIFECYCLE` block in each PR body (semantics: `ops/state/truth/delivery.json`) |
| Host health | `statvfs`, `/proc/meminfo`, `/proc/loadavg`, the disk guard's last report, build-admission reservations |

Refresh intervals: local state every 20 s, host records every 60 s, git and GitHub every 5 min, `r` for all now.
A failed refresh keeps the last good data visible, marked stale, with the error.

Non-authoritative local state lives in `~/.local/state/icn-control/` (0700): UI preferences and
a cache of the last GitHub/git observation for fast startup. `programs.json` is a curated
navigation catalog (descriptions and matching rules), explicitly `canonical: false`.

## Install / upgrade

```bash
ops/dev-control/install.sh            # from an icn worktree; installs into ~/icn-dev
~/icn-dev/bin/icn-control --restart   # pick up the new code if it was running
```

The installer copies an immutable release to `~/icn-dev/share/icn-control/releases/<commit>`,
points `current` at it and installs the launcher. The installed copy keeps working after the
source worktree is cleaned up.

## Tests

```bash
cd ops/dev-control && python3 -m unittest discover -s tests -v
```

`test_activities.py` covers activity consolidation: grouping by evidence (and never by similar
names), every classification rule, the NOW/OPEN/ARCHIVE views, display-only marks (a live agent
always wins, protections are never removed), duplicate session records folded into one activity and
one switcher row, retirement requests that run nothing, and the recovery labels.

`test_sessions.py` adds the session manager: join verification (stale targets, pid reuse, agent no
longer in the target, changed conversation), only-our-client switching, resume/open planning and
refusals, injection-safe argv, picker ordering and filtering, and an end-to-end run of the real UI
in an isolated tmux server with a real attached client: switch → return with prefix+L (agent still
running) → switch to another session → cancelled resume (nothing started) → confirmed resume →
no second resume, plus a stale target, resizing and the quit confirmation.

`test_release_blockers.py` covers the three FULL-review blockers: readiness gated by the policy's
merge-state and review-decision allowlists (exhaustively over every gate), per-repository open-PR
coverage (partial, failed, truncated and cached fetches protect their worktrees; a successful empty
list does not), and resumable launcher conversations as a retirement protection in every end state.

`test_control.py` covers process matching (pid reuse), registry read-only access, missing/unreadable sources,
stale cache labelling, GitHub reconciliation (lifecycle, automated vs human review, required
checks, readiness), session reconciliation, worktree ownership verdicts, program classification,
file-viewer containment and launcher safety against an isolated tmux server.

## Known limits

- Codex app-server/proxy processes are shown as services; the remote threads they host are not observable here.
- The task ↔ session link is inferred (branch, directory name, handoff text) unless a session registered
  `pr_ref`/`task_ref` in the icn-ops registry. An explicit registration always wins.
- Program grouping is rule-based; every item shows the rule that placed it.
- Required checks for repos whose branch protection is not readable show as UNKNOWN.
