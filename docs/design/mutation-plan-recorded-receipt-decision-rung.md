# MutationPlanRecordedReceipt decision rung — M1/M2/M3 (plan → activation reference, plan-body representation, timestamp)

**Status:** draft — design / decision rung (not runtime implementation)
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `0a84dc86` (the merged #2300 contract's tip). Code anchors (`icn/crates/icn-governance/src/proof.rs`, `icn/apps/governance/src/receipt_backend.rs`, `icn/apps/governance/src/manager.rs`) were verified at that commit — re-verify before relying on exact line numbers or hashes; they drift.
**Related:** #2301 (this rung's issue) · #2299 (the `MutationPlanRecordedReceipt` design-contract issue) · #2300 (merged design/audit contract, [`docs/design/mutation-plan-recorded-receipt.md`](mutation-plan-recorded-receipt.md)) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2041 (human/AT pass — open/parked) · #2296 (`ActivationCrossedReceipt` implementation) · #2298 (activation render in the process-evidence member-shell demo) · PR #2295 (the sibling activation decision rung, [`activation-crossed-receipt-decision-rung.md`](activation-crossed-receipt-decision-rung.md)) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope) · `ops/ideas/framing/institutional-process-substrate.md` (framing)

> Narrow decision document resolving the three implementation blockers the merged #2300 `MutationPlanRecordedReceipt` design/audit contract named in its §14 — **M1** (plan → activation cross-receipt reference), **M2** (plan-body representation and whether a typed operation model / plan-kind taxonomy is required), and **M3** (`recorded_at` vs distinct `planned_at` timestamp semantics). It mirrors the `activation-crossed-receipt-decision-rung.md` cadence: land the contract (#2300), then resolve the hash-participating structure **in writing** before a tag is pinned, then implement. This document decides nothing else: no runtime change, no receipt class added, no mutation application, no evidence-packet production, no member-shell change, no human/AT run. Receipts record institutional facts. They grant zero authority.

## 1. Purpose

The #2300 design contract scoped a candidate `MutationPlanRecordedReceipt` — the sixth `ProcessTransitionReceipt` rung under #1748 / #2141 that would witness that a **mutation plan was recorded** after an `ActivationCrossedReceipt`, before any mutation is applied. The contract deliberately refused to pin the candidate `icn:gov:mutation_plan_recorded:v1` layout and blocked implementation on three questions whose answers change the canonical hash layout or the class's inter-receipt relationships:

- **M1** — does the receipt name the activation it follows, and by what?
- **M2** — how is the plan body represented, and is a typed operation model or plan-kind taxonomy required?
- **M3** — what is the plan timestamp source, and how does it stay deterministic?

The landed rule (from the #2278 review cycle, restated by the #2281 Q4 decision and applied again by the #2295 activation rung) is that **hash-participating structure is decided in writing before a tag is pinned, never silently in an implementation PR.** This document resolves M1/M2/M3 so a contract-conformant implementation PR can begin. It is not a workflow engine, not a policy engine, not mutation application, and not evidence-packet production.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `0a84dc86`):

- **#2300** — `MutationPlanRecordedReceipt` design/audit contract — **landed** (merged `0a84dc86`).
- **#2299** — the design-contract issue — **closed / completed** (by #2300).
- **#2296** — `ActivationCrossedReceipt` runtime implementation (the receipt this plan references) — **landed**; the fifth `ProcessTransitionReceipt` class.
- **#2298** — activation render in the fixture-only process-evidence member-shell surface — **landed**.
- **#1748 / #2141** — Institutional Process Substrate milestone / vertical spine — **open**.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.
- `MutationPlanRecordedReceipt` **is not implemented** — no Rust struct, tag, manager method, backend class constant, route, or test exists anywhere in `icn/crates/` (confirmed by live audit).
- `MutationPlan` / `MutationAppliedReceipt` / `EvidencePacketProducedReceipt`, and the framing's proposed read-model `PreviewReviewPacket` / `pending_publish_summary`, remain **framing / doc-only**; the live audit found no runtime seam for any of them.

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Repo audit update (verified against live code)

Confirming the #2300 audit against `origin/main` @ `0a84dc86` — the facts M1/M2/M3 depend on:

| Subject | Finding | Anchor |
|---------|---------|--------|
| `ActivationCrossedReceipt` (the receipt this plan references) | fields `domain_id, session_id, activation_id, decision_id, decision_record_hash, gate_basis, crossed_by, recorded_at, record_hash`; tag `icn:gov:activation_crossed:v1`; `record_hash` is the sole `PartialEq`/`Eq` anchor. It already binds the decision (`decision_id` + `decision_record_hash`) and the gate basis (`gate_basis`) — so a plan referencing the activation inherits those transitively | `proof.rs` (`ActivationCrossedReceipt`, `DOMAIN_TAG`, `compute_record_hash`) |
| activation lookup / uniqueness | `put_activation_crossed` persists via **`put_opaque_if_absent`** under class `"activation_crossed"`, `key1 =` the injective netstring `activation_crossed_composite_key1(domain_id, session_id)`, `key2 = activation_id`; `get_activation_crossed(domain_id, session_id, activation_id)` reads it back. A plan can therefore verify its activation reference fail-closed by `get_activation_crossed(...)` then comparing `record_hash` | `receipt_backend.rs` (`put_activation_crossed`, `get_activation_crossed`, `activation_crossed_composite_key1`); `manager.rs` (`record_activation_crossed`, `get_activation_crossed`) |
| the five landed classes | `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` are the only runtime `ProcessTransitionReceipt`s; all five tags present in `proof.rs` | `proof.rs` |
| inter-receipt references | exactly **one** exists: `ActivationCrossedReceipt` → `DecisionRecordedReceipt` (by `decision_id` + `decision_record_hash`), the #2295 B1 link. A plan → activation link would be the lane's **second** inter-receipt reference | whole-repo search |
| `MutationPlan` / `PreviewReviewPacket` / `pending_publish_summary` / `MutationAppliedReceipt` / `EvidencePacketProducedReceipt` | **framing/doc-only** — no Rust type in `icn/crates` or `icn/apps`. `icn-baseline-lock/src/evidence.rs` defines a separate `EvidencePacket` baseline-lock bundle, **not** a governance process class | whole-repo search |
| `put_opaque_if_absent` | the idempotence primitive on the gateway `ReceiptStore` and the `GovernanceReceiptBackend` trait — atomic insert-if-absent keyed on `(class, key1, key2)`; `None` ⇒ this write won, `Some(existing)` ⇒ return the original (never restamp). A sixth class reuses it | `receipt_backend.rs`, `receipt_store.rs` |

**Bottom line:** every #2300 audit claim that M1/M2/M3 rely on is accurate against live code. The five landed classes are the only runtime `ProcessTransitionReceipt`s; the mutation-plan rung remains seam-discovery work, and the `ActivationCrossedReceipt` it would reference already carries the decision + gate-basis bindings the plan can inherit transitively.

## 4. M1 decision — plan → activation cross-receipt reference

**Question.** Does `MutationPlanRecordedReceipt` name the `ActivationCrossedReceipt` it follows, and if so by `activation_id`, by `activation_record_hash`, or both? Must it also directly reference the decision and/or gate basis? This is the lane's **second** inter-receipt reference.

Options considered:

1. **No in-receipt reference** — rely on the shared `(domain_id, session_id)` session anchor; a read-model joins activation and plan by session. Rejected: a session may cross more than one activation; "which activation this plan follows" then has no cryptographic answer, only a temporal guess. It also fails the contract's framing (a plan is recorded *as a consequence of* a specific activation).
2. **`activation_id` only** — the caller-opaque handle. Rejected as the *sole* link: `activation_id` is unique only within a session and caller-opaque; it names *which slot* but does not bind to the activation's recorded **content**. A plan could cite an `activation_id` whose crossing later differs from what was recorded, and nothing would detect it.
3. **`activation_record_hash` only** — the 32-byte content-addressed `record_hash` of the `ActivationCrossedReceipt`. Strong cryptographic binding, but drops the human/index handle.
4. **Both `activation_id` and `activation_record_hash`. CHOSEN.** Directly mirrors the #2295 B1 decision (`decision_id` + `decision_record_hash`) — the proven posture for this lane's first inter-receipt link.
5. Also directly reference the decision (`decision_id` / `decision_record_hash`) and/or gate basis on the plan receipt. **Rejected for `:v1`:** the referenced `ActivationCrossedReceipt` *already* binds `decision_id`, `decision_record_hash`, and `gate_basis`. Duplicating them on the plan receipt is redundant, invites drift between the two copies, and grows the hash layout for no cryptographic gain — the plan inherits them **transitively** through the activation link. If a future consumer needs a direct plan→decision link, that is a `:v2`-or-later decision, not a silent `:v1` add.

**Decision M1: `:v1` carries a direct reference to the activation it follows by *both* its caller-opaque `activation_id` and its content-addressed `activation_record_hash`.** The decision and gate basis are **not** re-referenced on the plan receipt in `:v1`; they are inherited transitively through the activation. Candidate field names (subject to implementation proof and golden-vector pinning):

- `activation_id: String` — the activation being followed; the human/index handle, unique within the session.
- `activation_record_hash: Hash` — the 32-byte `record_hash` of that `ActivationCrossedReceipt`; the content-addressed proof link.

Binding consequences:

- **Both fields participate in the canonical `record_hash` and in stable duplicate identity.** A same-identity retry returns the original receipt un-restamped; a different `activation_record_hash` (or `activation_id`) for the same `plan_id` is a fail-closed conflict (`mutation_plan_recorded_conflict`), mirroring `activation_crossed_conflict`.
- **The reference is verified, not merely asserted (fail-closed precondition).** The implementation MUST require that an `ActivationCrossedReceipt` with exactly `activation_record_hash` exists in the **same** `(domain_id, session_id)` — resolved via `get_activation_crossed(domain_id, session_id, activation_id)` and compared on `record_hash` — before the plan is recorded. If it is absent, present under a different session/domain, or its `activation_id` does not match the supplied one, the plan is **not** recorded and **no receipt is emitted** (mirroring the #2295 B1 verified-not-asserted precondition). This is what makes the link a *proof* ("this activation was recorded and I planned on it"), not a claim.
- **ADR-0026 preserved.** The link points at the activation's own self-hashed `record_hash`; `MutationPlanRecordedReceipt` inherits the *process-transition* discipline (self-contained blake3 `record_hash`, opaque-store persistence) and asserts **no** signed-envelope/merkle inheritance.
- **Idempotence / replay.** Because `activation_record_hash` is content-addressed and deterministic (not wall-clock), two nodes replaying the same logical plan derive the same plan identity and converge on the **original** receipt via `put_opaque_if_absent`.

**Test that proves it:** a runtime-slice test that (a) records an `ActivationCrossedReceipt`, then records a `MutationPlanRecordedReceipt` citing that activation's real `record_hash`, round-trips it, and asserts the stored `activation_record_hash` equals the activation receipt's `record_hash`; (b) a plan whose `activation_record_hash` names no activation in the session (or names one from a different session, or whose `activation_id` mismatches) is refused fail-closed and persists nothing; (c) a same-identity retry returns the original plan un-restamped; (d) a conflicting `activation_record_hash`/`activation_id` for the same `plan_id` is a fail-closed conflict.

## 5. M2 decision — plan-body representation

**Question.** Is the receipt `body_hash`-only (like deliberation/decision), or does it carry a typed minimal operation/target/effect model? Is a plan-kind taxonomy needed?

Options considered:

1. **`body_hash`-only — a caller-supplied 32-byte fingerprint of the `MutationPlan` body; the body is never stored. CHOSEN.**
2. A typed minimal operation/target/effect model on the receipt (e.g. a list of `{op, target}`). Rejected for `:v1`: it puts kernel-readable mutation semantics into the receipt, violating the meaning firewall (the kernel enforces constraints without understanding their origin; the framing §"MutationPlan" is explicit that *"the kernel does not read the plan semantically"*). It also stores potentially sensitive operation content, breaking the privacy posture, and is variable-length and unbounded in the hash layout. The typed plan model, if any, stays **app-side** and is fingerprinted, not stored.
3. A `PreviewReviewPacket`/`pending_publish_summary`-shaped payload. Rejected for `:v1`: that read-model is itself framing-only (no runtime type), and embedding it would import an unfixed schema into a hash-participating layout.
4. A plan-kind taxonomy (à la `DeliberationEntryKind` / `ProcessGateKind`), e.g. `create/update/retire/reassign/allocate/settle/install/bind`. Rejected for `:v1`: a kind would be a hash-participating `u8` discriminant and an **ADR-controlled taxonomy** (closed enum, no free append) — exactly the boundary the #2295 B2 decision drew for gate kinds. It also leaks operation semantics into the receipt. No plan-kind in `:v1`.

**Decision M2: `:v1` is `body_hash`-only.** A caller-supplied 32-byte `body_hash` fingerprints the `MutationPlan` body; **the plan body — its operation list, target list, effect payload, or any typed operation model — is never stored by the receipt.** No plan-kind taxonomy in `:v1`. This mirrors the `DecisionRecordedReceipt` / `DeliberationEntryRecordedReceipt` `body_hash` discipline exactly.

Binding consequences:

- **`body_hash` participates in the canonical `record_hash` and in stable duplicate identity** (like the decision/deliberation `body_hash`). It is a fixed-32 field appended raw (no length prefix).
- **The firewall holds:** the receipt carries no kernel-readable operation content; it witnesses *that a plan (fingerprinted) was recorded*, not what the plan does. Whether the plan is safe/authorized to apply is a charter/gate/authority question upstream of this type.
- **Privacy holds:** no plan body text, operation list, target list, or effect payload is stored; a future evidence/export summary carries proof pointers only (the #2289/#2291/#2298 pattern).
- **`body_hash` is not verified for content** (the receipt cannot re-derive what it never stored) — it is the caller's content fingerprint, exactly as for decision/deliberation. No non-zero requirement is imposed (consistent with the landed classes).

**Test that proves it:** the serialized receipt carries exactly the `:v1` field set — no `body`/`content`/operation/target/effect field; only `body_hash` (a per-field/golden test confirms `body_hash` participates in the hash; a serde payload-audit test confirms no body field is present).

## 6. M3 decision — plan timestamp source

**Question.** Caller-supplied `recorded_at` excluded from stable duplicate identity (the current receipt pattern)? Any distinct `planned_at`? What stays deterministic?

Three conceptually distinct times could exist: the activation crossing time (already on the referenced `ActivationCrossedReceipt` as its `recorded_at`); a plan-authored time; and the receipt record time.

Options considered:

1. **Single caller-supplied `recorded_at: u64`, byte-parallel with the five landed classes. CHOSEN.**
2. Distinct `planned_at` **and** `recorded_at`. Rejected for `:v1`: in this local/dev/fixture slice the plan is *recorded at the moment it is authored* — the two instants coincide. Two fields would invite drift and a second, undecided determinism question for zero current benefit.
3. Derive a time from the referenced activation. Rejected: importing the activation's `recorded_at` onto the plan manufactures a duplicated timestamp with unclear semantics; the plan's own `recorded_at` is its recording time, and the activation time is already reachable via the reference.

**Decision M3: `:v1` carries a single caller-supplied `recorded_at: u64`, hashed into `record_hash` but excluded from stable duplicate identity — identical to the five landed classes. No distinct `planned_at` is added; no time is derived from the activation.** In this slice `recorded_at` denotes the moment the plan was recorded, which coincides with the planning itself.

Binding consequences:

- **No wall-clock time is a cross-node-deterministic identity input** (per the #2283/#2284 membership-determinism doctrine). Determinism in the receipt comes entirely from its content-addressed identity — `(domain_id, session_id, plan_id, activation_id, activation_record_hash, recorded_by, body_hash)` — **not** from any timestamp. `recorded_at` may live inside `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity, so replay converges on the original stamp.
- **What appears in evidence/export: `recorded_at`** (human-readable "when recorded") plus the content-addressed proof pointers (`activation_record_hash`, `body_hash`, `record_hash`). Evidence MUST NOT invent a `planned_at` distinct from `recorded_at`, and MUST NOT surface a plan body time pulled from the never-stored plan body.
- **Future split is a `:v2` decision.** If a real consumer later needs to distinguish planning time from recording time, that is a `:v2`-or-later field addition under its own decision — never a silent `:v1` add — and any such time also stays out of cross-node identity.

## 7. Consolidated candidate `:v1` layout (for the implementation PR)

Resolving M1/M2/M3 pins the candidate `icn:gov:mutation_plan_recorded:v1` field set (all names **candidate — subject to implementation proof and golden-vector pinning**; the tag must hash-separate from, and never converge with, `icn:gov:activation_crossed:v1`, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, and the proposal/vote `icn:gov:decision:v1/v2/v3` lineage):

| Field | Type | In stable identity? | Source |
|-------|------|---------------------|--------|
| `domain_id` | `String` | yes (`key1` half) | anchor |
| `session_id` | `String` | yes (`key1` half) | anchor; session must be opened first |
| `plan_id` | `String` | yes (`key2`) | caller-opaque per-plan id |
| `activation_id` | `String` | yes | **M1** — activation being followed (must exist in-session) |
| `activation_record_hash` | `Hash` (32) | yes | **M1** — content-addressed proof link to the `ActivationCrossedReceipt` |
| `recorded_by` | `String` (DID) | yes | recorder-not-planner; grants zero authority |
| `body_hash` | `Hash` (32) | yes | **M2** — fingerprint of the `MutationPlan` body; body never stored |
| `recorded_at` | `u64` | **no** | **M3** — caller-supplied; hashed; excluded from identity (retry never restamps) |
| `record_hash` | `Hash` (32) | (equality anchor) | canonical blake3; the sole `PartialEq`/`Eq` anchor |

**Candidate canonical hashing:** `DOMAIN_TAG` (`icn:gov:mutation_plan_recorded:v1`) first → length-prefixed `domain_id`, `session_id`, `plan_id`, `activation_id`, `recorded_by` → `activation_record_hash` raw 32 (no length prefix) → `body_hash` raw 32 (no length prefix) → `recorded_at` LE. Exact layout is fixed by the implementation PR and pinned by a golden vector.

**Uniqueness / conflict:** `put_opaque_if_absent` keyed on `(class, key1, key2)` where `key1` is an injective netstring composite of `(domain_id, session_id)` and `key2` is `plan_id`; conflict detection on `(activation_id, activation_record_hash, recorded_by, body_hash)`. `recorded_at` and `record_hash` are not identity. Same-identity retry ⇒ original returned; mismatch ⇒ fail-closed `mutation_plan_recorded_conflict`.

**Preconditions (all fail-closed; on any failure nothing is persisted):** (1) the `(domain_id, session_id)` session was opened first; (2) an `ActivationCrossedReceipt` with `record_hash == activation_record_hash` exists in that same session and its `activation_id` equals the supplied `activation_id`; (3) `domain_id` / `session_id` / `plan_id` / `activation_id` / `recorded_by` are non-empty / non-whitespace.

## 8. Implementation constraints for the next PR

The later implementation PR **may**:

- add the `MutationPlanRecordedReceipt` class **only**, conforming to the #2300 contract plus this rung (§7 above);
- add the minimum reference (M1), body-hash (M2), and timestamp (M3) support pinned here;
- add `proof.rs` unit tests and a runtime-slice integration test where the existing receipt pattern supports them (construction / emission / persistence / retrieval), mirroring `activation_crossed_receipt_runtime_slice.rs`.

The later implementation PR **must not**:

- implement `MutationAppliedReceipt` or `EvidencePacketProducedReceipt`, or apply any mutation;
- add a typed/kernel-readable mutation-plan operation model, a `target_ref`/`effect_ref`, or a plan-kind taxonomy;
- add or change any `ProcessGateKind` variant, or add an `ActivationRequest` gate object;
- extend `web/member-shell/` or any evidence surface (rendering stays deferred) unless separately scoped and reviewed;
- touch OpenAPI / SDK, or publish a served schema;
- auto-close any protected issue (#1748, #2141, #2041) or its own implementation issue — leave it open for maintainer disposition.

## 9. Validation requirements for the implementation PR

Both test tiers the landed classes use, plus the rung-specific checks:

- **`proof.rs` unit tests:** a golden vector pinning the `:v1` `record_hash` of a fixed sample; a determinism test (same inputs ⇒ same hash); a per-field test (every field change, including `activation_record_hash` and `body_hash`, ⇒ different hash); a tag-disjointness test asserting `icn:gov:mutation_plan_recorded:v1` never collides with — and a comment that it must never converge with — `activation_crossed`, `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage.
- **Runtime-slice integration test:** emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns the original, never restamped; different `activation_id` / `activation_record_hash` / `recorded_by` / `body_hash` for the same identity fail closed; unopened session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` never mix).
- **M1 cross-link test** (§4): the referenced `ActivationCrossedReceipt` (by `activation_record_hash`) must exist in the same `(domain_id, session_id)` with a matching `activation_id`; an absent, wrong-session, wrong-domain, or `activation_id`-mismatched reference is refused fail-closed and persists nothing.
- **M2 body test** (§5): the serialized payload carries exactly the `:v1` field set — no `body`/operation/target/effect field; `body_hash` participates in `record_hash`.
- **M3 timestamp test** (§6): two records differing only in `recorded_at` share duplicate identity (retry returns original, no conflict); `recorded_at` participates in `record_hash` but not in identity.
- **Idempotence / replay test:** a logical plan replayed on a second node converges on the original receipt (original stamp, original hash).
- **Privacy grep:** no plan body / operation / target / effect text in any serialized receipt or fixture — fingerprints only.
- **No-overclaim grep:** no "mutation applied / plan applied / production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2" claims introduced.
- **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim.
- **Protected close-keyword grep:** the implementation PR carries no closing keyword (fix / close / resolve) adjacent to a protected issue number (#1748, #2141, #2041) — use `Refs` only.

## 10. Deferred work (explicitly out of scope of this rung and its future implementation)

- `MutationAppliedReceipt` — the receipt that would witness a mutation actually applied.
- `EvidencePacketProducedReceipt` — a runtime evidence-packet producer.
- Any typed/kernel-readable `MutationPlan` operation model, `target_ref`/`effect_ref`, plan-kind taxonomy, or `PreviewReviewPacket` runtime type.
- Member-shell / process-evidence rendering of `MutationPlanRecordedReceipt` (a later separately-scoped fixture-only surface may add it after the receipt lands, as #2298 did for `ActivationCrossedReceipt`).
- Action-card triggers (ADR-0027 / #1713).
- The actual **#2041** human/AT pass — parked for a real human-testing phase.
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274), service hosting, K3s / DNS / Forgejo.

## 11. Non-goals

Restated from #2301 / the #2300 contract — this rung and its future implementation are:

- not `MutationAppliedReceipt`; not mutation application; not applying any plan;
- not `EvidencePacketProducedReceipt`; not an evidence-packet producer;
- not an action-card trigger; not a general workflow engine; not a policy/authority engine; not a new authorization semantic;
- not a typed/kernel-readable mutation-plan operation model; not a plan-kind taxonomy;
- not a new `ProcessGateKind`; not an `ActivationRequest` object;
- not OpenAPI / SDK / served-schema work; not member-shell implementation;
- not #2041 completion; not human/AT execution; not #1748 or #2141 closure;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion;
- not proposal / vote / quorum / mandate / outcome semantics.

Receipts record institutional facts. They grant zero authority.

## 12. Implementation sequencing & protected issue state

**Recommendation (matching the ActivationCrossed lane cadence #2294 → #2295 → #2296):** with this decision rung landed on top of the #2300 contract, a contract-conformant implementation PR may add the `MutationPlanRecordedReceipt` class **only**, per §7–§9. The implementation PR must keep #1748 / #2141 / #2041 open unless separately reviewed, and must leave its own issue open for maintainer disposition rather than auto-closing it by side effect.

Protected issue state at authoring: #2299 closed/completed (design contract); #1748 open; #2141 open; #2041 open/parked; #2289 closed; #2081 / #2080 / #2274 open/untouched; #1907 untouched.

## 13. Related

Refs #2301.
Refs #2299.
Refs #2300.
Refs #1748.
Refs #2141.
Refs #2041.
Refs #2296.
Refs #2298.
