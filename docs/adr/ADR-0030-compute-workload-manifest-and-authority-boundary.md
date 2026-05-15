---
id: "0030"
title: "Compute Workload Manifest and Authority Boundary"
status: "accepted"
date: "2026-04-26"
deciders: ["Matt Faherty"]
tags: ["compute", "authority-boundary", "constrained-execution", "workload-manifest", "back-fill"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented"
references:
  - "ADR-0014 (Constitutional Object Model)"
  - "ADR-0021 (CCL Determinism, Fuel, and Capability Safety)"
  - "ADR-0026 (Receipt and Provenance Proof Envelope)"
  - "ADR-0031 (Commons Compute Admission and Settlement Policy)"
  - "icn/crates/icn-compute/src/task.rs"
  - "icn/crates/icn-compute/src/executor.rs"
  - "icn/crates/icn-compute/src/types.rs"
  - "icn/crates/icn-gateway/src/compute_mgr.rs"
---

# ADR 0030: Compute Workload Manifest and Authority Boundary

## Status

**Accepted (2026-04-26).** Back-fill of the existing compute task model and the authority boundary it operates under. Compute has shipped with these properties; this ADR records the decision so the boundary does not erode.

## Context

ICN's compute layer (`icn-compute`) executes workloads — CCL contracts, WASM modules, and a small SQL-reference surface — with fuel limits, privacy/determinism classes, scope tags (Commons / Coop / Federation), and trust-gated executor admission.

Compute is real and runs today. What is **not** written down is the authority boundary: compute as **constrained execution**, never autonomous decision. The kernel/app separation doctrine ([docs/architecture/KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)) covers the broader meaning firewall; this ADR specializes it for compute.

The boundary matters now because:

1. Future ADRs (0080 result verification, 0081 WASM module governance, 0082 privacy/locality) plug into this boundary.
2. Public-facing claims about compute on the website (per ADR-0032) need a written authority claim to cite.
3. As LLM-style work and large-task patterns grow, the temptation to let compute "decide" things grows with them. The ADR closes that drift before it starts.

## Decision

A compute workload is described by a `ComputeTask` manifest with the following properties, and operates under the following authority boundary.

### Workload manifest

1. **Code reference.** One of: a CCL contract, a WASM module hash, a SQL-reference query.
2. **Fuel limit.** A finite numeric ceiling.
3. **Privacy class.** Public, Encrypted, or Sealed (the privacy ADR-0082 will refine; for now: ICN supports each as a class).
4. **Determinism class.** Required for governance-grade output; advisory for assistive output.
5. **Scope.** `Commons | Coop(coop_id) | Federation(fed_id)`. Determines admission and settlement policy.

   *Entity-scope naming note (added 2026-05-14):* This ADR uses `Coop(coop_id)` as the local-domain scope tag because that was the vocabulary at the time of adoption. Current architecture (per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 "Entity-scope vocabulary" and `docs/spec/institutional-domain.md`) reads this slot as `LocalDomain(domain_id)`: a scope owned by an `InstitutionalDomain` whose owning entity class may be `Cooperative`, `Community`, `Federation`, `Individual`, or another governed class permitted by policy. Future schema and runtime work should prefer `LocalDomain` names; existing serialized `Coop`-prefixed strings remain valid for backward compatibility. The ADR's decision is preserved as-is; the slot's reading is updated.
6. **Resource profile.** CPU, memory, storage envelope. Used by the commons pool and by federated placement.
7. **Mandate reference (optional).** If the workload's output will produce institutional effects, a reference to the covering mandate (ADR-0014, ADR-0019).

### Authority boundary

1. **Compute executes; compute does not decide.** Compute outputs feed governance; they do not replace governance.
2. **Outputs are advisory unless covered by a mandate.** A workload without a covering mandate may produce useful information; that information cannot itself produce institutional effects (no role assignment, no allocation, no admission). To produce effects, a covering mandate is required (ADR-0019).
3. **Trust-gated admission.** Executors are admitted via PolicyOracle (today: trust score + sybil checks); the kernel never sees raw trust scores.
4. **Fuel and capability bounded.** Every execution is bounded; runaway is impossible (ADR-0021).
5. **Privacy class is binding.** A `Sealed` workload cannot be re-disclosed; an `Encrypted` workload restricts who can read inputs and outputs; a `Public` workload is freely re-distributable.
6. **Determinism class is binding for governance grade.** Non-deterministic results cannot feed governance; they may feed assistive surfaces with a clear "advisory" marker.

### What compute is allowed to do

- Execute under fuel; emit results.
- Compute prioritization scores for action cards (ADR-0027) under a covering mandate.
- Pre-compile schema bridges (ADR-0022) for a charter under proposal.
- Compute commons pool admission scores (input to PolicyOracle), but not the admission decision itself.
- Re-execute a CCL contract during a dispute (ADR-0029) for divergence detection.

### What compute is not allowed to do

- Issue effect records without a covering mandate (ADR-0025).
- Decide governance outcomes (proposal acceptance, role assignment, sanction issuance).
- Cross privacy class boundaries (a Public workload reading a Sealed input is rejected).
- Emit non-deterministic output to a governance-grade consumer.

## Consequences

- The compute layer is auditable as a constrained-execution surface. Any temptation to drift into decision authority is a violation of this ADR.
- Public claims about compute on the ICN site can honestly say "real implementation, trust-gated, constrained execution; not autonomous decision-making."
- Future work on assistive compute (LLM-augmented, member-helper) plugs into the boundary by being declared `Advisory` and bound by privacy class.
- Trade-off: every workload that wants to produce institutional effects needs a covering mandate. This is the right cost; silent dispatch through compute is precisely what the ADR forecloses.
- Trade-off: the privacy class enforcement requires runtime support for re-disclosure prevention. Not all of that exists today; the ADR records the contract and registry candidate ADR-0082 will record the implementation policy.

## Implementation status

Implemented for the manifest and core boundary; partial for the privacy/locality enforcement.

Evidence:
- Task type and fields: [icn/crates/icn-compute/src/task.rs](../../icn/crates/icn-compute/src/task.rs), [icn/crates/icn-compute/src/types.rs](../../icn/crates/icn-compute/src/types.rs).
- Executor surface and fuel: [icn/crates/icn-compute/src/executor.rs](../../icn/crates/icn-compute/src/executor.rs).
- Gateway compute manager: [icn/crates/icn-gateway/src/compute_mgr.rs](../../icn/crates/icn-gateway/src/compute_mgr.rs).
- Trust-gated admission via PolicyOracle: see ADR-0014 for the boundary.
- Test coverage: [icn/crates/icn-compute/tests/compute_integration.rs](../../icn/crates/icn-compute/tests/compute_integration.rs).
- Federation cross-signing: [icn/crates/icn-compute/tests/federation_integration.rs](../../icn/crates/icn-compute/tests/federation_integration.rs).

What is not yet covered by this ADR (and is intentionally out of scope here):
- Result verification model — registry candidate ADR-0080.
- WASM module registry governance — registry candidate ADR-0081.
- Privacy / data locality / selective disclosure — registry candidate ADR-0082.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Allow compute to dispatch institutional effects directly | Collapses the kernel/app separation. Compute under a mandate can produce effect records; without a mandate, it cannot. |
| Make all compute output "governance-grade" by default | Forces unnecessary determinism on assistive use cases. The class system is the right granularity. |
| Treat each compute scope (Commons / Coop / Federation) as a separate ADR | They share the boundary contract. The settlement and admission differences are scope-specific; ADR-0031 handles the commons specifics. |
| Defer this ADR until LLM-style assistive compute lands | Earlier is cheaper. Without a written boundary now, assistive compute will be tempted to grow decision authority by default. |
