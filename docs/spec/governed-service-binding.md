---
Status: normative
Authority: spec (defines the forward-direction primitives the spine doc names under §"Governed workloads and service envelopes" and tracks under #1815; some clauses are explicitly forward-direction)
Canonical: no
Last Reviewed: 2026-05-14
---

# Governed Service Binding, Workload Manifest, and Runtime Provider

> **Status: spec, work-in-progress.** Defines `GovernedServiceBinding`, `WorkloadManifest`, and `RuntimeProvider` as the design-level integrating envelope for hosted services, installable tools, compute jobs, CCL evaluators, and future container / microVM workloads. The spec generalizes existing types (`ComputeTask` from ADR-0030, `ToolManifest` / `ToolBinding` from RFC-0017) without redefining them. Clauses that depend on schema fields or kernel behavior that have not yet landed are marked as forward-direction. The PR introducing this doc advances `#1815` without closing it.

## Purpose

The spine doc names three primitives — `GovernedServiceBinding`, `WorkloadManifest`, `RuntimeProvider` — as the missing-middle concept that ties together hosted services, installable tools, compute jobs, CCL evaluators, and future container or microVM workloads under one envelope. Today the canon has separate models:

- `ComputeTask` (ADR-0030, defined in `icn/crates/icn-compute/src/types.rs`) defines the compute-side workload manifest.
- `ToolManifest` and `ToolBinding` (RFC-0017) define the tool-install-side manifest and binding.
- `EvaluatorBinding` (`docs/spec/ccl-policy-registry.md`, named but not yet typed) defines the CCL-evaluator-side binding.
- `SERVICE_HOSTING_MODEL.md` names the hosted → governed → native stages a service moves through.

What is still missing — what `#1815` exists to define — is the **abstract object model** behind those special cases: the binding that any governed workload uses, the manifest shape every workload declares against, and the runtime provider interface every substrate-side executor implements.

This spec defines the abstract model. It explicitly **does not** redefine the existing concrete types; it names them as specialized cases of the abstract model.

## What this spec is not

- Not runtime implementation. No code lands here.
- Not a schema migration. No Rust types or wire formats are introduced.
- Not a redefinition of `ComputeTask` (ADR-0030), `ToolManifest` / `ToolBinding` (RFC-0017), or `Executor` (`icn-compute/src/executor.rs`). Where this spec touches those areas, the existing ADR or code is authoritative.
- Not authority to mutate DNS, K3s, Forgejo, identity-bridge, gateway, or any deployed infrastructure.
- Not a production-rollout claim. No partner federation is operating under these primitives today.
- Not a redefinition of the meaning firewall (per `docs/architecture/KERNEL_APP_SEPARATION.md`). The kernel does not import `GovernedServiceBinding`, `WorkloadManifest`, or `RuntimeProvider`.
- Not authority to expand the set of compute, tool, or evaluator runtimes beyond what their respective specs sanction.
- Not a backup, replication, recovery, or archive policy (those live in `#1816`); this spec names that a binding declares its custody class and exit policy but does not define their wire-stable form.
- Not closure of `#1815`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's scope.

## The three integrating primitives

The new abstract model has three load-bearing pieces. None of them is implemented yet. Each is described at design granularity; the forthcoming schema work (drafted as a follow-up in this PR's handoff) will turn them into wire-stable records.

### `WorkloadManifest`

The declared shape of what a workload is supposed to do, before any specific institution binds it. A manifest is content-addressed and version-pinned: the manifest a binding refers to never changes silently after the binding lands.

**Identity**

- **Manifest identifier.** A stable identifier (DID-style or content-addressed parent) for the manifest as a logical document.
- **Stable version id.** Each version of the manifest is content-addressed (reusing the existing `ContentHash` convention from `icn-ccl/src/registry.rs`) and version-tagged (`SemanticVersion` from the same module). A binding pins exactly one version.

**Inputs and outputs**

- **Inputs.** The data shapes the workload accepts. May reference schemas (per ADR-0022) or be typed in the manifest's own grammar.
- **Outputs.** The data shapes the workload produces. Outputs are advisory unless the binding's authority context binds them as authoritative (see "Authority context" below and ADR-0030 §"Compute outputs feed governance; they do not replace governance").
- **Expected receipts.** Which receipt classes the workload is expected to emit, per `docs/spec/effect-dispatch-contract.md` §"Receipt class summary."

**Runtime class**

- **Runtime class.** One of the seven classes named in §"Runtime classes" below. The class is closed; new classes require an ADR amendment.

**Constraints**

- **Custody class expectation.** The storage classes the workload will read or write, per `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Storage as custody" and `docs/state/storage-governance-spec.md`. Custody class is binding for what the runtime provider permits.
- **Privacy class.** The manifest declares a privacy class. ADR-0030's text names three classes (`Public`, `Encrypted`, `Sealed`) at the design level. The implemented `PrivacyClass` enum in `icn/crates/icn-compute/src/types.rs` (line 422 area) carries different variants (`Public`, `Member`, `NeedToKnow`) reflecting member-disclosure semantics from later work. The abstract `WorkloadManifest` references the same privacy concept; reconciliation of the variant names between ADR-0030's text and the implementation is **forward work** (a candidate `spec(privacy): reconcile PrivacyClass naming` follow-up). **Downstream manifests SHOULD use the implementation's current variant names** (`Public` / `Member` / `NeedToKnow`) for round-trip compatibility until the reconciliation lands. Workloads that handle restricted material declare a class above `Public`.
- **Determinism class.** Inherited from ADR-0030. Workloads whose outputs feed governance must be deterministic; advisory workloads (utility computation) may be non-deterministic but cannot produce authoritative transitions.
- **Resource profile.** CPU, memory, storage, network envelope. Inherited from ADR-0030 for compute workloads; generalized to other runtime classes.
- **Fuel limit.** Reuses `FuelLimit` from `icn-compute/src/types.rs`. Where the runtime class has a meaningful fuel notion (CCL evaluation, WASM, deterministic compute), the manifest declares it.

**Side-effect bounds**

- **Capability requirements.** The capabilities the workload requires the runtime provider to grant. Capabilities are bounded; per RFC-0017's `ToolBinding` pattern, bindings narrow the manifest's requested set to what the domain authorizes.
- **Disclosure policy.** Which inputs and outputs are public-by-policy, scope-restricted, or vault-stored. Cross-link `#1792` (private data disclosure boundary) and `#1767` (encrypted private-overlay storage).

`ComputeTask` (ADR-0030) is the **compute-specific projection** of `WorkloadManifest`. The fields ADR-0030 already defines (code reference, fuel limit, privacy class, determinism class, scope, resource profile, optional mandate reference) populate the abstract slots above. Nothing in this spec contradicts ADR-0030; the abstract model lifts the same fields to a name not tied to compute.

`ToolManifest` (RFC-0017) is the **tool-install projection** of `WorkloadManifest`. RFC-0017's tool-specific fields (capabilities declared, data touched, storage needs, privacy classes, UI surfaces, compute jobs, schemas, receipts emitted) populate the abstract slots above. Nothing in this spec contradicts RFC-0017; the abstract model lifts the same fields to a name not tied to tools.

### `GovernedServiceBinding`

The institutional record that binds a `WorkloadManifest` to a specific `InstitutionalDomain`. A binding is the artifact governance produces when it adopts a workload.

**Identity and references**

- **Binding identifier.** A stable identifier for this binding inside the adopting domain.
- **Domain reference.** The `InstitutionalDomain` that adopted this binding (per `docs/spec/institutional-domain.md`).
- **Manifest reference.** The `(manifest_id, version_id)` pair the binding pins. Version is fixed at adoption time; upgrade requires a new governance act.

**Authority context**

- **Mandate reference.** The `Mandate` (per ADR-0014, ADR-0019) under which the binding operates. A binding without a covering mandate is not allowed to run (see §"Boundary rules" below).
- **`GovernanceDecisionReceipt` reference.** The receipt anchoring the adoption decision (per `docs/spec/effect-dispatch-contract.md` §"Stage 1: Decision recording").
- **Authority classes granted.** Which `AuthorityClass` instances (per ADR-0014: `Representation` / `Execution` / `Attestation`) the binding may exercise on behalf of the domain.

**Operational policy**

- **Runtime class.** Pinned at the binding; must match what the manifest declared. The provider selected to run this binding is the substrate-side counterpart for this class.
- **Custody class.** The storage classes the binding's workload may touch in this domain. Tightens or matches the manifest's declared custody class; never widens.
- **Dispatch policy.** What effects the binding is permitted to dispatch into the effect dispatch chain (per `effect-dispatch-contract.md`). Bindings without dispatch policy are read-only or evidence-only.
- **Capability scope.** The capabilities the domain has granted to this binding. Narrows the manifest's requested set; never expands it.
- **Resource allocation.** The resource envelope the domain has allocated. Bounded by `DomainPolicy` compute-placement defaults (cross-link `#1801`).

**Durability and exit**

- **Backup policy reference.** Pointer to the `BackupPolicy` the binding inherits (forward-direction, per `#1816`).
- **Exit policy.** What happens when the binding is removed: export rules, custody handoff, receipt of termination.
- **Operator scope.** Who is operationally responsible for running the workload (per `docs/architecture/SERVICE_HOSTING_MODEL.md`).

**Lifecycle state**

- **Current lifecycle state.** One of the named states in §"Lifecycle" below.
- **Adoption history.** The proposal(s) that established and amended this binding.

`ToolBinding` (RFC-0017) is the **tool-install projection** of `GovernedServiceBinding`. RFC-0017's per-institution configuration fields populate the abstract slots above. The tool-install lifecycle transitions (Submitted / Reviewing / Approved / Bound / Running / Suspended / Upgraded / Removed) populate the abstract lifecycle states below.

### `RuntimeProvider`

The substrate-side counterpart that knows how to run a manifest under a binding's constraints. Each runtime class (next section) has one or more provider implementations.

**Provider interface (design level)**

- **Capability discovery.** A provider names the runtime class(es) it implements, the resource budgets it accepts, the capabilities it can grant, and the receipt classes it can emit.
- **Workload instantiation.** Given a `(GovernedServiceBinding, WorkloadManifest)` pair, the provider produces a runnable instance.
- **Constraint enforcement.** The provider enforces the binding's bounds — fuel, privacy class, determinism class, resource profile, custody class, capability scope, disclosure policy. The kernel sees only `KernelEffect`s the provider emits (per `effect-dispatch-contract.md` §"Stage 4"), not the provider's internal logic.
- **Lifecycle hooks.** Allocate, run, observe, upgrade, suspend, remove, export — see §"Lifecycle" below.
- **Receipt emission.** The provider emits receipts at the transitions the binding's `Expected receipts` list names. Steady-state running does not emit a receipt per tick (per RFC-0017 §"Lifecycle receipt-emit mapping"); transitions do.

`Executor` (`icn-compute/src/executor.rs`) is the **compute-specific** provider trait already implemented in code. `RuntimeProvider` is its generalization to other classes (utility computation, container, microVM, accelerator, local device, external bridge). The compute provider continues to implement `Executor`; new provider classes implement an analogous trait at the same level.

## Runtime classes

Seven classes, closed taxonomy. New classes require an ADR amendment. Each class has its own determinism and authority expectations.

| Runtime class | Determinism | Authority expectation | Provider example |
|---|---|---|---|
| **Deterministic legitimacy compute** | Mandatory. | Outputs may feed authoritative transitions when covered by a mandate. | CCL evaluator (per `ccl-policy-registry.md`), schema-bridge `charter_to_constraints` execution (per ADR-0022). |
| **Utility computation** | Optional. | Outputs are advisory; cannot produce authoritative transitions. | Plugins, transforms, summarizers, model assists. |
| **Container workloads** | Not assumed. | Operational utility; authority granted only by explicit binding policy. | Hosted services (forge, status page, identity-bridge, registry). |
| **Stronger-isolation (microVM) workloads** | Not assumed. | As container; isolation harder. | Arbitrary user code, untrusted code paths. |
| **Hardware-accelerated workloads** | Not assumed. | Resource bounded; authority granted by explicit allocation. | Model inference, deliberation tooling. |
| **Local device / client workloads** | Not assumed. | Operates on the member's own device; offline-first member shell, signing surfaces. | Mobile member shell components, kiosk operations. |
| **External bridge workloads** | Not assumed. | Authority granted only by explicit policy; never default. | Integration with external systems by treaty or custodian arrangement. |

The cross-cutting rule from the spine doc applies:

> **Utility services can propose or produce evidence. Legitimacy compute verifies and records authoritative transitions.**

A workload that draws conclusions is not authoritative. A workload that records conclusions under a covering mandate is. The runtime class is a constraint on which side of that line the workload may sit.

## Lifecycle

Every governed binding moves through ten named states. Each transition requires authority basis and emits a receipt where the binding's `Expected receipts` list names one.

| State | What happens | Authority required | Receipt expectation |
|---|---|---|---|
| **Declare** | A proposal declares the binding's intent: which manifest, which runtime class, which capability scope, which custody class. | Per the domain's `DomainPolicy` for proposal sponsorship. | Proposal-created record. |
| **Authorize** | The proposal reaches an accepted terminal state in the governance state machine. | Per the domain's threshold rules. | `GovernanceDecisionReceipt`. |
| **Allocate** | The runtime provider allocates resources under the binding's resource envelope. | Per the domain's compute-placement policy (cross-link `#1801`). | Allocation record. |
| **Bind** | The binding is persisted; the provider is ready. Resource handles are claimed; mandate is associated. | Implicit from `Authorize` + `Allocate`. | Binding-created effect record (per ADR-0025 forward-direction). |
| **Run** | Steady state. The workload executes on demand or continuously per its runtime class. | The binding's `Capability scope`. | None at steady state. Transitions inside the workload may emit receipts per the workload's manifest. |
| **Observe** | The provider reports status. Health and progress are observable in the steward cockpit (`#1795`). | Read-only. | Observability records (optional; not authoritative). |
| **Upgrade** | A new manifest version is adopted. The binding rebinds at the new version; lifecycle returns to `Bind`. | A fresh governance act with its own `GovernanceDecisionReceipt`. The old binding's adoption history records the supersession. | Upgrade effect record. |
| **Suspend** | The binding's capabilities are revoked but the binding is not removed. The workload stops; resources may or may not be freed depending on policy. | Per the domain's emergency-authority rules. | Suspension record. |
| **Remove** | The binding is removed. The provider deallocates resources. Adoption history retains the binding's lifecycle for audit. | A governance act. | Removal effect record + termination receipt. |
| **Export** | When the domain exits, the binding's manifest and any portable state are exported per the binding's `Exit policy`. Custody handoff is recorded. | The domain's exit policy. | Export receipt. |

**Lifecycle invariants:**

- No transition without authority basis.
- No transition without receipt expectation (where the manifest lists one).
- Reversal is a separate transition. Removal of a binding is not retroactive; the binding's lifecycle stays in adoption history.
- Upgrade is a governance act, not a quiet replacement. Manifest version is pinned at bind time.
- A binding without a covering mandate may not enter `Bind` from `Authorize`. (Where the seam falls through to `Mandate::new_pending_grants` per ADR-0019, the binding stays in `Authorize` until the mandate composes a covering grant set.)

## Service maturity stages

The spine doc names a five-stage progression a service moves through: `Unmodeled → Hosted → Governed → Adapted → ICN-native`. These stages are **not** a separate lifecycle alongside the binding-state lifecycle above; they are **constraints on what the binding has bound**, applied to the same lifecycle states.

| Stage | What the binding must have to be in this stage |
|---|---|
| **Unmodeled** | No binding exists; the service runs without a `GovernedServiceBinding`. ICN does not know about it. |
| **Hosted** | The service runs on infrastructure ICN or an ICN-aligned operator manages. The binding may exist as a record of operational responsibility but does not yet bind authority. |
| **Governed** | The binding includes authority context (mandate reference, authority classes granted) and custody class. Changes to the binding are governance acts. |
| **Adapted** | The binding emits ICN receipts for institutionally relevant transitions per its `Expected receipts` list. Member-facing surfaces use `ServiceIdentity` (forward-direction, partially specified by RFC-0017 and `SERVICE_HOSTING_MODEL.md`), not operator identities. |
| **ICN-native** | The binding is governed end-to-end: declared by manifest, bound by `GovernedServiceBinding`, executed by a named `RuntimeProvider`, backed by an explicit custody and exit policy, observable in the steward cockpit, exportable on exit. |

A reachable service is not a native service. Reachability is an operational fact; nativeness is the institutional fact that every transition in the binding's lifecycle has authority and a receipt.

Stage transitions are not automatic. A service moves from Hosted to Governed only when the binding's authority context is populated. A service moves from Adapted to Native only when the full lifecycle commitments hold (receipts, observability, export). Each stage transition is itself a governance act.

## Relationship to existing surfaces

This spec generalizes — it does not replace.

| Existing surface | Relationship to this spec |
|---|---|
| `ComputeTask` (ADR-0030, defined in `icn/crates/icn-compute/src/types.rs`; task lifecycle in `task.rs`) | Specialized projection of `WorkloadManifest` for the **Deterministic legitimacy compute** and **Utility computation** classes. ADR-0030's fields populate `WorkloadManifest`'s abstract slots. The compute layer continues to operate on `ComputeTask`; the abstract `WorkloadManifest` is the integrating envelope for the rest of the system. |
| `ToolManifest` / `ToolBinding` (RFC-0017) | Specialized projection of `WorkloadManifest` and `GovernedServiceBinding` for the tool-install case. RFC-0017's per-institution configuration is the same `GovernedServiceBinding` shape, with tool-specific fields populated. |
| `Executor` trait (`icn-compute/src/executor.rs`) | Compute-specific `RuntimeProvider` implementation. Other runtime classes implement analogous traits at the same level. |
| `EvaluatorBinding` (`docs/spec/ccl-policy-registry.md`) | Specialized projection of `GovernedServiceBinding` for CCL evaluators. CCL evaluators are workloads in the **Deterministic legitimacy compute** class; their binding is a `GovernedServiceBinding` whose `Runtime class` is fixed and whose `Authority context` derives from the CCL adoption decision. |
| `ServiceIdentity` (named in `institutional-domain.md`, `SERVICE_HOSTING_MODEL.md`, RFC-0017) | Identity surface used by service workloads. A `GovernedServiceBinding` that has reached the **Adapted** stage uses a `ServiceIdentity` for member-facing actions. Issuance and rotation are forward work (cross-link RFC-0017 and the deferred follow-up in `institutional-domain.md`). |
| `Mandate` / `AuthorityGrant` / `TypedScope` (ADR-0014) | Authority primitives the binding's authority context references. The binding does not redefine them. |
| `EffectManifest` / `KernelEffect` / `EffectOutcome` (the dispatch chain) | Workloads that produce authoritative transitions do so by emitting `KernelEffect`s through the dispatch chain (per `effect-dispatch-contract.md` §"Stage 4"). The runtime provider is the producer; the kernel is the consumer. |

## Boundary rules

| Rule | What it means |
|---|---|
| **Utility produces evidence; legitimacy compute records authoritative transitions.** | A workload's runtime class fixes which side of this line it is on. Utility workloads may inform decisions; only legitimacy compute under a covering mandate may produce institutional effects. |
| **A workload without a binding is not allowed to run.** | The binding is the institutional record of who authorized this workload to act in this domain. Without it, the workload has no authority context and no provider. |
| **A binding without a custody class and exit policy is incomplete.** | A binding that does not declare what storage it touches and what happens when it leaves cannot be safely adopted. The domain MUST surface the gap before accepting the adoption proposal. |
| **A binding without a covering mandate may not exercise execution authority.** | The seam's conservative-broadening invariants (ADR-0019) apply: where the mandate composes via `new_pending_grants`, the binding stays in `Authorize`; it does not enter `Bind` until grants are present. |
| **Manifest version is pinned at adoption.** | Upgrade is a governance act. Silent swap to a newer manifest version is forbidden. (Symmetric with `ccl-policy-registry.md` §"Evaluator selection contract" step 8: registry-level supersession is informational; only governance adoption changes the version a binding uses.) |
| **The kernel does not consult `GovernedServiceBinding`.** | Per `KERNEL_APP_SEPARATION.md`: the kernel sees only `KernelEffect`s. Bindings live at the governance app layer; runtime providers convert the binding's policy into `KernelEffect`s the kernel can enforce blindly. |
| **Package vocabulary stays in packages.** | ICN core defines the abstract types and the closed runtime-class taxonomy. Institution packages bring their own role identifiers, ceremony markers, and proposal kinds; these translate to the generic shapes at the package boundary before any binding lands. Generic scope language in this spec uses `Domain` / `InstitutionalDomain` / `LocalDomain`; it does not use `Coop` as a generic scope name. See [`../architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) §C3 "Entity-scope vocabulary." |
| **A binding never widens what the manifest declared.** | Capability scope, custody class, resource allocation, disclosure policy, operator scope, dispatch policy, and exit/backup requirements — all are tightened by the binding, never expanded. The manifest is the upper bound; the binding is the actual grant. **Runtime class is exact-match-only** between manifest and binding (see also "Runtime class mismatch" in §"Failure and safety rules"); moving from one runtime class to another requires a new manifest version or a governed upgrade with its own receipts. The binding does not narrow runtime class. |

## Failure and safety rules

Each failure has a deterministic response. Cross-cutting rule: **fail closed, surface honestly, never silently fall back.**

| Failure | Response |
|---|---|
| **Manifest version drift** (registry has a newer version than the binding pinned). | Informational; binding continues with the pinned version. Upgrade is a governance act. (Symmetric with `ccl-policy-registry.md` step 8.) |
| **Capability scope exceeds manifest's declared requirements** (binding tries to grant more than manifest declared it would need). | Adoption MUST fail. The domain cannot grant capability the manifest did not request. |
| **Runtime class mismatch** (binding declares one class; manifest declares another). | Adoption MUST fail. Class is a binding-time pin. |
| **Mandate missing or revoked while binding is in `Run`.** | Binding moves to `Suspend`. The workload stops dispatching authoritative effects. Resumption requires a fresh mandate (a governance act). |
| **Custody class violation** (workload tries to access storage outside its declared custody class). | The runtime provider refuses. The attempt is recorded as evidence; repeated violations escalate per the domain's emergency-authority rules. |
| **Provider unavailable for the binding's runtime class.** | Binding stays in `Allocate` or `Bind` failure. Operator must restore the provider or governance must amend the binding (e.g., to a runtime class with an available provider). |
| **Nondeterministic output from a Deterministic-legitimacy-compute workload.** | Bug. The workload is quarantined per ADR-0030's determinism rule; the manifest version MUST be marked deprecated until the bug is fixed. |
| **Workload tries to emit an effect outside its dispatch policy.** | The runtime provider refuses. The attempt is recorded in dispatch evidence (per `effect-dispatch-contract.md` §"Stage 5") with `success = false`. |
| **Workload tries to read or write data outside its disclosure policy.** | The runtime provider refuses; vault access produces no receipt for the workload, and the violation is recorded. (Cross-link `#1792` and `#1767`.) |
| **Binding remains in `Authorize` indefinitely without a covering mandate.** | The steward cockpit surfaces the gap. After the domain's policy-defined timeout, the binding may be revoked (a governance act) or amended. |
| **Operator removes a workload's resources without going through `Remove`.** | The binding moves to `Suspend` (resources gone, binding intact). The institutional record stays correct; the operational record is honest about the gap. |

## Relationship to sibling work

Cross-link, do not duplicate.

| Concern | Where it lives |
|---|---|
| `InstitutionalDomain` and `DomainPolicy` primitive | `docs/spec/institutional-domain.md` (`#1794`). |
| Accepted-proposal effect dispatch contract | `docs/spec/effect-dispatch-contract.md` (`#1797`, merged). |
| CCL policy registry, versioning, evaluator-selection contract | `docs/spec/ccl-policy-registry.md` (`#1817`, merged). |
| Integrated cooperative operating model | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` (`#1793`, merged). |
| Backup, replication, recovery, archive policies | `#1816`. |
| Member shell v0 UX | `#1818`. |
| ArtifactRegistry / ScopedVault boundary | `#1798`. |
| Compute placement policy | `#1801`. |
| Network anti-entropy proof loops | `#1799`. |
| Steward cockpit v0 | `#1795`. |
| Institutional process substrate | `#1748`. |
| Compute workload manifest (compute-specific projection) | ADR-0030. |
| Commons compute admission and settlement policy | ADR-0031. |
| Tool install infrastructure | RFC-0017. |
| Service hosting model (stage progression) | `docs/architecture/SERVICE_HOSTING_MODEL.md`. |
| Cooperative tool commons doctrine | `docs/architecture/COOPERATIVE_TOOL_COMMONS.md`. |
| Constitutional object model | ADR-0014. |
| Authority Grant Minting and Mandate Persistence Seam | ADR-0019. |
| Receipt and provenance proof envelope | ADR-0026. |
| Institutional Effect Record (proposed) | ADR-0025. |

## Open questions and deferred decisions

Each is named so future readers know where this spec stops.

| Question | Deferred to |
|---|---|
| Wire-stable schema for `GovernedServiceBinding`, `WorkloadManifest`, `RuntimeProvider` records | Follow-up `schema(runtime): …` issue drafted in this PR's handoff. |
| Generic `RuntimeProvider` Rust trait (the cross-class generalization of `Executor`) | Follow-up `spec(runtime): …` issue drafted in this PR's handoff. |
| Per-class provider specifications (container, microVM, accelerator, local device, external bridge) | Forward-direction; one or more `spec(runtime-<class>): …` follow-ups after this spec lands. |
| `ServiceIdentity` issuance, rotation, recovery lifecycle inside domains | Cross-link the deferred follow-up from `institutional-domain.md` and RFC-0017. |
| Hosted → Governed → Adapted → Native stage acceptance gates (the signoff process for promoting a binding) | Forward-direction; candidate follow-up `spec(governance): define service-binding stage promotion contract`. |
| `BackupPolicy` and exit-policy data model that bindings reference | `#1816`. |
| `EffectRecord` closed-taxonomy implementation (the binding's adoption / upgrade / removal effect records) | ADR-0025 (proposed). |
| Federation-side recognition of one domain's `GovernedServiceBinding`s | ADR-0019 "Non-decisions"; future ADR. |
| Kernel-side enforcement of `Mandate`s the binding's authority context references | ADR-0019 "Non-decisions"; future ADR. |
| Member-shell rendering of bindings the member can act on | `#1818`. |
| Steward-cockpit surface for binding lifecycle and runtime-provider health | `#1795`. |

## Non-claims (repeated for clarity)

- This document does not introduce any of the forward-direction primitives it names. They remain forward-direction.
- It does not introduce schema, wire format, or contract changes.
- It does not redefine `ComputeTask` (ADR-0030), `ToolManifest` / `ToolBinding` (RFC-0017), or the `Executor` trait.
- It does not authorize the kernel to consult `GovernedServiceBinding` directly.
- It does not authorize mutation of any deployed infrastructure (DNS, K3s, Forgejo, identity-bridge, gateway).
- It does not claim production readiness for any subsystem.
- It does not claim live inter-cooperative federation.
- It does not claim any partner federation is operating under these primitives today. NYCN appears nowhere in the spec; institution-package vocabulary stays in institution packages.
- It does not use payment, wallet, balance, currency, or blockchain as ICN-native framing. The discipline list in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full.
- It does not by itself close `#1815`. The PR uses `Refs:`.

## Related documents and issues

Architecture and spine:

- `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` — §"Governed workloads and service envelopes" (the section this spec details).
- `docs/architecture/SERVICE_HOSTING_MODEL.md` — hosted → governed → native stage progression.
- `docs/architecture/COOPERATIVE_TOOL_COMMONS.md` — anti-capture doctrine; tools do not own institutional truth.
- `docs/architecture/KERNEL_APP_SEPARATION.md` — meaning firewall; kernel never imports bindings or manifests.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — generic shapes vs. package vocabulary.
- `docs/architecture/DOMAIN_ROUTING_AND_DNS_BINDINGS.md` — DNS routes people; bindings hold authority.

Specs (sibling and parent):

- `docs/spec/institutional-domain.md` — `InstitutionalDomain` and `DomainPolicy`.
- `docs/spec/effect-dispatch-contract.md` — the dispatch chain runtime providers feed.
- `docs/spec/ccl-policy-registry.md` — CCL evaluators are workloads under this spec; evaluator bindings are specialized `GovernedServiceBinding`s.
- `docs/spec/KERNEL_CONTRACTS.md` — kernel primitive contracts.

ADRs:

- ADR-0014 — Constitutional Object Model.
- ADR-0019 — Authority Grant Minting and Mandate Persistence Seam.
- ADR-0021 — CCL Determinism, Fuel, Capability Safety (the deterministic-compute envelope).
- ADR-0022 — CCL Schema Bridge and Constraint Compilation.
- ADR-0025 — Institutional Effect Record Canonical Schema (proposed).
- ADR-0026 — Receipt and Provenance Proof Envelope.
- ADR-0027 — Action Card Contract.
- ADR-0030 — Compute Workload Manifest and Authority Boundary (`ComputeTask`).
- ADR-0031 — Commons Compute Admission and Settlement Policy.

RFCs:

- RFC-0017 — Tool Install Infrastructure (`ToolManifest`, `ToolBinding`, install lifecycle).

Code surface (existing types this spec references):

- `icn/crates/icn-compute/src/types.rs` — `ComputeTask` (the compute-specific projection of `WorkloadManifest`), `FuelLimit`, `TaskHash`, `ExecutorCapability`, `TaskPriority`, `ExecutionOutcome`, `ComputeResult`, `PrivacyClass`.
- `icn/crates/icn-compute/src/task.rs` — task lifecycle and storage built on top of the `ComputeTask` type from `types.rs`.
- `icn/crates/icn-compute/src/executor.rs` — `Executor` trait (the compute-specific projection of `RuntimeProvider`).
- `icn/crates/icn-compute/src/commons_pool.rs` — `CommonsPool` (admission per ADR-0031).
- `icn/crates/icn-compute/src/receipt.rs` — `ComputeReceipt` (settlement per ADR-0031).
- `icn/crates/icn-governance/src/mandate.rs` — `Mandate`.
- `icn/crates/icn-governance/src/authority.rs` — `AuthorityClass`, `AuthorityGrant`, `TypedScope`.
- `icn/crates/icn-governance/src/effect_manifest.rs` — `EffectManifest`.
- `icn/crates/icn-kernel-api/src/effects.rs` — `KernelEffect` (the kernel-side consumer).

Issues:

- `#1815` — this spec.
- `#1793` — integrated operating model (merged via PR #1814).
- `#1794` — institutional domain (merged via PR #1820).
- `#1797` — effect dispatch contract (merged via PR #1819).
- `#1817` — CCL policy registry (merged via PR #1821).
- `#1816` — backup / replication / recovery / archive policies.
- `#1818` — member shell v0.
- `#1798` — ArtifactRegistry / ScopedVault.
- `#1801` — compute placement.
- `#1799` — network anti-entropy proof loops.
- `#1795` — steward cockpit.
- `#1748` — institutional process substrate.
