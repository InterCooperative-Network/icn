---
id: "0023"
title: "CCL Institutional Process Language"
status: "proposed"
date: "2026-04-26"
deciders: []
tags: ["ccl", "institutional-process", "workflows", "deadlines", "escalation", "forward-direction"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed"
references:
  - "ADR-0021 (CCL Determinism, Fuel, and Capability Safety)"
  - "ADR-0022 (CCL Schema Bridge and Constraint Compilation)"
  - "ADR-0028 (Accessibility Baseline for Member Interfaces)"
  - "ADR-0029 (Conflict Resolution Object Model)"
  - "GitHub #863 (Federation Agreement Support)"
  - "ops/coordination/adr_candidates.yaml"
---

# ADR 0023: CCL Institutional Process Language

## Status

**Proposed (2026-04-26).** This ADR names a forward direction. It is not an accepted decision. Drafted so that later ADRs (federation agreements, conflict resolution processes, accessibility process steps) can reference a single concept rather than re-inventing it.

## Context

CCL today encodes static rules: voting thresholds, allocation formulas, eligibility predicates. These are sufficient for charter constraints but insufficient for institutional life, which is mostly process: onboarding flows, grievance procedures, allocation reviews, deadline escalations, mediation steps.

Today these processes are either (a) implemented in Rust and bound to release cycles, (b) implemented as ad-hoc workflows in apps with no governance amendment path, or (c) not implemented at all. Each option fragments institutional design.

The forward direction is **CCL as institutional process language**: process definitions written in CCL, ratified through governance, executed by a process runner that reuses the CCL interpreter's safety properties.

## Decision (proposed)

CCL grows a process layer with these properties:

1. **Process step grammar.** A `Process` is a directed acyclic graph of `Step` nodes. Each step has a kind (`AutoStep`, `MemberStep`, `RoleStep`, `SignalStep`), inputs, outputs, deadline, and escalation rule.
2. **Deterministic step semantics.** `AutoStep` evaluates as a CCL expression under ADR-0021 fuel and capability bounds. `MemberStep` and `RoleStep` halt the process pending member input, with deadline tracking.
3. **Deadline as data, not as preemption.** Deadlines do not interrupt; they trigger declared escalation. Escalation is itself a process step, expressible in CCL.
4. **Process records as effects.** A process completing produces an effect record (ADR-0025 territory) carrying the final state and provenance for every step.
5. **Process ratification through governance.** Process definitions live in charters or as charter amendments. Activating a new process is a governance decision, not an app deployment.
6. **Process amendment without in-flight disruption.** A new version of a process applies to processes started after activation; in-flight processes complete under the version they started under.

## Out of scope (deliberate)

- Specific process libraries (mediation, allocation review, onboarding). Those are domain content, expressed by institutions in their own packages.
- Cross-process composition (one process triggering another). Defer to a follow-up ADR once the single-process semantics are stable.
- Real-time human collaboration (synchronous editing, presence). Out of scope for CCL.

## Consequences

- Institutional processes become legible, governable, and amendable on the same surface as charter rules.
- The line between "rule" and "process" becomes a CCL surface concern, not a code-vs-rule concern.
- Trade-off: the process runner is a new runtime surface (likely a new actor in the supervisor) with its own persistence and replay semantics. Designing it well takes work.
- Trade-off: deadline-driven escalation is a behavioral change with accessibility implications (caregiving, time justice, language). ADR-0028 governs the floor.

## Implementation status

Proposed. Nothing implemented under this ADR. Pre-existing pieces that this ADR plans to reuse:

- The CCL interpreter (ADR-0021).
- The schema bridge for compiling expressions (ADR-0022).
- The mandate/effect model (ADR-0014, ADR-0025) for binding process outcomes to authority.

Required follow-up issues (not yet open):
- Process step grammar specification.
- Deadline and escalation semantics specification.
- Process runner actor design (new crate `icn-process` or extension of `icn-governance`).
- Process amendment migration semantics.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Hard-code each institutional process in Rust | Fragments institutional design; ties amendment to release cadence; violates the principle that institutional design lives in CCL. |
| Adopt a generic workflow engine (Temporal, etc.) | Generic engines optimize for distributed-system reliability, not for institutional accountability. They have no provenance contract, no governance amendment path, no meaning firewall. |
| Defer until federation agreement support (#863) drives the requirement | Naming the direction now lets ADR-0029 (conflict resolution) and ADR-0036 (federation agreements) cite a single concept. Deferring forces each follow-up to re-litigate. |
| Treat process steps as off-chain coordination tools | The point of ICN is that institutional life is on-substrate, with provenance. Off-chain coordination is exactly the failure mode this design avoids. |

---

## Notes

This ADR is a **direction**, not a decision. Modifying or refusing it is expected. Drafting it now means later ADRs do not have to invent a new term every time they reach for "we need processes here."
