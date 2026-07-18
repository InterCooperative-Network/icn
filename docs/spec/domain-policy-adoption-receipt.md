---
Status: normative (design contract; implemented + tested in this repository)
Canonical: no
Current-state source: docs/STATE.md + docs/PHASE_PROGRESS.md
Last Reviewed: 2026-07-17
---

# DomainPolicy adoption receipt + supersession pointer

> **Implementation status.** Implemented and integration-tested in this
> repository: the `DomainPolicyAdoptedReceipt` type (`icn-governance::proof`) and
> its emission at the persisted adoption boundary
> (`adopt_domain_policy_persisted_with_body`). The receipt is emitted **after**
> the durable `save_institutional_domain`, so a receipt always corresponds to an
> adoption that durably happened; `supersedes` is resolved deterministically from
> the domain's prior `current_policy` (a keyed `get_latest_opaque` lookup, with
> read/decode failures propagated fail-closed), and the write is an idempotent
> `put_opaque_if_absent` keyed `(class, domain_id, hex(policy_id))`. A no-op
> re-adoption of the already-current policy emits nothing. An integration test
> adopts v1 then v2 and verifies the resulting supersession chain through the
> verifier contract above. The receipt records a process fact and grants **zero
> authority**.

> **What this document is.** The normative design contract for a receipt on the
> domain-policy adoption transition — a domain adopting (or replacing) its
> `current_policy` — via an ADR-0026 Layer-2 `DomainPolicyAdoptedReceipt` with a
> `supersedes` back-pointer, so "what rules governed this domain on date X"
> becomes mechanically answerable.
>
> **What this document is NOT.** It does not change the authorization model (the
> adoption gate is unchanged and still runs before the receipt), the CCL
> evaluation question (adopted bodies are still inert — separate gap), or any
> auth/write path beyond adding the receipt emission at an already-authorized,
> already-persisted transition. A valid receipt proves the adoption was recorded,
> not that it was legitimate.

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
siblings), as implemented in `icn-governance::proof`:

- `domain_id: String` — the adopting domain (`GovernanceDomainId.0`).
- `policy_id: Hash` — the content-addressed id of the now-current policy version
  (`DomainPolicyId.0`, raw 32 bytes; the content hash of the adopted policy).
- `adopted_by: String` — the actor DID that recorded the adoption. This is
  **recorder evidence**, consistent with the sibling ladder classes'
  `recorded_by` field; it is not an authority grant and confers none.
- `recorded_at: u64` — wall-clock seconds the adoption was recorded (hashed into
  `record_hash`).
- `supersedes: Option<Hash>` — the `record_hash` of the prior
  `DomainPolicyAdoptedReceipt` for this domain, or `None` for the first adoption.
  This forms a per-domain adoption chain the verifier
  ([`receipt-chain-verification.md`](receipt-chain-verification.md)) can walk.
- `record_hash: Hash` — `compute_record_hash` over the fields above
  (domain-separation tag `icn:gov:domain-policy-adopted:v1`; a 1-byte
  discriminant distinguishes `supersedes: None` from `Some(all-zero)`).

No policy *body* is embedded — only the content-addressed `policy_id`. No package
vocabulary. Firewall-clean: this is a generic core noun (`Policy` adoption), not
an institution-ceremony word.

## Emission point

Inside the persisted adoption seam `adopt_domain_policy_persisted_with_body`,
**after** the mandate gate passes and `save_institutional_domain` durably commits
the new `current_policy`, emit the receipt through the `put_opaque_if_absent`
cascade the other Layer-2 classes use, keyed `(class=DomainPolicyAdopted,
key1=domain_id, key2=hex(policy_id))`.

Ordering is **gate → commit → receipt** (emit *after* the durable save). This
guarantees a receipt always corresponds to an adoption that durably happened — a
receipt never over-claims. The weaker alternative (emit-before-save) can leave a
durable receipt for an adoption that failed to persist, which is a lying receipt
and worse for an evidence system.

`supersedes` is resolved deterministically from the domain's **prior
`current_policy`** (captured before adoption overwrites it): a keyed
`get_latest_opaque(class, domain_id, hex(prior_policy_id))` lookup. Read and
decode failures are **propagated (fail-closed)**, never silently degraded to a
genesis link (which would corrupt the chain). A prior policy that predates this
feature has no receipt → the chain legitimately starts there. Re-adopting the
already-current policy (`prior == new`) is a no-op and emits nothing.

Known, bounded limitation: because the domain store and the receipt (opaque)
store are not a single transaction, an emission failure *after* a successful save
returns an error with a durable adoption whose receipt is momentarily missing.
This missing-receipt failure mode is deliberately chosen over the lying-receipt
mode; automatic re-emission/backfill is future work.

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
   `supersedes: None`. *Previously, no such receipt existed.*
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
