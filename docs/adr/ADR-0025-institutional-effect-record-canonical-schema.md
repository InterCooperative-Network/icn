---
id: "0025"
title: "Institutional Effect Record Canonical Schema"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["effect-record", "provenance", "mandate", "dispatch", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed"
references:
  - "ADR-0014 (Constitutional Object Model)"
  - "ADR-0019 (Authority Grant Minting and Mandate Persistence Seam)"
  - "ADR-0020 (Bootstrap Activation and Standing Read Model)"
  - "ADR-0024 (Institution Package Manifest Schema)"
  - "ADR-0026 (Receipt and Provenance Proof Envelope)"
  - "GitHub #1607 (RepresentativeVote / EntityAction)"
---

# ADR 0025: Institutional Effect Record Canonical Schema

## Status

**Proposed (2026-04-26).** Names the forward direction. The decision spine (proposal → mandate → grant) is in place; what is missing is the canonical schema for the **effect** that a mandate produces when it dispatches.

## Context

ADR-0014 froze the constitutional object model (AuthorityClass, AuthorityGrant, TypedScope, Mandate). ADR-0019 recorded the minting seam — how mandates are produced at proposal-acceptance time. ADR-0020 records the activation chain through to a member-facing standing surface. ADR-0026 records the receipt envelope that wraps decisions and provenance.

The piece not yet decided is the **EffectRecord**: when a mandate dispatches and produces an institutional outcome (a role assigned, an allocation made, an admission granted, a sanction issued), what does that record look like?

Today, governance state is in-memory, rebuilt on restart, and the linkage from mandate to side-effect is implicit. ADR-0019 acknowledged this: "kernel dispatch consulting mandates" is named as future work. Before that gap can be closed, ICN needs a canonical schema for what gets produced.

## Decision (proposed)

`EffectRecord` is a stored type with the following shape:

1. **Identity.** Effect id (content-addressed hash), kind (closed taxonomy), produced-at timestamp, producer DID.
2. **Authority chain.** Mandate id, authority grant id, governance proof hash. The chain that proves this effect was authorized.
3. **Target.** Domain id, entity id, optional structure id, optional holder DID. Which institutional location this effect lands in.
4. **Payload.** Effect-kind-specific data (closed schema per kind).
5. **Reversal pointer.** If this effect reverses an earlier effect, the earlier effect id; otherwise null. Reversal is a counter-effect, not a deletion.
6. **Plain-language summary.** Required short string; the canonical "what happened in human terms" rendering.
7. **Persistence.** Effects are persisted alongside mandates in the receipt store. They survive restart. They are queryable from `/me/standing` and (future) audit endpoints.

### Effect kind taxonomy (closed; growable by ADR amendment)

- `RoleAssigned`, `RoleRevoked`
- `MembershipAdmitted`, `MembershipDeparted`
- `AllocationMade`, `AllocationDeclined`, `AllocationReversed`
- `EntityCreated`, `StructureCreated`, `ActivityCreated`
- `CharterActivated`, `CharterAmended`
- `AuthorityGranted`, `AuthorityRevoked` (effect record about authority itself)
- `SanctionIssued`, `RepairAgreed` (placeholder for ADR-0029 / ADR-0062)
- `EffectReversed` (counter-effect)

Each kind has a typed payload schema; unknown kinds are not dispatched.

## Decision: who emits effect records

- **Producer:** the actor or process executing under a mandate (governance app, compute executor under mandate, federation actor under treaty).
- **Witness:** the gateway/governance receipt store, which persists the effect and signs it.
- **Consumer:** standing read model, action card derivation, audit endpoints, dispute/appeal pathways.

## Consequences

- The institutional memory chain becomes complete: decision → mandate → effect → receipt. Today the chain skips from mandate to ad-hoc state; this ADR fills the gap.
- Reversal becomes a first-class operation, not a delete. Audit logs do not lie about history.
- Trade-off: a closed taxonomy means new effect kinds require ADR amendment. This is the right trade — silent expansion of effect shape is exactly what auditors should not accept.
- Trade-off: every system that writes to governance state must produce effect records. Migration of existing in-memory state is non-trivial.

## Implementation status

Proposed. Nothing implemented under this ADR.

Pre-existing pieces this ADR builds on:
- Mandate type and persistence (ADR-0014, ADR-0019).
- ReceiptBackend and sled-backed receipt store (icn-gateway).
- Governance proof envelope (ADR-0026).

Required to call this `implemented`:
- `EffectRecord` Rust type and kind enum in `icn-governance` (or a new `icn-effects` crate).
- ReceiptBackend extension to put effect records.
- Migration of existing dispatch paths (role assignment, charter activation, etc.) to produce effect records.
- `/me/standing` extension to surface recent effects.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Treat effects as ad-hoc domain state per kind | What we have today. Produces silent state mutations and a non-replayable history. |
| Use the receipt envelope itself as the effect record | Receipts carry decision provenance; effects carry institutional outcomes. Conflating them loses the distinction between "the proposal was accepted" and "the role was actually assigned." |
| Open taxonomy (any string is a valid effect kind) | Removes the audit guarantee. Closed taxonomy is the audit-grade choice. |
| Defer until kernel dispatch consults mandates | The taxonomy is the precondition for that work, not an output of it. Drafting now lets dispatch consult against a real schema. |
