---
Status: draft spec / steward review surface
Canonical: no
Authority: architecture / Tool Commons planning (downstream demand signal; not yet normative)
Last Reviewed: 2026-07-08
---

# Governed Bridge Steward Review Surface

> **Status: draft spec, steward review surface.** Models the **minimum** steward-facing
> review surface a governed bridge import must present before it may turn a
> dry-run proposal into real ICN custody writes. It is the "Steward review surface
> (#2369)" that both the `BridgeReviewDecisionReceipt` in
> [`governed-bridge-receipts.md`](governed-bridge-receipts.md) (#2370) and the
> binding's `steward_review_surface` field in
> [`governed-bridge-service-binding.md`](governed-bridge-service-binding.md)
> (#2372) point at. It is best modeled as a **bridge-review affordance within the
> Steward Cockpit** ([`steward-cockpit-v0.md`](steward-cockpit-v0.md)), not a new
> cockpit — every steward action still runs through the cockpit's mandate /
> authority / receipt envelope, under its "stewardship-not-domination" principle.
> Derived from the NYCN airlock requirements note
> ([`../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md`](../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md)),
> tracking ICN issue #2369. It implements no cockpit, no shell, and does not imply
> any bridge can import real rows today. The PR introducing this doc advances
> #2369 without closing it.

## 1. Purpose

A governed bridge never writes real rows on its own say-so: it produces a
dry-run proposal and yields to a human steward, who authorizes (or refuses)
custody. This document models the smallest set of things that steward must be
able to **see** and **decide** for that authorization to be meaningful, and ties
each to the receipt vocabulary (#2370) so a review leaves verifiable evidence.

It answers:

> What must a steward be able to see and decide before a governed bridge may turn
> a dry-run proposal into actual ICN custody writes?

This is **docs/spec planning only** — a review-surface model, not an
implementation. It defines no cockpit behavior, no endpoint, no wire schema (the
tables below are illustrative), and asserts nothing about a running system.

## 2. Review doctrine

> The bridge proposes custody; the steward authorizes custody.
> A dry-run is evidence, not authority.
> A steward decision is not a target write.
> A receipt proves a decision happened, not that the decision was wise.

And the design north star:

> The review surface must make the safe decision easy and the unsafe decision
> impossible.

The steward is not a rubber stamp and not a ruler: the surface renders
obligations, boundaries, and evidence, and constrains the decision space so that
refuse-by-default holds and no write escapes review.

## 3. Minimum review inputs

What the surface must show. Raw values are the exception, not the default.

| Input | What the steward sees | Why it is required | Privacy boundary | Receipt relationship | Status |
| --- | --- | --- | --- | --- | --- |
| dry-run plan hash / id | the exact proposal under review | the decision binds to this exact set | id/hash only | cited by `BridgeReviewDecisionReceipt` + `BridgeImportReceipt` | Planned |
| source system label | which external system | to weigh source authority | generic label | scopes provenance | Planned |
| source authority class | who asserts the fields | classification precedes custody | class, not identity | — | Planned |
| source record refs | **opaque/hash-bound** references | per-`(record, field)` coverage | never a raw PII key | bound into the reviewed set | Planned |
| field paths / field classes | which fields, of which class | to decide per field | path/class, not raw value | reviewed set | Planned |
| proposed custody target | vault scope / registry namespace / governed object | to authorize a destination | target id | target write receipt | Planned |
| privacy class | care / sponsor / finance / public / … | to gate exposure | drives §7 | — | Planned |
| proposed receipts | the receipt set the run would emit | to see the evidence contract | — | the run's required set | Planned |
| policy blocks | automatic blocks that fired | to see what was refused by policy | — | `ConsentPolicyBlockReceipt` / `PublicationConsentBlockReceipt` | Planned |
| gate fields | permission flags consumed as gates | gates authorize, never publish | consumed, never shown as published | — | Planned |
| external reference observations | observed external status/reference | to weigh freshness (§10) | opaque ref | `ExternalReferenceObservationReceipt` | Planned |
| export/delete/recovery path | the reversibility path per target | custody must be reversible | ref only | referenced by `BridgeImportReceipt` | Planned |
| refusal policy result | what the refusal policy caught | to confirm fail-closed | — | policy-block / discard receipts | Planned |
| prior review / supersession status | whether this supersedes an earlier decision | to avoid double-import | id refs | prior `BridgeReviewDecisionReceipt` | Planned |

Rules:

- **never show raw PII-bearing source keys by default** — references are
  opaque/hash-bound;
- **raw field values only** if the target privacy policy allows *and* the reviewer
  has the authority for that class (§7);
- **gate fields may be consumed as gates, never published** (a permission flag is
  not a display field).

## 4. Required steward decisions

Each decision leaves its own evidence. "Must not imply" guards against a decision
overclaiming.

| Decision | Meaning | Allowed when | Required receipt | Must not imply |
| --- | --- | --- | --- | --- |
| approve | authorize the proposed custody writes | all gates hold; reviewer has authority | `BridgeReviewDecisionReceipt` (+ the run's write receipts follow) | that the write already happened, or that the decision was wise |
| reject | refuse the proposed writes | any time | `BridgeReviewDecisionReceipt` | that the source is invalid — only that ICN will not write it |
| hold | defer pending more information | e.g. ambiguous class, stale observation | `BridgeReviewDecisionReceipt` (decision = hold) | approval; a hold is not a write |
| block | enforce a policy boundary | a policy condition applies | a policy-block receipt (§8) when automatic; `BridgeReviewDecisionReceipt` when the steward blocks | human review, when the block was automatic |
| discard | drop a field/record from the repo-safe path | free-text / sensitive / unmapped | `DiscardDecisionReceipt` (basis = review) | that the content was stored |
| request re-observation | ask for a fresh external observation | external reference is stale (§10) | a new `ExternalReferenceObservationReceipt` | that the old observation is now true |
| request reclassification | ask that a field's class be corrected | class looks wrong | `BridgeReviewDecisionReceipt` (decision = hold, reason = reclassify) | approval of the current class |
| request member consent review | route a consent question to the member surface | a member-consent question exists | (routes to the future member surface, §11) | that the steward decided the member's consent |

## 5. Role display vs authority proof

- the surface **may display role labels** (`summit_data_steward`, …) for legibility;
- the **receipt must bind a verifiable reviewer authority reference** — an
  `actor_did` / signature / authority basis, per existing receipt patterns;
- a **role label alone is not evidence** — a bare role string can be asserted by
  anyone;
- **personal names are not needed** in repo-safe or spec examples — role-only for
  display;
- **identity may be redacted or audience-scoped** in rendering (a member need not
  see the steward's DID);
- **authority proof must be machine-verifiable later** — the display is for
  humans, the binding is for verification.

This matches the `BridgeReviewDecisionReceipt` rule: "role-labeled for display,
never a bare role string as the only evidence."

## 6. Field coverage and reviewed-set binding

- decisions **bind to source-record refs + field paths, or a dry-run plan hash**
  that commits to the exact reviewed set;
- **never a bare field-name set** — a shared name (e.g. `email`) across records is
  a false-pass trap;
- coverage is **per `(record, field)`**;
- the decision must state, for **every** dry-run input field, whether it was
  approved, rejected, held, blocked, discarded, or sent to consent review;
- **no silent gaps** — an unaddressed field is a failure of the review, not a
  default-allow.

## 7. Care / accessibility and sensitive-field boundaries

- care, accessibility, and private-organizer data require **need-to-know**
  review;
- the surface must **avoid over-rendering sensitive values** — a steward can often
  authorize custody of a care-restricted field from its class and target without
  seeing the raw value;
- review can **authorize custody without exposing the full value** where possible;
- **member-facing rendering is separate and privacy-bounded**
  ([`member-shell-v0.md`](member-shell-v0.md));
- **receipt refs must not leak private data** — refs are opaque/hash-bound.

## 8. Publication and consent boundaries

- publication permission flags are **gate fields** (consumed, never published);
- **no publication permission** produces a `PublicationConsentBlockReceipt`;
- **no consent** produces a `ConsentPolicyBlockReceipt`;
- **policy blocks are automatic** and **must not masquerade as human review** — an
  automatic block never carries a `BridgeReviewDecisionReceipt`;
- **member consent review** is a separate, future surface (§11), distinct from
  steward review.

## 9. Action-card and follow-up visibility

- **action cards are derived read views** ([ADR-0027](../adr/ADR-0027-action-card-contract.md)) —
  `GET /v1/gov/me/action-cards` has no mutation API;
- **steward review does not create or write an action card** directly;
- an **approved follow-up creates or authorizes the underlying governed object**,
  from which a card is later derived;
- **card visibility appears only after the underlying object exists** and policy
  allows rendering;
- evidence uses the receipt vocabulary's **provisional underlying-object receipt**
  naming (`FollowUpObjectCreationReceipt`), never a card-write receipt.

## 10. External references in review

- a steward **may see an external reference observation**
  ([`governed-bridge-external-references.md`](governed-bridge-external-references.md));
- an observation is **not proof the external fact is eternally true** — it is
  dated evidence that ICN observed a reference;
- the steward may **approve custody of an observation, request re-observation,
  hold for freshness, or reject as stale**;
- **no settlement / payment processing** — external settlement is observed, never
  processed.

## 11. Member-facing consent review relationship

- **steward review is institution-side custody authorization**;
- **member-facing consent review is a separate, future surface** — a member
  answering "may we keep / publish / follow up on this?" is not the same act as a
  steward authorizing custody;
- the member surface **must not expose private organizer data**;
- a member may see **simplified / opaque references**, never raw internal ids;
- **this doc does not model member consent review** — it only names where steward
  review hands a consent question off.

## 12. Non-goals / non-claims

- no runtime implementation; no Steward Cockpit implementation; no Member Shell
  implementation; no new API;
- no production, pilot-readiness, or live-federation claim; no deployed bridge
  behavior;
- no raw Drive import; no live sync;
- no private data;
- no payment-processing / wallet / token / cryptocurrency framing (external
  settlement is *observed*, never processed);
- no claim that current NYCN operations are ICN-native.

## 13. Open questions

1. Does the steward review surface live in the **Steward Cockpit**, in a **Member
   Shell admin mode**, or in a **bridge-specific cockpit panel** consuming the
   cockpit envelope?
2. How are reviewer authority references **signed and displayed** (DID + signature
   vs an authority-basis credential), and how is the display redacted per audience?
3. **How much field-value visibility** is actually necessary for a sound review,
   per privacy class?
4. How are **care / accessibility fields** reviewed without overexposure — class +
   target only, a redacted preview, or a need-to-know reveal?
5. How does a steward **request re-observation or reclassification** — an inline
   action, or a routed request to the bridge?
6. How does **member consent review** feed back into steward review (blocking,
   advisory, or parallel)?
7. What is the **minimum fixture** needed to test that a review covers every
   dry-run input field per `(record, field)` (akin to the NYCN airlock fixture
   validator)?

---

_Provenance: derived from the NYCN airlock lane (NYCN #85/#86/#88/#89), the ICN
requirements note (#2364), the receipt vocabulary (#2370), the ToolManifest modes
(#2371), the binding custody map (#2372), and the external reference model
(#2373), tracking ICN issue #2369. A review-surface model, not an implementation
or deployment claim. #2369 stays open — referenced, not addressed by this doc._
