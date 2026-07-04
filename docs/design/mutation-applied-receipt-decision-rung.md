# MutationAppliedReceipt decision rung — A1/A2/A3/A4 (plan → application reference, result representation, timestamp, applied-witness boundary)

**Status:** draft — design / decision rung (not runtime implementation)
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `e96456f3` (the merged #2307 contract's tip). Code anchors (`icn/crates/icn-governance/src/proof.rs`, `icn/apps/governance/src/receipt_backend.rs`, `icn/apps/governance/src/manager.rs`) were verified at that commit — re-verify before relying on exact line numbers or hashes; they drift.
**Related:** #2308 (this rung's issue) · #2306 (the `MutationAppliedReceipt` design-contract issue) · #2307 (merged design/audit contract, [`docs/design/mutation-applied-receipt.md`](mutation-applied-receipt.md)) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2041 (human/AT pass — open/parked) · #2303 (`MutationPlanRecordedReceipt` implementation) · #2305 (mutation-plan render in the process-evidence member-shell demo) · PR #2302 (the sibling plan decision rung, [`mutation-plan-recorded-receipt-decision-rung.md`](mutation-plan-recorded-receipt-decision-rung.md)) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope) · `ops/ideas/framing/institutional-process-substrate.md` (framing)

> Narrow decision document resolving the four implementation blockers the merged #2307 `MutationAppliedReceipt` design/audit contract named in its §14 — **A1** (plan → application cross-receipt reference), **A2** (application body/result representation and whether a typed operation/result/effect model is required), **A3** (`applied_at` timestamp semantics), and **A4** (what "applied" means without turning the receipt into an execution engine). It mirrors the `mutation-plan-recorded-receipt-decision-rung.md` cadence: land the contract (#2307), then resolve the hash-participating structure **in writing** before a tag is pinned, then implement. This document decides nothing else: no runtime change, no receipt class added, no mutation application, no evidence-packet production, no member-shell change, no human/AT run. Receipts record institutional facts. They grant zero authority.

## 1. Purpose

The #2307 design contract scoped a candidate `MutationAppliedReceipt` — the seventh `ProcessTransitionReceipt` rung under #1748 / #2141 that would witness that a **mutation plan was applied**, recorded after a `MutationPlanRecordedReceipt`. The contract deliberately refused to pin the candidate `icn:gov:mutation_applied:v1` layout and blocked implementation on four questions whose answers change the canonical hash layout, the class's inter-receipt relationships, or the meaning of the receipt itself:

- **A1** — does the receipt name the plan it follows, and by what?
- **A2** — how is the applied result represented, and is a typed operation/result/effect model required?
- **A3** — what is the applied timestamp source, and how does it stay deterministic?
- **A4** — what does "applied" assert, and how is the receipt kept from becoming an execution/authority engine?

The landed rule (from the #2278 review cycle, restated by the #2281 Q4 decision and applied again by the #2295 activation and #2302 plan rungs) is that **hash-participating structure is decided in writing before a tag is pinned, never silently in an implementation PR.** This document resolves A1/A2/A3/A4 so a contract-conformant implementation PR can begin. It is not a workflow engine, not a policy engine, not a mutation-application engine, and not evidence-packet production.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `e96456f3`):

- **#2307** — `MutationAppliedReceipt` design/audit contract — **landed** (merged `e96456f3`).
- **#2306** — the design-contract issue — **closed / completed** (by #2307).
- **#2303** — `MutationPlanRecordedReceipt` runtime implementation (the receipt this application references) — **landed**; the sixth `ProcessTransitionReceipt` class.
- **#2305** — mutation-plan render in the fixture-only process-evidence member-shell surface — **landed**; **#2304** — its render issue — **closed / completed**.
- **#1748 / #2141** — Institutional Process Substrate milestone / vertical spine — **open**.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.
- `MutationAppliedReceipt` **is not implemented** — no Rust struct, tag, manager method, backend class constant, route, or test exists anywhere in `icn/crates/` or `icn/apps/` (confirmed by live audit: `rg "MutationAppliedReceipt|mutation_applied" icn/crates icn/apps` → no match).
- `EvidencePacketProducedReceipt` remains **framing / doc-only**; the live audit found no runtime seam for it.

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Repo audit update (verified against live code)

Confirming the #2307 audit against `origin/main` @ `e96456f3` — the facts A1/A2/A3/A4 depend on:

| Subject | Finding | Anchor |
|---------|---------|--------|
| `MutationPlanRecordedReceipt` (the receipt this application references) | fields `domain_id, session_id, plan_id, activation_id, activation_record_hash, recorded_by, body_hash, recorded_at, record_hash`; tag `icn:gov:mutation_plan_recorded:v1`; `record_hash` is the sole `PartialEq`/`Eq` anchor. It already references the activation (`activation_id` + `activation_record_hash`), which itself binds the decision (`decision_id` + `decision_record_hash`) and gate basis (`gate_basis`) — so an application referencing the plan inherits activation → decision → gate transitively | `icn/crates/icn-governance/src/proof.rs` (`MutationPlanRecordedReceipt`, `DOMAIN_TAG`, `compute_record_hash`) |
| plan lookup / uniqueness | `put_mutation_plan_recorded` persists via **`put_opaque_if_absent`** under class `"mutation_plan_recorded"`, `key1 =` the injective netstring `mutation_plan_recorded_composite_key1(domain_id, session_id)`, `key2 = plan_id`; `get_mutation_plan_recorded(domain_id, session_id, plan_id)` reads it back. An application can therefore verify its plan reference fail-closed by `get_mutation_plan_recorded(...)` then comparing `record_hash` | `icn/apps/governance/src/receipt_backend.rs` (`put_mutation_plan_recorded`, `get_mutation_plan_recorded`, `mutation_plan_recorded_composite_key1`); `icn/apps/governance/src/manager.rs` (`record_mutation_plan_recorded`, `get_mutation_plan_recorded`) |
| the six landed classes | `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` / `MutationPlanRecordedReceipt` are the only runtime `ProcessTransitionReceipt`s; all six tags present in `proof.rs` | `proof.rs` |
| inter-receipt references | exactly **two** exist: `ActivationCrossedReceipt` → `DecisionRecordedReceipt` (#2295 B1), and `MutationPlanRecordedReceipt` → `ActivationCrossedReceipt` (#2302 M1). An application → plan link would be the lane's **third** inter-receipt reference | whole-repo search |
| `MutationAppliedReceipt` / `EvidencePacketProducedReceipt` | **framing/doc-only** — no Rust type in `icn/crates` or `icn/apps`. `icn/crates/icn-baseline-lock/src/evidence.rs` defines a separate `EvidencePacket` baseline-lock bundle, **not** a governance process class | whole-repo search |
| `put_opaque_if_absent` | the idempotence primitive on the gateway `ReceiptStore` and the `GovernanceReceiptBackend` trait — atomic insert-if-absent keyed on `(class, key1, key2)`; `None` ⇒ this write won, `Some(existing)` ⇒ return the original (never restamp). A seventh class reuses it | `receipt_backend.rs`, `receipt_store.rs` |

**Bottom line:** every #2307 audit claim that A1/A2/A3/A4 rely on is accurate against live code. The six landed classes are the only runtime `ProcessTransitionReceipt`s; the mutation-applied rung remains seam-discovery work, and the `MutationPlanRecordedReceipt` it would reference already carries (transitively, via its activation link) the activation + decision + gate bindings the application can inherit.

## 4. A1 decision — plan → application cross-receipt reference

**Question.** Does `MutationAppliedReceipt` name the `MutationPlanRecordedReceipt` it follows, and if so by `plan_id`, by `plan_record_hash`, or both? Must it also directly reference the activation, decision, and/or gate basis? This is the lane's **third** inter-receipt reference.

Options considered:

1. **No in-receipt reference** — rely on the shared `(domain_id, session_id)` session anchor; a read-model joins plan and application by session. Rejected: a session may record more than one plan; "which plan this application applied" then has no cryptographic answer, only a temporal guess. It also fails the contract's framing (an application is recorded *as the application of a specific recorded plan*).
2. **`plan_id` only** — the caller-opaque handle. Rejected as the *sole* link: `plan_id` is unique only within a session and caller-opaque; it names *which slot* but does not bind to the plan's recorded **content**. An application could cite a `plan_id` whose plan later differs from what was recorded, and nothing would detect it.
3. **`plan_record_hash` only** — the 32-byte content-addressed `record_hash` of the `MutationPlanRecordedReceipt`. Strong cryptographic binding, but drops the human/index handle.
4. **Both `plan_id` and `plan_record_hash`. CHOSEN.** Directly mirrors the #2295 B1 (`decision_id` + `decision_record_hash`) and #2302 M1 (`activation_id` + `activation_record_hash`) decisions — the proven posture for this lane's inter-receipt links.
5. Also directly reference the activation (`activation_id`/`activation_record_hash`), decision (`decision_id`/`decision_record_hash`), and/or gate basis on the application receipt. **Rejected for `:v1`:** the referenced `MutationPlanRecordedReceipt` *already* references the activation, which binds decision + gate basis. Duplicating any of them on the application receipt is redundant, invites drift, and grows the hash layout for no cryptographic gain — the application inherits them **transitively** through the plan → activation chain. If a future consumer needs a direct application→activation/decision link, that is a `:v2`-or-later decision, not a silent `:v1` add.

**Decision A1: `:v1` carries a direct reference to the plan it applies by *both* its caller-opaque `plan_id` and its content-addressed `plan_record_hash`.** The activation, decision, and gate basis are **not** re-referenced on the application receipt in `:v1`; they are inherited transitively through the plan → activation chain. Candidate field names (subject to implementation proof and golden-vector pinning):

- `plan_id: String` — the plan being applied; the human/index handle, unique within the session.
- `plan_record_hash: Hash` — the 32-byte `record_hash` of that `MutationPlanRecordedReceipt`; the content-addressed proof link.

Binding consequences:

- **Both fields participate in the canonical `record_hash` and in stable duplicate identity.** A same-identity retry returns the original receipt un-restamped; a different `plan_record_hash` (or `plan_id`) for the same `application_id` is a fail-closed conflict (`mutation_applied_conflict`), mirroring `mutation_plan_recorded_conflict`.
- **The reference is verified, not merely asserted (fail-closed precondition).** The implementation MUST require that a `MutationPlanRecordedReceipt` with exactly `plan_record_hash` exists in the **same** `(domain_id, session_id)` — resolved via `get_mutation_plan_recorded(domain_id, session_id, plan_id)` and compared on `record_hash` — before the application is recorded. If it is absent, present under a different session/domain, or its `plan_id` does not match the supplied one, the application is **not** recorded and **no receipt is emitted** (mirroring the #2302 M1 verified-not-asserted precondition). This is what makes the link a *proof* ("this plan was recorded and I applied it"), not a claim.
- **ADR-0026 preserved.** The link points at the plan's own self-hashed `record_hash`; `MutationAppliedReceipt` inherits the *process-transition* discipline (self-contained blake3 `record_hash`, opaque-store persistence) and asserts **no** signed-envelope/merkle inheritance.
- **Idempotence / replay.** Because `plan_record_hash` is content-addressed and deterministic (not wall-clock), two nodes replaying the same logical application derive the same application identity and converge on the **original** receipt via `put_opaque_if_absent`.

**Test that proves it:** a runtime-slice test that (a) records the plan (and its prerequisite chain), then records a `MutationAppliedReceipt` citing that plan's real `record_hash`, round-trips it, and asserts the stored `plan_record_hash` equals the plan receipt's `record_hash`; (b) an application whose `plan_record_hash` names no plan in the session (or names one from a different session, or whose `plan_id` mismatches) is refused fail-closed and persists nothing; (c) a same-identity retry returns the original application un-restamped; (d) a conflicting `plan_record_hash`/`plan_id` for the same `application_id` is a fail-closed conflict.

## 5. A2 decision — application body/result representation

**Question.** Is the receipt result-hash-only (like the plan's `body_hash`), or does it carry a typed minimal operation/result/effect model? Is an application-kind taxonomy needed? What is the fingerprint field named?

Options considered:

1. **`result_hash`-only — a caller-supplied 32-byte fingerprint of the application-result record; the result body is never stored. CHOSEN.**
2. A typed minimal operation/result/effect model on the receipt (e.g. a list of `{op, target, effect}`). Rejected for `:v1`: it puts kernel-readable mutation semantics into the receipt, violating the meaning firewall (the kernel enforces constraints without understanding their origin; the framing is explicit that the kernel does not read the plan/effect semantically). It also stores potentially sensitive effect content, breaking the privacy posture, and is variable-length and unbounded in the hash layout. The typed result model, if any, stays **app-side** and is fingerprinted, not stored.
3. Reuse the name `body_hash` (as the plan/decision/deliberation classes do). Rejected: the plan already fingerprints the *plan body* as `body_hash`; the application fingerprints a distinct artifact (the *applied result / effect record*). Naming it `result_hash` distinguishes the two at the type level and prevents a reader from assuming the application re-fingerprints the plan body. `result_hash` **CHOSEN** as the name.
4. An application-kind taxonomy (à la `DeliberationEntryKind` / `ProcessGateKind`). Rejected for `:v1`: a kind would be a hash-participating `u8` discriminant and an **ADR-controlled taxonomy** (closed enum, no free append) — exactly the boundary the #2295 B2 and #2302 M2 decisions drew. It also leaks operation/effect semantics into the receipt. No application-kind in `:v1`.

**Decision A2: `:v1` is `result_hash`-only.** A caller-supplied 32-byte `result_hash` fingerprints the application-result record; **the applied-result body — the plan body, operation list, target list, effect payload, or any typed operation/result/effect model — is never stored by the receipt.** No application-kind taxonomy in `:v1`. This mirrors the `MutationPlanRecordedReceipt` `body_hash` discipline exactly, under a distinct field name.

Binding consequences:

- **`result_hash` participates in the canonical `record_hash` and in stable duplicate identity** (like the plan's `body_hash`). It is a fixed-32 field appended raw (no length prefix).
- **The firewall holds:** the receipt carries no kernel-readable operation/result/effect content; it witnesses *that an application (fingerprinted) was recorded*, not what the application did to any domain state. Whether the application was correct or authorized is a charter/gate/authority question upstream of this type.
- **Privacy holds:** no applied-result body text, plan body text, operation list, target list, or effect payload is stored; a future evidence/export summary carries proof pointers only (the #2289/#2291/#2305 pattern).
- **`result_hash` is not verified for content** (the receipt cannot re-derive what it never stored) — it is the caller's content fingerprint, exactly as for the plan's `body_hash`. No non-zero requirement is imposed (consistent with the landed classes).

**Test that proves it:** the serialized receipt carries exactly the `:v1` field set — no `body`/`content`/operation/target/effect/result-body field; only `result_hash` (a per-field/golden test confirms `result_hash` participates in the hash; a serde payload-audit test confirms no result-body field is present).

## 6. A3 decision — applied timestamp source

**Question.** Caller-supplied `applied_at` excluded from stable duplicate identity (the current receipt pattern)? Any distinct `executed_at`/`effective_at`/`recorded_at`? What stays deterministic?

Conceptually distinct times could exist: the plan recording time (already on the referenced `MutationPlanRecordedReceipt` as its `recorded_at`); an effect-effective time; and the application record time.

Options considered:

1. **Single caller-supplied `applied_at: u64`, byte-parallel with the six landed classes' `recorded_at`. CHOSEN.**
2. Distinct `executed_at`/`effective_at` **and** an application-record time. Rejected for `:v1`: in this local/dev/fixture slice the application is *recorded at the moment it is applied* — the instants coincide. Multiple time fields would invite drift and additional, undecided determinism questions for zero current benefit.
3. Derive a time from the referenced plan. Rejected: importing the plan's `recorded_at` onto the application manufactures a duplicated timestamp with unclear semantics; the application's own `applied_at` is its recording time, and the plan time is already reachable via the reference.
4. Name the field `recorded_at` for exact byte-parallelism with the six landed classes. Rejected in favor of `applied_at`: the field records *when the application was applied/recorded*; `applied_at` is the honest domain name and parallels the plan's `recorded_at` role without pretending this is a plan/decision recording. (This is a name choice only; the hash/identity treatment is identical to `recorded_at`.)

**Decision A3: `:v1` carries a single caller-supplied `applied_at: u64`, hashed into `record_hash` but excluded from stable duplicate identity — the same treatment the six landed classes give `recorded_at`. No distinct `executed_at`, `effective_at`, or `recorded_at` is added; no time is derived from the plan.** In this slice `applied_at` denotes the moment the application was applied, which coincides with its recording.

Binding consequences:

- **No wall-clock time is a cross-node-deterministic identity input** (per the #2283/#2284 membership-determinism doctrine). Determinism in the receipt comes entirely from its content-addressed identity — `(domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash)` — **not** from any timestamp. `applied_at` may live inside `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity, so replay converges on the original stamp.
- **What appears in evidence/export: `applied_at`** (human-readable "when applied") plus the content-addressed proof pointers (`plan_record_hash`, `result_hash`, `record_hash`). Evidence MUST NOT invent an `executed_at`/`effective_at` distinct from `applied_at`, and MUST NOT surface an effect time pulled from the never-stored result body.
- **Future split is a `:v2` decision.** If a real consumer later needs to distinguish an effect-effective time from the recording time, that is a `:v2`-or-later field addition under its own decision — never a silent `:v1` add — and any such time also stays out of cross-node identity.

## 7. A4 decision — applied-witness boundary vs execution/authority boundary

**Question.** What must the runtime require to legitimately record an application, and what does "applied" assert, without the receipt becoming an execution engine or an authority grant? Must "applied" bind a *verifiable* effect, or is a caller-supplied `result_hash` sufficient for `:v1`?

This is the rung's new question — the previous rungs (session/deliberation/decision/gate/activation/plan) all witnessed *recording* facts whose truth is self-contained (a thing was recorded). "Applied" is tempting to read as "the mutation's effects are real and correct," which would pull verification, execution, and authority into the receipt.

Options considered:

1. **Application-recorded process fact; caller-supplied `result_hash` is a sufficient fingerprint; the receipt neither executes, authorizes, validates, enforces, rolls back, nor proves semantic correctness. CHOSEN.**
2. Verifiable-effect binding — require the receipt to bind a downstream artifact/state hash the substrate can later re-derive and re-check. Rejected for `:v1`: there is no runtime effect model, no downstream-state addressing scheme, and no re-derivation path in the current codebase (all framing-only). Requiring it would (a) block this rung on building an effect/verification subsystem, and (b) change the field set (a verifiable-effect reference is a different, larger contract). This is deferred; if a real consumer later needs it, it is a `:v2`-or-later decision under its own rung.
3. Execution/authority semantics — let the receipt *cause* or *authorize* the mutation. Rejected outright and permanently for this class: that would make the receipt an execution engine and an authority grant, violating the lane's core doctrine (*receipts record facts and grant zero authority*) and the meaning firewall. Authority stays upstream in charter/gate/capability/policy/governance paths.

**Decision A4: `MutationAppliedReceipt` records that an *application fact was recorded* — a recorder/apply-witness attests that the plan (verified via A1) was applied and supplies a `result_hash` (A2) fingerprinting the application-result record.** Specifically, for `:v1`:

- `applied_by` is the **recorder / apply-witness DID** — actor evidence, **not** an authority grant, and not a claim that the applier was permitted to apply.
- The receipt **does not execute, authorize, validate, enforce, roll back, or prove the semantic correctness of** the mutation. It performs no side effect on domain state; it only records the application *fact*.
- A caller-supplied `result_hash` is **sufficient** for `:v1` as the content-addressed fingerprint of the application-result record. `:v1` makes **no** claim that the fingerprinted effect is real, complete, correct, or reversible — only that an application with that result fingerprint was recorded here.
- **Verifiable effect semantics, rollback/compensation semantics, typed result/effect models, and evidence-packet production are deferred** to later rungs (and, where relevant, `EvidencePacketProducedReceipt`).
- **Authority remains upstream** in charter / gate / capability / policy / governance paths, not in this receipt. A recorded application is not proof of legitimacy.

Binding consequences:

- The receipt's honesty label in any future evidence surface must read as "an application was recorded here," not "the mutation succeeded / is correct / is authorized."
- Because A4 keeps `:v1` a *recording* fact (no verifiable-effect field), it introduces **no** additional hash-participating field beyond A1–A3; the §8 layout is complete and pinnable.

**Test that proves it:** a no-overclaim grep asserts the receipt/type carries no "authorized / executed / enforced / rolled back / verified-correct" claim; the runtime-slice test asserts recording an application performs no mutation of any domain state beyond persisting the receipt itself (it only calls `put_opaque_if_absent`), and that `applied_by` is stored as an opaque DID string with no capability/authority check attached to it.

## 8. Consolidated candidate `:v1` layout (for the implementation PR)

Resolving A1/A2/A3/A4 pins the candidate `icn:gov:mutation_applied:v1` field set (all names **candidate — subject to implementation proof and golden-vector pinning**; the tag must hash-separate from, and never converge with, `icn:gov:mutation_plan_recorded:v1`, `icn:gov:activation_crossed:v1`, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, and the proposal/vote `icn:gov:decision:v1/v2/v3` lineage):

| Field | Type | In stable identity? | Source |
|-------|------|---------------------|--------|
| `domain_id` | `String` | yes (`key1` half) | anchor |
| `session_id` | `String` | yes (`key1` half) | anchor; session must be opened first |
| `application_id` | `String` | yes (`key2`) | caller-opaque per-application id |
| `plan_id` | `String` | yes | **A1** — plan being applied (must exist in-session) |
| `plan_record_hash` | `Hash` (32) | yes | **A1** — content-addressed proof link to the `MutationPlanRecordedReceipt` |
| `applied_by` | `String` (DID) | yes | **A4** — recorder / apply-witness; grants zero authority |
| `result_hash` | `Hash` (32) | yes | **A2** — fingerprint of the application-result record; result body never stored |
| `applied_at` | `u64` | **no** | **A3** — caller-supplied; hashed; excluded from identity (retry never restamps) |
| `record_hash` | `Hash` (32) | (equality anchor) | canonical blake3; the sole `PartialEq`/`Eq` anchor |

**Candidate canonical hashing:** `DOMAIN_TAG` (`icn:gov:mutation_applied:v1`) first → length-prefixed `domain_id`, `session_id`, `application_id`, `plan_id`, `applied_by` → `plan_record_hash` raw 32 (no length prefix) → `result_hash` raw 32 (no length prefix) → `applied_at` LE. Exact layout is fixed by the implementation PR and pinned by a golden vector.

**Candidate stable duplicate identity:** `(domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash)`. `applied_at` and `record_hash` are **not** identity.

**Uniqueness / conflict:** `put_opaque_if_absent` keyed on `(class, key1, key2)` where `key1` is an injective netstring composite of `(domain_id, session_id)` and `key2` is `application_id`; conflict detection on `(plan_id, plan_record_hash, applied_by, result_hash)`. `applied_at` and `record_hash` are not identity. Same-identity retry ⇒ original returned; mismatch ⇒ fail-closed `mutation_applied_conflict`.

**Preconditions (all fail-closed; on any failure nothing is persisted):** (1) the `(domain_id, session_id)` session was opened first; (2) a `MutationPlanRecordedReceipt` with `record_hash == plan_record_hash` exists in that same session and its `plan_id` equals the supplied `plan_id` (resolved via `get_mutation_plan_recorded(domain_id, session_id, plan_id)` then compared on `record_hash`); (3) `domain_id` / `session_id` / `application_id` / `plan_id` / `applied_by` are non-empty / non-whitespace.

## 9. Implementation constraints for the next PR

The later implementation PR **may**:

- add the `MutationAppliedReceipt` class **only**, conforming to the #2307 contract plus this rung (§8 above);
- add the minimum plan reference (A1), result-hash (A2), timestamp (A3), and applied-witness (A4) support pinned here;
- add `proof.rs` unit tests and a runtime-slice integration test where the existing receipt pattern supports them (construction / emission / persistence / retrieval), mirroring `mutation_plan_recorded_receipt_runtime_slice.rs`.

The later implementation PR **must not**:

- implement `EvidencePacketProducedReceipt`, or apply / execute / verify / roll back any mutation;
- add a typed/kernel-readable mutation operation/result/effect model, a `target_ref`/`effect_ref`, an application-kind taxonomy, or a verifiable-effect binding;
- attach any capability/authority check to `applied_by` (it is opaque actor evidence);
- extend `web/member-shell/` or any evidence surface (rendering stays deferred) unless separately scoped and reviewed;
- touch OpenAPI / SDK, or publish a served schema;
- auto-close any protected issue (#1748, #2141, #2041) or its own implementation issue — leave it open for maintainer disposition.

## 10. Validation requirements for the implementation PR

Both test tiers the landed classes use, plus the rung-specific checks:

- **`proof.rs` unit tests:** a golden vector pinning the `:v1` `record_hash` of a fixed sample; a determinism test (same inputs ⇒ same hash); a per-field test (every field change, including `plan_record_hash` and `result_hash`, ⇒ different hash); a tag-disjointness test asserting `icn:gov:mutation_applied:v1` never collides with — and a comment that it must never converge with — `mutation_plan_recorded`, `activation_crossed`, `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage; a serde/payload-audit test confirming no result-body field is present.
- **Runtime-slice integration test:** emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns the original, never restamped; different `plan_id` / `plan_record_hash` / `applied_by` / `result_hash` for the same identity fail closed (`mutation_applied_conflict`); unopened session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` never mix).
- **A1 cross-link test** (§4): the referenced `MutationPlanRecordedReceipt` (by `plan_record_hash`) must exist in the same `(domain_id, session_id)` with a matching `plan_id`; an absent, wrong-session, wrong-domain, or `plan_id`-mismatched reference is refused fail-closed and persists nothing.
- **A2 result test** (§5): the serialized payload carries exactly the `:v1` field set — no `body`/operation/target/effect/result-body field; `result_hash` participates in `record_hash`.
- **A3 timestamp test** (§6): two records differing only in `applied_at` share duplicate identity (retry returns original, no conflict); `applied_at` participates in `record_hash` but not in identity.
- **A4 boundary test** (§7): recording an application performs no domain-state mutation beyond persisting the receipt; `applied_by` is an opaque DID with no attached authority/capability check; no "authorized/executed/verified/rolled-back" claim in the type or its serialization.
- **Idempotence / replay test:** a logical application replayed on a second node converges on the original receipt (original stamp, original hash).
- **Privacy grep:** no applied-result body / plan body / operation / target / effect text in any serialized receipt or fixture — fingerprints only.
- **No-overclaim grep:** no "mutation applied-and-verified / plan applied-and-authorized / production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2" claims introduced.
- **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim.
- **Protected close-keyword grep:** the implementation PR carries no closing keyword (fix / close / resolve) adjacent to a protected issue number (#1748, #2141, #2041) — use `Refs` only.

## 11. Deferred work (explicitly out of scope of this rung and its future implementation)

- `EvidencePacketProducedReceipt` — a runtime evidence-packet producer.
- Any verifiable-effect binding, downstream-state addressing, typed/kernel-readable result/effect model, `target_ref`/`effect_ref`, or application-kind taxonomy.
- Any mutation-application, execution, verification, or rollback **engine** — the receipt witnesses a *reported* application; it never performs, verifies, or reverses one.
- Member-shell / process-evidence rendering of `MutationAppliedReceipt` (a later separately-scoped fixture-only surface may add it after the receipt lands, as #2305 did for `MutationPlanRecordedReceipt`).
- Action-card triggers (ADR-0027 / #1713).
- The actual **#2041** human/AT pass — parked for a real human-testing phase.
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274), service hosting, K3s / DNS / Forgejo.

## 12. Non-goals

Restated from #2308 / the #2307 contract — this rung and its future implementation are:

- not `EvidencePacketProducedReceipt`; not an evidence-packet producer;
- not a mutation-application engine; not applying, executing, verifying, or rolling back any plan;
- not an action-card trigger; not a general workflow engine; not a policy/authority engine; not a new authorization semantic;
- not a typed/kernel-readable mutation operation/result/effect model; not an application-kind taxonomy; not a verifiable-effect binding;
- not a new `ProcessGateKind`; not an `ActivationRequest` object;
- not OpenAPI / SDK / served-schema work; not member-shell implementation;
- not #2041 completion; not human/AT execution; not #1748 or #2141 closure;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion;
- not proposal / vote / quorum / mandate / outcome semantics.

Receipts record institutional facts. They grant zero authority.

## 13. Implementation sequencing & protected issue state

**Recommendation (matching the plan lane cadence #2300 → #2302 → #2303):** with this decision rung landed on top of the #2307 contract, a contract-conformant implementation PR may add the `MutationAppliedReceipt` class **only**, per §8–§10. The implementation PR must keep #1748 / #2141 / #2041 open unless separately reviewed, and must leave its own issue open for maintainer disposition rather than auto-closing it by side effect.

Protected issue state at authoring: #2306 closed/completed (design contract); #2304 closed/completed (plan render); #1748 open; #2141 open; #2041 open/parked; #2289 closed; #2081 / #2080 / #2274 open/untouched.

## 14. Related

Refs #2308.
Refs #2306.
Refs #2307.
Refs #2305.
Refs #2303.
Refs #1748.
Refs #2141.
Refs #2041.
