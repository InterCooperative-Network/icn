# Handoff — Network Anti-Entropy Proof Loops (#1799)

**Date:** 2026-05-15
**Branch:** `spec/network-anti-entropy-proof-loops`
**Primary issue advanced:** [#1799](https://github.com/InterCooperative-Network/icn/issues/1799) (network routing, redundancy, and anti-entropy proof loops)
**Refs (not closed):** `#1799`, `#1010`, `#1365`, `#1767`, `#1792`, `#1795`, `#1796`, `#1797`, `#1798`, `#1801`, `#1815`, `#1816`, `#1817`, `#1818`, `#1820`, `#1822`, `#1823`, `#1824`, `#1825`, `#1826`
**Closes:** none

---

## Session Goal
<!-- TRUTH TYPE: Intent — what we set out to do -->

Define `docs/spec/network-anti-entropy-proof-loops.md` as the design-level contract for ICN's anti-entropy proof loops: the institutional evidence loop sitting beneath compute placement, storage durability, artifact registry, receipt clearing, and federation settlement finality. The spec consumes existing primitives in `icn-gossip` (BloomFilter, VectorClock, PeerSyncManager, PartitionDetector, anti-entropy module) and `icn-core` (background anti-entropy task) without redefining them; it introduces design-level proof-artifact names (`AntiEntropyProbe`, `StateDigest`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `QuorumSyncCheck`, `FederationSyncWindow`, `RoutingProof`, `RedundancyProof`, etc.) and names the closed sets of state classes, divergence classes, and boundary rules.

The spec advances #1799's acceptance criterion 1 ("Network proof-loop design doc or test plan merged") and provides a credible interpretation of criterion 6 ("Spec identifies first safe proof-loop or dogfood slice without starting implementation") via three fixture-first slices. Closure is left for human review.

---

## Current Repo State Reviewed
<!-- TRUTH TYPE: Implementation truth — facts verified before drafting -->

### `main` HEAD

`2a5b75c58` — `docs(agents): reconcile handoff path with template (#1827)`

### Recent merged PRs

`#1814`, `#1819`, `#1820`, `#1821`, `#1822`, `#1823`, `#1824`, `#1825`, `#1826`, `#1827`.

### Sibling issues verified OPEN at session start

`#1799` (primary; this PR advances), `#1801`, `#1818`, `#1798`, `#1767`, `#1792`, `#1010`, `#1796`, `#1795`, `#1815`, `#1816`, `#1817`.

### Existing code surface inspected

- `icn/crates/icn-gossip/src/anti_entropy.rs` — `GossipActor::get_bloom_filter`, `find_missing`, `anti_entropy_check`, `emit_digest`, `emit_all_digests`. The Bloom-filter-based sync primitive is already implemented.
- `icn/crates/icn-gossip/src/bloom.rs` — `BloomFilter`.
- `icn/crates/icn-gossip/src/vector_clock.rs`, `partition.rs` — `VectorClock`, `VectorClockMerger`, `PartitionDetector`, `Conflict`, `ConflictResolution`, `ResolutionOutcome`, `VersionGap`, `GapDirection`, `ConflictResolver`.
- `icn/crates/icn-gossip/src/sync.rs` — `PeerSyncState`, `PeerSyncManager`, `Backoff`.
- `icn/crates/icn-core/src/anti_entropy.rs` — `AntiEntropyConfig` (default 30 s interval; 100 max missing per round), `spawn_anti_entropy_task` (background gossip-anti-entropy task).
- `icn/crates/icn-governance/src/protocol_defaults.rs` — `gossip.anti_entropy_interval` is a governance-tunable parameter.
- `icn/crates/icn-federation/src/lib.rs` — federation crate exists; modules are `error`, `gossip`, `metrics`, `registry`, `types`; DID prefix-format routing.
- `docs/spec/federation-settlement-finality.md` — Phase 1 spec defines `ClearingReceipt`, `ReceiptClearingManager`, 72-hour dispute window, bilateral / multilateral netting.

### Forward-direction names introduced by this spec (all design-level only)

`AntiEntropyProbe`, `StateDigest`, `ReceiptDigest`, `ArtifactDigest`, `PeerSyncReport`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, `SyncDegradedStatus`, `QuorumSyncCheck`, `FederationSyncWindow`, `RoutingProof`, `RedundancyProof`. None lands as a Rust type in this PR.

---

## Files Changed

1. `docs/spec/network-anti-entropy-proof-loops.md` — new file (~470 lines).
2. `docs/registry.toml` — new entry inserted before `[docs."docs/spec/compute-placement-policy.md"]`.
3. `docs/INDEX.md` — Specifications section: one new line after the `compute-placement-policy.md` entry.
4. `docs/DOCUMENT_REGISTRY.md` — regenerated via `--write-document-registry`. Corpus moves from 811 to 812 markdown files under `docs/`.
5. `docs/dev/handoff-2026-05-15-network-anti-entropy-proof-loops.md` — this file.

No Rust code touched. SDK untouched. Website untouched. Deploy scripts untouched. Existing specs not modified.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. `docs/spec/network-anti-entropy-proof-loops.md` (new doc)

Authority class: `normative`. Length: roughly 470 lines. Sections:

- **Purpose** — names the proof-loop institutional evidence layer above existing gossip / Bloom-filter primitives; lists the six loop steps (detect, prove, identify, repair, emit, surface).
- **Scope and non-goals** — twelve non-claim bullets including: no implementation, no schema migration, no protocol mutation, no live federation, no production claim, no scheduler / gateway / runtime change, no receipt class redefinition, no `#1767` / `#1792` taxonomy migration, no `#1010` chaos work, no closure of `#1799`.
- **Relationship to current canon** — explicit pointers to each merged spec / ADR / architecture doc this spec sits beneath.
- **Existing code surface (anchors only)** — eleven-row table of existing types / files / parameters that this spec consumes without redefining.
- **Anti-entropy loop model** — eight phases: schedule / trigger, probe, compare, classify, plan, apply, evidence, surface. Each phase names which artifact it produces and which boundary rules apply.
- **State classes covered** — nine-row table covering governance state, receipts, artifact registry metadata, scoped vault references, storage replicas, compute receipts, settlement records, federation membership, CCL policy versions. Per-class privacy / custody rule recorded.
- **Proof artifacts (forward-direction names)** — thirteen design-level identifiers, each with a one-line definition; explicit "no new top-level receipt class introduced" claim, with all artifacts traveling inside existing receipt envelopes.
- **Routing and redundancy checks** — `RoutingProof` and `RedundancyProof` as the proof complements of placement and storage policies; explicit freshness bounds and absence of production-reachability claims.
- **Divergence classes** — eighteen closed classes (missing receipt, conflicting receipt, missing artifact metadata, content hash mismatch, replica lag, replica missing, backup verification failure, restore drill missing, peer behind sync window, peer equivocation, federation agreement mismatch, CCL policy version mismatch, evaluator binding mismatch, placement evidence missing, settlement record mismatch, private object reference mismatch without content disclosure, integrity policy violation, unclassifiable).
- **Boundary rules** — ten load-bearing rules. Rule 6 explicitly says federation- and commons-scope placement require a fresh `QuorumSyncCheck` within the relevant `FederationSyncWindow`. Rule 7 explicitly says settlement finality requires anti-entropy proof under federation scope.
- **Privacy and custody rules** — restates the non-leakage rules separately because they are the most easily misread. Names the divergence-class-16 surface ("private object reference mismatch without content disclosure") as the explicit pattern for proving mismatch without revealing the object.
- **Steward cockpit surface** — eight rendering fields (affected scope, state class, peers, digest mismatch, last successful proof, repair plan, authority required, receipts / evidence, escalation status).
- **Member shell surface** — closed set of seven plain-language status strings (`Synced`, `Sync delayed`, `Some records are being verified`, `Action paused until records sync`, `Receipt available`, `Review required`, `Sync delayed / degraded`).
- **Failure and safety table** — eighteen rows.
- **First safe proof-loop / dogfood slice** — three fixture-first slices: read-only receipt-index anti-entropy rehearsal (Slice A, preferred), `RedundancyProof` simulation (Slice B), `QuorumSyncCheck` fixture for federation placement (Slice C). All fixture-only; no live network; no real artifacts; no real replicas.
- **Relationship to sibling work** — table of cross-links across every merged spec, the federation-settlement-finality spec, every relevant open issue, and the five relevant ADRs.
- **Open questions and follow-up drafts** — eight follow-ups named (see below).
- **Non-claims (repeat block for grep clarity)** — twelve grep-friendly negation lines.

### 2. `docs/registry.toml` (one new entry)

`[docs."docs/spec/network-anti-entropy-proof-loops.md"]`:
- `category = "architecture"`
- `status = "draft"`
- `domain_tags = ["network", "anti-entropy", "gossip", "federation", "settlement", "storage", "replication", "divergence", "repair", "proof-loop", "kernel-app-boundary", "spec"]`
- `depends_on` enumerates the integrating spine doc, meaning-firewall doc, boundary doc, six sibling specs, the federation-settlement-finality spec, and seven ADRs.

### 3. `docs/INDEX.md` (Specifications section)

One new line after the `compute-placement-policy.md` entry.

### 4. `docs/DOCUMENT_REGISTRY.md` (regenerated)

`python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md`. 811 → 812 markdown files. 54 pre-existing yaml-mismatch warnings persist; none introduced by this PR.

### 5. `docs/dev/handoff-2026-05-15-network-anti-entropy-proof-loops.md` (this file)

Filename uses the descriptive topic suffix convention now canonicalized by PR #1827.

---

## What Explicitly Did NOT Change
<!-- TRUTH TYPE: Execution truth — preserved boundaries -->

- No Rust code in `icn/crates/icn-gossip`, `icn-net`, `icn-core`, `icn-federation`, `icn-protocol`, `icn-store`, `icn-kernel-api`, or anywhere else.
- No new or modified gossip topic strings. The existing set (`services:announce`, `services:query`, `key:rotation`, `bonds:issuance`, `bonds:payments`, `network:candidates`, governance / contract topics) is preserved verbatim.
- No new receipt classes. `ArtifactReceipt` (Layer 2 ADR-0026), `GovernanceProof`, `MerkleProof`, `ClearingReceipt` are referenced, not redefined.
- No edits to `docs/spec/compute-placement-policy.md`, `docs/spec/storage-durability-policies.md`, `docs/spec/artifact-registry-and-scoped-vault.md`, `docs/spec/effect-dispatch-contract.md`, `docs/spec/institutional-domain.md`, `docs/spec/governed-service-binding.md`, `docs/spec/ccl-policy-registry.md`, or `docs/spec/federation-settlement-finality.md`. This spec sits beneath them as a separate document.
- No edits to ADRs.
- No edits to `#1799`'s GitHub issue body.
- No `DataLocality::CoopReplicated` migration. No `FuelLimit` → `execution budget` code rename. No `payment_rate` / `payment_currency` reconciliation. No `PrivacyClass` taxonomy migration. All carried forward as separate follow-ups in prior handoffs.
- No K3s, DNS, Forgejo, gateway, storage-backend, identity-bridge, or deploy-script changes.
- No `#1010` / `#1818` / `#1767` / `#1792` work.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- **`icn/crates/icn-federation` is a real crate at the path named.** Verified by `wc -l icn/crates/icn-federation/src/lib.rs` returning 145 lines. The spec names `icn-federation` in the existing-code-surface table; if the crate is later restructured or renamed, the table needs a re-read.
- **`docs/spec/federation-settlement-finality.md` exists in `main`.** Verified by `head -25` against the file. The spec cross-links it for the dispute-window and clearing-receipt semantics; the cross-link assumes that doc's vocabulary (`ClearingReceipt`, `ReceiptClearingManager`, 72-hour dispute window) remains as it is in main.
- **The set of forward-direction proof-artifact names does not collide with existing identifiers.** Verified by `rg "AntiEntropyProbe|StateDigest|ReceiptDigest|ArtifactDigest|PeerSyncReport|DivergenceEvidence|RepairPlan|RepairReceipt|SyncDegradedStatus|QuorumSyncCheck|FederationSyncWindow|RoutingProof|RedundancyProof"` returning no matches in the codebase outside this spec. If a parallel in-flight PR adds any of these names with a different shape, the spec's forward-direction claim needs reconciliation.
- **`gossip.anti_entropy_interval` is a real governance parameter.** Verified by `grep` against `icn-governance/src/protocol_defaults.rs`. The spec's §"Schedule / trigger" cites this name; if it's renamed in code before implementation work starts, the spec's anchor needs an update.
- **The eighteen-class divergence taxonomy is closed but extensible.** The spec asserts "closed set" for clarity, but the failure-and-safety table also names a fallback ("Probe attempted on a state class with no canonical digest form") whose disposition is to track extending the state-class set as a follow-up. If reviewers prefer "open with rules for extension" over "closed with extension protocol," the wording can be adjusted in a follow-up commit.
- **`#1799` acceptance criteria.** The spec satisfies criterion 1 ("Network proof-loop design doc or test plan merged") on merge, criterion 4 ("Dashboard/member status mapping is defined") via §"Steward cockpit surface" + §"Member shell surface", and offers a credible interpretation of criterion 6 ("first safe proof-loop or dogfood slice") via the three fixture slices. Criteria 2 ("proof scenarios classified by proof level using #1796 taxonomy") and 3 ("each scenario identifies required source paths/endpoints/tests and current gaps") are partially addressed via the §"Existing code surface" table and the §"First safe proof-loop / dogfood slice" section, but `#1796`'s taxonomy is itself forward work; full classification awaits #1796. Criterion 5 ("Follow-up implementation/test issues are opened only after the proof plan is accepted") is honored — this PR drafts follow-ups in the handoff but does not file them. **Closure of `#1799` is left to the user.**

---

## Validation Results
<!-- TRUTH TYPE: Verification — exact commands run and outcomes -->

```bash
cd /home/matt/projects/icn

# Doc control plane (strict)
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md
# OK: doc control check passed (812 docs markdown files under docs/); 54 enforcement warnings
# 54 yaml-mismatch warnings are pre-existing; none introduced by this PR.

# Architecture vocabulary lint
python3 docs/scripts/lint-arch.py docs/spec/network-anti-entropy-proof-loops.md --cargo icn/Cargo.toml
# Expected: 0 errors; some "soft-forbidden" warnings in negation context (vocabulary boundary, non-claims).

# Regulatory compliance linter
python3 .github/scripts/compliance_linter.py
# Expected: ✅ No compliance violations detected!

# Freshness check (pre-existing stale sections unrelated to this PR)
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
# Expected: pre-existing stale sections (06, 10, 11, 12, 14). None introduced by this PR.

# Targeted vocabulary check
rg -n "payment|currency|balance|wallet|token|blockchain|crypto|NYCN|Summit|live federation|production-ready|production readiness" \
   docs/spec/network-anti-entropy-proof-loops.md docs/dev/handoff-2026-05-15-network-anti-entropy-proof-loops.md docs/INDEX.md docs/registry.toml
# Expected: only matches in explicit negation context (Boundary rules, Privacy / custody rules, Non-claims).

# Cross-link smoke check
rg -o "docs/[a-zA-Z0-9_/\-]+\.md" docs/spec/network-anti-entropy-proof-loops.md | sort -u | while read p; do test -f "$p" && echo OK $p || echo MISSING $p; done
# Expected: every link resolves.
```

(Validation outputs are recorded against the initial commit SHA; the branch head moves with each push to address review feedback.)

---

## Follow-up Issue Drafts (Not Filed)

Eight follow-ups drafted per the user's instruction. Paste-ready for separate review.

### 1. `schema(network): define AntiEntropyProbe and StateDigest records`

```
## Purpose

Land wire-stable record shapes for `AntiEntropyProbe` and the family of `StateDigest` forms named in `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts (forward-direction names)" — including `ReceiptDigest`, `ArtifactDigest`, and the Bloom-filter / Merkle-root / vector-clock projections.

## Scope

- Probe record shape (state class, target scope, bounded digest field, prober DID and signature, trigger source, freshness timestamp, freshness validity window, requested response class).
- StateDigest projections (Bloom filter, Merkle root, vector clock, short digest list) with explicit serialization.
- Round-trip tests.
- Cross-link to existing `icn-gossip` BloomFilter and the per-topic Bloom-filter sync primitive in `anti_entropy.rs`.

## Non-goals

- No protocol mutation. Existing gossip topic strings remain.
- No new receipt class. The probe is an envelope, not a receipt.
- No live federation rollout.

## Related

- `docs/spec/network-anti-entropy-proof-loops.md` §"Probe" and §"Proof artifacts (forward-direction names)."
- `icn/crates/icn-gossip/src/anti_entropy.rs`, `bloom.rs`.
```

### 2. `schema(network): define DivergenceEvidence and RepairPlan records`

```
## Purpose

Wire-stable record shapes for `DivergenceEvidence` and `RepairPlan` per `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts." Includes the eighteen-class divergence taxonomy as a closed enum (with the "unclassifiable" fallback).

## Scope

- DivergenceEvidence shape (class, affected state class, scope, peers, digest forms, policy clause, freshness window, private-content implication flag).
- RepairPlan shape (action, authority basis, scope, boundary rules, expected RepairReceipt class).
- Round-trip tests; ADR-0026 amendment if needed for any new evidence layer.

## Non-goals

- No automatic-repair implementation.
- No governance review surface (separate concern).

## Related

- `docs/spec/network-anti-entropy-proof-loops.md` §"Classify" and §"Plan."
- ADR-0026 §"Layer 2 / Stage 5 evidence."
```

### 3. `test(devnet): add receipt-index anti-entropy fixture`

```
## Purpose

Implement Slice A from `docs/spec/network-anti-entropy-proof-loops.md` §"First safe proof-loop / dogfood slice": a fixture-only read-only receipt-index anti-entropy rehearsal across two or three peer fixtures with no live network and no real artifacts.

## Scope

- Fixture peers (DIDs assigned; signing keys local; receipt index over fixture-only public receipts).
- Probe / compare / classify / plan / apply / evidence / surface pass.
- DivergenceEvidence of class "missing receipt" produced and resolved.
- Fixture cockpit and member-shell views render the open-then-resolved divergence.

## Non-goals

- No live network. No private artifacts. No runtime mutation.
- Not a chaos test (that's `#1010`).

## Related

- `docs/spec/network-anti-entropy-proof-loops.md` §"First safe proof-loop / dogfood slice" → Slice A.
```

### 4. `spec(steward-cockpit): define sync degradation and repair surface`

```
## Purpose

Define the cockpit-side rendering contract for `DivergenceEvidence`, `RepairPlan`, `RepairReceipt`, and `SyncDegradedStatus` per `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface."

## Scope

- Cockpit field set (affected scope, state class, peers, digest mismatch, last successful proof, repair plan, authority required, receipts/evidence, escalation status).
- Cockpit refresh cadence.
- Cockpit access control (only stewards with the relevant authority class see private-implication evidence).

## Non-goals

- No member-shell content (separate spec).
- No automatic repair (separate concern).

## Related

- `#1795` (steward cockpit dashboard).
- `docs/spec/network-anti-entropy-proof-loops.md` §"Steward cockpit surface."
```

### 5. `spec(member-shell): define sync status rendering`

```
## Purpose

Define the member-shell-side rendering contract for the seven plain-language status strings named in `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface."

## Scope

- The seven closed strings.
- The transitions between them (Synced → Sync delayed → Some records are being verified → Action paused until records sync → Receipt available; or Review required as terminal).
- Boundary rule 8 enforcement at the rendering layer (no member-facing lie).

## Non-goals

- No raw protocol surface in the shell.
- No cockpit content.

## Related

- `#1818` (member shell v0).
- `docs/spec/network-anti-entropy-proof-loops.md` §"Member shell surface" and Boundary rule 8.
```

### 6. `spec(federation): define quorum sync window for federation-bound placement`

```
## Purpose

Define the cross-link between `QuorumSyncCheck` / `FederationSyncWindow` (per `docs/spec/network-anti-entropy-proof-loops.md`) and the federation-scope fail-closed gates in `docs/spec/compute-placement-policy.md` §"Boundary rules" 4 and 6.

## Scope

- The protocol for assembling a quorum of named federation peers for a state-class digest exchange.
- Per-state-class freshness windows (settlement records may require a stricter window than artifact-registry metadata).
- The interaction with the federation-settlement-finality dispute window (per `docs/spec/federation-settlement-finality.md`).

## Non-goals

- No new federation agreement schema.
- No new ADR.

## Related

- `docs/spec/network-anti-entropy-proof-loops.md` §"Routing and redundancy checks," Boundary rules 6 + 7.
- `docs/spec/compute-placement-policy.md` §"Boundary rules" 4 + 6.
- `docs/spec/federation-settlement-finality.md`.
```

### 7. `spec(storage): connect replication repair receipts to StorageSpec / RecoveryPolicy`

```
## Purpose

Close the cross-link between `docs/spec/storage-durability-policies.md` policy objects (`StorageSpec`, `RecoveryPolicy`, `IntegrityPolicy`) and the `RepairReceipt` / `RedundancyProof` artifacts named in `docs/spec/network-anti-entropy-proof-loops.md`.

## Scope

- How a `BackupPolicy` retry path produces a `RepairReceipt` of which class.
- How an `IntegrityPolicy` verification failure produces a `DivergenceEvidence` of class "integrity policy violation."
- How a missed `RecoveryPolicy` restore-test cadence escalates without auto-repair.

## Non-goals

- No backup-engine implementation.
- No restore-engine implementation.

## Related

- `docs/spec/storage-durability-policies.md`.
- `docs/spec/network-anti-entropy-proof-loops.md` §"Divergence classes" 5–8 and 17.
```

### 8. `spec(privacy): define private-object digest proof without content disclosure`

```
## Purpose

Formal contract for divergence class 16 in `docs/spec/network-anti-entropy-proof-loops.md` ("private object reference mismatch without content disclosure"). Defines how a probe can prove that a scoped-vault reference's `ArtifactDigest` disagrees between peers without exposing the artifact body or its disclosure-sensitive metadata.

## Scope

- The shape of an `ArtifactDigest` over a scoped-vault object (e.g., HMAC over a content-hash plus a domain-scoped salt).
- The cockpit / member-shell rendering rule (existence + scope + review path, never content).
- The interaction with `#1767`'s encrypted private overlay scheme.

## Non-goals

- No encryption-scheme implementation.
- No `PrivacyClass` taxonomy migration (tracked in `#1792`).

## Related

- `#1792` (private data disclosure boundary).
- `#1767` (encrypted distributed private-overlay storage).
- `docs/spec/network-anti-entropy-proof-loops.md` §"Privacy and custody rules" and divergence class 16.
```

---

## Architectural Decisions
<!-- TRUTH TYPE: Decision truth — choices made and why -->

### 1. The proof-loop layer is an app-side concept, not a kernel-side one.

`AntiEntropyProbe`, `DivergenceEvidence`, `RepairPlan`, `RepairReceipt` are policy-oracle outputs (per `docs/architecture/KERNEL_APP_SEPARATION.md`). The kernel runs the gossip / Bloom-filter / vector-clock primitives blindly; the policy oracle decides what classification the comparison gets, what plan is authorized, and what evidence is recorded. This preserves the meaning firewall.

### 2. No new receipt class.

All artifacts ride inside existing receipt envelopes: Stage 5 `EffectDispatchEvidence` (per `docs/spec/effect-dispatch-contract.md`) for divergence / plan / evidence artifacts, or Layer 2 `ArtifactReceipt` (per ADR-0026) for blob-transfer repair. This matches the pattern from #1826 (compute placement, also no new receipt class).

### 3. The divergence taxonomy is closed with an explicit fallback.

Eighteen classes including "unclassifiable" as the explicit escape valve. The failure-and-safety table names the disposition for a probe targeting a state class without a canonical digest form (track as a follow-up to extend the state-class set). This avoids both unbounded open enums and silent failure on novel divergence shapes.

### 4. The spec consumes existing primitives without redefining them.

The existing `icn-gossip` and `icn-core` anti-entropy code is real and load-bearing. The spec names it in an anchors-only table at the top of the doc, then builds the design-level artifact layer above it. This prevents the spec from being treated as a rewrite request for working code.

### 5. The settlement-finality cross-link is explicit.

Boundary rule 7 names that the receipt-chain-complete condition in `docs/spec/federation-settlement-finality.md` is evaluated against a fresh `QuorumSyncCheck`. This closes the load-bearing gap that compute placement's federation gate (Boundary rules 4 + 6 in `docs/spec/compute-placement-policy.md`) implicitly depended on.

### 6. Privacy / custody rules are restated separately.

They appear in §"Privacy and custody rules" (their own section) and again in Boundary rules 3 + 4 and in the failure-and-safety table. Restating is intentional: this is the most easily misread constraint, and reviewers can grep for "private" or "disclosure" and find the rule in three places.

### 7. The PR uses `Refs:`, not `Closes:`.

Per the standing session pattern and the `feedback_pr_refs_not_closes_unless_fully_satisfied` memory: closure of `#1799` requires verification that all seven acceptance criteria are fully satisfied. The spec advances criterion 1 (the spec is merged), criterion 4 (status mapping), and criterion 6 (first safe slice); it partially addresses criteria 2 and 3 (proof-level classification awaits `#1796`); criterion 5 (follow-ups only after acceptance) is honored by drafting in the handoff. **Closure is a user-driven decision.**

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn

git checkout spec/network-anti-entropy-proof-loops
git status --short
gh pr view <PR-number> --json mergeStateStatus,state,headRefOid,statusCheckRollup

# To re-run the validation suite:
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict --write-document-registry docs/DOCUMENT_REGISTRY.md
python3 docs/scripts/lint-arch.py docs/spec/network-anti-entropy-proof-loops.md --cargo icn/Cargo.toml
python3 .github/scripts/compliance_linter.py
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .

# To see the spec contents:
sed -n '1,80p' docs/spec/network-anti-entropy-proof-loops.md
```

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth:** loaded from `docs/PHASE_HISTORY.md` (Phase 18 last completed; ~75% implementation as of 2025-12-03 K3s deploy) and `docs/STATE.md` (current state). No conflict with this PR; the spec is doc-only design work.
- **Implementation truth:** verified against `icn/crates/icn-gossip/src/anti_entropy.rs`, `bloom.rs`, `vector_clock.rs`, `partition.rs`, `sync.rs`; `icn/crates/icn-core/src/anti_entropy.rs`; `icn/crates/icn-governance/src/protocol_defaults.rs`. The existing-code-surface table reflects what is actually in `main` at session start.
- **Execution truth:** verified branch + PR state via `gh` commands. Initial commit SHA, PR number, head SHA recorded after commit / push.
- **Narrative truth:** loaded from prior session handoffs and the eight merged architecture-spec PRs. No conflict.
- **Known conflicts between layers:** none introduced by this PR. The `DataLocality::CoopReplicated` Rust enum still uses `Coop` framing while the spec layer prefers `LocalDomain` framing — preserved per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 and tracked as a follow-up across the sprint.

---

## Process Note

The AGENTS.md handoff-path drift was resolved by **PR #1827** (merged 2026-05-15). The active convention is `docs/dev/handoff-YYYY-MM-DD-<topic>.md`, which is what this handoff uses. **Do not carry the old AGENTS.md drift forward** in future handoffs unless an AI reviewer surfaces it again, in which case respond with a verified rebuttal that points at #1827.
