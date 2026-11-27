---
id: "0093"
title: "Specific-Action Authority Issuance for Institutional-Domain Acts"
status: "proposed"
date: "2026-11-27"
deciders: ["Matt Faherty"]
tags: ["governance", "authority", "mandate", "grant-minting", "institutional-domain", "domain-policy"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "not-started"
references:
  - "ADR-0014 (Constitutional Object Model — AuthorityGrant, Mandate, TypedScope, payload_hash)"
  - "ADR-0019 (Authority Grant Minting and Mandate Persistence Seam — broadening requires a follow-up ADR)"
  - "ADR-0083 (proposed) (InstitutionalDomain and DomainPolicy runtime root)"
  - "docs/architecture/IDENTITY_SEMANTICS.md (institutional authority is a governance decision, never one key's signature)"
  - "icn/apps/governance/src/grant_minting.rs (derive_grants_for_accepted_proposal)"
  - "icn/apps/governance/src/mandate_gate.rs (DefaultMandateGate)"
  - "icn/apps/governance/src/decision_commitment.rs (decision-commitment verification)"
  - "icn/apps/governance/src/domain_policy_adoption.rs (the two gated consumers)"
---

# ADR 0093: Specific-Action Authority Issuance for Institutional-Domain Acts

## Status

Proposed. This ADR is a design for maintainer review. Nothing here is implemented, and
nothing here changes production issuance until it is accepted.

## Context

Two production acts are gated on ADR-0014 authority:

- `adopt_domain_policy_gated`, which sets an `InstitutionalDomain`'s current policy;
- `declare_institutional_domain_gated`, which creates an `InstitutionalDomain`.

Each requires, through `DefaultMandateGate`:

- an **Execution** `AuthorityGrant` to the actor;
- a scope that binds the domain and the act token (`domain_policy:adopt` / `institutional_domain:declare`);
- a mandate behind it whose proposal has committed to exactly the mandate's decision and payload.

**No legitimate decision produces that authority today.**

- `derive_grants_for_accepted_proposal` mints grants only for SDIS steward proposals, which yield Attestation-class grants.
- Every other accepted proposal records a pending-grants mandate, which authorizes nothing.
- ADR-0019 deliberately keeps the minting set small and requires a follow-up ADR to broaden it. That is this ADR.
- So authority for these acts can currently exist only if written outside governance. The gates fail closed and must stay fail-closed until this ADR is decided.

**The consumers have a second gap.** Adoption does not compare the policy it is given with the decision that authorized it, and it never discharges the mandate. A grant obtained for one policy could therefore adopt another, repeatedly, until it expires.

## Decision (proposed)

### 1. Specific-action authority first

The first issuance path is **specific-action authority**. An accepted proposal authorizes exactly
one institutional act with exactly specified content, executed by a named executor.

Future shapes are acknowledged but **not** introduced here (see §9):
- scoped discretionary authority;
- continuing office or role authority;
- conditional authority under institutional rules.

### 2. Dedicated typed payloads

The proposal payload gains two dedicated variants. Names and fields may be refined in review:

```
AdoptDomainPolicy {
    domain: GovernanceDomainId,      // the InstitutionalDomain whose policy changes
    policy_id: DomainPolicyId,       // content address of the exact policy text
    executor: Did,                   // who may perform the adoption
    valid_for: Seconds,              // truthful time bound (ADR-0019)
}
DeclareInstitutionalDomain {
    domain: GovernanceDomainId,
    owning_entity_class: BootstrapEntityType,
    charter_ref: Option<CharterId>,
    executor: Did,
    valid_for: Seconds,
}
```

There is **no** generic "grant authority" payload. Each new act class needs its own variant
and its own ADR-level justification. That is the ADR-0019 "small on purpose" rule applied
again.

### 3. Exact action-content binding

On acceptance, the derive seam mints **one** Execution grant to `executor`:
- scope `{domain, action_kind: [<act token>]}`;
- validity `now .. now + valid_for`;
- provenance = the decision.

The mandate's `payload_hash` already binds the mandate to the payload (ADR-0014). The consumer
must additionally check the requested act against the **accepted payload**, not only the act
token:
- adoption requires `policy.id == payload.policy_id` and `policy.domain_id == payload.domain`;
- declaration requires the full `(domain, owning_entity_class, charter_ref)` tuple.

A substituted action is refused even when every other check passes.

The gate's `proposal_class` tokens for these acts ("DomainPolicy", "InstitutionalDomain") are a
class-level binding. Class-level binding is discretionary scope (§9), so they are removed for these acts.

### 4. Named or explicitly authorized executors

The executor is named in the payload, and the grant's grantee is that executor. A node
operator, gateway or daemon identity is **never** an implicit executor or grantor:
- the grantor is the deciding governance domain (ADR-0019);
- ICN enforces the institution's allocation, it does not originate one.

### 5. Durable, recoverable execution

The adoption or declaration itself is the execution of the mandate. It writes, durably and
atomically with respect to recovery:
- the act's effect (the `current_policy` pointer, or the declared domain record);
- the mandate's transition to `Discharged`.

A crash between the two must be recoverable. The design follows the existing write-ahead
close-journal pattern rather than introducing a new mechanism.

### 6. Idempotent mandate discharge

A specific-action mandate is **single-use**:
- After a successful act it is `Discharged`, and `DefaultMandateGate` already refuses discharged mandates.
- Re-executing the identical act against an already-discharged mandate reports the prior result. It does not repeat the effect and does not fail.
- A different act against it is refused.

### 7. Constitutive versus constituted authority

- **Constituted authority** is authority exercised *within* an existing institution under its established rules. This ADR's adoption path is constituted authority.
- **Constitutive authority** creates or re-founds the institution itself.

Declaring an `InstitutionalDomain` sits at the boundary: the domain being declared does not yet hold authority. This ADR proposes:
- declaration is decided by the governance domain that will *become* the institutional domain (it must exist to hold a vote);
- the deciding domain is the grantor.

That choice is a maintainer decision (D6), not an agent inference.

### 8. Founding authority

The first act of a new institution cannot be authorized by authority the institution does not
yet have. Founding authority is integrated with the existing genesis, charter and continuity
mechanisms, not invented here. Concretely, the founding declaration and first policy adoption
derive their legitimacy from the institution's genesis or charter ratification record.

The exact linkage is a maintainer decision (D11). It must not resolve to "the node operator decides".

### 9. Future authority shapes (acknowledged, not introduced)

- **Scoped discretionary authority:** a class of acts bounded by a typed scope and institutional constraints (for example, an allowlist of policy ids). It needs a TypedScope extension and its own ADR.
- **Continuing office/role authority:** authority attached to a `RoleAssignment` for its tenure. It needs role-to-grant minting, revocation at tenure end, and Role grantees or targets in the gate (ADR-0014 future work).
- **Conditional authority:** authority established by institutional rules (charter clauses, policy evaluation). It needs a charter authority-clause schema and an evaluator.

None of these may be emulated by widening the specific-action path.

## Consequences

- The two gated consumers gain a legitimate source of authority for the first time, under a single-decision, single-act model.
- A full decision-to-execution integration test becomes possible: create, open, vote, close, mint, adopt, discharge, restart, refuse a repeat.
- Until this ADR is accepted and implemented, the consumers remain fail-closed. Component tests that seed grants stay component evidence only.

## Open maintainer decisions

The S31 issuance inventory carries the recommendations. The directions adopted above settle D1–D5 as proposals; the rest remain open.

| # | Decision | This draft |
|---|---|---|
| D1 | First authority shape | Specific-action (§1) |
| D2 | Vehicle | This ADR |
| D3 | Payload form | Dedicated typed variants (§2) |
| D4 | Execution model | Grant to a named executor, checked at the gate (§4) |
| D5 | Consumer binding and reuse | Exact content binding plus single-use discharge (§3, §6) |
| D6 | Who may decide a declaration | **Open.** Draft proposes the future institutional domain itself (§7) |
| D7 | Charter-direct grants (`granted_by: None`) | **Open.** Draft keeps them skipped |
| D8 | Office/role authority | Deferred (§9) |
| D9 | Class-level tokens for these acts | Removed (§3) |
| D10 | ADR-0083 status | **Open:** accept ADR-0083 with an amendment pointing here, or supersede its authority section |
| D11 | Founding-act linkage to genesis/charter | **Open** (§8) |
