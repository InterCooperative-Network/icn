---
Status: normative (design contract; implemented + locally tested on branch task/gap-atlas-20260717, NOT merged)
Canonical: no
Current-state source: docs/STATE.md + docs/PHASE_PROGRESS.md
Last Reviewed: 2026-07-17
---

# DomainPolicy adoption receipt + supersession pointer

> **Implementation status (2026-07-17).** Implemented and integration-tested on
> branch `task/gap-atlas-20260717` (uncommitted, not merged): the
> `DomainPolicyAdoptedReceipt` type (`icn-governance::proof`) and its fail-closed
> emission at the persisted adoption boundary
> (`adopt_domain_policy_persisted_with_body`), with `supersedes` resolved from the
> domain's prior adoption receipt via `list_opaque_for(...).last()` and an
> idempotent `put_opaque_if_absent` keyed `(class, domain_id, hex(policy_id))`.
> An integration test adopts v1 then v2 and verifies the resulting supersession
> chain through the verifier above. The receipt records a process fact and grants
> **zero authority**.

> **What this document is.** A design contract for the **one spine transition
> that currently emits no receipt**: a domain adopting (or replacing) its
> `current_policy`. It specifies a new ADR-0026 Layer-2 receipt class,
> `DomainPolicyAdoptedReceipt`, and a `supersedes` back-pointer so "what rules
> governed this domain on date X" becomes mechanically answerable.
>
> **What this document is NOT.** Not an implementation, not a claim that
> adoption receipts exist today, and it lands no code. It does not change the
> authorization model (the adoption gate is unchanged), the CCL evaluation
> question (adopted bodies are still inert — separate gap), or any auth/write
> path beyond adding a receipt emission at an already-authorized transition.

## Problem (evidence, not assertion)

The vertical spine ICN is making real is
`package → domain → policy → binding → process → receipt → surface → evidence`
([`ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md)). Adoption —
a domain deciding which policy governs it — is, by the operating model's own
transition grammar, a first-class governance act (`declare → review → adopt →
bind → …`). It is arguably the **most constitutionally significant** transition
on the spine: it is the moment a rule acquires authority over an institution.

As of `b44a1821` (2026-07-17), that transition leaves no receipt:

1. `apps/governance/src/domain_policy_adoption.rs::adopt_domain_policy` resolves
   the caller's authority through the mandate gate (correct, fail-closed), then
   calls `adopt_domain_policy_gated`, which sets `current_policy` on a
   caller-held `InstitutionalDomain`. **The receipt backend is used only to
   resolve the grant** — no `*Adopted*` receipt is written for the adoption
   itself. (Grep of the module: every `receipt`/`Receipt` reference is either the
   backend used for gate resolution or an error variant name.)
2. `InstitutionalDomain::adopt_policy` sets `current_policy =
   Some(policy.policy_ref())` "replacing any prior policy"; the doc comment is
   explicit that "prior versions are history" — i.e. the previous ref simply
   disappears from the struct. There is **no `supersedes` pointer, no version
   lineage, no adoption history**.
3. Adoption *is* durably persisted — `adopt_domain_policy_persisted_with_body`
   (`domain_policy_adoption.rs:449`) loads the domain, commits the new
   `current_policy`, and persists via `store.save_institutional_domain`, and it
   is wired to the #2142 HTTP adoption route (`apps/governance/src/http/handlers.rs`).
   So the state is stored; what is missing is a **receipt** and a **supersession
   pointer**. The prior policy ref is simply overwritten in the persisted record.
   (An older, non-persisted `adopt_domain_policy` seam operating on a caller-held
   domain also exists; neither seam emits an adoption receipt.)

**Consequences:**
- **Provenance hole.** "Which policy governed domain D when decision X was made?"
  cannot be answered mechanically — the governing rule at a past instant is
  unrecoverable once superseded. This directly undercuts the audit/evidence
  promise for every decision taken under an adopted policy.
- **The spine's center has no proof.** Every other landed rung of the spine
  (session, deliberation, decision, activation, mutation) emits an ADR-0026
  receipt; adoption — the rung that confers authority on a rule — does not.
- **It blocks the evaluator seam.** The parked CCL evaluator-selection runtime
  (needs `(domain, proposal_kind, policy snapshot) → one evaluator`) has no
  durable, receipted "policy snapshot at time T" to select against. The adoption
  receipt is that snapshot's provenance anchor.

## Scope

In scope: (a) a new ADR-0026 Layer-2 receipt class `DomainPolicyAdoptedReceipt`
emitted at the already-authorized adoption transition; (b) a `supersedes:
Option<record_hash>` back-pointer forming an adoption chain per domain; (c)
persisting that receipt through the existing opaque-storage cascade
(`put_opaque`), keyed like the other Layer-2 classes.

Out of scope (explicitly, to keep the scaffold bounded and honest):
- **Evaluating** the adopted policy body — the body stays opaque bytes; the CCL
  evaluator-selection seam is a separate parked gap. This receipt records *that*
  a policy was adopted, not that its rules now execute.
- Durable adoption persistence is **already landed** (the persisted seam
  `adopt_domain_policy_persisted_with_body`, wired to #2142). This contract does
  not re-do persistence; it adds a receipt emission at that existing, already-
  persisted, already-authorized transition.
- Changing the adoption authorization model — the mandate gate is unchanged.

## The receipt class

`DomainPolicyAdoptedReceipt` (ADR-0026 Layer 2, domain-tagged BLAKE3 like its
siblings), canonical fields:

- `domain_id: GovernanceDomainId` — the adopting domain.
- `policy_ref: DomainPolicyRef` — the now-current policy (id + version +
  body content-hash, reusing the existing ref shape).
- `supersedes: Option<Hash>` — the `record_hash` of the prior
  `DomainPolicyAdoptedReceipt` for this domain, or `None` for first adoption.
  This forms a per-domain adoption chain the verifier
  ([`receipt-chain-verification.md`](receipt-chain-verification.md)) can walk.
- `adopted_by: <authority reference>` — the mandate/grant record hash the gate
  resolved (value-withheld: the *authority basis*, not the raw DID, consistent
  with existing value-withheld bindings).
- `adopted_at: Timestamp`.
- `record_hash: Hash` — computed over the above via a `compute_record_hash`
  method mirroring the sibling classes.

No policy *body* is embedded — only the content-addressed `policy_ref`. No
package vocabulary. No DID echoed. Firewall-clean: this is a generic core noun
(`Policy` adoption), not an institution-ceremony word.

## Emission point

Inside the persisted adoption seam `adopt_domain_policy_persisted_with_body`,
**after** the mandate gate passes and `save_institutional_domain` commits the new
`current_policy`, emit the receipt through the same `put_opaque` cascade the
other Layer-2 classes use. The `supersedes` value is the current domain's
most-recent adoption-receipt hash (looked up from the opaque store by
`(class=DomainPolicyAdopted, key1=domain_id)`), or `None`.

Fail-closed ordering: gate → commit → receipt. If the receipt write fails, the
adoption must not be reported as complete (same posture as the other rungs).

## Authorization, receipts, surfaces, custody (feature-placement checklist)

1. **What kind of thing is this?** A generic core noun's transition receipt
   (`Policy` adopted into a `Domain`). Not package meaning, not a new authority.
2. **Where does it belong?** `icn-governance::proof` (the class, beside its
   siblings) + `apps/governance` emission + the gateway opaque store (no new
   store primitive; reuse `put_opaque`). No kernel expansion.
3. **Who authorizes it?** The existing adoption mandate gate — **unchanged**.
   The receipt records the act the gate already authorized; it adds no new
   authority and cannot be emitted on an unauthorized adoption (it lands after
   the gate).
4. **What receipt proves it?** This one — that is the point of the scaffold.
5. **Which surface shows it?** Member "what governs us" / steward "policy
   history" is a natural follow-on; not required for this slice.
6. **Custody/privacy/exit?** No new stored PII (policy refs + value-withheld
   authority basis). The adoption chain is part of the domain's exportable
   evidence, strengthening the export/exit story rather than expanding
   disclosure.

## Acceptance criteria (falsifiable)

1. **Adoption is provable.** After a domain adopts policy v1, an ADR-0026
   `DomainPolicyAdoptedReceipt` exists, keyed by `domain_id`, with
   `supersedes: None`. *Today, no such receipt exists.*
2. **Supersession is a chain, not an erasure.** Adopting v2 emits a second
   receipt whose `supersedes` equals the v1 receipt's `record_hash`; the v1
   receipt is unchanged and still retrievable. "What governed us before v2?" is
   answerable by walking `supersedes`.
3. **Verifier-compatible.** The adoption chain passes the receipt-chain verifier
   (companion contract): each rung's `record_hash` re-derives, and each
   `supersedes` link resolves to a verified prior rung.
4. **Fail-closed.** A gate rejection emits no receipt and does not mutate
   `current_policy` (unchanged from today). A receipt-write failure does not
   report adoption as complete.
5. **No new authority.** A test confirms the receipt grants nothing: presence of
   the receipt does not, by itself, let any actor act — authority still flows
   only through the gate/mandate path.

## Non-claims

- This records *that* a policy was adopted; it does **not** make the policy's
  rules execute (CCL evaluation is a separate, parked gap). Planned ≠ evaluated.
- Not federation, not production, not a pilot; closes no human gate.
- Persistence already exists; this adds only the receipt + supersession chain.

## Related work

- [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (envelope
  + Layer-2 class family); the process-substrate ladder (#1748) and the vertical
  spine control issue (#2141) are the umbrellas this rung sits under.
- Companion scaffold: [`receipt-chain-verification.md`](receipt-chain-verification.md)
  — the verifier that walks this adoption chain.
- Downstream: the CCL evaluator-selection seam (parked under #2141) consumes the
  "policy snapshot at T" this receipt anchors. The persisted adoption seam
  (`adopt_domain_policy_persisted_with_body`, #2142) is the existing emission
  point.
