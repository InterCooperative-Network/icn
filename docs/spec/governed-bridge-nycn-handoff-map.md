---
Status: draft spec / handoff map
Canonical: no
Authority: architecture / Tool Commons planning (downstream demand signal; not yet normative)
Last Reviewed: 2026-07-08
---

# Governed Bridge — NYCN Intake Handoff Map

> **Status: draft spec, handoff map.** Maps NYCN's **fake** intake-import airlock
> rehearsal output onto ICN's governed-bridge conformance contract, so the fake
> fixture at `../../tools/bridge-conformance/nycn-intake-handoff-v0/` can be read
> alongside its NYCN source. It defines a vocabulary/field mapping only — it
> implements no bridge, adds no route, reads no source, and does not imply any
> bridge can import real rows today. Both sides are fake. It advances ICN #2377
> without closing it.

## 1. Purpose

ICN #2375 landed the governed-bridge conformance contract and its validator
(`tools/validate-governed-bridge-conformance.py`), with a first fake fixture
`review-coverage-v0`. NYCN #85 / #87 / #88 / #89 landed fake airlock fixtures and
a local validator on the producing side. Neither side referenced the other. This
document is the explicit bridge between the two fake vocabularies, and it backs
the fixture `nycn-intake-handoff-v0`, which expresses NYCN's fake intake
rehearsal in the ICN contract and passes the ICN validator unchanged.

The NYCN source is `docs/bridge-rehearsals/fake-intake-import-airlock/`
(`source-records`, `dry-run-plan`, `review-decision`) in the NYCN repo. Every
value on both sides is invented.

## 2. Non-claims

- Fake data only; nothing was imported, handed off, synced, or written to a node.
- No runtime bridge, no connector, no live sync, no real Drive / Sheets /
  SimpleTix rows, no private operational data.
- No production, pilot-readiness, or live-federation claim; no claim that NYCN
  operations are ICN-native today.
- No payment / settlement / wallet / token / cryptocurrency framing. External
  references are **observed, never processed**.
- Action cards are derived read views, never write targets.
- `ArtifactReceipt` (verified-transfer proof) never satisfies
  `ArtifactRegistrationReceipt`.
- A receipt records an institutional fact and grants zero authority.

## 3. Record and field identity

| NYCN concept | ICN concept | Note |
|---|---|---|
| `record_id` (`fake-intake-001`) | `source_record_ref` (`nycn_rec_fake_001`) | ICN requires an **opaque/hash-bound** ref, never a raw natural or PII key. The fixture uses fresh opaque ids; a real bridge would hash-bind the source key. |
| `source_field` (flat, e.g. `accessibility_need`) | `field_path` (dotted, `attendee.accessibility_need`) | Mechanical rule: `attendee.<source_field>` for intake records. The same `field_path` must appear identically across binding, dry-run, and steward-review. |
| `source_simulated` prose + external `EXT-*-fake-*` tokens | `allowed_source_systems` + `source_system_id` (`src_sys_nycn_fake_intake`) + `source_shape_ref` | NYCN names its source in prose; ICN needs an allowlisted opaque system id. |

## 4. Privacy-class handling

ICN's validator treats `privacy_class` as an opaque string, so NYCN's richer
taxonomy passes through **verbatim** — `public`, `participant_visible`,
`care_sensitive`, `organizer_only`, `follow_up_only`, `external_reference`,
`discard`. ICN does not force NYCN to collapse these into a shorter set; the
class travels with the field into the custody map. This is the anti-capture
property: the contract fixes the safety invariants (custody kinds, receipt
obligations, coverage, privacy scan) but leaves the institution's data model to
the institution.

## 5. Custody-target translation

NYCN's free-text `future_icn_target` strings map onto ICN's closed 7-kind
`custody_target.kind` enum:

| NYCN `future_icn_target` | ICN `custody_target.kind` | Receipt(s) on approve |
|---|---|---|
| `ArtifactRegistry / icn-publish public program signal` | `artifact_registry` (namespace) | `BridgeImportReceipt` + `ArtifactRegistrationReceipt` |
| `icn-directory participant projection` | `scoped_vault` (scope `directory-participant-visible`) | `BridgeImportReceipt` + `VaultObjectWriteReceipt` |
| `ScopedVault care-restricted` | `scoped_vault` (scope `care-restricted`) | `BridgeImportReceipt` + `VaultObjectWriteReceipt` |
| `ScopedVault attendees-internal` | `scoped_vault` (scope `attendees-internal`) | `BridgeImportReceipt` + `VaultObjectWriteReceipt` |
| `icn-action-cards follow-up candidate` (consent present) | `governed_object` (`follow-up-record`) | `BridgeImportReceipt` + `FollowUpObjectCreationReceipt` |
| `External bridge reference (ExternalCustodian) — observed, not processed` | `external_reference` | `BridgeImportReceipt` + `ExternalReferenceObservationReceipt` |
| `none — do-not-import` | `discard` | `DiscardDecisionReceipt` |

The intake flow naturally exercises **five** custody kinds (`artifact_registry`,
`scoped_vault`, `governed_object`, `external_reference`, `discard`). It does
**not** naturally exercise `policy_gate` or `policy_block`; see §7. No fields were
synthesized to reach full taxonomy coverage — `review-coverage-v0` already proves
that.

## 6. Gate-field handling

NYCN's `follow_up_consent` is a gate: consent present authorizes a follow-up;
consent absent forbids it. NYCN also treats any field name containing
`permission` as gate-like (e.g. the sponsor flow's `public_logo_permission`,
out of scope here). In this intake fixture the gate is expressed on the
follow-up's own `field_path`: consent present → `approve` the underlying
`governed_object`; consent absent → an automatic `block` (see §9).

## 7. Custody kinds not exercised

`policy_gate` and `policy_block` are ICN custody kinds where a field is routed to
a gate/block target rather than a write target (e.g. an unapproved publication
permission that is blocked from publication, as in `review-coverage-v0`'s sponsor
`public_logo_permission`). The intake flow has no such published-artifact gate;
its one gate (follow-up consent) blocks the creation of a follow-up
`governed_object` rather than the publication of an artifact, so it is modeled as
a steward **block verb** on the governed object, not as a `policy_block` custody
kind. A sponsor publication-permission gate — a natural `policy_block` — is a
candidate follow-up slice (§13).

## 8. Reviewer authority gap

ICN requires a **verifiable** `reviewer_authority_ref` (a DID/signature/authority
basis); a display role alone is explicitly insufficient. NYCN's fake review
decision carries only a role-only `reviewer_role` (`summit_data_steward`) and has
no verifiable-authority field. The fixture supplies a fake
`reviewer_authority_ref` (`authref_nycn_fake_did_sig_001`) with the role as a
display label. NYCN adding a verifiable authority reference to its airlock
review-decision shape is a candidate follow-up (§13).

## 9. Decision decomposition

ICN requires **exactly one atomic decision per `(source_record_ref, field_path)`**,
using one of the closed verbs (`approve`, `reject`, `hold`, `block`, `discard`,
`request_reobservation`, `request_reclassification`,
`request_member_consent_review`). NYCN's review decisions are **compound and
record-level** (e.g. `approve-with-split`, `approve-public-only-hold-rest`) with
field-level lists (`approved_targets` / `rejected_fields` / `held_for_review` /
`blocked_fields` / `gate_fields`). The handoff decomposes each compound
record-level NYCN decision into N atomic ICN per-field decisions. For example, a
single NYCN "approve-with-split, discard the free text, block the no-consent
follow-up" record becomes distinct `approve`, `discard`, and `block` rows keyed
by `(record, field)`.

An **automatic** block (no-consent follow-up) is marked `automatic: true` and
carries **no** `BridgeReviewDecisionReceipt` — an automatic policy block is not a
human review.

## 10. Receipt translation and the action-card conflict

NYCN's fake planning materials name a follow-up as an action-card creation and
use a receipt named `ActionCardCreationReceipt`. ICN **forbids** that name: it is
a deprecated placeholder (see `governed-bridge-receipts.md`), and the conformance
validator rejects it as a literal anywhere inside a fixture directory. The
doctrinal reason is ADR-0027: an action card is a **derived read view** of an
underlying governed object; it is never a write target, so there is no
card-write receipt. The handoff routes a consented follow-up to an underlying
`governed_object` with `FollowUpObjectCreationReceipt`; a card derives from that
object downstream. This deprecated name is named here (outside the fixture
directory) only to document the conflict; it never appears in the fixture.

Otherwise the receipt vocabularies align: `BridgeDryRunReceipt`,
`BridgeReviewDecisionReceipt`, `BridgeImportReceipt`, `VaultObjectWriteReceipt`,
`ArtifactRegistrationReceipt`, `FollowUpObjectCreationReceipt`,
`ExternalReferenceObservationReceipt`, `DiscardDecisionReceipt`, and a
consent policy-block receipt are shared by both sides.

## 11. External references are observe-only

NYCN's `registration_reference` (an external ticketing/registration id) maps to
ICN's `external_reference` custody kind with
`ExternalReferenceObservationReceipt`. ICN records **that** an external system
holds authority over a fact — it does not import the external document, does not
process any settlement, and does not claim to be the source of truth. The binding
marks `external_reference_policy.observe_only: true`.

## 12. Plan-hash gap

ICN binds the review to the dry-run via `reviewed_plan_hash == plan_hash`
(a hash-bound review). NYCN's airlock has no plan-hash concept. The fixture
supplies a fake `planhash_nycn_fake_0001` on both sides. NYCN adding a
plan-hash / reviewed-plan-hash binding to its airlock shape is a candidate
follow-up (§13).

## 13. Candidate follow-ups (not opened by this document)

- A sponsor-flow handoff fixture (exercises `policy_block` publication gates and
  sponsor-domain receipts).
- NYCN-side plan-hash / reviewed-plan-hash support in the airlock review shape.
- NYCN-side verifiable reviewer authority reference (beyond the role-only field).
- A stronger NYCN fake-fixture privacy scan (real-name and external-id shape
  detection, which the NYCN validator does not yet perform).
- A receipt-naming decision for sponsor obligation / governed-object receipts
  (the sponsor flow needs receipts the current `TARGET_RECEIPT` map does not
  name).

These are recorded as direction only; this document opens no issues.
