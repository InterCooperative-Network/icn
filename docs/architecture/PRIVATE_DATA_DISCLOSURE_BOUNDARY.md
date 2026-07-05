# Private Data Disclosure Boundary, Scoped Vaults, and Access Receipts

**Status:** draft — design/architecture boundary contract
**Truth class:** descriptive
**Canonical:** no — implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last reviewed:** 2026-07-05
**Source basis:** read against `origin/main` @ `749fffb5` (the merged #2328 tip — re-verify before relying on exact anchors; they drift)
**Related:** #1792 (this doc's issue) · #1767 (encrypted distributed private-overlay storage — the storage/encryption sibling) · #1748 (Institutional Process Substrate milestone) · #2141 (vertical institutional spine control) · #2041 (human/AT accessibility pass — open/parked) · #2326 (`EvidencePacketExportPreparedReceipt` runtime slice — merged) · #2328 (member-shell export-prepared fixture render — merged) · [`evidence-export-delivery-boundary.md`](../design/evidence-export-delivery-boundary.md) (#2321 export/delivery boundary — defers here) · [`evidence-export-delivery-boundary-decision-rung.md`](../design/evidence-export-delivery-boundary-decision-rung.md) (#2324 EX1–EX8) · [`artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) (#1798 registry/vault outline — defers here) · [`private-overlay-did-activation-flow.md`](../spec/private-overlay-did-activation-flow.md) (#1730 private-overlay activation) · [KERNEL_APP_SEPARATION.md](KERNEL_APP_SEPARATION.md) (Meaning Firewall / opaque receipt storage) · [DESIGN_PRINCIPLES.md](../DESIGN_PRINCIPLES.md) (§1.5 Meaning Firewall) · [INSTITUTION_PACKAGE_BOUNDARY.md](INSTITUTION_PACKAGE_BOUNDARY.md) (package vs core) · [ADR-0020](../adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md) (institutional bootstrap activation & standing read-model) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance envelope)

> This is a **design/architecture boundary contract, not a runtime spec and not a receipt class.** It adds no code, no receipt class, no route, no OpenAPI/SDK change, no gateway/auth change, no member-shell change, and no fixture change. Its job is to pin one coherent private-data disclosure/access architecture now that the receipt ladder can *prepare* an export (`EvidencePacketExportPreparedReceipt`, #2326/#2328) but cannot yet honestly express *made available*, *accessed*, *disclosed*, or *redacted-then-shared*. It names the vocabulary, the candidate model, the hard rules, and the surface affordances — and defers **encryption and distributed storage** to #1767 and **any implementation** to a later decision rung. Receipts record institutional facts. They grant zero authority. No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim is made or implied by this document.

---

## 1. Purpose
<!-- truth: descriptive -->

Define ICN's generic private-data and disclosure boundary so that private overlays, scoped vaults, opaque receipt storage, redaction, selective disclosure, private object references, disclosure policies, access receipts, evidence export, and member/operator UI privacy affordances all follow **one** coherent architecture instead of scattering per feature.

This is the architecture home issue #1792 asks for. Three landed/normative documents already **defer to** this boundary rather than defining it:

- [`evidence-export-delivery-boundary.md`](../design/evidence-export-delivery-boundary.md) names candidate `ExportReceipt`/`AccessReceipt` shapes and states "this boundary document defers to #1792's vocabulary wherever the two overlap."
- [`artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) cross-links "the forward-direction `PrivacyClass` / `DisclosurePolicy` / `PrivateObjectRef` / `AccessReceipt` / `ExportReceipt` / `RedactionMap` vocabulary proposed under #1792."
- `ICN_INTEGRATED_SYSTEM_MODEL.md` describes a scoped vault as "private state with restricted disclosure. Tracked under #1792 and #1767."

Until this document lands, that vocabulary is undefined and each consumer improvises. This document is the definition.

## 2. Status basis
<!-- truth: descriptive -->

- The process/evidence receipt ladder is **nine classes deep**, runtime-landed and rendered read-only in the fixture-only member-shell process-evidence demo: `ProcessSessionOpened` → `DeliberationEntryRecorded` → `DecisionRecorded` → `ProcessGateResult` → `ActivationCrossed` → `MutationPlanRecorded` → `MutationApplied` → `EvidencePacketProduced` → `EvidencePacketExportPrepared`.
- The newest rung, **`EvidencePacketExportPreparedReceipt`** (`icn:gov:evidence_packet_export_prepared:v1`, #2326), records only that a produced packet was *prepared* for export to a recipient scope under an export policy. Per its own contract: **nothing was made available, delivered, received, accepted, audited, or certified; no private data moved; no access was granted; no authority was granted.**
- That is exactly the seam this document opens. "Prepared for a recipient scope" presumes a *scope*, a *policy*, and eventually an *access* — none of which have a defined model yet. The candidate `AccessReceipt`, `ExportReceipt`, `DisclosurePolicy`, `PrivateObjectRef`, and `RedactionMap` shapes are named in sibling docs but **defined nowhere**.
- The kernel already persists typed receipts as **opaque bytes** (see [KERNEL_APP_SEPARATION.md §"Opaque Storage for Receipts"](KERNEL_APP_SEPARATION.md)); that primitive is the enforcement surface this boundary relies on. No new storage primitive is proposed here.

This document is design-only. It changes no runtime and pins no hash-participating structure; a later decision rung (see §15) does that before any tag or implementation.

## 3. Core principle
<!-- truth: normative -->

> **Data can be private. Power cannot be invisible.**

ICN must support private *contents* with public or scoped *provenance* where safe. The two halves are independent:

- **Data can be private.** The *contents* of a restricted record (real names, contact data, accommodation needs, care notes, conflict details, sponsor pipelines, external settlement credentials) may be withheld, scoped, encrypted, or held by an external custodian.
- **Power cannot be invisible.** The *existence*, responsible body, authority basis, and transition history of an institutional act must remain provable where safe, so that power exercised through private records still leaves a receipt. Hiding *every* trace of a restricted record is permitted only under a narrowly justified safety policy, never as a default.

This principle is enforced structurally by the **Meaning Firewall** (canonical: [DESIGN_PRINCIPLES.md §1.5](../DESIGN_PRINCIPLES.md); [KERNEL_APP_SEPARATION.md](KERNEL_APP_SEPARATION.md)). The kernel stores and indexes **opaque bytes** and does not parse domain meaning. A receipt can therefore prove that an access, disclosure, or export-preparation *happened* — with a content fingerprint and proof-pointer chain — without the kernel (or a public reader) learning the private contents. Confidentiality of the contents (encryption) is a separate concern, owned by #1767; this document owns *who may read what, under what policy, and what receipt witnesses it*.

## 4. Vocabulary — define and distinguish
<!-- truth: normative -->

These terms are **defined here for the first time**. Sibling docs reference them; none pin them. Where a term names a *candidate* runtime shape, §5 gives the field outline; this section fixes the *meaning*.

- **private overlay** — the non-public mapping/context that binds public placeholder identifiers (holder labels, placeholder IDs) in an institution package to real DIDs, contact data, and real-person records. Lives **outside** public git. Storage/encryption of overlays is #1767; activation is [`private-overlay-did-activation-flow.md`](../spec/private-overlay-did-activation-flow.md) / ADR-0020. Distinct from a scoped vault: an overlay is a *binding context*, a vault is a *store*.
- **scoped vault** — a store for private runtime objects whose read access is bounded by scope and governed by a disclosure policy. Runtime private data belongs in scoped vaults, **not** in public packages and **not** in generic unscoped stores. A vault has an id, a scope, a disclosure policy, and (per #1767) an encryption/custody backend this document does not define.
- **opaque receipt storage** — the kernel primitive that persists a typed receipt as `(class, record_hash) → bytes` with a secondary audit-chain index, **without parsing the typed body** (KERNEL_APP_SEPARATION.md). Opaque ≠ encrypted: opaque means *the kernel does not interpret meaning*; the bytes may be plaintext-to-an-app or ciphertext. This is what lets a receipt's *existence* be provable while its *meaning* stays app-side.
- **encrypted storage** — confidentiality of the stored bytes themselves (ciphertext at rest / in transit, key custody, rotation, recovery). Orthogonal to opaque storage. **Owned by #1767**, referenced here, not defined here.
- **redaction** — deriving a public/redacted artifact from a private/source artifact by removing or masking fields under a profile, and committing to that transformation by fingerprint. The landed `EvidencePacketProducedReceipt` already carries a `redaction_profile_hash`; a full redaction *model* (a `RedactionMap`, §5) is candidate.
- **selective disclosure** — revealing a bounded subset of a private object's fields (or a proof about them) to a specific scope under policy, while withholding the rest. A disclosure policy's `redaction_rules` + `allowed_scopes` express which subset which scope may see.
- **private object reference (`PrivateObjectRef`)** — the public/scoped handle used *in place of* private contents: names the object, its content fingerprint, its vault, its privacy class, its policy fingerprint, and the receipt that governs it — **never the contents**. This is how a public/scoped surface can point at a private object without leaking it.
- **disclosure policy (`DisclosurePolicy`)** — the governing rules for a private object or class: who may see it, at what scope, with what redaction, how access is requested, how it is challenged, how long it is retained, and how it may be exported. The policy *body* is never stored in a public surface; a `policy_hash` fingerprints it.
- **access receipt (`AccessReceipt`)** — a receipt that records that an actor accessed (or attempted to access) a private object under a disclosure policy and an authority basis. Access to private data is **itself a receipted institutional act** — no silent reads.
- **export receipt (`ExportReceipt`)** — a receipt that records that a (typically redacted) artifact was exported to a recipient scope under an export policy. The landed `EvidencePacketExportPreparedReceipt` is the *sender-side preparation* form of this; a general `ExportReceipt` is candidate and must not be conflated with delivery.
- **made-available receipt** — a candidate receipt recording that a prepared export was placed in governed custody where an authorized recipient scope can retrieve it (a unilateral availability/custody fact). Made-available is **not** delivered, received, or accepted (see [`evidence-export-delivery-boundary.md`](../design/evidence-export-delivery-boundary.md) §5).
- **external custodian** — a party (outside ICN core) that holds private contents under agreement (e.g. a partner-held overlay store during migration). A privacy class marks such objects so surfaces never imply ICN core holds the plaintext.
- **sealed / restricted record** — a record whose contents are withheld now (`SealedUntil` a condition/time, or `ScopeRestricted` to a body). Its *existence and responsible body* may still be public where safe.
- **public existence metadata vs restricted contents** — the load-bearing split. **Public existence metadata** = the safe-to-expose facts about a record (that it exists, its responsible body, its access path, its challenge path, custody/provenance fingerprints, its receipt hash). **Restricted contents** = the private body the metadata deliberately does not contain. A surface may show the former without ever showing the latter.

## 5. Candidate model (proposed, not implemented)
<!-- truth: descriptive -->

The following shapes are **candidates** carried forward from the #1792 issue body and sibling references. They are named here so consumers stop improvising; **field layout, canonicalization, and any hash-participation are NOT pinned here** — a decision rung does that (§15). No runtime type is created by this document.

- **`PrivacyClass`** (taxonomy of how private an object is): `Public`, `MembersOnly`, `ScopeRestricted`, `PrivateOverlay`, `SecretCredential`, `ExternalCustodian`, `SealedUntil`.
  - ⚠️ **Naming-collision note:** distinct `PrivacyClass`-named enums already exist in the codebase for unrelated purposes. This candidate taxonomy must be disambiguated (renamed, or namespaced) at the decision rung before any implementation; do not assume the existing enum is this taxonomy.
- **`DisclosurePolicy`**: `visibility`, `allowed_scopes`, `redaction_rules`, `access_request_path`, `challenge_path`, `retention_policy`, `export_policy`.
- **`PrivateObjectRef`**: `object_id`, `content_hash`, `vault_id`, `privacy_class`, `policy_hash`, `receipt_ref`.
- **`RedactionMap`**: `original_hash`, `redacted_hash`, `fields_redacted`, `reason`, `policy_ref`.
- **`AccessReceipt`**: `object_ref`, `actor`, `authority_basis`, `purpose`, `timestamp`, `outcome`.
- **`ExportReceipt`**: `object_ref`, `exported_by`, `recipient_scope`, `redaction_policy`, `reason`, `receipt_hash`.

Every one of these carries **references and fingerprints, not bodies**: `content_hash`/`policy_hash`/`original_hash`/`redacted_hash` are fingerprints; `object_ref`/`vault_id`/`recipient_scope` are opaque handles. Contact data, real DIDs, and private bodies never appear in these structures.

## 6. Public existence metadata vs restricted contents
<!-- truth: normative -->

Every restricted record is modeled as a **public/scoped `PrivateObjectRef` over a private body**. The reference is the surface-visible half; the body lives in a scoped vault. Where safe, the reference exposes:

- **existence** — that the record exists;
- **responsible body** — which governed body holds/answers for it;
- **access path** — how an authorized actor requests/obtains access;
- **challenge path** — how misuse or overbroad access is challenged;
- **custody/provenance metadata** — where the contents are held (vault id, external-custodian marker) and their content fingerprint;
- **receipt hash** — the receipt that proves the record's restriction and any access to it.

It deliberately does **not** expose the private body. The safety-policy exception in §3 ("Power cannot be invisible … unless narrowly justified") is the only case where even existence is withheld, and it must be justified per record class, not applied by default.

## 7. Staged receipt vocabulary (candidate lifecycle)
<!-- truth: descriptive -->

The disclosure/access lifecycle beyond the landed export-prepared fact is a **staged, candidate** sequence. Each stage is a *possible future* receipt; **names may change at the decision rung and none is implemented here.** Each stage witnesses a narrower fact than a naïve reader might assume.

| Stage | Candidate receipt | Records | Explicitly does NOT mean |
|---|---|---|---|
| 0 (landed) | `EvidencePacketExportPreparedReceipt` (#2326) | a produced packet was **prepared** for export to a recipient scope under an export policy — sender-side only | made available · delivered · received · accepted · audited · certified · accessed · authority granted |
| 1 | `EvidencePacketMadeAvailableReceipt` *(candidate)* | a prepared export was **made available** to a recipient scope under policy (unilateral custody/availability) | retrieved · received · accepted · audited · certified |
| 2 | `AccessReceipt` *(candidate)* | an actor **accessed or attempted to access** a private object under a disclosure policy and authority basis (records `outcome`) | that access was authorized *by this receipt* (authority basis is decided by the authority model, not the receipt) · that contents were disclosed in full |
| 3 | `DisclosureDecisionReceipt` *(candidate)* | a disclosure request was **approved / denied / limited** | that any access then occurred |
| 4 | `RedactionAppliedReceipt` *(candidate)* | a **redaction profile was applied** to a private/source artifact, producing a public/redacted artifact fingerprint | that the redaction is complete, correct, or legally sufficient |

Two invariants across the ladder: (a) each receipt records a **fact**, not a permission — "receipts record facts; they do not grant permission"; the authority *basis* referenced by an `AccessReceipt` is decided by the authority-hardening model (#1868 per-action capabilities, #2061 entity-aware authorization), not minted by the receipt. (b) each stage's non-claims are **load-bearing**: prepared ≠ available ≠ accessed ≠ disclosed ≠ redacted-sufficiently, exactly as produced ≠ exported per [`evidence-export-delivery-boundary.md`](../design/evidence-export-delivery-boundary.md).

## 8. Opaque receipt storage and the Meaning Firewall
<!-- truth: normative -->

`AccessReceipt`, `ExportReceipt`, and the candidate disclosure/redaction receipts are **app-emitted, kernel-blind facts**. Per [KERNEL_APP_SEPARATION.md §"Opaque Storage for Receipts"](KERNEL_APP_SEPARATION.md), when a vault/governance app generates a typed receipt, the kernel persists it as opaque bytes in a `(class, record_hash) → bytes` store with a secondary `(class, key1, key2_opt, recorded_at, record_hash)` audit-chain index, and **does not parse or pattern-match the typed body**.

Consequences for this boundary:

- The kernel can prove a receipt **exists** and order it in an audit chain without learning what private object it concerns.
- The private *contents* never enter the kernel as parsed meaning; only fingerprints and opaque handles do. This is how "power cannot be invisible" and "data can be private" hold simultaneously.
- Opaque storage gives *kernel-blindness*, not *confidentiality*. If the receipt bytes themselves must be ciphertext (e.g. because even the fingerprint set is sensitive), that is #1767's encryption concern layered underneath, not a change to the opaque primitive.
- No new kernel storage primitive is required or proposed. This boundary rides the existing opaque store.

## 9. Relationship to #1767 and sibling documents
<!-- truth: descriptive -->

- **#1767 (encrypted distributed private-overlay storage) vs #1792 (this doc).** #1767 owns *how the private bytes are encrypted and where keys/replicas live* — client/steward-side encryption, key model, replication across nodes/commons, consent/revocation/rotation/recovery, and the threat model against node operators, ICN core maintainers, and public repo readers. #1792 owns *the disclosure/access policy layer over any private object* — privacy classes, disclosure policies, access/export/disclosure/redaction receipts, the public-existence-vs-contents split, and UI affordances. **This document references #1767 for encryption and does not redefine it;** it names the vault's encryption/custody slot and defers.
- **`artifact-registry-and-scoped-vault.md` (#1798)** outlines a `ScopedVault` object and states every read MUST emit an `AccessReceipt` per #1792. That spec is a *consumer* of this vocabulary; it should reference this document once landed.
- **`evidence-export-delivery-boundary.md` (#2321) / decision rung (#2324)** define the export/made-available/delivery taxonomy and already defer to #1792. The export-prepared fact (#2326/#2328) is the stage-0 anchor of §7.
- **INSTITUTION_PACKAGE_BOUNDARY.md** — public institution packages use placeholder IDs; private overlays bind the real values outside public git. This document's hard rules (§10) enforce that split at the disclosure layer.
- **#1748 / #2141** — the "real visibility/privacy-boundary run with redaction" owed under #1748, and the "storage/artifact/vault custody enforcement" node of the #2141 spine, both depend on this boundary being defined.

## 10. Hard rules
<!-- truth: normative -->

1. **No private data in public package repos.** No real names, emails, phone numbers, addresses, accommodation needs, demographic data, sponsor/private pipelines, external settlement credentials, or private-overlay contents may appear in public package or public git material. Public packages use placeholder IDs; private overlays bind the real values outside public git.
2. **No private data access without an authority basis.** Every access to a private object is gated by an authority basis decided by the authority model (#1868/#2061) and witnessed by an `AccessReceipt`. No silent reads.
3. **No sensitive export without redaction/disclosure policy and receipted access/made-available semantics.** An export of sensitive material requires a disclosure/export policy and a redaction where required, and produces receipts; export-prepared is never treated as delivery.
4. **No dashboard or member-shell preview of private vault contents.** Operator and member surfaces show *privacy posture and public existence metadata*, never restricted contents.
5. **No kernel parsing of typed receipt bodies.** The kernel stores opaque bytes; all typed-body meaning stays app-side (Meaning Firewall).
6. **No public disappearance of institutional power.** Hiding every trace of a restricted record is allowed only under a narrowly justified safety policy, never by default; where safe, existence and the responsible body remain provable.
7. **Every restricted record exposes, where safe:** existence, responsible body, access path, challenge path, custody/provenance metadata, and receipt hash.
8. **References and fingerprints, never bodies.** Every disclosure/access/export structure and every surface carries `PrivateObjectRef`-style handles and fingerprints, never contents, real DIDs, or contact data.

## 11. Member-shell implications
<!-- truth: descriptive -->

A member should be able to see, in plain language and without any private body being exposed:

- **that this restricted record exists** (where safe);
- **whether it contains private details** (privacy class, not contents);
- **who can access it** (responsible body / allowed scopes, as handles);
- **why they can access it** (authority basis category, not credentials);
- **what receipt proves the access or restriction** (receipt hash / proof pointer);
- **how to request review, revoke sharing where allowed, or challenge misuse** (access/challenge paths from the disclosure policy);
- **what has not happened yet** — the same load-bearing non-claims as the export-prepared surface: prepared ≠ made available ≠ accessed ≠ delivered ≠ received ≠ accepted ≠ audited ≠ certified.

No member-shell surface may preview private vault contents. The existing fixture-only process-evidence demo (#2328) is the pattern: plain-language summary first, fingerprints/handles under a disclosure, honesty banner, and explicit negative boundaries. Any new privacy/access states rendered here add human/AT surface owed under #2041 (see §12/§15).

## 12. Operator / steward implications
<!-- truth: descriptive -->

A steward/operator dashboard shows **privacy posture, not private content**:

- vault health (reachable, consistent) — no contents;
- private overlay **loaded / missing**; public package **clean** (no private leakage detected);
- access grants **expiring**;
- **overbroad disclosure-policy warnings** (e.g. a policy whose `allowed_scopes` is wider than the record's sensitivity warrants);
- **export-prepared receipts awaiting** made-available / access follow-up;
- redaction / evidence-export status;
- **opaque receipt store health** (audit chain intact) — no typed-body previews.

The operator surface must never render restricted contents, real DIDs, or contact data — only posture, counts, fingerprints, and handles.

## 13. Failure and safety table
<!-- truth: operational -->

| Situation | Correct behavior | Never |
|---|---|---|
| Private overlay missing at runtime | surface "private overlay missing" posture; deny access; emit no plaintext | fabricate or infer the mapping |
| Access requested without authority basis | deny; witness the attempt as an `AccessReceipt` with `outcome = denied` | allow a silent read |
| Sensitive artifact exported without redaction where required | block; require a disclosure/redaction policy first | ship unredacted sensitive contents |
| Operator opens the dashboard | show posture, counts, fingerprints, handles | preview vault contents |
| Public reader inspects a public package | see placeholder IDs and public existence metadata | learn real DIDs / contact data |
| Kernel indexes a new receipt | store opaque bytes; order the audit chain | parse the typed body |
| A restricted record's existence is itself dangerous to reveal | withhold under a narrowly justified, per-class safety policy | hide all institutional power by default |
| Export prepared but not yet available | surface "prepared — not made available/delivered/accepted" | imply delivery, receipt, or acceptance |

## 14. Non-goals and non-claims
<!-- truth: normative -->

- **No runtime implementation.** No vault runtime, no encryption implementation, no access/made-available/disclosure/redaction receipt runtime, no `PrivacyClass`/`DisclosurePolicy`/`PrivateObjectRef` types created.
- **No route, OpenAPI, SDK, or gateway/auth change.** Design only.
- **No encryption or distributed-storage design.** That is #1767; this document references it.
- **No Drive migration, no NYCN-specific private-data schema, no directory rollout.**
- **No change to #1767, #2326, or the landed receipt classes** beyond cross-reference.
- **No production, pilot, organizer-ready, member-ready, live-federation, NYCN-activation, or Phase-2 claim.** Nothing here asserts anything was made available, delivered, received, accepted, audited, certified, or is legally sufficient, nor that private data is handled, that access is enforced, or that a vault/encryption runtime exists.
- **Leaves #1748, #2141, #1792, #2041, #1868, #2061, #2080, and #2081 open.** This document is an input to #1792 and to #1748's owed visibility/privacy criterion; it settles none of them.

## 15. Follow-up issue sequence (proposed, not opened)
<!-- truth: descriptive -->

This document **proposes** the following child sequence. Per the export-lane cadence, each rung is a separate, narrowly-scoped artifact; **none is opened by this document.** Opening them is a later, explicitly-authorized step.

1. **docs(process): resolve access / made-available / disclosure receipt decision rung** — pin, for each candidate receipt, the fact name/moment, the field layout, hash-participation, uniqueness anchor, proof-pointer chain, private-data exclusions, non-claims, and surface language. Mirrors the EX1–EX8 rung. Blocks all runtime below.
2. **feat(process): emit `EvidencePacketMadeAvailableReceipt` runtime slice** — the stage-1 fact, after (1).
3. **feat(process): emit `AccessReceipt` runtime slice** — the stage-2 fact, after (1) and the authority-basis seam (#1868/#2061) is at least specified.
4. **feat(member-shell): render made-available / access privacy states (fixture-only)** — mirrors #2328.
5. **test(a11y): human/AT pass over the privacy/access member-shell states** — extends #2041 §4G; automated floor ≠ human pass.
6. **spec(authz): align access receipts with the #1868/#2061 authority model** — the authority *basis* an `AccessReceipt` references.
7. **feat(operator): privacy-posture view (no content preview)** — the §12 dashboard, into the operator/appliance path.
8. **docs(process): challenge / repair path for access misuse** — append-only supersession/withdrawal after access/disclosure exists.

**Recommendation on whether to open these now:** do **not** open them from this document. The repo convention for this lane is one design/boundary doc → one decision rung → per-rung runtime, each separately authorized; child issues are opened at the point of pickup, not at design time. If the maintainer wants the sequence tracked as issues, that is a deliberate follow-up, not part of this PR.

## 16. Related
<!-- truth: descriptive -->

- Issues: #1792 (this) · #1767 · #1748 · #2141 · #2041 · #1868 · #2061 · #2080 · #2081 · #2326 · #2328 · #1798 (artifact registry / scoped vault) · #1730 (private-overlay activation).
- Docs: [KERNEL_APP_SEPARATION.md](KERNEL_APP_SEPARATION.md) · [DESIGN_PRINCIPLES.md](../DESIGN_PRINCIPLES.md) · [INSTITUTION_PACKAGE_BOUNDARY.md](INSTITUTION_PACKAGE_BOUNDARY.md) · [`evidence-export-delivery-boundary.md`](../design/evidence-export-delivery-boundary.md) · [`evidence-export-delivery-boundary-decision-rung.md`](../design/evidence-export-delivery-boundary-decision-rung.md) · [`artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md) · [`private-overlay-did-activation-flow.md`](../spec/private-overlay-did-activation-flow.md).
- ADRs: [ADR-0020](../adr/ADR-0020-institutional-bootstrap-activation-and-standing-read-model.md) (institutional bootstrap activation & standing read-model) · [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) (receipt & provenance envelope).
