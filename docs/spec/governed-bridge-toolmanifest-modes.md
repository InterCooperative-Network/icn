---
Status: draft spec / capability declaration model
Canonical: no
Authority: architecture / Tool Commons planning (downstream demand signal; not yet normative)
Last Reviewed: 2026-07-08
---

# Governed Bridge ToolManifest Modes

> **Status: draft spec, capability declaration model.** Models how a future
> governed bridge would declare its safety modes in a `ToolManifest` /
> `ToolBinding` (RFC-0017), so that no-default-write, dry-run, and gated real-row
> reading are *declared capabilities* rather than runtime hopes. Derived from the
> NYCN airlock requirements note
> ([`../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md`](../architecture/NYCN_AIRLOCK_BRIDGE_REQUIREMENTS.md)),
> ICN issue #2366, and the landed receipt vocabulary
> ([`governed-bridge-receipts.md`](governed-bridge-receipts.md), #2370). It
> implements no manifest change, adds no route, and does not imply any bridge can
> import real rows today. A bridge tool is one
> [`ToolRuntimeMode`](../architecture/COOPERATIVE_TOOL_COMMONS.md) — *bridge
> adapter* — of the Tool Commons; these modes are capability declarations *within*
> such a manifest. The PR introducing this doc advances #2366 without closing it.

## 1. Purpose

[RFC-0017](../rfcs/RFC-0017-tool-install-infrastructure.md) already establishes
that a `ToolManifest` "declares capabilities, data touched, storage needs,
privacy classes, UI surfaces, compute jobs, schemas, receipts emitted," and that
a `ToolBinding` carries the institution-specific values that fill a generic
tool's slots. A governed bridge is a `ToolRuntimeMode: bridge adapter` tool. This
document models the specific safety **modes** such a bridge would declare, so
downstream `ToolManifest` / `ToolBinding` work binds to a consistent capability
vocabulary.

It answers:

> How does a future bridge tool declare source-shape reading, gated real-row
> reading, no-default-write behavior, dry-run output, steward review handoff,
> custody-target write permissions, receipt emission, and refusal policy?

This is **docs/spec planning only** — a declaration model, not an implementation.
It changes no manifest, defines no wire schema (the YAML in §4 is illustrative and
must reconcile with RFC-0017's own type sketches), and asserts nothing about
runtime enforcement mechanics, which RFC-0017 explicitly leaves to adjacent
sandboxing work.

## 2. Design doctrine

> A bridge tool must declare what it can do before it is allowed to do anything.
> No-default-write is a capability boundary, not a UI preference.
> A dry-run is evidence, not authority.
> The bridge proposes custody; the steward authorizes custody.

And the airlock's read boundary:

> A rehearsal reads source shape only.
> A real bridge reads real rows only after promotion gates hold.

The modes below are the declaration surface for that doctrine: the manifest names
which modes the tool supports; the binding (#2367) pins the actual sources,
scopes, and targets a run may touch; and the kernel/registry never has to
pattern-match on tool-specific keys (per `KERNEL_APP_SEPARATION.md`, cited by
RFC-0017).

## 3. Required modes

Each mode is a capability the manifest declares. **Status is `planned` — none
exists in RFC-0017 today; they are proposed capability declarations.** "Required
receipts" reference [`governed-bridge-receipts.md`](governed-bridge-receipts.md).

| Mode | What it permits | What it forbids | Required receipts | Required gates | Status |
| --- | --- | --- | --- | --- | --- |
| `source_shape_read` | Reading a source **schema/shape** | reading any real row | — (a shape read writes nothing) | none beyond install/binding | Planned |
| `real_row_read` | Reading actual source rows into a custody flow | reading rows unless promotion gates hold | contributes to `BridgeImportReceipt` provenance | **off unless** the §7 promotion gates in the requirements note hold | Planned |
| `dry_run_preview` | Producing a preview plan of proposed custody actions | any write; any authorization | emits `BridgeDryRunReceipt` | none (writes nothing) | Planned |
| `no_default_write` | Establishing that the default action is **none** | any write without a steward review decision | (guards writes; emits nothing itself) | fail-closed default | Planned |
| `steward_review_handoff` | Yielding to a human review gate before any write | proceeding on the tool's own say-so | emits `BridgeReviewDecisionReceipt` | a review surface exists (#2369) | Planned |
| `custody_write` | Writing to declared custody targets after review | writing to any un-declared target; writing an action card | `BridgeImportReceipt` **plus** target write receipts (`VaultObjectWriteReceipt` / `ArtifactRegistrationReceipt` / `FollowUpObjectCreationReceipt`) | review decision + cited dry-run | Planned |
| `external_reference_observe` | Observing an external status/reference | processing settlement; any payment path | emits `ExternalReferenceObservationReceipt` | none (observe-only) | Planned |
| `refusal_policy_enforcement` | Refusing disallowed classes by policy | importing credentials, payment instruments, unsupported classes, unbounded free text | emits `ConsentPolicyBlockReceipt` / `PublicationConsentBlockReceipt` / `DiscardDecisionReceipt` as appropriate | fail-closed | Planned |
| `export_delete_recovery` | Declaring that imports are reversible/recoverable | irreversible custody | (declares the export/delete path referenced by `BridgeImportReceipt`) | export/delete path exists for each target | Planned |

## 4. Manifest declaration shape

**Illustrative and non-normative.** The sketch below shows the *shape* of a
declaration, not a committed schema; final field names and structure must
reconcile with RFC-0017's own (equally illustrative) `CapabilityDeclaration` /
`ReceiptClassRef` type sketches.

```yaml
# ILLUSTRATIVE ONLY — not a committed schema; reconcile with RFC-0017.
bridge_modes:
  source_shape_read:
    enabled: true
    row_access: false
  real_row_read:
    enabled: false
    requires_promotion_gates: true
  dry_run_preview:
    emits:
      - BridgeDryRunReceipt
  no_default_write:
    default_action: none
    writes_require:
      - BridgeReviewDecisionReceipt
      - BridgeImportReceipt
  custody_write:
    allowed_targets:            # capability only; the binding pins the actual values
      scoped_vault: []
      artifact_registry: []
      governed_object: []
    emits_target_receipts: true
  external_reference_observe:
    process_settlement: false
  refusal_policy:
    refuse:
      - credentials
      - payment_instruments
      - unsupported_classes
      - unbounded_free_text
  export_delete_recovery:
    reversible: true
```

The empty `allowed_targets` lists are deliberate: the *manifest* declares the tool
*can* write to those target kinds; the *binding* (#2367) fills in the specific
scopes / namespaces / object classes a given run may use.

## 5. Evidence contract

The modes bind to the receipt classes in
[`governed-bridge-receipts.md`](governed-bridge-receipts.md). A run's declared
modes imply a required receipt set; a run with a missing expected receipt is
**incomplete**:

- a **dry-run-only** run (`dry_run_preview`, no `custody_write`) emits only a
  `BridgeDryRunReceipt` — evidence, never authority;
- a **real import** must cite the dry-run preview (`BridgeDryRunReceipt` id /
  preview-plan hash) *and* the `BridgeReviewDecisionReceipt` it rests on, per the
  `dry-run → review → import` chain;
- a **write** run emits `BridgeImportReceipt` **plus** the target write receipts
  it coordinated;
- **policy refusals** emit policy-block / discard receipts (`ConsentPolicyBlockReceipt`,
  `PublicationConsentBlockReceipt`, `DiscardDecisionReceipt`) as appropriate — an
  automatic block never carries a review-decision receipt;
- **missing expected receipts means the run is incomplete** — the binding's
  required receipt set is the checklist, and absence is a failure, not a silent
  pass.

## 6. Steward handoff

For `steward_review_handoff`, the manifest must declare, at minimum:

- the **review surface target** — likely the Steward Cockpit
  ([`steward-cockpit-v0.md`](steward-cockpit-v0.md));
- the **role-display vs authority-proof boundary** — the receipt binds a
  *verifiable* reviewer authority reference; a role label is for display only and
  never the sole evidence (per `BridgeReviewDecisionReceipt`);
- that **no write happens without a review decision** (`no_default_write` +
  `steward_review_handoff` together);
- that the **review decision set binds to source-record refs + field paths (or a
  dry-run plan hash)** committing to the exact reviewed set — never a bare
  field-name set (per the requirements note's per-`(record, field)` coverage);
- that **member-facing consent review is a separate, future surface**
  ([`member-shell-v0.md`](member-shell-v0.md)), privacy-bounded so it never
  exposes private organizer data.

## 7. Custody target permissions

The manifest declares *capabilities*; the binding pins *actual targets*:

- the **manifest** says the tool supports writing to `ScopedVault`,
  `ArtifactRegistry`, and underlying governed objects (see
  [`artifact-registry-and-scoped-vault.md`](artifact-registry-and-scoped-vault.md));
- the **`GovernedServiceBinding`** (#2367, which generalizes RFC-0017's
  `ToolBinding` — see [`governed-service-binding.md`](governed-service-binding.md))
  decides which specific scopes / namespaces / object classes are allowed for a
  run;
- **action cards are derived read views and never write targets** (ADR-0027); a
  consent-gated follow-up is realized by writing the underlying governed object,
  from which the card is derived.

This split keeps the manifest generic and the institution-specific values in the
binding, per RFC-0017's `INSTITUTION_PACKAGE_BOUNDARY` rule.

## 8. Refusal policy

`refusal_policy_enforcement` must **fail closed** for at least these categories,
and may emit policy-block / discard evidence where a bounded decision was made:

- credentials;
- payment instruments;
- unsupported classes;
- unbounded free text;
- private data in any repo-safe output;
- raw external payload in a receipt body;
- raw PII-bearing source keys (references must be opaque/hash-bound or
  vault-backed).

Refusal is the safe action: a refused class is never imported, and the refusal is
itself evidence (a policy-block or discard receipt), not a silent drop.

## 9. Relationship to #2367

This document models **`ToolManifest`-level capability declarations** — what a
bridge tool *can* do. It deliberately stops at the manifest boundary. Issue
**#2367** must model the **run / institution-specific binding** in
`GovernedServiceBinding`:

- allowed source systems;
- allowed scopes;
- allowed namespaces;
- per-field custody mapping;
- receipt sink;
- export / delete / recovery path.

The manifest says "this tool supports these modes and target kinds"; the binding
says "for *this* run, these exact sources, scopes, and targets are permitted." **#2367
stays open — it is referenced here, not addressed by this PR.**

## 10. Non-goals / non-claims

- no runtime implementation; no new API; no OpenAPI change; no migration;
- no production, pilot-readiness, or live-federation claim; no deployed bridge
  behavior;
- no raw Drive import; no live sync;
- no private data;
- no payment-processing / wallet / token / cryptocurrency framing (external
  settlement is *observed*, never processed);
- no claim that current NYCN operations are ICN-native.

## 11. Open questions

1. Should modes be first-class fields in `ToolManifest`, in `ToolBinding`, or
   split across both (capability in the manifest, enablement in the binding)?
2. How should `no_default_write` be **machine-validated** — a manifest lint, a
   binding-time check, or a runtime guard (or all three)?
3. How should the manifest express the **receipt classes it can emit** — reusing
   RFC-0017's `ReceiptClassRef` sketch, or a bridge-specific extension?
4. How should **dry-run plan hashes** be bound to the later review / import
   receipts so the chain is verifiable?
5. How should **refusal policy** become *testable* (a fixture-driven conformance
   check, akin to the NYCN airlock fixture validator)?
6. How much of this belongs in **RFC-0017** itself vs. a separate *bridge
   profile* layered on top of the generic manifest?

---

_Provenance: derived from the NYCN airlock lane (NYCN #84–#89), the ICN
requirements note (#2364), and the receipt vocabulary (#2370), tracking ICN issue
#2366. A capability-declaration model, not an implementation or deployment claim._
