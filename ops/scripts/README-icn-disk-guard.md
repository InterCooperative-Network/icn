# icn-dev disk guard

Bounds Cargo build-artifact growth on the `icn-dev` development VM.

## The incident this exists to prevent

Every agent worktree under `~/icn-dev/worktrees/<repo>/<name>` builds into its own Cargo
`target/`. Each is 3–26 GB, and a full workspace build across feature sets can exceed 100 GB.
Nothing ever reclaimed them. On **2026-08-23** the root filesystem reached **96% (20 GB free)**
with **372 GB of build output across 19 worktrees** — one worktree alone held 119 GB.

## Files

| File | Purpose |
|---|---|
| `icn-disk-guard` | The guard. All policy lives in one CONFIG block at the top. |
| `icn-disk-guard.service` | System-level oneshot, runs as the dev user. |
| `icn-disk-guard.timer` | Boot-persistent, every 4 h with randomised delay. |
| `install-icn-disk-guard.sh` | Idempotent installer; refuses to schedule a guard failing its own tests. |

```bash
ops/scripts/install-icn-disk-guard.sh            # install or update
ops/scripts/install-icn-disk-guard.sh --check    # report state, change nothing
icn-disk-guard                                    # audit; dry-run, never deletes
icn-disk-guard --self-test                        # 40 policy assertions
```

## Policy

Three layers, because filesystem percentage is a *lagging* signal — one worktree reached
119 GB on its own, so this disk can go from comfortable to critical inside a single session.

1. **Routine housekeeping** — runs whatever the filesystem looks like. Reclaims Cargo output
   that is `MERGED-INACTIVE`, unpinned, clean, **≥ 7 days idle and ≥ 5 GB**. This is what
   actually bounds growth.
2. **Pressure housekeeping** — above **78%** the age/size floors drop to 2 days / 1 GB. It
   never relaxes the identity, liveness or git-safety checks; only how stale and large the
   output must be. Critical at **90%**.
3. **Budgets** — reports on the resource itself: total footprint (warn **150 GB**, crit
   **200 GB**) and largest single target (warn **50 GB**, crit **100 GB**). These raise the
   exit code, so a scheduled run is loud *before* the filesystem is endangered.

The 5 GB routine floor is chosen from the observed distribution: across 19 measured targets
the median is 7.2 GB. A 10 GB floor sits above the median and would cover 6/19 targets
(308 GB); 5 GB covers 13/19 (355 GB). The incident was caused as much by many mid-sized
targets as by the two >100 GB outliers, and a routine clean's safety rests on the
`MERGED-INACTIVE` classification, not on the size floor.

## What it will not do

Removes only directories named `target/` that carry a Cargo `CACHEDIR.TAG`, and only from
worktrees positively classified `MERGED-INACTIVE`. Never deletes source, git metadata,
branches or worktrees; never prunes Docker or package caches; never mutates git.

**Fails safe in every unknown.** If `gh` is missing, unauthenticated or rate-limited, PR state
is unknown, the worktree classifies `STALE-UNKNOWN`, and nothing is removed.

**Never terminates a process.** A merged worktree still pinned by a live process is reported
as `MERGED-BUT-PROCESS-PINNED`, with PIDs, command and age. That is agent-lifecycle debt for
an operator, not something a disk guard should resolve by killing things.

## What this does NOT bound

State plainly, because the guard's name invites the wrong assumption: **it cannot bound build
output belonging to an active, open-PR, or process-pinned worktree.** Those are protected by
design and at any size.

That limit is load-bearing right now, not hypothetical. At the time of writing a *merged* lane
(`task-2640-respelling-replay`, PR #2644) holds **114 GB** that the guard refuses to touch
because shells with a cwd inside it are still running — some days old. The guard reports it,
raises the per-target CRITICAL budget on it, and stops there.

So the guard bounds *reclaimable* output. Bounding the rest requires retiring stale agent
sessions, which is an agent-lifecycle concern and deliberately not in this tool's remit.

## Why a system unit rather than a user unit

A user timer only runs while a login session exists, and this host has `Linger=no` — so it
would silently stop after an unattended reboot. Enabling lingering was the alternative, but on
this host that would also start `dotfiles-sync.service` (a `git pull`) unattended at boot,
which is outside this guard's remit. One system unit running as the dev user gives
boot-persistent housekeeping with the smallest blast radius.

## Known gap

This repository has no dev-host provisioning subsystem. `deploy/appliance/systemd/` is the
shipped *product* appliance, not the development VM, and there is no `ansible/`, `provision/`
or `hosts/` tree. These files therefore sit in `ops/scripts/`, the narrowest existing location
matching the convention already set by `setup-skill-symlinks.sh` (tracked-in-repo tooling that
configures machine-local developer-host state).

**Consequence: reprovisioning `icn-dev` does not install this automatically.** The installer
must be run by hand. If a dev-host provisioning home is ever established, these four files
should move there wholesale.

## Not done: shared `CARGO_TARGET_DIR`

Investigated and rejected. `sccache` already provides cross-worktree compile reuse (79.5% hit
rate, `SCCACHE_DIR=/mnt/build/sccache`); what per-worktree `target/` duplicates is link output
and incremental state, which sccache does not dedupe. A shared target directory would be
actively harmful: Cargo takes an exclusive lock per target directory, so concurrent agents
would serialise every build, and differing feature sets (`post-quantum` vs default) would
thrash it. Revisit only with evidence that overturns this.
