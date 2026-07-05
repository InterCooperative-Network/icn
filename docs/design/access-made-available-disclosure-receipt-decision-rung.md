# Access, Made-Available, and Disclosure Receipt Decision Rung — R1–R10 (what comes after export-prepared)

**Status:** draft — design / decision rung (not runtime implementation)
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-05
**Source basis:** read against `origin/main` @ `98195a10` (the merged #2329 tip). Code anchors (`icn/crates/icn-governance/src/proof.rs`, `icn/apps/governance/src/receipt_backend.rs`, `icn/apps/governance/src/manager.rs`) were verified at that commit — re-verify before relying on exact line numbers or hashes; they drift.
**Related:** #2330 (this rung's issue) · #1792 (private disclosure/access boundary — closed/completed by #2329, [`PRIVATE_DATA_DISCLOSURE_BOUNDARY.md`](../architecture/PRIVATE_DATA_DISCLOSURE_BOUNDARY.md)) · #2322 (evidence export/delivery boundary contract, [`evidence-export-delivery-boundary.md`](evidence-export-delivery-boundary.md)) · #2324 (EX1–EX8 rung, [`evidence-export-delivery-boundary-decision-rung.md`](evidence-export-delivery-boundary-decision-rung.md) — the sibling rung this document mirrors) · #2326 (`EvidencePacketExportPreparedReceipt` runtime slice — the predecessor this family references) · #2328 (member-shell export-prepared fixture render) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #1868 (decompose `governance:write`) · #2061 (entity-aware request authorization) · #2080 (trusted positive token issuance) · #2081 (treasury entity-auth enforcement cutover) · #2041 (human/AT pass — open/parked) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope)

> Narrow decision document resolving what receipt facts may follow `EvidencePacketExportPreparedReceipt`, per #2330 and the private-data disclosure/access boundary landed in #1792/#2329. It mirrors the `evidence-export-delivery-boundary-decision-rung.md` cadence: land the boundary architecture (#2329), then resolve the hash-participating structure **in writing** before any tag is pinned, then implement. This document decides nothing else: no runtime change, no receipt class added, nothing made available, no access performed, no disclosure evaluated, no redaction applied, no route/OpenAPI/SDK, no gateway/auth change, no member-shell change, no fixture change, no human/AT run. Receipts record institutional facts. They grant zero authority. No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 1. Status basis

Verified live at authoring time (`origin/main` @ `98195a10`):

- **#2329** — private disclosure/access architecture doc — **merged** (`98195a10`); **#1792** — its issue — **closed / completed**. The candidate vocabulary this rung refines (`PrivacyClass`, `DisclosurePolicy`, `PrivateObjectRef`, `RedactionMap`, `AccessReceipt`, `ExportReceipt`) and the staged candidate lifecycle (export-prepared → made-available → access → disclosure-decision → redaction-applied) are pinned there as candidates, not implemented.
- **#2326** — `EvidencePacketExportPreparedReceipt` runtime slice — **landed**; tag `icn:gov:evidence_packet_export_prepared:v1`; the ninth process/evidence receipt class and the direct predecessor this family extends. **#2328** — its member-shell fixture render — **merged**.
- **The predecessor lookup seam this rung relies on exists** (verified in `icn/apps/governance/src/receipt_backend.rs`): `get_evidence_packet_export_prepared(domain_id, session_id, export_id)` resolves the export-prepared receipt via the opaque store under class `evidence_packet_export_prepared`, `key1 =` injective netstring `evidence_packet_export_prepared_composite_key1(domain_id, session_id)`, `key2 = export_id`; mirrored on the manager. A made-available fact can therefore verify its predecessor **fail-closed** (`get_evidence_packet_export_prepared(...)` then compare `record_hash`) and, in the same fetch, verify the echoed `packet_hash` and `recipient_scope_id` at no extra cost.
- **No made-available / access / disclosure / redaction seam exists in the runtime:** no `EvidencePacketMadeAvailable*`, `AccessReceipt`, `DisclosureDecision*`, or `RedactionApplied*` type and no matching identifier in `icn/crates` or `icn/apps` (generic words like *accessed*/*delivered*/*accepted* appear only in unrelated subsystems). This rung pins structure before any tag exists.
- **#1748 / #2141 / #2041 / #1868 / #2061 / #2080 / #2081 / #1907** — all **open**; #2041 remains parked for a real human/AT pass, not attempted here. The authority lanes (#1868/#2061/#2080/#2081) are open and are the reason authority adjudication is deferred (§ R7, R10).

## 2. Current landed boundary

`EvidencePacketExportPreparedReceipt` (`:v1`, #2326) records the **sender-side preparation** of an export: a produced packet artifact was bound to a named recipient scope under a declared export policy, and that binding was recorded. Per its own contract it does **not** assert that anything was made available, delivered, received, accessed, accepted, audited, certified, retrieved, or is legally sufficient; it grants no access and no authority. Its `:v1` fields are `domain_id, session_id, export_id, packet_id, packet_produced_record_hash, packet_hash, export_policy_hash, recipient_scope_id, prepared_by, prepared_at, record_hash`; `record_hash` is the sole equality anchor; `key1 = (domain_id, session_id)`, `key2 = export_id`.

Everything below extends that ladder without letting the generic receipt layer adjudicate authority or witness anything the substrate cannot see.

## 3. Terms to distinguish

The whole family exists to keep these facts separate. Each arrow is a boundary, not an implication:

- **prepared** — an export was staged/bound for a recipient scope under a policy, and recorded (sender-side; #2326, landed).
- **made available** — the prepared export was placed in governed custody where an authorized recipient scope *can* retrieve it, under a disclosure policy (a unilateral availability/custody fact; candidate, this rung).
- **accessed** — an actor accessed, or attempted to access, a restricted object under a disclosure policy and a cited authority basis; the receipt records the *outcome* (candidate, this rung).
- **delivered** — a sender-side transmission report (out of scope; #2322 §5.4 territory, deferred).
- **received** — a recipient-side fact that the recipient obtained the object (institution/domain-package and bridge territory; not generic ICN — #2324 EX4).
- **accepted** — a recipient-side fact that the recipient accepted the object on institutional criteria (institution/domain-package territory; not generic ICN — #2324 EX4).
- **audited** — a report that an external audit occurred (out of near-term scope; #1009 relation).
- **certified** — a report that an external certification occurred (out of near-term scope).
- **legally sufficient** — never asserted by any receipt in this family.

## 4. Decision summary

| # | Question | Decision |
|---|----------|----------|
| R1 | Is made-available generic? | **Yes, narrow.** A generic `EvidencePacketMadeAvailableReceipt` is warranted now that #1792 defines governed custody by *fingerprint* — it records availability to a recipient scope under a disclosure policy, referencing custody/method by fingerprint only. |
| R2 | What comes after export-prepared? | **made-available first**, then access, then (later) disclosure-decision and redaction-applied. |
| R3 | Minimum `AccessReceipt` fact | an actor accessed/attempted a restricted object under a policy and cited authority basis; the receipt records the **outcome**, not the contents. |
| R4 | Access outcomes | `allowed / denied / expired / revoked / not_found_or_not_visible / policy_mismatch`; denied/failed attempts are receiptable when safe; `not_found_or_not_visible` deliberately conflates absence and invisibility to avoid an existence oracle. |
| R5 | Safe generic vs institution fields | fingerprints, opaque ids, scope handles, opaque authority-basis references are generic; scope membership, policy bodies, acceptance criteria, contact data, custody locations are institution/package or #1792-owned and never generic. |
| R6 | Object / vault / policy references | by opaque id + fingerprint only (`private_object_ref_id`, `object_ref_hash`, `disclosure_policy_hash`); never a vault path, URL, endpoint, or the policy body. |
| R7 | Authority-basis boundary | the receipt records an **opaque `authority_basis_hash`** — *what basis was cited*, never *whether it was valid*; validity is #1868/#2061 territory. |
| R8 | Redaction / disclosure-decision boundary | candidate `RedactionAppliedReceipt` and `DisclosureDecisionReceipt`, deferred; neither is access, made-available, delivery, or acceptance. |
| R9 | Member-shell language | plain-language negative boundaries pinned per fact (§ R9). |
| R10 | Deferred until authority work | who-may-make-available / who-may-access / valid authority basis / entity delegation / production token issuance / treasury enforcement all wait on #1868/#2061/#2080/#2081. |

**D1 — the pinned inequality chain (verbatim, load-bearing across the family):**

```
prepared        != made available
made available  != accessed
accessed        != received
received        != accepted
accepted        != audited
audited          != certified
certified        != legally sufficient
```

## R1 — Is made-available generic?

**Question.** Should `made available` be a generic ICN receipt, or stay an institution/package-specific fact?

The #2324 EX2 decision deferred `made available` from the export-prepared `:v1` family with a specific reason: *"recording 'made available' without a governed custody reference would either leak location/endpoint data or record nothing verifiable."* That reason is now resolved. #1792/#2329 defines governed custody and disclosure policy as things a receipt references **by fingerprint** (`disclosure_policy_hash`, `PrivateObjectRef`), and pins the hard rule that surfaces expose *public existence metadata*, never contents or locations. A made-available fact can therefore carry a verifiable custody/method reference (a fingerprint) that leaks no location, endpoint, or contact data.

**Decision R1: `EvidencePacketMadeAvailableReceipt` is warranted as a generic ICN receipt, kept narrow.** It records that a previously prepared evidence-packet export was made available to a recipient scope under a disclosure policy, referencing the availability method and policy by **fingerprint only**. It does **not** prove retrieval, access, receipt, acceptance, audit, certification, delivery, or legal sufficiency. It is a unilateral sender/custodian-side availability fact, not a recipient-side fact.

The custody *location* stays out by construction: `availability_method_hash` is a fingerprint of the method descriptor, never a URL, vault path, endpoint, retrieval token, email, phone, or address. This is the design move that reconciles EX2's earlier deferral with #1792 having landed.

## R2 — What comes after export-prepared?

**Decision R2: the candidate sequence is `made-available → access → disclosure-decision → redaction-applied`** (matching #1792 §7), with **made-available first** (§ D3). Access is second because it depends on authority/subject-identity (#1868/#2061). Disclosure-decision and redaction-applied are later still — they govern *whether* and *how* disclosure happens and are only useful once access exists.

## R3 — What is the minimum `AccessReceipt` fact?

**Decision R3.** The minimum access fact is: *an authenticated actor scope accessed, or attempted to access, a referenced restricted object under a named disclosure policy, citing an authority basis, with a recorded outcome.* The receipt carries references and fingerprints (object ref, policy hash, purpose hash, authority-basis hash) and the outcome enum — **never the object contents** and never an adjudication of whether the cited authority was valid. It is candidate only in this rung (§ Candidate field layouts); its exact runtime shape is refined by a later `docs(process)` rung (§ D10 step 3) once #1868/#2061 clarify the authority-basis representation.

## R4 — Access outcomes

**Decision R4.** Access records one of six outcomes, as separate facts:

```
allowed
denied
expired
revoked
not_found_or_not_visible
policy_mismatch
```

- **Attempted and denied access are receiptable institutional facts** when safe — an access-control system that records only successful reads cannot prove misuse was refused. `denied`, `expired`, `revoked`, and `policy_mismatch` are all recorded.
- **`not_found_or_not_visible` deliberately conflates "the object does not exist" with "you are not permitted to see it exists"** — the same enumeration-safe posture as the gateway's enumeration-safe 404 (#1642). This prevents the access receipt from becoming an **existence oracle** for hidden objects, honoring #1792's "no public disappearance of institutional power … unless a narrowly justified safety policy" rule from the safe side.
- Public/member surfaces may need to **redact or aggregate** access receipts: an access receipt must record outcomes without leaking private contents, and without revealing a hidden object's existence where safety policy forbids it. Whether a given outcome is member-visible, steward-visible, or aggregate-only is disclosure-policy territory, not a generic receipt field.

## R5 — Safe generic fields vs institution/package fields

**Decision R5.**

- **Safe for generic runtime:** `domain_id`, `session_id`, opaque per-fact ids (`availability_id`, `access_id`, `request_id`), predecessor `*_record_hash` proof links, echoed-and-verified fingerprints (`packet_hash`), caller-supplied fingerprints of bodies-never-stored (`disclosure_policy_hash`, `availability_method_hash`, `purpose_hash`, `authority_basis_hash`, `redaction_policy_hash`, `redaction_map_hash`), opaque scope handles (`recipient_scope_id`, `actor_scope_id`), private-object references (`private_object_ref_id`, `object_ref_hash`), outcome/decision enums, recorder DIDs (`made_available_by`, `accessed_by`/`decided_by`/`applied_by`), and node-stamped timestamps.
- **Must remain institution/package-specific or #1792-owned (never generic receipt content):** scope membership and definitions; disclosure-policy bodies; acceptance criteria; audit/certification criteria; real names, DIDs-of-persons, emails, phone numbers, addresses, accommodation needs, demographic data; custody locations, endpoints, vault paths, retrieval tokens; the private object contents themselves.

## R6 — Private object / scoped vault / disclosure policy references

**Decision R6.** Restricted objects are referenced **only** by an opaque id plus a content-addressed fingerprint, per #1792's `PrivateObjectRef`:

- `private_object_ref_id` — the caller-opaque handle naming *which* private object.
- `object_ref_hash` — the content-addressed fingerprint of that reference.
- `disclosure_policy_hash` — a fingerprint of the disclosure policy governing the object/act; the **policy body is never stored** (mirrors `export_policy_hash`/`redaction_profile_hash` precedent).
- Scoped-vault location, endpoints, and retrieval mechanics are **absent** — access to the vaulted bytes is itself the receipted act, and where the bytes live is #1792 scoped-vault / artifact-registry territory, referenced by fingerprint, never by path.

## R7 — Authority-basis boundary

**Decision R7 (the load-bearing deferral mechanism).** Every access/availability fact may carry an **opaque `authority_basis_hash`** — a fingerprint of *what authority basis was cited* for the act. The generic receipt layer records the cited basis; it does **not** adjudicate whether the basis was valid, sufficient, or currently held. Adjudication is the authority model's job (#1868 per-action capabilities, #2061 entity-aware `require_entity_access`), and generic ICN must not hard-code it (the Meaning Firewall keeps evaluative semantics in apps).

This lets future runtime record "what authority basis was cited" without the receipt claiming the act was authorized — "receipts record facts; they do not grant permission." A separate authority-decision fact (aligned with #1868/#2061) may later attest validity; this family never conflates *cited* with *valid*.

## R8 — Redaction and disclosure-decision boundary

**Decision R8.** Two further candidate facts are named and **deferred** (candidate layouts below):

- **`DisclosureDecisionReceipt`** — records that a disclosure request was approved / denied / limited. A disclosure decision is **not** itself access, made-available, delivery, or acceptance; it governs whether a later access *may* occur.
- **`RedactionAppliedReceipt`** — records that a redaction profile was applied to a source artifact to produce a public/redacted artifact fingerprint. It proves a transformation was **recorded**; it does **not** prove the redaction is correct, complete, sufficient, legally compliant, accessible, or accepted by any recipient. (`EvidencePacketProducedReceipt` already carries a `redaction_profile_hash`; a standalone `RedactionAppliedReceipt` is only warranted when redaction happens *outside* packet production — its necessity is itself a later decision.)

## R9 — Member-shell negative-boundary language

**Decision R9.** Pinned plain-language rendering (for the later, separately-scoped render rungs; no member-shell change here):

- **Made-available:** *"This evidence packet export was made available to a named recipient scope under a recorded disclosure policy. This does not mean the recipient opened it, retrieved it, received it, accepted it, audited it, certified it, or agreed with it. No location, endpoint, or contents are recorded here."*
- **Access:** *"An access attempt or access event was recorded for a restricted object under a disclosure policy. The receipt records the access outcome and the cited authority basis — not the private contents, and not a ruling that the access was authorized."*
- Both surfaces keep the honesty banner, show fingerprints/handles under a disclosure, and render the negative boundaries as first-class text — mirroring the export-prepared render (#2328). Any such render adds human/AT surface owed under #2041.

## R10 — Deferred until authority work

**Decision R10.** The following are explicitly deferred until #1868 / #2061 / #2080 / #2081 progress, and must **not** be decided by this family:

- who may make an export available;
- who may access a restricted object;
- what authority basis is valid (vs merely cited);
- how entity hierarchy / delegation is enforced;
- how production token issuance binds actor / entity / scope;
- treasury/entity-specific enforcement.

The receipts still define **opaque authority-basis references** (`authority_basis_hash`, `actor_scope_id`) so future runtime can record *what was cited* without adjudicating it in the generic layer.

## Candidate receipt sequence

All candidate, none implemented here. Each references its predecessor by `*_id` + `*_record_hash`, verified fail-closed, echoing verified content fingerprints where cheap:

1. `EvidencePacketExportPreparedReceipt` — **landed** (#2326), the anchor.
2. `EvidencePacketMadeAvailableReceipt` — **first runtime slice** (§ D3); references the export-prepared receipt (the lane's **sixth** inter-receipt link).
3. `AccessReceipt` — candidate; references a `PrivateObjectRef`, gated by the authority model.
4. `DisclosureDecisionReceipt` — candidate, later.
5. `RedactionAppliedReceipt` — candidate, later (only if redaction happens outside packet production).

## Candidate field layouts

All names **candidate — subject to implementation proof and golden-vector pinning**. Each tag must hash-separate from, and never converge with, every landed governance domain tag (verified set at authoring: `icn:governance-proof:v1`; `icn:gov:decision:v1/v2/v3`; `icn:gov:attest:v1`; `icn:gov:action_item_completion:v1/v2`; `icn:gov:meeting_attendance:v1/v2`; `icn:gov:process_gate_result:v1`; `icn:gov:process_session_opened:v1`; `icn:gov:deliberation_entry_recorded:v1`; `icn:gov:decision_recorded:v1`; `icn:gov:activation_crossed:v1`; `icn:gov:mutation_plan_recorded:v1`; `icn:gov:mutation_applied:v1`; `icn:gov:evidence_packet_produced:v1` + `:receipt_set:v1`; `icn:gov:evidence_packet_export_prepared:v1`; `icn:gov:mandate_grant_ref:v1`).

### D4 — `EvidencePacketMadeAvailableReceipt` (candidate `:v1`, first runtime slice)

Candidate tag: `icn:gov:evidence_packet_made_available:v1`. `key1 =` injective netstring `(domain_id, session_id)`; `key2 = availability_id`. Conflict sentinel: `evidence_packet_made_available_conflict`.

| Field | Type | In stable identity? | Meaning |
|-------|------|---------------------|---------|
| `domain_id` | `String` | yes (`key1` half) | governance domain; session must be opened first |
| `session_id` | `String` | yes (`key1` half) | the process session this availability attaches to |
| `availability_id` | `String` | yes (`key2`) | caller-opaque per-availability unit of uniqueness; multiple availabilities per export permitted at the substrate layer (re-availability, policy change) — how many is charter policy |
| `export_id` | `String` | yes | the `EvidencePacketExportPreparedReceipt`'s handle this availability follows (transitive) |
| `packet_id` | `String` | yes | the produced packet handle, inherited transitively through the export-prepared receipt |
| `export_prepared_record_hash` | `Hash` (32) | yes | proof link to the `EvidencePacketExportPreparedReceipt` (verified fail-closed via `get_evidence_packet_export_prepared`); the lane's **sixth** inter-receipt link |
| `packet_hash` | `Hash` (32) | yes | echo of the export-prepared receipt's public/redacted packet fingerprint, **verified equal** to the stored predecessor's `packet_hash` (fail-closed); body never stored |
| `recipient_scope_id` | `String` | yes | echo of the export-prepared receipt's recipient scope, **verified equal** (fail-closed) — availability must be to the scope the export was prepared for; opaque handle, never contact data |
| `disclosure_policy_hash` | `Hash` (32) | yes | caller-supplied fingerprint of the disclosure policy governing this availability; body never stored; distinct from `export_policy_hash` (which shaped the *preparation*) — not required to equal it |
| `availability_method_hash` | `Hash` (32) | yes | caller-supplied fingerprint of the availability *method descriptor* — **never a URL, vault path, endpoint, retrieval token, location, email, phone, or address**; body never stored |
| `made_available_by` | `String` (DID) | yes | recorder / availability-witness; **grants zero authority** to make available, deliver, certify, or audit |
| `made_available_at` | `u64` | **no** | node-stamped Unix seconds; hashed into `record_hash` but **excluded** from duplicate identity (no-wall-clock cross-node identity doctrine, #2283/#2284; a retry never restamps) |
| `record_hash` | `Hash` (32) | (equality anchor) | canonical blake3; the sole `PartialEq`/`Eq` anchor |

**Candidate stable duplicate identity:** all fields **except** `made_available_at` and `record_hash`.

**Preconditions (all fail-closed; on any failure nothing is persisted):** (1) the `(domain_id, session_id)` session was opened first; (2) an `EvidencePacketExportPreparedReceipt` with `record_hash == export_prepared_record_hash` exists in that same session, and its stored `export_id`, `packet_id`, `packet_hash`, and `recipient_scope_id` equal the supplied values (all resolved in one `get_evidence_packet_export_prepared(domain_id, session_id, export_id)` fetch); (3) `domain_id / session_id / availability_id / export_id / packet_id / recipient_scope_id / made_available_by` are non-empty / non-whitespace.

**Deliberately absent from `:v1` (must never appear):** any `delivered / sent / transmitted / received / accepted / audited / certified / accessed / retrieved` field or claim; any URL / endpoint / vault path / location / retrieval-token / credential / recipient-DID / contact-data value; any status / supersession / withdrawal / challenge field; any packet body / policy body / method body / private data (fingerprints only); any human/AT status field (#2041 stays open); any authority grant, capability, mandate, or legal-sufficiency assertion.

### D5 — `AccessReceipt` (candidate; **not** the first runtime slice)

Candidate tag: `icn:gov:access:v1` (final name at its own rung). Candidate `key2 = access_id`.

| Field | Type | Meaning |
|-------|------|---------|
| `domain_id` | `String` | governance domain |
| `session_id` | `String` | the process session (if the access is session-bound; may be optional at its own rung) |
| `access_id` | `String` | caller-opaque per-access unit |
| `object_ref_hash` | `Hash` (32) | content-addressed fingerprint of the `PrivateObjectRef` accessed |
| `private_object_ref_id` | `String` | caller-opaque handle of the private object |
| `disclosure_policy_hash` | `Hash` (32) | fingerprint of the disclosure policy under which access was evaluated; body never stored |
| `actor_scope_id` | `String` | opaque scope handle of the accessing actor — **not** a person DID or contact data (whether to also record an actor DID is deferred to the AccessReceipt rung) |
| `authority_basis_hash` | `Hash` (32) | opaque fingerprint of the **cited** authority basis (§ R7) — never an adjudication of validity |
| `purpose_hash` | `Hash` (32) | fingerprint of the stated access purpose; purpose body never stored |
| `access_outcome` | enum | `allowed / denied / expired / revoked / not_found_or_not_visible / policy_mismatch` (§ R4) |
| `accessed_at` | `u64` | node-stamped Unix seconds; hashed; excluded from identity |
| `record_hash` | `Hash` (32) | canonical blake3; equality anchor |

**Boundary:** an `AccessReceipt` records an access *fact and outcome* under a *cited* basis. It never exposes the private contents, never asserts the access was authorized, and (via `not_found_or_not_visible`) never becomes an existence oracle for hidden objects.

### D6 — `DisclosureDecisionReceipt` (candidate, later)

| Field | Type | Meaning |
|-------|------|---------|
| `request_id` | `String` | caller-opaque disclosure-request handle |
| `object_ref_hash` (or `private_object_ref_id`) | `Hash` (32) / `String` | the private object the decision concerns |
| `recipient_scope_id` | `String` | opaque scope the decision is about; never contact data |
| `decision` | enum | `approved / denied / approved_with_redactions / deferred / withdrawn / superseded` |
| `decision_basis_hash` | `Hash` (32) | fingerprint of the basis for the decision; body never stored |
| `decided_by` | `String` (DID) | recorder / decision-witness; grants zero authority |
| `decided_at` | `u64` | node-stamped; excluded from identity |
| `record_hash` | `Hash` (32) | equality anchor |

**Boundary:** a disclosure decision is **not** itself access, made-available, delivery, or acceptance.

### D7 — `RedactionAppliedReceipt` (candidate, later)

| Field | Type | Meaning |
|-------|------|---------|
| `source_object_ref_hash` | `Hash` (32) | fingerprint of the source (private) artifact reference |
| `redaction_policy_hash` | `Hash` (32) | fingerprint of the redaction policy applied; body never stored |
| `redacted_artifact_hash` | `Hash` (32) | fingerprint of the resulting public/redacted artifact; body never stored |
| `redaction_map_hash` | `Hash` (32) | fingerprint of the `RedactionMap` (which fields were removed); body never stored |
| `applied_by` | `String` (DID) | recorder; grants zero authority |
| `applied_at` | `u64` | node-stamped; excluded from identity |
| `record_hash` | `Hash` (32) | equality anchor |

**Boundary:** proves a redaction transformation was **recorded**; does **not** prove correctness, sufficiency, legal compliance, accessibility, or recipient acceptance.

## First runtime slice recommendation

**D3 — the first runtime slice after this rung is `EvidencePacketMadeAvailableReceipt`.** It is the smallest next sender/custodian-side boundary after export-prepared; it can remain **policy/vault opaque** (fingerprints only, no location or authority adjudication); and it has a **landed predecessor** to verify fail-closed (`EvidencePacketExportPreparedReceipt`, #2326). `AccessReceipt` is deliberately **not** first: it depends far more directly on authority, subject identity, and the #1868/#2061 authority model, and recording access before those are clearer risks either an under-specified authority-basis field or an existence-oracle leak.

The made-available implementation PR (later, separately authorized) would mirror the export-prepared slice (#2326): the `EvidencePacketMadeAvailableReceipt` class only, per D4, with fail-closed predecessor + echoed-field verification, `proof.rs` unit tests (golden vector, determinism, per-field, tag-disjointness, serde payload-audit), and a runtime-slice integration test mirroring `evidence_packet_export_prepared_receipt_runtime_slice.rs`. It must add no field from the deliberately-absent list, no route/OpenAPI/SDK, no member-shell/fixture change (its render is its own later rung), no authority check on `made_available_by`, and it must keep the protected issues open (`Refs` only).

## Non-goals

This rung and its future implementation are: not a custodian, exporter, transport, notification, or delivery service; not delivery, receipt, access-authorization, acceptance, audit, or certification; not a vault runtime, encryption, or private-data schema; not a new authorization semantic (recording an availability or access grants and proves no permission); not an action-card trigger or workflow engine; not a route/OpenAPI/SDK change; not a gateway/auth enforcement change; not a member-shell, operator-dashboard, or fixture change; not live/private data handling; not NYCN-specific semantics in generic ICN; not #2041 completion; not #1748 / #2141 / #1868 / #2061 / #2080 / #2081 closure; not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion; not legally sufficient anything.

Receipts record institutional facts. They grant zero authority.

## Follow-up issue sequence

Proposed, **not opened** by this rung (opening them is a later, separately-authorized step):

1. `feat(process)`: emit `EvidencePacketMadeAvailableReceipt` runtime slice (per D4).
2. `feat(process)`: render `EvidencePacketMadeAvailableReceipt` in the member-shell process-evidence fixture (mirror #2328).
3. `docs(process)`: define `AccessReceipt` runtime decision details (actor-DID-vs-scope, session-boundedness, outcome surfacing) once #1868/#2061 clarify authority basis.
4. `feat(process)`: emit `AccessReceipt` runtime slice.
5. `feat(member-shell)`: render access / privacy outcome states.
6. `test(a11y)`: human/AT pass over privacy/access states (extends #2041 §4G).
7. `docs(process)`: define `DisclosureDecisionReceipt` / `RedactionAppliedReceipt` receipts.
8. `docs(process)`: challenge / repair path for access misuse (append-only supersession).

## Related

Refs #2330.
Refs #1792.
Refs #2322.
Refs #2324.
Refs #2326.
Refs #2328.
Refs #1748.
Refs #2141.
Refs #1868.
Refs #2061.
Refs #2080.
Refs #2081.
Refs #2041.
