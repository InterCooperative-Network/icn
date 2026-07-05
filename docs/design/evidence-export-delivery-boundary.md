# Evidence Export and Delivery Boundary — Design/Audit Contract

**Status:** draft — design/audit
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-05
**Source basis:** read against `origin/main` @ `cf3e7d47` (the merged #2320 tip — re-verify before relying on exact anchors; they drift)
**Related:** #2321 (this contract's issue) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #1792 (private data disclosure boundary, scoped vaults, and access receipts — open) · #2041 (human/AT pass — open/parked) · #2318 (`EvidencePacketProducedReceipt` runtime slice — merged) · #2320 (member-shell process-evidence fixture render — merged) · [`evidence-packet-produced-receipt.md`](evidence-packet-produced-receipt.md) (#2314 contract) · [`evidence-packet-produced-receipt-decision-rung.md`](evidence-packet-produced-receipt-decision-rung.md) (#2316 EP1–EP5 decisions) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance proof envelope) · `docs/contracts/rehearsal-evidence-export.schema.json` (`urn:icn:contract:rehearsal-evidence-export:v1`) · `ops/ideas/framing/institutional-process-substrate.md` (spine framing)

> This is a **design/audit contract for a boundary, not for a receipt class.** It adds no runtime code, no receipt class, no gateway/OpenAPI/SDK change, no member-shell change, and no fixture change. Its job is to prevent semantic drift now that `EvidencePacketProducedReceipt` is landed and rendered: **produced must never quietly become exported, delivered, made available, received, accepted, audited, certified, legally sufficient, human/AT verified, or ready.** It names the lifecycle facts that could follow production, states what each would and would not mean, and recommends the single next narrow rung — subject to a decision rung before any implementation. Receipts record institutional facts. They grant zero authority. No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

---

## 1. Purpose

The process-transition receipt lane under #1748 / #2141 is complete through its terminal *production* fact. All **eight** `ProcessTransitionReceipt` classes are runtime-landed and rendered read-only in the fixture member-shell process-evidence demo:

`ProcessSessionOpened` → `DeliberationEntryRecorded` → `DecisionRecorded` → `ProcessGateResult` → `ActivationCrossed` → `MutationPlanRecorded` → `MutationApplied` → `EvidencePacketProduced`.

The spine framing (`institutional-process-substrate.md`) ends at **evidence** — "the receipt produced, the evidence exported" — and the vertical spine's tail reads `receipt → surface → evidence/export`. Production is now witnessed. What follows production — export, availability, delivery, receipt, acceptance, audit — is **not** witnessed, and the #2316 EP4 decision deliberately pinned that gap: *"Production is a recorded fact about an artifact; export/summary/delivery/acceptance/audit are separate, later concerns this receipt does not assert."*

That deliberate gap is now the drift risk. A reader of the produced receipt (or its member-shell rendering) may assume the packet went somewhere. This document defines the boundary taxonomy so every future fact in this family is named precisely, scoped honestly, and never inflated — and so the next lane can be selected with the same contract → decision rung → implementation cadence the eight landed classes used.

## 2. Status basis

Verified live at authoring time (`origin/main` @ `cf3e7d47`):

- **#2318** — `EvidencePacketProducedReceipt` runtime slice — **merged** (`5f898b92`); eighth `ProcessTransitionReceipt` class, tag `icn:gov:evidence_packet_produced:v1`.
- **#2320** — member-shell process-evidence fixture render of the eighth class — **merged** (`cf3e7d47`); **#2319** — its render issue — **closed / completed**.
- **#1748 / #2141** — milestone / spine control — **open**.
- **#1792** — private data disclosure boundary, scoped vaults, and access receipts — **open**; names candidate `ExportReceipt` and `AccessReceipt` shapes and the hard rule *"No sensitive export without redaction policy and export receipt."* This boundary document defers to #1792's vocabulary wherever the two overlap.
- **#2041** — real screen-reader / low-vision / switch / AT-compat human pass — **open / parked**; not attempted here.
- **No evidence-packet export/delivery/acceptance receipt, route, type, fixture, or field exists in the runtime.** No `EvidencePacketExport*` / `EvidencePacketDelivered*` / `EvidencePacketAccepted*` type and no `evidence_packet_export*` / `evidence_packet_delivered*` / `evidence_packet_accepted*` identifier exists anywhere in `icn/crates` or `icn/apps` (the generic words *exported* / *delivered* / *accepted* do appear throughout unrelated subsystems — keystore export, message delivery, proposal acceptance — none of which is an evidence-packet lifecycle fact). The only landed "export" artifacts are the **read-only rehearsal-evidence-export summary contract** (`urn:icn:contract:rehearsal-evidence-export:v1`) and its fixture (`web/member-shell/fixtures/process-evidence-export.json`) — a demo *summary surface*, not a lifecycle fact (see §4).

## 3. Current landed fact (the floor this boundary stands on)

`EvidencePacketProducedReceipt` records exactly this: **a redacted evidence packet artifact was produced and content-addressed, and that fact was recorded.** It stores hashes, opaque ids, DIDs, and a node-stamped timestamp only:

- `packet_hash` — fingerprint of the public/redacted packet artifact; the body is never stored (EP2);
- `receipt_set_hash` — canonical commitment to the source-receipt references (references, never bodies; EP1/EP2);
- `redaction_profile_hash` — fingerprint of the redaction profile; profile body never stored (EP3);
- `mutation_application_id` + `mutation_applied_record_hash` — verified fail-closed proof link to the immediate prior process boundary (EP1);
- `produced_by` — recorder / producer-witness evidence; **grants zero authority** (EP4);
- `produced_at` — node-stamped, hashed, excluded from duplicate identity.

It proves **no** delivery, availability, receipt, acceptance, correctness, completeness, legal sufficiency, audit certification, human/AT verification, readiness, or live federation (EP4/EP5). Everything below in §5 is *downstream of* and *distinct from* this fact.

## 4. Two meanings of "export" already live in the repo (disambiguation)

The word **export** currently does two different jobs, and this boundary must keep them apart:

1. **The rehearsal-evidence-export summary** (`urn:icn:contract:rehearsal-evidence-export:v1`): a repo-safe, read-only *summary of a receipt sequence* for a demo/rehearsal surface. It is not a receipt, not a lifecycle fact, and asserts nothing about any packet leaving anywhere. The member-shell fixture demo renders it read-only; the surface never generates, downloads, or sends it. This meaning is **already shipped** and stays as-is.
2. **The export lifecycle fact** (this boundary, future): a recordable process fact that an evidence packet artifact was *prepared for / released toward a recipient scope*. This does not exist anywhere in the runtime today.

Any future rung in this family must name which meaning it touches. This document is about meaning (2) only.

## 5. Boundary taxonomy

Ordered lifecycle candidates after **produced**. For each: what the fact would mean; who may record it; the minimum references; what must never be stored; what it does **not** claim; and the member/steward surface language. Two structural rules apply to every row:

- **Witness posture.** The substrate can only witness *reports by an authenticated recorder*, exactly as `produced_by` works today. Facts whose truth lives with an external party (received, accepted, audited) cannot be substrate-verified from the sender side; if they are ever recorded, they are recorded as *the recipient's or reporter's attested statement*, never as a substrate-verified outcome.
- **Proof-pointer chain.** Every downstream fact names its predecessor by caller-opaque id + content-addressed `record_hash` (the lane's landed inter-receipt discipline), so the chain produced → exported → … is cryptographically walkable without storing any body.

### 5.1 `produced` — **landed** (#2318/#2320)

Meaning, recorder, references, exclusions, non-claims, and surface language are pinned by the #2314 contract and #2316 rung; restated in §3. Surface language (already shipped): *"An evidence packet was produced and recorded here."*

### 5.2 `export-prepared` / `exported`

- **Would mean:** a specific produced packet artifact was prepared for release toward a named **recipient scope** under a declared export/redaction policy — the sender-side release fact. (Whether the v1 fact is the *preparation* or the *release act* is decision-rung question **EX1**.)
- **Recorder:** an authenticated app-side actor (`exported_by`) — recorder / export-witness evidence, not an authority to release, and not a claim the release was permitted; charter/gate policy governs permission, exactly as for the landed classes.
- **Minimum references:** the produced receipt's `packet_id` + `record_hash` (predecessor proof link, verified fail-closed in-session); `packet_hash` (the artifact being exported must be the fingerprinted artifact); an export-policy / redaction-policy fingerprint (hash, not body — aligned with #1792's `ExportReceipt.redaction_policy`); a **recipient-scope handle or fingerprint** (see privacy rule in §6 — never contact data).
- **Must not store:** packet body; source-receipt bodies; redaction/export policy bodies; recipient names, emails, addresses, or any personal contact data; transport credentials or endpoints.
- **Does not claim:** availability, transmission, receipt, acceptance, audit, certification, correctness, completeness, legal sufficiency, human/AT verification, readiness.
- **Surface language:** *"An export of this evidence packet was prepared/recorded for a named recipient scope. This does not mean it was delivered, received, or accepted."*

### 5.3 `made available`

- **Would mean:** the export artifact was placed in governed custody where the authorized recipient scope *can retrieve it* (a unilateral availability/custody fact — e.g. placed in a scoped vault per #1792 / the artifact-registry-and-scoped-vault spec).
- **Recorder:** the custodian-side actor; recorder evidence only.
- **Minimum references:** the export fact's id + `record_hash`; a custody/location *fingerprint or vault reference* (never a private endpoint or credential).
- **Must not store:** anything §5.2 excludes; additionally no retrieval tokens, URLs bearing secrets, or vault contents.
- **Does not claim:** that anyone retrieved it, was notified of it, received it, or accepted it.
- **Surface language:** *"The export was made available to the authorized recipient scope. Retrieval, receipt, and acceptance are separate facts, not recorded here."*
- **Boundary note:** *made available* ≠ *delivered*. Availability is unilateral; delivery claims a transmission act (**EX2**).

### 5.4 `delivered` / `transmitted`

- **Would mean:** a sender-side actor reports a transmission act toward the recipient scope was performed. The substrate witnesses the **sender's report of transmission**, never arrival.
- **Recorder:** sender-side actor; recorder / transmission-witness evidence.
- **Minimum references:** the export (or availability) fact's id + `record_hash`; a transport-class label at most (no endpoints, no credentials).
- **Must not store:** recipient contact data; transport payloads; endpoints; credentials; delivery-service message bodies.
- **Does not claim:** arrival, readability, receipt, acceptance, or anything §5.2 excludes. A delivered-report is **not** proof of receipt.
- **Surface language:** *"A transmission of this export was recorded by the sender. This does not prove it arrived or was accepted."*

### 5.5 `received`

- **Would mean:** the **recipient** attests the export artifact (by `packet_hash`) reached them.
- **Recorder:** only an authenticated recipient-scope actor. The sender must never be able to fabricate this fact; if the recipient is outside the substrate, this fact either arrives through a bridge (which "translates; emits import/translation receipts" per the operating model) or is not recorded at all.
- **Minimum references:** the delivery/export fact's id + `record_hash`; `packet_hash` as received.
- **Does not claim:** acceptance, evaluation, audit, or agreement — only arrival-as-attested.
- **Surface language:** *"The recipient recorded that the export reached them. This is their statement of arrival, not acceptance."*
- **Boundary note:** recipient-side facts likely belong to bilateral/federation or institution-package semantics rather than the generic substrate (**EX4**).

### 5.6 `accepted`

- **Would mean:** the recipient attests they accept the packet for their own stated purpose. An evaluative act by the recipient, on the recipient's authority basis, under the recipient's (institutional) rules.
- **Recorder:** recipient-scope actor only; the acceptance *criteria* are institution/domain-package territory, never substrate semantics.
- **Does not claim:** audit, certification, correctness, completeness, legal sufficiency, or that acceptance binds anyone else.
- **Surface language:** *"The recipient recorded acceptance under their own rules. ICN records the statement; it does not certify the evaluation."*

### 5.7 `audited` / `certified`

- **Would mean (if ever recorded):** a report that an external evaluative outcome exists — a *report-of-external-outcome* fact, referencing an external attestation by fingerprint. The substrate can never verify the audit itself.
- **Out of any near-term scope.** Recording it generically risks exactly the overclaim this lane forbids ("audited" reads as "ICN certifies"). If a real consumer ever needs it, it requires its own contract + decision rung, tied to the attestation/dispute model (#1009) — not a silent add.
- **Surface language (if ever):** *"A third-party evaluation outcome was reported and fingerprinted. ICN did not perform, verify, or certify that evaluation."*

### 5.8 `challenged` / `rejected` / `withdrawn`

- **Would mean:** a subsequent fact disputing, declining, or retracting an earlier fact in this family. Receipts are append-only: withdrawal/rejection never deletes or mutates a prior receipt — it records a later fact that references it by `record_hash`.
- **Recorder:** challenge/rejection by the affected scope's actor; withdrawal by the originating scope's actor; all recorder-evidence-only.
- **Ties to:** #1792's `challenge_path` requirement ("every restricted record should expose … challenge path") and the #1009 dispute-pathway lane.
- **Surface language:** *"A challenge/withdrawal was recorded against an earlier fact. The earlier receipt remains part of the record."*

## 6. Privacy and redaction (defers to #1792)

- The **packet body stays outside every receipt** in this family, exactly as in the landed class. Fingerprints only.
- The **redaction profile body and export policy body stay outside** receipts unless a future scoped/vaulted design under #1792 explicitly decides otherwise; v1 posture is hash-only, matching EP3.
- **Source receipt bodies stay outside**; `receipt_set_hash` commits to references only.
- **Recipient identity is itself sensitive data.** Any recipient reference must be a scope handle or fingerprint — never names, emails, phone numbers, or contact data (a #1792 hard rule). #1792's candidate `ExportReceipt.recipient_scope` is the vocabulary to align with; this boundary must not fork it.
- **Proof pointers are hashes/references, not disclosure.** Publishing a receipt in this family discloses that a fact happened, never what the packet contains.
- **Access is a receipted act.** If a recipient retrieves an available export from governed custody, that access belongs to #1792's `AccessReceipt` lane ("access to private data is itself a receipted institutional act") — an access fact is distinct from every row in §5, and this boundary does not absorb it (**EX6**).

## 7. Accessibility and readiness (defers to #2041)

- **No fact in this family can imply #2041 completion.** Exporting, delivering, or even a recipient accepting a packet says nothing about screen-reader, low-vision, switch, or AT-compat verification. Human/AT verification remains separate evidence produced only by an actual human/AT pass.
- Any future surface rendering of these facts inherits the process-evidence surface's obligations: plain-language summary before raw fields, progressive disclosure, hash fields explained in understandable language, recorder DIDs described as "who recorded this fact," and the fixture/dev boundary visible without JSON inspection.
- Organizer-ready / member-ready / pilot-ready language remains forbidden throughout this family.

## 8. Candidate next artifacts (recommendation, not commitment)

Matching the landed cadence (contract → decision rung → implementation → render), the recommended **single next rung** is the **export fact (§5.2)** — it is the only row that is (a) sender-side witnessable with the substrate's existing recorder posture, (b) already named by #1792 (`ExportReceipt`, "no sensitive export without redaction policy and export receipt"), and (c) the immediate successor of the landed produced fact on the spine tail.

1. **Decision rung (next):** a narrow decision doc resolving EX1–EX8 (§9) — exact fact name, v1 field set, recipient-scope representation, and which rows of §5 stay out of v1. No tag is pinned before this rung, per the lane's landed rule that hash-participating structure is decided in writing first.
2. **Runtime receipt slice (only after the rung):** one class only, mirroring the eighth class's slice shape (proof type + golden vector, opaque storage, fail-closed predecessor verification, conflict sentinel, runtime-slice tests).
3. **Member/steward surface extension (only after runtime or a pinned fixture contract):** fixture-only render mirroring #2312/#2320, with the §5.2 surface language and the §7 obligations.

Rows §5.3–§5.8 are **explicitly deferred**: availability/delivery need the vault/custody seam (#1792) to mature; received/accepted are recipient-authority facts (**EX4**); audited/certified is out of near-term scope entirely.

## 9. Open questions (for the decision rung — none decided here)

- **EX1 — fact name and moment.** Is the v1 fact `export-prepared` (an artifact staged under a policy) or `exported` (a release act)? Candidate: whichever is chosen, the name must not imply transmission.
- **EX2 — availability vs delivery.** Is `made available` a distinct fact from `delivered`? Candidate: yes — unilateral custody vs claimed transmission — but v1 likely needs neither.
- **EX3 — recipient-scope representation.** Handle, fingerprint, or DID? What guarantees keep contact data out by construction?
- **EX4 — where acceptance lives.** Do recipient-side facts (received/accepted) belong in generic ICN at all, or in institution/domain-package and bridge semantics? Candidate: the substrate provides the receipt grammar; acceptance criteria are package territory.
- **EX5 — minimal v1 field set.** Which of predecessor link, `packet_hash`, export-policy fingerprint, recipient-scope reference, `exported_by`, `exported_at` are identity-participating, and what is the conflict sentinel?
- **EX6 — vault/access interaction.** Is the export artifact placed in a scoped vault, and is retrieval an #1792 `AccessReceipt` rather than anything in this family?
- **EX7 — challenge/rejection shape.** How does §5.8 reference the challenged fact, and does it need its own class or a shared dispute shape (#1009)?
- **EX8 — smallest repo-safe dogfood.** What is the minimal fixture-only path (mirroring #2291/#2312/#2320) that can tell the export story without performing any real export?

## 10. Non-goals

This document and any artifact it recommends are:

- not runtime code; not a new receipt class; not a tag or hash layout;
- not an HTTP route; not OpenAPI/SDK/served-schema work; not a member-shell change; not a fixture change;
- not packet-body, policy-body, or source-receipt-body storage; not private-data handling; not recipient contact-data storage;
- not a live external delivery, transport integration, notification system, or delivery service;
- not recipient acceptance, audit certification, or any legal-sufficiency claim;
- not an action-card trigger; not a workflow engine; not new authorization semantics;
- not #2041 completion; not #1748 / #2141 / #1792 closure;
- not production / pilot / organizer-ready / member-ready readiness; not live federation; not NYCN activation; not Phase-2 completion.

Receipts record institutional facts. They grant zero authority.

## 11. Related

Refs #2321.
Refs #2141.
Refs #1748.
Refs #1792.
Refs #2041.
Refs #2320.
Refs #2318.
Refs #2319.
