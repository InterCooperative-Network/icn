---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Live register of planned and tracked visual assets for ICN. One row per asset. The brief is the substance; this register is the index.
---

# ICN Visual Asset Register

The register is the index. Each row is one tracked visual asset. Briefs live in [`briefs/`](briefs/). Doctrine lives in the [ICN Visual Explainer Bible](../ICN_VISUAL_EXPLAINER_BIBLE.md).

## Status values

| Status | Meaning |
|---|---|
| `planned` | Row exists. Brief not yet written. |
| `briefed` | Brief exists and is closed to generation until the gate items are met. |
| `gate-open` | Brief gate (bible §10) satisfied. Source-asset build may begin. |
| `built` | Source asset implemented. Awaiting review. |
| `shipped` | Review passed (per [VISUAL_REVIEW_CHECKLIST.md](VISUAL_REVIEW_CHECKLIST.md)) and asset is in use. |
| `historical` | Used to ship; superseded. Preserved for trajectory, not for current use. |
| `do-not-use` | Recognized as misleading or against doctrine. Preserved only as a counter-example. |

## Truth labels

See bible §3. Asset-level labels: `implemented / current UI`, `repo-grounded public explainer`, `repo-grounded architecture explainer`, `illustrative direction`, `future-state / roadmap`, `historical`, `do not use`.

## Asset classes

See bible §9. Classes: Astro/SVG component, SVG-in-docs, Mermaid, generated-image sketch (never load-bearing).

## Register

| ID | Title | Audience | Truth label | Class (target) | Source anchors | Brief | Status |
|---|---|---|---|---|---|---|---|
| VE-001 | How ICN Works / Closure Loop | Public + first-conversation reader | repo-grounded public explainer | Astro component (exists) + public-facing explainer expansion | [concept-map.md](../../design-language/concept-map.md), [ClosureLoop.astro](../../../website/src/components/ClosureLoop.astro), [HowIcnWorksExplainer.astro](../../../website/src/components/HowIcnWorksExplainer.astro) | [VE-001](briefs/VE-001-how-icn-works-closure-loop.md) | built |
| VE-002 | Where You Are Acting / Scope Model | Public + member | repo-grounded public explainer | Astro component (exists) + member-facing scope diagram | [concept-map.md](../../design-language/concept-map.md), [ScopeModel.astro](../../../website/src/components/ScopeModel.astro) | [VE-002](briefs/VE-002-scope-model.md) | gate-open |
| VE-003 | Decision to Receipt / Provenance Trail | Public + organizer + developer | repo-grounded public explainer | Astro component (exists) + annotated proof-loop figure | [ProvenanceTrail.astro](../../../website/src/components/ProvenanceTrail.astro), [runtime-surface-map.md](../../reference/project-index/runtime-surface-map.md), [NYCN_ACTION_ITEM_RECEIPT_PATH.md](../../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md) | [VE-003](briefs/VE-003-decision-to-receipt.md) | gate-open |
| VE-004 | Member Shell Concept | Public + member + grant reviewer | illustrative direction | Astro component (exists) — extend as concept surface | [MemberSurface.astro](../../../website/src/components/MemberSurface.astro), [MEMBER_STANDING.md](../../architecture/MEMBER_STANDING.md), [runtime-surface-map.md](../../reference/project-index/runtime-surface-map.md) | [VE-004](briefs/VE-004-member-shell-concept.md) | gate-open |
| VE-005 | Kernel / App Separation | Developer + architect + grant reviewer | repo-grounded architecture explainer | SVG-in-docs or Astro component (new) | [KERNEL_APP_SEPARATION.md](../../architecture/KERNEL_APP_SEPARATION.md), `icn-kernel-api` crate, `icn/apps/governance/`, `apps/trust/` | [VE-005](briefs/VE-005-kernel-app-separation.md) | gate-open |
| VE-006 | Federation Without Centralization | Public + organizer + developer | repo-grounded architecture explainer (primitives) + future-state / roadmap (live federation) | SVG-in-docs or Astro component (new) | [FEDERATION_INTEROP_CONTRACT.md](../../architecture/FEDERATION_INTEROP_CONTRACT.md), `icn-federation` crate, [show-readiness-map.md](../../reference/project-index/show-readiness-map.md) | not yet | planned |
| VE-007 | Commons and Compute | Public + organizer + developer | repo-grounded architecture explainer + future-state / roadmap | SVG-in-docs or Astro component (new) | [compute-substrate-design.md](../compute-substrate-design.md), [COMMONS_EVOLUTION.md](../COMMONS_EVOLUTION.md), `icn-compute` crate | not yet | planned |
| VE-008 | Action Card Anatomy | Member + developer + organizer | illustrative direction (member view) + repo-grounded public explainer (data shape) | SVG-in-docs anatomy figure | [`/v1/gov/me/action-cards`](../../reference/api/API_REFERENCE.md), [ADR-0027](../../adr/ADR-0027-action-card-contract.md), [runtime-surface-map.md](../../reference/project-index/runtime-surface-map.md) | not yet | planned |
| VE-009 | Receipt Detail Anatomy | Member + organizer + developer | repo-grounded public explainer | SVG-in-docs anatomy figure | [`/v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`](../../reference/api/API_REFERENCE.md), [NYCN_ACTION_ITEM_RECEIPT_PATH.md](../../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md), `icn-governance` receipt types | not yet | planned |
| VE-010 | Steward / Operator Cockpit | Operator + steward | illustrative direction | Astro component (new) | [runtime-surface-map.md](../../reference/project-index/runtime-surface-map.md), [NYCN_K3S_PROOF_PATH.md](../../dev/NYCN_K3S_PROOF_PATH.md) | not yet | planned |
| VE-011 | Regulatory-Safe Verifiable State | Grant reviewer + developer + organizer | repo-grounded architecture explainer | SVG-in-docs or Astro component (new) | [regulatory-safe-verifiable-state.md](../regulatory-safe-verifiable-state.md), [CONTENT_STYLE_GUIDE.md](../CONTENT_STYLE_GUIDE.md) | not yet | planned |
| VE-012 | What ICN Is / Is Not | Public + first-conversation reader + grant reviewer | repo-grounded public explainer | SVG-in-docs or Astro component (new) | [THE_COMMONS.md](../../architecture/THE_COMMONS.md), [show-readiness-map.md](../../reference/project-index/show-readiness-map.md), [genesis.md](../../genesis.md) | not yet | planned |

## How to add a row

1. Pick the next `VE-NNN` ID.
2. Add a row to the table.
3. Open status as `planned`.
4. Write a brief at `briefs/VE-NNN-slug.md` using the [VE-005 brief](briefs/VE-005-kernel-app-separation.md) as a structural template.
5. Move the row's status to `briefed` once the brief exists, and to `gate-open` once the brief gate is satisfied (bible §10).
6. Run [VISUAL_REVIEW_CHECKLIST.md](VISUAL_REVIEW_CHECKLIST.md) before shipping.

## How to retire a row

A row can be retired when:

- the surface it described has been superseded by a different explainer (move to `historical`);
- the visual it tracks has been judged misleading or against doctrine (move to `do-not-use`);
- the brief has been merged with another brief (move to `historical` with a `superseded_by:` note).

Retirement is documented in the brief, not by deletion. The register row stays.

## See also

- [docs/design/ICN_VISUAL_EXPLAINER_BIBLE.md](../ICN_VISUAL_EXPLAINER_BIBLE.md)
- [docs/design/assets/README.md](README.md)
- [docs/design/assets/VISUAL_REVIEW_CHECKLIST.md](VISUAL_REVIEW_CHECKLIST.md)
