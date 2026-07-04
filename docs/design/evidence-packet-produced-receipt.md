# EvidencePacketProducedReceipt — Design/Audit Contract

**Status:** draft — design/audit
**Truth class:** descriptive
**Canonical:** no
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `4fb15051` (re-verify before relying on exact line numbers or hashes — they drift)
**Related:** #2313 (this contract's issue), #1748 (Institutional Process Substrate), #2141 (vertical institutional spine), #2041 (human/AT pass — open/parked), #2310 (`MutationAppliedReceipt` implementation), #2312 (mutation-applied render in the process-evidence member-shell demo), #2307 (`MutationAppliedReceipt` design contract), #2309 (its A1/A2/A3/A4 decision rung), ADR-0026 (receipt & provenance proof envelope), ADR-0027 (action cards)

> This is the **design/audit contract** #2313 requires before any implementation. It scopes a candidate `EvidencePacketProducedReceipt` as the next narrow process-transition receipt rung under #1748 / #2141 — the receipt that would witness that a **redacted evidence packet artifact was produced** from a set of prior process receipts, recorded **after** the process receipts it draws from (typically following a `MutationAppliedReceipt` for the same session).
>
> **This document adds no runtime code and asserts no implementation.** `EvidencePacketProducedReceipt` does not exist anywhere in the runtime today. This contract audits current state honestly, proposes a candidate contract *subject to implementation proof*, and names the blockers that a narrow decision rung must resolve before an implementation PR can begin. Receipts record facts and grant no authority. This is not evidence-packet production, not an evidence-packet producer, not external delivery, not audit acceptance, not human/AT completion, not a workflow engine, and not live/private data handling.

---

## 1. Purpose

The process-transition receipt lane under #1748 / #2141 has now landed **seven** runtime classes that make institutional process legible as replayable, hash-anchored evidence:

- `ProcessSessionOpenedReceipt` (anchor);
- `DeliberationEntryRecordedReceipt`;
- `DecisionRecordedReceipt`;
- `ProcessGateResultReceipt`;
- `ActivationCrossedReceipt`;
- `MutationPlanRecordedReceipt`;
- `MutationAppliedReceipt` (#2310, now also rendered read-only in the fixture-only process-evidence member-shell demo per #2312).

The framing spine (`ops/ideas/framing/institutional-process-substrate.md`) orders the substrate as:

```text
preview → deliberation → decision → activation → mutation plan → action cards → receipts → evidence
```

The seven landed classes cover *preview → mutation applied*. The **terminal** stage of the spine is **evidence** — "the receipt produced, the evidence exported." The next narrow, VM-executable rung is therefore an **evidence-packet-produced** receipt: a receipt of record that a **redacted evidence packet artifact** was produced from a set of the prior process receipts, recorded **after** those receipts, so the fact of production is auditable and replayable independently of the packet's contents. This document is the design/audit contract for the receipt that would witness the *fact that such a packet was produced*.

It is deliberately **not** an evidence-packet producer, **not** an evidence exporter, **not** an external delivery/acceptance/audit mechanism, and **not** a general workflow engine. It is one receipt rung, and — as the audit below shows — like the mutation-applied rung before it (#2307 → #2309 → #2310), it needs a narrow **decision rung** of its own before implementation. It introduces two genuinely new concepts (`receipt_set_hash`, `redaction_profile_hash`) with **no repo precedent** (§3), which raises the design stakes of that decision rung above the prior classes.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `4fb15051`):

- **#2310** — `MutationAppliedReceipt` runtime implementation — **landed** (merged; seventh `ProcessTransitionReceipt` class; merge commit `ef96f0f2`).
- **#2312** — mutation-applied render in the fixture-only process-evidence member-shell surface (`?mode=demo&set=process-evidence`) — **landed** (merged, `4fb15051`).
- **#2311** — mutation-applied member-shell render — **closed / completed** (by #2312).
- **#2309 / #2307** — `MutationAppliedReceipt` decision rung + design contract — **closed / merged**.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Current repo audit

Classification of every evidence-packet / evidence-export / redaction term relevant to this rung, read against `origin/main` @ `4fb15051`:

| Term | State | Where |
|------|-------|-------|
| `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` / `MutationPlanRecordedReceipt` / `MutationAppliedReceipt` | **runtime (Rust)** | `icn/crates/icn-governance/src/proof.rs`; wired in `icn/apps/governance/{manager,receipt_backend}`; per-class `*_runtime_slice.rs` tests. These seven are the only runtime `ProcessTransitionReceipt`s. |
| `MutationAppliedReceipt` (the receipt this one would typically follow) | **runtime (Rust), landed #2310** | `proof.rs` (`MutationAppliedReceipt`, `DOMAIN_TAG = icn:gov:mutation_applied:v1`, `compute_record_hash`); `manager.rs` (`record_mutation_applied`, `MutationAppliedOutcome`); `receipt_backend.rs` (class `"mutation_applied"`, `put/get_mutation_applied`, injective composite `key1(domain_id, session_id)` / `key2 = application_id`). Fields: `domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash, applied_at, record_hash`; `record_hash` is the sole `PartialEq`/`Eq` anchor. It references the plan (`plan_id` + `plan_record_hash`, verified fail-closed), which references the activation, which binds decision + gate basis — so a receipt referencing the applied step inherits applied → plan → activation → decision → gate **transitively**. |
| `EvidencePacketProducedReceipt` | **docs/framing only — no runtime seam** | framing (`institutional-process-substrate.md`), STATE/PHASE_PROGRESS, prior design docs' deferred-work sections (e.g. `mutation-applied-receipt.md` §13). **Audit found no Rust struct, tag, manager method, backend class constant, route, fixture, or test** (`rg "EvidencePacketProducedReceipt\|evidence_packet_produced" icn/crates icn/apps` ⇒ no match). It is a named candidate with no seam — exactly the position `MutationAppliedReceipt` was in before #2310. |
| **evidence export / rehearsal-evidence-export** | **fixture + validator contract only — NOT a receipt** | `web/member-shell/fixtures/process-evidence-export.json` (a repo-safe fixture summary of the receipt sequence), validated by `docs/scripts/validate-rehearsal-evidence.py` against `urn:icn:contract:rehearsal-evidence-export:v1`. This is a **read-only export/summary artifact**, not a governance `ProcessTransitionReceipt`. Its existence is exactly why E4 must distinguish *produced* (a receipt of record that a packet artifact was produced) from *exported/summarized* (this fixture) — see §5, §14 **EP5**. |
| `receipt_set_hash` | **no precedent anywhere** | `rg "receipt_set_hash" icn docs` ⇒ **no match**. This contract proposes it as a **new** v1 concept: a hash committing to the ordered set of source receipt references included in the packet. Because it participates in `record_hash`, its exact definition is a hard decision-rung blocker (**EP2**). |
| `redaction_profile_hash` / `redaction_profile_id` | **no precedent as a typed field** | `rg "redaction_profile_hash\|redaction_profile_id" icn docs` ⇒ **no match**. The word "redaction" appears only as human-readable *redaction-reason* prose in the fixture/demo surface (`process-evidence-*.json`, `shell.js`, `i18n.js`, the July demo docs) — never as a hash or id field. So `redaction_profile_hash` is a **new** v1 concept and `redaction_profile_id` has no id precedent to lean on (**EP4**). |
| `EvidencePacket` (baseline-lock) | **runtime (Rust), NOT prior art** | `icn/crates/icn-baseline-lock/src/evidence.rs` defines a separate `EvidencePacket` — a baseline-lock evidence/test bundle with its own type, **not** a governance `ProcessTransitionReceipt` and **not** prior art for this lane. Name collision only. **Out of scope here (§13).** |

**Honest bottom line:** the seven landed classes are the only `ProcessTransitionReceipt` runtime types. `EvidencePacketProducedReceipt` — this contract's subject — is a named candidate with **no runtime seam**, and it introduces two hash-participating fields (`receipt_set_hash`, `redaction_profile_hash`) that have **no precedent anywhere in the repo**. This class is entirely seam-discovery work *plus* two novel-concept definitions, which makes its decision rung heavier than the mutation-applied rung's.

### 3.1 The pattern the seven landed classes share (what an eighth class would mirror)

- `#[derive(Clone, Debug, Serialize, Deserialize)]`; hand-written `PartialEq`/`Eq` anchored **only** to `record_hash`.
- A `DOMAIN_TAG` const following `icn:gov:<class_snake_case>:v1`, hashed **first**, required to be disjoint from every other tag.
- Anchor is always the `(domain_id, session_id)` pair — `session_id` is meaningful only with `domain_id`; a receipt requires the session to have been opened first (`ProcessSessionOpenedReceipt` precondition).
- A caller-opaque per-item id (`entry_id` / `decision_id` / `activation_id` / `plan_id` / `application_id`).
- `recorded_by` / `crossed_by` / `applied_by` / `author`: a DID string, **actor evidence that grants zero authority** ("recorder, not decider/crosser/applier/producer").
- `recorded_at: u64` (Unix seconds), hashed into `record_hash` **but excluded from duplicate identity** — a retry never restamps.
- `body_hash` / `result_hash: Hash`: a caller-supplied 32-byte fingerprint; **the body is never stored**.
- `record_hash: Hash`: canonical blake3 over `DOMAIN_TAG` + length-prefixed variable-length strings + explicit-`u8` enum ordinals (if any) + raw fixed-size 32-byte hashes + `recorded_at.to_le_bytes()`.
- Uniqueness/idempotence via the `put_opaque_if_absent` backend primitive; duplicate identity is the *stable* fields only (`recorded_at`/`record_hash` excluded).
- **Inter-receipt reference (since #2296/#2303/#2310):** `ActivationCrossedReceipt` established the lane's **first** inter-receipt link (activation → decision). `MutationPlanRecordedReceipt` established the **second** (plan → activation). `MutationAppliedReceipt` established the **third** (applied → plan, by `plan_id` + `plan_record_hash`, verified fail-closed). An eighth class linking to the applied step **and** to a set of source receipts would be the lane's **fourth** such reference — and the **first** to reference a *set* rather than a single predecessor (which is precisely why `receipt_set_hash` is novel and blocker-worthy — see **EP1/EP2**).

## 4. Problem statement

A mutation application can be **recorded** today (`MutationAppliedReceipt`, #2310): an application-of-record, fingerprinted by `result_hash`, referencing the plan it applied. But there is not yet a narrow, receipt-backed **record that a redacted evidence packet was produced** from a set of the prior process receipts — recorded after them, so the fact of production is auditable and replayable independently of the packet's contents or any downstream delivery.

Framing (`institutional-process-substrate.md`) states the terminal ordering directly: the spine ends in **evidence** — "the receipt produced, the evidence exported."

For this dogfood slice, an **evidence packet produced** is a local/dev/fixture institutional fact: *a redacted evidence packet artifact was produced from a named, ordered set of prior process receipts, and that production was recorded.* The receipt witnesses that the packet was produced and content-addressed — **not** that it was externally delivered, accepted, audited, human-verified, or complete; **not** the packet's private source contents; and **not** that the receipt itself produced, delivered, or certified anything. What "produced" is permitted to assert without becoming a delivery/acceptance/audit engine is a central open question (§14, blocker **EP5**).

This is **not** external delivery, audit acceptance, human/AT completion, production readiness, pilot readiness, or an evidence-production engine. It records a *receipt of the production fact* and nothing else; it mutates no domain state, handles no live/private data, and grants no authority.

## 5. Evidence-packet-produced boundary definition (for this slice)

> For this dogfood slice, **evidence packet produced** means: an app-side producer step reported that a **redacted** evidence packet artifact was produced from a named, ordered set of prior process receipts (for the same session), and that *fact* was recorded — after those receipts, with the produced packet fingerprinted (`packet_hash`, never stored), the source receipt set committed (`receipt_set_hash`), and the redaction profile that shaped the public packet committed (`redaction_profile_hash`). The receipt records only the *fact and fingerprints* of that production step.
>
> It is **not** the external delivery of the packet, **not** its acceptance by any recipient, **not** an audit of its contents, **not** a human or assistive-technology verification, and **not** a kernel-readable packet payload. Recording a production produces a *receipt of the production* and nothing else; it mutates no domain state and grants no authority. **The receipt does not itself produce, deliver, certify, audit, validate, authorize, or roll back anything** — an app-side actor produces (or claims to have produced) the packet and asks the substrate to witness that fact. Whether the packet was correctly redacted, actually delivered, or accepted by anyone is upstream/downstream of this type.
>
> **Produced ≠ exported/summarized.** The existing `rehearsal-evidence-export` fixture (§3) is a read-only *summary* of the receipt sequence for a demo surface; it is not a receipt and not this rung. This contract's candidate stance (§14 **EP5**) is that packet *production* is a **separate** boundary from any *export/summary/delivery* — production is a recorded fact about an artifact; export/summary/delivery are distinct, later concerns this receipt does not assert.

## 6. Proposed receipt contract (candidate — subject to implementation proof)

Candidate tag: `icn:gov:evidence_packet_produced:v1` (must hash-separate from all existing tags, and **must never converge** with `icn:gov:mutation_applied:v1`, `icn:gov:mutation_plan_recorded:v1`, `icn:gov:activation_crossed:v1`, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, or the proposal/vote `icn:gov:decision:v1/v2/v3` lineage).

Candidate fields (naming follows the landed classes; anything marked **OPEN** is a blocker deferred to §14's decision rung, not an asserted field):

| Field | Type | Notes |
|-------|------|-------|
| `domain_id` | `String` | anchor half; hashed length-prefixed |
| `session_id` | `String` | anchor half; session must be opened first (precondition) |
| `packet_id` | `String` | caller-opaque per-packet id (mirrors `application_id`/`plan_id`); the `key2` uniqueness half |
| `mutation_application_id` | `String` | **the applied step this packet draws from** — caller-opaque handle (immediate prior boundary; see **EP1**) |
| `mutation_applied_record_hash` | `Hash` (32) | content-addressed `record_hash` of the `MutationAppliedReceipt` this packet follows; the cryptographic proof link to the immediate prior boundary (**EP1**) |
| `receipt_set_hash` | `Hash` (32) | **NEW — no precedent.** a hash committing to the *ordered set* of source receipt references (their `record_hash`es, in a canonical order) included in the packet; the multi-predecessor commitment (**EP1/EP2**) |
| `packet_hash` | `Hash` (32) | caller-supplied fingerprint of the **public/redacted** evidence packet artifact; **the packet body is never stored**; must **not** cover private source-receipt bodies or hidden data (**EP3**) |
| `redaction_profile_hash` | `Hash` (32) | **NEW — no precedent.** content-addressed hash of the redaction profile that shaped the public packet; records *which* redaction profile was applied without storing private data or claiming the profile is complete/approved/legally sufficient (**EP4**) |
| `produced_by` | `String` (DID) | actor evidence — the recorder/producer-witness of the packet, **not** an authority to produce, deliver, certify, or audit ("recorder, not producer"); grants zero authority |
| `produced_at` | `u64` | node-stamped Unix seconds at record time; hashed into `record_hash`, **excluded** from duplicate identity (**EP-time**) |
| `record_hash` | `Hash` (32) | canonical blake3 per §3.1 hashing discipline; the sole `PartialEq`/`Eq` anchor |

**Candidate domain tag:** `icn:gov:evidence_packet_produced:v1`.

**Candidate canonical hashing:** `DOMAIN_TAG` first → length-prefixed `domain_id`, `session_id`, `packet_id`, `mutation_application_id`, `produced_by` → `mutation_applied_record_hash` raw 32 (no length prefix) → `receipt_set_hash` raw 32 → `packet_hash` raw 32 → `redaction_profile_hash` raw 32 → `produced_at.to_le_bytes()`. Exact layout is fixed by the implementation PR and pinned by a golden vector (§12) — **and only after the hash-participating blockers EP1/EP2/EP3/EP4 are resolved by the decision rung (§14).**

**Candidate stable duplicate identity:** `(domain_id, session_id, packet_id, mutation_application_id, mutation_applied_record_hash, receipt_set_hash, packet_hash, redaction_profile_hash, produced_by)`. `produced_at` and `record_hash` are **excluded** (retry never restamps).

**Deliberately absent (must never appear in v1):**

- no kernel-readable packet payload, packet body, source-receipt bodies, private source data, plan body, applied-result body, operation list, target list, or effect payload (the packet **body** is fingerprinted, never stored — meaning firewall);
- no private organizer / member / sponsor / attendee data of any kind;
- no typed packet/content/receipt-set model beyond the hashes (**EP2/EP3**);
- no authority grant, capability, mandate, or token; no "this packet was delivered/accepted/audited/approved/valid" assertion;
- no delivery, acceptance, transport, or audit-outcome fields; no external-recipient reference; no signature/attestation of delivery;
- no human/AT status or completion field (**EP-human**; #2041 stays open);
- no rollback, re-production, or supersession semantics; no execution trigger;
- no re-reference of `plan_id`/`plan_record_hash`/`activation_id`/`decision_id`/`gate_basis` as *separate scalar fields* (the applied → plan → activation → decision → gate chain is inherited transitively through `mutation_applied_record_hash`, and the broader source set is committed by `receipt_set_hash` — see §8);
- no proposal/vote/tally/quorum/outcome semantics;
- no stored packet/plan/decision/deliberation/result **body** (fingerprints only).

**Session precondition & duplicate semantics (candidate):** identical to the landed classes — the `(domain_id, session_id)` session must be opened first (fail-closed otherwise); at most one packet per `(domain_id, session_id, packet_id)`; a same-identity retry returns the **original** receipt un-restamped; a different `mutation_application_id`/`mutation_applied_record_hash`/`receipt_set_hash`/`packet_hash`/`redaction_profile_hash`/`produced_by` for the same identity is a fail-closed conflict (`evidence_packet_produced_conflict`, mirroring `mutation_applied_conflict`).

## 7. ADR-0026 envelope usage

`EvidencePacketProducedReceipt` should sit where the other seven landed process classes sit: **ADR-0026 Layer 2**, as a self-contained record carrying its own canonical blake3 `record_hash`.

**Honest layering caveat the implementation PR must respect (unchanged from the prior contracts):** ADR-0026's *written* Layer-2 model (`ArtifactReceipt` wrapping a signed, merkle-rooted Layer-1 `GovernanceProof`) predates the process-transition classes. Those classes reuse the Layer-2 *slot* but use a lighter model — a self-hashed blake3 `record_hash`, **no signature, no merkle root**. This contract does **not** claim the produced receipt inherits the signed-proof envelope; it inherits the *process-transition* discipline (self-contained record hash, opaque-store persistence). Any future signature/merkle upgrade is out of scope here and would be an ADR-0026 revision, not a receipt rung.

## 8. Links and provenance

How the receipt would link back (design-level):

- **process/session** — via the `(domain_id, session_id)` anchor (existing pattern; no new seam).
- **applied step** — via `mutation_application_id` + `mutation_applied_record_hash` (**EP1**): naming the `MutationAppliedReceipt` this packet immediately follows. This mirrors the verified-not-asserted posture the #2309 A1 decision set for the applied → plan link (the referenced `MutationAppliedReceipt` must exist in the same session, and its `application_id` must match, resolved via `get_mutation_applied(domain_id, session_id, application_id)` and compared on `record_hash`).
- **source receipt set** — via `receipt_set_hash` (**EP1/EP2**): committing to the *ordered set* of source-receipt `record_hash`es the packet draws from. This is the lane's first *set* reference; the decision rung must pin (a) which receipts are eligible members, (b) the canonical ordering, and (c) the exact hashing of the set (length-prefixed count + ordered member hashes is the candidate). Whether the runtime **verifies** each member exists in-session (like the scalar applied link) or treats the set hash as caller-supplied-only for `:v1` is an explicit **EP2** sub-question.
- **plan / activation / decision / gate basis** — inherited **transitively** through the applied → plan → activation chain (reachable from `mutation_applied_record_hash`) and/or committed within `receipt_set_hash`. The packet does **not** re-reference the plan, activation, decision, or gates as separate scalar fields in `:v1` (see **EP1**).
- **redaction basis** — via `redaction_profile_hash` (**EP4**): committing to the redaction profile that shaped the public packet, without storing the profile body or the private data it removed.
- **proof/envelope metadata** — the receipt's own `record_hash` is the provenance pointer; persistence and retrieval go through the same opaque receipt store as the other seven landed classes.

## 9. Idempotence and replay

Design requirements (mechanism already exists for the landed classes; this class would reuse it):

- Emission goes through the backend primitive **`put_opaque_if_absent`** (`GovernanceReceiptBackend`; production impl in `icn-gateway`'s `ReceiptStore`, atomic within one sled transaction). `None` returned ⇒ this write won; `Some(existing)` ⇒ hydrate and return the **original** persisted receipt — **never re-stamp**.
- The uniqueness marker is keyed on `(class, key1, key2)` where `key1` is an **injective** netstring-style composite of `(domain_id, session_id)` and `key2` is `packet_id`. Injectivity must be tested (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` must never mix).
- Stable duplicate identity is `(domain_id, session_id, packet_id, mutation_application_id, mutation_applied_record_hash, receipt_set_hash, packet_hash, redaction_profile_hash, produced_by)`. Same-identity retry ⇒ idempotent return of the original; a different value for any identity field for the same key ⇒ **fail-closed conflict** (e.g. `evidence_packet_produced_conflict`, mirroring `mutation_applied_conflict`).
- Concurrent duplicate records must serialize to exactly one winner; losers observe the winner.
- **Timestamp doctrine (the invariant holds regardless):** `produced_at` is **node-stamped at record time** (mirroring `applied_at`/`recorded_at` in the landed classes — those are node-stamped `now`, and the no-wall-clock invariant is about cross-node *identity*, which excludes the timestamp). A timestamp may live in `record_hash` **only because** the receipt is idempotent on stable, non-timestamp identity — so two nodes replaying the same logical production converge on the original receipt (original timestamp, original hash) rather than minting divergent wall-clock hashes. Local wall-clock must **not** be an input to any cross-node-deterministic *identity* (per #2283/#2284). `mutation_applied_record_hash`, `receipt_set_hash`, `packet_hash`, and `redaction_profile_hash` are all content-addressed hashes, so they introduce no node-local nondeterminism.

## 10. Privacy boundary

- **No packet body, source-receipt bodies, private source data, operation list, target object list, or effect payload** in the receipt — caller-supplied hashes (`packet_hash`, `receipt_set_hash`, `redaction_profile_hash`) fingerprint the public packet, the source set, and the redaction profile; none of those bodies is **ever stored** (exactly as the landed classes store `body_hash`/`result_hash` and never the body).
- `packet_hash` covers the **public/redacted** packet artifact **only** — it must **not** cover private source-receipt bodies, hidden organizer/member/sponsor/attendee data, or any pre-redaction content (**EP3**).
- The kernel never reads the packet semantically; the receipt carries no kernel-readable packet content (meaning-firewall discipline).
- Only hashes, opaque ids, DIDs, and repo-safe metadata are carried.
- Any private content behind a produced packet stays fixture-safe or redacted; the receipt proves a packet was produced, not that all audiences may read its contents.
- `redaction_profile_hash` records *which* profile shaped the public packet — it is **not** a claim that the redaction is complete, externally approved, or legally sufficient.
- A future evidence/export summary of a produced receipt must be a **repo-safe fixture summary** (the #2291/#2305/#2312 pattern: `record_hash`/`packet_hash`/`receipt_set_hash`/`redaction_profile_hash`/`mutation_applied_record_hash` proof pointers with redaction reasons, never private text).

## 11. Authority non-claim

Recording an evidence-packet production records an **institutional fact and grants zero authority.** `produced_by` is the recorder/producer-witness of the packet — recorder evidence, not an authority to produce, deliver, certify, or audit anything. A recorded production is **not** proof the packet is correct, complete, or correctly redacted; **not** proof it was delivered or accepted; **not** an approval, mandate, capability, or kernel-enforced permission. **The receipt does not produce, deliver, certify, audit, validate, authorize, or roll back the packet.** Whether the packet was legitimately produced, adequately redacted, delivered, or accepted is a charter/gate/authority/downstream question strictly outside this type. The receipt witnesses "a packet was produced and recorded here," nothing more.

## 12. Validation plan (for the future implementation PR)

The implementation PR must include **both** test tiers the landed classes use:

1. **`proof.rs` unit tests:** a **golden vector** pinning the v1 `record_hash` of a fixed sample; a **determinism** test (same inputs ⇒ same hash); a **per-field** test (every field change — including `receipt_set_hash`, `packet_hash`, and `redaction_profile_hash` — ⇒ different hash); and a **tag-disjointness** test asserting `icn:gov:evidence_packet_produced:v1` never collides with — and carries a comment that it must never converge with — `mutation_applied`, `mutation_plan_recorded`, `activation_crossed`, `decision_recorded`, `process_gate_result`, and the proposal/vote `icn:gov:decision:vN` lineage.
2. **Runtime-slice integration test** (mirror `mutation_applied_receipt_runtime_slice.rs`): emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns original, never restamped; different-`mutation_application_id`/`mutation_applied_record_hash`/`receipt_set_hash`/`packet_hash`/`redaction_profile_hash`/`produced_by` conflicts fail closed; unopened-session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (no aliasing); two domains sharing a `session_id` never mix.
3. **Applied-reference precondition test:** the referenced `MutationAppliedReceipt` (by `mutation_applied_record_hash`) must exist in the **same** `(domain_id, session_id)` and its `application_id` must match; an absent, wrong-session, wrong-domain, or `application_id`-mismatched reference is refused fail-closed and persists nothing (mirroring the #2309 A1 verified-not-asserted test). If the decision rung (**EP2**) requires per-member verification of the source set, add the analogous set-membership precondition test.
4. **Privacy grep:** no packet body / source-receipt body / private source text / operation list / target / effect text in any serialized receipt or fixture — fingerprints only.
5. **No-overclaim grep:** no "production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2 / delivered / accepted / audited / human-AT complete / certified / legally sufficient" claims introduced by the change.
6. **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim (§7).
7. **Protected close-keyword grep:** the implementation PR carries no closing keyword adjacent to a protected issue number — `Refs` only.

## 13. Deferred work (explicitly out of scope of this contract and its future implementation)

- Any **evidence-packet producer** — code that actually assembles, redacts, or emits an evidence packet. This contract stops strictly at *production recorded*; producing the packet artifact itself is a separate concern the receipt only witnesses.
- Any **evidence export / delivery / transport / acceptance / audit** mechanism — sending a packet to a recipient, recording its acceptance, or auditing its contents. The receipt witnesses *production*, never delivery or acceptance (§5, **EP5**).
- Any typed, kernel-readable packet/content/source-set model, `packet_ref`/`recipient_ref` on the receipt, or verifiable-packet binding beyond the `packet_hash`/`receipt_set_hash`/`redaction_profile_hash` fingerprints (see §14 EP2/EP3).
- The baseline-lock `EvidencePacket` type (`icn/crates/icn-baseline-lock/src/evidence.rs`) — a separate, unrelated type; not touched, not extended, not merged with this class.
- The actual **#2041** human/AT pass (screen-reader / low-vision / switch / AT-compat) — parked for a real human-testing phase; explicitly **excluded** from `:v1` (**EP-human**).
- Member-shell / process-evidence rendering of `EvidencePacketProducedReceipt` (§15).
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- Action-card triggers (ADR-0027 / #1713); entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274); service hosting, K3s/DNS/Forgejo.

## 14. Implementation sequencing

Implementation **cannot begin from this contract alone.** Mirroring the applied lane (#2307 contract → #2309 decision rung → #2310 implementation), the blockers below have no existing seam and should be resolved by a **narrow decision rung** (a sibling decision doc, in the `mutation-applied-receipt-decision-rung.md` cadence — candidate path `docs/design/evidence-packet-produced-receipt-decision-rung.md`) before an implementation PR. **Runtime implementation must not begin while any hash-participating blocker (EP1, EP2, EP3, EP4) remains unresolved**, because each can change the `:v1` field set and therefore the pinned `record_hash`. This rung carries **more** novelty than its predecessors: two of its fields have no repo precedent.

- **EP1 — predecessor link (hash-participating).** Does `EvidencePacketProducedReceipt` link to the immediate prior boundary (`mutation_application_id` + `mutation_applied_record_hash`), to a source-receipt-set hash (`receipt_set_hash`), or to **both**? *(This contract's candidate: **both** — `mutation_applied_record_hash` anchors the immediate prior process boundary, and `receipt_set_hash` commits to the set/order of source receipts the packet includes. This avoids pretending the packet depends on only one receipt while also avoiding storing any source body.)*
- **EP2 — `receipt_set_hash` definition (hash-participating, NEW — no precedent).** What exactly does `receipt_set_hash` commit to, and how is it computed? *(Candidate: a canonical blake3 over a length-prefixed count followed by the source receipts' `record_hash`es in a fixed canonical order — e.g. lane/emit order or sorted-by-hash; the decision rung must pin the ordering and the hashing so the golden vector is stable.)* Sub-questions the rung must answer: which receipt classes are eligible set members; whether the empty set is permitted; whether the runtime **verifies** each member exists in-session or accepts a caller-supplied set hash for `:v1`; and whether ordering is by emission or by sorted hash.
- **EP3 — `packet_hash` coverage (hash-participating).** Does `packet_hash` cover only the **public/redacted** packet artifact, or also private source references? *(Candidate: **public/redacted artifact only.** `packet_hash` must **not** cover private source-receipt bodies or hidden organizer/member/sponsor/attendee data; the ordered source references live under `receipt_set_hash`, and private bodies are never stored.)*
- **EP4 — redaction boundary representation (hash-participating, NEW — no precedent).** Is redaction represented by a `redaction_profile_hash`, a `redaction_profile_id`, both, or neither in `:v1`? *(Candidate: **`redaction_profile_hash` only.** The hash is the proof boundary. A human-readable `redaction_profile_id` should be added **only if** repo precedent for ids-alongside-hashes emerges — none exists today (§3) — so `:v1` stays hash-only. The hash records which profile shaped the public packet without storing private data or claiming the profile is complete, externally approved, or legally sufficient.)*
- **EP-time — produced timestamp source.** Node-stamped `produced_at` excluded from identity (current receipt pattern), consistent with the #2309 A3 decision and the `applied_at`/`recorded_at` node-stamp doctrine. *(Candidate: **single node-stamped `produced_at`**, hashed, excluded from identity; no distinct `delivered_at`/`accepted_at`; no time derived from the source receipts.)*
- **EP5 — "produced" witness boundary vs delivery/acceptance/audit boundary (the new question).** What must the runtime require to legitimately record a production, without becoming a delivery/acceptance/audit engine? And is packet *production* the same boundary as evidence *export/summary* (the existing `rehearsal-evidence-export` fixture) or a **separate** one? *(Candidate: `:v1` witnesses a **produced** artifact fact — an app-side actor attests it produced a redacted packet and supplies `packet_hash`/`receipt_set_hash`/`redaction_profile_hash`; the receipt does not verify the packet, does not deliver, does not certify acceptance, and does not audit. **Produced is a separate boundary from export/summary/delivery** — the read-only export fixture is a demo summary, not this receipt.)* The open decision: must "produced" bind a *retrievable* packet artifact (e.g. a store handle the substrate can later re-fingerprint), or is a caller-supplied `packet_hash` sufficient for `:v1`? If a retrievable-artifact binding is required, the field set changes (EP2/EP3/EP5 interact), which is precisely why this is deferred to the decision rung rather than pinned here.
- **EP-human — human/AT status.** Human/AT completion is **excluded** from `:v1`. The receipt may *mention* that human/AT remains governed by #2041, but it must **not** carry or imply a completed human/AT status field. #2041 stays open.

**Recommendation (Option C, matching the applied lane cadence):** land *this* design/audit contract; then a narrow decision doc resolving EP1/EP2/EP3/EP4/EP-time/EP5 (heavier than the prior rung's A1–A4 because EP2 and EP4 define brand-new concepts); only then a contract-conformant implementation PR. The implementation PR **must keep #1748 / #2141 / #2041 open** unless separately reviewed, and must leave its issue open for maintainer disposition rather than auto-closing it by side effect.

## 15. Member-shell / evidence-surface follow-up

**Recommendation: defer rendering.** Member-shell rendering is **out of scope** for the design contract, the decision rung, and the first implementation PR. The #2291 / #2305 / #2312 process-evidence surface is fixture-only and read-only; wiring a real `EvidencePacketProducedReceipt` into it should follow the receipt landing, as a later, separately-scoped fixture-only surface extension (exactly as #2312 did for `MutationAppliedReceipt` after #2310), and must preserve the redaction/privacy discipline (proof pointers only — `packet_hash`/`receipt_set_hash`/`redaction_profile_hash`/`mutation_applied_record_hash` — no packet body or private source text) and the doctrine that the receipt records a process fact and grants zero authority.

## 16. Non-goals

Restated from #2313 — this contract and its future implementation are:

- not an evidence-packet producer; not assembling, redacting, or emitting any packet;
- not evidence export, delivery, transport, acceptance, or audit; not certifying acceptance or auditing contents;
- not an action-card trigger; not a general workflow engine; not a policy/authority engine;
- not a new `ProcessGateKind`; not new authorization semantics;
- not OpenAPI / SDK / served-schema work; not member-shell implementation; not a fixture change;
- not an HTTP route; not runtime receipt implementation; not Rust/backend code;
- not live/private data handling; not private organizer/member/sponsor/attendee data handling;
- not #2041 completion; not human/AT execution;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion; not certified; not audited; not legally sufficient;
- not proposal / vote / quorum / mandate / outcome semantics;
- not the baseline-lock `EvidencePacket` type; not #2081 / #2080 / #2274; not entity-auth enforcement; not trusted token issuance; not UnknownLegacy repair; not service hosting; not K3s/DNS/Forgejo.

Receipts record institutional facts. They grant zero authority.

## 17. Related

Refs #2313.
Refs #2312.
Refs #2310.
Refs #2309.
Refs #2307.
Refs #1748.
Refs #2141.
Refs #2041.
