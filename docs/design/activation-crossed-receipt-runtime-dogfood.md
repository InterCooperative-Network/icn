# ActivationCrossedReceipt — Design/Audit Contract

**Status:** draft — design/audit
**Truth class:** descriptive
**Canonical:** no
**Last reviewed:** 2026-07-03
**Source basis:** read against `origin/main` @ `0f4fa895` (re-verify before relying on exact line numbers or hashes — they drift)
**Related:** #2293 (this contract's issue), #1748 (Institutional Process Substrate), #2141 (vertical institutional spine), #2041 (human/AT pass — open/parked), #2291 (process-evidence surface), #2292 (human/AT packet), ADR-0026 (receipt & provenance proof envelope)

> This is the **design contract** #2293 requires before any implementation. It scopes a candidate `ActivationCrossedReceipt` as the next narrow process-transition receipt rung under #1748 / #2141 — the receipt that would witness that an already-recorded decision **crossed the activation boundary** (the spine's "boundary between deciding and doing") before any later mutation/evidence work.
>
> **This document adds no runtime code and asserts no implementation.** `ActivationCrossedReceipt` does not exist anywhere in the runtime today. This contract audits current state honestly, proposes a candidate contract *subject to implementation proof*, and names the blockers that must be resolved by a narrow decision rung before an implementation PR can begin. Receipts record facts and grant no authority.

---

## 1. Purpose

The process-transition receipt lane under #1748 / #2141 has landed four classes that make institutional process legible as replayable, hash-anchored evidence:

- `ProcessSessionOpenedReceipt` (anchor);
- `DeliberationEntryRecordedReceipt`;
- `DecisionRecordedReceipt`;
- `ProcessGateResultReceipt`.

The framing spine (`ops/ideas/framing/institutional-process-substrate.md`) orders the substrate as:

```text
preview → deliberation → decision → activation → mutation plan → action cards → receipts → evidence
```

The four landed classes cover *preview → decision*. The next narrow, VM-executable rung is **activation** — the explicit boundary a recorded decision crosses on its way toward action, before any mutation is planned or applied. This document is the design contract for the receipt that would witness the *fact of that crossing*.

It is deliberately **not** a general workflow engine, not mutation planning, and not evidence-packet production. It is one receipt rung, and — as the audit below shows — it needs a decision rung of its own before implementation, exactly as `DecisionRecordedReceipt` needed the Q4 decision (`decision-recorded-q4-decision.md`) before #2280–#2282.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `0f4fa895`):

- **#1749** — read-model dogfood slice for the Institutional Process Substrate — **landed** (merged).
- **#2291** — fixture-only process-evidence member-shell surface (`?mode=demo&set=process-evidence`) rendering the four existing receipt classes — **landed** (merged, `b28fbeb2`).
- **#2292** — human/AT validation packet extended to the process-evidence surface — **landed** (merged, `0f4fa895`); the packet was *extended*, not executed.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.
- **#2289** — organizer-steward evidence surface scope — **closed / completed** (by #2290 design + #2291 impl).

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Current repo audit

Classification of every process-transition / activation / mutation / evidence term, read against `origin/main` @ `0f4fa895`:

| Term | State | Where |
|------|-------|-------|
| `ProcessSessionOpenedReceipt` | **runtime (Rust)** | `icn/crates/icn-governance/src/proof.rs`; wired in `apps/governance/{manager,http/handlers,receipt_backend}`; `*_runtime_slice.rs` test |
| `DeliberationEntryRecordedReceipt` | **runtime (Rust)** | `proof.rs` + app wiring + runtime slice |
| `DecisionRecordedReceipt` | **runtime (Rust)** | `proof.rs` + app wiring + runtime slice |
| `ProcessGateResultReceipt` | **runtime (Rust)** | `proof.rs`; wired in `apps/governance`. Note: `icn-baseline-lock/src/receipt_emit.rs` emits `BaselineProcessGateResultReceipt` — a baseline-lock **test stand-in** with a *separate type/domain tag*, **not** production governance emission of this class. |
| `ActivationCrossedReceipt` | **docs/framing only** | framing + dogfood MVP + `decision-recorded-receipt.md` / `-q4-decision.md` (name candidate). **No Rust struct, tag, manager method, backend class constant, route, or test.** |
| `ActivationRequest` (gate object) | **docs/framing only** | framing + dogfood MVP. No Rust gate object, no "activation authority", no "second-screen confirm" primitive. |
| "activation crossed" / "activation boundary" (phrases) | **framing only / pre-contract audit found no seam** | concept expressed in framing as "the boundary between deciding and doing". Before this contract, the repo audit found no `activation boundary` runtime or documented seam outside those framing concepts; this design document — and the registry/index references generated from it — now introduces the design term for #2293. (The no-hit finding is stated as of the pre-contract audit, outside this document, so it stays true after merge.) |
| `MutationPlanRecordedReceipt` | **docs/framing only** | framing/dogfood/STATE. No Rust. |
| `MutationAppliedReceipt` | **docs/framing only** | framing notes it exists "only in concept" via existing action-item/governance receipt families; no dedicated class. |
| `EvidencePacketProducedReceipt` | **docs/framing only** | framing/dogfood/STATE. No Rust; #2289 used a repo-safe *fixture* export summary, not a runtime producer. |

**Honest bottom line:** the four landed classes are the only ProcessTransitionReceipt runtime types. Everything from `activation` onward — including this contract's subject — is a named candidate with **no runtime seam**. This class is entirely seam-discovery work.

### 3.1 The pattern the four landed classes share (what a fifth class would mirror)

- `#[derive(Clone, Debug, Serialize, Deserialize)]`; hand-written `PartialEq`/`Eq` anchored **only** to `record_hash`.
- A `DOMAIN_TAG` const following `icn:gov:<class_snake_case>:v1`, hashed **first**, required to be disjoint from every other tag.
- Anchor is always the `(domain_id, session_id)` pair — `session_id` is meaningful only with `domain_id`; a receipt requires the session to have been opened first (`ProcessSessionOpenedReceipt` precondition).
- A caller-opaque per-item id (`entry_id` / `decision_id`).
- `recorded_by` / `opened_by` / `author`: a DID string, **actor evidence that grants zero authority** ("recorder, not decider").
- `recorded_at: u64` (Unix seconds), hashed into `record_hash` **but excluded from duplicate identity** — a retry never restamps.
- `body_hash: Hash` (Deliberation/Decision only): a caller-supplied 32-byte fingerprint; **the body is never stored**.
- `record_hash: Hash`: canonical blake3 over `DOMAIN_TAG` + length-prefixed variable-length strings + explicit-`u8` enum ordinals + `recorded_at.to_le_bytes()` + any fixed-size `body_hash` (raw, no length prefix).
- Uniqueness/idempotence via the `put_opaque_if_absent` backend primitive (§9); duplicate identity is the *stable* fields only (`domain_id, session_id, item_id` + author/body_hash for conflict detection).
- **No `target_ref` on any class** (Q1 deferred); **no cross-receipt reference** on any class (no receipt names another receipt's `record_hash`/id).

## 4. Problem statement

A human decision can be **recorded** today (`DecisionRecordedReceipt`), gate results can be **recorded** (`ProcessGateResultReceipt`), and the process-evidence surface (#2291) can **render** that evidence read-only. But there is not yet a narrow, receipt-backed **activation boundary**: no evidence object that says *a recorded decision crossed from review-only into "ready to drive action," with the required gates observed as passed.*

Framing (`institutional-process-substrate.md`) states the boundary directly:

> *"a decision is not a mutation. A decision authorizes a mutation; a mutation plan describes one; an activation request crosses the boundary; only then does runtime mutate."*

> *"`ActivationRequest` … declares: a decision has been recorded, the authority to act is established, and the institution is now ready to cross from review-only into mutation. … It is the gate. It can be refused (process gate result fails, accessibility review pending, privacy review pending, charter rule unmet, settlement window not yet open)."*

The dogfood MVP (`institutional-process-substrate-mvp.md`, Step 5) walks activation as **read-model only**: *"activation does not occur in this slice. `ActivationRequest` is sketched, not issued."* It pairs a future `ActivationCrossedReceipt` (name candidate) with the existing envelope, gated behind gate-results all `pass`.

This is **not** production activation, NYCN activation, service deployment, pilot readiness, or mutation application. For this dogfood slice, "activation" is a local/dev/fixture institutional fact: *a recorded decision was accepted as ready to drive a later action-planning step, with its required gates observed as passed.*

## 5. Activation boundary definition (for this slice)

> For this dogfood slice, **activation** means: an already-recorded decision (witnessed by a `DecisionRecordedReceipt`) is accepted as **ready to drive a later action/mutation-planning step**, inside a local/dev/fixture process path, **conditioned on the required `ProcessGateResultReceipt`s observing `pass`**. It is the gate, not the mutation.
>
> Activation in this slice is **not** production activation, **not** service deployment, **not** NYCN launch, **not** mutation planning, and **not** mutation application. Crossing the boundary produces a *receipt of the crossing* and nothing else; it mutates no domain state and grants no authority.

The gate can be **refused** — a design-level requirement, not an implementation claim: if a required gate result is `Fail` or absent, the boundary is *not* crossed and no `ActivationCrossedReceipt` is emitted (fail-closed, mirroring the session-precondition and conflict discipline of the landed classes).

## 6. Proposed receipt contract (candidate — subject to implementation proof)

Candidate tag: `icn:gov:activation_crossed:v1` (must hash-separate from all existing tags, and **must never converge** with `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, or the proposal/vote `icn:gov:decision:v1/v2/v3` lineage).

Candidate fields (naming follows the landed classes; anything marked **OPEN** is a blocker deferred to §11's decision rung, not an asserted field):

| Field | Type | Notes |
|-------|------|-------|
| `domain_id` | `String` | anchor half; hashed length-prefixed |
| `session_id` | `String` | anchor half; session must be opened first (precondition) |
| `activation_id` | `String` | caller-opaque per-activation id (mirrors `decision_id`/`entry_id`); the `key2` uniqueness half |
| `decision_ref` | **OPEN** | *how* this activation names the decision it crosses — see blocker **B1**. A cross-receipt reference (by `decision_id` and/or a decision `record_hash`) would be the **first** inter-receipt link in this lane; no existing pattern supports it. |
| `gate_basis` | **OPEN** | *how* the "gates observed as passed" precondition is witnessed in the receipt — see blocker **B2** (a fingerprint of the required gate-result set? a count? nothing, relying on external query?). |
| `crossed_by` | `String` (DID) | actor evidence — the recorder of the crossing, **not** an authority to act ("recorder, not crosser"); grants zero authority |
| `crossed_at` / `effective_at` | `u64` | **OPEN** — caller-supplied `recorded_at` (current receipt pattern) vs decision-carried effective time — see blocker **B3** |
| `body_hash` | `Hash` (optional) | if an `ActivationRequest` payload is fingerprinted, a caller-supplied 32-byte hash; the request body is **never stored** |
| `record_hash` | `Hash` | canonical blake3 per §3.1 hashing discipline; the sole `PartialEq`/`Eq` anchor |

**Canonical hashing (candidate):** `DOMAIN_TAG` first → length-prefixed `domain_id`, `session_id`, `activation_id`, `crossed_by` (and any resolved `decision_ref` string) → `crossed_at.to_le_bytes()` → any fixed-size `body_hash`/`gate_basis` raw (no length prefix). Exact layout is fixed by the implementation PR and pinned by a golden vector (§12).

**Deliberately absent (must never appear in v1):**

- no mutation content, mutation plan, action-card payload, or applied-effect;
- no proposal/vote/tally/quorum/mandate/outcome semantics;
- no authority grant, capability, or token;
- no new `ProcessGateKind` semantics embedded in the receipt (the receipt *witnesses* that gates passed; it does not *evaluate* gates);
- no `target_ref` (Q1 stays deferred; binding is the session anchor only);
- no stored deliberation/decision/request **body** (fingerprints only).

**Session precondition & duplicate semantics:** identical to the landed classes — the `(domain_id, session_id)` session must be opened first (fail-closed otherwise); at most one activation per `(domain_id, session_id, activation_id)`; a same-identity retry returns the **original** receipt un-restamped; a different `crossed_by`/`body_hash`/`decision_ref` for the same identity is a fail-closed conflict.

## 7. ADR-0026 envelope usage

`ActivationCrossedReceipt` should sit where the other four sit: **ADR-0026 Layer 2**, alongside `GovernanceDecisionReceipt`, as a self-contained record carrying its own canonical blake3 `record_hash`.

**Honest layering caveat the implementation PR must respect:** ADR-0026's *written* Layer-2 model (`ArtifactReceipt` wrapping a signed, merkle-rooted Layer-1 `GovernanceProof`) predates the process-transition classes. Those classes reuse the Layer-2 *slot* but use a lighter model — a self-hashed blake3 `record_hash`, **no signature, no merkle root**. This contract does **not** claim the activation receipt inherits the signed-proof envelope; it inherits the *process-transition* discipline (self-contained record hash, opaque-store persistence). Any future signature/merkle upgrade is out of scope here and would be an ADR-0026 revision, not a receipt rung.

## 8. Links and provenance

How the receipt would link back (design-level; the cross-receipt link is blocker **B1**):

- **process/session** — via the `(domain_id, session_id)` anchor (existing pattern; no new seam).
- **decision** — via `decision_ref` (**OPEN, B1**): naming the `DecisionRecordedReceipt` being activated. This is the lane's first inter-receipt reference; the DecisionRecorded contract deferred even linking a decision to its deliberation entries, so this posture is a genuine open question, not a copy of an existing pattern.
- **gate result(s)** — via `gate_basis` (**OPEN, B2**): witnessing that the required `ProcessGateResultReceipt`s were `pass`. Candidate approaches: a fingerprint over the required gate-result `record_hash`es; a plain "all required gates passed" boolean with the gate set named out-of-band; or no in-receipt basis (the surface joins gate receipts by session at read time).
- **proof/envelope metadata** — the receipt's own `record_hash` is the provenance pointer; persistence and retrieval go through the same opaque receipt store as the other four (§9).

## 9. Idempotence and replay

Design requirements (mechanism already exists for the landed classes; this class would reuse it):

- Emission goes through the backend primitive **`put_opaque_if_absent`** (`GovernanceReceiptBackend`; production impl in `icn-gateway`'s `ReceiptStore`, atomic within one sled transaction). `None` returned ⇒ this write won; `Some(existing)` ⇒ hydrate and return the **original** persisted receipt — **never re-stamp**.
- The uniqueness marker is keyed on `(class, key1, key2)` where `key1` is an **injective** netstring-style composite of `(domain_id, session_id)` and `key2` is `activation_id`. Injectivity must be tested (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` must never mix).
- Same-identity retry ⇒ idempotent return of the original; different stable-identity fields for the same key ⇒ **fail-closed conflict** (e.g. `activation_crossed_conflict`, mirroring `decision_recorded_conflict`).
- Concurrent duplicate crossings must serialize to exactly one winner; losers observe the winner.
- **Timestamp doctrine (blocker B3, but the invariant holds regardless):** a timestamp may live in `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity — so two nodes replaying the same logical crossing converge on the original receipt (original timestamp, original hash) rather than minting divergent wall-clock hashes. Local wall-clock must **not** be an input to any cross-node-deterministic *identity*. Whether `crossed_at` is a caller-supplied `recorded_at` or a decision-carried `effective_at` (the #2288 doctrine, which currently lives only in the membership actor lane, not in any receipt) is the open choice.

## 10. Privacy boundary

- **No private deliberation, decision, or activation-request body text** in the receipt — fingerprints (`body_hash`) only, exactly as Deliberation/Decision store body_hash and never the body.
- Only hashes, opaque ids, DIDs, and repo-safe metadata are carried.
- Any private content behind an `ActivationRequest` stays fixture-safe or redacted; the receipt proves a crossing occurred, not that all audiences may read the underlying request.
- A future evidence/export summary of activation must be a **repo-safe fixture summary** (the #2289 pattern: `record_hash`/`body_hash` proof pointers with redaction reasons, never private text).

## 11. Member-shell / evidence-surface decision

**Recommendation: defer rendering.** Member-shell rendering is **out of scope** for the first implementation PR (and entirely out of scope for this docs-only PR — `web/member-shell/` is not touched here). The #2291 surface is fixture-only and read-only; wiring a real `ActivationCrossedReceipt` into it should follow the receipt landing, not precede it.

The contract must nonetheless specify, at minimum, **how a future member-shell evidence story would explain the boundary**: a plain-language "activation crossed / not yet crossed" state for a session, showing *which required gates were observed as passed* and the decision it activated, sourced entirely from `record_hash`/`body_hash` proof pointers (no private text), inside the existing fixture/dry-run/live labeling. A later, separately-scoped PR may add a fixture-only surface extension if it can do so safely, exactly as #2291 did for the first four classes.

## 12. Validation plan (for the future implementation PR)

The implementation PR must include **both** test tiers the landed classes use:

1. **`proof.rs` unit tests:** a **golden vector** pinning the v1 `record_hash` of a fixed sample; a **determinism** test (same inputs ⇒ same hash); a **per-field** test (every field change ⇒ different hash); and a **tag-disjointness** test asserting `icn:gov:activation_crossed:v1` never collides with — and carries a comment that it must never converge with — `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage.
2. **Runtime-slice integration test** (mirror `decision_recorded_receipt_runtime_slice.rs`): emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns original, never restamped; different-`crossed_by` and different-`body_hash`/`decision_ref` conflicts fail closed; unopened-session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (no aliasing); two domains sharing a `session_id` never mix.
3. **Gate-precondition test:** if a required gate result is `Fail` or absent, the boundary is not crossed and no receipt is emitted (fail-closed).
4. **Privacy grep:** no private body text in any serialized receipt or fixture.
5. **No-overclaim grep:** no "activation implemented / complete / production / pilot / organizer-ready / member-ready / live federation / NYCN" claims introduced by the change.
6. **Docs/fixture check** if any fixture is added later (doc-control + generated-index convergence).

## 13. Deferred work (explicitly out of scope of this contract and its future implementation)

- `MutationPlanRecordedReceipt` — the receipt that would describe a planned mutation after activation.
- `MutationAppliedReceipt` — the receipt that would witness a mutation actually applied.
- `EvidencePacketProducedReceipt` — a runtime evidence-packet producer (the #2289 export was a repo-safe *fixture* summary, not this).
- The actual **#2041** human/AT pass (screen-reader / low-vision / switch / AT-compat) — parked for a real human-testing phase.
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274), service hosting, K3s/DNS/Forgejo.

## 14. Implementation sequencing

Implementation **cannot begin from this contract alone.** Three blockers below have no existing seam and must be resolved by a **narrow decision rung** (a sibling decision doc, mirroring `decision-recorded-q4-decision.md`) before an implementation PR:

- **B1 — decision→activation reference posture.** Does `ActivationCrossedReceipt` name the decision it activates, and if so by `decision_id`, by decision `record_hash`, or both? This would be the lane's first inter-receipt link.
- **B2 — gate-basis representation.** How does the receipt witness "required gates observed as passed"? (fingerprint of the gate-result set / boolean+external join / nothing.) And: is a new `ActivationRequest` gate object and/or a new `ProcessGateKind` variant required? `ProcessGateKind` is a **closed 6-variant enum** (`PrivacyReview`, `AccessibilityReview`, `RepoSafetyReview`, `ScopeConfirmation`, `NoMutationCheck`, `SecondReviewerSignoff`); adding an activation gate is an **ADR-controlled taxonomy change**, not a free append.
- **B3 — activation timestamp source.** Caller-supplied `crossed_at` (current receipt pattern) vs decision-carried `effective_at` (the #2288 doctrine, currently membership-actor-only).

**Recommendation (Option C, matching the DecisionRecorded lane cadence):** land *this* design contract; then a narrow decision doc resolving B1/B2/B3; only then a contract-conformant implementation PR. The implementation PR **must keep #1748 / #2141 / #2041 open** unless separately reviewed, and must leave #2293 open for maintainer disposition rather than auto-closing it by side effect.

## 15. Non-goals

Restated from #2293 — this contract and its future implementation are:

- not #2041 completion; not human/AT execution;
- not production / pilot / organizer-ready / member-ready readiness;
- not live federation; not NYCN activation; not Phase-2 completion;
- not #2081 / #2080 / #2274; not entity-auth enforcement; not trusted token issuance; not UnknownLegacy repair; not service hosting; not K3s/DNS/Forgejo;
- not a general workflow engine; not chat/comment/moderation/social feed;
- not proposal/vote/quorum/mandate semantics;
- not mutation planning or mutation application;
- not `EvidencePacketProducedReceipt` — unless a decision rung proves the sequence must be split differently.

## 16. Related

Refs #2293.
Refs #1748.
Refs #2141.
Refs #2041.
Refs #2291.
Refs #2292.
