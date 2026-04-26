---
id: "0022"
title: "CCL Schema Bridge and Constraint Compilation"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["ccl", "schema-bridge", "meaning-firewall", "constraints", "back-fill"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented"
references:
  - "ADR-0007 (Commons Resource Policy — CCL Formula Extraction)"
  - "ADR-0014 (Constitutional Object Model)"
  - "ADR-0021 (CCL Determinism, Fuel, and Capability Safety)"
  - "docs/architecture/KERNEL_APP_SEPARATION.md"
  - "icn/crates/icn-ccl/src/schema/mod.rs"
  - "icn/crates/icn-ccl/src/schema/bridge.rs"
  - "icn/crates/icn-ccl/tests/charter_bridge_integration.rs"
---

# ADR 0022: CCL Schema Bridge and Constraint Compilation

## Status

**Accepted (2026-04-26).** Back-fill of the existing schema bridge contract. The bridge has been the meaning firewall for charter-driven policy since the CCL schema layer landed; this ADR records the decision so the boundary does not erode.

## Context

ICN's kernel must enforce constraints without understanding their domain meaning ([docs/architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)). Charters express domain meaning: voting thresholds in plain numbers or formulas, surplus allocation percentages, credit ceilings, eligibility rules. The schema bridge is the seam where charter semantics become a `ConstraintSet` the kernel can enforce blindly.

ADR-0007 recorded the commons-resource case (CCL formula extraction). The bridge has since generalized to governance, economics, and federation-agreement schemas. Recording the boundary as an ADR matters now because:

1. Later ADRs in the registry (0023 institutional process language, 0036 federation agreements) extend the bridge with new schema kinds.
2. The bridge is the only sanctioned place to translate charter semantics; reviewers need a written reference for what counts as "schema-bridge work" and what counts as kernel pollution.

## Decision

The CCL schema bridge is the **only** path by which charter semantics become kernel-enforceable constraints. It has the following properties:

1. **Schema-driven input.** Charter authors write YAML against a known schema (governance, economics, agreement, …). Schemas live in [icn/crates/icn-ccl/src/schema/](../../icn/crates/icn-ccl/src/schema/) and are versioned with the bridge.
2. **CCL-driven evaluation.** Where a schema field is a formula or expression (e.g., voting threshold as a function of membership size), the bridge evaluates it via the CCL interpreter under ADR-0021 safety properties.
3. **Closed output.** The bridge emits `ConstraintSet` values composed of kernel-known primitives (rate limits, weights, multipliers, thresholds, scope tags). The kernel never sees the source rule.
4. **No domain leakage.** The bridge is the firewall: charter-side names ("steward eligibility threshold," "patronage allocation rule") do not appear in the kernel's view. They are compiled to opaque numeric or boolean constraints.
5. **Re-executable.** Given the same charter, schema version, and inputs, the bridge produces the same `ConstraintSet`. This is the precondition for replay-based audit and for dispute resolution that re-runs the bridge.

## Consequences

- The kernel is invariant under charter changes: charters change without forcing kernel-side modifications.
- Charter semantics are auditable by re-running the bridge against the same inputs.
- Schema additions are the canonical extension surface for new institutional concepts (federation agreements, process steps); kernel changes are not.
- Trade-off: any constraint the kernel does not already understand cannot be expressed until the kernel surface grows. Adding a new constraint primitive is therefore a coordinated move (kernel side + bridge side).
- Schema versioning is implicit today (in the schema files); a future ADR may formalize migration when in-place schema evolution becomes a real risk.

## Implementation status

Implemented. Evidence:

- Bridge entry points: [icn/crates/icn-ccl/src/schema/bridge.rs](../../icn/crates/icn-ccl/src/schema/bridge.rs).
- Schema kinds: [icn/crates/icn-ccl/src/schema/mod.rs](../../icn/crates/icn-ccl/src/schema/mod.rs), with submodules `governance`, `economics`, `agreement`, `entity`.
- Constraint output type lives in `icn-kernel-api` and is consumed by `PolicyOracle` implementations.
- Test coverage: [icn/crates/icn-ccl/tests/charter_bridge_integration.rs](../../icn/crates/icn-ccl/tests/charter_bridge_integration.rs) (~447 lines of bridge-evaluation cases).

Not yet implemented (out of scope here, named in the registry as future work):
- Formal schema versioning and migration semantics (registry candidate; future ADR).
- Bridge-side telemetry for compilation cost and rule complexity.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Let governance code compile constraints inline (no separate bridge) | Erodes the meaning firewall. Every governance change risks re-deriving the same translation differently in different places. |
| Embed kernel primitives directly in charter YAML | Forces charter authors to think in kernel terms. The bridge exists so charter authors think in institutional terms and the kernel sees only constraint primitives. |
| Make the bridge a runtime service (RPC) | Adds availability and consistency burden for what is by construction deterministic and pure. The bridge is a library, not a service. |
