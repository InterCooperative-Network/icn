---
Status: normative
Authority: spec (defines the forward-direction primitives the merged effect-dispatch contract and institutional-domain spec defer to #1817; some clauses are explicitly forward-direction and named as such)
Canonical: no
Last Reviewed: 2026-05-14
---

# CCL Policy Registry and Hook Contract

> **Status: spec, work-in-progress.** Defines the CCL policy registry, policy-version model, adoption contract, evaluator-selection contract, evaluator-output → effect-plan contract, review/audit surfaces, and failure/safety rules. The spec extends the canonical CCL safety properties (ADR-0021), the schema-bridge doctrine (ADR-0022), and the CCL institutional process language framing (ADR-0023) without redefining them. It fills the gap that the merged effect dispatch contract (`#1797`, PR #1819) and institutional-domain spec (`#1794`, PR #1820) both explicitly defer to `#1817`. Clauses that depend on schema fields or kernel behavior that have not yet landed are marked as forward-direction. The PR introducing this doc advances `#1817` without closing it.

## Purpose

CCL — the Cooperative Contract Language — is the **executable institutional rule layer inside governance**. ADR-0021 froze its safety properties (deterministic, fuel-metered, not Turing-complete, capability-bounded, pure expression and rule). ADR-0022 froze the schema-bridge doctrine that turns CCL output into `ConstraintSet` for the kernel. ADR-0023 named "institutional process language" as the surface CCL inhabits. The merged dispatch and domain specs say where CCL evaluators participate in the chain.

What is still missing — what `#1817` exists to define — is the **bridge** between three already-defined surfaces:

- the `DomainPolicy` carrying "adopted CCL policy references" (from `docs/spec/institutional-domain.md` §"DomainPolicy");
- the `CclDocument` and `SchemaVersion` types that already exist in the `icn-ccl` crate;
- the Stage 2 (mandate composition) and Stage 3 (effect plan generation) hook points named in `docs/spec/effect-dispatch-contract.md` §"CCL hook points."

The bridge has three load-bearing pieces:

1. A **policy registry** that addresses and discovers CCL policies and their versions.
2. An **adoption contract** that turns a draft policy version into one that a specific domain binds via governance.
3. An **evaluator-selection contract** that resolves a given proposal-kind under a given domain to the right CCL evaluator, deterministically, with bounded fuel and bounded capabilities.

This spec defines all three at design granularity. It does **not** invent new wire formats, change CCL grammar, redefine the safety properties, or specify the registry's persistent representation in code. Those are forward-direction work tracked as follow-ups in this PR's handoff.

## What this spec is not

- Not a CCL grammar change. The grammar is ADR-0021's and the language design at the time of writing.
- Not a runtime implementation. No code lands here.
- Not a schema migration. No Rust types or wire formats are introduced.
- Not authority to mutate DNS, K3s, Forgejo, identity-bridge, or any deployed infrastructure.
- Not a model/agent-tooling integration. Models or agents may draft CCL text; this spec does not specify the drafting tools.
- Not a package-specific adoption workflow. A federation's adoption ceremony lives in its institution package.
- Not a redefinition of ADR-0021's five safety properties or ADR-0022's schema-bridge doctrine. Both remain canonical for what they decide.
- Not a redefinition of the meaning firewall (per `docs/architecture/KERNEL_APP_SEPARATION.md`). The kernel runs no CCL.
- Not closure of `#1817`. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's scope and rules.

## Boundary rules

These distinctions are load-bearing. Conflating them is exactly the failure mode the spec exists to prevent.

| Concept | Distinct from in this way |
|---|---|
| **CCL document vs adopted CCL policy version** | A `CclDocument` is text. A *policy version* is the specific, content-addressed snapshot of that document a domain has adopted. The same `CclDocument` may exist in many versions; only versions get adopted. |
| **Draft policy vs adopted policy** | A draft is registered, reviewable, and inert. Adoption is a governance act that binds a draft (now a versioned policy) to a domain. Without the governance act, no rule the draft expresses has authority. |
| **Policy registry vs `DomainPolicy`** | The registry is the **catalog of CCL policies** in the system: drafts, adopted versions, supersession history, evaluator bindings. `DomainPolicy` is **the set of policy references a specific domain has currently adopted**. The registry is the universe; `DomainPolicy` is what one domain has bound. |
| **CCL evaluator vs runtime dispatcher** | A CCL evaluator runs a policy expression deterministically under ADR-0021 properties and returns structured output. The runtime dispatcher turns governance decisions into `KernelEffect`s under kernel-enforced constraints. The evaluator informs the dispatcher; the dispatcher invokes the kernel. |
| **CCL output vs authoritative mutation** | Per ADR-0022: CCL output is `ConstraintSet` plus structured outputs the host consumes. CCL never mutates state directly. Mutation happens via the effect dispatch chain (`docs/spec/effect-dispatch-contract.md`) and its emitted receipts. |
| **Model- or agent-drafted text vs governance-adopted policy** | A model may produce CCL text. Governance adopts it. The text becomes authoritative only after the adoption decision lands and the registry records the binding. Model output is never authoritative by virtue of being persuasive. |
| **Institution-package vocabulary vs ICN core grammar** | A package may name proposal kinds, role identifiers, and ceremony markers in its own vocabulary. ICN core CCL grammar carries the generic shapes (expression, rule, capability, constraint). Package nouns translate to generic kernel-side outputs before they leave the package boundary. |

In short:

> A CCL document is not authoritative because it exists.
> A CCL version becomes authoritative only when a domain adopts it through governance.
> `DomainPolicy` binds adopted policy versions to a governed domain.
> A CCL evaluator may produce a structured effect plan.
> The runtime dispatches bounded effects.
> The kernel does not evaluate CCL.
> Models and agents may draft policy text; governance adopts it.

## Policy registry (object outline)

The registry is the system-wide catalog of CCL policies. It is **not** a per-domain object. Domains adopt entries from the registry; the registry itself is shared.

Fields are described at design granularity. No Rust type, JSON schema, or serialization format is fixed here. The forthcoming schema work (drafted as a follow-up in this PR's handoff) will turn this into a wire-stable record.

**Identity**

- **Policy identifier.** A stable identifier for the policy as a logical document, separate from any single version. Multiple versions share an identifier; a version moves the content.
- **Stable policy address.** A canonical pointer (DID-style or content-addressed parent) that survives version churn. References from `DomainPolicy` point at the address plus a version selector.

**Descriptive metadata**

- **Title / summary / purpose.** Human-readable identifiers. Authoritative; surface in the member shell.
- **Policy class.** What kind of rule the policy expresses (e.g., proposal-class evaluator, eligibility predicate, threshold formula, settlement rule, dispute-resolution mediator). Closed taxonomy; growable by ADR amendment. The class is what evaluator-selection keys on.
- **Authorship / proposer provenance.** Who drafted, who reviewed, who proposed adoption. The registry records the trail even for never-adopted drafts.

**Lifecycle status**

- **Status.** One of `draft` / `proposed` / `adopted` / `superseded` / `deprecated`. Status is a per-version label; the policy as a whole may have multiple versions in different statuses.
- **Adoption history.** For every adopted version: which domain, which adoption proposal, what authority basis, what receipt.
- **Amendment path.** Pointer to the next version a domain should consider; supersession lineage.

**Versioning**

- **Version list.** All known versions of this policy, in canonical order.
- **Per-version metadata.** See "Policy version (outline)" below.

**Review and audit**

- **Review status.** Whether the policy has been reviewed for the safety properties, schema compatibility, and (where applicable) accessibility and translation requirements.
- **Compatibility notes.** Which schema versions, evaluator versions, and `DomainPolicy` shapes this policy is compatible with.

**Bindings**

- **Adopting domains.** The set of `InstitutionalDomain`s that have currently adopted at least one version of this policy.
- **Linked proposal kinds.** Which proposal kinds (in registered domains) this policy currently evaluates.
- **Evaluator binding references.** The pointer(s) to the evaluator entry the runtime selects when this policy is consulted (see §"Evaluator selection contract").

**Output expectations**

- **Receipt expectations.** What receipt class(es) the dispatch chain produces after an evaluator under this policy contributes to a decision (per `effect-dispatch-contract.md` §"Receipt class summary").
- **Audit / export visibility.** Whether the policy and its versions are publicly visible, partner-visible, or private-by-policy. Private-by-policy entries surface only to authorized roles, but the **fact** that a policy version exists is never invisible.

## Policy version (outline)

A policy version is the specific, content-addressed snapshot a domain adopts.

- **Stable version id.** A deterministic identifier for this version, combining `ContentHash` (the existing blake3 32-byte type in `icn-ccl/src/registry.rs`) and `SemanticVersion` (already defined in the same module). Together they uniquely identify the version across history.
- **Content reference.** The reference to the canonical content. Content addressing is the existing convention from the contract registry; this spec reuses it.
- **Authorship and review provenance.** Who wrote this version, who reviewed it, which review issues it resolves, what tests it carries.
- **Adoption proposal reference.** The proposal whose acceptance bound this version to a domain (per `effect-dispatch-contract.md` Stage 1, `GovernanceDecisionReceipt`). A version that has not yet been adopted has no adoption proposal reference yet; that is a normal state.
- **Effective date / activation condition.** When the version takes effect after adoption (immediately, on next proposal of the relevant kind, at a fixed block, …). Activation is a design parameter, not a schema field at this stage.
- **Supersession / deprecation relation.** Pointer to the version that supersedes this one, if any; pointer to the deprecation rationale, if any.
- **Compatibility with `DomainPolicy`.** Whether the version is shape-compatible with the current `DomainPolicy` of the adopting domain.
- **Rollback / reversal / challenge relation.** Per `effect-dispatch-contract.md` §"Challenge / reversal / counter-receipt": a reversal of the adoption is a new governance act that supersedes the original.
- **Receipt / provenance references.** Pointers to the receipts that mark this version's introduction, adoption, amendment, and (where applicable) reversal.

## Adoption contract

How a domain adopts a CCL policy version, in the language of the existing dispatch chain.

1. **Proposal created.** A member or steward authorized by `DomainPolicy` creates a proposal of class "adopt CCL policy version" naming a specific `(policy_identifier, version_id)` pair from the registry. The proposal carries rationale and review evidence.
2. **Review path.** The proposal enters the domain's process substrate. The policy's `Review status` informs the review requirement; the domain's `DomainPolicy` informs the review window and threshold.
3. **Vote / decision.** The proposal moves through the governance state machine to an accepted or rejected terminal state, per `docs/architecture/GOVERNANCE_STATE_MACHINE.md`.
4. **`GovernanceDecisionReceipt`.** On acceptance, the governance app records a `GovernanceDecisionReceipt` anchoring the `decision_hash` per `effect-dispatch-contract.md` §"Stage 1: Decision recording." Authenticity is carried by `GovernanceProof` and (where applicable) `GovernanceDecisionAttestation`; the receipt itself anchors the decision.
5. **`DomainPolicy` update / binding.** A subsequent effect within the dispatched manifest updates `DomainPolicy.adopted_ccl_policy_refs` to include the new version. The registry's per-policy `Adopting domains` set is updated to include the domain, and the version's `Adoption history` gains an entry.
6. **Adoption receipt.** The dispatched effect emits an `InstitutionalEffectRecord` and `EffectDispatchEvidence` per `effect-dispatch-contract.md` §"Stage 5: Application + evidence." The forward-direction `EffectRecord` taxonomy (ADR-0025) names this as a `PolicyAdopted` outcome; the closed-taxonomy implementation is deferred.
7. **Member / operator visibility.** The member shell (`#1818`) surfaces the new adopted version with a plain-language summary; the steward cockpit (`#1795`) surfaces the binding for auditability.
8. **Challenge / reversal path.** A challenge proposal references the adoption's `decision_hash` and may propose full reversal (the version is unbound), partial supersession (a successor version replaces it), or amendment (a new version supersedes). Per `effect-dispatch-contract.md` §"Challenge / reversal / counter-receipt", reversal is a separate decision; the original adoption record stays, with a reversal-pointer.

This contract does not invent steps. Every step is already specified in `effect-dispatch-contract.md` or `institutional-domain.md`. The contract names which dispatch-chain stage each adoption step corresponds to.

## Evaluator selection contract

How the runtime resolves a proposal kind in a domain to the right CCL evaluator. Selection happens deterministically; selection failures fail closed.

**The selection key**

The selection input is the triple `(domain_id, proposal_kind, current DomainPolicy snapshot)`. The selection output is a single `EvaluatorBinding` (forward-direction primitive named in this spec; not yet a type in code).

**The selection procedure**

1. The governance app reads the current `DomainPolicy` for the domain.
2. It enumerates the adopted CCL policy versions whose `Linked proposal kinds` include this proposal kind.
3. It resolves the registry entry for each version and follows its `Evaluator binding references` to find candidate evaluators.
4. **If exactly one binding resolves and the policy version is current:** the evaluator is selected. Selection emits a `(decision_hash, policy_version_id, evaluator_id)` provenance triple that travels with the evaluation.
5. **If zero bindings resolve:** selection fails. The proposal cannot be evaluated under CCL; the governance app routes it to manual review per the domain's emergency-authority rules. No silent fallback to a "default" evaluator is permitted.
6. **If multiple bindings resolve:** selection fails. The runtime emits an ambiguity record and routes the proposal to a domain-policy conflict resolution path. Multiple-binding states are themselves bugs in the domain's adoption set and SHOULD be surfaced in the steward cockpit.
7. **If the policy version is deprecated:** selection fails (or, where the domain's `DomainPolicy` permits a grace period, selection emits a deprecation warning and the dispatch chain marks the resulting effect record accordingly). No silent execution of a deprecated policy.
8. **If the version the domain has adopted is superseded at the registry level by a successor the domain has not adopted:** selection still uses the adopted version. Registry-level supersession is informational; it is not a governance act. The successor becomes authoritative for the domain **only after the domain adopts it through the adoption contract**, producing its own `GovernanceDecisionReceipt`. Until that adoption lands, evaluation continues against the version actually adopted. The steward cockpit surfaces the pending amendment opportunity so members and stewards see that a successor exists, but no silent switchover occurs. (If the adopted version has been *deprecated* at the registry level, step 7's fail-closed rule applies; deprecation of an adopted version is institutionally consequential because the registry is asserting the version should no longer be relied on.)

**Determinism**

Per ADR-0021: same `(domain_id, proposal_kind, DomainPolicy version, policy version, evaluator id, inputs)` → same evaluator output. Non-deterministic evaluation is a bug, not a tunable.

**The kernel does not consult the registry.** Per `KERNEL_APP_SEPARATION.md` and `effect-dispatch-contract.md` Stage 4: the kernel sees only `KernelEffect`. Selection happens in the governance app, before the manifest is dispatched.

## Evaluator output → effect plan

What an evaluator returns, and how that return shape feeds the dispatch chain's Stage 2 and Stage 3 hook points.

**Output shape (design level)**

An evaluator under this contract returns a **structured envelope** containing:

- **Decision suggestion.** One of `accepted` / `rejected` / `needs-review`. The suggestion is **advisory** until the governance state machine reaches its own terminal state; the evaluator does not make decisions, it informs them.
- **Reasons / rule references.** Which specific clauses of the adopted policy version were applied to reach the suggestion. Reasons are part of the audit trail.
- **Structured effect plan candidate.** For proposals that, if accepted, would dispatch effects: a candidate plan in the shape `effect-dispatch-contract.md` Stage 3 consumes (an `EffectManifest`-shaped value or the structured inputs from which a manifest is derived). The plan is itself bound by ADR-0022's schema-bridge — values it includes are bridge-compatible.
- **Disclosure policy.** Which fields of the output are public-by-policy, scope-restricted, or vault-stored (per `institutional-domain.md` §"DomainPolicy" → privacy/disclosure defaults).
- **Receipt expectations.** Which receipt class(es) the dispatched effects should emit. Per `effect-dispatch-contract.md` §"Receipt class summary."
- **Challengeability metadata.** Which clauses of the suggestion are challengeable, and through which challenge proposal class.
- **Required authority basis.** Which authority class(es) under ADR-0014 the dispatched effects require. The output names the requirement; it does not mint the authority.
- **Unresolved questions / review gates.** Where the evaluator could not reach a clean suggestion (ambiguous policy, missing input, contested signal), the output names the gate and the human review path.

**The Stage 2 hook**

Per `effect-dispatch-contract.md` §"Stage 2 hook — Mandate composition," a CCL evaluator may inform whether `grant_minting.rs` mints `AuthorityGrant`s or falls through to `Mandate::new_pending_grants`. The evaluator's output is consumed by the governance-app seam; the seam's conservative-broadening invariants (ADR-0019) still apply. A CCL evaluator that proposes a grant the payload does not name MUST be ignored. **CCL informs; it does not fabricate.**

**The Stage 3 hook**

For proposal kinds CCL governs, the evaluator's structured effect plan flows into `EffectManifest` translation (per `effect-dispatch-contract.md` §"Stage 3"). Per ADR-0022, the schema bridge then compiles the manifest's expressions and references into the `ConstraintSet` the kernel will enforce.

**Stages where CCL does not run**

CCL runs in neither Stage 4 nor Stage 5. The kernel never imports CCL. Application + evidence is the dispatcher's and the subsystem's contract; no CCL evaluator is invoked there.

## Review and audit surfaces

The registry is observable. Without that, "adopted means authoritative" becomes unverifiable.

- **The registry surfaces drafts, adopted versions, superseded versions, and amendment history.** Drafts are visible (so review can happen); adopted versions are visible (so members can see what governs them); superseded versions stay visible (so audit is possible); amendment history is visible (so reasoning is reconstructable).
- **Members see what rules govern them in plain language.** The policy's `Title / summary / purpose` and each version's plain-language rendering are surfaced in the member shell (`#1818`). Where the policy expression is opaque, the registry MUST carry a human-readable equivalent.
- **Operators and stewards see evaluator bindings and selection failures.** The steward cockpit (`#1795`) surfaces ambiguity, deprecation, missing-version, and other selection-failure states named in §"Evaluator selection contract."
- **Receipts show which policy version was applied.** Per Stage 5, the `InstitutionalEffectRecord` and `EffectDispatchEvidence` reference the policy version id that the evaluator ran. The forward-direction `EffectRecord` (ADR-0025) carries this in its provenance fields.
- **Exported records include enough policy and version provenance to audit later.** Export receipts (per the forthcoming backup spec, `#1816`) include the `(policy_id, version_id, evaluator_id, decision_hash)` triple.
- **Accessibility and translation are part of legitimacy, not polish.** Per `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`: a policy that members cannot read in their language, with their access needs, is not legitimate.

## Failure and safety rules

Each failure mode below has a named, deterministic response. The rule across all of them: **fail closed, surface honestly, never silently fall back.**

| Failure | Response |
|---|---|
| **Unadopted policy.** | Inert. No evaluator runs; no rule applies. References to unadopted policy versions from `DomainPolicy` are rejected at the dispatch-chain entry point. |
| **Missing policy version.** | Selection fails. Governance app routes to manual review per the domain's emergency-authority rules. No "latest" or "default" fallback. |
| **Conflicting adopted policies.** | Ambiguity-of-binding state per §"Evaluator selection contract." Steward cockpit surfaces; governance must resolve via a conflict-resolution proposal. |
| **Unknown proposal kind.** | Selection fails. The proposal cannot be evaluated under CCL; manual review per emergency-authority rules. |
| **Evaluator unavailable** (binding present but runtime cannot load). | Selection fails. The failure is recorded with cause; operator must restore the evaluator or governance must amend the binding. |
| **Nondeterministic evaluator output.** | Bug. The evaluator MUST be quarantined and the policy version MUST be marked deprecated until the bug is fixed. ADR-0021's determinism property is non-negotiable. |
| **Model-generated policy text without adoption.** | Inert. The text is a draft in the registry at best. No evaluator runs from unadopted text; no member-facing surface treats it as binding. |
| **Package-specific nouns leaking into ICN core CCL grammar.** | Per `INSTITUTION_PACKAGE_BOUNDARY.md`: rejected. Generic shapes only in core; packages translate their nouns at the boundary. |
| **Private data in evaluator inputs or outputs.** | The evaluator's disclosure policy (see §"Evaluator output → effect plan") binds the visibility of inputs and outputs. Vault-stored material is fetched and returned through the same scoped-vault pathway named in `institutional-domain.md` §"DomainPolicy" → privacy/disclosure defaults. |
| **Stale or deprecated policy references in `DomainPolicy`.** | The domain MUST amend its `DomainPolicy` via a governance act before the referenced policy can be evaluated. References that point at deprecated versions cause selection to fail closed. |

## Relationship to sibling work

Cross-link, do not duplicate.

| Concern | Where it lives |
|---|---|
| `InstitutionalDomain` and `DomainPolicy` primitive | `docs/spec/institutional-domain.md` (`#1794`, merged via PR #1820). |
| Accepted-proposal effect dispatch contract (Stage 1–5, CCL hook points) | `docs/spec/effect-dispatch-contract.md` (`#1797`, merged via PR #1819). |
| Integrated cooperative operating model (civic loop, vocabulary discipline, Cooperative OS framing) | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` (`#1793`, merged via PR #1814). |
| Governed service binding / workload manifest / runtime provider | `#1815`. |
| Backup / replication / recovery / archive policies | `#1816`. |
| Member shell v0 UX | `#1818`. |
| ArtifactRegistry / ScopedVault boundary | `#1798`. |
| Compute placement policy | `#1801`. |
| Network anti-entropy proof loops | `#1799`. |
| Steward cockpit v0 | `#1795`. |
| Institutional process substrate | `#1748`. |
| CCL determinism, fuel, capability safety | ADR-0021. |
| CCL schema bridge and constraint compilation | ADR-0022. |
| CCL institutional process language | ADR-0023. |
| Institution package manifest schema | ADR-0024. |
| Constitutional object model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`) | ADR-0014. |
| Receipt and provenance proof envelope | ADR-0026. |
| Institutional Effect Record canonical schema (proposed) | ADR-0025. |

## Open questions and deferred decisions

Each is named so future readers know where this spec stops.

| Question | Deferred to |
|---|---|
| Wire-stable schema for `CCLPolicyRegistry`, `PolicyVersion`, `EvaluatorBinding` records | Follow-up `schema(ccl): …` issue drafted in this PR's handoff. |
| Deterministic execution envelope for evaluators (fuel budget per evaluator class, capability set per evaluator class, lifecycle of an in-process evaluator) | Follow-up `spec(ccl): define evaluator binding and deterministic execution envelope` drafted in this PR's handoff. |
| Adoption / amendment proposal lifecycle as a closed taxonomy (proposal-class shape, threshold defaults, default review window) | Follow-up `spec(governance): define policy adoption/amendment proposal lifecycle` drafted in this PR's handoff. |
| Member-shell rendering of adopted policies and their plain-language summaries | `#1818`. |
| Steward-cockpit surface for evaluator failures and binding ambiguity | `#1795`. |
| Export / audit-record format that carries `(policy_id, version_id, evaluator_id)` provenance | `#1816` (and the forward-direction `ProvenanceQuery` Layer 4 surface from ADR-0026 §4, tracked under `#1438`). |
| Federation-side recognition of one domain's adopted policy versions | ADR-0019 "Non-decisions"; future ADR. |
| Kernel-side enforcement of `Mandate`s composed under CCL evaluator output | ADR-0019 "Non-decisions"; future ADR. |
| Policy schema versioning migration (e.g., `governance.schema_v2` → `v3`) | Future ADR amendment of ADR-0022. |

## Non-claims (repeated for clarity)

- This document does not change CCL grammar.
- It does not introduce schema, wire format, or contract changes.
- It does not authorize the kernel to consult the registry.
- It does not authorize mutation of any deployed infrastructure (DNS, K3s, Forgejo, identity-bridge, gateway).
- It does not claim production readiness for any subsystem.
- It does not claim live inter-cooperative federation.
- It does not claim any partner federation is operating under these primitives today. NYCN appears nowhere in the spec; institution-package vocabulary stays in institution packages.
- It does not specify a model- or agent-tooling integration. Models may draft CCL text; governance adopts it.
- It does not use payment, wallet, balance, currency, or blockchain as ICN-native framing. The discipline list in `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` §"Vocabulary discipline" applies in full.
- It does not by itself close `#1817`. The PR uses `Refs:`.

## Related documents and issues

Architecture and spine:

- `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` — §"CCL — the executable institutional rule layer."
- `docs/architecture/KERNEL_APP_SEPARATION.md` — meaning firewall; the kernel runs no CCL.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` — generic shapes vs. package vocabulary.
- `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` — design-direction overview of cooperative domain infrastructure.

Specs:

- `docs/spec/institutional-domain.md` — `InstitutionalDomain` and `DomainPolicy` primitive (the consumer of adopted policy refs).
- `docs/spec/effect-dispatch-contract.md` — the dispatch chain into which evaluator output flows.
- `docs/spec/KERNEL_CONTRACTS.md` — kernel primitive contracts.

ADRs:

- ADR-0021 — CCL Determinism, Fuel, and Capability Safety.
- ADR-0022 — CCL Schema Bridge and Constraint Compilation.
- ADR-0023 — CCL Institutional Process Language.
- ADR-0024 — Institution Package Manifest Schema.
- ADR-0014 — Constitutional Object Model.
- ADR-0019 — Authority Grant Minting and Mandate Persistence Seam.
- ADR-0025 — Institutional Effect Record Canonical Schema (proposed).
- ADR-0026 — Receipt and Provenance Proof Envelope.
- ADR-0027 — Action Card Contract.
- ADR-0030 — Compute Workload Manifest and Authority Boundary.

Code surface (existing types this spec references):

- `icn/crates/icn-ccl/src/registry.rs` — `ContentHash` (blake3, 32 bytes), `SemanticVersion`, contract `Registry`.
- `icn/crates/icn-ccl/src/schema/mod.rs` — `CclDocument`, schema subsystems (governance / economics / entity / agreement / expr).
- `icn/crates/icn-ccl/src/schema/version.rs` — `SchemaVersion`.
- `icn/crates/icn-ccl/src/schema/bridge.rs` — `charter_to_constraints(...)` (ADR-0022 entry point).
- `icn/crates/icn-ccl/src/types.rs` — `Capability` enum, `ContractInstallation`.
- `icn/crates/icn-governance/src/proof.rs` — `GovernanceProof`, `GovernanceDecisionReceipt`, `GovernanceDecisionAttestation`.
- `icn/crates/icn-governance/src/mandate.rs` — `Mandate` (ADR-0014 / ADR-0019).
- `icn/crates/icn-governance/src/authority.rs` — `AuthorityClass`, `AuthorityGrant`, `TypedScope`.
- `icn/crates/icn-governance/src/effect_manifest.rs` — `EffectManifest`.
- `icn/crates/icn-kernel-api/src/effects.rs` — `KernelEffect` aggregate.

Issues:

- `#1817` — this spec.
- `#1794` — `InstitutionalDomain` and `DomainPolicy` (merged via PR #1820).
- `#1793` — integrated operating model (merged via PR #1814).
- `#1797` — effect dispatch contract (merged via PR #1819).
- `#1815` — service binding / workload manifest / runtime provider.
- `#1816` — backup / replication / recovery / archive policies.
- `#1818` — member shell v0.
- `#1798` — ArtifactRegistry / ScopedVault.
- `#1801` — compute placement.
- `#1799` — network anti-entropy proof loops.
- `#1795` — steward cockpit.
- `#1748` — institutional process substrate.
- `#1438` — `ProvenanceQuery` Layer 4 surface.
