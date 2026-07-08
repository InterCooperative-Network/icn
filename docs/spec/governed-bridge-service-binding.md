---
Status: draft spec / binding model
Canonical: no
Authority: architecture / Tool Commons planning (downstream demand signal; not yet normative)
Last Reviewed: 2026-07-08
---

# Governed Bridge Service Binding

> **Status: draft spec, binding model.** Models how a future governed bridge
> **refines** the existing [`GovernedServiceBinding`](governed-service-binding.md)
> (#1815) to pin **per-field custody targets** for one governed import run — which
> source systems may be read, which per-field custody classes are allowed, which
> vault scopes / registry namespaces / underlying governed-object targets may be
> written, which receipt sink is required, and what export/delete/recovery path
> must exist before real rows are read. Derived from the NYCN airlock requirements
> note ([`../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md`](../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md)),
> the receipt vocabulary ([`governed-bridge-receipts.md`](governed-bridge-receipts.md),
> #2370), and the ToolManifest modes
> ([`governed-bridge-toolmanifest-modes.md`](governed-bridge-toolmanifest-modes.md),
> #2371), tracking ICN issue #2367. It implements no binding, adds no route, and
> does not imply any bridge can import real rows today. The PR introducing this
> doc advances #2367 without closing it.

## 1. Purpose

The existing `GovernedServiceBinding` already carries a **`Custody class`** field
— "the storage classes the binding's workload may touch in this domain; tightens
or matches the manifest's declared custody class, **never widens**" — plus a
`Capability scope`, a `Mandate reference`, and `Expected receipts`. A governed
bridge needs those coarse fields **refined to a per-field grain**: not "this tool
may touch care-restricted storage," but "this *source field*, of this *privacy
class*, may be written to *this* vault scope, producing *these* receipts."

This document models that refinement. `ToolBinding` (RFC-0017) is the
tool-install projection of `GovernedServiceBinding`; a bridge binding is a further
projection that pins per-`(record, field)` custody for a bridge-adapter run. It
answers:

> How does a future governed bridge binding say which source systems may be read,
> which per-field custody classes are allowed, which vault scopes / registry
> namespaces / underlying governed-object targets may be written, which receipt
> sink is required, and what export/delete/recovery path must exist before real
> rows are read?

This is **docs/spec planning only** — a binding model, not an implementation. It
defines no wire schema (the YAML in §5 is illustrative), changes no runtime, and
inherits the existing binding's invariants (never widens the manifest; a binding
without a covering mandate may not run) rather than redefining them.

## 2. Binding doctrine

> The manifest says what a tool **can** do.
> The binding says what this institution allows **this run** to do.
> The bridge proposes custody; the steward authorizes custody.
> Per-field custody mapping must be explicit before real rows are read.

And the layered-evidence rule:

> A manifest capability is not a permission.
> A binding permission is not a steward decision.
> A steward decision is not a target write.
> Each layer needs its own evidence.

The binding is the **permission** layer: it narrows the manifest's declared modes
(#2371) to concrete sources, scopes, and targets, and it never widens what the
manifest or the domain's mandate allow.

## 3. Manifest vs binding split

| Concern | `ToolManifest` / `ToolBinding` capability declaration (#2371) | `GovernedServiceBinding` run/institution pin (this spec) | Why the split matters |
| --- | --- | --- | --- |
| source-shape read | declares `source_shape_read` support | pins **which** source shapes (`source_shape_refs`) | a capability is not a chosen source |
| real-row read | declares gated `real_row_read` support | pins `real_row_read_enabled` + the `promotion_gates` that must hold | reading real rows is a per-run decision, gated |
| classification output | declares `classification_output` support | pins the `field_custody_map` (per-field source-authority + privacy class) | classification must be concrete before writes |
| dry-run preview | declares `dry_run_preview` support | pins that a dry-run precedes any write | evidence precedes authority |
| no-default-write | declares `no_default_write` support | pins fail-closed default for this run | permission is explicit, not ambient |
| steward review handoff | declares `steward_review_handoff` support | pins the concrete `steward_review_surface` the run yields to (#2369) | a capability to yield is not a chosen review surface |
| custody write | declares `custody_write` support + target *kinds* | pins the **exact** vault scopes / registry namespaces / governed-object targets | a capability is not a destination |
| external reference observation | declares `external_reference_observe` support | pins allowed external systems + observe-only policy | observation is enumerated, not open-ended |
| refusal policy | declares `refusal_policy_enforcement` support | pins the refusal set for this run | refusal is enforced per run |
| export/delete/recovery | declares `export_delete_recovery` support | pins the concrete export/delete/recovery path per target | reversibility must be a real path, not a promise |

> #2371 declares capability **modes**; this spec pins concrete **sources, scopes,
> targets, sinks, and export/delete paths** for one governed run.

## 4. Required binding fields

All fields are **planned / future** — a refinement of the `GovernedServiceBinding`
spec (#1815); none exists in code. "Required before real-row read?" marks the
fields that must be present and satisfied before `real_row_read_enabled` may be
true.

| Binding field | Meaning | Required before real-row read? | Example shape | Evidence / receipt relationship | Status |
| --- | --- | --- | --- | --- | --- |
| `binding_id` | Identifier of this bridge binding | yes | opaque id | anchors all receipts for the run | Planned |
| `tool_manifest_id` | The `ToolManifest` (#2371) this binding refines | yes | opaque id | ties declared modes to pinned permissions | Planned |
| `operator_authority` | Mandate + `GovernanceDecisionReceipt` under which the binding runs | yes | mandate ref + receipt ref | a binding without a covering mandate may not run (inherited) | Planned |
| `allowed_source_systems` | Enumerated external systems this run may read | yes | list of system ids | scopes `ExternalReferenceObservationReceipt` provenance | Planned |
| `source_shape_refs` | The repo-safe source shapes the run is bound to | yes | list of shape refs | shape-read needs no row access | Planned |
| `real_row_read_enabled` | Whether this run may read real rows | — | bool (default false) | gated on all `promotion_gates` | Planned |
| `promotion_gates` | The §8 gates that must hold before real rows are read | yes | list of gate ids | each gate is a precondition, not a receipt | Planned |
| `field_custody_map` | Per-field source-authority → privacy class → custody target (§5) | yes | map (see §5) | drives `BridgeImportReceipt` + target receipts | Planned |
| `allowed_scoped_vault_targets` | The exact `ScopedVault` scopes writable | yes (for vault fields) | list of scope ids | `VaultObjectWriteReceipt` | Planned |
| `allowed_artifact_registry_namespaces` | The exact registry namespaces writable | yes (for public fields) | list of namespace ids | `ArtifactRegistrationReceipt` | Planned |
| `allowed_governed_object_targets` | The underlying governed-object classes writable (for follow-ups) | yes (for follow-up fields) | list of object classes | `FollowUpObjectCreationReceipt` | Planned |
| `receipt_sink` | Where emitted receipts land (a `ReceiptStore` / registry `receipt_refs`) | yes | sink ref | persists all run receipts | Planned |
| `required_receipts` | The receipt classes this run must emit to be complete | yes | list of classes | absence = incomplete run | Planned |
| `export_delete_recovery_refs` | The concrete export/delete/recovery path per target | yes | per-target refs | referenced by `BridgeImportReceipt` | Planned |
| `external_reference_policy` | Allowed external systems + observe-only rule | yes (if observing) | policy block | `ExternalReferenceObservationReceipt` | Planned |
| `refusal_policy` | The refusal set enforced for this run | yes | list of refused classes | policy-block / discard receipts | Planned |
| `steward_review_surface` | The review surface the run yields to (#2369) | yes | surface ref | `BridgeReviewDecisionReceipt` | Planned |
| `member_visibility_policy` | What (if anything) is member-visible, privacy-bounded | — | policy block | governs member-facing receipt rendering | Planned |

The custody / capability fields **tighten or match** the manifest's declared modes
and the domain's mandate; they **never widen** them (inherited from the
`GovernedServiceBinding` custody-class and capability-scope rules).

## 5. Per-field custody map

**Illustrative and non-normative.** Field names below are *examples of field
paths*, not real source fields; real source-record references are
**opaque/hash-bound** (never raw natural or PII-bearing keys), and coverage is
enforced per `(record, field)` at runtime even though the map is *configured* by
field path / privacy class.

```yaml
# ILLUSTRATIVE ONLY — not a schema; field paths are examples, not real data.
field_custody_map:
  attendee.accessibility_note:
    source_authority: external_form_submission
    privacy_class: care_restricted        # NYCN operational class; maps onto the
                                           # binding's privacy class (Public/Member/
                                           # NeedToKnow) — reconciliation is forward work
    custody_target:
      kind: scoped_vault
      scope: care-restricted
    required_receipts:
      - BridgeImportReceipt
      - VaultObjectWriteReceipt
  sponsor.public_logo_permission:
    source_authority: sponsor_form
    privacy_class: gate_only               # a gate, consumed — never published/vaulted
    custody_target:
      kind: policy_gate
    required_receipts:
      - PublicationConsentBlockReceipt     # when permission is absent (automatic block)
```

The map binds each field to a source authority, a privacy class, a custody
*target* (§6), and the receipt set that target requires (§7). A field with no map
entry has no permitted destination — the refusal-by-default posture.

## 6. Custody target classes

Supported `custody_target.kind` values:

- `scoped_vault` — a write into a [`ScopedVault`](artifact-registry-and-scoped-vault.md)
  scope (care / sponsor / finance / attendees-internal / …);
- `artifact_registry` — registration of a public artifact into a registry
  namespace;
- `governed_object` — creation of an **underlying governed object** (from which a
  derived action card may later be read);
- `external_reference` — an observed external reference (observe-only);
- `policy_gate` / `policy_block` — a gate consumed or an automatic block, producing
  receipt evidence, no custody object;
- `discard` — a discard decision, producing receipt evidence, no custody object.

Boundaries:

- **action cards are derived read views, not write targets** (ADR-0027) — a
  consent-gated follow-up writes a `governed_object`, and the card is derived from
  it;
- **external references are observe-only** — never a settlement / payment path;
- **discard / policy-block** may create receipt evidence without any custody
  object.

## 7. Required receipt set

The binding lists the expected receipts for each target, tied to
[`governed-bridge-receipts.md`](governed-bridge-receipts.md):

- a **dry-run-only** run requires a `BridgeDryRunReceipt` only;
- a **real write** requires a `BridgeReviewDecisionReceipt`, a `BridgeImportReceipt`
  (citing the dry-run preview it confirms), and the **target** write receipts
  (`VaultObjectWriteReceipt` / `ArtifactRegistrationReceipt` /
  `FollowUpObjectCreationReceipt`);
- a **no-review policy block** uses a policy-block receipt
  (`ConsentPolicyBlockReceipt` / `PublicationConsentBlockReceipt`), **never** a
  review-decision receipt;
- a **missing expected receipt means the run is incomplete** — the binding's
  `required_receipts` is the checklist, and absence is a failure, not a silent
  pass.

## 8. Promotion gates

Adapted from the requirements note, at binding grain. No real row may be read
until **all** of these hold for the binding:

1. source authority registered;
2. allowed source configured (`allowed_source_systems`);
3. privacy/custody classes defined for every mapped field;
4. target vault scopes exist (`allowed_scoped_vault_targets`);
5. artifact namespace exists (`allowed_artifact_registry_namespaces`);
6. governed-object class exists (`allowed_governed_object_targets`);
7. receipt sink exists (`receipt_sink`);
8. steward review surface exists (`steward_review_surface`, #2369);
9. dry-run preview exists;
10. no-default-write enforced;
11. export/delete/recovery path exists (`export_delete_recovery_refs`);
12. external-reference observe-only policy exists (`external_reference_policy`);
13. private data never enters git or a repo-safe artifact — the invariant that
    outlives the rest.

## 9. Steward and member visibility

- the binding **pins the steward review surface target** (likely the Steward
  Cockpit, [`steward-cockpit-v0.md`](steward-cockpit-v0.md));
- **role display vs authority proof**: the `BridgeReviewDecisionReceipt` binds a
  *verifiable* reviewer authority reference; a role label is display-only, never
  the sole evidence;
- **member-facing visibility is separate and privacy-bounded**
  ([`member-shell-v0.md`](member-shell-v0.md)) — a member sees only what the
  `member_visibility_policy` permits;
- **receipt refs may render differently** to steward vs member; and
- **private organizer data must not leak** through receipt refs or the member
  shell — references are opaque/hash-bound or vault-backed.

## 10. External reference policy

For `external_reference_observe`:

- allowed external reference systems are **enumerated** (`allowed_source_systems`
  / `external_reference_policy`);
- the observation records a source id / source hash / mapping version — never a
  raw payload;
- the bridge **observes a status/reference only**;
- **no settlement, payment-processing, wallet, token, or cryptocurrency path**;
- export/delete behavior must be defined for observed references, just as for
  written custody.

## 11. Non-goals / non-claims

- no runtime implementation; no new API; no OpenAPI change; no migration;
- no production, pilot-readiness, or live-federation claim; no deployed bridge
  behavior;
- no raw Drive import; no live sync;
- no private data;
- no payment-processing / wallet / token / cryptocurrency framing (external
  settlement is *observed*, never processed);
- no claim that current NYCN operations are ICN-native.

## 12. Open questions

1. Does `field_custody_map` live in `GovernedServiceBinding` directly, in
   `ToolBinding`, or in an attached **bridge profile** that refines the binding's
   custody-class field?
2. How are opaque/hash-bound source references represented so per-`(record, field)`
   coverage is enforceable without leaking private keys?
3. How are target scopes/namespaces validated to exist (and be export/delete
   capable) *before* `real_row_read_enabled` may be set?
4. How does a binding **prove** its export/delete/recovery capability exists,
   rather than merely asserting it?
5. How are receipt sinks authorized and discovered?
6. How should a binding lint catch a mapped field whose target receipt set is
   missing a required class?
7. How should this interact with the future steward review surface (#2369) — does
   the binding reference a surface, or does the surface reference the binding?

---

_Provenance: derived from the NYCN airlock lane (NYCN #84–#89), the ICN
requirements note (#2364), the receipt vocabulary (#2370), and the ToolManifest
modes (#2371), tracking ICN issue #2367. A binding-refinement model, not an
implementation or deployment claim. #2367 stays open — referenced, not addressed
by this doc._
