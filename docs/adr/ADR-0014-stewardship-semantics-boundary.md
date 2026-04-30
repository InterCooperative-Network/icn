---
id: "0014"
title: "Stewardship Semantics Boundary — Trust-Mandate Office, Not Stake-Backed Role"
status: "accepted"
date: "2026-04-07"
context: "semantic-correction / bond-slash-removal"
deciders: ["Matt Faherty"]
tags: ["governance", "stewardship", "sdis", "identity-recovery", "architecture", "semantics"]
---

# ADR 0014: Stewardship Semantics Boundary

## Status

**Accepted (2026-04-07)**

## Context

During Sprint 26 implementation of stewardship governance dispatch (SanctionSteward,
AppointSteward, RemoveSteward), bond/slash semantics were imported from DeFi/staking
mental models: stewards posted a financial bond that could be "slashed" as a penalty for
misconduct.

This is architecturally wrong for ICN. The discovery triggered a systematic semantic
correction removing all bond collateral concepts from the stewardship domain.

## Decision

### Stewardship Is an Elected/Permissioned Institutional Office

ICN stewardship is backed by **governance legitimacy and trust**, not capital at risk.
The governance vote — not an economic deposit — is the legitimating act for steward
appointment.

A steward is an **institutional office holder**, analogous to an elected board member or
a designated representative, not a validator posting stake.

**What this means in code:**
- `StewardRecord` has no `bond_amount` field
- `CommonsHandle::register_steward()` takes no bond parameter
- `SdisEffect::ApproveSteward` carries no `bond_amount`
- `AppointStewardRequest` carries no bond field
- `StewardProfile::activate()` takes no bond parameter
- `StewardConfig` has no `min_bond_amount`

### Steward Sanctions Are Status/Authority Changes Only

`StewardPenalty` variants represent institutional status or authority changes:

| Variant | Meaning |
|---------|---------|
| `Warning` | Formal notice, no state change |
| `Censure` | Institutional rebuke, recorded, no financial payload |
| `Suspension` | Temporary loss of office authority |
| `TierDemotion` | Reduced jurisdiction scope |
| `Probation` | Conditional continuation of office |
| `Removal` | Loss of office (for cause) |

`Censure` replaces the former `BondSlash` variant. A censure is a formal institutional
rebuke — it changes the public record of the steward's conduct, but does not touch any
treasury or credit balance. The kernel sees this as `KernelEffect::NoOp { reason }`.

**Removed forever:**
- `StewardPenalty::BondSlash { amount: i64 }` — financial punishment at baseline
- `Removal { return_bond: bool }` — bond return on removal
- `CommonsHandle::add_steward_bond()` — bond deposit endpoint
- `CommonsHandle::slash_steward_bond()` — bond penalty endpoint
- Gateway endpoints `POST /v1/steward/{id}/bond/add` and `POST /v1/steward/{id}/bond/slash`

### Recall as a Governance Lifecycle Concept

In addition to `Removal` (for cause), stewards may be **recalled** through a governance
process — the cooperative's membership votes to end a steward's term, not necessarily
due to misconduct. This is modeled via a governance proposal of type `SdisProposal::RemoveSteward`
with a reason that indicates recall rather than cause. Policy/CCL may further differentiate
these cases in downstream enforcement.

### SDIS/Identity Recovery Is Trust-Governed, Not Capital-Weighted

Stewards participate in SDIS identity recovery flows (POP issuance, pepper share attestation,
dispute resolution). The authority to participate in these flows is gated by:

1. **Trust score** — computed by the trust graph, not by bond size
2. **Governance approval** — the proposal that appointed the steward
3. **Institutional mandate** — the domain/charter under which the steward operates

Recovery participants are selected by trust, governance, and institutional legitimacy —
never by capital weight. A steward with a large bond does not get more votes in a recovery
quorum than a steward with none.

### Where Economic Bonding IS Appropriate

Economic bonding is not wrong in general — it is wrong for the steward office. Bonding
may be appropriate in explicitly scoped, non-steward-office contexts:

- CCL contract enforcement (a cooperative posting collateral against a bilateral agreement)
- Federation clearing positions (imbalance limits in clearing contracts)
- Compute escrow (task allocation bonds)

Any such bonding must be:
1. Explicitly scoped to a specific economic agreement
2. Governed by a CCL clause, not by core governance roles
3. Documented in a separate ADR

### Policy/CCL Boundary for Trust/Reputation Consequences

Sanctions may have downstream trust or reputation consequences. These are defined in CCL
policy, not in the kernel:

- A censure may reduce a steward's trust score — policy decision, CCL-configurable
- A suspension may trigger cascading access revocations — policy decision, CCL-configurable
- Removal may blacklist a DID from future steward candidacy — policy decision, CCL-configurable

The kernel only enforces status changes. The meaning of those status changes for trust
and reputation is above the kernel boundary.

## Consequences

### Positive

- Stewardship semantics are now coherent with the cooperative/civic model
- SDIS identity recovery cannot be gamed by capital accumulation
- The governance vote remains the sole legitimating act for steward authority
- Censure provides a meaningful sanction without introducing treasury side effects at
  the kernel level
- Trust/reputation consequences remain configurable via CCL without touching core types

### Negative

- Breaking change to `RegisterStewardRequest` API (removed `bond_amount` field)
- Breaking change to `AppointStewardProposalRequest` (removed `bond_amount` field)
- Existing bond data in any serialized state will be ignored on deserialization

### Guardrails

Tests asserting the semantic boundary:
- `test_steward_penalty_variants` — verifies `Censure` is a unit variant with no financial payload
- `test_voting_requirements` — verifies `Censure` does not require supermajority (unlike `TierDemotion`)
- Future: `test_sanction_does_not_affect_treasury` — proposed test verifying sanctions
  only change steward status, never ledger balances

## See Also

- `icn-governance/src/sdis.rs` — `StewardPenalty` enum definition and doc comments
- `icn-governance/src/steward.rs` — `StewardRecord` struct (no bond fields)
- `icn-commons/src/handle.rs` — `CommonsHandle` (no bond methods)
- `icn-kernel-api/src/effects.rs` — `SdisEffect::ApproveSteward` (no bond fields)
- ADR-0013 — Federation clearing adoption (for correct use of economic bonding at CCL level)
