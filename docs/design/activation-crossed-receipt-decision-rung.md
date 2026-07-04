# ActivationCrossedReceipt decision rung — B1/B2/B3 (decision → activation reference, gate basis, timestamp)

**Status:** draft — design / decision rung (not runtime implementation)
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `a170d8e7` (the merged #2294 contract's tip). Code anchors (`icn/crates/icn-governance/src/proof.rs`, `icn/crates/icn-gateway/src/receipt_store.rs`, `icn/crates/icn-coop/src/types.rs`) were verified at that commit — re-verify before relying on exact line numbers or hashes; they drift.
**Related:** #2293 (this rung's issue) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2041 (human/AT pass — open/parked) · PR #2294 (merged design/audit contract, [`docs/design/activation-crossed-receipt-runtime-dogfood.md`](activation-crossed-receipt-runtime-dogfood.md)) · PR #2278 / #2281 (the sibling decision-doc pattern: [`deliberation-entry-kind-taxonomy.md`](deliberation-entry-kind-taxonomy.md), [`decision-recorded-q4-decision.md`](decision-recorded-q4-decision.md)) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope) · `ops/ideas/framing/institutional-process-substrate.md` (framing)

> Narrow decision document resolving the three implementation blockers the merged
> #2294 design contract named in its §14 — **B1** (decision → activation
> cross-receipt reference), **B2** (gate-basis representation and whether a new
> `ProcessGateKind` variant is required), and **B3** (`crossed_at` vs
> `effective_at` timestamp semantics). It mirrors the `decision-recorded-q4-decision.md`
> cadence: land the contract (#2294), then resolve the hash-participating
> structure **in writing** before a tag is pinned, then implement. This document
> decides nothing else: no runtime change, no receipt class added, no mutation
> planning, no evidence-packet production, no member-shell change, no human/AT run.
> Receipts record institutional facts. They grant zero authority.

## 1. Purpose

The #2294 design contract scoped a candidate `ActivationCrossedReceipt` — the fifth `ProcessTransitionReceipt` rung under #1748 / #2141 that would witness that an already-recorded decision **crossed the activation boundary** (the spine's "boundary between deciding and doing"), with the required gates observed as passed, before any later mutation/evidence work. The contract deliberately refused to pin the candidate `icn:gov:activation_crossed:v1` layout and blocked implementation on three questions whose answers change the canonical hash layout or the class's inter-receipt relationships:

- **B1** — does the receipt name the decision it activates, and by what?
- **B2** — how does the receipt witness "required gates observed as passed," and is a new gate kind required?
- **B3** — what is the activation timestamp source, and how does it stay deterministic?

The landed rule (from the #2278 review cycle, restated by the #2281 Q4 decision) is that **hash-participating structure is decided in writing before a tag is pinned, never silently in an implementation PR.** This document resolves B1/B2/B3 so a contract-conformant implementation PR can begin. It is not a workflow engine, not a policy engine, not mutation planning, and not evidence-packet production.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `a170d8e7`):

- **#2294** — `ActivationCrossedReceipt` design/audit contract — **landed** (merged `a170d8e7`).
- **#2293** — `ActivationCrossedReceipt` runtime dogfood slice — **open**; implementation has not started.
- **#1748 / #2141** — Institutional Process Substrate milestone / vertical spine — **open**.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.
- `ActivationCrossedReceipt` **is not implemented** — no Rust struct, tag, manager method, backend class constant, route, or test exists anywhere in `icn/crates/` (confirmed by live audit).
- `ActivationRequest` / "activation boundary" remain **framing / doc-only**; the live audit found no runtime gate object or activation seam beyond this design lane's own docs.
- `MutationPlanRecordedReceipt` / `MutationAppliedReceipt` / `EvidencePacketProducedReceipt` remain **deferred** (docs/framing only; no Rust).

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Repo audit update (verified against live code)

Confirming the #2294 audit against `origin/main` @ `a170d8e7` — the facts B1/B2/B3 depend on:

| Subject | Finding | Anchor |
|---------|---------|--------|
| `DecisionRecordedReceipt` | fields `domain_id, session_id, decision_id, recorded_by, recorded_at, body_hash, record_hash`; tag `icn:gov:decision_recorded:v1`; stable duplicate identity = `(domain_id, session_id, decision_id, recorded_by, body_hash)`; `recorded_at` **excluded** from identity (retry never restamps) | `proof.rs` (`DecisionRecordedReceipt`, `DOMAIN_TAG`, `compute_record_hash`) |
| `ProcessGateResultReceipt` | fields `session_id, domain_id, gate_kind: ProcessGateKind, result: ProcessGateResult, recorded_by, recorded_at, record_hash`; tag `icn:gov:process_gate_result:v1`; `result` is a two-variant `Pass` / `Fail` enum; `record_hash` (the sole equality anchor) is computed over **all** fields **including `recorded_at`**, while the stable duplicate identity is the non-timestamp fields `(session_id, domain_id, gate_kind, result, recorded_by)` — `recorded_at` is hashed into `record_hash` but **excluded from identity** (retry never restamps), exactly as for `DecisionRecordedReceipt` | `proof.rs` (`ProcessGateResultReceipt`, `compute_record_hash`) |
| `ProcessGateKind` | **closed enum, six variants**: `PrivacyReview`(0), `AccessibilityReview`(1), `RepoSafetyReview`(2), `ScopeConfirmation`(3), `NoMutationCheck`(4), `SecondReviewerSignoff`(5); `Copy`, serde `snake_case`; **no `#[non_exhaustive]`, no catch-all**; ordinals are hard-mapped (`gate_kind_ordinal`) and **participate in the gate receipt's `record_hash`** | `proof.rs` (`ProcessGateKind`, `gate_kind_ordinal`) |
| cross-receipt references | **none exist**: no receipt struct carries a field that points at another receipt's `record_hash` or id. The only "ref" concepts (`MandateGrantRef` / grant-to-receipt binding in the v2/v3 proposal-vote lineage) are not receipt-to-receipt links. B1's link is genuinely the lane's **first** inter-receipt reference | whole-repo search |
| `effective_at` | **membership-actor lane only** — `MemberBuilder::with_joined_at_secs` (`icn-coop/src/types.rs`) and the `Add/Remove/Freeze/Unfreeze` requests (`icn-core/src/services/membership_service.rs`), plus kernel-api protocol-parameter scheduling. It is **not a field on any receipt.** `decided_at` / `crossed_at` do not exist anywhere | `icn-coop/src/types.rs`, `membership_service.rs` |
| `put_opaque_if_absent` | exists on the gateway `ReceiptStore` — signature `(class, key1, key2: Option<&str>, recorded_at, record_hash, payload) -> Result<Option<[u8;32]>, String>`; atomic sled transaction keyed on `(class, key1, key2)`; `None` ⇒ this write won, `Some(existing)` ⇒ return the original (never restamp); different payload for same `(class, record_hash)` ⇒ fail-closed. This is the idempotence mechanism a fifth class reuses | `receipt_store.rs` (`put_opaque_if_absent`) |

**Bottom line:** every #2294 audit claim that B1/B2/B3 rely on is accurate against live code. The four landed classes are the only runtime `ProcessTransitionReceipt`s; the activation rung remains seam-discovery work.

## 4. B1 decision — decision → activation cross-receipt reference

**Question.** Does `ActivationCrossedReceipt` name the `DecisionRecordedReceipt` it activates, and if so by `decision_id`, by decision `record_hash`, or both — while preserving ADR-0026 proof/envelope semantics, idempotence, and replay safety? This is the lane's first inter-receipt reference; no existing pattern supports it.

Options considered:

1. **No in-receipt reference** — rely on the shared `(domain_id, session_id)` session anchor; a read-model joins decision and activation by session. Rejected: it leaves the crossing's decision unbound. A session may record more than one decision; "which decision was activated" then has no cryptographic answer, only a temporal guess. It also fails the contract's own framing (§8: the crossing must *name* the decision it activates).
2. **`decision_id` only** — carry the caller-opaque per-decision id. Rejected as the *sole* link: `decision_id` is unique only within a session and is caller-opaque; it names *which slot* but does not bind to the decision's recorded **content**. A crossing could cite a `decision_id` whose body later differs from what was recorded, and nothing would detect it.
3. **decision `record_hash` only** — carry the 32-byte content-addressed `record_hash` of the `DecisionRecordedReceipt`. Strong cryptographic binding, but drops the human/index handle; joining back to the session's decision list by id becomes indirect.
4. **Both `decision_id` and decision `record_hash`. CHOSEN.**
5. Proof-envelope / signature reference. Rejected for `:v1`: the process-transition classes are ADR-0026 Layer-2 **self-hashed** (blake3 `record_hash`, no signature/merkle — §7 of the contract). There is no signed envelope to point at; inventing one is an ADR-0026 revision, out of scope.

**Decision B1: `:v1` carries a direct reference to the activated decision by *both* its caller-opaque `decision_id` and its content-addressed decision `record_hash`.** Candidate field names (subject to implementation proof and golden-vector pinning):

- `decision_id: String` — the decision being activated; the human/index handle, unique within the session.
- `decision_record_hash: Hash` — the 32-byte `record_hash` of that `DecisionRecordedReceipt`; the content-addressed proof link.

Binding consequences:

- **Both fields participate in the canonical `record_hash` and in stable duplicate identity.** A same-identity retry returns the original receipt un-restamped; a different `decision_record_hash` (or `decision_id`) for the same `activation_id` is a fail-closed conflict (`activation_crossed_conflict`), mirroring `decision_recorded_conflict`.
- **The reference is verified, not merely asserted (fail-closed precondition).** The implementation MUST require that a `DecisionRecordedReceipt` with exactly `decision_record_hash` exists in the **same** `(domain_id, session_id)` before the crossing is recorded. If it is absent — or present under a different session — the boundary is not crossed and **no receipt is emitted**, mirroring the session-open precondition of the landed classes. This is what makes the link a *proof* ("this decision was recorded and I crossed on it"), not a claim. It reads existing receipts; it does not evaluate or re-decide them.
- **ADR-0026 preserved.** The link points at the decision's own self-hashed `record_hash` — the decision's existing provenance pointer. `ActivationCrossedReceipt` inherits the *process-transition* discipline (self-contained blake3 `record_hash`, opaque-store persistence), and asserts **no** signed-envelope/merkle inheritance. Any future signature upgrade is an ADR-0026 revision, not a receipt rung.
- **Idempotence / replay.** Because `decision_record_hash` is content-addressed and deterministic (not wall-clock), two nodes replaying the same logical crossing derive the same activation identity and converge on the **original** receipt (original `recorded_at`, original `record_hash`) via `put_opaque_if_absent`. The cross-link is a hash, so it never introduces node-local nondeterminism.
- **Non-convergence.** `decision_record_hash` is an opaque 32-byte value; it carries no proposal/vote/tally/outcome semantics and creates no tie to the `icn:gov:decision:v1/v2/v3` proposal-vote lineage. The activation rung references the *process-spine* `DecisionRecordedReceipt`, not the outcome-machinery `GovernanceDecisionReceipt`.

**Test that proves it:** a runtime-slice test that (a) records a `DecisionRecordedReceipt`, then records an `ActivationCrossedReceipt` citing that decision's real `record_hash`, round-trips it, and asserts the stored `decision_record_hash` equals the decision receipt's `record_hash`; (b) an activation whose `decision_record_hash` names no decision in the session (or names a decision from a different session) is refused fail-closed and persists nothing; (c) a same-identity retry returns the original activation un-restamped; (d) a conflicting `decision_record_hash`/`decision_id` for the same `activation_id` is a fail-closed conflict.

## 5. B2 decision — gate-basis representation and `ProcessGateKind`

**Question.** How does the receipt witness "required gates observed as passed"? Is a new `ActivationRequest` gate object and/or a new `ProcessGateKind` variant required? Is gate-basis required, optional, or fixture-only for this first slice?

Two sub-decisions.

### 5.1 — No new `ProcessGateKind` variant; no new `ActivationRequest` gate object

Options considered:

1. **Reuse the existing closed six-variant `ProcessGateKind`; add nothing. CHOSEN.**
2. Add an `ActivationReadiness` (or similar) variant. Rejected: `ProcessGateKind` is a `Copy` enum with **no `#[non_exhaustive]`** whose ordinals feed the gate receipt's `record_hash`. A new variant is (a) a breaking change at every exhaustive match site and (b) an **ADR-controlled taxonomy change**, not a free append — exactly the boundary the contract §14 flagged. It also conflates *evaluating* a gate with *witnessing* a crossing.
3. Introduce a runtime `ActivationRequest` gate object. Rejected for this slice: it is a new primitive with its own authority/refusal semantics; the dogfood slice witnesses a crossing, it does not build a request/approval object. `ActivationRequest` stays framing-only.

**Decision B2a: activation crossing reuses the existing `ProcessGateKind` taxonomy unchanged.** The six variants (`PrivacyReview`, `AccessibilityReview`, `RepoSafetyReview`, `ScopeConfirmation`, `NoMutationCheck`, `SecondReviewerSignoff`) already express the gate semantics that matter at the boundary. The activation receipt **witnesses** that the relevant `ProcessGateResultReceipt`s were `Pass`; it does **not** evaluate gates, define which gates are *required*, or add a gate kind. Any future activation-specific gate kind is a separate ADR-controlled taxonomy change, never a silent `:v1` add.

### 5.2 — Gate basis is an explicit, verified, content-addressed fingerprint

Options considered:

1. **No in-receipt basis** — read-model joins gate receipts by session at read time. Rejected: the crossing then witnesses nothing about *which* gates justified it; "the gate can be refused" (contract §5) has no in-receipt anchor.
2. **A boolean "all required gates passed."** Rejected: it asserts a readiness claim the receipt cannot prove and smuggles a "required set" policy the receipt layer must not own. This is the mushy path the membership-determinism doctrine (#2283/#2284) warns against.
3. **A content fingerprint `gate_basis: Hash` over the sorted set of the passed `ProcessGateResultReceipt` `record_hash`es declared as the basis. CHOSEN.**
4. A stored variable-length vector of gate-result `record_hash`es. Rejected for `:v1`: heavier and variable-length in the hash layout for no benefit over a fingerprint; the individual gate receipts remain queryable by session.

**Decision B2b: `:v1` carries a `gate_basis: Hash` — a blake3 fingerprint over the sorted, de-duplicated `record_hash`es of the `ProcessGateResultReceipt`s the caller declares as the basis for this crossing.** Candidate name, subject to implementation proof and golden-vector pinning. It is the analog of `body_hash`: a fixed-32 content fingerprint that hashes cleanly, participates in the canonical `record_hash` and in stable duplicate identity, and binds the crossing to exactly the gate receipts that justified it — content-addressed and deterministic.

Binding consequences:

- **Required, non-empty, fail-closed (verified, not asserted).** For this slice a crossing MUST declare a non-empty basis, and the implementation MUST verify that every gate-result receipt whose `record_hash` is in the declared basis (i) exists in the **same** `(domain_id, session_id)` and (ii) carries `result == Pass`. If any declared gate result is absent or `Fail`, the boundary is not crossed and **no receipt is emitted** (contract §5 / §12.3). An empty basis is refused. This keeps "the gate can be refused" honest **without a policy engine**: the receipt layer verifies the *declared* basis; it does not decide which gates are *required* — that policy is charter/app-layer and deferred.
- **The firewall the contract §6 requires holds:** the receipt carries **no `ProcessGateKind`, no `result`, no gate evaluation** — only the fingerprint of the passed gate receipts. It witnesses that gates passed; it does not re-derive the passing.
- **Determinism.** `gate_basis` is a hash over content-addressed inputs, so it introduces no node-local nondeterminism; replay converges.

**Test that proves it:** (a) a crossing declaring a basis of two real `Pass` gate receipts in-session round-trips and its `gate_basis` equals the independently-recomputed fingerprint of those two `record_hash`es; (b) a crossing whose declared basis includes a `Fail` or absent gate result is refused fail-closed and persists nothing; (c) an empty basis is refused; (d) basis ordering does not matter (sorted before hashing) — the same set in any order yields the same `gate_basis`.

## 6. B3 decision — activation timestamp source

**Question.** Does `ActivationCrossedReceipt` need its own `crossed_at`? Should it carry or derive `effective_at` from the decision? What is deterministic, what is operational, what appears in evidence?

Three conceptually distinct times exist:

- **decision effective / accepted time** — a property of the *decision*. It is **not** a field on `DecisionRecordedReceipt` today (that class carries only `recorded_at`, a recording time; any effective time lives inside the never-stored, fingerprinted decision body). `effective_at` as a runtime field exists only in the **membership actor lane** (`icn-coop` / `membership_service`, the #2286/#2288 durable-timestamp work), not in any receipt.
- **activation crossing time** — the instant the boundary is crossed.
- **receipt recorded time** — the ADR-0026 envelope's record time; on the landed classes this is `recorded_at: u64`, caller-supplied, hashed into `record_hash` but **excluded** from duplicate identity.

Options considered:

1. **Single caller-supplied `recorded_at: u64`, byte-parallel with the four landed classes. CHOSEN.**
2. Distinct `crossed_at` **and** `recorded_at`. Rejected for `:v1`: in this local/dev/fixture slice the crossing is *recorded at the moment it is crossed* — the two instants coincide. Two fields would invite drift and a second, undecided determinism question for zero current benefit.
3. Carry or derive `effective_at` from the decision. Rejected: `effective_at` is a membership-lane concept absent from the receipt world; the referenced `DecisionRecordedReceipt` exposes no effective time (its `recorded_at` is a recording time; a genuine effective time, if any, is inside the never-stored body). Importing it would cross lanes and manufacture a mushy timestamp with unclear determinism — exactly the #2283/#2284 failure mode.

**Decision B3: `:v1` carries a single caller-supplied `recorded_at: u64`, hashed into `record_hash` but excluded from stable duplicate identity — identical to the four landed classes. No distinct `crossed_at` field is added; no `effective_at` is carried or derived.** In this slice `recorded_at` denotes the moment the crossing was recorded, which coincides with the crossing itself.

Binding consequences:

- **Which timestamp is deterministic across nodes: none of them.** No wall-clock time is a cross-node-deterministic input. Determinism in the receipt comes entirely from its content-addressed identity — `(domain_id, session_id, activation_id, decision_id, decision_record_hash, gate_basis, crossed_by)` — **not** from any timestamp. `recorded_at` may live inside `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity, so replay converges on the original stamp (contract §9). Local wall-clock must never be an identity input.
- **Which is operational: `recorded_at`.** It is a local wall-clock stamp for human/audit legibility, not a coordination value.
- **What appears in evidence/export: `recorded_at`** (human-readable "when recorded") plus the content-addressed proof pointers (`decision_record_hash`, `gate_basis`, `record_hash`). Evidence MUST NOT surface a decision effective time pulled from the never-stored decision body, and MUST NOT invent a `crossed_at` distinct from `recorded_at`.
- **Future split is a `:v2` decision.** If a real consumer later needs to distinguish crossing time from recording time (e.g. a crossing recorded asynchronously after the boundary event), that is a `:v2`-or-later field addition under its own decision — never a silent `:v1` add — and any such time also stays out of cross-node identity.

## 7. Consolidated candidate `:v1` layout (for the implementation PR)

Resolving B1/B2/B3 pins the candidate `icn:gov:activation_crossed:v1` field set (all names **candidate — subject to implementation proof and golden-vector pinning**; the tag must hash-separate from, and never converge with, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, and the proposal/vote `icn:gov:decision:v1/v2/v3` lineage):

| Field | Type | In stable identity? | Source |
|-------|------|---------------------|--------|
| `domain_id` | `String` | yes (`key1` half) | anchor |
| `session_id` | `String` | yes (`key1` half) | anchor; session must be opened first |
| `activation_id` | `String` | yes (`key2`) | caller-opaque per-activation id |
| `decision_id` | `String` | yes | **B1** — decision being activated (must exist in-session) |
| `decision_record_hash` | `Hash` (32) | yes | **B1** — content-addressed proof link to the `DecisionRecordedReceipt` |
| `gate_basis` | `Hash` (32) | yes | **B2** — fingerprint over the sorted passed gate-result `record_hash`es |
| `crossed_by` | `String` (DID) | yes | recorder-not-crosser; grants zero authority |
| `recorded_at` | `u64` | **no** | **B3** — caller-supplied; hashed; excluded from identity (retry never restamps) |
| `record_hash` | `Hash` (32) | (equality anchor) | canonical blake3; the sole `PartialEq`/`Eq` anchor |

An optional `body_hash: Hash` (a fixed-32 fingerprint of an `ActivationRequest` payload, if one is ever fingerprinted; the body is never stored) is **deferred** — the first slice witnesses a crossing without needing a request body. If added later it joins stable identity like the decision class's `body_hash`.

**Candidate canonical hashing:** `DOMAIN_TAG` (`icn:gov:activation_crossed:v1`) first → length-prefixed `domain_id`, `session_id`, `activation_id`, `decision_id`, `crossed_by` → `decision_record_hash` raw 32 (no length prefix) → `gate_basis` raw 32 (no length prefix) → `recorded_at` LE → any optional `body_hash` raw 32. Exact layout is fixed by the implementation PR and pinned by a golden vector.

**Uniqueness / conflict:** `put_opaque_if_absent` keyed on `(class, key1, key2)` where `key1` is an injective netstring composite of `(domain_id, session_id)` and `key2` is `activation_id`; conflict detection on `(decision_id, decision_record_hash, gate_basis, crossed_by, body_hash?)`. `recorded_at` and `record_hash` are not identity. Same-identity retry ⇒ original returned; mismatch ⇒ fail-closed `activation_crossed_conflict`.

**Preconditions (all fail-closed; on any failure nothing is persisted):** (1) the `(domain_id, session_id)` session was opened first; (2) a `DecisionRecordedReceipt` with `record_hash == decision_record_hash` exists in that same session; (3) every gate-result receipt in the declared basis exists in that same session and carries `result == Pass`; the basis is non-empty; (4) ids are non-empty/non-whitespace.

## 8. Implementation constraints for the next PR

The later implementation PR **may**:

- add the `ActivationCrossedReceipt` class **only**, conforming to the #2294 contract plus this rung (§7 above);
- add the minimum reference (B1), gate-basis (B2), and timestamp (B3) support pinned here;
- add `proof.rs` unit tests and a runtime-slice integration test where the existing receipt pattern supports them (construction / emission / persistence / retrieval), mirroring `decision_recorded_receipt_runtime_slice.rs`.

The later implementation PR **must not**:

- implement `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`, or `EvidencePacketProducedReceipt`;
- add or change a `ProcessGateKind` variant, or add an `ActivationRequest` gate object;
- extend `web/member-shell/` or any evidence surface (rendering stays deferred per contract §11) unless separately scoped and reviewed;
- touch OpenAPI / SDK, or publish a served schema;
- auto-close any protected issue (#2293, #1748, #2141, #2041) or reopen #2289 — it must leave #2293 open for maintainer disposition.

## 9. Validation requirements for the implementation PR

Both test tiers the landed classes use, plus the rung-specific checks:

- **`proof.rs` unit tests:** a golden vector pinning the `:v1` `record_hash` of a fixed sample; a determinism test (same inputs ⇒ same hash); a per-field test (every field change, including `decision_record_hash` and `gate_basis`, ⇒ different hash); a tag-disjointness test asserting `icn:gov:activation_crossed:v1` never collides with — and a comment that it must never converge with — `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage.
- **Runtime-slice integration test:** emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns the original, never restamped; different `crossed_by` / `decision_record_hash` / `gate_basis` for the same identity fail closed; unopened session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` never mix).
- **B1 cross-link test** (§4): reference resolves to the persisted decision; absent / wrong-session decision refused fail-closed.
- **B2 gate-basis test** (§5.2): declared basis of real `Pass` gate receipts recomputes to `gate_basis`; a `Fail`/absent gate in the basis refused fail-closed; empty basis refused; basis order-independent.
- **B3 timestamp test** (§6): two records differing only in `recorded_at` share duplicate identity (retry returns original, no conflict); `recorded_at` participates in `record_hash` but not in identity.
- **Idempotence / replay test:** a logical crossing replayed on a second node converges on the original receipt (original stamp, original hash).
- **Privacy grep:** no private deliberation / decision / activation-request body text in any serialized receipt or fixture — fingerprints only.
- **No-overclaim grep:** no "activation implemented / complete / production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2" claims introduced.
- **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim (§7 of the contract).
- **Protected close-keyword grep:** the implementation PR carries no closing keyword (fix / close / resolve) adjacent to a protected issue number (#2293, #1748, #2141, #2041, #2289) — use `Refs` only.

## 10. Deferred work (explicitly out of scope of this rung and its future implementation)

- `MutationPlanRecordedReceipt`, `MutationAppliedReceipt`, `EvidencePacketProducedReceipt`.
- Any new `ProcessGateKind` variant or `ActivationRequest` gate object.
- Member-shell / evidence-surface rendering of `ActivationCrossedReceipt` (contract §11 defers it; a later separately-scoped fixture-only surface may add it after the receipt lands, as #2291 did for the first four classes).
- The actual **#2041** human/AT pass (screen-reader / low-vision / switch / AT-compat) — parked for a real human-testing phase.
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274), service hosting, K3s / DNS / Forgejo.

## 11. Non-goals

Restated from #2293 / the #2294 contract — this rung and its future implementation are:

- not #2041 completion; not human/AT execution; no screen-reader, switch-control, or low-vision human validation is claimed or performed here;
- not production / pilot / organizer-ready / member-ready readiness;
- not live federation; not NYCN activation; not Phase-2 completion;
- not #2081 / #2080 / #2274; not entity-auth enforcement; not trusted token issuance; not UnknownLegacy repair; not service hosting; not K3s / DNS / Forgejo;
- not a general workflow engine; not a policy engine (which gates are *required* stays charter/app-layer and deferred); not chat / comment / moderation / social feed;
- not proposal / vote / quorum / mandate semantics; the reference to `DecisionRecordedReceipt` creates no tie to the proposal/vote `GovernanceDecisionReceipt` lineage;
- not mutation planning or mutation application;
- not `EvidencePacketProducedReceipt`.

Receipts record institutional facts. They grant zero authority.

## 12. Related

Refs #2293.
Refs #2294.
Refs #1748.
Refs #2141.
Refs #2041.
