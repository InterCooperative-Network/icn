---
Status: requirements note / downstream demand signal
Canonical: no
Authority: architecture (downstream demand signal from NYCN; not doctrine, not normative)
Last Reviewed: 2026-07-08
---

# NYCN Airlock Bridge Requirements

> **This is downstream demand pressure from NYCN, not an ICN implementation
> claim.** It captures what ICN would need to provide *before* a governed
> bridge could safely move real NYCN operational rows from external systems
> into ICN custody. It implements nothing, deploys nothing, and does not claim
> any real NYCN data is in ICN custody today. Every primitive named below is
> cross-referenced to its actual implementation status; the honest default is
> **planned / spec / RFC**, not implemented.

## 1. Purpose

The New York Cooperative Network (NYCN) has been rehearsing, in a separate
repo, how its Summit operations (registration/intake, sponsor pipeline, and
similar external-system data) would eventually come into ICN-native custody.
Those rehearsals are deliberately **fake and repo-safe** — no real data — but
they surfaced a consistent set of demands on the ICN substrate.

This document lifts those demands into ICN's own planning vocabulary — the
[Cooperative Tool Commons](COOPERATIVE_TOOL_COMMONS.md),
[`GovernedServiceBinding`](../spec/governed-service-binding.md),
[`ToolManifest`/`ToolBinding` (RFC-0017)](../rfcs/RFC-0017-tool-install-infrastructure.md),
and [`ArtifactRegistry` / `ScopedVault`](../spec/artifact-registry-and-scoped-vault.md) —
so that the substrate work can be planned against a concrete downstream
consumer.

It answers one question:

> **What must ICN provide before a governed bridge can safely move real NYCN
> operational rows from external systems into ICN custody?**

What this document is **not**:

- not an implementation, and not a bridge connector;
- not a roadmap commitment or a sequencing promise;
- not a deployment, pilot-readiness, or live-federation claim;
- not a claim that current NYCN operations are ICN-native.

## 2. Source signal

The demand originates in these NYCN artifacts (cross-repo; cited by path, not
linked):

| NYCN artifact | What it is |
| --- | --- |
| `docs/architecture/OPERATIONS_TO_ICN_NATIVE_MIGRATION.md` | Migration doctrine: a 0–8 ladder (nine rungs) from external reality to external-tool retirement, plus a surface-by-surface sequencing table. |
| `docs/bridge-rehearsals/fake-intake-import-airlock/` | Fake registration/intake airlock rehearsal (fictional records → classification → dry-run → steward review). |
| `docs/bridge-rehearsals/fake-sponsor-pipeline-airlock/` | Fake sponsor-pipeline airlock rehearsal (adds finance/sponsor/publication-consent classes). |
| `docs/bridge-rehearsals/AIRLOCK_FIXTURE_CONVENTION.md` | The shared shape both rehearsals converged on. |
| `docs/bridge-rehearsals/BRIDGE_OUTPUT_REQUIREMENTS.md` | Maps the recurring rehearsal outputs (15 output classes, ~17 primitives, 12 promotion gates) onto required ICN primitives. |
| `tools/validate-airlock-fixtures.py` | A machine-checkable validator over the fake fixtures (structure, per-record coverage, privacy scan). |

What those artifacts proved (all on fake data):

- **Airlock, not quarantine.** The rule is not "never bring data to ICN"; it is
  "no private data in git — real data comes home through a governed bridge into
  a vault." The repo is the map and safety boundary; the ICN node is the
  eventual custody home.
- **Rehearsals read shape only.** A rehearsal reads a repo-safe *shape*, never a
  real row.
- **A real governed bridge eventually reads real rows** — but only at migration
  rung 5→6 (governed bridge → ICN-native custody), once the promotion gates in
  §8 hold, and it routes those rows into vault/registry custody, **never into
  git or a repo-safe artifact.**
- **Every field needs a full story:** a source-authority classification, a
  privacy/custody class, a dry-run proposal, a steward decision, an expected
  receipt, and an export/delete path — before any real write.
- **Coverage is per `(record, field)`.** The validator learned (through review)
  that coverage keyed on a bare field name is a false-pass trap: a field decided
  for one record must not count as decided for another record sharing the name.

## 3. Bridge capability model

The future governed bridge is best modeled as a **tool/service under a
[`GovernedServiceBinding`](../spec/governed-service-binding.md)** — an installed
Tool Commons capability operating on institution-owned state, not a tool that
owns institutional truth. To move real rows safely it must be able to:

- **read** an external source export or connector stream **only under explicit
  operator/steward authorization** (never by default, never ambiently);
- **classify** every field *before* writing anything;
- **produce a dry-run plan** — a preview of proposed custody actions that writes
  nothing;
- **require steward confirmation** before any write;
- **write only to allowed custody targets** (the vault scopes / registry
  namespace / **underlying governed object** named in the binding — never an
  action card: a card is a derived read view, not a write target);
- **produce receipts** for each decision and each write;
- **refuse by policy** unbounded free text, credentials, payment instruments,
  and unsupported classes;
- **capture provenance** — source id/hash, mapping version, target scope,
  privacy class — on every import decision;
- **support export / delete / recovery** paths for anything it writes;
- **observe external settlement references without processing payment** — a
  reference or status is recorded; no settlement action is taken.

Three doctrine lines the rehearsals kept returning to:

> The bridge proposes custody; the steward authorizes custody.
> A dry-run is evidence, not authority.
> A receipt proves a decision happened, not that the decision was wise.

## 4. ToolManifest implications

What the bridge's [`ToolManifest` (RFC-0017)](../rfcs/RFC-0017-tool-install-infrastructure.md)
capability declaration would need to express. **Status is `planned` / `RFC`
unless noted; `ToolManifest`/`ToolBinding` are RFC-0017 constructs with no code
today.**

| Capability | Why NYCN needs it | ToolManifest / capability-declaration implication | Safety gate | Status |
| --- | --- | --- | --- | --- |
| Source-shape read mode | Rehearsals read a repo-safe shape only | Declare a read capability scoped to a *shape/schema*, no row access | Never touches real rows | Planned (RFC-0017) |
| Real-row read mode | Rung 5→6 bridge reads actual rows into custody | Distinct, higher capability, gated on §8 promotion gates + operator authorization | Off unless gates hold | Planned (RFC-0017) |
| No-default-write mode | Refuse-by-default is the safe posture | A declared mode where the default action is "none"; writes require explicit confirmation | Fails closed | Planned |
| Classification output | Every field classified before any write | Declare a classification output the manifest binds to a custody taxonomy | Precedes all writes | Planned |
| Dry-run output | A preview must exist before a write | Declare a dry-run/preview capability that produces evidence, not state | Writes nothing | Planned |
| Steward review handoff | The human gate authorizes custody | Declare a review-handoff surface (steward cockpit) the bridge yields to | No write without review | Planned (see [steward-cockpit-v0](../spec/steward-cockpit-v0.md)) |
| Custody-target write permissions | Only allowed targets may be written | Enumerate permitted vault scopes / registry namespace / underlying governed-object target (not the read-only action-card surface) | Deny-by-default | Planned |
| Receipt emission | Each decision/write needs evidence | Declare which receipt classes the tool emits | Receipt per write | Only `ArtifactReceipt` implemented; bridge receipts planned |
| Export/delete hooks | Custody must be reversible | Declare export/delete/recovery affordances per target | No irreversible custody | Planned |
| External-custodian reference observation | Settlement is observed, not processed | Declare an observe-only reference capability (no payment path) | No payment/settlement action | Planned |
| Credential / payment-instrument refusal | Secrets/instruments never enter tool state | Declare a refusal policy for these classes | Rejected outright | Planned |

## 5. GovernedServiceBinding implications

What the [`GovernedServiceBinding`](../spec/governed-service-binding.md) around
the bridge would need to pin. All rows are **planned / spec-WIP** (the binding
spec advances #1815; no code today).

| Binding concern | Requirement | Reason | Failure mode if missing | Status |
| --- | --- | --- | --- | --- |
| Explicit authorization | Operator/steward must authorize the binding and each real-import run | Custody is a governed act, not an ambient tool power | Silent/ambient import of real rows | Planned |
| Allowed source systems | Enumerate the specific external systems the bridge may read | An unbounded reader is an exfiltration surface in reverse | Reads arbitrary sources | Planned |
| Allowed custody targets | Enumerate the vault scopes / registry namespace / underlying governed-object target | Writes must be to pre-approved destinations only | Writes land anywhere | Planned |
| Vault scopes / classes | Bind each field-class to a `ScopedVault` scope | Care/sponsor/finance data are need-to-know | Sensitive data mis-scoped | Planned ([artifact-registry-and-scoped-vault](../spec/artifact-registry-and-scoped-vault.md)) |
| Artifact registry namespace | Bind public artifacts to an `ArtifactRegistry` namespace | Public recognition/output must be addressable + governed | Public artifacts ungoverned | Planned (same spec) |
| Follow-up governed object | Bind a consent-gated follow-up to the **underlying governed object** the action-card surface derives from — the bridge writes that object, never the card (ADR-0027: cards are derived views with no mutation API) | Follow-ups are member-facing consent-gated work realized through a governed object, not a card write | Follow-ups bypass consent, or a non-existent card-write path gets designed | `GET /me/action-cards` read surface implemented; the governed follow-up object is planned |
| Receipt sink | Bind a durable sink for the emitted receipts | Evidence must persist and be auditable | Decisions leave no trail | Planned; `ArtifactReceipt` is the only implemented class |
| External custodian references | Declare where observed external references live | Settlement stays external; ICN records a reference only | Payment logic pulled in-house | Planned |
| Rollback / export / delete | A reversal path for any written custody | Custody without deletion is a one-way trap | No way to withdraw imported data | Planned |
| Audit visibility | Steward + member surfaces can see what was imported and why | Governance requires legibility | Opaque imports | Planned (member-shell / steward-cockpit specs) |
| Denial policy | Explicit refusal for credentials, instruments, unbounded free text, unsupported classes | Some classes must never enter tool state | Secrets/PII leak into custody | Planned |

## 6. Required receipt vocabulary

The rehearsals coined a receipt family. **Only `ArtifactReceipt` exists in code
today** (`icn/crates/icn-kernel-api/src/proofs.rs`); every bridge-specific
receipt below is **expected / future** vocabulary, not an ICN type.

One precision the airlock design must not lose: the implemented `ArtifactReceipt`
proves a **verified blob transfer** (an action *on* an artifact), **not** that an
artifact was recorded in the registry. Per
[`artifact-registry-and-scoped-vault.md`](../spec/artifact-registry-and-scoped-vault.md)
(§"`ArtifactReceipt` vs `ArtifactRegistry`"): "the receipt proves a blob
transfer; the registry records that an artifact exists and what governs it —
these are distinct concepts." So a real bridge write into custody still owes the
**planned** registry-recording (`ArtifactRegistrationReceipt`) and vault-write
(`VaultObjectWriteReceipt`) evidence; `ArtifactReceipt` does not stand in for
them.

| Receipt | Role | Status |
| --- | --- | --- |
| `ArtifactReceipt` | Proof a **blob transfer completed and content was verified** (ADR-0026 Layer 2) — an action *on* an artifact, **not** proof it was recorded in the registry | **Implemented** (`icn-kernel-api`) |
| `BridgeDryRunReceipt` | The dry-run pass itself | Expected / future |
| `BridgeReviewDecisionReceipt` | A steward review decision (a human reviewed) | Expected / future |
| `BridgeImportReceipt` | The import decision itself (source id/hash, mapping version, target scope, privacy class) — expected on **every** write path | Expected / future |
| `VaultObjectWriteReceipt` | A write into a `ScopedVault` | Expected / future |
| `ArtifactRegistrationReceipt` | Proof an artifact was **recorded in the `ArtifactRegistry`** (that it exists and what governs it) — distinct from `ArtifactReceipt`'s transfer proof, and not satisfied by it | Expected / future |
| `ExternalReferenceObservationReceipt` | An observed external reference (observed, not processed) | Expected / future |
| `DiscardDecisionReceipt` | A discard decision | Expected / future |
| `ConsentPolicyBlockReceipt` | An automatic no-consent block (no human review implied) | Expected / future |
| `PublicationConsentBlockReceipt` | An automatic no-publication-permission block | Expected / future |
| `ActionCardCreationReceipt` | Proof the **underlying governed object** behind a consent-gated follow-up was created (the card is *derived* from it per ADR-0027 — there is no card write; the name is subject to reconciliation with that object's own creation receipt) | Expected / future |

Open design question (§10): whether these should be a first-class bridge
receipt family or projections of existing artifact receipts.

## 7. Custody targets and scopes

The rehearsals routed fields into a small set of custody classes. Each maps to a
forward-direction ICN primitive:

| Custody class | ICN primitive it maps to |
| --- | --- |
| Public artifact | [`ArtifactRegistry`](../spec/artifact-registry-and-scoped-vault.md) namespace (governed public output) |
| Operational record | On-node operational record (institution-owned state under the Tool Commons) |
| Private vault object | [`ScopedVault`](../spec/artifact-registry-and-scoped-vault.md) — general private scope |
| Care-restricted vault object | `ScopedVault` care-restricted scope (need-to-know) |
| Sponsor-restricted vault object | `ScopedVault` sponsor-restricted scope |
| Finance-restricted vault object | `ScopedVault` finance-restricted scope |
| External bridge reference | External-custodian reference model (observed, not processed) |
| Action-card candidate | The **underlying governed object** (a proposal / action-item / signal, or a governed follow-up object) that the action-card surface *derives* a read-only card from — `GET /v1/gov/me/action-cards` has **no mutation API** (ADR-0027); the bridge writes the object, never the card |
| Discard / policy-block evidence | Receipt-only evidence (no custody object) |

The taxonomy above is NYCN's operational custody framing. It is deliberately
**mapped, not merged**, onto ICN's forward-direction `PrivacyClass` /
`ScopedVault` model — the two vocabularies stay distinct until an ICN spec
unifies them.

One row deserves emphasis: an **action card is a derived read view, not a
custody target.** Per [ADR-0027](../adr/ADR-0027-action-card-contract.md), cards
are derived views with no mutation API — "cards reference underlying objects;
mutation flows through those." A consent-gated follow-up is therefore realized by
creating or updating the underlying governed object; the card is derived from it.
Designing a card-write path (or a stored `ActionCardCreationReceipt` against the
card itself) would conflict with the existing `standing → card → action →
receipt` proof loop.

## 8. Promotion gates before real bridge use

No real row may pass rung 5→6 until **all** of these hold. Until then the bridge
stays a rehearsal:

1. Source authority is registered (who asserts each field).
2. Privacy/custody classes are defined for every field.
3. The allowed source system is configured (not open-ended).
4. The target `ScopedVault` scopes exist and are export/delete-capable.
5. The `ArtifactRegistry` namespace exists.
6. The receipt vocabulary exists (at least `BridgeImportReceipt` + the target-specific classes).
7. A steward review flow exists (the human authorization gate).
8. A dry-run preview capability exists.
9. No-default-write is enforced (refuse-by-default).
10. An export/delete/recovery path exists for every custody target.
11. External settlement remains observe-only (no payment path).
12. Private data never enters git — the invariant that outlives all of the above.

## 9. Non-goals / non-claims

To be explicit — this document claims none of the following:

- no raw Drive import;
- no live sync;
- no planning-database claim;
- no runtime implementation;
- no production claim;
- no pilot-readiness claim;
- no live-federation claim;
- no payment-processing / wallet / token / cryptocurrency framing;
- no private data;
- no claim that current NYCN operations are ICN-native.

The bridge is an airlock: it may eventually move real rows into governed
custody, but it must never spill them into git.

## 10. Open questions for ICN

Restrained, for ICN architecture to decide:

1. Should the bridge receipts be a **first-class receipt family**, or folded
   into existing artifact-receipt projections?
2. Where should **external-custodian references** live — a dedicated reference
   model, or an attribute on an existing custody object?
3. How should [`ToolManifest`](../rfcs/RFC-0017-tool-install-infrastructure.md)
   express **no-default-write / dry-run-only** modes as declared capabilities?
4. How should [`GovernedServiceBinding`](../spec/governed-service-binding.md)
   express **per-field custody targets** (a field-class → scope map), not just a
   whole-tool scope?
5. What is the **minimum steward review surface** (the smallest
   [steward-cockpit](../spec/steward-cockpit-v0.md) affordance that can
   authorize an import)?
6. How does **member consent review** appear later
   ([member-shell](../spec/member-shell-v0.md) /
   [MEMBER_STANDING](MEMBER_STANDING.md)) without exposing private organizer
   data?
7. How should **export / delete / recovery** be *proven* — a receipt, a
   capability probe, or a periodic attestation?

---

_Provenance: derived from NYCN's fake airlock rehearsals and fixture validator
(NYCN #85/#86/#87/#88/#89). This is a demand signal into ICN planning, not an
ICN implementation or deployment claim._
