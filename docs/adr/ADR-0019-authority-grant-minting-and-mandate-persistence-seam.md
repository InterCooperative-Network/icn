---
id: "0019"
title: "Authority Grant Minting and Mandate Persistence Seam"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["governance", "authority", "mandate", "grant-minting", "decision-seam", "constitutional-memory"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partially implemented (minting + persistence landed; enforcement gating not yet)"
references:
  - "ADR-0014 (Constitutional Object Model — semantic source)"
  - "ADR-0018 (ADR Lifecycle — for the decision-vs-implementation status separation)"
  - "icn/crates/icn-governance/src/authority.rs"
  - "icn/crates/icn-governance/src/mandate.rs"
  - "icn/apps/governance/src/grant_minting.rs"
---

# ADR 0019: Authority Grant Minting and Mandate Persistence Seam

## Status

**Accepted (2026-04-26).** This ADR records the *seam* — the deterministic translation from accepted decisions into persisted constitutional-memory records — that already exists in code. ADR-0014 froze the *semantic model* (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`); this ADR records what code does with those types at the accepted-decision boundary.

**This is not an enforcement-gate decision.** The seam mints and persists records. Whether kernel dispatch consults those records to gate execution is a separate, future decision (see "Non-decisions" below).

## Context

ADR-0014 introduced four types — `AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate` — as the constitutional-memory primitives. It explicitly deferred behavior to a later tranche:

> "This ADR introduces no behavior changes, no new code paths, no new crate public surface."  
> — ADR-0014, Status section

The behavior tranche has since landed. Code now sits at the accepted-decision boundary and emits constitutional-memory records when a proposal passes. The shape of that emission — what gets minted, who is the grantor, what falls back to a pending-grants mandate, and what *never* happens — is not currently recorded as a decision. This ADR records it.

The need is acute because **silent broadening of grant minting is the canonical institutional-capture failure mode.** "We accept the proposal, therefore we mint the grants its sponsor would like to have minted" is exactly the move ADR-0014's distinct-by-construction `AuthorityClass` enum exists to prevent. The seam recorded here is deliberately conservative; this ADR makes the conservatism load-bearing.

## Decision

When the governance app processes an accepted proposal, the seam in `icn/apps/governance/src/grant_minting.rs` produces:

1. **Zero or more `AuthorityGrant`s, minted only when the accepted payload truthfully carries the data.** The grantor is always the sovereign entity behind the accepted decision (the governance domain). The platform, the runtime, the gateway, and any shared service are *never* grantors. Scope is populated only with categories the payload unambiguously supplies — no invented action kinds, ceilings, or durations.

2. **Exactly one `Mandate`, persisted as durable institutional memory.** A mandate binds a decision to a bounded authorization. If the seam can derive a coherent grant set from the payload, the mandate composes those grants. If it cannot — because the proposal class is not yet in the supported set, or because the payload is malformed in a way that would force fabrication — the mandate is recorded via the bootstrap-phase `Mandate::new_pending_grants` constructor instead. The mandate exists; the grants are explicitly pending.

3. **Zero kernel-dispatch behavior change.** This seam produces records. It does not authorize executors, does not gate dispatch, and does not change effect semantics. `KernelEffect` dispatch continues to run under the constraints already enforced by the kernel.

### Conservative-broadening invariants

- **No fabricated grants.** A grant is minted only when the accepted payload directly names the grantee, the class, and a truthful time bound. If any of those is absent, the mint returns the empty set for that proposal.
- **`new_pending_grants` is the truthful fall-through, not the easy way out.** Recording a pending-grants mandate is *more* honest than recording a "synthetic grant for everything" mandate. The institutional-memory record reads "we decided this; the authority shape is not yet expressible," which is what is actually true.
- **The grantor is never the platform.** The grantor field is bound to the governance domain that decided the proposal. There is no provision for "ICN granted X to Y." Authority flows from sovereign entities, not from runtime.
- **The supported proposal-class set is small on purpose.** Today only steward-appointment and steward-reconfirmation proposals mint grants (Attestation class). Expanding this set is explicit future work via additional ADRs; silently broadening it is forbidden.

### Implementation-status separation

Per ADR-0018, decision lifecycle is independent from implementation status. The decision recorded here is `accepted`. The implementation status is `partially implemented`:

| Layer | Status |
|---|---|
| `AuthorityClass` / `AuthorityGrant` / `TypedScope` / `Mandate` types | implemented (`icn/crates/icn-governance/src/authority.rs`, `mandate.rs`) |
| Mint at accepted-decision seam | implemented (`icn/apps/governance/src/grant_minting.rs`) |
| `Mandate::new_pending_grants` truthful fall-through | implemented (referenced from `grant_minting.rs:241`) |
| Mandate persistence in receipt store | implemented (`icn/crates/icn-gateway/src/receipt_store.rs` mints test fixtures using these types) |
| Kernel dispatch gated by mandates | **NOT IMPLEMENTED.** Mandates are institutional memory only at present. |
| Kernel capability minting from `AuthorityGrant` | **NOT IMPLEMENTED.** ADR-0014 explicitly leaves this for a future implementation. |
| Authority-revocation flow | partially implemented (revocation writes referenced in `grant_minting.rs:343`); end-to-end not verified by this ADR |

## Code evidence

- `icn/crates/icn-governance/src/authority.rs` declares `AuthorityClass` (closed enum: `Representation`, `Execution`, `Attestation`), `AuthorityGrant`, `TypedScope`. Module doc: "These types live at the **governance app layer** and must never be imported by kernel crates."
- `icn/crates/icn-governance/src/mandate.rs` declares `Mandate` with `Mandate::new_pending_grants(...)` and `Mandate::new(...)` constructors. Module doc explicitly distinguishes mandate from delegation, role assignment, structure, institutional effect record, governance decision receipt, federation agreement, and charter.
- `icn/apps/governance/src/grant_minting.rs` is the seam. Module doc:
  > "It mints a grant **only** for proposal classes whose payload already names the grantee, the class of authority, and a truthful time bound. If a payload does not carry that information, we return an empty `Vec` — no grant is better than a fabricated one."
  > "Today the only classes that mint grants are steward-appointment and steward-reconfirmation proposals … Every other accepted proposal currently produces zero grants, and its mandate is recorded via [`crate::manager`] using the bootstrap-phase `new_pending_grants` constructor."
  > "This module does not gate dispatch, authorize executors, or change effect semantics. It only produces records."
- `icn/apps/governance/tests/actor_close_proposal_mandate_parity.rs` exercises `AuthorityClass::Attestation` parity through the close-proposal flow.
- Kernel-dispatch enforcement search (`grep -rn "Mandate" icn/crates/icn-kernel-api/ icn/crates/icn-core/src/dispatch*`) returns no matches: the kernel does not consult mandates.
- `icn/crates/icn-gateway/tests/authority_enforcement_integration.rs` enforces *cross-domain authority rejection* in handler-layer code (capability + jurisdiction checks). This is a separate enforcement axis from mandate-based dispatch gating; it predates ADR-0014's typed authority model and operates on `Did`/`Capability`/`Domain` directly.

## Consequences

- The accepted-decision seam has a written contract that future authors can read instead of reverse-engineering from `grant_minting.rs`.
- Reviewers proposing to broaden the supported-class set of grant minting must do so via a follow-up ADR; silent broadening is a process violation.
- Institutional-memory readers (auditors, federation peers, governance researchers) can rely on the rule that a `Mandate` exists for every accepted decision, even if no grants do.
- The kernel/app boundary remains intact: mandates and grants live at the governance app layer; the kernel never imports them.
- The "next ADR" frontier on this axis is *enforcement* — when (if ever) kernel dispatch starts consulting mandates as an authorization check. That is a non-trivial decision that imposes a hard correctness bar on the seam this ADR records, and it deserves its own ADR.

## Non-decisions (out of scope)

This ADR does **not**:

- Authorize the kernel dispatcher to consult mandates as a gate.
- Change the supported-proposal-class set for grant minting.
- Mint kernel `Capability` tokens from `AuthorityGrant`s.
- Define a revocation/conflict-resolution protocol for grants.
- Specify a federation-side mandate-recognition contract.
- Govern how non-governance app code (e.g., gateway handlers) consults grants.

Each of those is a separate decision and gets its own ADR if and when it is taken.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Mint default grants for unsupported proposal classes | Would fabricate authority. ADR-0014 is explicit: "These classes are distinct and must not be silently conflated. A grant of one class does not confer the powers of another. Conflating them is the usual mechanism of institutional capture; typing prevents it." A "default Attestation grant" for any proposal would re-introduce exactly that conflation. |
| Skip mandate persistence for unsupported classes | Mandates are the constitutional-memory anchor between decision and action. Skipping persistence breaks the chain `Charter → Decision → Mandate → Action → Receipt → Evidence` (ADR-0014 §What a Mandate is). The pending-grants record is a truthful answer to "we decided this, but the authority shape is not yet expressible," and that answer is more useful than a missing record. |
| Make this ADR an enforcement-gate decision | Two distinct decisions, two distinct review surfaces. Bundling them obscures whether the project signed off on (a) "we mint these records," (b) "the kernel enforces these records," or both. ADR-0019 is (a) only. |
| Wait for an enforcement decision before recording the seam | The seam already exists in code. Leaving it un-recorded means the next reviewer has to re-derive it from `grant_minting.rs`. Recording the seam is a precondition, not a successor, for a future enforcement decision. |
