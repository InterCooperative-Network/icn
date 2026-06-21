---
Status: template
Authority: process
Canonical: no
Last verified: 2026-06-21
---

# ICN Live State Overlay

Bounded, on-demand **session-start grounding** for agents and humans working on the ICN repo. Before planning any work it answers: what is the current repo/project state, which facts are canonical vs generated-reference, what recently changed, what is stale, what must **not** be claimed, which lanes own the next work, and what checks to run.

This is **not** canonical truth — canonical state is `docs/STATE.md` + `docs/PHASE_PROGRESS.md`. It is **not** a committed snapshot (a committed "live snapshot" rots); it is generated on demand and never cached across sessions.

---

## Generate it (recommended)

Run the generator at session start; read its output, do not cache it:

```bash
python3 scripts/generate-live-state-overlay.py                 # markdown to stdout (default)
python3 scripts/generate-live-state-overlay.py --format json   # json to stdout
python3 scripts/generate-live-state-overlay.py --no-gh         # no GitHub calls (offline-safe)
python3 scripts/generate-live-state-overlay.py --check         # self-validate, exit 0/1
```

- **On-demand, not committed.** Default output is stdout. Use `--output PATH` to write a local copy you will **not** commit. There is intentionally no committed snapshot file.
- **No network required for a useful overlay.** It reads canonical docs, the generated grounding artifacts, the latest `docs/dev/` handoff, and git locally. `gh` is consulted **only** for the live OPEN/CLOSED state of the curated issue lanes (`gh issue view …`) and is labeled `live-reconfirmed` at generation time; without it (or with `--no-gh`) those fields are marked `NEEDS_LIVE_RECONFIRMATION`, never guessed.
- **Every line is source- or freshness-bound.** Nothing exceeds canonical state; the overlay carries an explicit `claim_boundaries` section.

### Sources it reads

- `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, `docs/ai/ICN_CONSTITUTIONAL_CORE.md` — canonical / reasoning state, each with its own freshness date.
- `docs/reference/project-index/generated/agent-context-spine.json`, `…/icn-file-record.json`, `…/route-inventory.md` — generated-reference grounding artifacts (orientation, not truth roots).
- the latest `docs/dev/handoff-*.md`; `git` HEAD / branch / working-tree; and `gh` issue state for the curated active lanes (optional, labeled when used).

### What it means / does not mean

- **Means:** a fast, honest orientation so you do not re-derive (or mis-derive) current state, reinvent existing systems, miss required checks, confuse planned/demo/private with implemented/public/canonical, or overclaim readiness.
- **Does NOT mean:** canonical truth, a runtime service, a dashboard, a Forge/receipt integration, or a new truth root. It is a thin grounding layer that can later feed those once the truth layer is reliable.

### How to use it

Agents: run it, then follow the overlay's own `agent_start_rules` (read overlay → read the relevant Agent Context Spine path brief → identify canonical vs generated-reference → identify required checks → identify claim hazards → only then plan; never merge without explicit per-PR authorization). Humans: skim sections 1–6 to orient, then 7–8 for what to do next. The `claim_boundaries` section lists what must never be claimed.

The self-check (`--check`) verifies the eight required sections exist, the structured entries in the canonical-state, grounding-artifact, and lane sections carry source/freshness (or `NEEDS_LIVE_RECONFIRMATION`) markers, the `claim_boundaries` cover the required hazards, the JSON round-trips, the markdown carries caveats, and no production-readiness / live-federation / formal-pilot / entity-auth-enforced overclaim appears in the fact-bearing sections.

---

## Manual fallback — what the overlay contains

If the generator is unavailable, fill the overlay by hand from the same sources. The sections below mirror the generated overlay's intent.

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
