---
Status: normative
Authority: spec (harmonizes existing types and ADRs into a single end-to-end behavior contract; some clauses are explicitly forward-direction and named as such)
Canonical: no
Last Reviewed: 2026-05-14
---

# Effect Dispatch Contract

> **Status: spec, work-in-progress.** This document names the end-to-end behavior contract for turning an accepted governance decision into bounded operational effects with receipts. It harmonizes existing types and ADRs (ADR-0014, ADR-0019, ADR-0025, ADR-0026, ADR-0027, ADR-0030, ADR-0031) into one chain. Clauses that depend on schema fields or kernel behavior that have not yet landed are split into a "current contract" half and a "future schema work" half, marked explicitly per stage. The PR introducing this doc advances #1797 without closing it.

## Purpose

Governance acceptance is necessary but not sufficient. Without a written contract for what happens after acceptance, every dispatch path drifts in isolation — and acceptance becomes documented consent without operational consequence.

This spec answers a single question:

> Once a proposal is accepted, what is the contract for turning the decision into bounded effects, with provable receipts, while preserving the meaning firewall?

The constraint:

> Governance authorizes. CCL expresses adopted rules. The runtime executes bounded effects. Receipts prove transitions. The kernel never understands meaning.

This document does not invent new primitives. The repository already holds most of the pieces — `EffectManifest`, `Mandate`, `KernelEffect`, `InstitutionalEffectRecord`, `EffectDispatchEvidence`, `GovernanceDecisionReceipt`. The job here is to name the contract those pieces collectively implement, surface the seams that are not yet written down, and identify the first safe runtime slice.

## What this spec is not

- Not implementation. No code or schema lands here.
- Not authority to expand the supported proposal-class set beyond what `apps/governance/src/grant_minting.rs` already mints (ADR-0019's conservative-broadening invariants stand unchanged).
- Not a redefinition of any existing ADR. Where this spec describes ADR-0014/0019/0025/0026/0030/0031 behavior, the ADRs remain canonical for their respective decisions.
- Not authorization to gate kernel dispatch on mandates. Per ADR-0019, that is a separate future decision.
- Not a federation mandate-recognition contract. Per ADR-0019, also separate.
- Not a CCL grammar specification. CCL hook points are named here; CCL syntax and adoption live in the (forthcoming) CCL policy registry spec (#1817).
- Not closure of #1797. The PR introducing this doc uses `Refs:`; closure is left for separate review against the issue's six acceptance criteria.

## The dispatch chain

Every legitimate effect of an accepted proposal moves through five named stages. Each stage produces a durable artifact. Skipping a stage breaks the chain.

```text
Stage 1: Decision recording          → GovernanceDecisionReceipt   (ADR-0026 Layer 1)
Stage 2: Mandate minting             → Mandate + zero-or-more AuthorityGrants  (ADR-0014, ADR-0019)
Stage 3: Effect plan                 → EffectManifest              (deterministic; manifest_hash)
Stage 4: Effect dispatch             → KernelEffect per target subsystem
Stage 5: Application + evidence      → EffectOutcome + receipt_ref → InstitutionalEffectRecord + EffectDispatchEvidence
```

The chain is sequential. Stage N cannot begin without the artifact from Stage N-1.

### The chain inside the civic loop

The chain is the substrate-side half of one segment of the civic loop named in [`../architecture/ICN_INTEGRATED_SYSTEM_MODEL.md`](../architecture/ICN_INTEGRATED_SYSTEM_MODEL.md). Specifically: it covers `authorized action → CCL/runtime evaluation → storage/compute/governance/economic transition → receipt`. The stages before (identity, standing, authority, action card) and after (sync, federation, member shell, challenge/repair) sit outside this spec but are cross-linked.

## Canonical objects (already implemented)

The contract uses the following types, all of which exist in the repository today. This spec does not redefine them.

| Object | Crate / File | Purpose | Provenance |
|---|---|---|---|
| `GovernanceDecisionReceipt` | `icn-governance/src/proof.rs` | Signed, persisted proof a decision was reached under canonical process. Keyed by `proposal_id`; carries `decision_hash`. | ADR-0026 Layer 1 |
| `Mandate` | `icn-governance/src/mandate.rs` | Bounded institutional authorization issued per decision (or per charter clause). First-class institutional-memory record. | ADR-0014 |
| `AuthorityGrant` / `AuthorityClass` / `TypedScope` | `icn-governance/src/authority.rs` | The grant the mandate composes (or `new_pending_grants` if the seam cannot truthfully derive one). | ADR-0014, ADR-0019 |
| `EffectManifest` | `icn-governance/src/effect_manifest.rs` | Normalized, deterministic, hashable description of governance mutations. Versioned. Carries capability/economic/membership/protocol effects. | (intrinsic; referenced by ADR-0014 Implementation Update) |
| `KernelEffect` | `icn-kernel-api/src/effects.rs` | Aggregate kernel-safe effect enum (Treasury / Membership / Protocol / Control / Federation / Dispute / Resource / SDIS / NoOp). Primitive-only. | (intrinsic; meaning-firewall contract) |
| `EffectOutcome` | `icn-kernel-api/src/governance.rs` (re-exported) | Result class returned by an executor: `Applied`, `NoOp`, `Partial`, `Failed`. | (intrinsic) |
| `InstitutionalEffectRecord` | `apps/governance/src/institutional_effect.rs` | Persisted artifact of the translated effect at acceptance time. Keyed by `record_id`; indexed by `proposal_id`; carries `decision_hash`. | (intrinsic; ADR-0026 Layer 2 placement) |
| `EffectDispatchEvidence` | `apps/governance/src/dispatch_evidence.rs` | Append-only log of downstream subsystem reporting back synchronously, with `EffectOutcome` and opaque `receipt_ref`. | (intrinsic) |
| `EffectRecord` (proposed) | per ADR-0025 (no crate yet) | Closed-taxonomy institutional outcome record (`RoleAssigned`, `AllocationMade`, `EffectReversed`, …). Spec-stage. | ADR-0025 (proposed) |

The contract glues these objects into one chain.

## Stage 1: Decision recording

**Input:** an accepted proposal at the terminal state of the governance state machine (per [`../architecture/GOVERNANCE_STATE_MACHINE.md`](../architecture/GOVERNANCE_STATE_MACHINE.md)).

**Action:** the governance app records the decision as a `GovernanceDecisionReceipt` (`icn-governance/src/proof.rs`). The receipt anchors the canonical `decision_hash`; it is the institutional-memory record that the decision occurred. The receipt itself is not the signature carrier — signatures live on the companion `GovernanceProof` artifact, and per-attester signatures live on `GovernanceDecisionAttestation`. The receipt and its proof / attestation evidence are persisted in the gateway receipt store together.

**Output:** a content-addressed `decision_hash`. This hash is the load-bearing identifier for the rest of the chain.

**Invariants:**

- Exactly one `GovernanceDecisionReceipt` per accepted proposal.
- Authenticity is established by the companion `GovernanceProof` (and, where applicable, `GovernanceDecisionAttestation`). A receipt without a verifiable proof is institutional memory of a decision the system cannot vouch for; the chain MUST surface that state honestly rather than silently treat the receipt as authoritative.
- The receipt is immutable. Subsequent challenge/reversal does not edit the receipt; it produces a separate decision and a separate counter-effect (see Stage 5 and "Challenge / reversal / counter-receipt" below).
- `decision_hash` is the canonical key the rest of the chain references. No stage may reference a proposal without its decision hash.

## Stage 2: Mandate minting

**Input:** the accepted proposal's payload and the `GovernanceDecisionReceipt`.

**Action:** the seam in `apps/governance/src/grant_minting.rs` (ADR-0019) produces:

1. Zero or more `AuthorityGrant`s — minted only when the payload truthfully names grantee, class, and a time bound.
2. Exactly one `Mandate` — composing those grants if any, or falling through to `Mandate::new_pending_grants` when the seam cannot derive a coherent grant set.

**Output:** a persisted `Mandate`, keyed to the decision.

**Invariants (ADR-0019, restated here for the chain's contract):**

- **No fabricated grants.** If the payload does not directly name grantee + class + time bound, the mint returns the empty set for that proposal.
- **`new_pending_grants` is the truthful fall-through.** The mandate exists; the grants are explicitly pending. This is more honest than a "synthetic grant for everything" record.
- **The grantor is the sovereign entity.** The platform, runtime, gateway, and shared services are never grantors.
- **The supported proposal-class set is small on purpose.** Today: steward-appointment and steward-reconfirmation only. Expansion is by ADR amendment; silent broadening is forbidden.

**Non-enforcement (ADR-0019):** the kernel dispatcher does not consult mandates as a precondition for `KernelEffect` execution. Mandates are institutional memory at this layer. Future enforcement is a separate ADR.

## Stage 3: Effect plan (`EffectManifest`)

**Input:** the `Mandate`, the accepted payload, and the baseline state snapshot.

**Action:** the governance app translates the payload into a deterministic `EffectManifest`. Translation is pure: same payload + same baseline → same manifest, byte-equal.

**Output:** an `EffectManifest` containing zero or more typed effects (capability, economic, membership, protocol) plus `change_hash`, `baseline_snapshot_hash`, `ratification_block`, `activation_block`, and `manifest_hash`.

**Invariants:**

- Determinism is mandatory. The manifest must be replayable: re-running translation on the same payload and baseline must produce the same `manifest_hash`.
- The `EffectManifest` is the dispatch unit. Stage 4 dispatches every effect in the manifest, in order, against the named targets.
- The manifest is content-addressed. `manifest_hash` together with `decision_hash` and `effect_index` is the idempotency key (see "Idempotency" below).
- A manifest with zero effects is valid. Text proposals, attestation-only acceptances, and process-only transitions all produce empty or `NoOp`-only manifests, and that is the correct shape.
- The manifest is the seam where domain semantics end. Capability, economic, membership, protocol effects are primitive-only by ADR-0014. Domain meaning lives in the translation, not in the manifest.

## Stage 4: Effect dispatch

**Input:** the `EffectManifest`.

**Action:** the governance app converts each manifest effect into a `KernelEffect` and dispatches it to the target subsystem (treasury, membership, protocol, control, federation, dispute, resource, or SDIS executor). The kernel executes the effect under the constraints it already enforces; it does not consult mandates or domain types.

**Output:** for each dispatched effect, an `EffectOutcome` (`Applied`, `NoOp`, `Partial`, `Failed`) plus an opaque `receipt_ref` from the downstream subsystem.

**Invariants:**

- **Kernel sees only `KernelEffect`.** No domain crate is imported by kernel crates. No pattern-matching on custom keys. This is the meaning firewall.
- **Dispatch is per-effect, not per-manifest.** Each `KernelEffect` returns its own outcome. The manifest succeeds as a unit only if every dispatched effect returns `Applied` or `NoOp`.
- **Dispatch is synchronous from the governance app's perspective.** "Evidenced" means the downstream subsystem reported back synchronously; it does not mean the action is externally observable or that downstream side effects are durable beyond what the subsystem itself guarantees.
- **Executors do not interpret governance meaning.** A treasury executor sees an allocation; it does not see "this allocation came from a steward-reconfirmation proposal."

## Stage 5: Application + evidence

**Input:** the dispatched effect and the subsystem's `EffectOutcome` + `receipt_ref`.

**Action:** the governance app persists:

1. One `InstitutionalEffectRecord` per accepted proposal that translates to a structured effect (keyed by `record_id`, indexed by `proposal_id`, carrying `decision_hash`).
2. One `EffectDispatchEvidence` entry per dispatched effect (keyed by `evidence_id`, attached to the `effect_record_id`, carrying `subsystem`, `receipt_ref`, `success`, `EffectOutcome`).
3. (Forward direction, per ADR-0025) one `EffectRecord` per institutional outcome inside the closed-taxonomy schema (`RoleAssigned`, `AllocationMade`, `EffectReversed`, etc.) — pending the implementation tranche named by that ADR.

**Output:** durable institutional memory keyed back to `decision_hash`. The chain is complete.

**Invariants:**

- The `decision → mandate → manifest → effect → receipt` chain is auditable post-restart. Any consumer holding a `decision_hash` can walk forward to every effect and back to the originating decision.
- Reversal does not delete. Reversal is a counter-effect (`EffectReversed`, ADR-0025) that emits a fresh record pointing at the original.
- `Partial` outcomes are first-class. They are not silently retried.
- Records without dispatch evidence are valid states. ADR-0019's `Mandate::new_pending_grants` path, and dispatch paths whose downstream hook returns `()` today, produce records that are `emitted_only` (per `EffectDispatchEvidence` module doc). The chain admits this state; it does not pretend otherwise.

## Dry-run / preview

A proposal in deliberation may render a preview manifest. Preview is what authors and reviewers see before they accept.

**Contract:**

- Preview is a pure derivation from `(draft_payload, baseline_snapshot)`. It produces a candidate `EffectManifest` and a candidate effect list rendered in plain language.
- Preview must not mint mandates, must not dispatch, must not emit institutional receipts. Specifically: no `GovernanceDecisionReceipt`, no `Mandate`, no `EffectDispatchEvidence`, no `InstitutionalEffectRecord` is written from a preview.
- Preview output is advisory. If the preview is shown to members, it must carry a label indicating it is a preview and that no transition has occurred.
- Preview rendering belongs on member-facing surfaces (the member shell, #1818) and operator surfaces (the steward cockpit, #1795). Both must be able to show the same preview from the same input.
- Preview must be content-equivalent to the live manifest the system would produce if the proposal were accepted at the same baseline. If preview and live diverge for the same input, that is a bug.

A non-goal: preview is not authorization to dispatch. A reviewer who likes the preview still has to accept the proposal through the governance state machine.

## Idempotency

The doctrine: a re-dispatch of the same effect must not produce a second mutation. How strictly that can be enforced today depends on what the current schema can store. This section is split into the current contract (using fields that exist) and forward schema work (using fields that do not yet exist).

### Current contract

Today's persisted records carry enough institutional memory to detect duplicate dispatch at the institution-level granularity. The contract is:

- The governance app SHOULD short-circuit before dispatching to the subsystem when an `InstitutionalEffectRecord` for the same `(proposal_id, effect_kind)` already exists with `decision_hash` populated. This blocks the obvious replay-on-restart class of duplicate dispatch using `InstitutionalEffectRecord`'s existing `proposal_id` index and `decision_hash` field.
- Where the subsystem itself is naturally idempotent (steward registration by content-addressed `StewardId`, charter deploy by `decision_hash` keying, etc.), the governance-app short-circuit is a defense-in-depth check, not the sole guard.
- Where the subsystem is not naturally idempotent, the chain SHOULD prefer to dispatch once and record `Partial` or `Failed` on retry rather than risk a second mutation. `EffectDispatchEvidence`'s existing `success` field plus `EffectOutcome` is the surface that surfaces this state.

This contract is observably enforceable with current schema. It is **not** a kernel-side gate; per ADR-0019, kernel dispatch does not consult mandates or institutional effect records.

### Future schema work (deferred)

The stronger idempotency contract — keying every dispatch by a stable tuple and recording per-effect idempotency intent in the manifest — requires schema fields that do not yet exist:

- A stable idempotency key of shape `(decision_hash, manifest_hash, effect_index)`. `decision_hash` exists; `manifest_hash` exists on `EffectManifest`; `effect_index` and a per-effect persisted record keyed by that tuple do not.
- A retry-policy field on `EffectManifest` (or on per-effect entries within it) naming whether retry is allowed and under what conditions. No such field exists today.
- A `not-idempotent` flag or equivalent on `EffectDispatchEvidence` (or the subsystem's own record) so consumers can detect subsystems whose internal contract is weaker than the dispatcher's expectation. The current `EffectDispatchEvidence` carries `subsystem`, `receipt_ref`, `success`, and `EffectOutcome`; no idempotency-intent field exists.

These fields are net-new schema. This PR does not introduce schema changes, and #1797 is a spec PR not an implementation PR. The stronger contract is intentionally left as forward-direction work, to be filed as one or more follow-up implementation issues only after this spec is accepted (per #1797's sixth acceptance criterion).

When the schema lands, the contract becomes: subsystems MUST be idempotent on `(decision_hash, manifest_hash, effect_index)`; re-dispatch MUST return the prior outcome; the governance app MUST short-circuit on this key before any subsystem call. The behavioral doctrine does not change — only its enforceability widens.

### What the contract observes today

Regardless of schema state: the contract observes idempotency at the institutional-memory boundary. Records and evidence already in storage make duplicate dispatch detectable in the obvious cases. Where they do not, the chain MUST surface the gap honestly through evidence rather than paper over it.

## Partial failure semantics

The four-valued `EffectOutcome` makes partial failure first-class:

| Outcome | Meaning | What the chain does |
|---|---|---|
| `Applied` | The subsystem mutated state successfully. | Persist evidence; chain proceeds. |
| `NoOp` | The dispatch was legal but produced no state change (e.g., a revoke on an already-revoked record). | Persist evidence; chain proceeds. |
| `Partial` | Real mutation occurred; a later step inside the subsystem failed. | Persist evidence with `success=false`. The mandate is marked partially executed. The chain does NOT auto-retry. The institution must reach a decision about closure. |
| `Failed` | No mutation occurred; the subsystem rejected the effect. | Persist evidence with `success=false`. The manifest may be abandoned, retried under a fresh manifest, or escalated as a challenge. |

**Contract:**

- A `Partial` outcome is institutional information. It tells members and stewards that the system is in a state the institution did not fully authorize. The member shell and steward cockpit must surface it.
- A `Partial` outcome must not be silently retried. The same idempotency key would either succeed (no-op-ing the partial mutation) or compound the partial state. Either way, the next decision must be human.
- Reversal of a `Partial` outcome is a separate decision. It produces a fresh `GovernanceDecisionReceipt`, a fresh `Mandate`, and a reversal manifest. See "Challenge / reversal / counter-receipt."

## Challenge / reversal / counter-receipt

An accepted decision can be challenged. The challenge is itself a governance act.

**Contract:**

- A challenge proposal references the original `decision_hash` and proposes one of: full reversal, partial reversal of named effects, or a corrective superseding decision.
- If the challenge proposal is itself accepted, it produces:
  1. A new `GovernanceDecisionReceipt` (the challenge's own decision).
  2. A new `Mandate` (the reversal mandate).
  3. A new `EffectManifest` whose effects are counter-effects keyed by the original `decision_hash` and `effect_record_id`.
  4. New `InstitutionalEffectRecord` and `EffectDispatchEvidence` entries for the counter-effects.
  5. (Forward direction, per ADR-0025) a fresh `EffectRecord` of kind `EffectReversed` pointing at the original record.
- Counter-effects do not delete original records. They emit reversal records. The chain is append-only; audit logs do not lie about history.
- A `Partial` outcome on the original may produce a counter-effect that completes the failed step, reverses the successful step, or both. The reversal manifest names each counter-effect explicitly.

This is the same chain, applied to itself. The reversal proposal is not privileged. It runs through the governance state machine like any other proposal.

## CCL hook points

CCL (the executable institutional rule layer) participates in the dispatch chain at three named points. CCL never runs inside the kernel.

**Stage 2 hook — Mandate composition:**

- A CCL evaluator may inform whether the seam mints `AuthorityGrant`s or falls through to `Mandate::new_pending_grants`. The evaluator reads the accepted payload and the current `DomainPolicy` and returns a structured grant set (or empty).
- The seam's conservative-broadening invariants (ADR-0019) still apply. A CCL evaluator that proposes a grant the payload does not name MUST be ignored. CCL informs; it does not fabricate.

**Stage 3 hook — Effect plan generation:**

- For proposal kinds CCL governs, the evaluator may produce a structured `EffectManifest` (or the inputs from which the manifest is derived). This is where CCL "produces effect plans, not unilateral mutations" (per [`../architecture/ICN_INTEGRATED_SYSTEM_MODEL.md`](../architecture/ICN_INTEGRATED_SYSTEM_MODEL.md), "CCL — the executable institutional rule layer").
- Determinism is mandatory: same CCL document, same payload, same baseline → same manifest. Non-deterministic CCL output is a bug.

**Stage 4 (no hook):**

- CCL does not run inside the kernel. The kernel enforces constraints; the constraint set was already produced by a policy oracle earlier in the chain (per [`../architecture/KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md)). CCL output is never executed in the kernel.

**Contract:**

- CCL must not bypass governance acceptance. A CCL evaluator output that contradicts the accepted decision (or runs without one) is a bug.
- CCL is governed itself. Adopted versions are bound to a domain by governance; unadopted versions are inert. The CCL policy registry and versioning contract are specified separately under #1817.

## Privacy and redaction

Effects touching scoped vaults or private overlays carry additional contract.

**Contract:**

- Every dispatched effect MUST carry an explicit disclosure policy. The policy names which fields are publicly readable, which are scope-restricted, and which are vault-stored.
- `InstitutionalEffectRecord.payload` stores only the public-by-policy fields plus pointers to vault entries for restricted material.
- Vault access produces its own receipt. Disclosure is a governed event; reading is not free.
- Backups inherit the disclosure policy of the source (see "Backup, redundancy, recovery, archive" in the spine doc, and the forthcoming backup spec under #1816). A backup that crosses a locality boundary violates the same policy the source enforces.

Cross-link: #1792 (private data disclosure boundary), #1767 (encrypted distributed private-overlay storage).

## Action-card trigger rules

Some effect outcomes imply action cards for affected DIDs.

**Contract:**

- Action cards are derived views, not stored entities (ADR-0027). Derivation reads the holder's standing plus `InstitutionalEffectRecord` plus `EffectDispatchEvidence` plus pending governance objects.
- Effect kinds that imply action cards include (provisional, package-specific names live in packages):
  - `RoleAssigned` → an action card for the assigned DID confirming the role and naming next steps.
  - `MembershipAdmitted` → an action card for the new member with orientation steps.
  - `AllocationMade` → an action card for the recipient confirming the allocation and naming any acknowledgement step.
  - `SanctionIssued` / `RepairAgreed` (placeholder per ADR-0029) → action cards for the affected parties surfacing the repair path.
- Action-card derivation rules belong in the gateway; the contract here is that the rules read effect records, never raw kernel state.

Cross-link: #1608, #1646, #1711, #1712, #1818 (member shell).

## Package boundary

ICN core knows generic effect families. Institution packages carry local vocabulary.

**Contract:**

- ICN core's effect families are the closed taxonomy named by `KernelEffect` (Treasury / Membership / Protocol / Control / Federation / Dispute / Resource / SDIS / NoOp) and the proposed `EffectRecord` kinds (ADR-0025).
- Package payloads (e.g. a federation's domain-specific proposal kinds) MUST translate to generic kernel effects at the governance-app boundary BEFORE dispatch.
- Kernel never sees package nouns. Cross-link [`../architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md).
- Where a package needs a new effect family, the right path is to amend the generic taxonomy (ADR), not to leak package nouns into core.

## First safe runtime dogfood slice

The acceptance criterion in #1797 calls out the first safe runtime dogfood slice. The candidate is #1748 — the institutional process substrate's `ProcessTransitionReceipt` flow.

**Why this slice first:**

- The process substrate produces transition receipts that already align with Stage 1 (decision recording) and Stage 5 (application + evidence) of this contract.
- A process-transition acceptance can produce an empty `EffectManifest` (no domain effects) or a `NoOp`-only manifest. The dispatch chain runs end-to-end with no kernel-side mutation required.
- The `Mandate` for a non-grant-minting acceptance falls through to `Mandate::new_pending_grants` (ADR-0019). The chain exercises the truthful fall-through, not the synthetic-grant path.
- The chain exercises preview, idempotency on replay, and evidence persistence without depending on kernel mandate enforcement (which is deferred per ADR-0019).

**What this slice does not require:**

- It does not require kernel-side gating on mandates (deferred, per ADR-0019).
- It does not require expansion of the supported proposal-class set for grant minting (ADR-0019's conservative-broadening invariants stand).
- It does not require ADR-0025's `EffectRecord` implementation to land before the slice runs. The chain works with `InstitutionalEffectRecord` + `EffectDispatchEvidence` as today.
- It does not require new ADRs.

**What this slice teaches:**

- Whether the contract's stage boundaries hold against a real flow.
- Whether dispatch evidence + records together provide enough institutional memory for the member shell and steward cockpit to render legibly.
- Where partial-outcome handling is under-specified once a real subsystem returns `Partial`.

Subsequent slices (steward appointment, allocation, capability grants, etc.) layer in more effect kinds. Each new slice is its own implementation tranche; this spec does not authorize the order.

## Receipt class summary

The chain emits receipts at multiple layers, all of which are already specified by ADR-0026 except where noted.

| Receipt | Where emitted | Provenance |
|---|---|---|
| `GovernanceDecisionReceipt` (Layer 1) | Stage 1 | ADR-0026 §1 |
| `Mandate` (institutional-memory record, not strictly a receipt) | Stage 2 | ADR-0014, ADR-0019 |
| `AuthorityGrant` (audit record) | Stage 2 | ADR-0014, ADR-0019 |
| `ArtifactReceipt` (Layer 2 wrapper binding L1 to institutional context) | Stage 5 | ADR-0026 §2 |
| `InstitutionalEffectRecord` (governance-app artifact) | Stage 5 | (intrinsic; sits at Layer 2 placement) |
| `EffectDispatchEvidence` (append-only dispatch log) | Stage 5 | (intrinsic) |
| `EffectRecord` (closed-taxonomy institutional outcome) | Stage 5 (forward direction) | ADR-0025 (proposed) |
| `FederationProvenance` (Layer 3, cross-coop) | post-Stage 5; out of scope here | ADR-0026 §3 |
| `ProvenanceQuery` (Layer 4 surface) | not yet implemented; tracked under #1438 | ADR-0026 §4 |

This spec introduces no new receipt class. It names which classes each stage emits.

## Open questions and deferred decisions

The following are explicitly out of scope here. Each is named so future readers know where the contract stops.

| Question | Deferred to |
|---|---|
| When (if ever) the kernel dispatcher consults `Mandate`s as a gate. | ADR-0019 "Non-decisions"; future ADR. |
| Kernel `Capability` issuance from `AuthorityGrant`s (per ADR-0014's deferred tranche). | ADR-0014; future ADR. |
| Federation-side mandate recognition. | ADR-0019; future ADR. |
| Authority revocation end-to-end. | Partial in code (`grant_minting.rs:343`); future ADR. |
| `ProvenanceQuery` (Layer 4) surface. | ADR-0026 §4; tracked under #1438. |
| `EffectRecord` Rust type + closed taxonomy implementation. | ADR-0025 (proposed). |
| CCL policy registry, versioning, and evaluator-selection contract. | #1817. |
| Backup, replication, recovery policy objects (incl. restore-test receipts). | #1816. |
| `GovernedServiceBinding` / `WorkloadManifest` / `RuntimeProvider` model. | #1815. |
| Member shell version zero UX specification. | #1818. |

## Non-claims

- Does not claim production readiness for any of the named stages.
- Does not claim kernel-side mandate enforcement; that remains deferred.
- Does not claim federation mandate recognition.
- Does not expand the supported proposal-class set for grant minting beyond ADR-0019.
- Does not introduce new ADR-0025 `EffectRecord` kinds.
- Does not specify CCL grammar.
- Does not define the backup, replication, or recovery policy objects.
- Does not by itself close #1797. The PR introducing this doc uses `Refs:`.

## Related documents and issues

Architecture and spine:

- [`../architecture/ICN_INTEGRATED_SYSTEM_MODEL.md`](../architecture/ICN_INTEGRATED_SYSTEM_MODEL.md) — civic loop and the governance / effect-dispatch section.
- [`../architecture/KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md) — meaning firewall.
- [`../architecture/GOVERNANCE_STATE_MACHINE.md`](../architecture/GOVERNANCE_STATE_MACHINE.md) — proposal lifecycle.
- [`../architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](../architecture/INSTITUTION_PACKAGE_BOUNDARY.md) — generic shapes vs. package vocabulary.

ADRs:

- ADR-0014 — Constitutional Object Model (`AuthorityClass`, `AuthorityGrant`, `TypedScope`, `Mandate`).
- ADR-0019 — Authority Grant Minting and Mandate Persistence Seam.
- ADR-0025 — Institutional Effect Record Canonical Schema (proposed).
- ADR-0026 — Receipt and Provenance Proof Envelope.
- ADR-0027 — Action Card Contract.
- ADR-0029 — Conflict Resolution Object Model.
- ADR-0030 — Compute Workload Manifest and Authority Boundary.
- ADR-0031 — Commons Compute Admission and Settlement Policy.

Issues:

- #1797 — accepted-proposal effect dispatch contract (this spec).
- #1793 — integrated cooperative operating model (parent of this work, merged via PR #1814).
- #1748 — institutional process substrate (first dogfood slice).
- #1817 — CCL policy registry / versioning / governance-effect hook contract.
- #1815 — `GovernedServiceBinding` / `WorkloadManifest` / `RuntimeProvider`.
- #1816 — backup, replication, recovery, archive policies.
- #1818 — member shell version zero.
- #1792 — private data disclosure boundary.
- #1767 — encrypted distributed private-overlay storage.
- #1646, #1711, #1712, #1608 — action-card source paths.
- #1438 — `ProvenanceQuery` Layer 4 surface.
