---
Status: normative
Authority: spec (defines the forward-direction proof-loop contract beneath compute placement, storage durability, artifact registry, receipt clearing, federation settlement, and the steward / member status surfaces; some clauses are explicitly forward-direction)
Canonical: no
Last Reviewed: 2026-05-15
---

# Network Anti-Entropy Proof Loops

> **Status: spec, work-in-progress.** Defines the design-level contract for how ICN nodes prove that routing, replication, receipt propagation, artifact references, and federation / settlement state have not silently diverged. Names the proof-loop phases, the artifact identifiers (`AntiEntropyProbe`, `StateDigest`, `ReceiptDigest`, `ArtifactDigest`, `PeerSyncReport`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `SyncDegradedStatus`, `QuorumSyncCheck`, `FederationSyncWindow`, `RoutingProof`, `RedundancyProof`), the divergence classes, the boundary rules that keep repair from broadening locality or disclosure, the steward / member rendering surfaces, and the first safe proof-loop slice. Anchors against existing primitives in `icn-gossip` (Bloom filters, vector clocks, anti-entropy module) and `icn-core` (anti-entropy background task) without redefining them. The PR introducing this doc advances `#1799` without closing it.

<!-- truth-class: normative -->

## Purpose

ICN nodes already exchange state through gossip with Bloom filters, vector clocks, peer sync managers, and a background anti-entropy task. What is still missing — what `#1799` exists to define — is the **institutional proof-loop layer** above those mechanisms: when two peers disagree, what does the disagreement mean, who is allowed to repair it, what evidence is produced, and how is the resulting state surfaced to stewards and members?

Anti-entropy is not "eventual consistency vibes." It is a structured institutional evidence loop:

1. **Detect** divergence between peers.
2. **Prove** what diverged — which state class, which scope, which records.
3. **Identify** authority, custody, and scope affected.
4. **Repair** within authority, or escalate.
5. **Emit** receipts and evidence that the loop ran.
6. **Surface** state honestly to stewards (technical detail) and members (plain participation status).

This spec defines that loop as the proof-layer beneath:

- federation-bound and commons compute placement (per `docs/spec/compute-placement-policy.md`);
- replicated storage and durability policies (per `docs/spec/storage-durability-policies.md`);
- artifact registry references (per `docs/spec/artifact-registry-and-scoped-vault.md`);
- receipt clearing and federation settlement (per `docs/spec/federation-settlement-finality.md`);
- the steward cockpit operability surface (per `#1795`);
- the member shell "sync delayed / degraded" surface (per `#1818`).

## Scope and non-goals

This spec was originally introduced (PR #1829) as a design-level document that named the proof-loop phases, artifact identifiers, divergence classes, and boundary rules without landing implementation. Wire-stable Rust shapes have landed incrementally in `icn-kernel-api`:

- `#1834` / PR `#1843` — `AntiEntropyProbe`, the `StateDigest` family (`BloomProjection`, `MerkleRootProjection`, `VectorClockProjection`, `ShortDigestList`), the `ReceiptDigest` and `ArtifactDigest` specializations, and the `StateClass` / `ProbeScope` / `TriggerSource` / `RequestedResponseClass` enums.
- `#1835` — `DivergenceEvidence`, `RepairPlan`, the eighteen-class `DivergenceClass` taxonomy (with `Unclassifiable` fallback), and supporting helpers (`PeerSet`, `DigestMismatch`, `PolicyClauseRef`, `RepairAction`, `AuthorityBasis`, `BoundaryRuleRef`, `BoundaryRuleSet`, `ExpectedRepairReceiptClass`).
- `#1849` — `RepairReceipt`, the closed `RepairReceiptClass` taxonomy (1:1 from `ExpectedRepairReceiptClass`), and the bounded `RepairFailureReason` taxonomy. Reuses `EffectOutcome` from `icn-kernel-api/src/effects.rs` (per spec §"Evidence" line 181) rather than redefining outcome vocabulary. `RepairReceipt` remains an evidence-artifact identifier traveling inside an existing envelope; no new top-level ADR-0026 receipt class is introduced.
- `#1852` — `PeerSyncReport`, the closed five-variant `PeerSyncOutcome` taxonomy (`Matching` / `MissingOnLocal` / `MissingOnRemote` / `Divergent` / `UnknownOutOfScope`), and the closed four-variant `UnknownOutOfScopeReason` taxonomy. Cross-links to the originating `AntiEntropyProbe` via `probe_hash` and (optionally, for non-matching outcomes) to a downstream `DivergenceEvidence` via `divergence_evidence_hash`. Same self-authentication pattern as the records above. `PeerSyncReport` remains an evidence-artifact identifier traveling inside an existing envelope; no new top-level ADR-0026 receipt class is introduced.

The remaining identifiers stay design-level / forward-direction.

**In scope:**

- Naming the proof-loop phases, artifact identifiers, divergence classes, and boundary rules.
- Cross-linking the existing code surface (`icn-gossip` anti-entropy module, Bloom filter primitives, peer sync manager, partition detector, vector clock merger, `icn-core` background anti-entropy task) so future implementation work has unambiguous anchors.
- Defining steward and member surface vocabulary.
- Naming the first safe proof-loop / dogfood slice.
- Naming the privacy / custody rules that keep proof loops from leaking private data.
- As of `#1834` / PR `#1843`: wire-stable record shapes for `AntiEntropyProbe` and the `StateDigest` family. As of `#1835`: wire-stable record shapes for `DivergenceEvidence` (with the eighteen-class `DivergenceClass` taxonomy) and `RepairPlan`. As of `#1849`: wire-stable record shape for `RepairReceipt` (with the closed `RepairReceiptClass` taxonomy mapping 1:1 from `ExpectedRepairReceiptClass` and the bounded `RepairFailureReason` taxonomy; reuses `EffectOutcome` from `icn-kernel-api/src/effects.rs`). As of `#1852`: wire-stable record shape for `PeerSyncReport` (with the closed five-variant `PeerSyncOutcome` taxonomy and the closed four-variant `UnknownOutOfScopeReason` taxonomy; cross-links to the originating `AntiEntropyProbe` and, for non-matching outcomes, optionally to a downstream `DivergenceEvidence`). See §"Proof artifacts (forward-direction names)" below for which identifiers are now wire-stable and which remain design-level.

**Not in scope (preserved out of this spec and out of `#1834` / `#1835` / `#1849` / `#1852`):**

- Implementation of the remaining design-level artifact names. `SyncDegradedStatus`, `QuorumSyncCheck`, `FederationSyncWindow`, `RoutingProof`, and `RedundancyProof` remain forward-direction names. The receipt-index anti-entropy fixture loop (Slice A) is tracked under `#1838`. The follow-up that retrofits the `FixtureSyncOutcome` stand-in in `receipt_index_anti_entropy_slice_a.rs` to consume the public `PeerSyncReport` is intentionally split out of `#1852` and remains forward work.
- Live emission, classification, or repair. Even with the `AntiEntropyProbe`, `StateDigest`, `PeerSyncReport`, `DivergenceEvidence`, `RepairPlan`, and `RepairReceipt` wire shapes now defined, no code path constructs, signs, gossips, classifies, repairs, or autonomously emits these records. They are constructible and testable in isolation only.
- Schema migration for existing types in `icn-gossip` / `icn-net` / `icn-federation`.
- Network protocol mutation. No changes to gossip topic strings, message envelopes, or peer discovery.
- Live federation rollout, devnet provisioning, K3s manifest changes, DNS changes, Forgejo or identity-bridge changes.
- Production-readiness claim. No clause of this spec is a claim that ICN-native anti-entropy is production-ready, that any partner federation is operating proof loops today, or that NYCN is a formal pilot.
- Scheduler, gateway, runtime, admission engine, or settlement engine implementation.
- Re-definition of `ArtifactReceipt`, `GovernanceProof`, `MerkleProof`, `ClearingReceipt`, or any other receipt class already named in ADR-0026 / ADR-0031 / `docs/spec/federation-settlement-finality.md`. No new top-level ADR-0026 receipt class is introduced; the `AntiEntropyProbe` is an evidence envelope, not a receipt.
- Re-definition of the `BackupPolicy`, `ReplicationPolicy`, `RecoveryPolicy`, `ArchivePolicy`, `IntegrityPolicy` shapes (per `docs/spec/storage-durability-policies.md`).
- Encrypted private-overlay implementation (tracked in `#1767`).
- PrivacyClass taxonomy reconciliation (tracked in `#1792`).
- Adversarial / chaos harness (tracked in `#1010`).
- Closure of `#1799`. The PR introducing this doc used `Refs:`; closure remains forward work against `#1799`'s acceptance criteria as additional proof-rail slices land.

## Relationship to current canon

This spec sits beneath all of the following and supplies the evidence loop they assume:

- `docs/spec/compute-placement-policy.md` §"Boundary rules" 4 and 6 (federation / external-custodian fail-closed gates) and §"Fallback behavior" rule 5 ("No silent fallback to commons") all assume that placement can require fresh anti-entropy evidence before authorizing federation- or commons-scope work.
- `docs/spec/storage-durability-policies.md` §"Anti-entropy expectation" and §"Locality and privacy inheritance" name this spec as the verification surface for replica divergence detection, backup verification, restore-test evidence, and repair authority.
- `docs/spec/artifact-registry-and-scoped-vault.md` integration point 6 ("Replication policy tied to privacy / disclosure class") relies on anti-entropy proofs to detect content-hash mismatch and receipt-index divergence without leaking private artifact bodies.
- `docs/spec/effect-dispatch-contract.md` §"Stage 5 — Application and evidence" is where this spec's evidence artifacts (`DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `PeerSyncReport`) travel inside `EffectDispatchEvidence` envelopes; no new ADR-0026 receipt class is introduced.
- `docs/spec/federation-settlement-finality.md` settlement-finality conditions (1–5: receipt chain complete, signature valid, dispute window elapsed, not disputed, batch flushed) all assume that the federation participants can prove their state has not silently diverged across the dispute window. This spec names that proof.
- `docs/spec/institutional-domain.md` §"Compute placement and review" and the broader `DomainPolicy` references this spec as the place where the domain's "anti-entropy expectations" are evaluated.
- `docs/spec/governed-service-binding.md` §"Lifecycle state" → "Observe" is where binding-level divergence surfaces — the binding's runtime provider feeds this spec's proof loop.
- `docs/spec/ccl-policy-registry.md` policy adoption is a peer-to-peer event; this spec names the divergence class for `policy_version_id` mismatch between peers.
- `docs/adr/ADR-0026-receipt-and-provenance-proof-envelope.md` Layer 1 (`GovernanceProof`), Layer 2 (`ArtifactReceipt`), Layer 3 (`FederationProvenance`), Layer 4 (`ProvenanceQuery`) — this spec's evidence artifacts ride inside Layer 2 / Layer 3 envelopes; no new layer is added.
- `docs/architecture/KERNEL_APP_SEPARATION.md` meaning firewall — the kernel may run `icn-gossip` and `icn-core` anti-entropy primitives blindly; the policy oracle decides whether a given divergence is harmless lag, a degraded sync, or a custody / authority violation.
- `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 — generic scope vocabulary (`LocalDomain`, `InstitutionalDomain`, `DomainPolicy`); no `Coop`-prefixed framing in generic primitives.

## Existing code surface (anchors only)

This spec **does not redefine** any of the following. They exist; the spec assumes their continued correctness and names the design-level artifact layer above them.

| Surface | Location | Role |
|---|---|---|
| `BloomFilter` | `icn/crates/icn-gossip/src/bloom.rs` | Compact set-membership digest exchanged between peers. |
| `GossipActor::get_bloom_filter` / `find_missing` / `anti_entropy_check` / `emit_digest` / `emit_all_digests` | `icn/crates/icn-gossip/src/anti_entropy.rs` | Existing per-topic Bloom-filter sync primitive. |
| `VectorClock` and `VectorClockMerger` | `icn/crates/icn-gossip/src/vector_clock.rs`, `partition.rs` | Causal ordering and merge for partition / rejoin. |
| `PeerSyncState`, `PeerSyncManager`, `Backoff` | `icn/crates/icn-gossip/src/sync.rs` | Per-peer sync tracking and back-off scheduling. |
| `PartitionDetector`, `Conflict`, `ConflictResolution`, `ResolutionOutcome`, `VersionGap`, `GapDirection`, `ConflictResolver` | `icn/crates/icn-gossip/src/partition.rs` | Partition detection and per-data-type conflict resolution. |
| `AntiEntropyConfig`, `spawn_anti_entropy_task` | `icn/crates/icn-core/src/anti_entropy.rs` | Background task that periodically exchanges Bloom filters with peers (default 30 s interval; max 100 missing per round). |
| `gossip.anti_entropy_interval` parameter | `icn/crates/icn-governance/src/protocol_defaults.rs` | Governance-tunable anti-entropy interval. |
| `ArtifactReceipt` (Layer 2 ADR-0026) | `icn/crates/icn-kernel-api/src/proofs.rs` | Existing blob-transfer / artifact receipt; the envelope this spec's repair evidence travels inside. |
| `MerkleProof` / proof-of-storage primitives | `icn/crates/icn-store/` | Existing primitives for content-addressed verification. |
| `ReceiptClearingManager`, `ClearingReceipt` | per `docs/spec/federation-settlement-finality.md` | Federation-side receipt batching; this spec names the proof requirement for the dispute window. |
| Existing gossip topic strings | `services:announce`, `services:query`, `key:rotation`, `bonds:issuance`, `bonds:payments` (legacy topic identifier in `icn-gossip/src/labor_shares.rs`; not ICN-native vocabulary framing), `network:candidates`, governance / contract topics | The closed set of topic strings. This spec does not propose new topics; future implementation work may. Verbatim quotation of existing topic identifiers — including the legacy `bonds:payments` name — does not imply endorsement of payment-vocabulary framing; the topic identifier predates the vocabulary boundary doctrine and is preserved without renaming. |

## Anti-entropy loop model

The loop runs in eight ordered phases. A complete pass through the loop produces evidence; an aborted pass produces evidence of the abort. The kernel and the gossip layer execute phases 1–3 and 6 blindly; the policy oracle (per `docs/architecture/KERNEL_APP_SEPARATION.md`) executes phases 4, 5, and 7. Phase 8 is rendering.

### 1. Schedule / trigger

Loop entry conditions, in order of priority:

- **Periodic.** Governance-tunable interval per `gossip.anti_entropy_interval` (default 30 s).
- **Threshold-triggered.** Replica count below `ReplicationPolicy.target_replicas`; backup verification overdue; restore-test cadence missed; receipt-clearing batch nearing dispute-window expiry.
- **Governance-triggered.** A `DomainPolicy` change adopting a stricter sync expectation or a new `FederationSyncWindow` cadence; explicit steward request via the cockpit.
- **Incident-triggered.** Suspected equivocation, peer churn beyond a threshold, partition detected by `PartitionDetector`, network candidate cache anomaly.
- **Peer-requested.** A remote peer's `AntiEntropyProbe` arrives and the local node responds in scope.

The trigger source is recorded on every `AntiEntropyProbe`.

### 2. Probe

The probing peer assembles an `AntiEntropyProbe` for one state class (or a bundled set) and emits it. The probe carries:

- the state class identifier (see §"State classes covered" below);
- the target scope (`LocalDomain`, `Federation`, `Commons`, or peer-pair);
- a bounded `StateDigest` (Bloom filter, Merkle root, vector clock, or short digest list — not full state);
- the prober's identity (DID) and signature;
- a freshness timestamp and a freshness validity window;
- the requested response class (read-only digest exchange, fetch-missing, repair authorization).

Probes are content-bounded. A probe MUST NOT carry raw private content. A probe MAY carry an opaque reference to a scoped-vault object plus its `ArtifactDigest`, but never the object body.

### 3. Compare

The responding peer compares the incoming digest against its own state and produces a `PeerSyncReport`:

- **matching** — both peers' digests for this state class agree at the boundary.
- **missing on local** — peer has entries the responder does not.
- **missing on remote** — responder has entries the peer does not (responder may volunteer them subject to scope and disclosure rules).
- **divergent** — both peers claim entries at the same address (e.g., same Merkle root path) with different contents; this requires classification in phase 4.
- **unknown / out of scope** — responder cannot evaluate (insufficient authority, scope mismatch, stale freshness, unknown state class).

The `PeerSyncReport` records the comparison result, the digest forms used, the freshness, and the responder's signature. It does NOT carry raw state.

### 4. Classify

The policy oracle classifies the comparison result into a divergence class (see §"Divergence classes" below). Classification consumes the comparison, the workload's privacy / locality / authority context, the domain's policy, and the relevant federation agreement (if any). Classification produces a `DivergenceEvidence` artifact when the result is non-matching.

`DivergenceEvidence` records:

- the divergence class;
- the affected state class and scope;
- the peers involved (DIDs);
- the digest forms compared;
- the policy clause under which classification was made;
- the freshness window the evidence is valid for;
- whether private content was implicated (without disclosing it).

### 5. Plan

The policy oracle produces a `RepairPlan` (or a "no repair authorized" decision). The plan names:

- the repair action (fetch missing, re-replicate, quarantine, request governance review, escalate to federation clearing, no-op);
- the authority basis (which mandate or `DomainPolicy` clause authorizes the action);
- the scope of the repair (which records, which peers);
- the boundary rules the repair must respect (privacy class, locality, custody);
- the expected `RepairReceipt` class on completion.

Repair of governance-authoritative state always requires a covering mandate per ADR-0014 / ADR-0019; this spec does not soften that rule.

### 6. Apply

The repair executor (which may be the gossip layer, the storage layer, or a federation peer) performs the repair within the plan's scope. Existing primitives (Bloom-filter fetch-missing, partition rejoin, replica top-up, blob transfer) carry the work. The repair does not import the placement class taxonomy or the privacy taxonomy; it executes the bounded action the plan specifies.

### 7. Evidence

A `RepairReceipt` is recorded. It carries:

- the original `DivergenceEvidence` reference;
- the `RepairPlan` reference;
- the action taken and its outcome (`Applied`, `Partial`, `NoOp`, `Failed` — same vocabulary as `EffectOutcome` per ADR-0030);
- the after-state digest (so a later probe can confirm convergence);
- the actor identity and signature;
- the receipt envelope (Stage 5 `EffectDispatchEvidence` per `docs/spec/effect-dispatch-contract.md`, or Layer 2 `ArtifactReceipt` per ADR-0026 where the repair was a blob transfer).

No new top-level receipt class is added. `RepairReceipt` is an evidence-artifact identifier traveling inside an existing envelope.

### 8. Surface

Both the steward cockpit and the member shell update. The steward cockpit shows technical detail (`DivergenceEvidence`, `RepairPlan`, scope, peers, digest mismatch, last successful proof, authority required, escalation status). The member shell shows plain-language status (`Synced`, `Sync delayed`, `Some records are being verified`, `Action paused until records sync`, `Receipt available`, `Review required`). See §"Steward cockpit surface" and §"Member shell surface" below.

## State classes covered

Anti-entropy proof loops apply to the following state classes. Each class names what is digested and what privacy / custody rules apply.

| State class | Digested as | Privacy / custody rule |
|---|---|---|
| Governance state — accepted proposals, effect dispatch evidence (per `docs/spec/effect-dispatch-contract.md`) | Merkle root over canonical decisions and Stage 5 evidence artifacts | Public within the issuing domain; cross-domain probes require federation agreement. |
| Receipts and receipt indexes (per ADR-0026) | Bloom filter over receipt content-hashes, or Merkle root over receipt index | Indexed receipts may be probed by scope; the receipt body's disclosure rules apply on fetch. |
| Artifact registry metadata (per `docs/spec/artifact-registry-and-scoped-vault.md`) | Bloom filter over `content_hash` + `receipt_refs` index | Replication is bounded by `privacy_class`; `PrivateOverlay` artifacts never replicate to `FederationMirrored` or `CommonsPublic`. |
| Scoped vault references (per `docs/spec/artifact-registry-and-scoped-vault.md`) | Opaque `ArtifactDigest` only | Reference is digestible; body is not. Probes prove existence and scope; they do not reveal content. |
| Storage replicas and backup / restore verification metadata (per `docs/spec/storage-durability-policies.md`) | Replica-count digest, backup-verification digest, restore-test receipt reference | Locality inheritance is non-broadening. Repair MUST NOT widen `DataLocality` or privacy class. |
| Compute receipts and placement / admission evidence (per `docs/spec/compute-placement-policy.md`) | Stage 5 evidence-artifact digests + `policy_version_id` | Federation- and commons-scope probes require the corresponding agreement to be adopted by both ends. |
| Settlement / obligation / allocation / position records (per `docs/spec/federation-settlement-finality.md` and `#1634`) | Clearing-batch digest + dispute-window timestamp | Federation participants must have adopted the settlement policy. No payment / wallet / currency framing in any probe field. |
| Federation membership / peer identity / trust-and-admission metadata | Membership digest + signed peer roster | Peer identity changes require signed peer roster; equivocation produces `DivergenceEvidence` of suspected peer misbehavior. |
| CCL policy registry versions and evaluator bindings (per `docs/spec/ccl-policy-registry.md`) | `policy_version_id` and `evaluator_binding_id` digests | Adoption is a governance act; missing adoption is divergence, not silent upgrade. |

## Proof artifacts (forward-direction names)

The following identifiers are **design-level names** introduced by this spec. Each travels inside an existing receipt envelope (Stage 5 `EffectDispatchEvidence` per `docs/spec/effect-dispatch-contract.md`, or Layer 2 `ArtifactReceipt` per ADR-0026 for blob-transfer repair).

Several of these now have wire-stable Rust shapes in `icn/crates/icn-kernel-api/src/proofs.rs`:

- Per `#1834` (PR `#1843`): `AntiEntropyProbe` and the `StateDigest` family (`BloomProjection`, `MerkleRootProjection`, `VectorClockProjection`, `ShortDigestList`), together with the `ReceiptDigest` and `ArtifactDigest` newtype specializations and the `StateClass` / `ProbeScope` / `TriggerSource` / `RequestedResponseClass` enums. The Bloom projection is wire-equivalent to `icn_gossip::types::BloomFilterData` plus an explicit cardinality hint; cross-link helpers (`icn_gossip::to_bloom_projection` / `icn_gossip::to_bloom_filter_data`, re-exported from the crate root) preserve byte-level membership across the boundary.
- Per `#1835`: `DivergenceEvidence`, `RepairPlan`, the closed eighteen-class `DivergenceClass` taxonomy (with `Unclassifiable` fallback), and supporting helpers `PeerSet`, `DigestMismatch`, `PolicyClauseRef`, `RepairAction`, `AuthorityBasis`, `BoundaryRuleRef`, `BoundaryRuleSet`, `ExpectedRepairReceiptClass`. Both records follow the `#1843` self-authentication pattern (domain-tagged blake3 binding, `verify_binding()` fail-closed on unsupported `schema_version`, externally-tagged bincode-compatible enums, deserialize-time canonicalization for `PeerSet` / `BoundaryRuleSet`). `RepairPlan.divergence_evidence_hash` links a plan to the evidence it acts on for auditability.
- Per `#1849`: `RepairReceipt`, the closed `RepairReceiptClass` taxonomy (mapping 1:1 from `ExpectedRepairReceiptClass`), and the bounded `RepairFailureReason` taxonomy. `RepairReceipt` reuses `EffectOutcome` from `icn-kernel-api/src/effects.rs` (per §"Evidence" line 181), follows the `#1843` self-authentication pattern (domain tag `b"icn:repair-receipt:v1"`, `verify_binding()` fail-closed on unsupported `schema_version`), cross-links to both `DivergenceEvidence` (via `divergence_evidence_hash`) and `RepairPlan` (via `repair_plan_hash`), and validates structural outcome / reason / digest invariants on deserialize and at construction (`Applied` / `NoOp` reject `failure_reason`; `Partial` / `Failed` require it; `Failed` rejects `after_state_digest`).
- Per `#1852`: `PeerSyncReport`, the closed five-variant `PeerSyncOutcome` taxonomy (`Matching` / `MissingOnLocal` / `MissingOnRemote` / `Divergent` / `UnknownOutOfScope`), and the closed four-variant `UnknownOutOfScopeReason` taxonomy (`StaleProbe` / `ScopeMismatch` / `InsufficientAuthority` / `UnknownStateClass`). Follows the `#1843` self-authentication pattern (domain tag `b"icn:peer-sync-report:v1"`, `verify_binding()` fail-closed on unsupported `schema_version`), cross-links to the originating `AntiEntropyProbe` via `probe_hash`, and (for non-matching outcomes only) optionally cross-links to a downstream `DivergenceEvidence` via `divergence_evidence_hash`. Validates the structural outcome / divergence-link invariant on deserialize and at construction (`Matching` / `UnknownOutOfScope` reject `divergence_evidence_hash`).

None of these wire shapes mutates protocol topics, emits probes, classifies divergences live, applies repairs, or adds a new ADR-0026 receipt class — the kernel records are evidence envelopes that travel inside an existing Stage 5 `EffectDispatchEvidence` per `docs/spec/effect-dispatch-contract.md` (or, for blob-transfer repairs, alongside a Layer 2 `ArtifactReceipt` per ADR-0026). The remaining identifiers below remain design-level names; `SyncDegradedStatus`, `QuorumSyncCheck`, `FederationSyncWindow`, `RoutingProof`, and `RedundancyProof` are forward-direction and not yet tracked under a dedicated issue. The fixture loop is tracked under `#1838`, and the retrofit that wires the `FixtureSyncOutcome` stand-in in the receipt-index fixture to consume the public `PeerSyncReport` is intentionally a separate follow-up.

- **`AntiEntropyProbe`** — the probing message: state class, target scope, bounded digest, trigger source, freshness, signature. **Wire-stable** (`#1834`).
- **`StateDigest`** — a bounded representation of a state class at a freshness instant; concrete forms include Bloom filter (existing `BloomFilter`), Merkle root, vector clock, or short digest list. **Wire-stable** (`#1834`).
- **`ReceiptDigest`** — a `StateDigest` specialized to a receipt index. **Wire-stable** (`#1834`).
- **`ArtifactDigest`** — a `StateDigest` specialized to an artifact-registry entry or scoped-vault reference; never the artifact body. **Wire-stable** (`#1834`).
- **`PeerSyncReport`** — the comparison result: matching / missing on local / missing on remote / divergent / unknown. **Wire-stable** (`#1852`).
- **`DivergenceEvidence`** — classified non-matching outcome; records class, scope, peers, digest forms, policy clause, freshness, private-content implication flag. **Wire-stable** (`#1835`).
- **`RepairPlan`** — repair action, authority basis, scope, boundary rules, expected `RepairReceipt` class. **Wire-stable** (`#1835`).
- **`RepairReceipt`** — evidence-artifact identifier for the repair outcome; carries before / after digests and the `EffectOutcome` value. **Wire-stable** (`#1849`).
- **`SyncDegradedStatus`** — the steward / member-facing status when the loop has detected divergence that has not yet been repaired within policy.
- **`QuorumSyncCheck`** — proof that a quorum of named federation peers exchanged matching `StateDigest`s within a stated freshness window for a stated state class; the gate for federation-bound placement and federation settlement.
- **`FederationSyncWindow`** — the freshness window the domain's policy requires for `QuorumSyncCheck` to count as fresh; named per state class (e.g., settlement records may require a stricter window than artifact-registry metadata).
- **`RoutingProof`** — evidence that a message or receipt reached its intended peer(s); produced by gossip-layer acknowledgement and signed receipt-of-receipt.
- **`RedundancyProof`** — evidence that the live replica count for an artifact meets or exceeds `ReplicationPolicy.target_replicas`; produced by the replica-count probe.

## Routing and redundancy checks

`RoutingProof` and `RedundancyProof` are the proof complements of the placement and storage policies.

- **`RoutingProof`** is the evidence side of #1799's proof scenario 1 ("multi-node gossip"). It records that a generic event / receipt was emitted by node A and acknowledged by intended-recipient peers B / C, with hashes and signatures preserved. The kernel records the proof; the policy oracle decides whether the proof is fresh enough for the placement / settlement decision that depends on it.
- **`RedundancyProof`** is the evidence side of #1799's proof scenario 4 ("replica failure / re-replication"). It records that an artifact (per `docs/spec/artifact-registry-and-scoped-vault.md`) currently has at least `ReplicationPolicy.target_replicas` live replicas in scope. A failing `RedundancyProof` triggers a `DivergenceEvidence` of class "replica lag" or "replica missing" and a `RepairPlan` action of "re-replicate."

Both proofs are bounded:

- A `RoutingProof` ages out after the freshness window the domain's policy specifies.
- A `RedundancyProof` is per-artifact; the loop produces it only for artifacts whose `ReplicationPolicy` declares a `target_replicas > 1`.
- Neither proof claims production reachability or live federation; both are honest about freshness and scope.

## Divergence classes

A `DivergenceEvidence` artifact carries exactly one class. The closed set:

1. **Missing receipt** — peer A has a receipt for an effect at a known content hash; peer B does not.
2. **Conflicting receipt** — peer A and peer B both claim a receipt at the same logical identifier (same effect, same scope, same `policy_version_id`) but the receipts have different content hashes.
3. **Missing artifact metadata** — peer A has an `ArtifactRegistry` entry; peer B's index does not include it.
4. **Content hash mismatch** — both peers have the artifact under the same logical name; their content hashes differ.
5. **Replica lag** — current replica count is below `ReplicationPolicy.target_replicas` but the policy permits a grace window before escalation.
6. **Replica missing** — replica count has fallen outside the grace window; the policy authorizes re-replication.
7. **Backup verification failure** — the most recent `BackupPolicy`-prescribed backup did not verify (per `docs/spec/storage-durability-policies.md`).
8. **Restore drill missing** — the `RecoveryPolicy` cadence has elapsed without a successful restore-test receipt.
9. **Peer behind sync window** — peer's freshness timestamp falls outside the domain's `FederationSyncWindow` for the state class in question.
10. **Peer equivocation** — peer is observed making inconsistent claims to different peers about the same state at the same time. Treated as suspected misbehavior; classification is "suspected equivocation" pending governance review.
11. **Federation agreement mismatch** — both peers claim to be operating under the same federation agreement but reference different `agreement_id` content hashes or different adopted versions.
12. **CCL policy version mismatch** — peer's `policy_version_id` for a named policy disagrees with the local adopted version (per `docs/spec/ccl-policy-registry.md`).
13. **Evaluator binding mismatch** — peer's `evaluator_binding_id` for a named evaluator disagrees with the local binding (per `docs/spec/ccl-policy-registry.md`).
14. **Placement evidence missing** — peer cannot produce the `PlacementDecision` evidence (per `docs/spec/compute-placement-policy.md`) for a workload that was claimed to have completed in scope.
15. **Settlement record mismatch** — peer's clearing-batch digest disagrees with the local clearing manager's view (per `docs/spec/federation-settlement-finality.md`).
16. **Private object reference mismatch without content disclosure** — both peers have a scoped-vault reference at the same logical identifier; the `ArtifactDigest`s disagree; the divergence is recorded as existence-plus-scope-plus-affected-records, never as content.
17. **Integrity policy violation** — the most recent `IntegrityPolicy`-prescribed verification failed (per `docs/spec/storage-durability-policies.md`).
18. **Unclassifiable** — the comparison is non-matching but does not fit any of the above; produces `DivergenceEvidence` with class "unclassifiable" and triggers governance review rather than automatic repair.

## Boundary rules

The following rules are load-bearing. A repair plan that violates any rule is invalid. A scheduler, gossip layer, or storage backend that admits a repair violating any rule is a meaning-firewall violation.

1. **No silent repair of governance-authoritative state.** Repair of a `GovernanceProof` (Layer 1, ADR-0026) or an `InstitutionalEffectRecord` (per ADR-0025) requires a covering mandate per ADR-0014 / ADR-0019. The anti-entropy loop produces `DivergenceEvidence`; it does not produce governance-authoritative output without a mandate.
2. **No repair beyond authority.** A `RepairPlan` may only act within the authority basis it names. The kernel enforces the bound; the policy oracle decided it.
3. **No raw private content in gossip or probes.** Probes carry digests, not bodies. `ArtifactDigest` for a scoped-vault object reveals nothing about the body.
4. **No widening of locality or disclosure during repair.** A repair that would move data to a wider `DataLocality` than the source allowed is rejected. A repair that would re-tag an artifact's `privacy_class` to a wider class is rejected. Per `docs/spec/storage-durability-policies.md` §"Locality and privacy inheritance."
5. **No treating degraded sync as healthy.** If a `PeerSyncReport` is "divergent" or "missing on local" and the divergence is not yet repaired within the freshness window, the surface MUST render `SyncDegradedStatus`. The cockpit MUST NOT show healthy; the member shell MUST NOT show "Synced."
6. **No federation- or commons-scope placement if required sync proof is stale beyond policy window.** Per `docs/spec/compute-placement-policy.md` §"Boundary rules" 4 and 5, federation execution requires an explicit agreement and commons execution goes through ADR-0031. This spec adds: both also require a fresh `QuorumSyncCheck` within the relevant `FederationSyncWindow` for the state classes the workload depends on. A stale or missing proof yields `RejectedByPolicy` on the placement side.
7. **No settlement finality claim without anti-entropy proof where federation scope applies.** Per `docs/spec/federation-settlement-finality.md` finality conditions (receipt chain complete, signature valid, dispute window elapsed, not disputed, batch flushed): the "receipt chain complete" condition is evaluated against a fresh `QuorumSyncCheck` over the relevant state classes; if the check is missing or stale, finality is not claimed and the dispute window is restarted.
8. **No member-facing lie.** If sync is degraded, the member shell says "sync delayed" or "some records are being verified" — never "synced" or "confirmed."
9. **No production / live-federation claim.** No clause of this spec is a claim that ICN-native anti-entropy is production-ready; that any partner federation is operating proof loops today; or that NYCN is a formal pilot.
10. **No `Coop`-prefixed generic primitives.** Per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3, generic scope vocabulary uses `LocalDomain`, `InstitutionalDomain`, `Domain`, `DomainPolicy`. The federation- and commons-class names that appear here (`Federation`, `Commons`) are structural, not institution-package nouns.

## Privacy and custody rules

Anti-entropy must not leak private data. This is restated separately because it is the most easily misread constraint.

- **`ScopedVault`-backed artifacts** participate in proof loops as opaque `ArtifactDigest` only. The probe proves existence and scope; the body never moves through gossip.
- **Digest / proof surfaces must preserve the source's disclosure policy.** A digest derived over a `Member`- or `NeedToKnow`-class object MUST NOT travel outside the disclosure scope of the source.
- **Private divergence may be surfaced as existence / affected scope / review path** without exposing restricted content. Example: "Three private artifacts in the care-plan vault have divergent digests with peer X. Review required." Not: the contents of the artifacts.
- **Export or repair across domains requires explicit authority.** A repair that would copy a private artifact across a domain boundary requires a covering mandate that names the cross-domain authority.
- **No repair may broaden locality or disclosure.** Boundary rule 4 is restated here because it is easy to violate by accident in well-meaning repair flows.
- **Anti-entropy can prove mismatch without revealing the object** when policy requires. A `DivergenceEvidence` with class "private object reference mismatch without content disclosure" is the explicit surface for this case.

## Steward cockpit surface

Per `#1795`, the steward cockpit shows operator detail. Each open `DivergenceEvidence` renders with:

- **Affected scope** — `LocalDomain`, `Federation`, `Commons`, or peer-pair.
- **State class** — which class from §"State classes covered" the divergence affects.
- **Peers** — DIDs involved.
- **Digest mismatch** — which digest form was used and what it showed (Bloom-filter set-difference, Merkle root inequality, vector-clock divergence, etc.).
- **Last successful proof** — when the last matching `PeerSyncReport` for this scope was recorded.
- **Repair plan** — the action, the authority required, the expected `RepairReceipt`.
- **Authority required** — the mandate or `DomainPolicy` clause needed (if the plan requires explicit grant).
- **Receipts / evidence** — links to `DivergenceEvidence`, `RepairPlan`, and any `RepairReceipt` already produced.
- **Escalation status** — whether the divergence has escalated to governance review (e.g., for unclassifiable divergence, equivocation, or boundary-rule violations).

Cockpit copy uses the technical vocabulary defined in this spec. No member-shell plain-language substitution.

## Member shell surface

Per `#1818`, the member shell renders only plain participation status. The closed set of strings:

- **Synced** — all relevant proof loops have a fresh matching `PeerSyncReport` for this member's affected scope.
- **Sync delayed** — a recent `PeerSyncReport` is "missing on remote" or "missing on local" but the policy's grace window has not expired.
- **Some records are being verified** — a `DivergenceEvidence` is open and a `RepairPlan` is in flight; outcome is not yet known.
- **Action paused until records sync** — the member's pending action depends on a state class that is currently in `SyncDegradedStatus`; the action is held until the loop converges or is escalated.
- **Receipt available** — the relevant `RepairReceipt` (or `RoutingProof`, or `RedundancyProof`) has landed; the surface can render confirmation.
- **Review required** — divergence has been escalated to governance review; the member is informed without seeing technical detail.
- **Sync delayed / degraded** — a `SyncDegradedStatus` that has persisted beyond the policy's grace window; honest signaling that the institution is not currently in a healthy sync state.

No raw protocol jargon. No Bloom-filter, Merkle-root, vector-clock, or `policy_version_id` wording. The shell renders state, not mechanism.

## Failure and safety table

| Failure | Where it surfaces | Disposition |
|---|---|---|
| Probe arrives without a valid signature | Gossip layer | Drop; emit metric; do not produce `PeerSyncReport`. |
| Probe arrives with stale freshness | Compare phase | `PeerSyncReport` records "unknown / out of scope"; no `DivergenceEvidence` produced. |
| Probe targets a state class outside the domain's adopted scope | Compare phase | `PeerSyncReport` records "unknown / out of scope." |
| Comparison shows "missing on local" within grace window | Classify | `DivergenceEvidence` class "replica lag" or "missing receipt"; `RepairPlan` action "fetch missing." |
| Comparison shows "missing on local" past grace window | Classify | `DivergenceEvidence` class promoted to "replica missing"; `SyncDegradedStatus` surfaced. |
| Comparison shows "divergent" on a governance-authoritative artifact | Classify | `DivergenceEvidence` class "conflicting receipt"; `RepairPlan` requires governance review per Boundary rule 1. |
| Comparison shows "divergent" on a private scoped-vault reference | Classify | `DivergenceEvidence` class "private object reference mismatch without content disclosure"; cockpit renders existence + scope, never content. |
| Suspected peer equivocation detected via cross-peer comparison | Classify | `DivergenceEvidence` class "peer equivocation"; mandatory escalation to governance review; affected peer's contributions quarantined pending resolution. |
| `QuorumSyncCheck` cannot reach quorum within `FederationSyncWindow` | Classify | `DivergenceEvidence` class "peer behind sync window"; placement decisions consuming this proof yield `RejectedByPolicy` for `FederationBound` and `CommonsEligible` per Boundary rule 6. |
| `RepairPlan` proposes widening locality or disclosure | Plan | Plan rejected; `DivergenceEvidence` updated with rejection reason; escalation per Boundary rule 4. |
| `RepairPlan` proposes acting without authority basis | Plan | Plan rejected per Boundary rule 2. |
| Repair executes but the after-state digest still does not match the planned outcome | Evidence | `RepairReceipt` records `EffectOutcome::Partial` or `EffectOutcome::Failed`; cockpit retains open `DivergenceEvidence`. |
| Backup verification failure detected by `IntegrityPolicy`-driven probe | Classify | `DivergenceEvidence` class "backup verification failure"; `RepairPlan` references the `BackupPolicy` retry path. |
| `RecoveryPolicy` cadence elapsed without a successful restore-test receipt | Classify | `DivergenceEvidence` class "restore drill missing"; mandatory escalation per `docs/spec/storage-durability-policies.md`. |
| Settlement-side `QuorumSyncCheck` stale at dispute-window expiry | Classify | `DivergenceEvidence` class "settlement record mismatch"; finality not claimed; dispute window restarts per Boundary rule 7. |
| Peer roster changes without signed roster | Classify | `DivergenceEvidence` class "federation agreement mismatch"; affected peer's contributions held pending roster verification. |
| Probe attempted on a state class with no canonical digest form | Compare | `PeerSyncReport` "unknown / out of scope"; tracked as a follow-up to extend the closed state-class set. |

## First safe proof-loop / dogfood slice

Per `#1799`'s acceptance criterion 1 ("Network proof-loop design doc or test plan merged"), this spec names a docs-and-fixture-only first slice that exercises the contract without touching real network, real federation, or private data.

### Slice A (preferred): Read-only receipt-index anti-entropy rehearsal

Fixture-based exercise of phases 1–8:

- **Fixtures.** Two or three peer fixtures (DIDs assigned, signing keys local). Each peer holds a small receipt index over public, fixture-only receipts (no real artifacts; no private data).
- **Probe.** Peer A constructs an `AntiEntropyProbe` for the receipt-index state class, scoped to a fixture `LocalDomain`, carrying a `ReceiptDigest` in `BloomFilter` form (existing `icn-gossip` primitive).
- **Compare.** Peer B receives the probe and produces a `PeerSyncReport`. Peer B's index intentionally lacks one of peer A's receipts; the report records "missing on remote."
- **Classify.** A fixture policy oracle classifies the result as `DivergenceEvidence` of class "missing receipt." Private-content implication flag: false.
- **Plan.** The fixture policy oracle produces a `RepairPlan` of action "fetch missing receipt from peer A, scope-bounded to the fixture domain, no widening."
- **Apply.** Peer B fetches the missing receipt over the existing gossip primitive.
- **Evidence.** A `RepairReceipt` is recorded with `EffectOutcome::Applied`, the before-/after-digests, and the actor identity.
- **Surface.** A fixture steward-cockpit view renders the open-then-resolved `DivergenceEvidence`. A fixture member-shell view renders "sync delayed" → "receipt available."

The slice produces nothing that crosses a real network, exercises no private artifacts, mutates no real runtime state, and makes no live-federation claim.

**Implementation status (`#1838`).** Slice A is implemented as a fixture-only Rust integration test at `icn/crates/icn-kernel-api/tests/receipt_index_anti_entropy_slice_a.rs`. It exercises the schema chain end-to-end — `AntiEntropyProbe` → `ReceiptDigest` over `BloomProjection` → fixture compare → `DivergenceEvidence::MissingReceipt` → `RepairPlan::FetchMissing` → fixture apply → after-state assertion → fixture cockpit / member-shell render — using in-memory `BTreeMap` peer indexes. No sockets, no gossip actor, no spawned tasks, no runtime mutation; the `icn-gossip` `BloomFilter` primitive is used as a dev-only dependency for the wire-form Bloom projection. The `PeerSyncReport` and `RepairReceipt` identifiers remain design-level — the fixture uses a private `FixtureSyncOutcome` enum and asserts after-state directly against the peer indexes instead of constructing a public `RepairReceipt`. The fixture is a proof-of-shape, not a proof-of-runtime: it does not claim ICN-native anti-entropy operates today against real peers and does not advance the production-readiness or live-federation posture of the spec.

### Slice B (after Slice A): replica-count `RedundancyProof` simulation

Same fixture structure. A small set of fixture artifacts with `ReplicationPolicy.target_replicas = 3` is held by only two of the three fixture peers. A `RedundancyProof` probe surfaces the gap. `DivergenceEvidence` of class "replica missing" is recorded. A `RepairPlan` of action "re-replicate within scope" runs. A `RepairReceipt` records `EffectOutcome::Applied`. No real artifacts; no real replicas.

### Slice C (after governance-review path is specified): `QuorumSyncCheck` for fixture federation placement

A fixture federation agreement is adopted by three fixture domain peers. A `QuorumSyncCheck` is requested for the receipt-index state class within a fixture `FederationSyncWindow`. The proof either succeeds (placement decision proceeds) or fails (placement returns `RejectedByPolicy` per `docs/spec/compute-placement-policy.md` §"Boundary rules" 6 + this spec's Boundary rule 6).

All three slices are fixture-first. None is a live-network claim. None is an implementation milestone in this PR.

## Relationship to sibling work

| Concern | Where it lives |
|---|---|
| Compute placement classes and the federation / commons fail-closed gates | `docs/spec/compute-placement-policy.md` (#1801, merged #1826) |
| Storage durability policy objects and replica / backup / restore semantics | `docs/spec/storage-durability-policies.md` (#1816, merged #1823) |
| ArtifactRegistry metadata and ScopedVault opaque references | `docs/spec/artifact-registry-and-scoped-vault.md` (#1798, merged #1824) |
| Governance decision → mandate → effect dispatch chain | `docs/spec/effect-dispatch-contract.md` (#1797, merged #1819) |
| `InstitutionalDomain` and `DomainPolicy` | `docs/spec/institutional-domain.md` (#1794, merged #1820) |
| Governed service binding lifecycle | `docs/spec/governed-service-binding.md` (#1815, merged #1822) |
| CCL policy registry, policy adoption, evaluator selection | `docs/spec/ccl-policy-registry.md` (#1817, merged #1821) |
| Federation settlement finality and dispute window | `docs/spec/federation-settlement-finality.md` (#1365, Phase 1 spec) |
| Steward cockpit operability surface | `#1795` |
| Member shell v0 | `#1818` |
| Private data disclosure boundary | `#1792` |
| Encrypted distributed private-overlay storage | `#1767` |
| Adversarial / chaos harness | `#1010` |
| Proof-level taxonomy and capability matrix | `#1796` |
| Authority class / typed scope / mandate | ADR-0014 |
| Grant minting seam | ADR-0019 |
| Receipt and provenance proof envelope | ADR-0026 |
| Compute workload manifest and authority boundary | ADR-0030 |
| Commons compute admission and settlement | ADR-0031 |
| Entity-scope vocabulary boundary | `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 |
| Kernel / app separation and meaning firewall | `docs/architecture/KERNEL_APP_SEPARATION.md` |
| Integrated cooperative operating model spine | `docs/architecture/ICN_INTEGRATED_SYSTEM_MODEL.md` |

## Open questions and follow-up drafts (not filed)

Eight follow-ups drafted in the handoff for human review:

1. `schema(network): define AntiEntropyProbe and StateDigest records` — wire-stable record shapes for the probe and digest forms, with round-trip tests.
2. `schema(network): define DivergenceEvidence and RepairPlan records` — wire-stable record shapes for the classification and plan artifacts; ADR amendment to ADR-0026 if any new evidence layer is needed.
3. `test(devnet): add receipt-index anti-entropy fixture` — implement Slice A as a docs / fixture test that does not touch the real runtime.
4. `spec(steward-cockpit): define sync degradation and repair surface` — the cockpit-side rendering contract for `DivergenceEvidence` / `RepairPlan` / `RepairReceipt`.
5. `spec(member-shell): define sync status rendering` — the member-shell-side rendering contract for the seven status strings named here.
6. `spec(federation): define quorum sync window for federation-bound placement` — the cross-link binding between `QuorumSyncCheck` / `FederationSyncWindow` and `docs/spec/compute-placement-policy.md` §"Boundary rules" 4 + 6.
7. `spec(storage): connect replication repair receipts to StorageSpec / RecoveryPolicy` — close the cross-link between `docs/spec/storage-durability-policies.md` policy objects and this spec's `RepairReceipt` / `RedundancyProof`.
8. `spec(privacy): define private-object digest proof without content disclosure` — the formal contract for divergence class 16 ("private object reference mismatch without content disclosure"); cross-link `#1792` and `#1767`.

## Non-claims (repeat block for grep clarity)

- This spec does not implement an anti-entropy network protocol. No code lands here.
- This spec does not claim production readiness for ICN-native anti-entropy.
- This spec does not claim any partner federation is operating anti-entropy proof loops today.
- This spec does not claim NYCN is a formal pilot.
- This spec does not move private data through gossip.
- This spec does not implement encryption, key custody, or scoped-vault encryption schemes.
- This spec does not redefine `ArtifactReceipt`, `GovernanceProof`, `MerkleProof`, `ClearingReceipt`, the seven runtime classes, the seven placement classes, the storage-durability policy objects, or the CCL evaluator-selection contract.
- This spec does not rename `FuelLimit`, `fuel_limit`, `payment_rate`, `payment_currency`, `DataLocality::CoopReplicated`, the `icn-coop` crate, or `coop_core` module paths.
- This spec does not introduce new receipt classes; placement / divergence / repair artifacts travel inside existing receipt envelopes.
- This spec does not implement, mutate, or claim live state for K3s, DNS, Forgejo, the gateway, the SDK, the website, the scheduler, the storage backend, the federation peer registry, or any deployed infrastructure.
- This spec does not specify retry semantics for failed repairs beyond noting that an `EffectOutcome::Failed` re-enters the loop via a new probe if and when the trigger conditions are met.
- This spec does not use unsafe vocabulary (payment, wallet, balance, currency, token, timebank, crypto, blockchain) for ICN-native compute / settlement / federation surfaces. All such terms in this doc appear either in explicit negation context (Boundary rules, Privacy / custody rules, Non-claims) or as **verbatim quotations of existing code identifiers** that predate the vocabulary boundary doctrine (the legacy `bonds:payments` gossip topic identifier in `icn-gossip/src/labor_shares.rs`, and the legacy `payment_rate` / `payment_currency` fields on `ComputeTask` that this spec preserves without endorsement and that prior handoffs track as a separate reconciliation follow-up). Quoting an existing identifier verbatim is not the same as endorsing its framing.
