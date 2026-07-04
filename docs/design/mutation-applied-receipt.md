# MutationAppliedReceipt — Design/Audit Contract

**Status:** draft — design/audit
**Truth class:** descriptive
**Canonical:** no
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `6d5df598` (re-verify before relying on exact line numbers or hashes — they drift)
**Related:** #2306 (this contract's issue), #1748 (Institutional Process Substrate), #2141 (vertical institutional spine), #2041 (human/AT pass — open/parked), #2303 (`MutationPlanRecordedReceipt` implementation), #2305 (mutation-plan render in the process-evidence member-shell demo), #2300 (`MutationPlanRecordedReceipt` design contract), #2302 (its M1/M2/M3 decision rung), ADR-0026 (receipt & provenance proof envelope), ADR-0027 (action cards)

> This is the **design/audit contract** #2306 requires before any implementation. It scopes a candidate `MutationAppliedReceipt` as the next narrow process-transition receipt rung under #1748 / #2141 — the receipt that would witness that a previously recorded **mutation plan was applied**, recorded **after** a `MutationPlanRecordedReceipt` for the same session.
>
> **This document adds no runtime code and asserts no implementation.** `MutationAppliedReceipt` does not exist anywhere in the runtime today. This contract audits current state honestly, proposes a candidate contract *subject to implementation proof*, and names the blockers that a narrow decision rung must resolve before an implementation PR can begin. Receipts record facts and grant no authority. This is not a mutation, not a mutation-application engine, not a workflow engine, and not evidence-packet production.

---

## 1. Purpose

The process-transition receipt lane under #1748 / #2141 has now landed **six** runtime classes that make institutional process legible as replayable, hash-anchored evidence:

- `ProcessSessionOpenedReceipt` (anchor);
- `DeliberationEntryRecordedReceipt`;
- `DecisionRecordedReceipt`;
- `ProcessGateResultReceipt`;
- `ActivationCrossedReceipt` (#2296);
- `MutationPlanRecordedReceipt` (#2303, now also rendered read-only in the fixture-only process-evidence member-shell demo per #2305).

The framing spine (`ops/ideas/framing/institutional-process-substrate.md`) orders the substrate as:

```text
preview → deliberation → decision → activation → mutation plan → action cards → receipts → evidence
```

The six landed classes cover *preview → mutation plan*. The next narrow, VM-executable rung is **mutation applied** — a receipt of record that the plan-of-record recorded by a `MutationPlanRecordedReceipt` was *applied*, recorded **after** the plan, so the fact of application is auditable and replayable independently of the effect itself. This document is the design/audit contract for the receipt that would witness the *fact that such an application was recorded*.

It is deliberately **not** a mutation-application engine, **not** a general workflow engine, and **not** evidence-packet production. It is one receipt rung, and — as the audit below shows — like the plan rung before it (#2300 → #2302 → #2303), it needs a narrow **decision rung** of its own before implementation.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `6d5df598`):

- **#2303** — `MutationPlanRecordedReceipt` runtime implementation — **landed** (merged; sixth `ProcessTransitionReceipt` class).
- **#2305** — mutation-plan render in the fixture-only process-evidence member-shell surface (`?mode=demo&set=process-evidence`) — **landed** (merged, `6d5df598`).
- **#2304** — mutation-plan member-shell render — **closed / completed** (by #2305).
- **#2301 / #2300** — `MutationPlanRecordedReceipt` decision rung + design contract — **closed / merged**.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Current repo audit

Classification of every mutation-plan / mutation-applied / evidence-packet term, read against `origin/main` @ `6d5df598`:

| Term | State | Where |
|------|-------|-------|
| `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` / `MutationPlanRecordedReceipt` | **runtime (Rust)** | `icn/crates/icn-governance/src/proof.rs`; wired in `icn/apps/governance/{manager,receipt_backend}`; per-class `*_runtime_slice.rs` tests. These six are the only runtime `ProcessTransitionReceipt`s. |
| `MutationPlanRecordedReceipt` (the receipt this one would follow) | **runtime (Rust), landed #2303** | `proof.rs` (`MutationPlanRecordedReceipt`, `DOMAIN_TAG = icn:gov:mutation_plan_recorded:v1`, `compute_record_hash`); `manager.rs` (`record_mutation_plan_recorded`, `MutationPlanRecordedOutcome`); `receipt_backend.rs` (class `"mutation_plan_recorded"`, `put/get_mutation_plan_recorded`, injective `mutation_plan_recorded_composite_key1(domain_id, session_id)` / `key2 = plan_id`). Fields: `domain_id, session_id, plan_id, activation_id, activation_record_hash, recorded_by, body_hash, recorded_at, record_hash`; `record_hash` is the sole `PartialEq`/`Eq` anchor. It already references the activation (`activation_id` + `activation_record_hash`, verified fail-closed), which itself binds the decision + gate basis — so a receipt referencing the plan inherits activation → decision → gate **transitively**. |
| `MutationAppliedReceipt` | **docs/framing only — no runtime seam** | framing (`institutional-process-substrate.md`), dogfood MVP, STATE/PHASE_PROGRESS, the mutation-plan design docs' §13 deferred-work. **Audit found no Rust struct, tag, manager method, backend class constant, route, fixture, or test** (`rg "MutationAppliedReceipt|mutation_applied" icn/crates icn/apps` ⇒ no match). It is a named candidate with no seam — exactly the position `MutationPlanRecordedReceipt` was in before #2303. |
| `MutationPlan` (the planned artifact the plan receipt witnessed) | **docs/framing only** | framing §"MutationPlan"; no Rust type. Its body is fingerprinted (`body_hash`) by the plan receipt, never stored. |
| `EvidencePacketProducedReceipt` | **docs/framing only** | framing/dogfood/STATE. No Rust process class. **Note:** `icn/crates/icn-baseline-lock/src/evidence.rs` defines a separate `EvidencePacket` — a baseline-lock evidence/test bundle with its own type, **not** a governance `ProcessTransitionReceipt` and **not** prior art for this lane. **Out of scope here (§13).** |

**Honest bottom line:** the six landed classes are the only `ProcessTransitionReceipt` runtime types. `MutationAppliedReceipt` — this contract's subject — is a named candidate with **no runtime seam**. This class is entirely seam-discovery work, exactly as `MutationPlanRecordedReceipt` was before #2303.

### 3.1 The pattern the six landed classes share (what a seventh class would mirror)

- `#[derive(Clone, Debug, Serialize, Deserialize)]`; hand-written `PartialEq`/`Eq` anchored **only** to `record_hash`.
- A `DOMAIN_TAG` const following `icn:gov:<class_snake_case>:v1`, hashed **first**, required to be disjoint from every other tag.
- Anchor is always the `(domain_id, session_id)` pair — `session_id` is meaningful only with `domain_id`; a receipt requires the session to have been opened first (`ProcessSessionOpenedReceipt` precondition).
- A caller-opaque per-item id (`entry_id` / `decision_id` / `activation_id` / `plan_id`).
- `recorded_by` / `crossed_by` / `author`: a DID string, **actor evidence that grants zero authority** ("recorder, not decider/crosser/applier").
- `recorded_at: u64` (Unix seconds), hashed into `record_hash` **but excluded from duplicate identity** — a retry never restamps.
- `body_hash: Hash` (Deliberation/Decision/Plan): a caller-supplied 32-byte fingerprint; **the body is never stored**.
- `record_hash: Hash`: canonical blake3 over `DOMAIN_TAG` + length-prefixed variable-length strings + explicit-`u8` enum ordinals (if any) + raw fixed-size 32-byte hashes + `recorded_at.to_le_bytes()`.
- Uniqueness/idempotence via the `put_opaque_if_absent` backend primitive; duplicate identity is the *stable* fields only (`recorded_at`/`record_hash` excluded).
- **Inter-receipt reference (since #2296/#2303):** `ActivationCrossedReceipt` established the lane's **first** inter-receipt link (activation → decision, by `decision_id` + `decision_record_hash`, verified fail-closed — the #2295 B1 decision). `MutationPlanRecordedReceipt` established the **second** (plan → activation, by `activation_id` + `activation_record_hash`, verified fail-closed — the #2302 M1 decision). A seventh class linking to the plan would be the lane's **third** such reference and would mirror that verified-not-asserted posture.

## 4. Problem statement

A mutation plan can be **recorded** today (`MutationPlanRecordedReceipt`, #2303): a plan-of-record, fingerprinted by `body_hash`, referencing the activation it follows. But there is not yet a narrow, receipt-backed **record that the plan was applied** — recorded after the plan, so the fact of application is auditable and replayable independently of any effect.

Framing (`institutional-process-substrate.md`) states the ordering directly:

> *"a decision is not a mutation. A decision authorizes a mutation; a mutation plan describes one; an activation request crosses the boundary; only then does runtime mutate."*

For this dogfood slice, a **mutation applied** is a local/dev/fixture institutional fact: *an application-of-record was recorded against a mutation plan, after the plan was recorded.* The receipt witnesses that the application was recorded — **not** the applied effect's contents, **not** that the effect is valid, safe, or complete, and **not** that the receipt itself executed or authorized anything. What "applied" is permitted to assert without becoming an execution engine is the central open question (§14, blocker **A4**).

This is **not** production mutation, service deployment, pilot readiness, or an execution engine. It records a *receipt of the application fact* and nothing else; it mutates no domain state and grants no authority.

## 5. Mutation-applied boundary definition (for this slice)

> For this dogfood slice, **mutation applied** means: an app-side apply step reported that the `MutationPlan` recorded by a `MutationPlanRecordedReceipt` (for the same session) was applied, and that *fact* was recorded — after the plan, with the applied result fingerprinted (never stored). The receipt records only the *fact and fingerprint* of that application step.
>
> It is **not** the execution of the mutation, **not** an authorization to apply, **not** a validation that the effect is correct or complete, and **not** a kernel-readable effect payload. Recording an application produces a *receipt of the application* and nothing else; it mutates no domain state and grants no authority. **The receipt does not itself apply, execute, validate, authorize, or roll back anything** — an app-side actor performs (or claims to have performed) the application and asks the substrate to witness that fact. Whether the application was legitimate or its effect real is upstream of this type.

## 6. Proposed receipt contract (candidate — subject to implementation proof)

Candidate tag: `icn:gov:mutation_applied:v1` (must hash-separate from all existing tags, and **must never converge** with `icn:gov:mutation_plan_recorded:v1`, `icn:gov:activation_crossed:v1`, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, or the proposal/vote `icn:gov:decision:v1/v2/v3` lineage).

Candidate fields (naming follows the landed classes; anything marked **OPEN** is a blocker deferred to §14's decision rung, not an asserted field):

| Field | Type | Notes |
|-------|------|-------|
| `domain_id` | `String` | anchor half; hashed length-prefixed |
| `session_id` | `String` | anchor half; session must be opened first (precondition) |
| `application_id` | `String` | caller-opaque per-application id (mirrors `plan_id`/`activation_id`); the `key2` uniqueness half |
| `plan_id` | `String` | **the plan this application follows** — caller-opaque handle (lane's third inter-receipt reference; see **A1**) |
| `plan_record_hash` | `Hash` (32) | content-addressed `record_hash` of the `MutationPlanRecordedReceipt` this application follows; the cryptographic proof link (**A1**) |
| `applied_by` | `String` (DID) | actor evidence — the recorder/apply-witness of the application, **not** an authority to apply or act ("recorder, not applier"); grants zero authority |
| `result_hash` | `Hash` (32) | caller-supplied fingerprint of the applied result/effect body; **the result body is never stored** (**A2**) |
| `applied_at` | `u64` | caller-supplied Unix seconds; hashed into `record_hash`, **excluded** from duplicate identity (**A3**) |
| `record_hash` | `Hash` (32) | canonical blake3 per §3.1 hashing discipline; the sole `PartialEq`/`Eq` anchor |

**Candidate domain tag:** `icn:gov:mutation_applied:v1`.

**Candidate canonical hashing:** `DOMAIN_TAG` first → length-prefixed `domain_id`, `session_id`, `application_id`, `plan_id`, `applied_by` → `plan_record_hash` raw 32 (no length prefix) → `result_hash` raw 32 (no length prefix) → `applied_at.to_le_bytes()`. Exact layout is fixed by the implementation PR and pinned by a golden vector (§12) — **and only after the hash-participating blockers A1/A2/A4 are resolved by the decision rung (§14).**

**Candidate stable duplicate identity:** `(domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash)`. `applied_at` and `record_hash` are **excluded** (retry never restamps).

**Deliberately absent (must never appear in v1):**

- no kernel-readable operation list, target object list, effect payload, or applied-result body (the result **body** is fingerprinted, never stored — meaning firewall);
- no typed operation/result/effect model and no result-kind taxonomy (**A2**);
- no authority grant, capability, mandate, or token; no "this application was authorized/valid" assertion;
- no rollback, compensation, or re-apply semantics; no execution trigger;
- no re-reference of `activation_id`/`activation_record_hash`/`decision_id`/`decision_record_hash`/`gate_basis` (inherited transitively through the plan → activation — see §8);
- no proposal/vote/tally/quorum/outcome semantics;
- no `EvidencePacketProducedReceipt` fields (§13);
- no stored plan/decision/deliberation/result **body** (fingerprints only).

**Session precondition & duplicate semantics (candidate):** identical to the landed classes — the `(domain_id, session_id)` session must be opened first (fail-closed otherwise); at most one application per `(domain_id, session_id, application_id)`; a same-identity retry returns the **original** receipt un-restamped; a different `plan_id`/`plan_record_hash`/`applied_by`/`result_hash` for the same identity is a fail-closed conflict (`mutation_applied_conflict`, mirroring `mutation_plan_recorded_conflict`).

## 7. ADR-0026 envelope usage

`MutationAppliedReceipt` should sit where the other six landed process classes sit: **ADR-0026 Layer 2**, as a self-contained record carrying its own canonical blake3 `record_hash`.

**Honest layering caveat the implementation PR must respect (unchanged from the plan/activation contracts):** ADR-0026's *written* Layer-2 model (`ArtifactReceipt` wrapping a signed, merkle-rooted Layer-1 `GovernanceProof`) predates the process-transition classes. Those classes reuse the Layer-2 *slot* but use a lighter model — a self-hashed blake3 `record_hash`, **no signature, no merkle root**. This contract does **not** claim the applied receipt inherits the signed-proof envelope; it inherits the *process-transition* discipline (self-contained record hash, opaque-store persistence). Any future signature/merkle upgrade is out of scope here and would be an ADR-0026 revision, not a receipt rung.

## 8. Links and provenance

How the receipt would link back (design-level):

- **process/session** — via the `(domain_id, session_id)` anchor (existing pattern; no new seam).
- **plan** — via `plan_id` + `plan_record_hash` (**A1**): naming the `MutationPlanRecordedReceipt` this application follows. This is the lane's **third** inter-receipt reference; it mirrors the verified-not-asserted posture the #2302 M1 decision set for the plan → activation link (the referenced `MutationPlanRecordedReceipt` must exist in the same session, and its `plan_id` must match, resolved via `get_mutation_plan_recorded(domain_id, session_id, plan_id)` and compared on `record_hash`).
- **activation / decision / gate basis** — inherited **transitively** through the plan → activation chain (the plan binds the activation, which binds `decision_id`, `decision_record_hash`, and `gate_basis`). The application does **not** re-reference the activation, decision, or gates directly in `:v1` (see **A1**); the plan link is the single upstream anchor.
- **proof/envelope metadata** — the receipt's own `record_hash` is the provenance pointer; persistence and retrieval go through the same opaque receipt store as the other six landed classes.

## 9. Idempotence and replay

Design requirements (mechanism already exists for the landed classes; this class would reuse it):

- Emission goes through the backend primitive **`put_opaque_if_absent`** (`GovernanceReceiptBackend`; production impl in `icn-gateway`'s `ReceiptStore`, atomic within one sled transaction). `None` returned ⇒ this write won; `Some(existing)` ⇒ hydrate and return the **original** persisted receipt — **never re-stamp**.
- The uniqueness marker is keyed on `(class, key1, key2)` where `key1` is an **injective** netstring-style composite of `(domain_id, session_id)` and `key2` is `application_id`. Injectivity must be tested (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` must never mix).
- Stable duplicate identity is `(domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash)`. Same-identity retry ⇒ idempotent return of the original; a different value for any identity field for the same key ⇒ **fail-closed conflict** (e.g. `mutation_applied_conflict`, mirroring `mutation_plan_recorded_conflict`).
- Concurrent duplicate records must serialize to exactly one winner; losers observe the winner.
- **Timestamp doctrine (blocker A3, but the invariant holds regardless):** a timestamp may live in `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity — so two nodes replaying the same logical application converge on the original receipt (original timestamp, original hash) rather than minting divergent wall-clock hashes. Local wall-clock must **not** be an input to any cross-node-deterministic *identity* (per #2283/#2284). The `plan_record_hash` link is a content-addressed hash, so it introduces no node-local nondeterminism.

## 10. Privacy boundary

- **No applied-result body text, operation list, target object list, or effect payload** in the receipt — a caller-supplied `result_hash` fingerprints the applied result/effect body; the body itself is **never stored** (exactly as Deliberation/Decision/Plan store `body_hash` and never the body).
- The kernel never reads the applied effect semantically; the receipt carries no kernel-readable result content (meaning-firewall discipline).
- Only hashes, opaque ids, DIDs, and repo-safe metadata are carried.
- Any private content behind an applied result stays fixture-safe or redacted; the receipt proves an application was recorded, not that all audiences may read its effect.
- A future evidence/export summary of an applied receipt must be a **repo-safe fixture summary** (the #2289/#2291/#2305 pattern: `record_hash`/`result_hash`/`plan_record_hash` proof pointers with redaction reasons, never private text).

## 11. Authority non-claim

Recording a mutation application records an **institutional fact and grants zero authority.** `applied_by` is the recorder/apply-witness of the application — recorder evidence, not an authority to apply, to act, or to validate anything. A recorded application is **not** proof the effect is correct or complete, not an approval, not a mandate, not a capability, and not a kernel-enforced permission. **The receipt does not execute, authorize, validate, or roll back the mutation.** Whether the application was legitimate is a charter/gate/authority question strictly upstream of this type. The receipt witnesses "an application was recorded here," nothing more.

## 12. Validation plan (for the future implementation PR)

The implementation PR must include **both** test tiers the landed classes use:

1. **`proof.rs` unit tests:** a **golden vector** pinning the v1 `record_hash` of a fixed sample; a **determinism** test (same inputs ⇒ same hash); a **per-field** test (every field change — including `plan_record_hash` and `result_hash` — ⇒ different hash); and a **tag-disjointness** test asserting `icn:gov:mutation_applied:v1` never collides with — and carries a comment that it must never converge with — `mutation_plan_recorded`, `activation_crossed`, `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage.
2. **Runtime-slice integration test** (mirror `mutation_plan_recorded_receipt_runtime_slice.rs`): emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns original, never restamped; different-`plan_id`/`plan_record_hash`/`applied_by`/`result_hash` conflicts fail closed; unopened-session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (no aliasing); two domains sharing a `session_id` never mix.
3. **Plan-reference precondition test:** the referenced `MutationPlanRecordedReceipt` (by `plan_record_hash`) must exist in the **same** `(domain_id, session_id)` and its `plan_id` must match; an absent, wrong-session, wrong-domain, or `plan_id`-mismatched reference is refused fail-closed and persists nothing (mirroring the #2302 M1 verified-not-asserted test).
4. **Privacy grep:** no applied-result body / operation list / target / effect text in any serialized receipt or fixture — fingerprints only.
5. **No-overclaim grep:** no "production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2" claims introduced by the change.
6. **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim (§7).
7. **Protected close-keyword grep:** the implementation PR carries no closing keyword adjacent to a protected issue number — `Refs` only.

## 13. Deferred work (explicitly out of scope of this contract and its future implementation)

- `EvidencePacketProducedReceipt` — a runtime evidence-packet producer. This contract stops strictly at *application recorded*; producing an evidence packet from an application (or a plan) is a separate, later rung.
- Any typed, kernel-readable applied-result/effect model, `effect_ref`/`target_ref` on the receipt, or verifiable-effect binding beyond a `result_hash` fingerprint (see §14 A2/A4).
- Any mutation-application **engine** — code that actually performs, validates, or rolls back a mutation. The receipt witnesses a *reported* application; it never executes one.
- The actual **#2041** human/AT pass (screen-reader / low-vision / switch / AT-compat) — parked for a real human-testing phase.
- Member-shell / process-evidence rendering of `MutationAppliedReceipt` (§15).
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- Action-card triggers (ADR-0027 / #1713); entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274); service hosting, K3s/DNS/Forgejo.

## 14. Implementation sequencing

Implementation **cannot begin from this contract alone.** Mirroring the plan lane (#2300 contract → #2302 decision rung → #2303 implementation), the blockers below have no existing seam and should be resolved by a **narrow decision rung** (a sibling decision doc, in the `mutation-plan-recorded-receipt-decision-rung.md` cadence) before an implementation PR. **Runtime implementation must not begin while any hash-participating blocker (A1, A2, A4) remains unresolved**, because each can change the `:v1` field set and therefore the pinned `record_hash`.

- **A1 — plan → application reference posture (hash-participating).** Does `MutationAppliedReceipt` name the plan it follows by `plan_id`, by `plan_record_hash`, or both? *(This contract's candidate: **both**, verified fail-closed, mirroring the #2302 M1 decision.)* And: is the plan link sufficient, or must the application also directly reference the activation/decision/gate? *(Candidate: plan link only; activation + decision + gate basis are inherited transitively through the plan → activation chain.)*
- **A2 — application body/result representation (hash-participating).** Is the receipt `result_hash`-only (like plan/deliberation/decision `body_hash`), or does it carry a typed minimal result/effect model? *(Candidate: **`result_hash`-only** for `:v1` — preserves the meaning firewall and privacy; the applied-result body and any typed effect model stay app-side and are not stored by the receipt.)* And: is a result-kind taxonomy (à la `DeliberationEntryKind`/`ProcessGateKind`) needed? *(Candidate: **no kind** in `:v1`.)* And: should the field be named `result_hash`, `body_hash`, or `effect_hash`? *(Candidate: `result_hash` — distinguishes the applied result from the plan's `body_hash` at the type level; the decision rung must pin the name before the golden vector.)*
- **A3 — applied timestamp source.** Caller-supplied `applied_at` excluded from identity (current receipt pattern), consistent with the #2302 M3 decision. *(Candidate: **single `applied_at`**, hashed, excluded from identity; no distinct `executed_at`; no time derived from the plan.)*
- **A4 — "applied" witness boundary vs execution/authority boundary (hash-participating, the new question).** What must the runtime require to legitimately record an application, without becoming an execution engine? *(Candidate: `:v1` witnesses a **reported** application — an app-side actor attests it applied the plan and supplies a `result_hash`; the receipt does not verify the effect, does not execute, and does not authorize.)* The open decision: must "applied" bind a *verifiable* effect (e.g. a downstream artifact/state hash the substrate can later re-check), or is a caller-supplied `result_hash` fingerprint sufficient for `:v1`? If a verifiable-effect binding is required, the field set changes (A2/A4 interact), which is precisely why this is deferred to the decision rung rather than pinned here.

**Recommendation (Option C, matching the plan lane cadence):** land *this* design/audit contract; then a narrow decision doc resolving A1/A2/A3/A4; only then a contract-conformant implementation PR. The implementation PR **must keep #1748 / #2141 / #2041 open** unless separately reviewed, and must leave its issue open for maintainer disposition rather than auto-closing it by side effect.

## 15. Member-shell / evidence-surface follow-up

**Recommendation: defer rendering.** Member-shell rendering is **out of scope** for the design contract, the decision rung, and the first implementation PR. The #2291 / #2305 process-evidence surface is fixture-only and read-only; wiring a real `MutationAppliedReceipt` into it should follow the receipt landing, as a later, separately-scoped fixture-only surface extension (exactly as #2305 did for `MutationPlanRecordedReceipt` after #2303), and must preserve the redaction/privacy discipline (proof pointers only, no applied-result body text) and the doctrine that the receipt records a process fact and grants zero authority.

## 16. Non-goals

Restated from #2306 — this contract and its future implementation are:

- not a mutation-application engine; not applying, executing, validating, or rolling back any plan;
- not `EvidencePacketProducedReceipt`; not an evidence-packet producer;
- not an action-card trigger; not a general workflow engine; not a policy/authority engine;
- not a new `ProcessGateKind`; not new authorization semantics;
- not OpenAPI / SDK / served-schema work; not member-shell implementation;
- not #2041 completion; not human/AT execution;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion;
- not proposal / vote / quorum / mandate / outcome semantics;
- not #2081 / #2080 / #2274; not entity-auth enforcement; not trusted token issuance; not UnknownLegacy repair; not service hosting; not K3s/DNS/Forgejo.

Receipts record institutional facts. They grant zero authority.

## 17. Related

Refs #2306.
Refs #2305.
Refs #2303.
Refs #2302.
Refs #2300.
Refs #1748.
Refs #2141.
Refs #2041.
