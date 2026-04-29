---
Status: design-direction
Authority: architecture (forward-direction; not yet normative)
Canonical: no
Last Reviewed: 2026-04-27
---

# Model Workloads and Deliberation

> **Status: draft / design direction / roadmap.** Names a future class of compute workloads inside ICN — model-driven advisory work that supports deliberation. Items marked _planned_ are not in the repo today. Companion to [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) and [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md).

## Summary

ICN treats compute as **constrained execution with workload manifests, fuel/resource limits, privacy/determinism classes, scope, resource profile, and optional mandate reference** ([ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md), [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)). Models are one kind of compute workload.

> **Compute executes. Compute does not decide.**

> **Models can assist institutions. They cannot govern institutions.**

This document records what models are *for* in ICN, what they are explicitly *not for*, and how their outputs become institutional state — through governance review, never automatically.

## What models are for

Allowed model uses (advisory, source-linked, receipted):

- summarize meeting notes
- transcribe meetings
- translate materials
- create source-linked proposal briefs
- map arguments in deliberation
- summarize prior decisions and receipts
- extract proposed action items
- classify survey or evaluation themes
- draft reports and public copy
- prepare grant or sponsor drafts
- search archives (permission-aware embeddings)
- create embeddings for permission-aware search
- detect missing context or unresolved questions

In every case the model produces a **draft, suggestion, summary, classification, or artifact** — and that artifact remains advisory until an authorized human or governance act takes it up.

## What models are not for

Forbidden model uses:

- **decide votes** — a vote is a member act
- **approve budgets** — allocation is a governance act
- **admit or reject members** — a membership decision is a governance act
- **assign authority** — `authority_scope` is constitutional, not advisory
- **resolve disputes** — disputes go through institutional process, not model output
- **rank member worth** — the institution does not have a worth ranking
- **profile political reliability** — out of scope; not a thing ICN supports
- **auto-publish restricted data** — publication requires authorization and a publication receipt
- **auto-settle obligations** — settlement is a governance act with a settlement receipt
- **auto-sanction people** — sanctions are governance acts with appeal paths
- **make binding governance decisions** of any kind

Phrasing rule: a model **may** produce a draft; an **authorized institution** turns that output into institutional state.

## Model-assisted deliberation

The most valuable model use in cooperative infrastructure is **getting everyone on the same page**. A model can produce a neutral, source-linked deliberation packet:

- what are we deciding?
- why are we deciding it?
- what has already been said?
- what prior decisions and receipts matter?
- what are the options?
- what are the tradeoffs?
- what objections exist?
- what questions are unresolved?
- who is affected?
- what needs to happen next?

The packet is a **draft / advisory artifact**. It enters institutional state only after an authorized review.

This is not a robot deliberator. It is closer to a research librarian: it cites, it organizes, it does not vote.

## Workload shape

A model workload is a compute workload (per ADR-0030/0031) with a few extra pieces:

| Field | Purpose |
|---|---|
| `model_artifact_hash` | Content-addressed reference to the model artifact in a future model registry |
| `model_version` | Version metadata |
| `prompt_template_hash` | Hash of the prompt or generation template, so the workload is reproducible |
| `input_artifact_hashes` | Content-addressed references to inputs (meeting notes, prior receipts, source documents) |
| `output_artifact_hashes` | Content-addressed references to outputs (draft summary, packet, transcript, classification) |
| `privacy_class` | Inherits from compute substrate; bounds where the workload may run |
| `determinism_class` | `advisory` (model output) vs `deterministic` (e.g. canonical hashing). Advisory is the default for model workloads. |
| `runner_did` | The service identity that executed the workload |
| `scope` | Scopes the workload to a domain / structure / activity |
| `output_receipt` | Receipt produced by the workload, advisory-classed |
| `external_model_bridge_policy` | Whether and how the workload may call out to an external model service |

These are **planned**. Today the substrate has the workload manifest, fuel/resource limits, privacy/determinism classes, scope, resource profile, and optional mandate reference (ADR-0030). The model-specific fields are future additions.

## Model registry (planned)

A `ModelRegistry` records:

- model artifact hash
- model version
- provenance (where it came from, who packaged it)
- license / use restrictions
- privacy class compatibility
- known evaluation results
- federation visibility (per-institution, federation-shared, public-commons)

Without a registry, advisory workloads cannot pin a reproducible model — and a workload that cannot be re-run is not auditable.

## External model bridges

Some institutions will want to call external model services. The bridge is a **service identity** with explicit policy:

- which institutional domain may use it
- what privacy classes may flow to it
- what request hashes are recorded
- what response hashes are recorded
- whether and how outputs return as artifacts
- a `BridgeImportReceipt`-style record per call

External model bridges follow the **anti-capture rule**: institutional state never lives at the external service. Inputs and outputs flow back as artifacts under the institution's own `ArtifactRegistry`.

## Receipts and audit

Every advisory workload produces an `output_receipt` carrying:

- workload identity (manifest hash + runner DID)
- input artifact hashes
- output artifact hashes
- prompt / template hash
- determinism class (advisory)
- timestamp
- scope

This is the audit trail that lets an institution later answer "what model produced this draft, against what inputs, on what date?" — without that answer, the draft cannot meaningfully be reviewed or trusted.

## Deliberation pattern (end-to-end, planned)

```
member opens action card             ──► /me/action-cards (existing)
"deliberate proposal X"
       ↓
governance tool requests deliberation packet
       ↓
icn-compute-jobs runs a model workload
   inputs: proposal text, prior decisions/receipts, comments
   determinism class: advisory
       ↓
output artifact: source-linked deliberation packet
       ↓
output receipt persisted in receipt envelope
       ↓
members read packet, comment, propose amendments
       ↓
governance vote (existing: GovernanceDecisionReceipt path)
       ↓
decision becomes institutional state
       ↓
action items emitted (existing: decision-to-action bridge, PR #1532)
```

Model output never short-circuits this loop. Every transition that becomes institutional state is a governance act with a receipt.

## Boundary discipline

- An advisory artifact becomes institutional state **only** after an authorized human or governance act takes it up. The act is what transitions; the artifact is what informed the act.
- Models do not see vault data they were not explicitly scoped to. Permission-aware embeddings honor scope.
- Model output never bypasses the receipt envelope. A model-produced public draft becomes a public artifact only after a `PublicationReceipt`.
- A model that cannot be reproduced (missing artifact hash, missing prompt hash) cannot be cited as a source for institutional state.

## Existing repo support

- Compute substrate: workload manifest, fuel/resource limits, privacy/determinism classes, scope, resource profile, optional mandate reference ([ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md))
- Commons compute admission and settlement policy ([ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md))
- Receipt envelope including the artifact-receipt layer ([ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md))

## Missing buildout

- `ModelArtifact` registry with content-addressed hashing
- explicit `advisory` vs `deterministic` workload classification surface
- prompt-template hashing convention
- output-receipt class extension carrying model-specific fields
- external model bridge service-identity pattern + per-call receipt
- deliberation-packet template (the source-linked summary shape)

## Non-goals

- **No** automated decision-making on member, governance, allocation, or sanction questions.
- **No** model-as-arbiter framing.
- **No** runtime implementation in this PR.
- **No** specific model vendor or hosting choice — design is bridge-friendly to multiple model runtimes.
- **No** institution-specific deliberation rules in ICN core. Institution packages can constrain *which* model classes their domain allows, not the substrate's shape.

## References

- [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) — parent design
- [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md) — companion: tools
- [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md) — companion: routing
- [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md), [ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md), [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)
