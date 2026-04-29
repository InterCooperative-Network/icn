---
Status: operational
Canonical: no
Last Reviewed: 2026-04-29
---

# Docs Control Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map is a routing guide for the documentation control plane.

ICN's documentation has a real control plane: a hand-maintained navigation file, a machine-readable registry, a generated human summary, and a validator that runs in CI. Confusing one for another is a common drift source.

## The four control-plane artifacts

| Artifact | Path | What it is | Hand-maintained? |
|---|---|---|---|
| **`docs/INDEX.md`** | [`docs/INDEX.md`](../../INDEX.md) | Authoritative hand-maintained navigation. The "where do I look for X?" map. | **Yes — edit directly.** |
| **`docs/registry.toml`** | [`docs/registry.toml`](../../registry.toml) | Machine-readable registry. `[control]` defaults, `[[doc_path_defaults]]` prefix-based fallbacks, `[docs."path"]` per-doc overlays. Drives validators and the human summary. | **Yes — edit directly.** |
| **`docs/DOCUMENT_REGISTRY.md`** | [`docs/DOCUMENT_REGISTRY.md`](../../DOCUMENT_REGISTRY.md) | Human-readable summary of the registry. **Generated.** | No — regenerate via `python3 docs/scripts/doc_control_check.py --write-document-registry`. |
| **`docs/scripts/doc_control_check.py`** | [`docs/scripts/doc_control_check.py`](../../scripts/doc_control_check.py) | Validator. Runs in CI; can be run locally. | Yes (Python). |

There is also a generated **`docs/INDEX.generated.md`** for drift detection in CI; if it disagrees with `docs/INDEX.md`, reconcile in `INDEX.md` intentionally.

## Canonical paths

The control-plane "canonical" set is exactly the files listed in `[control].canonical_doc_paths` in `docs/registry.toml`:

```
docs/INDEX.md
docs/STATE.md
docs/ARCHITECTURE.md
docs/DOCUMENTATION_CONTROL_SYSTEM.md
```

These four paths are the only ones machine-checked for required headers (`Status`, `Canonical`, `Last Reviewed`) and merged-registry alignment. Other docs may carry `Canonical: yes` in their YAML for emphasis, but they are not in the control-plane set unless `canonical_doc_paths` is updated. Do not add to that list casually.

## Truth classes

Per `[[doc_path_defaults]]` and `[docs."path"]` rows, every doc lands in one of:

| Truth class | What it means | Example |
|---|---|---|
| **`normative`** | Defines a rule, contract, or invariant the system commits to. | [`docs/genesis.md`](../../genesis.md), [`docs/ai/ICN_CONSTITUTIONAL_CORE.md`](../../ai/ICN_CONSTITUTIONAL_CORE.md) |
| **`descriptive`** | Describes how something currently behaves. Authoritative for "what is," not "what must be." | [`docs/STATE.md`](../../STATE.md) |
| **`operational`** | A how-to / runbook / navigation surface. Used to act, not to decide. | [`docs/INDEX.md`](../../INDEX.md), this directory |
| **`historical`** | Archive material. Not current truth. | Anything under `docs/archive/` |
| **`draft`** | In flight. Do not treat as current. | RFCs, `docs/rfcs/`, design proposals not yet ratified |

The `Status:` YAML key on a doc must match `truth_class` in the registry. Mismatches show up as `[yaml-mismatch]` warnings from the validator.

## How to read drift signals

| Signal | What it means | What to do |
|---|---|---|
| Doc says "current" but it's under `docs/archive/` or `docs/dev-journal/snapshots/` | **Historical, not current.** | Look up the canonical path in `STATE.md` or `INDEX.md`. |
| Doc has YAML `Last Reviewed: <90+ days ago>` and is in `canonical_doc_paths` | **Stale canonical.** | Re-review and bump the date, or fix the substance. |
| Two docs have the same H1 in different directories | Duplicate identity (`warn_duplicate_h1_across_dirs`) | Pick one canonical home; archive the other. |
| Doc references a path that no longer exists | Broken link | Fix or remove the reference. |
| `[yaml-mismatch]` warning | YAML and registry disagree | Update one to match the other. |

## How not to confuse strategy / planning / status with current truth

- **`docs/strategy/`** — long-arc planning context. Each doc carries its own framing about "what we will do" or "what we are doing." It is **not** the current-state record. The exception is documents that explicitly state they are truth-synced (e.g., `docs/strategy/ICN-Roadmap-Live.md`, which now points at `STATE.md` and `PHASE_PROGRESS.md` as canonical).
- **`docs/planning/`** — design and forward plans. Often partial, often draft.
- **`docs/internal/`** — internal pilot and program planning. Not externally-facing truth.
- **`docs/status/`** — point-in-time status reports. Always check the date.
- **`docs/dev/`** — session handoffs (`handoff-YYYY-MM-DD.md`) and proof-path records. The latest handoff is a session capture; `STATE.md` absorbs its conclusions.
- **Current truth lives in:** `docs/STATE.md` (descriptive snapshot, refreshed every state-changing PR) and `docs/PHASE_PROGRESS.md` (phase model).

## Adding a new doc

1. **Write the doc** under the right `docs/` subdirectory. Never at the repo root. (See [`source-tree-map.md`](source-tree-map.md) for which subdirectories are allowlisted.)
2. **Add YAML frontmatter** with `Status`, `Canonical`, `Last Reviewed`.
3. **Register it** in `docs/registry.toml` with at least:
   - `category` (architecture, design, strategy, guide, reference, operations, security, archive, status, …)
   - `status` (canonical, living, draft, archived, superseded, missing)
   - `title`, `description`, `last_updated`, `owner`, `audiences`
   - For control-plane alignment, also `truth_class`, `role`, `canonical`, `freshness`, `domain_tags`, `recommended_action`, `last_reviewed`.
4. **Link it from `docs/INDEX.md`** under the right section.
5. **Validate** locally before committing: `python3 docs/scripts/doc_control_check.py --strict`. CI runs the same check.
6. **Vocabulary scan**: avoid `payment`, `currency`, `wallet`, `balance`. Prefer `settlement`, `unit`, `identity`, `position`, `obligation`, `allocation`. The Regulatory Compliance Linter in CI enforces this on key paths.

## Validators in CI

| Workflow / job | What it enforces |
|---|---|
| `Documentation Control Plane` (in `docs-freshness.yml`) | Runs `doc_control_check.py`; fails on critical drift. |
| `Documentation Freshness` | Section ages and subsystem verification dates. Posts a freshness report comment on PRs. |
| `Generate Documentation Index` | Regenerates `INDEX.generated.md`; flags drift vs hand-maintained `INDEX.md`. |
| `Lint Architecture Doc` | Checks `docs/ARCHITECTURE.md` for structural integrity. |
| `Regulatory Compliance Linter` | Vocabulary check against the avoid list. |
| `Meaning Firewall Check` | Crate-import boundary check (kernel vs apps). Not a doc check, but it lives next to docs in the workflow. |

## Where to read deeper

| Topic | Doc |
|---|---|
| Doc control system doctrine | [`docs/DOCUMENTATION_CONTROL_SYSTEM.md`](../../DOCUMENTATION_CONTROL_SYSTEM.md) |
| Doc maintenance and where to put new docs | [`docs/DOCUMENTATION_MAINTENANCE.md`](../../DOCUMENTATION_MAINTENANCE.md) |
| Documentation style | [`docs/guides/developer/DOCUMENTATION_STYLE.md`](../../guides/developer/DOCUMENTATION_STYLE.md) |
