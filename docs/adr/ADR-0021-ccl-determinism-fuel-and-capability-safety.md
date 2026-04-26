---
id: "0021"
title: "CCL Determinism, Fuel, and Capability Safety"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["ccl", "determinism", "fuel", "capabilities", "safety", "back-fill"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented"
references:
  - "ADR-0014 (Constitutional Object Model)"
  - "ADR-0018 (ADR Lifecycle)"
  - "docs/architecture/KERNEL_APP_SEPARATION.md"
  - "icn/crates/icn-ccl/src/interpreter.rs"
  - "icn/crates/icn-ccl/src/runtime.rs"
  - "icn/crates/icn-ccl/tests/contract_integration.rs"
---

# ADR 0021: CCL Determinism, Fuel, and Capability Safety

## Status

**Accepted (2026-04-26).** Back-fill of an existing implementation contract. CCL has shipped with these properties since the schema-bridge work; this ADR records the decision so later ADRs can depend on a written claim instead of an inferred one.

## Context

The Cooperative Contract Language (CCL) is ICN's domain-scoped expression and rule language. It is used in two production roles today: (a) compiling charter rules into kernel-enforceable `ConstraintSet` values via the schema bridge, and (b) packaging compute tasks for execution under fuel and capability bounds. A third forward role — CCL as institutional process language — is named in ADR-0023 and depends on the same safety properties.

CCL's safety properties (deterministic, fuel-metered, not Turing-complete, capability-bounded) have been implementation contracts for some time without a written ADR. The ADR exists so that:

1. Later ADRs (0022, 0023, 0030, 0031, 0036) can cite a stable safety claim.
2. Reviewers of CCL changes have a single document to check against.
3. Any proposal to relax a property requires a successor ADR, not a quiet code change.

## Decision

CCL is contractually:

1. **Deterministic.** Given the same inputs and capability set, repeated execution produces the same output. No wall-clock reads, no random sources, no network reads inside contract evaluation.
2. **Fuel-metered.** Every contract execution carries a finite fuel budget. Exceeding the budget halts execution with a structured error; partial state is not committed.
3. **Not Turing-complete.** Recursion and unbounded iteration are not expressible. All CCL programs terminate within fuel.
4. **Capability-bounded.** A contract can only read or write what its declared capability set permits (`ReadLedger`, `WriteLedger`, `ReadTrust`, etc.). Capabilities are checked at the interpreter boundary, not inside the contract body.
5. **Pure expression and rule.** Side effects are mediated by capabilities returned to the host (the schema bridge or executor); contracts do not perform I/O directly.

These properties are jointly the precondition for treating CCL output as governance-grade and audit-grade input.

## Consequences

- CCL output can be re-executed during dispute resolution and divergence is mechanically detectable.
- Compute schedulers can safely move CCL workloads between nodes because results do not depend on node-local state.
- Charter rules compile to a fixed `ConstraintSet` for a given input, which means the meaning firewall holds: the kernel sees only the compiled constraint, not the rule semantics.
- Trade-off: domain logic that needs non-determinism (LLM calls, real-world clocks, network reads) cannot live inside CCL. Such logic is a host concern delivered through capability outputs or a separate compute-with-attestation path (ADR-0030).
- Adding a property here (e.g., a new capability kind, a new fuel cost rule) is permissive; relaxing one (e.g., allowing recursion) requires a successor ADR.

## Implementation status

Implemented. Evidence:

- Interpreter and fuel metering: [icn/crates/icn-ccl/src/interpreter.rs](../../icn/crates/icn-ccl/src/interpreter.rs), [icn/crates/icn-ccl/src/runtime.rs](../../icn/crates/icn-ccl/src/runtime.rs).
- AST and capability model: [icn/crates/icn-ccl/src/lib.rs](../../icn/crates/icn-ccl/src/lib.rs), `Contract`/`Rule`/`Stmt`/`Expr`/`Value` types.
- Determinism and fuel test coverage: [icn/crates/icn-ccl/tests/contract_integration.rs](../../icn/crates/icn-ccl/tests/contract_integration.rs).
- Capability gating used at the schema-bridge boundary: see ADR-0022.
- Compute integration: `LocalExecutor` and `WasmExecutor` in [icn/crates/icn-compute/src/executor.rs](../../icn/crates/icn-compute/src/executor.rs).

What is not yet covered (and intentionally out of scope here):
- Formal verification of any of the above. The properties are enforced at the interpreter boundary by construction and by tests; not by proof.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Allow non-determinism via opt-in capabilities | Would split CCL into deterministic and non-deterministic dialects, breaking the meaning firewall promise. Non-deterministic work belongs to the compute layer with attestation (ADR-0030). |
| Make fuel optional for "trusted" contracts | "Trust" is already a domain concept; admitting it inside the CCL boundary collapses the kernel/app separation. |
| Promote CCL to Turing-complete with a step limit | Step limits without termination guarantees re-introduce halting-problem hazards into governance evaluation. The current bounded-iteration shape is sufficient for charter rules and process steps. |
