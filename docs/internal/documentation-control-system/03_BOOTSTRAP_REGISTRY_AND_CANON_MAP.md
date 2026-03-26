# Bootstrap Registry and Canon Map

This file is a **starting registry for the migration PR**, not the final full registry.

It captures the highest-leverage docs already identified in the repo and recommends how they should be treated.

## High-value bootstrap set

| Path | Title | Truth Class | Role | Recommended Action |
|---|---|---|---|---|
| `docs/README.md` | ICN Documentation | unknown | reference | keep |
| `docs/INDEX.md` | ICN Documentation Index | historical | reference | keep |
| `docs/ARCHITECTURE.md` | ICN Architecture Reference | normative | architecture | keep |
| `docs/GETTING_STARTED.md` | Getting Started with ICN | unknown | guide | keep |
| `docs/STATE.md` | ICN State (living doc) | normative | reference | keep |
| `docs/TODO.md` | ICN TODO (ordered) | operational | reference | keep |
| `docs/DOCUMENTATION_MAINTENANCE.md` | Documentation Maintenance Guide | historical | governance | merge into control system and keep as placement/rules reference |
| `docs/planning/agent-knowledge-architecture.md` | Agent Knowledge Architecture Spec | draft | planning | merge retrieval model into control system and keep as supporting design |
| `docs/strategy/ADR-001-What-ICN-Is.md` | ADR-001: What ICN Is | normative | strategy | keep |
| `docs/strategy/ICN-Gap-Analysis-March-2026.md` | ICN System-by-System Gap Analysis: Vision vs. Reality | unknown | strategy | keep |
| `docs/planning/FORWARD_PLAN_2026-03.md` | ICN Forward Plan — March 2026 | unknown | planning | keep |
| `docs/mobile/icn-mobile-ux-spec-v1.md` | ICN Mobile Member UX Spec v1 | historical | spec | keep |
| `docs/design/institution-in-a-box.md` | Institution-in-a-Box | unknown | design | keep |
| `docs/templates/supersession-template.md` | Supersession Banner Template | historical | template | keep |
| `docs/guides/developer/DOCUMENTATION_STYLE.md` | Documentation Style Guide | operational | reference | merge into control system/templates and trim |
| `docs/internal/status/DOCS_REALITY_AUDIT_2026-02-11.md` | Docs Reality Audit - 2026-02-11 | unknown | status | keep (historical operational audit) |
| `docs/registry.toml` | ICN Documentation Registry | draft | reference | preserve as machine-readable registry source |
| `docs/freshness.toml` | ICN Documentation Freshness Tracking | unknown | reference | preserve as machine-readable adjunct metadata |
| `docs/status.toml` | ICN Implementation Status Scorecard | draft | reference | preserve as machine-readable adjunct metadata |

---

## Canonical entry docs to preserve and sharpen

These should remain easy to find and explicitly marked.

- `docs/README.md`
- `docs/INDEX.md`
- `docs/ARCHITECTURE.md`
- `docs/GETTING_STARTED.md`
- `docs/STATE.md`
- the new formal docs-system spec
- the machine-readable registries (`registry.toml`, `freshness.toml`, `status.toml`)

---

## Important existing system elements that should be absorbed, not discarded

### 1. `docs/DOCUMENTATION_MAINTENANCE.md`
Keep the placement logic and directory taxonomy.  
Merge any duplicate policy into the new formal doc system.

### 2. `docs/planning/agent-knowledge-architecture.md`
Keep the retrieval model:
- lightweight orientation
- pull context on demand
- machine-readable indexes
- avoid giant static preload

### 3. `docs/registry.toml`
Preserve and upgrade.  
This is already the seed of the registry system.

### 4. `docs/freshness.toml`
Preserve.  
This is a good model for section-level review and staleness tracking.

### 5. `docs/status.toml`
Preserve.  
This cleanly separates implementation status from architecture prose.

### 6. `docs/templates/supersession-template.md`
Preserve and formalize.  
Supersession should become part of the migration workflow.

### 7. `docs/guides/developer/DOCUMENTATION_STYLE.md`
Keep the idea of machine-readable metadata, but modernize it to match the new truth-class / role system.

---

## Immediate migration priorities

1. Formalize truth headers
2. Normalize doc placement
3. Mark superseded docs instead of letting them silently drift
4. Build `DOCUMENT_REGISTRY.md`
5. Keep machine-readable registry files in sync
6. Enforce 2-hop discoverability from entry docs

