# ICN Development Control Center (`icn-control`)

A keyboard-driven terminal dashboard for the ICN development host. It answers one question:
**what are all the agents and development lanes doing right now, and what needs me?**

It is an *observer*. It reads existing owners and reconciles them. It does not become a
project-status database, an agent registry or a supervisor, and it never starts, stops,
deletes or merges anything. The one action that touches anything is a confirmed
`tmux attach` / `switch-client` to a session that already exists.

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

## Screens and keys

| Key | Screen | What it shows |
|---|---|---|
| `1` | Overview | live agent sessions, what needs attention, a rule-based "what next", PRs by state, blocked work, merges in the last 7 days, infrastructure warnings |
| `2` | Lanes | the seven long-lived programs, with their open PRs, sessions, worktrees, handoffs and next action |
| `3` | Agents & Sessions | every agent process, tmux session, launcher record and registry entry, reconciled into one row per session |
| `4` | Worktrees | every git worktree with dirty/unpushed state, locks, PRs and why it must be kept (`k`: only must-keep) |
| `5` | Pull Requests | open PRs across org repos: draft, lifecycle, required CI, reviews, ledger, readiness |
| `6` | Handoffs | recent handoff documents with their completed / open / decision / next sections |
| `7` | System Health | disks, memory, load, disk-guard report, build reservations, running builds, MCP updater |

`↑/↓` select · `Enter` full details · `o` open the record (handoff, design packet, review body) ·
`a` attach to the selected session's tmux (asks first) · `/` filter · `r` refresh now ·
`s` data sources and ages · `?` help · `q` quit. The help screen shows *your* tmux prefix key
(read from tmux, not assumed).

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

Covers process matching (pid reuse), registry read-only access, missing/unreadable sources,
stale cache labelling, GitHub reconciliation (lifecycle, automated vs human review, required
checks, readiness), session reconciliation, worktree ownership verdicts, program classification,
file-viewer containment and launcher safety against an isolated tmux server.

## Known limits

- Codex app-server/proxy processes are shown as services; the remote threads they host are not observable here.
- The task ↔ session link is inferred (branch, directory name, handoff text) unless a session registered
  `pr_ref`/`task_ref` in the icn-ops registry. An explicit registration always wins.
- Program grouping is rule-based; every item shows the rule that placed it.
- Required checks for repos whose branch protection is not readable show as UNKNOWN.
