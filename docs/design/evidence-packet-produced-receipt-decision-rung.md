# EvidencePacketProducedReceipt decision rung — EP1/EP2/EP3/EP4/EP5 (predecessor linkage, packet-hash coverage, redaction boundary, meaning of "produced", human/AT status)

**Status:** draft — design / decision rung (not runtime implementation)
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-04
**Source basis:** read against `origin/main` @ `1f0d6d9e` (the merged #2314 contract's tip). Code anchors (`icn/crates/icn-governance/src/proof.rs`, `icn/apps/governance/src/receipt_backend.rs`, `icn/apps/governance/src/manager.rs`) were verified at that commit — re-verify before relying on exact line numbers or hashes; they drift.
**Related:** #2315 (this rung's issue) · #2313 (the `EvidencePacketProducedReceipt` design-contract issue) · #2314 (merged design/audit contract, [`docs/design/evidence-packet-produced-receipt.md`](evidence-packet-produced-receipt.md)) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2041 (human/AT pass — open/parked) · #2310 (`MutationAppliedReceipt` implementation — the immediate predecessor this packet draws from) · #2312 (mutation-applied render in the process-evidence member-shell demo) · PR #2309 (the sibling applied decision rung, [`mutation-applied-receipt-decision-rung.md`](mutation-applied-receipt-decision-rung.md)) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope) · `ops/ideas/framing/institutional-process-substrate.md` (framing)

> Narrow decision document resolving the five implementation blockers the merged #2314 `EvidencePacketProducedReceipt` design/audit contract named in its §14 — **EP1** (predecessor linkage: immediate applied step and/or source-receipt set), **EP2** (packet-hash coverage vs source-set coverage), **EP3** (redaction boundary representation), **EP4** (what "produced" asserts, and whether production is distinct from export/delivery), and **EP5** (human/AT status in v1). It mirrors the `mutation-applied-receipt-decision-rung.md` cadence: land the contract (#2314), then resolve the hash-participating structure **in writing** before a tag is pinned, then implement. This rung is **heavier** than its predecessors because two of the fields it pins — `receipt_set_hash` and `redaction_profile_hash` — introduce concepts with **no precedent anywhere in the repo**. This document decides nothing else: no runtime change, no receipt class added, no evidence-packet production, no member-shell change, no fixture change, no human/AT run. Receipts record institutional facts. They grant zero authority.

## 1. Purpose

The #2314 design contract scoped a candidate `EvidencePacketProducedReceipt` — the eighth `ProcessTransitionReceipt` rung under #1748 / #2141 that would witness that a **redacted evidence packet artifact was produced** from a set of prior process receipts, recorded after them (typically after a `MutationAppliedReceipt`). The contract deliberately refused to pin the candidate `icn:gov:evidence_packet_produced:v1` layout and blocked implementation on five questions whose answers change the canonical hash layout, the class's inter-receipt relationships, or the meaning of the receipt itself:

- **EP1** — does the receipt name its immediate predecessor, a source-receipt set, or both?
- **EP2** — what does `packet_hash` cover, and what does `receipt_set_hash` cover?
- **EP3** — how is the redaction boundary represented (`redaction_profile_hash`, an id, both, or neither)?
- **EP4** — what does "produced" assert, and is production distinct from export/delivery?
- **EP5** — does `:v1` carry any human/AT status?

The landed rule (from the #2278 review cycle, restated by the #2281 Q4 decision and applied again by the #2295 activation, #2302 plan, and #2309 applied rungs) is that **hash-participating structure is decided in writing before a tag is pinned, never silently in an implementation PR.** This document resolves EP1/EP2/EP3/EP4/EP5 so a contract-conformant implementation PR can begin. It is not a workflow engine, not a policy engine, not an evidence-packet producer, not an export/delivery mechanism, and not an action-card trigger.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `1f0d6d9e`):

- **#2314** — `EvidencePacketProducedReceipt` design/audit contract — **landed** (merged `1f0d6d9e`).
- **#2313** — the design-contract issue — **closed / completed** (by #2314).
- **#2310** — `MutationAppliedReceipt` runtime implementation (the immediate predecessor this packet references) — **landed**; the seventh `ProcessTransitionReceipt` class.
- **#2312** — mutation-applied render in the fixture-only process-evidence member-shell surface — **landed**; **#2311** — its render issue — **closed / completed**.
- **#1748 / #2141** — Institutional Process Substrate milestone / vertical spine — **open**.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked** for a broader human-testing phase; not attempted here.
- `EvidencePacketProducedReceipt` **is not implemented** — no Rust struct, tag, manager method, backend class constant, route, fixture, or test exists anywhere in `icn/crates/` or `icn/apps/` (confirmed by live audit: `rg "EvidencePacketProducedReceipt|evidence_packet_produced" icn/crates icn/apps` → no match).
- `receipt_set_hash` and `redaction_profile_hash` have **no precedent anywhere** — the only matches are in `docs/registry.toml` / `docs/INDEX.generated.md`, i.e. the #2314 registry description of this very lane, not code or another design doc.

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Repo audit update (verified against live code)

Confirming the #2314 audit against `origin/main` @ `1f0d6d9e` — the facts EP1–EP5 depend on:

| Subject | Finding | Anchor |
|---------|---------|--------|
| `MutationAppliedReceipt` (the immediate predecessor this packet references) | fields `domain_id, session_id, application_id, plan_id, plan_record_hash, applied_by, result_hash, applied_at, record_hash`; tag `icn:gov:mutation_applied:v1`; `record_hash` is the sole `PartialEq`/`Eq` anchor. It references the plan (`plan_id` + `plan_record_hash`), which binds activation → decision → gate transitively — so a packet referencing the applied step inherits applied → plan → activation → decision → gate transitively | `icn/crates/icn-governance/src/proof.rs` (`MutationAppliedReceipt`, `DOMAIN_TAG`, `compute_record_hash`) |
| applied lookup / uniqueness | `put_mutation_applied` persists via **`put_opaque_if_absent`** under class `"mutation_applied"`, `key1 =` the injective netstring `mutation_applied_composite_key1(domain_id, session_id)`, `key2 = application_id`; `get_mutation_applied(domain_id, session_id, application_id)` reads it back (`get_latest_opaque(MUTATION_APPLIED_CLASS, key1, Some(application_id))`). A packet can therefore verify its immediate-predecessor reference fail-closed by `get_mutation_applied(...)` then comparing `record_hash` | `icn/apps/governance/src/receipt_backend.rs` (`put_mutation_applied`, `get_mutation_applied`, `mutation_applied_composite_key1`); `icn/apps/governance/src/manager.rs` (`record_mutation_applied`, `get_mutation_applied`) |
| the seven landed classes | `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` / `MutationPlanRecordedReceipt` / `MutationAppliedReceipt` are the only runtime `ProcessTransitionReceipt`s; all seven tags present in `proof.rs` | `proof.rs` |
| inter-receipt references | exactly **three** exist: `ActivationCrossedReceipt` → `DecisionRecordedReceipt` (#2295 B1), `MutationPlanRecordedReceipt` → `ActivationCrossedReceipt` (#2302 M1), and `MutationAppliedReceipt` → `MutationPlanRecordedReceipt` (#2309 A1). A packet → applied-step link plus a source-receipt-set commitment would be the lane's **fourth** inter-receipt reference and the **first to reference a set** | whole-repo search |
| `EvidencePacketProducedReceipt` | **framing/doc-only** — no Rust type in `icn/crates` or `icn/apps`. `icn/crates/icn-baseline-lock/src/evidence.rs` defines a separate `EvidencePacket` baseline-lock bundle, **not** a governance process class and **not** prior art for this lane | whole-repo search |
| `receipt_set_hash` / `redaction_profile_hash` / `redaction_profile_id` | **no precedent anywhere** — no runtime type, no other design doc; the only occurrences are the #2314 registry/index description of this lane | whole-repo search |
| the existing evidence export | `web/member-shell/fixtures/process-evidence-export.json` validated by `docs/scripts/validate-rehearsal-evidence.py` against `urn:icn:contract:rehearsal-evidence-export:v1` is a **read-only summary artifact**, not a receipt — the basis for EP4's produced ≠ export distinction | whole-repo search |
| `put_opaque_if_absent` | the idempotence primitive on the gateway `ReceiptStore` and the `GovernanceReceiptBackend` trait — atomic insert-if-absent keyed on `(class, key1, key2)`; `None` ⇒ this write won, `Some(existing)` ⇒ return the original (never restamp). An eighth class reuses it | `receipt_backend.rs`, `receipt_store.rs` |

**Bottom line:** every #2314 audit claim that EP1–EP5 rely on is accurate against live code. The seven landed classes are the only runtime `ProcessTransitionReceipt`s; the evidence-packet-produced rung remains seam-discovery work; the `MutationAppliedReceipt` it would reference already carries (transitively) the applied → plan → activation → decision → gate chain; and `receipt_set_hash` / `redaction_profile_hash` are genuinely new — this rung defines them, so it must pin their coverage and (for the set) their canonical ordering and hashing with extra care.

## 4. EP1 decision — predecessor linkage

**Question.** Does `EvidencePacketProducedReceipt` name its immediate predecessor (`MutationAppliedReceipt`) by `mutation_application_id` and/or `mutation_applied_record_hash`, and does it also commit to the *set* of source receipts the packet draws from via `receipt_set_hash`? This is the lane's **fourth** inter-receipt reference and its **first set commitment**.

Options considered:

1. **Immediate predecessor only** (`mutation_application_id` + `mutation_applied_record_hash`) — a single scalar link. Rejected as the *sole* link: an evidence packet is produced *from a body of prior receipts*, not just the last one; committing only to the immediate predecessor would leave "which receipts this packet's evidence rests on" with no cryptographic answer.
2. **`receipt_set_hash` only** — commit to the whole source set, drop the scalar immediate link. Rejected: loses the human/index handle (`mutation_application_id`) and the cheap, directly-verifiable fail-closed check that the immediate prior boundary exists in-session; a bare set hash is opaque and cannot be spot-verified against a single known receipt.
3. **All three: `mutation_application_id` + `mutation_applied_record_hash` + `receipt_set_hash`. CHOSEN.** Mirrors the proven scalar posture (`*_id` + `*_record_hash`) the #2295 B1 / #2302 M1 / #2309 A1 decisions set for the lane's inter-receipt links, and adds a set commitment for the multi-predecessor evidence boundary that this class uniquely needs.

**Decision EP1: `:v1` carries all three.**

- `mutation_application_id: String` — the applied step this packet draws from; the human/index handle, unique within the session.
- `mutation_applied_record_hash: Hash` — the 32-byte `record_hash` of that `MutationAppliedReceipt`; the content-addressed proof link to the **immediate prior process boundary**.
- `receipt_set_hash: Hash` — a 32-byte content-addressed commitment to the **canonical ordered set** of source-receipt references included in the packet's evidence boundary.

Canonical receipt-set rules (pinned):

- **`receipt_set_hash` commits to an ordered list of receipt *references* — each member's `record_hash` — never receipt *bodies*.** No receipt body, no private source data, no plan/applied-result body is stored or hashed; only 32-byte `record_hash`es participate.
- **v1 source-set membership is limited to in-session process/evidence *receipt* references** needed for the produced-packet boundary (the seven landed `ProcessTransitionReceipt` classes' `record_hash`es for that `(domain_id, session_id)`). **Export/delivery artifacts remain outside v1** unless a later decision explicitly adds them.
- **The immediate predecessor must be a member of the declared set.** `mutation_applied_record_hash` MUST appear among the set members — the scalar link and the set commitment must be consistent.
- **Canonical ordering (pinned, deterministic — a canonicalization, not an ordering engine):** members are sorted by **(a) receipt-ladder position** (`ProcessSessionOpened` < `DeliberationEntryRecorded` < `DecisionRecorded` < `ProcessGateResult` < `ActivationCrossed` < `MutationPlanRecorded` < `MutationApplied`), then **(b) `record_hash` bytewise ascending** within a class. The runtime canonicalizes to this order **before** hashing, so caller-supplied input order can never fork identity.
- **Members are a set — duplicates disallowed (pinned).** Each source receipt's `record_hash` appears **at most once**; a declared list containing a duplicate `record_hash` is refused **fail-closed** (it persists nothing). This is required because `receipt_set_hash` is a *set* commitment: permitting a repeated element would let a caller fork `receipt_set_hash` (and therefore `record_hash` and the receipt's identity) by padding the list with duplicates. The runtime rejects duplicates rather than silently de-duplicating, so the caller's declared set and the hashed set are always identical.
- **Canonical set hashing (pinned):** after de-duplication is confirmed (duplicates fail closed, above) and canonical ordering is applied, a domain-separated blake3 over a length-prefixed member count (`u64` LE) followed by each member's `record_hash` raw 32 (no per-member length prefix) in canonical order. (The implementation PR pins the exact set-hash domain separation and fixes it with a golden vector; the empty set is **disallowed** in `:v1` — a produced packet must draw from at least its immediate predecessor.)
- **Fail-closed predecessor verification (mandatory floor).** The runtime MUST require that a `MutationAppliedReceipt` with `record_hash == mutation_applied_record_hash` exists in the **same** `(domain_id, session_id)` — resolved via `get_mutation_applied(domain_id, session_id, mutation_application_id)` and compared on `record_hash` — before the packet is recorded. If it is absent, present under a different session/domain, or its `application_id` does not match the supplied one, the packet is **not** recorded and **no receipt is emitted** (mirroring the #2309 A1 verified-not-asserted precondition). This is what makes the immediate link a *proof*, not a claim.
- **Set-member verification (recommended default; minimal floor is the immediate predecessor).** Because the per-class `get_*` lookups already exist, the implementation SHOULD also verify that every declared set member's `record_hash` resolves to a receipt in the same `(domain_id, session_id)`, failing closed on any miss. The **mandatory** v1 floor is immediate-predecessor verification (above); full-set membership verification is the recommended default and is cheap (it reuses existing lookups), but deferring it for cost is acceptable only if the immediate-predecessor check and set-consistency check (immediate predecessor ∈ set) both hold. No set-walk subsystem is invented; verification is per-member `get_*` calls or it is deferred.

Binding consequences:

- **All three fields participate in the canonical `record_hash` and in stable duplicate identity.** A same-identity retry returns the original receipt un-restamped; a different `mutation_applied_record_hash`, `mutation_application_id`, or `receipt_set_hash` for the same `packet_id` is a fail-closed conflict (`evidence_packet_produced_conflict`).
- **ADR-0026 preserved.** The links point at content-addressed `record_hash`es; `EvidencePacketProducedReceipt` inherits the *process-transition* discipline (self-contained blake3 `record_hash`, opaque-store persistence) and asserts **no** signed-envelope/merkle inheritance.
- **Idempotence / replay.** Because `mutation_applied_record_hash` and `receipt_set_hash` are content-addressed and deterministic (not wall-clock, and — for the set — canonically ordered), two nodes replaying the same logical production derive the same packet identity and converge on the **original** receipt via `put_opaque_if_absent`.

**Test that proves it:** a runtime-slice test that (a) records the full prerequisite chain through a `MutationAppliedReceipt`, then records an `EvidencePacketProducedReceipt` citing that applied step's real `record_hash` and a set including it, round-trips it, and asserts the stored `mutation_applied_record_hash` and `receipt_set_hash` match; (b) a packet whose `mutation_applied_record_hash` names no applied receipt in the session (or names one from a different session/domain, or whose `application_id` mismatches) is refused fail-closed and persists nothing; (c) a packet whose declared set omits the immediate predecessor is refused fail-closed; (d) two callers supplying the same set members in different input orders produce the **same** `receipt_set_hash` (canonicalization proof); (e) a same-identity retry returns the original un-restamped; (f) a conflicting `receipt_set_hash`/`mutation_applied_record_hash` for the same `packet_id` is a fail-closed conflict.

## 5. EP2 decision — packet-hash coverage vs source-set coverage

**Question.** What does `packet_hash` cover, what does `receipt_set_hash` cover, and what must neither cover?

Options considered:

1. **`packet_hash` covers the public/redacted evidence packet artifact only; `receipt_set_hash` covers ordered source-receipt references; neither covers private bodies. CHOSEN.**
2. `packet_hash` covers the full pre-redaction packet (including private source bodies). Rejected: it would fingerprint (and thereby depend on) private organizer/member/sponsor/attendee data, breaking the privacy posture and the meaning firewall, and would make the hash unstable across redaction policies.
3. A single combined hash over packet + sources. Rejected: it conflates "what artifact was produced" with "what it was produced from" — two distinct proofs that consumers need separately (verify the public artifact vs. audit the evidence basis).

**Decision EP2: two distinct, single-purpose hashes.**

- **`packet_hash` covers the public/redacted evidence packet artifact only** — the content fingerprint of the artifact that was produced and could be shared publicly. It is a caller-supplied 32-byte fingerprint; **the packet body is never stored**.
- **`receipt_set_hash` covers the canonical ordered source-receipt references/hashes** (per EP1) — the evidence basis the packet rests on.
- **Neither hash covers private source bodies.** Private source-receipt bodies, private organizer/member/sponsor/attendee data, plan bodies, applied-result bodies, operation lists, target lists, and effect payloads are **out of `:v1` entirely** — not stored, not hashed, not referenced as bodies.

**What `packet_hash` explicitly does NOT prove** (must be stated in the type doc-comment and any future evidence surface):

- correctness; completeness; legal sufficiency;
- external delivery; acceptance; audit certification;
- human verification; assistive-technology verification;
- production readiness; pilot readiness; organizer/member readiness.

`packet_hash` proves only that *an artifact with this content fingerprint was produced and recorded here.*

Binding consequences:

- **Both hashes participate in the canonical `record_hash` and in stable duplicate identity.** They are fixed-32 fields appended raw (no length prefix).
- **The firewall holds:** the receipt carries no kernel-readable packet content and no source bodies; it witnesses *that a redacted packet (fingerprinted) was produced from a committed set of references*, not what the packet or its sources contain.
- **Neither hash is verified for content** (the receipt cannot re-derive what it never stored) — they are caller-supplied content fingerprints, exactly as for the landed classes' `body_hash`/`result_hash`. (The *references* inside `receipt_set_hash` may be existence-verified per EP1; the *packet body* behind `packet_hash` is not.)

**Test that proves it:** the serialized receipt carries exactly the `:v1` field set — no `packet_body`/`content`/source-body/operation/target/effect field; only the three hashes plus ids/DID/timestamp (a per-field/golden test confirms `packet_hash` and `receipt_set_hash` each independently participate in `record_hash`; a serde payload-audit test confirms no body field is present).

## 6. EP3 decision — redaction boundary

**Question.** Is the redaction boundary represented by `redaction_profile_hash`, a `redaction_profile_id`, both, or neither in `:v1`?

Options considered:

1. **`redaction_profile_hash` only. CHOSEN.**
2. `redaction_profile_id` + `redaction_profile_hash`. Rejected for `:v1`: the #2314 contract audit found **no repo precedent** for ids-alongside-hashes in this lane; a human-readable profile id implies a profile registry / policy-identity scheme that does not exist. Adding an id now would pin an identifier format and registry contract that belong to their own later decision.
3. Neither (no redaction field). Rejected: the receipt would then make no statement about how the public packet was shaped, leaving "was this redacted, and under what profile" unanswerable — a privacy-transparency gap for a receipt whose whole point is a *redacted* artifact.

**Decision EP3: `:v1` carries `redaction_profile_hash` only** — a 32-byte content-addressed fingerprint of the redaction profile that shaped the public packet. No `redaction_profile_id`, no profile registry, no human-readable redaction labels, no legal-sufficiency assertion in `:v1`; all deferred to a later decision if a real consumer needs them.

**What `redaction_profile_hash` explicitly does NOT prove** (must be stated in the type doc-comment):

- that redaction is complete;
- that redaction is legally sufficient;
- that private data was handled live;
- that an external reviewer accepted the packet;
- that human/AT review is complete.

It records only *which redaction profile was applied*, by fingerprint.

Binding consequences:

- **`redaction_profile_hash` participates in the canonical `record_hash` and in stable duplicate identity** (fixed-32, raw). Two packets differing only in redaction profile are distinct records.
- **Privacy holds:** the profile *body* (rules, patterns, policy text) is not stored — only its fingerprint. No private data is carried by the profile hash.

**Test that proves it:** the serialized payload carries `redaction_profile_hash` and **no** `redaction_profile_id`/profile-body/policy-text field; `redaction_profile_hash` participates in `record_hash`; a no-overclaim grep confirms no "redaction complete / legally sufficient / reviewer accepted" claim in the type or its serialization.

## 7. EP4 decision — meaning of "produced"

**Question.** What does "produced" assert, and is evidence-packet *production* the same boundary as evidence *export/delivery* (the existing `rehearsal-evidence-export` fixture)?

This is the rung's boundary question — "produced" is temptingly readable as "delivered to a recipient" or "accepted by an auditor," which would pull delivery, acceptance, and audit into the receipt.

Options considered:

1. **Produced = an evidence packet artifact was recorded/produced and content-addressed; the receipt is distinct from export, delivery, acceptance, external audit, and action-card triggering. CHOSEN.**
2. Produced = delivered/accepted/audited. Rejected outright and permanently for this class: that would make the receipt a delivery/acceptance/audit engine, asserting facts about external recipients and reviewers it cannot witness, violating the lane's core doctrine (*receipts record facts and grant zero authority*) and maturity-band honesty.
3. Fold production and export into one boundary. Rejected: the existing `rehearsal-evidence-export` fixture is a **read-only summary** artifact, not a receipt; conflating "a packet artifact was produced" (a recorded fact) with "a summary was exported/rendered" (a separate surface concern) would blur two distinct boundaries and over-claim what the receipt proves.

**Decision EP4: `:v1` "produced" means an evidence packet artifact was recorded/produced and content-addressed** (fingerprinted by `packet_hash`, per EP2). `EvidencePacketProducedReceipt` is **distinct from export, delivery, acceptance, external audit, and action-card triggering.** Production is a recorded fact about an artifact; export/summary/delivery/acceptance/audit are separate, later concerns this receipt does not assert.

`:v1` "produced" **explicitly excludes** (must be stated as non-claims in the type doc-comment and any future evidence surface):

- external delivery; acceptance; audit certification;
- human verification; assistive-technology verification;
- semantic correctness; legal sufficiency;
- production readiness; pilot readiness; organizer readiness; member readiness;
- live federation; NYCN activation; Phase-2 completion.

Binding consequences:

- **`produced_by`** is the recorder / producer-witness DID — actor evidence, **not** an authority to produce, deliver, certify, or audit, and not a claim that the producer was permitted to produce.
- The receipt **does not produce, deliver, certify, audit, validate, authorize, enforce, or roll back** the packet. It performs no side effect on domain state; it only records the production *fact*.
- The receipt's honesty label in any future evidence surface must read as "an evidence packet was produced and recorded here," not "the packet was delivered / accepted / audited / is correct."
- Because EP4 keeps `:v1` a *recording* fact (no delivery/acceptance/audit field), it introduces **no** additional hash-participating field beyond EP1–EP3; the §9 layout is complete and pinnable.

**Test that proves it:** a no-overclaim grep asserts the receipt/type carries no "delivered / accepted / audited / certified / verified-correct / production-ready / pilot-ready" claim; the runtime-slice test asserts recording a production performs no mutation of any domain state beyond persisting the receipt itself, and that `produced_by` is stored as an opaque DID string with no capability/authority check attached to it.

## 8. EP5 decision — human/AT status

**Question.** Does `:v1` carry any human/AT completion status, and how does it relate to #2041?

Options considered:

1. **Exclude human/AT completion status entirely from `:v1`; #2041 stays open; the receipt may reference #2041 only as an open dependency. CHOSEN.**
2. Carry a human/AT status field (e.g. `human_at_reviewed: bool`). Rejected outright: no real human/AT pass has occurred (this VM is headless and cannot run screen-reader / low-vision / switch / AT-compat testing), so any such field would be a false or premature claim; and automated a11y checks or evidence-packet production do **not** constitute a human/AT pass.

**Decision EP5: `:v1` carries no human/AT status field and makes no human/AT claim.** Specifically:

- **No completed human/AT field** in `:v1`.
- **No implied human/AT pass** — producing a packet, fingerprinting it, or running automated a11y checks does **not** complete #2041.
- **No claim that automated a11y or evidence-packet production completes #2041.**
- The decision document and the future type doc-comment **may reference #2041 only as an open dependency** — never as satisfied.
- **#2041 stays open.**

Binding consequences:

- The `:v1` field set contains no human/AT field; this is a hash-layout consequence (nothing added) and a claims-discipline requirement.
- Any future human/AT status belongs to #2041's own track and a separate later decision, never a silent `:v1` add.

## 9. Consolidated candidate `:v1` layout (for the implementation PR)

Resolving EP1/EP2/EP3/EP4/EP5 pins the candidate `icn:gov:evidence_packet_produced:v1` field set (all names **candidate — subject to implementation proof and golden-vector pinning**; the tag must hash-separate from, and never converge with, `icn:gov:mutation_applied:v1`, `icn:gov:mutation_plan_recorded:v1`, `icn:gov:activation_crossed:v1`, `icn:gov:decision_recorded:v1`, `icn:gov:process_gate_result:v1`, `icn:gov:deliberation_entry_recorded:v1`, `icn:gov:process_session_opened:v1`, and the proposal/vote `icn:gov:decision:v1/v2/v3` lineage):

| Field | Type | In stable identity? | Source |
|-------|------|---------------------|--------|
| `domain_id` | `String` | yes (`key1` half) | anchor |
| `session_id` | `String` | yes (`key1` half) | anchor; session must be opened first |
| `packet_id` | `String` | yes (`key2`) | caller-opaque per-packet id |
| `mutation_application_id` | `String` | yes | **EP1** — immediate applied step (must exist in-session) |
| `mutation_applied_record_hash` | `Hash` (32) | yes | **EP1** — content-addressed proof link to the `MutationAppliedReceipt` (verified fail-closed) |
| `receipt_set_hash` | `Hash` (32) | yes | **EP1/EP2** — canonical ordered commitment to the source-receipt reference set (references, not bodies) |
| `packet_hash` | `Hash` (32) | yes | **EP2** — fingerprint of the public/redacted packet artifact; packet body never stored |
| `redaction_profile_hash` | `Hash` (32) | yes | **EP3** — fingerprint of the redaction profile; profile body never stored |
| `produced_by` | `String` (DID) | yes | **EP4** — recorder / producer-witness; grants zero authority |
| `produced_at` | `u64` | **no** | node-stamped **Unix seconds** at record time (byte-parallel with the landed classes' `recorded_at`/`applied_at`); hashed; excluded from identity (retry never restamps) |
| `record_hash` | `Hash` (32) | (equality anchor) | canonical blake3; the sole `PartialEq`/`Eq` anchor |

**Candidate canonical hashing:** `DOMAIN_TAG` (`icn:gov:evidence_packet_produced:v1`) first → length-prefixed `domain_id`, `session_id`, `packet_id`, `mutation_application_id`, `produced_by` → `mutation_applied_record_hash` raw 32 (no length prefix) → `receipt_set_hash` raw 32 → `packet_hash` raw 32 → `redaction_profile_hash` raw 32 → `produced_at` LE. Exact layout is fixed by the implementation PR and pinned by a golden vector.

**Candidate stable duplicate identity:** `(domain_id, session_id, packet_id, mutation_application_id, mutation_applied_record_hash, receipt_set_hash, packet_hash, redaction_profile_hash, produced_by)`. `produced_at` and `record_hash` are **not** identity.

**Timestamp stance (pinned):** `produced_at` is a `u64` of **Unix seconds**, **node-stamped at record time** (byte-parallel with the landed classes' `recorded_at`/`applied_at`, which the runtime stamps as `now` in Unix seconds), hashed into `record_hash` but **excluded** from stable duplicate identity. No wall-clock time is a cross-node-deterministic identity input (per the #2283/#2284 determinism doctrine); the receipt's determinism comes entirely from its content-addressed identity, so `produced_at` may live inside `record_hash` only because replay converges on the original stamp. A same stable-identity retry returns the original receipt and does **not** restamp.

**Uniqueness / conflict:** `put_opaque_if_absent` keyed on `(class, key1, key2)` where `key1` is an injective netstring composite of `(domain_id, session_id)` and `key2` is `packet_id`; conflict detection on `(mutation_application_id, mutation_applied_record_hash, receipt_set_hash, packet_hash, redaction_profile_hash, produced_by)`. `produced_at` and `record_hash` are not identity. Same-identity retry ⇒ original returned; a **different stable identity for the same `(domain_id, session_id, packet_id)`** ⇒ fail-closed `evidence_packet_produced_conflict`.

**Preconditions (all fail-closed; on any failure nothing is persisted):** (1) the `(domain_id, session_id)` session was opened first; (2) a `MutationAppliedReceipt` with `record_hash == mutation_applied_record_hash` exists in that same session and its `application_id` equals the supplied `mutation_application_id` (resolved via `get_mutation_applied(domain_id, session_id, mutation_application_id)` then compared on `record_hash`); (3) the declared source set is non-empty, canonicalizable, and includes `mutation_applied_record_hash`; (4) `domain_id` / `session_id` / `packet_id` / `mutation_application_id` / `produced_by` are non-empty / non-whitespace.

**Deliberately absent from `:v1` (must never appear):** `redaction_profile_id`; any `human_at_status`/human-AT field; `packet_body`; private source-receipt bodies; private source data; plan body; applied-result body; operation list; target list; effect payload; any delivery/acceptance/transport/audit-outcome field; any external-recipient reference; any signature/attestation of delivery; any proposal/vote/quorum/mandate/outcome semantics.

## 10. Implementation constraints for the next PR

The later implementation PR **may**:

- add the `EvidencePacketProducedReceipt` class **only**, conforming to the #2314 contract plus this rung (§9 above);
- add the minimum immediate-predecessor reference (EP1), source-set commitment (EP1/EP2), packet-hash (EP2), redaction-profile-hash (EP3), producer-witness (EP4), and node-stamped timestamp support pinned here;
- add `proof.rs` unit tests and a runtime-slice integration test where the existing receipt pattern supports them (construction / emission / persistence / retrieval), mirroring `mutation_applied_receipt_runtime_slice.rs`.

The later implementation PR **must not**:

- produce, assemble, redact, export, deliver, or audit any evidence packet; the receipt witnesses a *reported* production only;
- add a typed/kernel-readable packet/content/source-set model, a `packet_ref`/`recipient_ref`, a `redaction_profile_id`/registry, or any delivery/acceptance/audit field;
- carry any human/AT status field or imply #2041 completion;
- attach any capability/authority check to `produced_by` (it is opaque actor evidence);
- extend `web/member-shell/` or any evidence surface (rendering stays deferred) unless separately scoped and reviewed; add no fixtures;
- touch OpenAPI / SDK, or publish a served schema;
- auto-close any protected issue (#1748, #2141, #2041) or its own implementation issue — leave it open for maintainer disposition.

## 11. Validation requirements for the implementation PR

Both test tiers the landed classes use, plus the rung-specific checks:

- **`proof.rs` unit tests:** a golden vector pinning the `:v1` `record_hash` of a fixed sample; a determinism test (same inputs ⇒ same hash); a per-field test (every field change, including `mutation_applied_record_hash`, `receipt_set_hash`, `packet_hash`, and `redaction_profile_hash`, ⇒ different hash); a **canonical-set-hash test** (the same set of member `record_hash`es supplied in different input orders ⇒ identical `receipt_set_hash`; a different member set ⇒ different `receipt_set_hash`; empty set disallowed; a declared list containing a duplicate `record_hash` fails closed and persists nothing); a tag-disjointness test asserting `icn:gov:evidence_packet_produced:v1` never collides with — and a comment that it must never converge with — the seven landed tags and the proposal/vote `icn:gov:decision:vN` lineage; a serde/payload-audit test confirming no packet-body / source-body / `redaction_profile_id` / human-AT field is present.
- **Runtime-slice integration test:** emission + field round-trip + non-zero `record_hash` + retrieval; same-identity retry returns the original, never restamped; a different `mutation_application_id` / `mutation_applied_record_hash` / `receipt_set_hash` / `packet_hash` / `redaction_profile_hash` / `produced_by` for the same identity fails closed (`evidence_packet_produced_conflict`); unopened session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (`("ab","c")` vs `("a","bc")` must not alias; two domains sharing a `session_id` never mix).
- **EP1 cross-link test** (§4): the referenced `MutationAppliedReceipt` (by `mutation_applied_record_hash`) must exist in the same `(domain_id, session_id)` with a matching `application_id`; an absent, wrong-session, wrong-domain, or `application_id`-mismatched reference is refused fail-closed and persists nothing; a declared set that omits the immediate predecessor is refused fail-closed; (if set-member verification is implemented) a set member that resolves to no in-session receipt is refused fail-closed.
- **EP2 coverage test** (§5): the serialized payload carries exactly the `:v1` field set — no packet-body / source-body / operation / target / effect field; `packet_hash` and `receipt_set_hash` each participate in `record_hash`.
- **EP3 redaction test** (§6): `redaction_profile_hash` participates in `record_hash`; no `redaction_profile_id`/profile-body field is present.
- **EP4 boundary test** (§7): recording a production performs no domain-state mutation beyond persisting the receipt; `produced_by` is an opaque DID with no attached authority/capability check; no "delivered/accepted/audited/certified/verified-correct/production-ready" claim in the type or its serialization.
- **EP5 human/AT test** (§8): no human/AT status field is present in the type or its serialization; no code path sets or implies a human/AT-complete status.
- **Timestamp test** (§9): two records differing only in `produced_at` share duplicate identity (retry returns original, no conflict); `produced_at` participates in `record_hash` but not in identity.
- **Idempotence / replay test:** a logical production replayed on a second node converges on the original receipt (original stamp, original hash).
- **Privacy grep:** no packet body / source-receipt body / private source text / plan body / applied-result body / operation / target / effect text in any serialized receipt or fixture — fingerprints only.
- **No-overclaim grep:** no "packet delivered / accepted / audited / certified / production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2 / human-AT complete" claims introduced.
- **ADR-0026 envelope check:** the receipt sits at Layer 2, self-hashed, no signature/merkle inheritance claim.
- **Protected close-keyword grep:** the implementation PR carries no closing keyword (fix / close / resolve) adjacent to a protected issue number (#1748, #2141, #2041) — use `Refs` only.

## 12. Deferred work (explicitly out of scope of this rung and its future implementation)

- Any **evidence-packet producer** — code that assembles, redacts, or emits an evidence packet.
- Any **evidence export / delivery / transport / acceptance / audit** mechanism, external-recipient reference, or delivery attestation.
- Any typed/kernel-readable packet/content/source-set model, `packet_ref`/`recipient_ref`, `redaction_profile_id`/profile registry, or verifiable-packet binding beyond the `packet_hash`/`receipt_set_hash`/`redaction_profile_hash` fingerprints.
- Full source-set membership *re-verification* as a mandatory floor (recommended default only in `:v1`; a mandatory-verified set is a later decision if a consumer requires it), and any expansion of the source set to include export/delivery artifacts.
- The baseline-lock `EvidencePacket` type (`icn/crates/icn-baseline-lock/src/evidence.rs`) — a separate, unrelated bundle; not touched, not extended, not merged with this class.
- Member-shell / process-evidence rendering of `EvidencePacketProducedReceipt` (a later separately-scoped fixture-only surface may add it after the receipt lands, as #2312 did for `MutationAppliedReceipt`).
- Action-card triggers (ADR-0027 / #1713).
- The actual **#2041** human/AT pass — parked for a real human-testing phase.
- Production / pilot / NYCN activation / live federation / Phase-2 work.
- entity-auth enforcement (#2081), trusted token issuance (#2080), UnknownLegacy repair (#2274), service hosting, K3s / DNS / Forgejo.

## 13. Non-goals

Restated from #2315 / the #2314 contract — this rung and its future implementation are:

- not an evidence-packet producer; not assembling, redacting, or emitting any packet;
- not evidence export / delivery / transport / acceptance / audit; not certifying acceptance or auditing contents;
- not an action-card trigger; not a general workflow engine; not a policy/authority engine; not a new authorization semantic;
- not a typed/kernel-readable packet/content/source-set model; not a `redaction_profile_id`/registry; not a verifiable-packet binding;
- not a new `ProcessGateKind`; not an `ActivationRequest` object;
- not OpenAPI / SDK / served-schema work; not member-shell implementation; not a fixture change;
- not live/private data handling; not private organizer/member/sponsor/attendee data handling;
- not #2041 completion; not human/AT execution; not #1748 or #2141 closure;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion;
- not the baseline-lock `EvidencePacket` type;
- not proposal / vote / quorum / mandate / outcome semantics.

Receipts record institutional facts. They grant zero authority.

## 14. Implementation sequencing & protected issue state

**Recommendation (matching the applied lane cadence #2307 → #2309 → #2310):** with this decision rung landed on top of the #2314 contract, a contract-conformant implementation PR may add the `EvidencePacketProducedReceipt` class **only**, per §9–§11. The implementation PR must keep #1748 / #2141 / #2041 open unless separately reviewed, and must leave its own issue open for maintainer disposition rather than auto-closing it by side effect.

Protected issue state at authoring: #2313 closed/completed (design contract); #2311 closed/completed (mutation-applied render); #1748 open; #2141 open; #2041 open/parked; #1907 / #2081 / #2080 / #2274 open/untouched.

## 15. Related

Refs #2315.
Refs #2314.
Refs #2313.
Refs #2312.
Refs #2310.
Refs #1748.
Refs #2141.
Refs #2041.
