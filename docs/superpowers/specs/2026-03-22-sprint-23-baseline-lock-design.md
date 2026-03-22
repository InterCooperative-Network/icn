# Sprint 23 Design — Baseline Lock + Narrative Surface

**Date:** 2026-03-22
**Status:** Approved
**Author:** Matt Faherty
**Owner:** Matt Faherty (sole executor unless otherwise assigned)

---

## Governing Rule

> **A task is only "done" when repo state, board state, and narrative state agree.**

This is the whole sprint in one line. Sprint 23 is not a velocity sprint. It is a legitimacy sprint. The measure of success is convergence, not coverage.

---

## Context

ICN just closed two major internal-hardening arcs simultaneously:

- **Meaning Firewall** (Sprints 19–22): All hardcoded policy constants extracted to typed config structs across `icn-security`, `icn-compute`, `icn-ledger`, `icn-obs`. Fully merged.
- **Pilot Completion** (#1099) and **Vertical Slice** (#1147): All tracks closed. Only 3 issues remain across both epics (#1095, #1096, #1131).

The system is operationally healthy: cluster green, cargo check passes, gateway stack reachable. But the repo is slightly lying. The sprint board shows work as "in-review" that is already merged. `main` is red from a non-blocking CI job. One dirty file and a stale worktree remain from the skills rewrite merge.

The danger at this inflection point is not lack of work. It is **diffusion**. Entering a new expansion wave (Commons Compute, Naming, Federation) before restoring repo truth would stack new architecture on a narratively blurry foundation.

Sprint 23 addresses this directly.

---

## Theme

**Baseline Lock + Narrative Surface**

Turn current completion into a stable, legible, demoable platform baseline. Do the work that makes everything else easier to trust, demo, explain, and sequence.

---

## Sprint 23 Tasks

### Track A — Operations Closure
*Make the repo tell the truth.*

**s23-t1** — Fix `Test Coverage` CI failure on `main`
- **Acceptance:** Test Coverage failure resolved; `main` passes all required non-observational gates, or remaining failures are explicitly classified as non-blocking with rationale committed to `ops/state/ci-exceptions.md`.

**s23-t2** — Resolve dirty file in `icn/` (from #1394 skills rewrite merge)
- **Acceptance:** `git status` is clean on `main`. The dirty file is either committed with a rationale commit message, or deleted. Stash is not an acceptable resolution — it is not a portable terminal state.

**s23-t3** — Remove stale `1310-execution-receipt-gate` worktree
- **Acceptance:** `icn-wt/` contains no detached or branchless worktree entries. Stale branch pruned or deleted.

**s23-t4** — Formally close Sprint 22
- **Artifact:** `ops/state/sprint/current.json` (sprint board source of truth)
- **Acceptance:** Sprint 22 state reconciled across board + `ops/state/sprint/current.json` + merged issue disposition on GitHub. All five s22 tasks marked done. Sprint board closed. No "in-review" state pointing at already-merged PRs.

---

### Track B — P0 Residue Closure
*Remove the dangling caveats.*

Runs in parallel with Track A. Does not block Track A.

**s23-t5** — `#1095` CRDT OrSet + LwwRegister implementation
- **Acceptance:** `#1095` (CRDT OrSet + LwwRegister) is merged, explicitly deferred with documented rationale committed, or decomposed into bounded child tasks with named ownership and acceptance criteria. No ambiguity about disposition.

**s23-t6** — `#1096` ContainerRuntime trait interface
- **Acceptance:** `#1096` (ContainerRuntime trait) is merged, explicitly deferred with documented rationale committed, or decomposed into bounded child tasks with named ownership and acceptance criteria. No ambiguity about disposition.

**s23-t7** — `#1131` Storage Specification — Storage is Governance
- **Acceptance:** Written spec merged to `docs/`, issue closed or explicitly deferred with rationale committed. Independent of t5/t6 — proceed in parallel.

---

### Track C — Baseline Narrative
*Convert technical truth into portable truth.*

These are **first-class sprint tasks**, not documentation exhaust. They are synthesis tasks that convert technical closure into institutional legibility. Their visibility on the board is intentional: it signals that "explaining the system clearly" is part of the sprint's definition of done.

Each has hard dependency relationships to the Track A/B work it crystallizes.

**s23-t8** — Publish current platform baseline
- **Depends on:** s23-t1, s23-t2, s23-t3, s23-t4 (operational truth fully restored before doc is authored)
- **Artifact:** `docs/platform-baseline-2026-03.md`
- **Acceptance:** One document exists that answers: what ICN is now, what is complete, what is in progress, what is next. A new contributor can orient from this document without requiring oral tradition.

**s23-t9** — Roadmap + sprint-state refresh
- **Depends on:** s23-t4 (Sprint 22 closed), s23-t7 (P0 tail disposition known)
- **Artifacts:**
  - `docs/strategy/ICN-Roadmap-Live.md` (update in place; create Sprint 23 section if absent)
  - `ops/state/sprint/current.json` (Sprint 23 tasks populated — distinct from s23-t4 which closes Sprint 22 in this file)
- **Acceptance:** `ICN-Roadmap-Live.md` updated to reflect post-Sprint-22 completion state. `ops/state/sprint/current.json` contains the Sprint 23 task list (separate from Sprint 22 closure handled by s23-t4). Sprint 24 candidate backlog listed with at least the Commons Compute trio (#925, #947, #964) shaped as candidates.

**s23-t10** — Validate and document canonical demo path
- **Depends on:** s23-t5, s23-t6 (P0 disposition must be known so the demo doc can explicitly state what is included and what is deferred. Note: #1095 CRDT OrSet and #1096 ContainerRuntime are not on the critical path for Flows A, B, or C — they are post-demo completion items. The dependency is about disposition documentation, not flow enablement.)
- **Artifact:** `docs/demo-path-2026-03.md`
- **Acceptance:** One document with exact commands to reproduce the current demo on `main`. Tested against reality, not aspiration. Covers at minimum: devnet startup, Flow A (WASM), Flow B (discovery), Flow C (treasury governance). Includes a brief "scope note" stating the current disposition of #1095 and #1096 and confirming they do not affect these three flows.

---

## Dependencies

```
t1 ─┐
t2 ─┼──▶ t8 (baseline doc)
t3 ─┤
t4 ─┘

t4 ──────────────────────────────────────────────▶ t9 (roadmap refresh)
t7 ──────────────────────────────────────────────▶ t9

t5 ─┬────────────────────────────────────────────▶ t10 (demo path)
t6 ─┘

t7  (independent — runs in parallel with t5/t6)
```

---

## What Is Explicitly Out of Scope

| Area | Why deferred |
|------|-------------|
| Commons Compute expansion (#925, #947, #964) | Sprint 24 spine — lands better on clean baseline |
| Naming Primitive (#862), Federation Agreements (#863) | Sprint 25 — conceptual expansion after baseline |
| Website / React Native SDK (#1366, #1368, #1369) | External-facing surfaces depend on narrative stability |
| Trust hardening perf (#1049, #1053, #1054) | Third tier — trailing work |

---

## Sequencing

1. **Track A + B in parallel** — restore operational truth, close P0 tail
2. **Track C** — synthesize and publish; closes the sprint
3. **Sprint 24** opens around Commons Compute expansion

The sprint is done when all 10 tasks reach their acceptance criteria and the governing rule holds: repo state, board state, and narrative state agree.

---

## Proposed Sprint Cadence Beyond Sprint 23

| Sprint | Theme | Primary spine |
|--------|-------|---------------|
| **23** | Baseline Lock + Narrative Surface | Legitimacy through convergence |
| **24** | Commons Compute Hardening | Unaffiliated nodes, resource pools, stale expiry |
| **25** | Federation Semantics | Naming primitive, federation agreements |
| **Parallel** | External surface | Website, roadmap page, SDK externalization |
