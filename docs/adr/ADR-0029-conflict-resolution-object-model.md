---
id: "0029"
title: "Conflict Resolution Object Model"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["conflict-resolution", "appeals", "mediation", "remediation", "institutional-care", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partial"
references:
  - "ADR-0023 (CCL Institutional Process Language)"
  - "ADR-0025 (Institutional Effect Record Canonical Schema)"
  - "ADR-0026 (Receipt and Provenance Proof Envelope)"
  - "icn/crates/icn-ccl/src/disputes.rs"
  - "icn/crates/icn-governance/src/appeal.rs"
---

# ADR 0029: Conflict Resolution Object Model

## Status

**Proposed (2026-04-26).** Names the object model. Compute disputes are implemented; institutional conflict has type sketches but no runtime. Drafting now establishes vocabulary so future ADRs (resolution processes, evidence model, remedies) plug in.

## Context

Conflict is the human reality layer. ICN today has two partial pieces:

1. **Compute disputes** — an actor (`icn-ccl/src/disputes.rs`) and tests covering the file/investigate/resolve flow for compute task results. Real, working, scoped to compute.
2. **Governance appeals** — an `Appeal` type sketch (`icn-governance/src/appeal.rs`) with `proposal_id`, `appellant_did`, `grounds`, `status`. No runtime, no handlers, no escalation rules.

What is missing is a model that says: conflicts have shapes, and the shape determines the path. Without this naming, every conflict pathway gets re-invented per surface.

The cost of not having this ADR: when ADR-0028 (accessibility) talks about "remediation pathway," when ADR-0027 (action cards) talks about "signal cards," when ADR-0042 (federation dispute resolution) gets drafted, each one needs a model. Drafting one shared model now means each follow-up references it.

## Decision (proposed)

ICN distinguishes three conflict shapes. Each has a distinct lifecycle and resolution path.

### Shape 1 — Conflict about effects (post-decision)

Someone challenges an institutional outcome that has already landed (a role assigned, an allocation made, a sanction issued).

- **Object:** `EffectChallenge` referencing the underlying `EffectRecord` (ADR-0025).
- **Resolution paths:** challenge dismissed; effect amended (counter-effect); effect reversed (full counter-effect with restoration); repair agreement (a structured outcome, not just reversal).
- **Authority:** challenges proceed against the original mandate's authority class; if authority was Representation, the body that holds that authority resolves.

### Shape 2 — Conflict about decisions (process appeal)

Someone challenges the process by which a decision was reached (quorum disputed, eligibility miscounted, deliberation curtailed).

- **Object:** `ProcessAppeal` referencing the underlying decision and its `GovernanceProof` (ADR-0026).
- **Resolution paths:** appeal dismissed; decision re-run with corrected process; decision voided.
- **Authority:** appeals proceed against the next-higher governance body (per package charter); if no higher body exists, against the federation if one is in scope.

### Shape 3 — Conflict between members (relational)

Two or more members are in conflict that is institutional but not effect-bound or decision-bound (a working group dispute, a stewardship disagreement, an interpersonal grievance with cooperative consequences).

- **Object:** `RelationalConflict` referencing the parties (DIDs) and the institutional location (entity / structure).
- **Resolution paths:** facilitation; mediation; structured agreement; disengagement (no fault); referral to a body with relevant authority.
- **Authority:** processed under a CCL-defined resolution process (ADR-0060 candidate) declared by the package.

### Shared properties across shapes

- **Identity.** Every conflict object has a content-addressed id and a creation receipt.
- **Lifecycle.** `Filed → InProgress → Resolved | Withdrawn | Escalated`. Resolution carries an outcome record.
- **Privacy.** Conflicts may carry redaction policy. Sensitive evidence is referenced by hash, not embedded.
- **Provenance.** Resolution outcomes produce effect records (ADR-0025). The conflict is institutional memory, not transient correspondence.
- **Repair-first.** The resolution language defaults to repair (restore the state, agree on going-forward); sanctions are the last-resort branch, not the default.

### Compute disputes are a special case

`icn-ccl/src/disputes.rs` `DisputeActor` handles disputes about compute task **results**. This ADR does not displace it. Compute disputes are technical (was the result divergent?) rather than institutional (was this decision authorized? was this effect just?). They share the lifecycle vocabulary but live in their own actor.

## Consequences

- Each conflict shape has a written runtime entry point. ADR-0027 signals can attach to a Shape 3 conflict; ADR-0042 federation disputes can extend Shape 1 across coops.
- Repair-first language is encoded in the model itself, not as an aspirational note.
- Trade-off: three shapes is more ceremony than one. The three are different enough that collapsing them produces accountability gaps.
- Trade-off: the ADR is descriptive of the model, not of the runtime. Building the runtime is real work (process runner per ADR-0023, evidence bundle per registry candidate ADR-0061).

## Implementation status

Partial.

| Surface | Status | Evidence |
|---|---|---|
| Compute disputes (separate path) | implemented | [icn/crates/icn-ccl/src/disputes.rs](../../icn/crates/icn-ccl/src/disputes.rs); test [icn/crates/icn-ccl/tests/dispute_actor_integration.rs](../../icn/crates/icn-ccl/tests/dispute_actor_integration.rs) |
| Governance appeal type | type-sketch only | [icn/crates/icn-governance/src/appeal.rs](../../icn/crates/icn-governance/src/appeal.rs) |
| EffectChallenge / RelationalConflict | NOT IMPLEMENTED | This ADR introduces the names |
| Resolution processes (CCL) | NOT IMPLEMENTED | Depends on ADR-0023 |
| Evidence bundle | NOT IMPLEMENTED | Registry candidate ADR-0061 |
| Remedies / sanctions / repair | NOT IMPLEMENTED | Registry candidate ADR-0062 |

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| One unified Conflict type with a kind enum | Looks elegant; in practice the three shapes have different identifiers, different authorities, and different resolution paths. The unified type would have so many optional fields that it would not constrain anything. |
| Merge process appeals with effect challenges | Fails when an effect was correctly produced from a flawed process or an effect was incorrectly produced from a sound process. The two failures need separate paths. |
| Defer this ADR until compute disputes generalize | They will not generalize cleanly. Compute is technical; institutional conflict is human. The DisputeActor pattern is right for compute and wrong for relational conflict. |
| Borrow a generic "issue tracker" model | Issue trackers optimize for engineering work, not institutional accountability. They have no provenance contract, no authority concept, no repair language. |
