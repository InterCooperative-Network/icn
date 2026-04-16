---
Status: template
Authority: process
Canonical: no
Last verified: 2026-04-15
---

# ICN Live State Overlay Template

Fill this overlay at the start of any non-trivial session. It grounds the agent in current project reality.

This is a **template**. The content you fill in comes from the canonical documents listed below. Do not cache or inherit overlay content from a previous session — always reload.

---

## Files to Load First

Load these files and extract current state before beginning work:

1. `docs/ai/ICN_CONSTITUTIONAL_CORE.md` — reasoning foundation (stable, rarely changes)
2. `docs/STATE.md` — declared project state
3. `docs/PHASE_PROGRESS.md` — phase tracking and metrics
4. Latest handoff in `docs/dev/` (find by date: `ls -t docs/dev/handoff-*.md | head -1`)
5. Active tranche/execution docs relevant to current task (if any)

---

## Declared Project State

<!-- Fill from docs/STATE.md and docs/PHASE_PROGRESS.md -->

**Current phase:** <from PHASE_PROGRESS.md>

**Phase status:** <complete / in-progress / blocked — with blocker if any>

**Strongest landed core:** <which subsystems are shipped and real>

**Weakest / most partial areas:** <which subsystems are incomplete or aspirational>

**Major project risk:** <the recurring risk named in canonical docs>

---

## Current Execution Direction

<!-- Fill from latest handoff and any active tranche/execution docs -->

**Latest handoff:** `docs/dev/handoff-YYYY-MM-DD.md`

**Current branch:** <from git>

**Open PRs:** <from gh pr list>

**Merge order / next steps:** <from handoff "Next Move" section>

**Active tranche docs:** <paths to any NYCN-Execution-Tranches.md or similar, if relevant>

---

## Current Ontology / Model Direction

<!-- Fill from architectural decisions in latest handoff or strategy docs -->

<Summarize the current entity/structure/activity model, naming conventions, authority model, and any locked architectural decisions relevant to the current task.>

---

## Conventions to Preserve

<!-- Fill from handoff, CLAUDE.md, and project conventions -->

- Events: <naming pattern>
- Storage keys: <primary and secondary key patterns>
- HTTP routes: <scope and path conventions>
- Kernel boundary: <what is forbidden in kernel crates>
- Domain-specific content: <where it belongs>

---

## Workspace Notes

<!-- Fill from CLAUDE.md and verified environment -->

- Rust workspace: `icn/` (all `cargo` commands run from here)
- Key verification commands:
  ```bash
  <from handoff or CLAUDE.md>
  ```

---

## Divergences Discovered

<!-- If current repo state differs from what canonical docs claim, note it here -->

- <divergence description — e.g., "STATE.md says X but code shows Y">
- <or "None discovered" if everything aligns>
