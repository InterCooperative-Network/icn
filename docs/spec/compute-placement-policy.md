---
Status: normative
Authority: spec (defines forward-direction placement policy primitives consumed by `DomainPolicy` per `docs/spec/institutional-domain.md` and the compute substrate per ADR-0030 / ADR-0031; some clauses are explicitly forward-direction)
Canonical: no
Last Reviewed: 2026-05-14
---

# Compute Placement Policy

> **Status: spec, work-in-progress.** Defines the placement policy that sits between ADR-0030 (workload manifest and authority boundary) and ADR-0031 (commons admission and settlement policy): the policy decision a workload passes through before it is admitted, executed, or rejected. Names placement classes, decision inputs, decision outputs, fallback behavior, dashboard/member-shell rendering, and the vocabulary boundaries (execution budget vs fuel vs capacity vs allocation; local-domain vs cooperative; settlement vs payment) that this spec deliberately fixes. The PR introducing this doc advances `#1801` without closing it.

<!-- truth-class: normative -->

## Purpose

ICN's compute substrate already has two ratified pieces:

- **ADR-0030 — Compute Workload Manifest and Authority Boundary** defines `ComputeTask` and the authority boundary every workload must satisfy.
- **ADR-0031 — Commons Compute Admission and Settlement Policy** defines how commons-eligible workloads are admitted and how settlement is receipted.

What is still missing — what `#1801` exists to define — is the **placement layer between them**: given a workload manifest, given a domain's policy, given the available executors, where should the workload run? Under what authority? With what fallback when the preferred placement fails? With what dashboard rendering for operators and what shell language for members?

This spec defines the placement policy contract. It does not implement a scheduler, does not redefine `ComputeTask`, and does not add new receipt classes beyond those already named in ADR-0026 (receipt envelope), ADR-0030 (compute outputs), and ADR-0031 (settlement). Where this spec touches the scope vocabulary, it consumes the doctrine landed in `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 "Entity-scope vocabulary."

## What this spec is not

- Not a scheduler implementation. No Rust code lands here.
- Not a redefinition of `ComputeTask` (ADR-0030), the seven runtime classes from `docs/spec/governed-service-binding.md`, the storage durability objects from `docs/spec/storage-durability-policies.md`, the receipt envelope from ADR-0026, or the commons settlement engine from ADR-0031. Where this spec touches those areas, the existing ADR or spec is authoritative.
- Not authority to mutate DNS, K3s, Forgejo, the runtime, the scheduler, the gateway, the SDK, deploy scripts, or any deployed infrastructure.
- Not a production-rollout claim. No partner federation is operating compute under this policy today.
- Not a redefinition of the meaning firewall (per `docs/architecture/KERNEL_APP_SEPARATION.md`). The kernel does not import `PlacementDecision`, `PlacementClass`, `ExecutorAdmissionDecision`, or the other forward-direction objects this spec names.
- Not a rename of Rust code. `FuelLimit`, `fuel_limit`, `payment_rate`, `payment_currency`, `DataLocality::CoopReplicated`, the `icn-coop` crate, and the `coop_core` module paths are out of scope for this PR. The vocabulary-boundary section below names the spec-facing terms; legacy implementation identifiers are preserved and tracked in a named follow-up.
- Not a new receipt class. The receipts a placement decision produces (placement, admission, execution, output, settlement, fallback) all map onto receipt classes already named in ADR-0026 and ADR-0031. See §"Receipts" below.
- Not a NYCN-specific or any other institution-specific placement policy. Institution packages bring their own role identifiers and ceremony markers; this spec stays generic.
- Not closure of `#1801`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's acceptance criteria.

## Vocabulary boundaries

This spec deliberately fixes three vocabulary boundaries that the placement layer has been drifting across. The boundaries do not require renaming any existing Rust identifier today; they bind the spec-facing terms a future schema and runtime should adopt.

### Scope vocabulary (per `INSTITUTION_PACKAGE_BOUNDARY.md` §C3)

- Generic scope language uses `LocalDomain`, `InstitutionalDomain`, `Domain`, `DomainPolicy`, and "owning entity class."
- Generic scope language does **not** use `Coop` or `Cooperative` as a stand-in for the local institutional scope. `Cooperative` is one possible owning entity class alongside `Community`, `Federation`, `Individual`, and other governed classes permitted by policy.
- Existing serialized `Coop`-prefixed identifiers in code (`DataLocality::CoopReplicated`, ADR-0030's `Coop(coop_id)` scope tag, ADR-0031's `Coop-scoped` prose) are preserved with naming notes; new spec text uses the corrected form.

### Execution vs capacity vocabulary

This boundary preserves the existing low-level runtime field while introducing the correct policy-facing term and reserving `capacity` for its proper meaning.

| Term | Meaning | Where it applies |
|---|---|---|
| `execution budget` | The policy-facing term for the bounded execution guard. The total amount of bounded computation a workload is permitted to consume before it is preempted or rejected. | Spec, policy, operator/steward surfaces, member-facing language. |
| `fuel_limit` / `FuelLimit` | The current implementation field on `ComputeTask` (per ADR-0030 and `icn-compute/src/types.rs`) that holds the runtime bounded-execution guard. | Code only. Preserved verbatim. |
| `capacity` | Executor / node resource availability: CPU, memory, storage, network, accelerator availability, and related placement-fit signals. **Not** an alias for fuel. | Executor admission, placement fit, operator surfaces. |
| `resource envelope` | The workload's requested CPU / memory / storage / network shape, as declared in the manifest. | Spec, policy, operator surfaces. Sometimes called "resource profile" interchangeably; the two terms are synonymous in this spec. |
| `allocation` | A governed permission to consume capacity. An admitted task carries an allocation. Allocations are receipted and time-bounded. | Spec, policy, dashboards. |
| `settlement` | The receipted accounting outcome after execution. **Not** "payment." | Per ADR-0031 and `#1634`. |

**Compatibility note.** Current compute code uses `FuelLimit` / `fuel_limit` as the bounded-execution guard. This spec uses `execution budget` as the policy-facing term and preserves `fuel_limit` as the implementation field. `Capacity` is not an alias for fuel; capacity refers to executor / node resource availability.

### Settlement vs payment vocabulary

ICN-native compute surfaces use settlement / unit / position / obligation / allocation / receipt framing. They do not use payment / currency / balance / wallet framing — see ADR-0031 §"Settlement policy" and `#1634`. The known legacy drift in `ComputeTask` (`payment_rate`, `payment_currency` fields on the existing struct) is preserved as code without endorsement and is the subject of a named follow-up; see §"Known drift" below.

## Placement classes

Seven closed placement classes. New classes require a spec amendment.

1. **`LocalOnly`** — the workload must execute on the submitting member's device or on a node within the workload's `LocalDomain`. Privacy class, data locality, or domain policy forbid wider placement. No federation or commons exposure.
2. **`DomainLocalPreferred`** — local execution is preferred but the workload may fall back to a `LocalDomainBound` executor (see below) inside the same `LocalDomain` if local capacity is unavailable. Fallback emits a `PlacementFallbackReceipt`.
3. **`LocalDomainBound`** — the workload runs on an executor bound to the workload's `LocalDomain` (the `InstitutionalDomain` owned by the workload's institution, regardless of whether that owning entity class is `Cooperative`, `Community`, `Federation`, `Individual`, or another governed class). This replaces the historical `CoopBound` class; existing `Coop`-prefixed names in ADR-0030 / ADR-0031 carry naming notes pointing here. Authority basis is the domain's adopted compute placement policy under `DomainPolicy`.
4. **`FederationBound`** — the workload runs on a federation-bound executor under an explicit federation agreement. Authority basis is the federation agreement plus the domain's policy authorization to use it. No federation execution without an agreement.
5. **`CommonsEligible`** — the workload is admitted to the commons compute pool per ADR-0031. Privacy and determinism constraints from ADR-0031 §"Admission policy" apply. Settlement is receipted per ADR-0031 §"Settlement policy."
6. **`ExternalCustodianRequired`** — the workload requires an external bridge / custodian executor under an explicit bridge or custodian policy. No external custodian execution without such a policy. This class exists to make the requirement explicit rather than implicit.
7. **`RejectedByPolicy`** — no placement satisfies the workload's constraints under the domain's current policy. A `PlacementRejected` artifact is produced with the rejection reason, the failing constraint, the fallback options (if any) the policy offers, and the appeals path (if any) the domain provides.

The class is selected by the placement decision (§"Decision contract" below) and recorded in the resulting `PlacementDecision` or `PlacementRejected` artifact. The kernel does not import the placement classes; the policy oracle returns a `ConstraintSet` derived from the placement class plus the manifest, and the kernel enforces that constraint set blindly (per `docs/architecture/KERNEL_APP_SEPARATION.md` §"Meaning firewall").

## Placement hierarchy

The default posture is local-first. The hierarchy below is the **default**; per-domain policy may override it, but every override needs an authority basis and a receipt path.

```text
1. Member device / local client cache when appropriate
2. Local domain node
3. Local-domain-bound executor
4. Federation-bound executor under explicit agreement
5. Commons compute pool under admission / settlement policy
6. External bridge / custodian only by explicit agreement and policy
```

The hierarchy is not absolute. Privacy, resource needs, reliability, determinism class, executor trust, federation agreement state, and governance policy may all override it. When the hierarchy is overridden, the override is recorded in the `PlacementDecision` with the policy clause that authorized it.

## Decision contract

The placement decision is a pure function of the inputs named below. It is evaluated by a policy oracle (in the sense of `docs/architecture/KERNEL_APP_SEPARATION.md`); the kernel does not evaluate it. The result is one of `PlacementDecision`, `PlacementRejected`, or `ReviewRequiredActionCard` (when the workload is advisory and the policy requires human ratification before placement is final).

### Candidate inputs

The placement decision consumes:

- **Domain id / scope.** The `LocalDomain` (per `docs/spec/institutional-domain.md`) the workload is being submitted within.
- **Submitting actor / service identity.** The DID of the actor or service submitting the workload.
- **Authority basis / mandate ref.** The mandate (per ADR-0014 / ADR-0019) covering the submission, if any. Required for governance-grade workloads; optional for advisory.
- **Workload kind.** A coarse logical category the workload declares: one of CCL evaluator / WASM workload / SQL-reference / model or advisory output / transform / verification. Workload kind is **distinct from runtime class**: a single kind may run on more than one runtime class (a WASM workload may target either the deterministic legitimacy compute runtime or the utility computation runtime, depending on determinism class), and the runtime-class taxonomy itself lives in `docs/spec/governed-service-binding.md` §"Runtime classes" (the seven closed classes: deterministic legitimacy compute, utility computation, container, microVM, accelerator, local device, external bridge). The placement decision consumes both: workload kind to constrain the policy space, runtime class to constrain the executor set.
- **Privacy class.** The manifest declares a privacy class. ADR-0030's text names three classes (`Public`, `Encrypted`, `Sealed`) at the design level. The implemented `PrivacyClass` enum in `icn/crates/icn-compute/src/types.rs` carries different variants (`Public`, `Member`, `NeedToKnow`) reflecting member-disclosure semantics from later work. Forward-direction richer taxonomies are tracked in `#1792`. Reconciliation of the variant names between ADR-0030 and the implementation is **forward work** (candidate `spec(privacy): reconcile PrivacyClass naming` follow-up; same disposition as `docs/spec/governed-service-binding.md` §"Constraints" → "Privacy class"). **Downstream manifests SHOULD use the implementation's current variant names** (`Public` / `Member` / `NeedToKnow`) for round-trip compatibility until the reconciliation lands.
- **Determinism class.** Governance-grade (deterministic, fuel-bounded, reproducible) vs advisory (utility, model-assisted, not authoritative without ratification), per ADR-0030 §"Determinism class."
- **Data locality requirement.** The locality the workload's inputs and outputs are constrained to, per `docs/spec/storage-durability-policies.md` §"Locality and privacy inheritance" and the kernel `DataLocality` enum.
- **Execution budget / `fuel_limit`.** The bounded execution the workload is permitted to consume, expressed as the runtime `FuelLimit` field. Spec-facing term is `execution budget`; runtime field is preserved.
- **Resource profile / resource envelope.** The workload's declared CPU / memory / storage / network shape.
- **Deadline / priority.** When the workload must complete by, and its priority class relative to other workloads.
- **Executor capabilities.** The required `ExecutorCapability` set per `icn-compute/src/types.rs`.
- **Executor capacity.** The candidate executors' available capacity at the moment of decision. Used for fit, not for fuel accounting.
- **Trust / admission class.** The trust score and admission class of candidate executors per `apps/trust` and ADR-0031 §"Admission policy."
- **Federation agreement refs.** The federation agreements the workload's domain has adopted, if any. Required for `FederationBound`.
- **Commons pool policy refs.** The commons pool's admission policy refs, per ADR-0031.
- **Allocation policy refs.** The domain's `DomainPolicy`-adopted allocation policy: who may allocate capacity, with what limits, against which mandates.
- **Settlement unit / position policy refs.** The settlement policy refs covering the workload (per `#1634` and ADR-0031).
- **Required review state.** Whether the workload requires human ratification before its output becomes institutional state. Advisory workloads almost always require review; governance-grade workloads have it embedded in the mandate.

### Candidate outputs

The decision contract has two layers of output, and each layer's outputs are mutually exclusive within that layer; outputs across layers compose.

**Layer 1 — policy-oracle return value.** Given the candidate inputs, the placement policy oracle returns **exactly one** of:

- **`PlacementDecision`** — placement class, selected executor or pool reference, authority basis (which policy clause and which mandate authorized the placement), resource envelope, execution budget, settlement estimate, expected receipt classes. Records which `LocalDomain`, which `DomainPolicy` version, which evaluator (per `docs/spec/ccl-policy-registry.md`).
- **`PlacementRejected`** — rejection reason, the specific constraint that failed (privacy class, locality, determinism, capacity, agreement, trust, policy-clause), fallback options the policy offers (if any), the appeals path (if any). Recorded as an effect-dispatch evidence artifact, not as a governance-authoritative effect.

A `PlacementDecision` MAY additionally carry an attached **`PlacementFallbackReceipt`** when the chosen placement class was reached by walking down the hierarchy from a preferred class. The fallback receipt is an auxiliary artifact on the decision, not a separate Layer 1 return value.

A `PlacementDecision` MAY also surface a **`ReviewRequiredActionCard`** when the workload is advisory and the policy requires human ratification before its output is treated as institutional state. The action card surfaces in the operator dashboard or member shell per `#1818` / `#1795`. The action card is rendered alongside the decision, not in place of it.

**Layer 2 — post-placement artifacts.** After Layer 1 returns a `PlacementDecision`, the executor (or commons pool) emits:

- **`ExecutorAdmissionDecision`** — emitted by the selected executor / commons pool. Records whether the executor admitted the workload, whether the admission is conditional (e.g., must wait for capacity), and any executor-side constraints (timeouts, retry limits). A refusal here does not invalidate the placement decision; it triggers a new placement attempt only if the domain's policy permits, per §"Fallback behavior."

**Naming clarification.** None of `PlacementDecision`, `PlacementRejected`, `PlacementFallbackReceipt`, `ExecutorAdmissionDecision`, or `ReviewRequiredActionCard` is a new entry in the ADR-0026 receipt-class taxonomy. They are **names of evidence artifacts** that travel inside existing receipt envelopes: `PlacementDecision` and `PlacementRejected` land as `EffectDispatchEvidence` per `docs/spec/effect-dispatch-contract.md` §"Stage 5 — Application and evidence"; `PlacementFallbackReceipt` is an attachment on a `PlacementDecision` evidence record; `ExecutorAdmissionDecision` lands at the executor / commons-pool boundary per ADR-0031 §"Admission policy"; `ReviewRequiredActionCard` is an action-card UX surface per `#1818` / `#1795`, not a receipt class. See §"Receipts" below for the full mapping.

`ComputeReceipt`, `OutputArtifactReceipt`, and `SettlementReceipt` are emitted by the post-execution chain (per ADR-0030, ADR-0031, and `docs/spec/artifact-registry-and-scoped-vault.md`) and are not produced by the placement decision itself; they are referenced from the `PlacementDecision` via `expected receipt classes`.

## Boundary rules

The following rules are load-bearing. A placement decision that violates any rule is invalid and must be rejected. A scheduler implementation that admits a workload violating any rule is a meaning-firewall violation.

1. **Privacy class is a hard upper bound.** No placement may move data, computation, or output to a wider privacy class than the workload's declared class. A `NeedToKnow` workload may not run on a federation executor unless the federation agreement explicitly extends `NeedToKnow` to that path; even then the receipt records the extension and its authority basis. The same rule applies to the richer privacy class taxonomies forward-tracked in `#1792`.
2. **Data locality is a hard upper bound.** No placement may move inputs or outputs to a wider `DataLocality` than the workload's declared locality. Inheritance is per `docs/spec/storage-durability-policies.md` §"Locality and privacy inheritance"; placement is one of the inheritance edges.
3. **Mandate-covering is required for institutional effects.** No compute output may dispatch an institutional effect (cause a state change recorded as an `InstitutionalEffectRecord` per ADR-0025) without a covering mandate. The placement decision verifies the mandate; the executor verifies it again at execution time; the kernel enforces it at effect dispatch. This is per ADR-0030 §"Compute outputs feed governance; they do not replace governance."
4. **Federation execution requires an explicit agreement.** No `FederationBound` placement without a federation agreement adopted by the domain. Agreement-absence is a `RejectedByPolicy` rejection, not an implicit fallback to commons or local.
5. **Commons admission goes through ADR-0031.** No `CommonsEligible` placement that bypasses the commons admission policy (affiliated / unaffiliated buckets, sybil checks, scope-aware admission per ADR-0031). The placement decision selects the commons pool; the commons pool's admission engine decides whether to admit.
6. **External custodian execution requires an explicit policy.** No `ExternalCustodianRequired` placement without an external-bridge or custodian policy adopted by the domain. Like the federation case, custodian-absence is a `RejectedByPolicy` rejection, not an implicit fallback.
7. **Settlement vocabulary is reserved.** No placement decision, dashboard, or member-shell surface uses payment / currency / balance / wallet / crypto / token / timebank framing for ICN-native compute. Use settlement / unit / position / obligation / allocation / receipt. The legacy `payment_rate` / `payment_currency` fields on `ComputeTask` are tracked in §"Known drift" for separate reconciliation.
8. **Package-specific nouns stay in packages.** No placement core uses `Coop` / `Cooperative` as a stand-in for the generic local-institutional scope (per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3), and no placement core uses institution-package-specific nouns (NYCN, named partner federations, named member cooperatives) as generic primitives. The structural scope names are `LocalDomain`, `InstitutionalDomain`, `Domain`, `DomainPolicy`; the structural placement-class names `Commons` and `Federation` are kept because they refer to structural concepts (the commons compute pool per ADR-0031; a federation as a structural higher-scope entity), not to specific inhabitant nouns. Institution packages bring their own role identifiers, ceremony markers, and partner labels.
9. **No production-readiness claim.** No clause in this spec is a claim that ICN-native compute is production-ready, that a federation is live, that NYCN is a formal pilot, or that any partner is operating workloads under this policy today. All such claims are out of scope.

## Fallback behavior

Fallback is structured. The decision algorithm walks the placement hierarchy (or the override the policy dictates) in order. When a preferred class is unavailable:

1. **Capacity-bound fallback.** If the preferred class has no executor with the required `ExecutorCapability` plus available capacity within the deadline, the decision falls back to the next class **only if** the next class satisfies privacy, locality, determinism, and authority. Otherwise the decision is `RejectedByPolicy`.
2. **Authority-bound fallback.** If the preferred class requires an authority basis the submitting actor does not hold (e.g., `FederationBound` requires a federation agreement; submitter has no covering mandate), the decision falls back to the next class **only if** that next class is permitted under the domain's policy for the workload's privacy and determinism class. Otherwise `RejectedByPolicy`.
3. **Fallback emits a receipt.** Every fallback emits a `PlacementFallbackReceipt` recording the original preferred class, the chosen fallback class, the reason, and the authority basis for the fallback decision.
4. **Fallback is not retry.** A placement decision that has fallen back is a final placement until the workload completes or the executor rejects admission. Retry-after-admission-failure is a separate concern handled by the scheduler implementation; this spec does not specify retry semantics beyond noting that an `ExecutorAdmissionDecision` that returns "rejected" produces a new placement decision attempt only if the policy permits it.
5. **No silent fallback to commons.** A workload submitted as `LocalOnly`, `DomainLocalPreferred`, or `LocalDomainBound` may not silently fall back to `CommonsEligible`. Commons admission requires explicit policy authorization for that workload class; the absence of local capacity is not sufficient.

## Example policies to specify

These are example domain policies a real `DomainPolicy` document might adopt. They are illustrative, not authoritative.

### Public advisory workload

A public meeting summary (advisory, utility-compute, non-deterministic) may be `CommonsEligible` when:

- inputs are public or have been explicitly approved for commons execution by the domain;
- output is marked `advisory`;
- review is required before the output becomes institutional state;
- the placement receipt links input and output hashes for provenance.

The same workload becomes `LocalDomainBound` (or `LocalOnly`) when any input fails the public-approval check.

### Private care / accessibility workload

A care plan, accommodation request, or other private member workload must be `LocalOnly` or `LocalDomainBound` when:

- inputs live in a scoped vault (per `docs/spec/artifact-registry-and-scoped-vault.md` §"ScopedVault");
- privacy class forbids federation or commons execution;
- output requires restricted storage with `DataLocality::CellLocal` or `DataLocality::CoopReplicated` (read as `LocalDomain`-replicated per `INSTITUTION_PACKAGE_BOUNDARY.md` §C3);
- access and export are receipted per ADR-0026.

Federation or commons placement of such a workload is a `RejectedByPolicy` outcome unless the domain has adopted an explicit, narrow, receipted policy authorizing a wider scope for that specific class of workload.

### Federation report workload

A federation-level analysis or report may be `FederationBound` when:

- a `ComputeAgreement` (per ADR-0030 §"Federation agreements" and the federation spec forward-tracked in `#1799`) exists between the participating domains;
- each contributing domain authorized its share of the input data for federation processing;
- the output redaction policy is defined and adopted;
- settlement and commons accounting are receipted per ADR-0031.

Absence of a `ComputeAgreement` reduces the placement to `LocalDomainBound` per contributing domain, with each domain producing its own report fragment.

### Deterministic governance-grade workload

A governance-grade verification (CCL evaluator output, mandate verification, ledger reconciliation) must run only where:

- determinism class is satisfied (deterministic CCL / WASM executor, no model output);
- `fuel_limit` (execution budget) and capability limits apply per ADR-0021;
- the result can be re-executed or verified by another executor;
- no non-deterministic model output becomes authoritative.

Governance-grade workloads are typically `LocalOnly` or `LocalDomainBound`; `CommonsEligible` is permitted only when the commons pool runs a deterministic substrate per ADR-0031 §"Admission policy" and the policy explicitly authorizes commons execution for that workload class.

## Operator / steward dashboard

The operator and steward surfaces (per `#1795` steward cockpit and `#1818` member shell) render placement state using the vocabulary fixed in §"Vocabulary boundaries." The dashboard shows:

- **Placement decision.** The placement class chosen (`LocalOnly` / `DomainLocalPreferred` / `LocalDomainBound` / `FederationBound` / `CommonsEligible` / `ExternalCustodianRequired` / `RejectedByPolicy`).
- **Runner / executor class.** Which runtime class (per `docs/spec/governed-service-binding.md`) the executor belongs to.
- **Scope.** The `LocalDomain` the workload is bound to and any wider scope the placement authorizes.
- **Privacy class.** Per the workload manifest.
- **Determinism class.** Governance-grade or advisory.
- **Resource envelope.** The workload's declared CPU / memory / storage / network shape.
- **Execution budget.** The bounded execution the workload is permitted (the policy-facing name for `fuel_limit`).
- **Capacity fit.** Whether the selected executor has capacity for the workload's resource envelope at the moment of placement.
- **Allocation decision.** The governed permission to consume capacity, including the mandate ref (if any) under which the allocation was made.
- **Federation / commons agreement ref.** When applicable.
- **Output artifact ref.** When the workload has produced an output artifact, the reference into the ArtifactRegistry per `docs/spec/artifact-registry-and-scoped-vault.md`.
- **Settlement receipt.** The settlement receipt ref per ADR-0031 (not "payment receipt").
- **Review requirement.** Whether the output is awaiting human ratification.
- **Failure / fallback reason.** When the placement fell back or rejected, the reason and the authority basis for the fallback (or the failing constraint for the rejection).

No raw scheduler jargon on operator surfaces. No `fuel` framing; use `execution budget`. No `payment` framing; use `settlement`. No `Coop` framing in generic operator copy; use `LocalDomain` (institution packages may localize further).

## Member shell

The member shell (per `#1818` and `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`) renders only plain participation status:

- local execution;
- processed inside your institution;
- sent to a federation executor under agreement;
- commons compute pending;
- review required;
- receipt available;
- sync delayed / degraded.

The phrasing "processed inside your institution" handles cooperative-, community-, and federation-owned domains identically without preferring one inhabitant class. An institution package may localize further (NYCN may say "processed inside your cooperative"; a care-network package may say "processed inside your collective") — but the generic core string does not pre-decide.

No raw scheduler jargon. No `fuel` framing. No `payment` framing. No package-specific nouns in the generic strings; institution packages localize.

## Receipts

Placement decisions emit receipts via the receipt envelope defined in ADR-0026 and the effect dispatch chain in `docs/spec/effect-dispatch-contract.md`. This spec does not add new receipt classes to the ADR-0026 taxonomy: the artifact names below are evidence-artifact identifiers that travel inside `EffectDispatchEvidence` or existing executor-side records, not new top-level receipt classes. The receipts placement work touches:

- **`PlacementDecision`** is recorded as a Stage 5 `EffectDispatchEvidence` artifact (per `docs/spec/effect-dispatch-contract.md` §"Stage 5 — Application and evidence"). It is not authoritative state by itself; the authoritative effect comes from execution.
- **`PlacementRejected`** is recorded as a Stage 5 `EffectDispatchEvidence` artifact. It does not produce an `InstitutionalEffectRecord`; it produces an evidence record capturing the rejection.
- **`PlacementFallbackReceipt`** is an evidence attachment carried on a Stage 5 `EffectDispatchEvidence` record alongside the `PlacementDecision` it accompanies. It is not a new receipt class; it is an attachment field on the existing evidence envelope. Its retention horizon is the same as the parent `PlacementDecision` evidence record.
- **`ExecutorAdmissionDecision`** is recorded at the executor / commons-pool boundary per ADR-0031 §"Admission policy."
- **`ComputeReceipt`** is recorded per ADR-0030 and ADR-0031 at execution completion. Carries the `policy_version_id` of the placement policy that authorized the placement.
- **`OutputArtifactReceipt`** is recorded per `docs/spec/artifact-registry-and-scoped-vault.md` when the workload produces an artifact. Carries `receipt_refs` linking back to the `PlacementDecision` and `ComputeReceipt`.
- **`SettlementReceipt`** is recorded per ADR-0031 §"Settlement policy" at settlement completion.
- **`ReviewRequiredActionCard`** is rendered on the steward cockpit / member shell per `#1818` / `#1795` and resolves into a `RatificationReceipt` (forward-direction, tracked in `#1818`) when the advisory output is ratified or rejected.

The retention horizon for each receipt class is set by the domain's `DomainPolicy` (per `docs/spec/institutional-domain.md` §"Receipts → Receipt retention defaults") and follows the receipt-class summary in `docs/spec/effect-dispatch-contract.md`.

## Failure and safety table

| Failure | Where it surfaces | Disposition |
|---|---|---|
| Workload manifest missing required input field | Placement decision | `RejectedByPolicy` with the missing field named. |
| Workload manifest declares wider data locality than its inputs | Placement decision | `RejectedByPolicy` per Boundary rule 2. |
| Workload privacy class wider than inputs' privacy class | Placement decision | `RejectedByPolicy` per Boundary rule 1. |
| Governance-grade workload submitted without covering mandate | Placement decision | `RejectedByPolicy` per Boundary rule 3. |
| `FederationBound` requested without federation agreement | Placement decision | `RejectedByPolicy` per Boundary rule 4. |
| `CommonsEligible` requested but commons admission engine refuses | Executor admission | `ExecutorAdmissionDecision` with refusal reason; placement may fall back per §"Fallback behavior" if policy permits. |
| `ExternalCustodianRequired` requested without external policy | Placement decision | `RejectedByPolicy` per Boundary rule 6. |
| Local executor unavailable for `LocalOnly` workload | Placement decision | `RejectedByPolicy` with `LocalOnly` constraint named. No silent fallback. |
| `DomainLocalPreferred` falls back to `LocalDomainBound` due to local capacity unavailability | Placement decision | `PlacementDecision` with `LocalDomainBound` class and a `PlacementFallbackReceipt` attached. |
| Advisory workload submitted with no review policy | Placement decision | `RejectedByPolicy` per Boundary rule 3 (no path to authoritative output). |
| Executor admits and then preempts due to capacity exhaustion | Executor / scheduler | `ExecutorAdmissionDecision` followed by a preemption event; not specified here (scheduler concern). Re-placement requires a new placement decision. |
| Workload's `fuel_limit` exceeds the executor's per-task ceiling | Executor admission | `ExecutorAdmissionDecision` with refusal reason. May fall back to a higher-budget executor class only if policy permits. |
| Workload requires accelerator capacity that no candidate executor has | Placement decision | `RejectedByPolicy` with "capacity-fit failure" reason; the operator dashboard surfaces the failing `ExecutorCapability`. |
| Settlement policy reference missing for a `CommonsEligible` workload | Placement decision | `RejectedByPolicy` per ADR-0031 §"Settlement policy"; commons admission requires a settlement reference. |
| Trust score of candidate executor below the workload's required admission class | Executor admission | `ExecutorAdmissionDecision` with refusal reason; placement may fall back per policy. |
| `RejectedByPolicy` outcome with no appeals path defined | Member shell / steward dashboard | The rejection surfaces with the failing constraint named; the domain's policy is responsible for offering (or not offering) an appeals path; this spec does not require one. |

## Known drift (preserved without endorsement)

This spec deliberately preserves several legacy code identifiers and ADR phrasings to keep the doc-only PR scoped. Each is named so future PRs can address it with proper migration discipline.

1. **`FuelLimit` / `fuel_limit`** in `icn-compute/src/types.rs` and `ComputeTask`. Preserved verbatim. The spec-facing term is `execution budget`. Rename or alias is out of scope for this PR; tracked as a follow-up.
2. **`payment_rate` and `payment_currency`** fields on `ComputeTask`. These conflict with the settlement vocabulary fixed in §"Vocabulary boundaries" and Boundary rule 7. Preserved without endorsement; their reconciliation belongs to a separate spec/code follow-up named in the PR handoff.
3. **`DataLocality::CoopReplicated`** in `icn-kernel-api/src/storage.rs`. Preserved verbatim as the kernel enum variant (numeric value `1`, serialized as `coop_replicated`). The placement decision uses `LocalDomain`-replicated framing in spec text; the implementation field is unchanged. A rename / alias migration is a separate refactor — see the follow-up draft in the entity-scope vocabulary boundary handoff.
4. **ADR-0030 `Coop(coop_id)` scope tag** and **ADR-0031 `Coop-scoped` admission/settlement prose.** Both carry naming notes per `INSTITUTION_PACKAGE_BOUNDARY.md` §C3 and are read as `LocalDomain(domain_id)` / `LocalDomain-scoped`. The ADR text is not rewritten.
5. **`icn-coop` crate name** and **`apps/membership/coop_core/*` paths.** Out of scope for this doc-only PR. Renaming would be a multi-crate refactor with downstream SDK consumer impact.
6. **`coop-scoped` comments in `icn-rpc` source** (`crates/icn-rpc/src/server.rs`, `crates/icn-rpc/src/handler/*.rs`, `crates/icn-rpc/src/auth.rs`). These are existing Rust comments and TODO references in implemented code, not generic vocabulary. Preserved verbatim.

## First safe proof-loop / dogfood slice

Per the `#1801` acceptance criterion "Spec identifies first safe proof-loop or dogfood slice without starting implementation":

The first safe slice is a **read-only placement-decision rehearsal**: given a representative `ComputeTask` manifest, the placement policy oracle returns a `PlacementDecision` artifact (or `PlacementRejected` artifact) with the placement class, the authority basis, and the failing constraint (if any), without invoking any executor. This produces evidence that the policy oracle is wired correctly and that the placement-class taxonomy covers the realistic workload mix, before any scheduler change or executor admission code is touched.

A second safe slice is a **dry-run fallback exercise**: submit a workload that would prefer `DomainLocalPreferred` and synthetically constrain local capacity to zero, then verify the resulting `PlacementDecision` records `LocalDomainBound` plus a `PlacementFallbackReceipt`. Again, no executor admission code change; this exercises the decision contract, not the runtime.

Both slices are forward-direction. Neither is implemented in this PR. They are named so the next implementation PR has an unambiguous first target.

## Cross-links

- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — §C3 "Entity-scope vocabulary," the doctrine this spec consumes for `LocalDomain` / `LocalDomainBound` / "processed inside your institution."
- `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` — the integrating spine that names compute placement as one of the policy decisions a `DomainPolicy` adopts.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — the meaning firewall that keeps placement classes and policy decisions out of the kernel.
- `docs/spec/institutional-domain.md` — `InstitutionalDomain`, `LocalDomain`, and `DomainPolicy` (which adopts the placement policy this spec defines).
- `docs/spec/effect-dispatch-contract.md` — Stage 5 evidence is where `PlacementDecision` and `PlacementRejected` artifacts land.
- `docs/spec/governed-service-binding.md` — the seven runtime classes the placement decision consumes; the binding lifecycle that consults placement at "Allocate."
- `docs/spec/storage-durability-policies.md` — locality and disclosure inheritance rule that placement must respect.
- `docs/spec/artifact-registry-and-scoped-vault.md` — `OutputArtifactReceipt` and `ScopedVault` custody for placement outputs.
- `docs/spec/ccl-policy-registry.md` — `policy_version_id` provenance for the placement policy version that authorized each decision.
- `docs/adr/ADR-0014-constitutional-object-model.md` — `AuthorityClass` / `AuthorityGrant` / `TypedScope` / `Mandate`.
- `docs/adr/ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md` — grant minting and mandate persistence.
- `docs/adr/ADR-0021-ccl-determinism-fuel-and-capability-safety.md` — determinism class and fuel/capability safety for governance-grade workloads.
- `docs/adr/ADR-0025-institutional-effect-record-canonical-schema.md` — `InstitutionalEffectRecord`.
- `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md` — the receipt envelope this spec's outputs travel inside.
- `docs/adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md` — `ComputeTask`, authority boundary, naming note for `Coop(coop_id)` → `LocalDomain(domain_id)`.
- `docs/adr/ADR-0031-commons-compute-admission-and-settlement-policy.md` — commons admission and settlement; naming note for `Coop-scoped` → `LocalDomain-scoped`.
- Issues: `#1801` (this spec), `#1794` (`InstitutionalDomain`), `#1795` (steward cockpit), `#1796` (proof-level taxonomy), `#1797` (effect dispatch), `#1798` (ArtifactRegistry / ScopedVault), `#1799` (network anti-entropy), `#1815` (governed service binding), `#1816` (storage durability), `#1817` (CCL policy registry), `#1818` (member shell), `#1792` (private data disclosure boundary), `#1634` (obligation / allocation / settlement primitives).

## Non-claims (repeat block for grep clarity)

- This spec does not claim production readiness for ICN-native compute.
- This spec does not claim any partner federation is operating workloads under this policy today.
- This spec does not claim NYCN is a formal pilot.
- This spec does not rename `FuelLimit`, `fuel_limit`, `payment_rate`, `payment_currency`, `DataLocality::CoopReplicated`, the `icn-coop` crate, or the `coop_core` module paths.
- This spec does not introduce new receipt classes; it references existing classes from ADR-0026 / ADR-0030 / ADR-0031 / `docs/spec/artifact-registry-and-scoped-vault.md`.
- This spec does not implement a scheduler, an executor, an admission engine, or a settlement engine.
- This spec does not specify retry semantics beyond noting that an admission refusal may produce a new placement-decision attempt if policy permits.
- This spec does not authorize any change to the kernel surface, the gateway, the SDK, the website, the deploy scripts, K3s, DNS, Forgejo, or any deployed infrastructure.
- This spec does not introduce schema or contract changes.
- This spec does not use unsafe vocabulary (payment, wallet, balance, currency, token, timebank) for ICN-native compute surfaces.
