# MutationPlanRecordedReceipt — Design/Audit Contract

**Status:** draft — design/audit
**Truth class:** descriptive
**Canonical:** no
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `2652a8d6` (re-verify before relying on exact line numbers or hashes — they drift)
**Related:** #2299 (this contract's issue), #1748 (Institutional Process Substrate), #2141 (vertical institutional spine), #2041 (human/AT pass — open/parked), #2296 (`ActivationCrossedReceipt` implementation), #2298 (activation render in the process-evidence member-shell demo), ADR-0026 (receipt & provenance proof envelope), ADR-0027 (action cards)

> This is the **design/audit contract** #2299 requires before any implementation. It scopes a candidate `MutationPlanRecordedReceipt` as the next narrow process-transition receipt rung under #1748 / #2141 — the receipt that would witness that a **mutation plan was recorded** after an activation crossing, **before any mutation is applied**.
>
> **This document adds no runtime code and asserts no implementation.** `MutationPlanRecordedReceipt` does not exist anywhere in the runtime today. This contract audits current state honestly, proposes a candidate contract *subject to implementation proof*, and names the blockers that a narrow decision rung must resolve before an implementation PR can begin. Receipts record facts and grant no authority. This is not a mutation, not a workflow engine, and not mutation application.

---

## 1. Purpose

The process-transition receipt lane under #1748 / #2141 has landed **five** runtime classes that make institutional process legible as replayable, hash-anchored evidence:

- `ProcessSessionOpenedReceipt` (anchor);
- `DeliberationEntryRecordedReceipt`;
- `DecisionRecordedReceipt`;
- `ProcessGateResultReceipt`;
- `ActivationCrossedReceipt` (#2296, now also rendered read-only in the fixture-only process-evidence member-shell demo per #2298).

The framing spine (`ops/ideas/framing/institutional-process-substrate.md`) orders the substrate as:

```text
preview → deliberation → decision → activation → mutation plan → action cards → receipts → evidence
```

The five landed classes cover *preview → activation*. The next narrow, VM-executable rung is **mutation plan** — the plan-of-record for what runtime should do *as a consequence of* an activation, recorded **before** any mutation is applied. This document is the design/audit contract for the receipt that would witness the *fact that such a plan was recorded*.

It is deliberately **not** mutation application, **not** a general workflow engine, and **not** evidence-packet production. It is one receipt rung, and — as the audit below shows — like the activation rung before it (#2294 → #2295 → #2296), it needs a narrow **decision rung** of its own before implementation.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `2652a8d6`):

- **#2296** — `ActivationCrossedReceipt` runtime implementation — **landed** (merged; fifth `ProcessTransitionReceipt` class).
- **#2298** — activation render in the fixture-only process-evidence member-shell surface (`?mode=demo&set=process-evidence`) — **landed** (merged, `2652a8d6`).
- **#2293** — `ActivationCrossedReceipt` runtime dogfood slice — **closed / completed** (by #2294 contract + #2295 decision rung + #2296 implementation).
- **#2297** — activation member-shell render — **closed / completed** (by #2298).
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Current repo audit

Classification of every mutation-plan / mutation-applied / evidence-packet term, read against `origin/main` @ `2652a8d6`:

| Term | State | Where |
|------|-------|-------|
| `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` | **runtime (Rust)** | `icn/crates/icn-governance/src/proof.rs`; wired in `apps/governance/{manager,receipt_backend}`; per-class `*_runtime_slice.rs` tests. These five are the only runtime `ProcessTransitionReceipt`s. |
| `MutationPlanRecordedReceipt` | **docs/framing only** | framing (`institutional-process-substrate.md` §"Receipt classes for process transitions"), dogfood MVP, `ideas.yaml`, the activation design docs, STATE/PHASE_PROGRESS, dev handoffs. **No Rust struct, tag, manager method, backend class constant, route, or test.** |
| `MutationPlan` (the planned artifact the receipt would witness) | **docs/framing only** | framing §"MutationPlan": *"the plan-of-record for what runtime should do as a consequence of the activation … the kernel does not read the plan semantically … the plan is for human and partner review, audit, and replay."* No Rust type. |
| `PreviewReviewPacket` / `pending_publish_summary` (framing's proposed read-model of a `MutationPlan`) | **docs/framing only** | framing pins *"a `MutationPlan` is preview-shaped: a `PreviewReviewPacket` with `preview_kind = pending_publish_summary` … the plan is the upstream artifact the pending-publish preview renders."* Audit found **no `PreviewReviewPacket` and no `pending_publish_summary` in `icn/crates` or `icn/apps`** — the proposed read-model is itself not-yet-runtime. |
| `ActivationRequest` (gate object) | **docs/framing only** | framing + dogfood MVP. No Rust gate object; the #2295 activation decision rung deliberately reused the closed `ProcessGateKind` and added no `ActivationRequest`. |
| `MutationAppliedReceipt` | **docs/framing only** | framing notes it exists "only in concept" via existing action-item/governance receipt families; no dedicated class. **Out of scope here (§13).** |
| `EvidencePacketProducedReceipt` | **docs/framing only** | framing/dogfood/STATE. No Rust process class. **Note:** `icn-baseline-lock/src/evidence.rs` defines a separate `EvidencePacket` — a baseline-lock evidence/test bundle with its own type, **not** a governance `ProcessTransitionReceipt` and **not** prior art for this lane. |

**Honest bottom line:** the five landed classes are the only `ProcessTransitionReceipt` runtime types. Everything from `mutation plan` onward — including this contract's subject and even the read-model shape (`PreviewReviewPacket`) the framing proposes for it — is a named candidate with **no runtime seam**. This class is entirely seam-discovery work, exactly as `ActivationCrossedReceipt` was before #2296.

### 3.1 The pattern the five landed classes share (what a sixth class would mirror)

- `#[derive(Clone, Debug, Serialize, Deserialize)]`; hand-written `PartialEq`/`Eq` anchored **only** to `record_hash`.
- A `DOMAIN_TAG` const following `icn:gov:<class_snake_case>:v1`, hashed **first**, required to be disjoint from every other tag.
- Anchor is always the `(domain_id, session_id)` pair — `session_id` is meaningful only with `domain_id`; a receipt requires the session to have been opened first (`ProcessSessionOpenedReceipt` precondition).
- A caller-opaque per-item id (`entry_id` / `decision_id` / `activation_id`).
- `recorded_by` / `crossed_by` / `author`: a DID string, **actor evidence that grants zero authority** ("recorder, not decider/crosser").
- `recorded_at: u64` (Unix seconds), hashed into `record_hash` **but excluded from duplicate identity** — a retry never restamps.
- `body_hash: Hash` (Deliberation/Decision): a caller-supplied 32-byte fingerprint; **the body is never stored**.
- `record_hash: Hash`: canonical blake3 over `DOMAIN_TAG` + length-prefixed variable-length strings + explicit-`u8` enum ordinals (if any) + raw fixed-size 32-byte hashes + `recorded_at.to_le_bytes()`.
- Uniqueness/idempotence via the `put_opaque_if_absent` backend primitive; duplicate identity is the *stable* fields only (`recorded_at`/`record_hash` excluded).
- **Inter-receipt reference (new since #2296):** `ActivationCrossedReceipt` established the lane's **first** inter-receipt link — it names the activated decision by **both** the caller-opaque `decision_id` **and** the content-addressed `decision_record_hash`, verified fail-closed against the recorded decision in the same session (the #2295 B1 decision). A sixth class linking to the activation would be the lane's **second** such reference and would mirror that verified-not-asserted posture.

## 4. Problem statement

An activation can be **recorded** today (`ActivationCrossedReceipt`, #2296): a recorded decision, with required gates observed as passed, crossed from decision toward later action planning. But there is not yet a narrow, receipt-backed **plan of record** for *what that later action planning proposes to do* — recorded before anything is applied, so the plan is auditable and replayable independently of any mutation.

Framing (`institutional-process-substrate.md` §"MutationPlan") states the artifact directly:

> *"a decision is not a mutation. A decision authorizes a mutation; a mutation plan describes one; an activation request crosses the boundary; only then does runtime mutate."*

> *"The plan-of-record for what runtime should do as a consequence of the activation. Names the affected objects, the specific operations (create, update, retire, reassign, allocate, settle, install, bind), and the expected receipts. The kernel does not read the plan semantically … The plan is for human and partner review, audit, and replay."*

For this dogfood slice, a **mutation plan recorded** is a local/dev/fixture institutional fact: *a plan-of-record was recorded against an activation crossing, before any mutation is applied.* The receipt witnesses that the plan was recorded — **not** its contents, **not** that it is valid, authorized, or safe to apply, and **not** that any mutation happened. Applying the plan is a strictly later rung (`MutationAppliedReceipt`, §13).

This is **not** production mutation, service deployment, pilot readiness, or mutation application. It records a *receipt of the planning fact* and nothing else; it mutates no domain state and grants no authority.

## 5. Mutation-plan boundary definition (for this slice)

> For this dogfood slice, **mutation plan recorded** means: a plan-of-record (an app-side `MutationPlan` artifact whose body is never stored by the receipt) was recorded **after** an `ActivationCrossedReceipt` for the same session, **before** any mutation is applied. The receipt records only the *fact and fingerprint* of that planning step.
>
> It is **not** mutation application, **not** service deployment, **not** an authorization to act, and **not** a kernel-readable operation list. Recording a plan produces a *receipt of the planning* and nothing else; it mutates no domain state and grants no authority. Whether the plan may ever be applied is a charter/gate/authority question upstream of this type.

## 6. Proposed receipt contract (candidate — subject to implementation proof)

Candidate tag: `icn:gov:mutation_plan_recorded:v1` (must hash-separate from all existing tags, and **must never converge** with `icn:gov:activation_crossed:v1`, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, or the proposal/vote `icn:gov:decision:v1/v2/v3` lineage).

Candidate fields (naming follows the landed classes; anything marked **OPEN** is a blocker deferred to §14's decision rung, not an asserted field):

| Field | Type | Notes |
|-------|------|-------|
| `domain_id` | `String` | anchor half; hashed length-prefixed |
| `session_id` | `String` | anchor half; session must be opened first (precondition) |
| `plan_id` | `String` | caller-opaque per-plan id (mirrors `activation_id`/`decision_id`); the `key2` uniqueness half |
| `activation_id` | `String` | **the activation this plan follows** — caller-opaque handle (lane's second inter-receipt reference; see **M1**) |
| `activation_record_hash` | `Hash` (32) | content-addressed `record_hash` of the `ActivationCrossedReceipt` this plan follows; the cryptographic proof link (**M1**) |
| `recorded_by` | `String` (DID) | actor evidence — the recorder of the plan, **not** an authority to plan or act ("recorder, not planner"); grants zero authority |
| `body_hash` | `Hash` (32) | caller-supplied fingerprint of the `MutationPlan` body; **the plan body is never stored** (**M2**) |
| `recorded_at` | `u64` | caller-supplied Unix seconds; hashed into `record_hash`, **excluded** from duplicate identity (**M3**) |
| `record_hash` | `Hash` (32) | canonical blake3 per §3.1 hashing discipline; the sole `PartialEq`/`Eq` anchor |

**Canonical hashing (candidate):** `DOMAIN_TAG` first → length-prefixed `domain_id`, `session_id`, `plan_id`, `activation_id`, `recorded_by` → `activation_record_hash` raw 32 (no length prefix) → `body_hash` raw 32 (no length prefix) → `recorded_at.to_le_bytes()`. Exact layout is fixed by the implementation PR and pinned by a golden vector (§12).

**Deliberately absent (must never appear in v1):**

- no kernel-readable operation list, target object list, or effect payload (the plan **body** is fingerprinted, never stored — meaning firewall);
- no mutation content, applied-effect, or "what actually changed" — that is `MutationAppliedReceipt` territory (§13);
- no authority grant, capability, mandate, or token; no "this plan may be applied" assertion;
- no proposal/vote/tally/quorum/outcome semantics;
- no new `ProcessGateKind` semantics and no `ActivationRequest` gate object (the #2295 decisions stand);
- no `target_ref` / `effect_ref` (deferred — §14 M2);
- no stored plan/decision/deliberation **body** (fingerprints only).

**Session precondition & duplicate semantics:** identical to the landed classes — the `(domain_id, session_id)` session must be opened first (fail-closed otherwise); at most one plan per `(domain_id, session_id, plan_id)`; a same-identity retry returns the **original** receipt un-restamped; a different `activation_id`/`activation_record_hash`/`recorded_by`/`body_hash` for the same identity is a fail-closed conflict.

## 7. ADR-0026 envelope usage

`MutationPlanRecordedReceipt` should sit where the other six process classes sit: **ADR-0026 Layer 2**, as a self-contained record carrying its own canonical blake3 `record_hash`.

**Honest layering caveat the implementation PR must respect (unchanged from the activation contract):** ADR-0026's *written* Layer-2 model (`ArtifactReceipt` wrapping a signed, merkle-rooted Layer-1 `GovernanceProof`) predates the process-transition classes. Those classes reuse the Layer-2 *slot* but use a lighter model — a self-hashed blake3 `record_hash`, **no signature, no merkle root**. This contract does **not** claim the plan receipt inherits the signed-proof envelope; it inherits the *process-transition* discipline (self-contained record hash, opaque-store persistence). Any future signature/merkle upgrade is out of scope here and would be an ADR-0026 revision, not a receipt rung.

## 8. Links and provenance

How the receipt would link back (design-level):

- **process/session** — via the `(domain_id, session_id)` anchor (existing pattern; no new seam).
- **activation** — via `activation_id` + `activation_record_hash` (**M1**): naming the `ActivationCrossedReceipt` this plan follows. This is the lane's **second** inter-receipt reference; it mirrors the verified-not-asserted posture the #2295 B1 decision set for the activation→decision link (the referenced `ActivationCrossedReceipt` must exist in the same session, and its `activation_id` must match).
- **decision / gate basis** — inherited **transitively** through the activation (the `ActivationCrossedReceipt` already binds `decision_id`, `decision_record_hash`, and `gate_basis`). The plan does **not** re-reference the decision or gates directly in `:v1` (see **M1**); the activation link is the single upstream anchor.
- **proof/envelope metadata** — the receipt's own `record_hash` is the provenance pointer; persistence and retrieval go through the same opaque receipt store as the other six.

## 9. Idempotence and replay

Design requirements (mechanism already exists for the landed classes; this class would reuse it):

- Emission goes through the backend primitive **`put_opaque_if_absent`** (`GovernanceReceiptBackend`; production impl in `icn-gateway`'s `ReceiptStore`, atomic within one sled transaction). `None` returned ⇒ this write won; `Some(existing)` ⇒ hydrate and return the **original** persisted receipt — **never re-stamp**.
- The uniqueness marker is keyed on `(class, key1, key2)` where `key1` is an **injective** netstring-style composite of `(domain_id, session_id)` and `key2` is `plan_id`. Injectivity must be tested (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` must never mix).
- Stable duplicate identity is `(domain_id, session_id, plan_id, activation_id, activation_record_hash, recorded_by, body_hash)`. Same-identity retry ⇒ idempotent return of the original; a different value for any identity field for the same key ⇒ **fail-closed conflict** (e.g. `mutation_plan_recorded_conflict`, mirroring `activation_crossed_conflict`).
- Concurrent duplicate records must serialize to exactly one winner; losers observe the winner.
- **Timestamp doctrine (blocker M3, but the invariant holds regardless):** a timestamp may live in `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity — so two nodes replaying the same logical plan converge on the original receipt (original timestamp, original hash) rather than minting divergent wall-clock hashes. Local wall-clock must **not** be an input to any cross-node-deterministic *identity* (per #2283/#2284). The `activation_record_hash` link is a content-addressed hash, so it introduces no node-local nondeterminism.

## 10. Privacy boundary

- **No plan body text, operation list, target object list, or effect payload** in the receipt — a caller-supplied `body_hash` fingerprints the `MutationPlan` body; the body itself is **never stored** (exactly as Deliberation/Decision store `body_hash` and never the body).
- The kernel never reads the plan semantically; the receipt carries no kernel-readable operation content (meaning-firewall discipline).
- Only hashes, opaque ids, DIDs, and repo-safe metadata are carried.
- Any private content behind a `MutationPlan` stays fixture-safe or redacted; the receipt proves a plan was recorded, not that all audiences may read its contents.
- A future evidence/export summary of a plan receipt must be a **repo-safe fixture summary** (the #2289/#2291 pattern: `record_hash`/`body_hash` proof pointers with redaction reasons, never private text).

## 11. Authority non-claim

Recording a mutation plan records an **institutional fact and grants zero authority.** `recorded_by` is the recorder of the plan — recorder evidence, not an authority to plan, to act, or to apply anything. A recorded plan is **not** an approval to apply it, not a mandate, not a capability, and not a kernel-enforced permission. Whether the plan may ever be applied is a charter/gate/authority question strictly upstream of this type and deferred to later rungs. The receipt witnesses "a plan was recorded here," nothing more.

## 12. Validation plan (for the future implementation PR)

The implementation PR must include **both** test tiers the landed classes use:

1. **`proof.rs` unit tests:** a **golden vector** pinning the v1 `record_hash` of a fixed sample; a **determinism** test (same inputs ⇒ same hash); a **per-field** test (every field change — including `activation_record_hash` and `body_hash` — ⇒ different hash); and a **tag-disjointness** test asserting `icn:gov:mutation_plan_recorded:v1` never collides with — and carries a comment that it must never converge with — `activation_crossed`, `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage.
2. **Runtime-slice integration test** (mirror `activation_crossed_receipt_runtime_slice.rs`): emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns original, never restamped; different-`activation_id`/`activation_record_hash`/`recorded_by`/`body_hash` conflicts fail closed; unopened-session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (no aliasing); two domains sharing a `session_id` never mix.
3. **Activation-reference precondition test:** the referenced `ActivationCrossedReceipt` (by `activation_record_hash`) must exist in the **same** `(domain_id, session_id)` and its `activation_id` must match; an absent, wrong-session, wrong-domain, or `activation_id`-mismatched reference is refused fail-closed and persists nothing (mirroring the #2295 B1 verified-not-asserted test).
4. **Privacy grep:** no plan body / operation list / target / effect text in any serialized receipt or fixture — fingerprints only.
5. **No-overclaim grep:** no "mutation applied / plan applied / production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2" claims introduced by the change.
6. **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim (§7).
7. **Protected close-keyword grep:** the implementation PR carries no closing keyword adjacent to a protected issue number — `Refs` only.

## 13. Deferred work (explicitly out of scope of this contract and its future implementation)

- `MutationAppliedReceipt` — the receipt that would witness a mutation actually applied (the plan *executed*, with real effects). This contract stops strictly at *plan recorded*; applying a plan, and any receipt of application, is a separate, later rung.
- `EvidencePacketProducedReceipt` — a runtime evidence-packet producer.
- Any typed, kernel-readable `MutationPlan` operation model, `target_ref`/`effect_ref` on the receipt, or `PreviewReviewPacket` runtime type (see §14 M2).
- The actual **#2041** human/AT pass (screen-reader / low-vision / switch / AT-compat) — parked for a real human-testing phase.
- Member-shell / process-evidence rendering of `MutationPlanRecordedReceipt` (§15).
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- Action-card triggers (ADR-0027 / #1713); entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274); service hosting, K3s/DNS/Forgejo.

## 14. Implementation sequencing

Implementation **cannot begin from this contract alone.** Mirroring the activation lane (#2294 contract → #2295 decision rung → #2296 implementation), the blockers below have no existing seam and should be resolved by a **narrow decision rung** (a sibling decision doc, in the `activation-crossed-receipt-decision-rung.md` cadence) before an implementation PR:

- **M1 — plan → activation reference posture.** Does `MutationPlanRecordedReceipt` name the activation it follows by `activation_id`, by `activation_record_hash`, or both? *(This contract's candidate: **both**, verified fail-closed, mirroring the #2295 B1 decision.)* And: is the activation link sufficient, or must the plan also directly reference the decision and/or gate basis? *(Candidate: activation link only; decision + gate basis are inherited transitively through the activation.)*
- **M2 — plan-body representation.** Is the receipt `body_hash`-only (like deliberation/decision), or does it carry a typed minimal operation/target model? *(Candidate: **`body_hash`-only** for `:v1` — preserves the meaning firewall and privacy; the plan body and any `PreviewReviewPacket`/typed-operation model stay app-side and are not stored by the receipt.)* And: is a plan-kind taxonomy (à la `DeliberationEntryKind`/`ProcessGateKind`) needed? *(Candidate: **no kind** in `:v1`.)*
- **M3 — plan timestamp source.** Caller-supplied `recorded_at` excluded from identity (current receipt pattern), consistent with the #2295 B3 decision. *(Candidate: **single `recorded_at`**, hashed, excluded from identity; no distinct `planned_at`.)*

**Recommendation (Option C, matching the ActivationCrossed lane cadence):** land *this* design/audit contract; then a narrow decision doc resolving M1/M2/M3; only then a contract-conformant implementation PR. The implementation PR **must keep #1748 / #2141 / #2041 open** unless separately reviewed, and must leave its issue open for maintainer disposition rather than auto-closing it by side effect.

## 15. Member-shell / evidence-surface follow-up

**Recommendation: defer rendering.** Member-shell rendering is **out of scope** for the design contract, the decision rung, and the first implementation PR. The #2291 / #2298 process-evidence surface is fixture-only and read-only; wiring a real `MutationPlanRecordedReceipt` into it should follow the receipt landing, as a later, separately-scoped fixture-only surface extension (exactly as #2298 did for `ActivationCrossedReceipt` after #2296), and must preserve the redaction/privacy discipline (proof pointers only, no plan body text) and the doctrine that the receipt records a process fact and grants zero authority.

## 16. Non-goals

Restated from #2299 — this contract and its future implementation are:

- not `MutationAppliedReceipt`; not mutation application; not applying any plan;
- not `EvidencePacketProducedReceipt`; not an evidence-packet producer;
- not an action-card trigger; not a general workflow engine; not a policy/authority engine;
- not a new `ProcessGateKind`; not an `ActivationRequest` object; not new authorization semantics;
- not OpenAPI / SDK / served-schema work; not member-shell implementation;
- not #2041 completion; not human/AT execution;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion;
- not proposal / vote / quorum / mandate / outcome semantics;
- not #2081 / #2080 / #2274; not entity-auth enforcement; not trusted token issuance; not UnknownLegacy repair; not service hosting; not K3s/DNS/Forgejo.

Receipts record institutional facts. They grant zero authority.

## 17. Related

Refs #2299.
Refs #1748.
Refs #2141.
Refs #2041.
Refs #2296.
Refs #2298.
