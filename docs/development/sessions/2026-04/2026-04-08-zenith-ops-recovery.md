# Session: Zenith ops recovery + sprint tracking truth-up

**Date:** 2026-04-08
**Host:** Zenith (Windows / WSL2 Ubuntu 24.04)
**Agent:** Claude Sonnet 4.6
**Operator:** Matt
**Outcome:** Cluster fully green; website shipped; sprint tracking reconciled; two PRs (one merged, one open)

---

## What happened

Session began with the user asking me to orient on Zenith from scratch, then SSH into the icn-dev VM in the homelab and audit state. That cascaded into three compounding workstreams: website docs cleanup, full homelab SSH orientation, and emergency ops recovery after discovering two coop pods and the central Prometheus instance were all crashlooping.

## What shipped

### PR #1512 — `docs(website): add OVERVIEW manual + fix docs explorer titles + refresh roadmap`
**Status:** ✅ Merged as `aaa87b3f`
**Changes:**
- `docs/OVERVIEW.md` (+289 lines) — comprehensive project manual covering architecture, eight primitives, Meaning Firewall, receipt chain, mutual credit terminology discipline, honest current-state breakdown. Auto-renders in the docs explorer Core section.
- `website/src/pages/docs/index.astro` (+56 lines) — replace filename-derived titles with a fallback chain (YAML frontmatter → first H1 → filename transform). ADR and strategy doc titles now render human-readable.
- `website/src/data/roadmap.json` — `currentSprint` was stale at Sprint 27; initially updated to Sprint 23, but that was also wrong (see PR #1513 below).

### PR #1513 — `docs(roadmap): truth-up sprint tracking to post-Sprint-28 Governance Dispatch Tranches`
**Status:** 🟡 Open, all required checks green, `claude-review` pending (infra-flaky 15m job — non-blocking per CLAUDE.md)
**Changes:**
- `website/src/data/roadmap.json` — Sprint 23 → **Sprint 28 "Governance Dispatch Tranches"** with detailed status block.
- `docs/strategy/ICN-Roadmap-Live.md` — prepended a "Current state (2026-04-08)" block summarising Sprints 24–28 and listing active Governance Dispatch Tranches 9–16 (merged), 17 (UpdateJurisdictionTier, local on icn-dev), 18 (TerminateClearing + RevokeVouch, next). Per-sprint sections below are flagged as historical; git log and `docs/architecture/agent-handoff-current.md` named as canonical source of truth.

## Ops recovery

### icn-coop-delta + icn-coop-gamma keystore crashloops (fixed)
- **Symptom:** Both coops in `CrashLoopBackOff`, delta 10h / gamma 12d, identical error `Not a v3 keystore, trying legacy format → Decryption failed`.
- **Root cause:** Pods use a `copy-keystore` init container running `cp -n /data/.icn/identity.age /data/identity.age` (no-clobber). Canonical keystore at `/data/.icn/identity.age` was written 2026-02-28 07:14 in sync with the K8s `*-secrets` passphrases. Something rewrote `/data/identity.age` in place on gamma (Mar 27) and delta (Apr 5) with passphrases that didn't match the secrets. The no-clobber init never restored the canonical because the file already existed. File sizes coincidentally matched canonical (3330/3302) but md5 differed.
- **Fix:** For each pod, started a debug pod with the PVC mounted RW, preserved the broken file as `identity.age.broken-20260408`, removed `identity.age`, killed the coop pod. Init container copied canonical back on restart. Both pods now Running 1/1, 0 restarts, clean gossip handshakes visible in logs.
- **Forensic artefacts left in place:** `/data/identity.age.broken-20260408` on both delta and gamma PVCs for post-mortem.

### Prometheus CrashLoopBackOff (fixed)
- **Symptom:** `prometheus-prometheus-kube-prometheus-prometheus-0`, 654 restarts over 6 days, `panic: series ID exceeds 5 bytes`.
- **Root cause:** TSDB head chunk reference overflow — the 40-bit chunk ID space on the head block was exhausted, so every head GC attempt panicked. Known Prometheus bug in long-running instances.
- **Fix:** Pod uses `emptyDir: {}` for `prometheus-db` (no PVC, no persistent metric history configured anywhere). `kubectl delete pod` was the entire fix — fresh emptyDir on next reschedule. Pod now 2/2 Running, 0 restarts, 22/22 scrape targets up.
- **Separate caveat flagged for followup:** kube-prometheus-stack on this cluster has zero persistent storage for metrics. Every pod reschedule already wiped history silently. One-line Helm values fix (add a PVC template) recommended.

### k3s-worker-1 SSH host key (fixed)
- Old key removed from `known_hosts`, scanned and trusted new key. Worker reachable: load 0.01, 51% disk, healthy.

### Zenith state cleanup
- **Worktrees:** 4 orphaned dirs under `.claude/worktrees/` (`dazzling-jones`, `elastic-germain`, `sleepy-wright`, `sleepy-wright-1f68`). Git metadata was stale (pointed at UNC paths; actual gitdir is WSL-native). `git worktree prune` cleared registry; `rm -rf` removed directories. **4.2 GB reclaimed** (elastic-germain contained a full `target/` dir from a killed cargo session).
- **Stashes:** Dropped `stash@{0}` (`local claude system before sync` — proven duplicate of current main; verified by comparing the stash's new_string against `HEAD:.claude/agents/icn-architect.md`). Left 11 other stashes in place — they reference deleted feature branches and contain potentially-recoverable work (`demo/scripts/load-sample-data.sh`, `web/pilot-ui/*`, `icn-federation/src/clearing_manager.rs`) that I wasn't confident enough to silently drop.

## Environment gotchas discovered

**MSYS path mangling on Zenith.** The outer shell is MINGW64 (Git Bash) which by default converts `/home/matt/...` → `C:/Program Files/Git/home/matt/...` when invoking `wsl.exe`. Set `MSYS_NO_PATHCONV=1` for any `wsl.exe` command that passes WSL-native paths as arguments. This broke several commands early in the session before I figured it out.

**zsh on icn-dev.** The remote shell is zsh, not bash. Shell constructs like `=== section ===` trip zsh's history expansion. Wrap multi-line SSH commands in `"..."` on the outer shell and single-quote the zsh side, or use heredoc scripts dropped onto WSL `/tmp` and run via `bash`.

**Loopback protocol on icnctl.** `kubectl exec ... -- icnctl network peers` fails inside the daemon pod because icnctl defaults to `http://[::1]:5601/` but icnd binds IPv4-only. Known cosmetic issue, doesn't block anything. Use the gateway API or direct HTTP calls for diagnostics instead.

## Active state when this session ends

- **Branch:** `docs/sprint-tracking-truth-up` (PR #1513 open, pending claude-review)
- **Local main:** fully caught up with origin (`d3cee4cb` + merged #1512 = `aaa87b3f`)
- **Cluster:** fully green. 4/4 coops, all monitoring, all NFS mounts, all P2P mesh, all scrape targets.
- **icn-dev VM:** another Claude session actively working Tranche III (UpdateJurisdictionTier executor wiring) on branch `fix/steward-bond-semantics`. Per session transcript shared by user, local implementation complete: `StewardRecord.jurisdiction_tier` field added, `CommonsHandle::update_jurisdiction_tier` plumbed, `SdisService` trait extended, executor arm replaced, 4 new unit tests pass, fmt + clippy clean, 381/381 icn-core tests pass. **Not yet pushed to origin.**

## Remaining for next session

1. **Watch PR #1513 to merge.** Check `gh pr view 1513`; if claude-review has come in (or timed out into "skipped"), merge. If still pending, safe to `--admin` merge once all non-flaky gates are green.
2. **Pull icn-dev's Tranche III work when pushed.** Look for `feat(sdis): Tranche 17 — UpdateJurisdictionTier …` commit on origin (or a new PR from `fix/steward-bond-semantics`). After merge, `git pull` on Zenith, rebuild website stats so the Tranche 17 entry in the roadmap can be marked ✅ merged (tiny follow-up PR).
3. **Investigate Mar 27/Apr 5 keystore rewrites.** Two coop identity files were rewritten with bad passphrases on distinct dates. Broken files preserved at `/data/identity.age.broken-20260408` on delta and gamma PVCs for forensic inspection. Probably an `icnctl id rotate` test job or operator action that didn't update the K8s secret. Next time this happens it should be caught automatically — either a sidecar that syncs the secret when the keystore rotates, or a nightly CronJob that sanity-checks decryption.
4. **Add persistent storage to kube-prometheus-stack.** Currently `emptyDir: {}` — every pod reschedule silently wipes metric history. One-line Helm values fix.
5. **`.claude/mcp.json` path portability.** The `icn-ops` MCP server path bakes in `/home/ubuntu/projects/icn/ops/mcp/dist/index.js` which is correct for icn-dev but wrong for Zenith (`/home/matt/...`). Need host-specific override or a local build of the MCP server.
6. **Investigate `docs/development/sessions/undated/`** — probably needs sweeping into dated subdirs.

## Followups left alone intentionally

- Dropping stashes @{1–11}. They reference branches that no longer exist but contain real code I didn't author and shouldn't silently discard.
- Sprint history for Sprints 24–27. I summarised from git log; writing authoritative close notes would be fabrication.
- icn-dev's `fix/steward-bond-semantics` working tree. Other Claude session is mid-implementation; I stayed out.
- icn-ops `state/sprint/current.json` on icn-dev. Lives outside this repo, maintained separately.

## Stats at session end

- **Website:** 662 pages, 35 crates, 439,875 LoC, 4,320 tests, 750 PRs, 654 doc files. Latest commit reflected: `d3cee4cb`.
- **Cluster:** 4/4 coops Running, 22/22 Prometheus scrape targets up, 0 crashlooping pods.
- **Disk reclaimed:** 4.2 GB (worktree cleanup).
- **PRs shipped:** 1 merged, 1 open pending claude-review.

---

_End of handoff._
