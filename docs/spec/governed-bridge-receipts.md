---
Status: draft spec / requirements vocabulary
Canonical: no
Authority: architecture / Tool Commons planning (downstream demand signal; not yet normative)
Last Reviewed: 2026-07-08
---

# Governed Bridge Receipt Vocabulary

> **Status: draft spec, requirements vocabulary.** Names the receipt vocabulary a
> future governed bridge import would need, so that later `ToolManifest` /
> `GovernedServiceBinding` work does not invent receipt names inconsistently. It
> is derived from the NYCN airlock requirements note
> ([`../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md`](../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md))
> and ICN issue #2365. It defines vocabulary and evidence boundaries only — it
> implements no receipt, adds no route, and does not imply any bridge can import
> real rows today. Only `ArtifactReceipt` (below) exists in code. The PR
> introducing this doc advances #2365 without closing it.

## 1. Purpose

A governed bridge would move data from an external source into ICN custody only
under steward authorization, leaving evidence at each bounded step. The NYCN
airlock rehearsals coined a family of receipt names for those steps; this
document names them once, precisely, so downstream substrate work
(`ToolManifest` #2366, `GovernedServiceBinding` #2367, external-custodian model
#2368, steward review surface #2369) binds to a consistent vocabulary rather
than re-coining it.

It answers:

> What receipts must exist before a governed bridge can safely move real rows
> from an external source into ICN custody?

This is **docs/spec planning only**. It does not implement the receipts, define a
wire schema, or imply any current bridge can import real rows. Every class named
here is **expected / future** except `ArtifactReceipt`, which already exists as a
transfer proof and is explicitly *not* a bridge receipt.

## 2. Receipt boundary doctrine

> A receipt proves that a bounded event was recorded. It does not prove the event
> was wise, complete, authorized by itself, or sufficient for custody.

From that, three doctrine lines the rehearsals kept returning to:

- **The bridge proposes custody; the steward authorizes custody.** A tool
  emitting a receipt does not make the tool the authority.
- **A dry-run is evidence, not authority.** `BridgeDryRunReceipt` proves a
  preview ran; it authorizes nothing.
- **A receipt proves a decision happened, not that the decision was wise.**
  Correctness lives in the review and the policy, not in the existence of a
  receipt.

Three hard boundaries this vocabulary must preserve:

1. **`ArtifactReceipt` proves verified blob transfer only** — an action *on* an
   artifact. It is not proof of `ArtifactRegistry` registration, not a
   `BridgeImportReceipt`, and not a `VaultObjectWriteReceipt`.
2. **Registry recording still needs its own evidence.** An
   `ArtifactRegistrationReceipt` (or equivalent registry-write evidence) proves
   that an artifact was *recorded in the registry*; the transfer receipt does not
   stand in for it.
3. **Action cards are derived read views.** Per
   [ADR-0027](../adr/ADR-0027-action-card-contract.md), `GET /v1/gov/me/action-cards`
   has no mutation API — "cards reference underlying objects; mutation flows
   through those." So this vocabulary names the **underlying governed-object**
   event, never a card write.

## 3. Existing implemented receipt

`ArtifactReceipt` is the **only** receipt class in this document that exists in
code today. It is defined in `icn-kernel-api` (`icn/crates/icn-kernel-api/src/proofs.rs`),
is the ADR-0026 Layer 2 receipt (see
[`../adr/ADR-0026-receipt-and-provenance-proof-envelope.md`](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)),
and carries the fields `blob_hash`, `provider_did`, `requester_did`,
`request_id`, `scope_id`, `verified_at`, `receipt_hash`, and `signature`.

> `ArtifactReceipt` proves a blob/artifact transfer completed and content was
> verified. It is an action on an artifact.
> It is **not** proof of `ArtifactRegistry` registration.
> It is **not** `BridgeImportReceipt`.
> It is **not** `VaultObjectWriteReceipt`.

Per [`artifact-registry-and-scoped-vault.md`](artifact-registry-and-scoped-vault.md)
the registry holds `receipt_refs` that *may point at* `ArtifactReceipt`
instances, alongside other classes — the transfer proof and the registration
record are deliberately separable.

## 4. Proposed / future bridge receipt family

All rows below are **expected / future** vocabulary. None exists in code. The
"Related primitive" column points at the substrate work that would own each.

| Receipt | Bounded event it proves | Required fields / evidence | Must not imply | Status | Related primitive |
| --- | --- | --- | --- | --- | --- |
| `BridgeDryRunReceipt` | A dry-run/preview pass ran over a source shape | bridge tool id · manifest id · binding id · source system id · candidate-action count · timestamp | any write occurred · steward authorization · that real rows were read | Expected / future | `ToolManifest` dry-run mode (#2366) |
| `BridgeReviewDecisionReceipt` | A **human** steward review decision was recorded | verifiable reviewer authority reference (`actor_did` / signature / authority basis, per existing receipt patterns — role-labeled for display, never a bare role string as the only evidence) · decision id · decision (approve/reject/hold/block) · reason ref · reviewed decision set bound to opaque source-record refs + field paths (or a dry-run plan hash that commits to that exact set) — never a bare field-name set · timestamp | the decision was correct or wise · that a write followed · that a role label alone proves authority · that a bare shared field name covers unrelated records | Expected / future | Steward review surface (#2369) |
| `BridgeImportReceipt` | The **import decision** itself (coordinating record) — see §5 | the §5 minimum-evidence set | raw source payload was stored · that the decision was wise | Expected / future | `GovernedServiceBinding` (#2367) |
| `VaultObjectWriteReceipt` | An object was written into a `ScopedVault` | vault id · scope/class · private object ref · content hash · privacy class · timestamp | public visibility · disclosure of vault content | Expected / future | `ScopedVault` |
| `ArtifactRegistrationReceipt` | An artifact was **recorded in `ArtifactRegistry`** | artifact id · content hash · registry namespace · artifact class · timestamp | a blob transfer (that is `ArtifactReceipt`) · publication | Expected / future | `ArtifactRegistry` |
| `ExternalReferenceObservationReceipt` | An external reference/status was **observed** | external system id · external ref id · observed status · source hash · timestamp | settlement was processed · a payment path · that ICN holds authority over the fact | Expected / future | External-custodian model (#2368) |
| `DiscardDecisionReceipt` | A discard decision was recorded | opaque/hash-bound source-record ref + field path (never a raw PII-bearing key; the pair preserves per-`(record, field)` coverage) · discard basis (policy or review) · timestamp | the discarded content was stored · that a human reviewed (unless basis = review) | Expected / future | — |
| `ConsentPolicyBlockReceipt` | An **automatic** no-consent block was enforced | opaque/hash-bound source-record ref + field path (never a raw PII-bearing key; a bare field name loses per-record coverage) · policy id/basis · timestamp | a human reviewed · that the field was imported | Expected / future | — |
| `PublicationConsentBlockReceipt` | An **automatic** no-publication-permission block was enforced | candidate artifact ref · missing-permission basis · timestamp | a human reviewed · that publication occurred | Expected / future | — |
| `GovernedObjectCreationReceipt` | A **governed object** was created under a reviewed bridge decision — the **generic** governed-object creation receipt, for any institution-declared object class | object id · `object_class` (institution-declared, **opaque to core**) · binding id · dry-run id · review id · review decision ref · bridge-import receipt ref · authorization basis ref · created-at timestamp | that ICN core interprets `object_class` · that an action card was written · any specific institutional meaning for the object | Expected / future | governed-object model |
| `FollowUpObjectCreationReceipt` *(provisional name)* | The **follow-up class instance** of governed-object creation: the underlying governed object behind a consent-gated follow-up was created (a card is *derived* from it) | **all `GovernedObjectCreationReceipt` bridge-decision provenance** (object id · `object_class` · binding/dry-run/review ids · review decision ref · bridge-import receipt ref · authorization basis ref · created-at) **plus** the follow-up-specific consent basis ref | that an action card was written · that `GET /me/action-cards` mutated · that it covers non-follow-up governed objects (use the generic receipt) | Expected / future | ADR-0027 (cards derived) |

**Do not use `ActionCardCreationReceipt` as a final name.** It is retained only as
a **deprecated placeholder**: action cards are derived read views (ADR-0027), so
no receipt should assert a card write. The provisional
`FollowUpObjectCreationReceipt` (or `GovernedFollowUpCreationReceipt`) names the
real event for **follow-up objects specifically** — the creation of the underlying
governed object. **The exact follow-up name is provisional and must reconcile with
the eventual governed-object model.** The generic event is named once by
`GovernedObjectCreationReceipt`: a governed-object bridge write emits
`BridgeImportReceipt` plus a governed-object creation receipt — the generic
receipt, or a class-specific instance such as the follow-up receipt while the
model reconciles — and the binding's per-field `required_receipts` pins which one
each field expects. `object_class` values are institution-declared opaque strings:
ICN core stores and carries them but never interprets them (Meaning Firewall).

## 5. `BridgeImportReceipt` minimum evidence

`BridgeImportReceipt` is the coordinating record of a single import decision. Its
minimum evidence set:

- source system id
- source record **reference** — an opaque/hash-bound id or a vault-backed private
  ref, **never** a raw natural or PII-bearing external key (a raw key can itself
  be private data, e.g. an email used as a source row key)
- source hash / content hash
- mapping version
- bridge tool id / manifest id
- governed service binding id
- steward decision id (the `BridgeReviewDecisionReceipt` it rests on)
- dry-run reference — the `BridgeDryRunReceipt` id / preview-plan hash the steward
  reviewed (the import must cite the exact dry-run proposal it confirms;
  refuse-by-default means no write without a preceding reviewed preview)
- target custody class
- target scope / namespace
- privacy class
- import timestamp
- receipt refs for the target writes it coordinated (vault / registry / external
  observation / follow-up object)
- export/delete path reference

This is a **decision-and-provenance record, not raw source payload storage.** It
captures *what was decided and where it landed*, addressed by hashes and opaque
references — never the source rows themselves, and never a raw PII-bearing source
key. The reference is itself opaque/hash-bound or vault-backed, so nothing
private lands in the receipt body or in any steward/member surface that renders
receipt refs. The private content lives (if anywhere) in a `ScopedVault`,
referenced, never inlined.

## 6. Review and policy-block receipts

These four are frequently conflated; the vocabulary must keep them distinct so
that **no-review policy blocks cannot masquerade as human review**:

- `BridgeReviewDecisionReceipt` — a **human** reviewed and decided; it must bind a
  **verifiable** reviewer authority reference (a signed `actor_did` / authority
  basis), role-labeled only for display — a bare role string does not prove an
  authorized steward acted.
- `ConsentPolicyBlockReceipt` — an **automatic** no-consent block; no human
  review implied.
- `PublicationConsentBlockReceipt` — an **automatic** publication-permission
  block; no human review implied.
- `DiscardDecisionReceipt` — a discard decision; its `basis` field records
  whether it was policy-directed or review-directed (the source of the decision
  matters and must be carried, not assumed).

A `ConsentPolicyBlockReceipt` or `PublicationConsentBlockReceipt` must never be
substituted for a `BridgeReviewDecisionReceipt`; an automatic block is evidence
that a policy fired, not that a steward looked.

Wherever a discard/policy-block receipt references source data, it uses the same
rule as `BridgeImportReceipt` (§5): an **opaque/hash-bound source-record
reference plus a field path**, never a raw natural/PII-bearing key. The record
reference + field path together preserve the per-`(record, field)` coverage that
`../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md` requires — a bare field
name (e.g. `email`) shared across many records would both leak a raw identifier
and collapse per-record coverage.

## 7. Target write receipts

The import decision (`BridgeImportReceipt`) **coordinates**; the target-specific
receipts prove the **effects**. Four distinct target effects:

- **Vault write** → `VaultObjectWriteReceipt` (an object landed in a
  `ScopedVault`).
- **Registry registration** → `ArtifactRegistrationReceipt` (an artifact was
  recorded in `ArtifactRegistry`) — distinct from the `ArtifactReceipt` transfer
  proof.
- **External reference observation** → `ExternalReferenceObservationReceipt` (an
  external status/reference was observed, never processed).
- **Governed object creation** → `GovernedObjectCreationReceipt` (generic; the
  object's class is institution-declared opaque data) or a class instance such as
  `FollowUpObjectCreationReceipt` *(provisional)* for consent-gated follow-ups (a
  governed object was created; where it backs a member surface, a card is derived
  from it, never written directly).

`BridgeImportReceipt` carries `receipt_refs` to whichever of these its decision
produced. A completed import decision therefore points at its coordinated target
receipts; a missing target receipt means the effect it claims is unproven.

Note the existing substrate already separates these concerns: `ReceiptStore`
(`icn/crates/icn-gateway/src/receipt_store.rs`) is the persistent receipt
backend — its opaque records are **write-once-by-hash under a `(class,
record_hash)` primary key** — and `ArtifactRegistry` carries `receipt_refs` to
existing classes — "the registry never invents a new receipt class." Where the
bridge receipts should live, and under what `(class, record_hash)` uniqueness
model, is an open question (§10).

## 8. Relationship to `ToolManifest` and `GovernedServiceBinding`

- **`ToolManifest` declares** which receipt classes a bridge tool *can* emit (a
  capability declaration — see #2366).
- **`GovernedServiceBinding` pins** which receipt classes are *required* for a
  given run (the run's evidence contract — see #2367).
- **A bridge run is incomplete if its expected receipts are missing.** The
  binding's required set is the checklist; absence is a failure, not a silent
  pass.
- **No-default-write / dry-run-only mode emits dry-run evidence only** — a
  `BridgeDryRunReceipt` and nothing that claims a write.

Issues #2366 and #2367 are the dependent substrate work; this document is
`Refs`-only and closes neither.

## 9. Non-goals / non-claims

- no runtime implementation; no new API route; no OpenAPI change; no migration;
- no production, pilot-readiness, or live-federation claim;
- no deployed bridge behavior; no claim any bridge can import real rows today;
- no raw Drive import; no live sync;
- no private data;
- no payment-processing / wallet / token / cryptocurrency framing (external
  settlement is *observed*, never processed);
- no claim that current NYCN operations are ICN-native.

## 10. Open questions

1. Should these be Rust types, receipt-enum variants, or schema-defined event
   names (or a mix — enum variants for kernel-visible classes, schema names for
   app-level ones)?
2. Does `BridgeImportReceipt` live in a general `ReceiptStore`, in the artifact
   registry's `receipt_refs`, or both (coordinating record in `ReceiptStore`,
   referenced from the registry)? If in `ReceiptStore`, what `class` string does
   it carry, and does the existing write-once `(class, record_hash)` uniqueness
   model fit bridge re-run / idempotency needs (a re-decided import produces a
   distinct `record_hash`, never an in-place overwrite)?
3. How do `VaultObjectWriteReceipt`s expose proof (existence, hash, privacy
   class) without leaking private content?
4. What is the final name for the follow-up underlying-object receipt, once the
   governed-object model is settled?
5. How are receipt refs surfaced in the Member Shell
   ([`member-shell-v0.md`](member-shell-v0.md)) and Steward Cockpit
   ([`steward-cockpit-v0.md`](steward-cockpit-v0.md)) without exposing private
   organizer data?
6. How are export / delete / recovery proofs attached to (or referenced from) a
   `BridgeImportReceipt`?

---

_Provenance: derived from the NYCN airlock lane (NYCN #84–#89) and the ICN
requirements note (#2364), tracking ICN issue #2365. A vocabulary/boundary spec,
not an implementation or deployment claim._
