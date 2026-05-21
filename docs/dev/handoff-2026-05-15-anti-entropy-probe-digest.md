# Session Handoff — 2026-05-15

## Session Goal
Land wire-stable Rust shapes for `AntiEntropyProbe` and the `StateDigest` family (issue #1834), without protocol mutation, without new ADR-0026 receipt classes, and without expanding into #1835 / #1838 scope.

## Decisive Test
After this PR, a future implementer of `#1838` (the Slice A receipt-index anti-entropy fixture) must be able to construct a probe and a digest, sign them, hand them across a peer boundary, and deserialize them deterministically — without guessing the shape of either record. The kernel must not have imported `icn-gossip`, `icn-trust`, or any domain crate to make that possible.

---

## Final State (Verified)
<!-- TRUTH TYPE: Implementation truth — only facts confirmed by commands or code inspection -->

### `main` HEAD
`d9035c792 fix(state): correct state-sync drift after #1841 review (#1842)`

### Branch
- `schema/network-anti-entropy-probe-digest` — head matches `origin/main` at session start; five files modified, not yet committed at the time this handoff was drafted (commit & PR are the immediate next move — see "Next Move").

### Open PRs at session start
Only Dependabot #1790, #1791 — not blocking.

---

## What Changed
<!-- TRUTH TYPE: Execution truth — actions taken this session -->

### 1. New kernel-API records in `icn-kernel-api/src/proofs.rs` (+993 lines)
Introduced wire-stable Rust shapes for the design-level identifiers named in `docs/spec/network-anti-entropy-proof-loops.md` §"Proof artifacts (forward-direction names)":

- `AntiEntropyProbe` — probe envelope with `schema_version` (u32, currently `ANTI_ENTROPY_PROBE_SCHEMA_VERSION = 1`), `state_class`, `target_scope`, `digest`, `prober_did`, `trigger_source`, `freshness_emitted_at`, `freshness_valid_until`, `requested_response`, `probe_nonce` ([u8; 32]), `probe_hash` (self-authenticating blake3 binding), and `signature` (empty until signed by a higher layer).
- `StateDigest` — externally-tagged enum over four projections:
  - `Bloom(BloomProjection)` — `{bits, num_hashes, size, hint_count}` — wire-equivalent to `icn_gossip::types::BloomFilterData` on the first three fields.
  - `MerkleRoot(MerkleRootProjection)` — `{root: [u8; 32], leaf_count: u32}`.
  - `VectorClock(VectorClockProjection)` — `Vec<(Did, u64)>` sorted by DID at construction; matches `merge` semantics (max count on duplicate DID).
  - `ShortList(ShortDigestList)` — `Vec<Hash>` sorted lexicographically and deduplicated at construction.
- `ReceiptDigest(StateDigest)` — newtype specialization binding `StateClass::ReceiptIndex`.
- `ArtifactDigest` — **closed enum** with `Registry(StateDigest)` and `ScopedVaultReference(StateDigest)` variants (revised after review; see "Review feedback applied" below). State-class specialization is now enforced by the type system; no construction path can produce an invalid state class.
- `StateClass` — closed enum, nine variants matching the spec table (snake_case serde).
- `ProbeScope` — closed enum: `LocalDomain { domain_id }`, `Federation { federation_id }`, `Commons`, `PeerPair { left, right }`. Uses generic vocabulary per `docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md` §C3 (no `Coop`-prefixed names introduced).
- `TriggerSource` — closed enum, five variants from spec §"Schedule / trigger".
- `RequestedResponseClass` — closed enum: `DigestExchange | FetchMissing | RepairAuthorization`.

Self-authentication pattern follows `ArtifactReceipt`: `DOMAIN_TAG = b"icn:anti-entropy-probe:v1"`, blake3 over the domain tag + length-prefixed bincode encoding of a private `ProbeBinding<'a>` struct that covers every bound field except `probe_hash` and `signature`. `verify_binding()` recomputes and compares.

### 2. New `lib.rs` re-exports in `icn-kernel-api`
`AntiEntropyProbe`, `ArtifactDigest`, `BloomProjection`, `MerkleRootProjection`, `ProbeScope`, `ReceiptDigest`, `RequestedResponseClass`, `ShortDigestList`, `StateClass`, `StateDigest`, `TriggerSource`, `VectorClockProjection`, and the `ANTI_ENTROPY_PROBE_SCHEMA_VERSION` constant.

### 3. Cross-link helpers in `icn-gossip/src/anti_entropy.rs` (+113 lines)
Two free functions and three round-trip tests:

- `to_bloom_projection(data: &BloomFilterData, hint_count: u32) -> BloomProjection`
- `to_bloom_filter_data(proj: &BloomProjection) -> BloomFilterData`

Re-exported from `icn-gossip/src/lib.rs`. The kernel→gossip direction is provided as a free function (not a `From` impl) because Rust's orphan rule forbids `impl From<&BloomFilterData> for BloomProjection` when both types live in foreign crates relative to the impl site.

Tests verify:
- byte-for-byte preservation of `bits`, `num_hashes`, `size`;
- preservation of Bloom-filter set membership for an adaptive 4-item filter;
- `hint_count` is supplied by the caller because `BloomFilterData` does not carry it.

### 4. Doc update — `docs/spec/network-anti-entropy-proof-loops.md` (+12 / -3 lines)
Replaced the "None lands as a Rust type in this PR" sentence (which became false the moment this PR landed) with a paragraph naming the wire shapes that now exist in `icn-kernel-api`. Marked the four affected forward-direction names ("`AntiEntropyProbe`", "`StateDigest`", "`ReceiptDigest`", "`ArtifactDigest`") as **Wire-stable** with `#1834`. The remaining nine identifiers stay as design-level names; `DivergenceEvidence` / `RepairPlan` / `RepairReceipt` are flagged for `#1835`, the fixture loop for `#1838`.

### 5. Test coverage added
- 28 new unit tests inside `anti_entropy_tests` in `proofs.rs`: binding determinism, tamper detection on every field, domain-tag separation, nonce-driven hash distinctness, freshness helper, bincode + JSON round-trip on `AntiEntropyProbe`, round-trip on every `StateDigest` projection, vector-clock canonicalization (sort by DID, max-count on duplicate, order-independent serialization), short-list dedup + sort, serde tag style assertions, and cross-protocol domain-tag distinctness vs `ArtifactReceipt::DOMAIN_TAG`.
- 3 new cross-link tests in `icn-gossip` covering bit-level round-trip, membership round-trip, and the `hint_count` semantics.

---

## What's Open
<!-- TRUTH TYPE: Execution truth — known incomplete work -->

- [ ] Commit, push, and open the PR against `main` with `Closes #1834` (acceptance criteria are met — see "Mapping to #1834 scope" in the PR body below).
- [ ] Retarget Dependabot PRs is **not** in scope here; they target `main` and do not stack on this branch.
- [ ] `#1835` (`DivergenceEvidence` / `RepairPlan` records) — left untouched on purpose; would have expanded the PR.
- [ ] `#1838` (Slice A fixture loop) — left untouched on purpose. The wire shapes added here make it implementable without further design work.
- [ ] `lint-arch.py` reports 3 pre-existing warnings at `docs/spec/network-anti-entropy-proof-loops.md:195` for `payment` / `wallet` / `currency`. That line is in the existing spec text I did not edit, inside an explicit negation context ("No payment / wallet / currency framing in any probe field"). The regex linter does not parse negation. Not introduced by this PR; not fixed by this PR.

---

## Unsafe Assumptions
<!-- Explicitly name anything this session relied on that was not verified -->

- I did not verify that an SDK / OpenAPI consumer expects these types via the gateway API — they are kernel-internal records right now, not exposed via REST. No OpenAPI regen ran because nothing on the gateway surface changed. If a future consumer wants them on the API, that exposure is forward work.
- I assumed bincode v1.3 will continue to be the canonical-serialization helper for kernel-api records (matching the existing `compute_canonical_hash` pattern in `receipts.rs:100`). Bincode v1 is deterministic for the structs introduced here. Should the workspace migrate to bincode v2 or postcard for kernel-api canonical encoding, `compute_probe_hash` will need to be re-validated.
- I assumed that internally-tagged serde enums (`#[serde(tag = "form")]`) are not required for the wire format. I switched to externally-tagged because bincode v1 does not support `deserialize_any`. The JSON wire form is now `{"bloom": {...}}` rather than `{"form": "bloom", "bits": [...], ...}`. Future gateway consumers that prefer flattened tagging will need to convert at the gateway boundary, not in the kernel record.
- I assumed `docs/registry.toml` does not need a description bump for this small forward-pointer edit. `last_updated = "2026-05-15"` already matches today; the description text remains accurate for what the spec doc itself contains. If a reviewer disagrees, the fix is one-line.

---

## Review Feedback Applied (second commit in this PR)

Copilot and Codex flagged four issues against the initial commit `dbde61a4`. A follow-up commit on the same branch addressed all four:

1. **Rustdoc path in `proofs.rs`** — original wording pointed callers to `icn_gossip::anti_entropy::to_bloom_*`, but `anti_entropy` is a private module. Fixed to `icn_gossip::to_bloom_projection` / `icn_gossip::to_bloom_filter_data` (re-exported from the crate root). No module visibility change in `icn-gossip`.
2. **Spec path** — same correction in `docs/spec/network-anti-entropy-proof-loops.md`.
3. **Scope/non-goals reconciliation** — the spec's Scope/non-goals block still asserted "Implementation of any of the new artifact names. None of them lands as Rust code in this PR." That sentence was true for PR #1829 but false the moment PR #1843 landed. Replaced with a framing that distinguishes the original spec PR scope from the now-wire-stable subset (`AntiEntropyProbe` + `StateDigest` family) and the remaining design-level names (`PeerSyncReport`, `DivergenceEvidence`, etc.). The "Not in scope" list now explicitly names what `#1835` and `#1838` cover.
4. **Canonical deserialization** — the **real bug** Copilot caught. Derived `Deserialize` bypassed the constructor invariants on three types:
   - `VectorClockProjection` — `entries` was public; derived deserialization accepted unsorted DIDs or duplicates. Fixed via `#[serde(from = "RawVectorClockProjection")]`: the field is now private, the wire form deserializes into a private raw struct, and the `From` impl normalizes through `Self::from_entries`. Added `entries(&self) -> &[(Did, u64)]` accessor.
   - `ShortDigestList` — same pattern: `hashes` made private, `#[serde(from = "RawShortDigestList")]` normalizes through `from_hashes`, accessor added.
   - `ArtifactDigest` — refactored from a two-field struct to a **closed enum** with `Registry(StateDigest)` and `ScopedVaultReference(StateDigest)` variants. Now impossible to deserialize an `ArtifactDigest` tagged with `ReceiptIndex` / `GovernanceState` / etc. — the variants are exhaustive and the wire form is tagged by variant. Constructors `registry()` / `scoped_vault_reference()` and accessors `state_class()` / `digest()` preserved.

Added 7 new tests covering: unsorted/duplicate JSON input normalizes for `VectorClockProjection` and `ShortDigestList`; bincode wire input takes the same path; invalid state-class tags rejected for `ArtifactDigest`; unknown variant tags rejected. Existing tests updated to use the new accessor methods.

The two suppressed low-confidence Copilot comments were the same canonicalization issue restated for `ShortDigestList` and `ArtifactDigest`; this commit covers both.

Validation re-run after the review-fix commit:
- `cargo fmt --check`, `cargo clippy -- -D warnings` on both crates: ✓
- `cargo test -p icn-kernel-api --lib`: 343 passed (7 more than the original 336).
- `cargo test -p icn-gossip --lib`: 220 passed.
- `cargo test -p icn-gossip --test '*'`: all passing.
- `cargo check --workspace`: ✓ (the `ArtifactDigest` field→variant refactor caused no downstream breakage; no external consumers existed yet).
- Doc validators: same 54 pre-existing warnings, same 3 pre-existing `lint-arch.py` negation-context warnings (shifted from line 195 → 199 because the Scope/non-goals edit added lines above them).
- Compliance linter: clean.

**Security Audit CI failure is pre-existing on `main`.** The advisory is RUSTSEC-2026-0141 (`lettre 0.11.19` in `icn-gateway`, published 2026-05-14, one day before this PR). This PR does not modify `Cargo.lock`, does not touch `lettre`, does not touch `icn-gateway`. Out of scope for `#1834`.

---

## Next Move
<!-- TRUTH TYPE: Execution truth — the exact sequence for the next session -->

1. From `/home/matt/projects/icn` on branch `schema/network-anti-entropy-probe-digest`:
   ```bash
   git add docs/spec/network-anti-entropy-proof-loops.md \
           icn/crates/icn-gossip/src/anti_entropy.rs \
           icn/crates/icn-gossip/src/lib.rs \
           icn/crates/icn-kernel-api/src/lib.rs \
           icn/crates/icn-kernel-api/src/proofs.rs
   git commit -m "schema(network): define AntiEntropyProbe and StateDigest records"
   ```
2. Push via `/push` (runs `cargo fmt --check` + `cargo clippy` gates first), then open the PR with `Closes #1834` and the body drafted below.
3. After AI reviewers (Copilot / Codex) post, address surgical drift only — do not broaden scope to claim `#1835` or `#1838` work.
4. Next discrete unit is **#1835**: `DivergenceEvidence` + `RepairPlan` records, following the same record pattern (domain tag, blake3 binding, bincode-compatible enum tagging). The 18-class divergence taxonomy from spec §"Divergence classes" becomes a closed enum.
5. After #1835 lands, **#1838**: build the Slice A fixture loop using the now-existing record shapes.

---

## Architectural Decisions
<!-- TRUTH TYPE: Declared project truth changes -->

1. **Probe is a kernel record, not an app record.** The probe message itself is content-agnostic and travels through gossip; classification (the policy-oracle phase) and repair planning live in apps. So `AntiEntropyProbe` belongs in `icn-kernel-api/src/proofs.rs`, alongside `ArtifactReceipt`. This is consistent with `docs/architecture/KERNEL_APP_SEPARATION.md` — the kernel runs the probe blindly; the policy oracle classifies the result.
2. **No new ADR-0026 receipt class.** The probe is an evidence envelope, not a receipt. The spec said so; this PR honors it. Re-validated by reading ADR-0026 directly — its four layers (`GovernanceProof`, `ArtifactReceipt`, `FederationProvenance`, `ProvenanceQuery`) carry classified receipts; the probe is *upstream* of any classification.
3. **Externally-tagged enums for canonical encoding.** Bincode v1 cannot deserialize internally-tagged enums (`DeserializeAnyNotSupported`). Externally-tagged is the serde default, is bincode-compatible, and matches the existing kernel-api enum pattern (e.g., `ScopeLevel`, `ConstraintValue`). The JSON form is slightly less self-describing but is the price of single-format canonicalization.
4. **Bloom cross-link via free functions, not `From` impls.** Rust orphan rule prevents `impl From<&BloomFilterData> for BloomProjection` in `icn-gossip`. Free functions name the direction explicitly and let `hint_count` (which `BloomFilterData` doesn't carry) be a first-class parameter rather than a hidden default.
5. **Sort-at-construction for `VectorClockProjection` and `ShortDigestList`.** Canonical encoding requires deterministic field order. `BTreeMap` (vector-clock) and `Vec::sort + dedup` (short-list) at construction is cheaper than serializing through a custom `Serialize` impl, and makes the invariant directly testable.

---

## Verification Commands
<!-- Commands the next session should run to confirm starting state -->

```bash
cd /home/matt/projects/icn/icn
cargo fmt -p icn-kernel-api -p icn-gossip --check
cargo clippy -p icn-kernel-api --all-targets --all-features -- -D warnings
cargo clippy -p icn-gossip --all-targets --all-features -- -D warnings
cargo test -p icn-kernel-api --lib
cargo test -p icn-gossip --lib
cargo check --workspace

cd /home/matt/projects/icn
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
python3 docs/scripts/lint-arch.py docs/spec/network-anti-entropy-proof-loops.md
python3 .github/scripts/compliance_linter.py
```

All passed in this session except the 3 pre-existing `lint-arch.py` warnings on line 195 noted above (negation context the regex linter does not parse).

---

## Truth-Plane Notes
<!-- Which truth types were relied on? Any known conflicts between layers? -->

- **Declared project truth (STATE.md / PHASE_PROGRESS.md):** Read for orientation; not modified in this PR. The spec doc was the load-bearing canonical surface for `#1834` scope.
- **Implementation truth:** Verified by reading `icn-kernel-api/src/proofs.rs` (existing `ArtifactReceipt` pattern + `compute_canonical_hash` in `receipts.rs:100`), `icn-gossip/src/{bloom,vector_clock,anti_entropy,types}.rs` (cross-link anchors), and `icn-encoding/src/lib.rs` (postcard wire encoding; bincode chosen for kernel-api canonical hashing to match `receipts.rs`).
- **Execution truth:** Branch and CI state confirmed clean before edits. `gh auth status` confirmed authenticated as `fahertym`. No live infrastructure touched.
- **Narrative truth:** None claimed. No production-readiness claim. No live-federation claim. No NYCN pilot claim. All four explicitly carried forward in the spec doc's existing "Non-claims" block.
- **Known conflicts:** None discovered between declared / implementation / execution truth. The spec doc said "None lands as a Rust type in this PR" — that sentence was tied to PR #1829 and is now adjusted for this PR.
