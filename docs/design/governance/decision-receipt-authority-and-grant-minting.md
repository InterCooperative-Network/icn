# GovernanceDecisionReceipt Authority + Grant-Minting — Design

> **Status:** draft for review (design-only). No code in this PR.
> **Issue:** #1868 (governance:write decomposition / mandate layer).
> **Companion docs:** [`governance-write-decomposition.md`](./governance-write-decomposition.md) (§6, §10, §12 Q4), [`mandate-gate-design.md`](./mandate-gate-design.md); ADR-0014 (AuthorityGrant non-conflation).

## 0. Non-claims

<!-- truth: descriptive -->

- This is a design draft, not an implementation. No receipt is emitted, no grant is
  minted, no handler is wired by this PR.
- No `GovernanceDecisionReceiptV2` production emission. No `MandateGate::require()` handler
  wiring. No grant-minting code. No `TypedScope` Federation/Role binding code. No
  `governance:write` retirement.
- No production-readiness, live-federation, or demo-ready claim.

## 1. Problem

Low-blast membership-standing receipt emission is live — `MeetingAttendanceReceiptV2`
(#1934) and `ActionItemCompletionReceiptV2` (#1935), both
`NoMandateRequired { MembershipStandingOnly }`, persisted via the fail-closed
`put_opaque` route. The next dormant receipt is `GovernanceDecisionReceiptV2` (schema from
#1929), but **its honest production emission is blocked on undecided authority semantics**,
and `MandateGate` handler wiring is blocked on a missing grant/target substrate. This doc
decides the authority model so the follow-up code lands honestly — no fake
`NoMandateRequired`, no always-reject theater.

## 2. Current facts (verified against `ae7fc00a`)

<!-- truth: descriptive -->

- **Decision-producing paths** are the proposal close/accept path: standalone manager
  close (`icn/apps/governance/src/manager.rs`), actor-backed close
  (`icn/apps/governance/src/actor.rs`), and the **forced-accept** path (same `actor.rs`,
  with forced provenance in `icn/apps/governance/src/lib.rs`).
- **Forced-accept is an explicit administrative override** of vote thresholds
  (`VoteTally::empty()`, no votes); the receipt still verifies because its hash is derived
  from those exact inputs.
- **Production grants** are minted only for `AppointSteward` / `ReinstateSteward` /
  `ReconfirmSteward` (`grant_minting.rs`), **all `AuthorityClass::Attestation`**.
  `mandate_gate.rs::expected_class` maps every wired `MandateAct` to `Execution` (or
  `CastVote` → `Representation`); `Attestation` "never authorizes acts gated here". **No
  minted grant authorizes any gated act today.**
- **`MandateGate` targets**: `Domain` (actor-first) and `Proposal` validate; **`Federation`
  (raw `String`; no `FederationId` type) and `Role` (`StructureId` + holder) always return
  `WrongTarget`** — `TypedScope` cannot bind them.
- **Taxonomy versioning**: `NoMandateReason` and `ReceiptMandateAttestation` are
  `#[non_exhaustive]`; `NoMandateReason`'s own note says adding a variant "requires a new
  domain-separation tag on the receipt that consumes it (`:v3`)". The existing v2 taxonomy
  is exactly `{ NoMandateRequired { MembershipStandingOnly | Bootstrap }, Grant }`.

## 3. The three authority modes

<!-- truth: doctrine -->

A receipt's `ReceiptMandateAttestation` must name the *actual* authority mode. There are at
least three; they must not be collapsed.

1. **Bootstrap override** — forced-accept / administrative override that bypasses vote
   thresholds. Attestation: `NoMandateRequired { Bootstrap }`. **Fits the current v2
   taxonomy.**
2. **Process / tally authority** — a normal proposal close where the **governance process
   itself** authorizes the outcome: eligible voters, voting period, quorum/threshold,
   tally, proposal scope, and outcome rules. This is **not** membership standing —
   membership standing authorizes *participation and low-blast record-keeping* (attendance,
   action-item completion), it does not by itself authorize an institutional decision. It
   is also **not** a personal grant. The current taxonomy has **no honest variant** for
   this mode (§4).
3. **Grant-backed execution** — a discrete actor authorized to *perform* a high/medium-blast
   act (charter/federation/steward execution). Attestation: `Grant { grant_ref }`. Blocked
   until Execution-class grants exist and the Federation/Role target-binding gap is solved.

### 3.1 Decision closure is not payload execution

A `GovernanceDecisionReceipt` records a **decision outcome** (closure / tally
certification). That outcome can be **process-authorized with no personal grant**.
**Executing** the accepted payload (e.g. activating a charter, joining a federation,
seating a steward) is a *separate* authority path that may require a grant-backed mandate.
Do not conflate the two: a decision receipt may carry a process-authorized attestation
while a downstream execution receipt/act carries `Grant { grant_ref }`.

## 4. Recommendation: a process-authorized attestation, on a v3 decision receipt

<!-- truth: prescriptive -->

**Add a positive "process-authorized" attestation mode and emit it on a new
`GovernanceDecisionReceiptV3`.**

- **Shape (recommended):** a distinct `ReceiptMandateAttestation` variant — e.g.
  `ProcessAuthorized` — rather than a new `NoMandateReason`. Rationale: a democratic
  decision is *positively authorized by process*, so labelling it `NoMandateRequired`
  ("no authority needed") mislabels it — exactly the conflation this doc forbids. The
  decision receipt already binds the vote tally and votes in its base hash, so the
  attestation only needs to *name* the mode (it need not re-encode the tally).
  - *Alternative (lower-churn):* `NoMandateReason::ProcessAuthorized` under the existing
    `NoMandateRequired` arm. Rejected as primary because it reuses the "no mandate" framing
    for a positively-authorized act. Either way the choice is an ADR-level commitment.
- **Versioning:** extending either taxonomy for a decision triggers the `:v3` rule — the
  decision receipt that *uses* the new mode must be `GovernanceDecisionReceiptV3`
  (`icn:gov:decision:v3`), fresh-hashed over its base fields + the new attestation, never
  derived from v2. Existing v2 receipts (meeting/action-item, and decision-v2 if/when used
  for `Bootstrap`) are unaffected because they never emit the new ordinal.

## 5. Per-path answers

<!-- truth: prescriptive -->

| Decision path | Authority mode | Honest attestation | Emit when |
|---|---|---|---|
| Forced-accept (admin override) | Bootstrap | `NoMandateRequired { Bootstrap }` | **Now** — fits v2 taxonomy |
| Normal democratic close (vote authorizes outcome) | Process/tally | `ProcessAuthorized` (new) | After §4 lands → **v3** |
| Close whose *outcome triggers/represents execution authority* (charter/federation/steward) | Grant-backed execution | `Grant { grant_ref }` on the **execution** receipt/act | Blocked on Execution grants + target binding |

Explicit answers to §12 Q4 and the review questions:
- **Which v2 paths emit immediately:** only **forced-accept → `Bootstrap`**.
- **Which need a new process-authorized mode first:** **all normal democratic closes** —
  they must not be emitted as `MembershipStandingOnly`, and the current taxonomy has no
  honest variant, so they wait for §4 (→ v3).
- **Which wait for `Grant`:** the **execution** of accepted high/medium-blast payloads
  (charter/federation/steward) — a separate receipt/act, not the decision closure itself.
- **Must `NoMandateReason`/attestation be extended before normal-close v2 emission?** Yes —
  and extending it for a decision means a **v3** decision receipt, not v2.
- **Mixed attestation across the chain?** Yes: the decision receipt may carry
  `ProcessAuthorized` while the downstream execution receipt carries `Grant`.

## 6. Grant-minting + target-binding gaps (for the execution path)

<!-- truth: descriptive/prescriptive -->

Before any `Grant`-backed decision/execution attestation or `MandateGate` handler wiring is
honest:
- **Execution-class grant minting** (decomposition Q7–Q10): decide which proposal classes
  mint `AuthorityClass::Execution` grants, the **grantee** (named person/role), a truthful
  **term/expiry** (fail-closed on overflow, per the `AppointSteward` precedent in
  `grant_minting.rs`), and the **target**. `derive_grants_for_accepted_proposal` is the
  extension point. None exist today.
- **Federation/Role target binding** (Q11–Q12): a `TypedScope` binding surface (or a
  `(domain, act, target) → mandate_id` index) so `Federation`/`Role` stop returning
  `WrongTarget`. Federation also lacks a `FederationId` type (raw `String`).

Until both land, `MandateGate::require()` handler wiring is **always-reject theater** and is
deferred.

## 7. Next-code sequencing (after this design RFCs)

1. **Extend the attestation taxonomy** per §4 (new `ProcessAuthorized` mode) — the smallest
   typed change in `icn-governance/src/proof.rs`, plus `GovernanceDecisionReceiptV3` if the
   `:v3` rule is confirmed.
2. **Emit `GovernanceDecisionReceiptV3` (and v2-Bootstrap) for the honestly-attestable
   paths**: forced-accept → `Bootstrap`; normal democratic close → `ProcessAuthorized`.
   Reuse the fail-closed `put_opaque` routing pattern (#1934/#1935); no typed gateway
   import.
3. **Only then** the execution-authority track: Execution-grant minting (§6), Federation/Role
   target binding (§6), and finally `MandateGate::require()` handler wiring.

## 8. Open questions for the ADR

- `ProcessAuthorized` as a distinct attestation variant (recommended) vs a new
  `NoMandateReason` — freeze the shape.
- Confirm the `:v3` rule applies (decision receipt must bump) vs a documented exception.
- Whether `ProcessAuthorized` should carry any process reference (it need not — the tally is
  already in the base hash) or stay a bare discriminator like `Bootstrap`.
- The Execution-grant grantee/term/target shape (§6) — likely its own design slice.
