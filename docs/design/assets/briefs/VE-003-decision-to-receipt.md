---
Status: draft
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-13
Last Updated: 2026-05-13
Purpose: Brief for VE-003 — Decision to Receipt / Provenance Trail. The canonical public explainer for how an ICN decision becomes a durable institutional receipt.
---

# VE-003 — Decision to Receipt / Provenance Trail

## Truth label

**`repo-grounded public explainer`** for the model. A depiction that names the three currently emitted source paths (`proposal/vote`, `action_item/complete`, `meeting/attend`) is `implemented / current UI`. A depiction that includes the two RFC-gated paths (`signal_rule`, `obligation_lifecycle`) is `future-state / roadmap` until those land.

## Source anchors

- [`website/src/components/ProvenanceTrail.astro`](../../../../website/src/components/ProvenanceTrail.astro) — canonical Astro implementation
- [`docs/reference/project-index/runtime-surface-map.md`](../../../reference/project-index/runtime-surface-map.md) — actual gateway surfaces and action-card source paths
- [`docs/reference/project-index/current-truth-map.md`](../../../reference/project-index/current-truth-map.md) — three of five currently emitted source paths
- [`docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`](../../../dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md) — local HTTP proof loop
- [`docs/dev/NYCN_K3S_PROOF_PATH.md`](../../../dev/NYCN_K3S_PROOF_PATH.md) — K3s smoke proof loop
- [`docs/architecture/MEMBER_STANDING.md`](../../../architecture/MEMBER_STANDING.md) — member-standing contract
- `icn/apps/governance/src/http/` — gateway handlers for action cards and receipts

## Audience

Public website visitor. Organizer asking "what does ICN actually give back to a decision?" Developer who needs to know the proof shape exists. Grant reviewer.

## Core question

> *"How does a decision in ICN become a durable institutional fact?"*

## One-sentence message

A proposal becomes a decision, the decision authorizes execution, execution produces effect, and effect produces a receipt — the receipt walks back to the authority that produced it, and that chain is institutional memory.

## Must show

- Five steps: **proposal → decision → execution → effect → receipt**.
- Each step's icon from the canonical symbol family (governance, authority, execution, accounting, provenance).
- A directional connector (`↓` mobile / `→` desktop) between steps.
- A footer line: the receipt is not the end — it becomes part of the record that confers standing and authority for the next round.
- Where the visual names the three currently emitted source paths, do so explicitly: `proposal / vote`, `action_item / complete`, `meeting / attend`. Receipt types: `GovernanceDecisionReceipt`, `ActionItemCompletionReceipt`, `MeetingAttendanceReceipt`.

## Must avoid

- Implying that all five action-card source paths are emitting today. Only three are. The other two are RFC-gated.
- Calling a receipt a "transaction record" or "transaction confirmation."
- Showing the receipt as a wallet entry or a payment confirmation.
- Implying ICN cross-coop federation in the receipt path (the proof loop is documented inside a single cooperative's governance surface; federation receipts are a separate surface).
- Animating the chain in a way that auto-plays on page load.
- NYCN-specific copy or fixture data.

## Canonical copy

From [`ProvenanceTrail.astro`](../../../../website/src/components/ProvenanceTrail.astro). Do not paraphrase.

| # | Step | Gloss |
|---|---|---|
| 01 | Proposal | A member or scope proposes an action. |
| 02 | Decision | The scope decides under its rules. The decision grants authorization. |
| 03 | Execution | The authorized action becomes operational effect. |
| 04 | Effect | The effect is recorded — obligations, allocations, state. |
| 05 | Receipt | A durable receipt ties the effect back to the authority that produced it. |

Intro:

> Every ICN action leaves a trail. The trail is not a log file — it is the chain by which a decision becomes an institutional fact that can be walked back to the authority that produced it.

Footer:

> **Proof flows forward.** The receipt at step 05 is not the end. It becomes part of the record that confers standing and authority for the next round of decisions. Institutional memory is how ICN ties one action to the next.

## Visual grammar

- **Extends** the canonical `ProvenanceTrail.astro`. New variants — annotated proof-loop figure, action-card-to-receipt walkthrough, deck slide — derive from that component.
- Numbered steps, framed icons, directional connectors.
- Mobile-first stack with `↓` connectors; the Astro component currently stacks vertically at every viewport for narrow prose containers.
- Token-grounded.
- Where a variant names specific endpoints (`/v1/gov/me/action-cards`, `/v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`), the strings must come verbatim from the runtime-surface map and the gateway code.

## Accessibility requirements

- `aria-label` on the figure: *"The provenance trail: how a proposal in ICN becomes a durable institutional receipt."*
- Each step readable as text alone — number + label + gloss.
- Icons `aria-hidden="true"` when paired with text.
- Connector arrows `aria-hidden="true"` and `role="presentation"`.
- Grayscale-safe.
- WCAG 2.2 AA contrast in both themes.
- No motion required; hover transition does not gate meaning.

## Review checklist

Run [VISUAL_REVIEW_CHECKLIST.md](../VISUAL_REVIEW_CHECKLIST.md). Per-brief notes:

- §1 source grounding — endpoint strings verified verbatim against the runtime-surface map and the gateway code; **do not** cite from memory.
- §5 substrate honesty — three of five source paths if all five are shown without label; otherwise label `future-state / roadmap` for the gated paths.
- §3 vocabulary — guard against "transaction" / "payment confirmation" drift.

## Status

`gate-open`. Brief gate satisfied. Source-asset variants beyond the existing `ProvenanceTrail.astro` may be built when needed.
