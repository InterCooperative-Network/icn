---
Status: draft spec / external reference model
Canonical: no
Authority: architecture / Tool Commons planning (downstream demand signal; not yet normative)
Last Reviewed: 2026-07-08
---

# Governed Bridge External References

> **Status: draft spec, external reference model.** Defines the **observe-only**
> external-custodian reference model a governed bridge would use to record that an
> external system holds authority over a fact — **without** importing the
> document, processing settlement, or pretending ICN is the source of truth for
> that external fact. It is the `#2368` "external-custodian model" that the
> `ExternalReferenceObservationReceipt` in
> [`governed-bridge-receipts.md`](governed-bridge-receipts.md) (#2370) was written
> against, the target of the `external_reference_observe` mode
> ([`governed-bridge-toolmanifest-modes.md`](governed-bridge-toolmanifest-modes.md),
> #2371), and the model behind the binding's `external_reference_policy`
> ([`governed-bridge-service-binding.md`](governed-bridge-service-binding.md),
> #2372). Derived from the NYCN airlock requirements note
> ([`../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md`](../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md)),
> tracking ICN issue #2367→#2368. It implements no connector, no settlement, and
> does not imply any bridge can import real rows today. The PR introducing this
> doc advances #2368 without closing it.

## 1. Purpose

Some facts a cooperative cares about live — and stay authoritative — in an
external system: an invoice's paid/unpaid status in an accounting package, a
ticket's check-in status in a registration system, a document's existence in a
file store. A governed bridge should be able to **note that such a fact exists
and who holds authority over it**, so a steward can act on it, without ICN
copying the document, settling the obligation, or claiming to be the fact's
source of truth.

This document models that observe-only reference. It answers:

> How can ICN record that an external system holds authority over a fact without
> importing the document, processing settlement, or pretending ICN is the source
> of truth for that external fact?

This is **docs/spec planning only** — an observe-only reference model, not an
implementation. It defines no connector, no wire schema (the tables below are
illustrative), no settlement path, and no live sync.

## 2. Observe-only doctrine

> ICN may record that an external custodian claims or holds authority over a fact.
> ICN does not become the external custodian.
> ICN observes a reference; it does not process settlement.
> A reference is evidence of an observation, not proof the external fact is true.

And the three things an observation is **not**:

> External reference observation is not custody of the external document.
> External reference observation is not payment processing.
> External reference observation is not source-authority transfer.

The external system keeps authority over its own fact; ICN keeps only a
**dated, opaque reference** to what it observed, and a receipt proving the
observation happened.

## 3. External authority boundary

Illustrative examples (fake/generic names only). For each, the external custodian
keeps authority; ICN observes a bounded reference and must not overclaim.

| External fact / artifact | External custodian authority | What ICN may observe | What ICN must not claim | Required evidence |
| --- | --- | --- | --- | --- |
| sponsor invoice id / invoice status | the external accounting system | that an invoice reference exists and its observed status label | that ICN settled, holds, or processed the invoice | `ExternalReferenceObservationReceipt` |
| external registration record id | the registration system | that a registration reference exists | that ICN is the registration authority | same |
| ticketing / check-in status | the ticketing system | an observed check-in status label | that ICN performed or verified check-in | same |
| external payment / settlement status | the external settlement system | an observed status label (e.g. "recorded as received") | that ICN settled or processed any payment | same |
| external file / document id | the external file store | that a document reference exists | that ICN holds the document body | same |
| external consent record id | the external consent system | that a consent reference exists | that ICN is the consent system of record | same |

In every row the pattern is identical: **observe a reference + status label,
never the payload; never claim authority ICN does not hold.**

## 4. Reference record model

**Illustrative and non-normative.** All external references are
**opaque/hash-bound** — never a raw natural or PII-bearing key.

| Field | Meaning | Required? | Privacy / custody note | Status |
| --- | --- | --- | --- | --- |
| `external_reference_id` | ICN-side id of this observation record | yes | opaque | Planned |
| `external_system_id` | Which external system was observed | yes | enumerated by the binding (`allowed_source_systems`) | Planned |
| `external_system_kind` | Category of system (accounting / registration / …) | yes | generic label, not a vendor secret | Planned |
| `external_record_ref` | Reference to the external record | yes | **opaque/hash-bound**; never a raw PII-bearing key | Planned |
| `external_status` | Observed status label | yes | a label, not the payload | Planned |
| `observed_at` | When ICN made the observation | yes | ICN clock; distinct from source time (§7) | Planned |
| `observed_by_tool_manifest_id` | The bridge `ToolManifest` (#2371) that observed | yes | ties to declared `external_reference_observe` mode | Planned |
| `governed_service_binding_id` | The binding (#2372) authorizing the observation | yes | scopes the run | Planned |
| `source_hash` | Hash of the observed shape/value | yes | integrity without payload | Planned |
| `mapping_version` | How the bridge interpreted the source at observation time | yes | provenance (§7) | Planned |
| `authority_basis` | Why the external custodian is authoritative here | yes | records whose authority, not ICN's | Planned |
| `observation_scope` | What the observation covers | yes | bounds the claim | Planned |
| `privacy_class` | Privacy/custody class of the reference | yes | drives rendering (§8) | Planned |
| `receipt_ref` | The `ExternalReferenceObservationReceipt` | yes | evidence the observation happened | Planned |
| `export_delete_recovery_ref` | The reference's export/delete/recovery path | yes | ICN-reference lifecycle, not the external source's (§9) | Planned |

Hard constraints on the record:

- external record refs are **opaque/hash-bound**;
- **no raw document body**;
- **no credentials**;
- **no payment instruments**;
- **no raw PII-bearing natural keys**;
- **no settlement instructions**.

## 5. `ExternalReferenceObservationReceipt`

Extends the base row in
[`governed-bridge-receipts.md`](governed-bridge-receipts.md) (external system id ·
external ref · observed status · source hash · timestamp). Minimum evidence:

- external system id;
- opaque external reference;
- observed status or reference kind;
- observation timestamp;
- source hash;
- mapping version;
- bridge tool id / manifest id;
- governed service binding id;
- observer authority / tool identity;
- privacy class;
- export/delete/recovery reference.

> This receipt proves an **observation was recorded**. It does **not** prove the
> external fact is true, current, paid, settled, or authoritative forever — only
> that, at `observed_at`, this bridge under this binding observed this reference
> with this status.

## 6. Binding policy

Per [`governed-bridge-service-binding.md`](governed-bridge-service-binding.md):

- **allowed external systems must be enumerated** (`allowed_source_systems`);
- **allowed reference kinds must be enumerated**;
- the **observe-only policy must be pinned before real-row read** — it is one of
  the binding's promotion gates (`external_reference_policy` exists);
- the binding **must forbid settlement / payment actions** — observation only;
- the binding **must define export/delete behavior** for references (§9).

An external observation therefore never happens outside an enumerated,
observe-only, mandate-covered binding.

## 7. Source authority and freshness

- an external reference may have **source authority over its own domain** — the
  accounting system is authoritative for its invoice status, not ICN;
- ICN records **observation freshness** (`observed_at`), which is distinct from
  the external fact's own timestamp;
- **stale observations need re-observation**, not mutation pretending the old
  observation is still current;
- external facts may be **revoked or superseded** by their custodian — ICN's
  reference does not override that;
- `mapping_version` records **how the bridge interpreted the source** at
  observation time, so a later re-observation under a different mapping is
  distinguishable.

The observation timestamp answers "when did ICN look?"; the source timestamp (if
observed) answers "as of when did the external system assert this?" — the two are
separate and must not be collapsed.

## 8. Privacy and rendering

- the **steward view may show more** than the member view;
- **member-facing rendering must avoid private organizer data**
  ([`member-shell-v0.md`](member-shell-v0.md));
- **raw external refs may be hidden behind opaque labels** — a member may see
  "invoice on file (external)", never the raw external id;
- **receipt refs render differently by audience** (steward cockpit vs member
  shell);
- external references **must not leak private contact data** through URLs, ids, or
  filenames — the reference is opaque/hash-bound, and any human-readable label is
  audience-scoped.

## 9. Export / delete / recovery

The ICN reference lifecycle is **distinct** from the external source lifecycle:

- **deleting an ICN reference does not delete the external source** — ICN never
  held it;
- **deleting the external source does not automatically delete the ICN
  observation** unless the binding's policy says so — the observation is a
  historical fact ("ICN saw this at this time");
- the export/delete/recovery path should be able to:
  - **remove** the local reference;
  - **mark** an observation stale / revoked;
  - **re-observe** to refresh;
  - **export** reference metadata (not payload);
  - **preserve or redact** receipt refs per policy (a receipt log may be
    append-only; redaction is a policy decision, not a silent delete).

## 10. Non-goals / non-claims

- no runtime implementation; no connector implementation; no new API;
- no production, pilot-readiness, or live-federation claim; no deployed bridge
  behavior;
- no raw Drive import; no live sync;
- no private data;
- no payment-processing / wallet / token / cryptocurrency framing; **no
  settlement processing** — external settlement is *observed*, never processed;
- no claim that current NYCN operations are ICN-native;
- **no claim that ICN settles or intermediates external obligations.**

## 11. Open questions

1. Does the external reference model live in `GovernedServiceBinding`, in the
   `ArtifactRegistry`, in a `ScopedVault`, or in a shared `ExternalReference`
   object referenced from all three?
2. How are external refs made **opaque** while still **deduplicatable** (so two
   observations of the same external record can be recognized without exposing a
   raw key)?
3. How are **stale** observations represented — a status flag, a freshness
   horizon, or a superseding record?
4. What exactly distinguishes the **observation timestamp** from the **source
   timestamp**, and should both always be recorded?
5. How should member / steward rendering **hide sensitive external ids** while
   keeping the reference actionable for a steward?
6. How do export/delete/recovery policies interact with **append-only receipt
   logs** (redaction vs deletion)?
7. Should external observations be **revalidated by scheduled jobs** later, or
   only re-observed manually?

---

_Provenance: derived from the NYCN airlock lane (NYCN #84–#89, esp. the sponsor
pipeline #86), the ICN requirements note (#2364), the receipt vocabulary (#2370),
the ToolManifest modes (#2371), and the binding custody map (#2372), tracking ICN
issue #2368. An observe-only reference model, not an implementation or deployment
claim. #2368 stays open — referenced, not addressed by this doc._
