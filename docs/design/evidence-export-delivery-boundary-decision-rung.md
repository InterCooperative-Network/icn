# Evidence export/delivery boundary decision rung — EX1–EX8 (fact name, availability vs delivery, recipient scope, acceptance placement, v1 field set, vault/access interaction, challenge shape, smallest dogfood)

**Status:** draft — design / decision rung (not runtime implementation)
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-05
**Source basis:** read against `origin/main` @ `87b425cf` (the merged #2322 boundary contract's tip). Code anchors (`icn/crates/icn-governance/src/proof.rs`, `icn/apps/governance/src/receipt_backend.rs`, `icn/apps/governance/src/manager.rs`) were verified at that commit — re-verify before relying on exact line numbers or hashes; they drift.
**Related:** #2323 (this rung's issue) · #2321 (the boundary-contract issue — closed/completed by #2322) · #2322 (merged boundary contract, [`evidence-export-delivery-boundary.md`](evidence-export-delivery-boundary.md)) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #1792 (private data disclosure boundary, scoped vaults, and access receipts — open) · #2041 (human/AT pass — open/parked) · #2318 (`EvidencePacketProducedReceipt` runtime slice) · #2320 (member-shell process-evidence fixture render) · [`evidence-packet-produced-receipt-decision-rung.md`](evidence-packet-produced-receipt-decision-rung.md) (the sibling EP rung this document mirrors) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope)

> Narrow decision document resolving the eight open questions the merged #2322 evidence export/delivery boundary contract named in its §9 — **EX1** (fact name and moment), **EX2** (availability vs delivery), **EX3** (recipient-scope representation), **EX4** (where acceptance lives), **EX5** (minimal v1 field set), **EX6** (vault/access interaction), **EX7** (challenge/rejection shape), **EX8** (smallest repo-safe dogfood). It mirrors the `evidence-packet-produced-receipt-decision-rung.md` cadence: land the boundary contract (#2322), then resolve the hash-participating structure **in writing** before a tag is pinned, then implement. This document decides nothing else: no runtime change, no receipt class added, no export performed, no member-shell change, no fixture change, no human/AT run. Receipts record institutional facts. They grant zero authority.

## 1. Purpose

The #2322 boundary contract separated the lifecycle facts that could follow `EvidencePacketProducedReceipt` — produced (landed) / export-prepared-or-exported / made available / delivered / received / accepted / audited-certified / challenged-rejected-withdrawn — and recommended the **export fact** as the single next rung, deferring its exact name, moment, and structure to this decision document. The landed rule (applied by every rung since #2278, most recently #2316) is that **hash-participating structure is decided in writing before a tag is pinned, never silently in an implementation PR.**

This rung resolves EX1–EX8 so a contract-conformant implementation PR *could* begin, if separately authorized. It is not an exporter, not a transport, not a delivery/acceptance/audit mechanism, and not an action-card trigger.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `87b425cf`):

- **#2322** — evidence export/delivery boundary design/audit contract — **merged** (`87b425cf`); **#2321** — its issue — **closed / completed**.
- **#2318** — `EvidencePacketProducedReceipt` runtime slice — **landed** (`5f898b92`); the eighth `ProcessTransitionReceipt` class. **#2320** — its member-shell fixture render — **merged** (`cf3e7d47`).
- **#1748 / #2141 / #1792 / #2041** — all **open**; #2041 remains parked for a real human/AT pass, not attempted here.
- **No evidence-packet export seam exists in the runtime:** no `EvidencePacketExport*` type and no `evidence_packet_export*` / `export_prepared` identifier anywhere in `icn/crates` or `icn/apps` (verified by scoped `rg`, per the #2322 audit discipline — generic words like *exported*/*delivered*/*accepted* appear only in unrelated subsystems).
- The predecessor lookup seam this rung relies on **exists**: `get_evidence_packet_produced(domain_id, session_id, packet_id)` resolves the produced receipt via `get_latest_opaque(EVIDENCE_PACKET_PRODUCED_CLASS, key1, Some(packet_id))` under the injective `evidence_packet_produced_composite_key1(domain_id, session_id)` (`icn/apps/governance/src/receipt_backend.rs`), mirrored on the manager (`icn/apps/governance/src/manager.rs`).

No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

## 3. Repo audit update (verified against live code)

| Subject | Finding | Anchor |
|---------|---------|--------|
| `EvidencePacketProducedReceipt` (the predecessor this fact references) | fields `domain_id, session_id, packet_id, mutation_application_id, mutation_applied_record_hash, receipt_set_hash, packet_hash, redaction_profile_hash, produced_by, produced_at, record_hash`; tag `icn:gov:evidence_packet_produced:v1`; `record_hash` is the sole `PartialEq`/`Eq` anchor; `packet_hash` fingerprints the public/redacted packet artifact (body never stored) | `icn/crates/icn-governance/src/proof.rs` (struct + `DOMAIN_TAG` + `compute_record_hash`) |
| produced lookup / uniqueness | `put_evidence_packet_produced` persists via `put_opaque_if_absent` under class `"evidence_packet_produced"`, `key1 =` injective netstring `evidence_packet_produced_composite_key1(domain_id, session_id)`, `key2 = packet_id`; `get_evidence_packet_produced(domain_id, session_id, packet_id)` reads it back. An export fact can therefore verify its predecessor **fail-closed** by `get_evidence_packet_produced(...)` then comparing `record_hash` — and, because the fetched receipt carries `packet_hash`, an echoed `packet_hash` can be verified in the same fetch at no extra cost | `receipt_backend.rs`, `manager.rs` |
| the eight landed classes | `ProcessSessionOpenedReceipt` / `DeliberationEntryRecordedReceipt` / `DecisionRecordedReceipt` / `ProcessGateResultReceipt` / `ActivationCrossedReceipt` / `MutationPlanRecordedReceipt` / `MutationAppliedReceipt` / `EvidencePacketProducedReceipt` are the only runtime `ProcessTransitionReceipt` classes | `proof.rs` |
| inter-receipt references | exactly **four** exist: activation → decision (#2295 B1), plan → activation (#2302 M1), applied → plan (#2309 A1), packet → applied + source set (#2316 EP1). An export → produced link would be the lane's **fifth** | whole-repo search |
| landed domain tags (disjointness set) | `icn:governance-proof:v1`; `icn:gov:decision:v1/v2/v3`; `icn:gov:attest:v1`; `icn:gov:action_item_completion:v1/v2`; `icn:gov:meeting_attendance:v1/v2`; `icn:gov:process_gate_result:v1`; `icn:gov:process_session_opened:v1`; `icn:gov:deliberation_entry_recorded:v1`; `icn:gov:decision_recorded:v1`; `icn:gov:activation_crossed:v1`; `icn:gov:mutation_plan_recorded:v1`; `icn:gov:mutation_applied:v1`; `icn:gov:evidence_packet_produced:v1` (+ its `:receipt_set:v1` sub-tag); `icn:gov:mandate_grant_ref:v1` | `proof.rs` |
| export vocabulary | #1792 names candidate `ExportReceipt` (`object_ref, exported_by, recipient_scope, redaction_policy, reason, receipt_hash`) and `AccessReceipt`, plus the hard rule "No sensitive export without redaction policy and export receipt"; the shipped `urn:icn:contract:rehearsal-evidence-export:v1` remains a read-only **summary** contract, not a lifecycle fact | #1792; `docs/contracts/` |
| framing scope note | the idea-0019 framing brief names **eight** `ProcessTransitionReceipt` classes ending at `EvidencePacketProducedReceipt`. An export fact is the **first class beyond the named spine** — same family discipline, but it extends the framing rather than filling it; it must not be presented as completing an existing #1748 acceptance gate | `ops/ideas/framing/institutional-process-substrate.md` |

**Bottom line:** every seam the EX decisions rely on exists and is verified; the export fact itself has no seam and no precedent, so this rung pins its structure before any tag exists.

## 4. EX1 decision — fact name and moment

**Question.** Is the v1 fact `export-prepared` (an export staged under a policy) or `exported` (a release act)? The #2322 rule: the chosen name must not imply transmission.

Options considered:

1. **`exported`** (`EvidencePacketExportedReceipt`). Rejected: "exported" is passively readable as "it left / it was sent" — precisely the transmission implication #2322 forbids. The substrate cannot witness departure, only a recorder's report; a name that suggests more than the witness posture supports is a standing overclaim.
2. **`export-recorded`**. Rejected: says nothing about *which* moment was recorded; "an export was recorded" invites the same "it left" reading with extra vagueness.
3. **`export-prepared`** (`EvidencePacketExportPreparedReceipt`). **CHOSEN.** The fact is the sender-side **preparation/staging** of an export: a specific produced packet artifact was bound to a named recipient scope under a declared export policy, and that binding was recorded. Nothing in the name or the fact asserts that anything moved.

**Decision EX1: the v1 fact is `export-prepared`.**

- Candidate class name: `EvidencePacketExportPreparedReceipt` (keeps the `EvidencePacket*` family prefix legible next to the produced class).
- Candidate tag: `icn:gov:evidence_packet_export_prepared:v1` — must hash-separate from, and never converge with, every tag in the §3 disjointness set.
- The *release act* (§5.4 of the boundary contract, `delivered`) remains a separate, deferred fact (EX2). "Prepared" is the strongest claim the sender-side witness posture supports without transmission semantics.
- Surface language (pinned for any future rendering): *"An export of this evidence packet was prepared for a named recipient scope and recorded here. This does not mean it was delivered, received, or accepted."*

**Test that proves it:** a no-overclaim grep over the future type and serialization asserting no `delivered` / `sent` / `transmitted` / `received` / `accepted` field or claim; the doc-comment states the prepared ≠ delivered boundary verbatim.

## 5. EX2 decision — availability vs delivery

**Question.** Is `made available` a distinct fact from `delivered`, and does either enter v1?

Options considered:

1. Fold availability into delivery. Rejected: they have different witnesses and different truth conditions — availability is a **unilateral custody fact** (the artifact sits where an authorized scope can retrieve it), delivery is a **claimed transmission act**. Folding them makes one receipt ambiguous between "it is retrievable" and "we pushed it," which is exactly the drift the boundary contract exists to prevent.
2. Put one or both in v1 alongside export-prepared. Rejected: availability needs the scoped-vault/custody seam (#1792) to mature — recording "made available" without a governed custody reference would either leak location/endpoint data or record nothing verifiable; delivery needs transport-class decisions with no current consumer.
3. **Distinct facts; both excluded from v1. CHOSEN.**

**Decision EX2: `made available` and `delivered` are distinct lifecycle facts, and neither enters v1.** The v1 family stops at export-prepared. Any later availability fact must reference governed custody (vault reference, per #1792) and any later delivery fact records a sender-side transmission report only — each requires its own contract + rung.

**Test that proves it:** the serialized v1 payload carries no availability / custody / location / vault / transport / delivery field (payload-audit test).

## 6. EX3 decision — recipient-scope representation

**Question.** Is the recipient scope a handle, a fingerprint, a DID, or a combination — and how does contact data stay out by construction?

Options considered:

1. Recipient DID(s). Rejected: enumerating recipients inside a receipt turns the receipt into a disclosure surface (who may see what is itself sensitive), and person-level recipients invite contact data. Scope membership is #1792 disclosure-policy territory, not receipt content.
2. `recipient_scope_id` + `recipient_scope_hash` (a fingerprint of the scope definition). Rejected for v1: the scope-definition body (membership, rules) is a **disclosure policy** owned by the #1792 lane; fingerprinting it here would pin a policy-body format that #1792 has not defined. Same reasoning that EP3 rejected `redaction_profile_id`: do not pin a second representation with no repo precedent.
3. **Caller-opaque `recipient_scope_id` string only. CHOSEN.** Mirrors every caller-opaque handle in the lane (`session_id`, `packet_id`, `application_id`); names *which* scope without carrying *what* the scope is.

**Decision EX3: v1 carries a single caller-opaque `recipient_scope_id`.**

- Non-empty / non-whitespace, validated pre-persistence like the sibling ids.
- **Hard rule (contractual, stated in the type doc-comment and enforced by fixture/privacy review):** `recipient_scope_id` is an opaque governance handle — never a name, email, phone number, address, or any personal contact data. The same rule already governs every caller-opaque id in the lane; this rung restates it because recipient identifiers are the highest-temptation field for contact data (#1792 hard rules).
- No scope-definition hash, no scope registry, no recipient enumeration in v1. If #1792 later defines a fingerprintable disclosure-policy body, adding a hash is a later decision, not a silent v1 add.

**Test that proves it:** payload-audit test confirms exactly one recipient field (`recipient_scope_id`, string); privacy grep over fixtures/tests confirms no contact-data-shaped values; empty/whitespace id rejected pre-persistence.

## 7. EX4 decision — where acceptance lives

**Question.** Do recipient-side facts (`received`, `accepted`) belong in generic ICN, or in institution/domain-package and bridge semantics?

Options considered:

1. Generic ICN receipt classes for received/accepted. Rejected: the substrate can only witness authenticated reports; a sender-side runtime recording "received/accepted" would let the sender fabricate the recipient's facts, violating the #2322 witness posture. A recipient inside the substrate could attest — but the *criteria* for acceptance are institutional, and generic ICN must not hard-code them (the meaning firewall keeps evaluative semantics in apps/packages).
2. **Recipient-side facts stay out of generic ICN. CHOSEN.** The substrate provides the receipt grammar; institution/domain packages define acceptance semantics on the recipient's own authority; external recipients arrive only through bridges, which emit import/translation receipts per the operating model.

**Decision EX4: `received` and `accepted` are institution/domain-package and bridge territory, not generic ICN.** No recipient-side fact enters this family without its own contract and rung, recorded on recipient authority. Nothing in v1 may imply either.

**Test that proves it:** payload-audit + no-overclaim grep: no `received` / `accepted` / recipient-attestation field or claim in the v1 type or serialization.

## 8. EX5 decision — minimal v1 field set

**Question.** What is the smallest identity-honest field set for `export-prepared`, what participates in `record_hash` and duplicate identity, and what are the preconditions?

Options considered per field group:

- **Predecessor link.** Mirror the lane's proven scalar posture (`*_id` + `*_record_hash`, verified fail-closed): `packet_id` + `packet_produced_record_hash` naming the `EvidencePacketProducedReceipt` this export prepares. The lane's fifth inter-receipt link. Chosen. (Transitivity: applied/plan/activation/decision/gate are inherited through the produced receipt, not restated — same rule as every prior rung.)
- **Artifact fingerprint.** Options: (a) no artifact hash (binding-only fact); (b) a new `export_artifact_hash` for a possibly-distinct export-shaped artifact; (c) **echo the produced receipt's `packet_hash`, verified fail-closed against the stored produced receipt. CHOSEN.** (a) leaves "an export of *what bytes*" answerable only by lookup; (b) invents an artifact no producer produces (v1 exports the produced artifact itself — a wrapped/re-encoded export form is a later version if a real producer ever needs it). The echo makes the receipt self-contained *and* is a proof rather than an assertion, because the predecessor verify already fetches the produced receipt — comparing `packet_hash` costs nothing. This is the lane's first **verified echoed content field** (predecessor echoes to date verify `record_hash` only).
- **Policy fingerprint.** `export_policy_hash` — caller-supplied 32-byte fingerprint of the export policy that shaped/authorized this preparation for this scope; the policy **body is never stored** (mirrors `redaction_profile_hash` exactly, EP3 precedent; aligns with #1792's `ExportReceipt.redaction_policy` and its "no sensitive export without redaction policy and export receipt" hard rule). It records *which* policy, not that the policy is complete, correct, satisfied, or legally sufficient.
- **Recipient.** `recipient_scope_id` per EX3.
- **Actor / time.** `prepared_by` (DID; recorder / export-witness evidence, **not** an authority to export, release, deliver, certify, or audit — grants zero authority) and `prepared_at` (node-stamped Unix seconds, hashed, **excluded** from duplicate identity; the #2283/#2284 doctrine — no wall clock in cross-node identity; a retry never restamps).

**Decision EX5: the candidate v1 layout is the 11-field set in §12**, with `export_id` as the caller-opaque per-export unit of uniqueness (`key2`). **Multiple exports per packet are permitted at the substrate layer** (different scopes, different policies, re-preparations) — `export_id` is the uniqueness unit; how many, and to whom, is charter policy (same posture as decisions-per-session).

## 9. EX6 decision — vault/access interaction

**Question.** Is the export artifact vaulted, and is retrieval an #1792 `AccessReceipt` rather than anything in this family?

**Decision EX6: v1 carries no custody, vault, location, or retrieval semantics.** Where the prepared export artifact lives is governed-custody territory (#1792 scoped vaults / the artifact-registry-and-scoped-vault spec); *access to it is a receipted act in the #1792 `AccessReceipt` lane, not in this family.* A future `made available` fact (EX2, deferred) would be the point where a governed custody reference enters — with its own rung. This keeps the export-prepared receipt free of endpoints, retrieval tokens, and location data by construction.

**Test that proves it:** payload-audit test — no vault / location / URL / credential / retrieval field in the v1 type or serialization.

## 10. EX7 decision — challenge/rejection shape

**Question.** How do challenged/rejected/withdrawn facts appear, and does any of it enter v1?

**Decision EX7: fully deferred from v1.** Receipts in this family are append-only facts; withdrawal or challenge never deletes or mutates a prior receipt. When a real consumer needs dispute facts, they must be designed against the #1009 attestation/dispute pathway as their own contract — referencing the challenged fact by `record_hash` — and #1792's challenge-path requirement. v1 carries no status, supersession, revocation, or dispute field.

**Test that proves it:** payload-audit test — no status / superseded / withdrawn / challenged field in v1.

## 11. EX8 decision — smallest repo-safe dogfood

**Question.** What is the minimal fixture-only path that tells the export story without performing any export?

**Decision EX8: mirror the landed render cadence (#2291 → #2312 → #2320), only after the runtime class lands.** The smallest honest dogfood is a fixture-only member-shell extension of `?mode=demo&set=process-evidence`: one wire-shaped `evidence_packet_export_prepared` fixture entry whose `packet_produced_record_hash` echoes the fixture's produced receipt, a fictional `recipient_scope_id` (e.g. `demo-partner-review-scope-0001` — fictional, contact-data-free), illustrative hashes, and the EX1 surface language ("prepared … not delivered, received, or accepted"). It must re-run the committed a11y walkthrough and the rehearsal-evidence-export validator, keep #2041 owed, and claim no readiness. **No fixture or member-shell change happens in this rung or in the runtime implementation PR** — the render is its own later, separately-scoped rung.

## 12. Consolidated candidate `:v1` layout (for a later, separately authorized implementation PR)

All names **candidate — subject to implementation proof and golden-vector pinning**. The tag must hash-separate from, and never converge with, every tag in the §3 disjointness set.

| Field | Type | In stable identity? | Source |
|-------|------|---------------------|--------|
| `domain_id` | `String` | yes (`key1` half) | anchor |
| `session_id` | `String` | yes (`key1` half) | anchor; session must be opened first |
| `export_id` | `String` | yes (`key2`) | caller-opaque per-export id |
| `packet_id` | `String` | yes | **EX5** — the produced packet's human/index handle (must exist in-session) |
| `packet_produced_record_hash` | `Hash` (32) | yes | **EX5** — proof link to the `EvidencePacketProducedReceipt` (verified fail-closed); the lane's fifth inter-receipt link |
| `packet_hash` | `Hash` (32) | yes | **EX5** — echo of the produced receipt's public/redacted artifact fingerprint, **verified equal to the stored produced receipt's `packet_hash`** (fail-closed); body never stored |
| `export_policy_hash` | `Hash` (32) | yes | **EX5** — fingerprint of the export policy that shaped this preparation; policy body never stored |
| `recipient_scope_id` | `String` | yes | **EX3** — caller-opaque scope handle; never contact data |
| `prepared_by` | `String` (DID) | yes | **EX1/EX5** — recorder / export-witness; grants zero authority |
| `prepared_at` | `u64` | **no** | node-stamped Unix seconds; hashed; excluded from identity (retry never restamps) |
| `record_hash` | `Hash` (32) | (equality anchor) | canonical blake3; the sole `PartialEq`/`Eq` anchor |

**Candidate canonical hashing:** `DOMAIN_TAG` (`icn:gov:evidence_packet_export_prepared:v1`) first → length-prefixed `domain_id`, `session_id`, `export_id`, `packet_id`, `recipient_scope_id`, `prepared_by` → `packet_produced_record_hash` raw 32 → `packet_hash` raw 32 → `export_policy_hash` raw 32 → `prepared_at` LE. Exact layout is fixed by the implementation PR and pinned by a golden vector.

**Candidate stable duplicate identity:** `(domain_id, session_id, export_id, packet_id, packet_produced_record_hash, packet_hash, export_policy_hash, recipient_scope_id, prepared_by)`. `prepared_at` and `record_hash` are **not** identity.

**Uniqueness / conflict:** `put_opaque_if_absent` keyed on `(class, key1, key2)` — `key1` an injective netstring composite of `(domain_id, session_id)`, `key2 = export_id`. Same-identity retry ⇒ the **original** receipt, never restamped; a different stable identity for the same `(domain_id, session_id, export_id)` ⇒ fail-closed **`evidence_packet_export_prepared_conflict`**.

**Preconditions (all fail-closed; on any failure nothing is persisted):** (1) the `(domain_id, session_id)` session was opened first; (2) an `EvidencePacketProducedReceipt` with `record_hash == packet_produced_record_hash` exists in that same session, its `packet_id` equals the supplied `packet_id`, **and its stored `packet_hash` equals the supplied `packet_hash`** (all resolved in one `get_evidence_packet_produced(domain_id, session_id, packet_id)` fetch); (3) `domain_id` / `session_id` / `export_id` / `packet_id` / `recipient_scope_id` / `prepared_by` are non-empty / non-whitespace.

**Deliberately absent from `:v1` (must never appear):** any `delivered` / `sent` / `transmitted` / `received` / `accepted` / `audited` / `certified` field or claim (EX1/EX4); availability / custody / vault / location / URL / endpoint / retrieval-token / transport field (EX2/EX6); recipient DIDs, recipient enumeration, scope-definition body or hash, or any contact data (EX3); status / supersession / withdrawal / challenge field (EX7); packet body, export-policy body, redaction-profile body, source-receipt bodies, or any private data (bodies are never stored — fingerprints only); any human/AT status field (#2041 stays open); any authority grant, capability, mandate, or legal-sufficiency assertion.

## 13. Implementation constraints for the (later, separately authorized) PR

The implementation PR **may**: add the `EvidencePacketExportPreparedReceipt` class **only**, per §12; add the fail-closed predecessor + packet-hash-echo verification; add `proof.rs` unit tests and a runtime-slice integration test mirroring `evidence_packet_produced_receipt_runtime_slice.rs`.

The implementation PR **must not**: prepare, assemble, export, deliver, or transmit any artifact (the receipt witnesses a *reported* preparation only); add any field from the deliberately-absent list; add a route, OpenAPI/SDK, or served schema; extend `web/member-shell/` or any fixture (EX8's render is its own later rung); attach any capability/authority check to `prepared_by`; imply #2041 completion; auto-close any protected issue (#1748, #2141, #1792, #2041) or its own issue — `Refs` only; present the ninth class as completing an existing #1748 acceptance gate (§3 framing note — it extends the family beyond the eight named classes).

## 14. Validation requirements for the implementation PR

- **`proof.rs` unit tests:** golden vector pinning the `:v1` `record_hash`; determinism test; per-field test (every field change ⇒ different hash, including `packet_produced_record_hash`, `packet_hash`, `export_policy_hash`); tag-disjointness test against every tag in the §3 set, with the never-converge comment; serde payload-audit test confirming no deliberately-absent field is present.
- **Runtime-slice integration test:** emission + round-trip + non-zero `record_hash` + retrieval; same-identity retry returns the original un-restamped; conflicting identity fails closed (`evidence_packet_export_prepared_conflict`); unopened session fails closed and creates nothing; empty/whitespace ids rejected pre-persistence; missing receipt store / backend failure fail closed; concurrent duplicates serialize to one winner; composite key injective (no aliasing; two domains sharing a `session_id` never mix).
- **EX5 cross-link tests:** absent / wrong-session / wrong-domain / `packet_id`-mismatched predecessor refused fail-closed and persists nothing; **`packet_hash` mismatch against the stored produced receipt refused fail-closed** (the echoed-field verification); multiple exports per packet with distinct `export_id`s succeed.
- **Timestamp test:** two records differing only in `prepared_at` share duplicate identity; `prepared_at` participates in `record_hash` but not identity.
- **Privacy grep:** no packet body / policy body / contact-data-shaped value / endpoint / URL in any serialized receipt, fixture, or test.
- **No-overclaim grep:** no "delivered / sent / transmitted / received / accepted / audited / certified / legally sufficient / production / pilot / organizer-ready / member-ready / live federation / NYCN / Phase-2 / human-AT complete" claims introduced.
- **ADR-0026 envelope check:** Layer 2, self-hashed, no signature/merkle inheritance claim.
- **Protected close-keyword grep:** no closing keyword adjacent to a protected issue number — `Refs` only.

## 15. Deferred work (explicitly out of scope of this rung and its future implementation)

- Any **exporter/producer** — code that assembles, wraps, redacts, or stages an export artifact.
- `made available` and `delivered` facts (EX2), any custody/vault binding (EX6 — #1792 territory), and any `AccessReceipt` integration.
- Recipient-side `received` / `accepted` facts (EX4 — institution/domain-package and bridge territory).
- `audited` / `certified` report-of-external-outcome facts (out of near-term scope per the boundary contract §5.7; #1009 relation).
- Challenge / rejection / withdrawal facts (EX7; #1009, #1792 challenge path).
- The member-shell / fixture render of the export-prepared fact (EX8 — its own later rung after the runtime lands).
- A scope-definition hash or registry (EX3 — awaits #1792's disclosure-policy model).
- OpenAPI/SDK/served-schema publication (the family-wide decision remains a future rung).
- Production / pilot / NYCN activation / live federation / Phase-2 work; the actual #2041 human/AT pass.

## 16. Non-goals

Restated from #2323 — this rung and its future implementation are: not an exporter, transport, notification, or delivery service; not delivery, receipt, acceptance, audit, or certification; not a typed packet/content/scope model; not a new authorization semantic (recording a preparation grants no permission to release anything); not an action-card trigger; not a workflow engine; not OpenAPI/SDK work; not a member-shell or fixture change; not live/private data handling; not #2041 completion; not #1748 / #2141 / #1792 closure; not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion; not legally sufficient anything.

Receipts record institutional facts. They grant zero authority.

## 17. Implementation sequencing & protected issue state

**Recommendation (matching the lane cadence):** with this rung landed on top of the #2322 contract, a contract-conformant implementation PR *may* add the `EvidencePacketExportPreparedReceipt` class **only**, per §12–§14 — subject to separate explicit authorization. After the runtime lands, the EX8 fixture render is its own rung. The implementation PR must keep #1748 / #2141 / #1792 / #2041 open and leave its own issue open for maintainer disposition.

Protected issue state at authoring: #2321 closed/completed (boundary contract); #2319 closed/completed (produced render); #1748 open; #2141 open; #1792 open; #2041 open/parked; #1907 / #2080 / #2081 / #2274 open/untouched.

## 18. Related

Refs #2323.
Refs #2322.
Refs #2321.
Refs #2318.
Refs #2320.
Refs #1748.
Refs #2141.
Refs #1792.
Refs #2041.
