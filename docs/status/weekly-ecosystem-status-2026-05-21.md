## Status Report: ICN Ecosystem — Week of May 14–21, 2026

**Author:** Matt Faherty | **Date:** 2026-05-21 | **Audience:** Internal / self

### Executive Summary

Strong engineering week. The architecture-spec sprint closed with ~18 PRs merged, landing twelve design-level specs and the abuse-case hardening doctrine; the first hardening implementation slice is now in design review. NYCN spent the week fully prepping the first organizer presentation. Two things need attention: the NLnet grant closes **June 1** (11 days out) and its draft hasn't started, and the NYCN pilot-formalization gate still hasn't been crossed. Today's cooperative-developer meeting is a calibration checkpoint, not a launch.

### Overall Status: 🟡 At Risk

Engineering is on track. The funding timeline and the pilot gate are the risk — both are time-bound and both currently depend on a single person finding focused blocks of time.

### Repo Snapshot

| Repo | Role | This week | Status |
|------|------|-----------|--------|
| icn | Rust substrate + core docs | Architecture-spec sprint, hardening doctrine, appliance verification | 🟢 On track |
| nycn | First-pilot ecosystem package | Organizer presentation prep completed | 🟡 Gate pending |
| network-ops | Homelab infra hosting the ICN K3s cluster | Quiet; cluster stable, no journaled changes | 🟢 Stable |
| icn-community-bridge | Planned Discord→Matrix bridge | Scaffold only; no application code | ⚪ Not started |
| icn-learn | ICN Academy learning site | Scaffold; most content still stubs | ⚪ Not started |

### Key Metrics

| Metric | Target | Actual | Trend | Status |
|--------|--------|--------|-------|--------|
| ICN PRs merged this week | — | ~18 (incl. 15-PR architecture sprint) | up | 🟢 |
| ICN open PRs | Keep low | 5 (3 docs + 2 Dependabot) | flat | 🟢 |
| Days to NLnet deadline | Submit by Jun 1 | 11 days remaining | down | 🟡 |
| ICN Phase 2 gate | Pilot formalized | Partner-bound, not formalized | flat | 🟡 |
| NYCN organizer presentation | Held | Prep complete, not yet scheduled | flat | 🟡 |

### Accomplishments This Period

- **ICN — architecture-spec sprint closed.** ~15 PRs (#1814–#1833) landed twelve design-level specs: the integrated operating-model spine, effect-dispatch contract, institutional domain, CCL policy registry, governed service binding, storage durability, ArtifactRegistry/ScopedVault, entity-scope vocabulary, compute placement, network anti-entropy proof loops, member shell v0, and steward cockpit v0. Ten sprint sibling issues closed; seven follow-ups filed (#1834–#1840).
- **ICN — abuse-case hardening doctrine landed (5-16).** `ABUSE_CASE_HARDENING_STRATEGY.md`: ten one-line doctrine rules, ten code-anchored abuse stories, and matching P0–P3 hardening tracks.
- **ICN — Debian appliance verified locally (5-17).** QCOW2 build plus one-VM boot smoke against `/v1/health` on port 8080; verified host toolchain recorded (#1876).
- **ICN — open-PR queue cleared (5-18).** First hardening implementation slice — `governance:write` decomposition (#1880) — opened and in design review.
- **NYCN — first organizer presentation fully prepped.** Presentation deck, facilitator guide, run sheet, pilot-rehearsal packet, and three rehearsal walkthroughs all in place.
- **Cross-cutting — Thursday meeting truth packet** assembled for today's cooperative-developer conversation.

### In Progress

| Item | Owner | Status | ETA | Notes |
|------|-------|--------|-----|-------|
| NLnet NGI Zero Commons application | Matt | Draft not started | Jun 1 | Tier 1, strongest fit, ~2 days of writing |
| `governance:write` decomposition (#1880 + follow-ons) | Matt | Design in review | — | 8-step migration sequence once design lands |
| ICN PRs #1878 / #1879 / #1880 | Matt | Open, awaiting required checks | — | Docs-only |
| Dependabot #1790, #1877 | Matt | Rebased CLEAN | — | Merge after local npm validation |
| NYCN organizer presentation | Matt | Prep done, not scheduled | — | Gates pilot formalization |
| icn-community-bridge | Matt | Scaffold | — | Discord→Matrix bridge, no code yet |
| icn-learn (ICN Academy) | Matt | Scaffold, content stubs | — | learn.icn.zone |

### Risks and Issues

| Risk/Issue | Impact | Mitigation | Owner |
|------------|--------|------------|-------|
| NLnet deadline Jun 1, draft not started | Miss the strongest Tier 1 funder for a 2-month cycle | Block ~2 focused days this week; start from `grant-narrative-core.md` | Matt |
| STF no-double-public-funding rule | Disqualification if STF and NLnet slices overlap | Carve discrete activity slices before either submission | Matt |
| NYCN pilot not formalized | ICN Phase 2 stays blocked | Schedule and hold the organizer presentation; record the decision | Matt |
| Solo bandwidth across five repos | Context-switching, dropped threads | Weekly status report + automated deadline checks | Matt |
| Bridge and Academy stalled at scaffold | Community-facing surfaces don't advance | Accepted for now — deprioritized behind funding and launch | Matt |

### Decisions Needed

| Decision | Context | Deadline | Recommended Action |
|----------|---------|----------|--------------------|
| Commit to NLnet this cycle | June 1 close; ~2 days of effort | ~May 28 | Yes — start the draft now |
| STF slice definition | Cannot overlap NLnet's funded activity | Before STF submission | Carve "kernel + identity hardening" as the STF-only slice |
| Schedule the NYCN organizer presentation | Gates pilot formalization and Phase 2 | This week | Pick a date with organizers within two weeks |
| Incorporate ICN as a worker co-op? | Affects Capital Impact eligibility and funding structure | Not urgent | Defer; revisit after NLnet and STF are in |

### Next Period Priorities

1. Draft and submit the NLnet NGI Zero Commons application — hard June 1 deadline.
2. Land the `governance:write` decomposition design (#1880) and begin the hardening implementation sequence.
3. Schedule the NYCN organizer presentation and clear the ICN PR + Dependabot queue.

---

*Sources: `icn/docs/STATE.md`, `icn/docs/dev/` session handoffs (2026-05-14 → 2026-05-18), `icn/docs/strategy/grants/funding-pipeline.md`, `nycn/docs/ROADMAP.md` and README, `network-ops` homelab journal. Generated 2026-05-21; GitHub was not connected, so PR counts are drawn from STATE.md sync edits and handoffs rather than live `gh` data.*
