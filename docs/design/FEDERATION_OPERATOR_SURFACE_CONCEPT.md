---
Status: operational
Canonical: no
Owner: Matt Faherty
Last Reviewed: 2026-05-19
Last Updated: 2026-05-19
Purpose: Illustrative-direction concept for a federation / operator surface. Stewardship surface, not cockpit. Not shipped. Not a claim to a live federation. Promotes the v0.2 Claude Design seed's federation-operator concept into the canonical design subsystems index for review.
---

# Federation / Operator Surface — Concept

> **Truth label:** *illustrative direction.*
>
> This surface is **not shipped**. The kernel hooks for federation reconciliation, scope-health probes, and receipt emission exist in [`icn-federation`](../../icn/crates/icn-federation), [`icn-trust`](../../icn/crates/icn-trust), and [`icn-net`](../../icn/crates/icn-net); the unified operator-facing surface that renders them does not. This concept is the design exploration the ICN repo tracks as "behind the system" maturity for federation-operator work.
>
> Promoted from the v0.2 Claude Design seed (`seed/FEDERATION_OPERATOR_CONCEPT.md`) per [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md).

## What this surface is

A **stewardship surface.** The operator of a federation runs reconciliations, acknowledges degraded scope states, re-emits stale receipts, and processes the federation's own action queue. They do *not* hold private member data, do *not* see member positions inside cooperatives, and do *not* have powers the federation charter did not grant.

The vocabulary is `operator` and `steward`, not `admin`. The role is bound by the federation's charter; it is not an escalation path.

## What this surface is not

| Anti-pattern | Why ICN rejects it |
|---|---|
| **Cockpit** | A cockpit centralizes attention and authority on one operator. Federations are co-equal scopes; the surface is mostly for *seeing*, not steering. ([`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 7) |
| **Surveillance console** | Operators see no more than members see on shared records. No drill-down into private member state. |
| **Admin god-panel** | Federations do not have admins. They have stewards bound by the federation charter. |
| **"Control center" aesthetic** | Glowing world maps, pulsing nodes, stats walls — reject. The surface looks like a member surface, scoped up. ([`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 8) |
| **Glowing network globe** | Three co-equal scopes, no hub, no animated network theater. ([`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 9) |
| **Faux-government seal** | No round seals, no laurels, no shield motifs. ([`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 10) |

## Surface inventory

The concept renders five regions, all in the same visual language as the member shell — same tokens, same primitives, same vocabulary.

### 1 · Member roster (federation health)

A list of every member coop / community participating in this federation, with three columns of state per row:

- **Scope health** — chip: healthy · degraded · partition. Text + icon, never color-only.
- **Last seen** — when the federation last successfully reconciled with this scope.
- **Action state** — sync chip: synced · pending · queued.

The roster is the only surface where the operator sees aggregate-by-scope state. Color reinforces; chip text names the state.

### 2 · Stewardship panels (receipt health · reconciliation queue · disputes)

Three small panels with one number and one sentence each:

- **Receipt health** — emitted-vs-expected over the last 7d, plus stale / re-emitted count.
- **Reconciliation queue** — scopes waiting to reconverge.
- **Disputes open** — disputes raised at federation scope (not coop-internal).

No vanity metrics. Every number is a thing the steward might need to act on.

### 3 · Operator action queue

The steward's work surface. A list of named operator actions, each:

- An **operator action card** rendered against [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) §"ActionCard rendering contract" and its "v0.2 rendering refinements" — same 14-field ADR-0027 schema, same sync-state chip, same "unavailable" rendering rule, same stale/degraded scope behavior. Operator cards inherit the member shell's contract; they are not a parallel surface.
- **Mandate column** names the federation-charter clause that authorizes the steward to perform the action. Operators cannot perform actions that no charter clause authorizes.

### 4 · Provenance ledger (federation-scope receipts)

A receipt strip rendering federation-scope receipts only:

- Cross-coop clearings.
- Federation-level governance decisions.
- Federation-level allocations.
- Charter changes.

Coop-internal receipts do not appear on the operator surface. They are visible to the coop's members through their own member shells.

### 5 · "Can see / cannot see" disclosure

Every operator surface carries an explicit visible disclosure of what the operator *can* and *cannot* see at this scope. This is doctrine, not chrome. A federation operator who cannot remember what they cannot see is dangerous; the surface reminds them every screen.

### 6 · Member-visible degradation correspondence

When the federation enters degraded mode, the operator surface and every member shell affected by it render the **same banner copy** with the same remediation expectations. Members and operators read the same words. This preserves the "operators see no more than members" rule even during failure.

The shared banner is the contract in [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) §"Stale, sync-in-flight, degraded scope (network honesty)" — the operator surface does not invent a parallel degradation vocabulary.

## Component primitives the concept reuses

- **Scope frame** — every panel declares it lives at federation scope.
- **Mandate line** — every operator action card names the charter clause that authorizes it.
- **Sync-state chip** — same six states as the member shell ([`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) §"v0.2 rendering refinements").
- **Provenance trail** — operators inspect the same chain members can.
- **Truth label** — persistent strip on the surface.
- **Degraded banner** — same banner copy as members see.
- **Challenge / review / exit** — disputes can be raised from the operator surface against federation-scope decisions; the dispute path is the same path a member uses.

These are pattern names from the design system, not canonical concept ids ([`docs/design-language/concept-map.md`](../design-language/concept-map.md)) — they remain design-system patterns per the v0.2 seed's D3 decision.

## Privacy boundaries

| Boundary | Mechanism |
|---|---|
| Operator cannot see member positions inside coops | Federation-scope receipts only · no drill-down to coop-internal mutual-credit positions |
| Operator cannot see coop-internal proposals | Federation queue surfaces only federation-charter-bound proposals |
| Operator cannot see member communications | No communications visible at any scope above the participants' own |
| Operator cannot escalate beyond charter | Every operator action declares the charter clause; actions without a clause are not offered |
| Operator cannot suppress dispute paths | "Challenge" affordance reachable on every operator action card; the dispute resolves at the appropriate scope, never at the operator's discretion |

These boundaries are doctrine for the concept. The kernel's scope-isolation guarantees are the ground truth; any implementation must verify against them before claiming this surface as shipped.

## Audit / export paths

- Every federation-scope receipt is exportable by any member of the federation, not only by the operator. Operators do not have privileged export access.
- The operator action queue is itself receipt-generating; the federation can audit the operator the same way the operator audits the federation.

## Accessibility / language obligations

- **Truth-label strip** persistent across every operator screen.
- **Sync-state chips** named, never color-only.
- **Scope health chips** named, never color-only.
- Renders correctly in all 4 themes × 2 modes — the production wiring for `hc-dark` / `hc-light` and `data-mode` attributes remains a separate architecture decision per the v0.2 seed's D1 and D2 decisions; the design assumes them.
- Long-string locales (German, Finnish) — every roster row wraps, every chip flexes.
- RTL flow — chip order reverses with `dir="rtl"`; roster columns flip.
- Language selector — explicit, member-chosen, persists per-device.

## Rejected pattern notes (specific to this surface)

| Reject | Use |
|---|---|
| Map view with glowing nodes for each federation member | Tabular roster with named scopes, health chips, last-seen timestamps |
| Pulsing "live" indicator | Static "last reconciled HH:MM ago" timestamp |
| "Active users" or "online members" count | No member-presence count at federation scope; presence is not a federation concern |
| "Force sync" / "Override" buttons | Reconciliation runs are receipt-emitting actions, not overrides. Every operator action is bound to a charter clause. |
| Operator-only chat / messaging | No member communications visible at any scope; operator-only channels are out of scope |

These render as additions to [`ICN_VISUAL_EXPLAINER_BIBLE.md` Appendix B](ICN_VISUAL_EXPLAINER_BIBLE.md#appendix-b--rejected-patterns-side-by-side-comparison) for the federation surface specifically. Reviewers can cite this section when blocking a future production-implementation PR.

## Claude Code handoff note

This concept is **docs-only material**. Do not implement the surface from this file. Per [`CLAUDE_DESIGN_REVIEW_PROTOCOL.md`](CLAUDE_DESIGN_REVIEW_PROTOCOL.md), production implementation is a downstream PR with its own scope and review gates. The truth label `illustrative direction` carries through every render of any artifact derived from this concept.

## Promotion gate (before any production-implementation follow-up)

Before any PR that ships a real federation operator surface lands:

- Federation operator role definition exists in [`docs/design-language/concept-map.md`](../design-language/concept-map.md) or in an ADR.
- Privacy boundaries cross-referenced against the kernel's actual scope-isolation guarantees in [`icn-federation`](../../icn/crates/icn-federation) and [`icn-trust`](../../icn/crates/icn-trust).
- Truth label `illustrative direction` carries through every render until end-to-end implementation is verified.
- No screenshots ship as "current state of federation operator UI" — per [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) item 11.
- Reviewers cite the "operator sees no more than members" rule when evaluating any future implementation PR.

## See also

- [`ICN_DESIGN_SYSTEM.md`](ICN_DESIGN_SYSTEM.md) — design subsystems index (row 16)
- [`ICN_VISUAL_EXPLAINER_BIBLE.md` Appendix B](ICN_VISUAL_EXPLAINER_BIBLE.md#appendix-b--rejected-patterns-side-by-side-comparison) — rejected patterns
- [`MUST_NOT_SHIP.md`](MUST_NOT_SHIP.md) items 7 (dashboard), 8 (AI sheen / glassmorphism), 9 (glowing-globe / central-hub), 10 (faux-government seal), 11 (screenshots as proof)
- [`docs/spec/member-shell-v0.md`](../spec/member-shell-v0.md) — ActionCard rendering contract the operator queue inherits
- [`docs/design-language/concept-map.md`](../design-language/concept-map.md) — canonical concepts
- [`CLAUDE_DESIGN_REVIEW_PROTOCOL.md`](CLAUDE_DESIGN_REVIEW_PROTOCOL.md) — gates that govern promotion
- [`claude-design-seed/CHANGELOG.md`](claude-design-seed/CHANGELOG.md) — seed identity and provenance
