---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-05
---

# Preview / review — contract notes

## Files

| File | Purpose |
|------|---------|
| [`preview-review.schema.json`](preview-review.schema.json) | JSON Schema (draft 2020-12) for one repo-safe preview/review packet. |
| [`preview-review.example.json`](preview-review.example.json) | Fictional sample packet. Validates against the schema. Demonstrates a meeting-notes-to-action-items preview. |
| [`../scripts/validate-preview-review.py`](../scripts/validate-preview-review.py) | Tiny validator — given a packet path, checks it against the schema. Used by CI / contributors before committing example or partner-side packets that will live in a public repository. |

## Purpose

This schema defines the shape of the **human review boundary** between source material (meeting notes, fixture material, package output, evidence-packet draft) and any subsequent action (publish, apply, export, mutate). It is the missing read-model contract between:

- the no-CLI organizer/member rehearsal workflow ([`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md), §3 steps 3 + 6, §7)
- the evidence export contract ([`docs/contracts/rehearsal-evidence-export.md`](rehearsal-evidence-export.md), `urn:icn:contract:rehearsal-evidence-export:v1`)
- the surface-side organizer/member accessibility gate ([`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md))
- future shell, wireframe, fixture-demo, and private-overlay activation work

The contract closes the "Generic preview/review API contract" follow-up bullet in §7 of the no-CLI workflow doc.

## What this contract is — and is not

This is a **read-only / read-model contract**. The schema describes what a reviewer was shown and what they decided. It does **not**:

- mutate, publish, export, or apply anything
- implement publishing, application, or any HTTP endpoint
- create a production API surface
- authorize a formal NYCN pilot
- permit private partner data in public git

The `review_status` value `approved_for_next_step` records that a human review boundary was crossed. It does **not** mean production deployment, formal pilot authorization, or any actual mutation. Mutation is performed by separate, named, documented endpoints; this contract only describes what a reviewer was shown and what they decided.

## Identity — non-DNS contract identifier

The schema `$id` is a non-DNS URN:

```
urn:icn:contract:preview-review:v1
```

Same rationale as [`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md) (Identity section) and [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) (convenience-vs-authority half): contract identity for ICN must not depend on centralized DNS, ICANN registrars, registrar renewal, or capitalist hosting/payment structures. Schema `$id` values are machine-facing identifiers — but "machine-facing" does not mean "DNS should be authoritative." HTTP URIs quietly make contract identity depend on a single domain registration and a single hosting decision. ICN should not put its contract identity behind a centralized naming choke point.

The companion field `x-icn-contract-name` (`icn.contract.preview_review.v1`) gives a human-readable handle for the same identity. The companion field `x-icn-distribution-hints` lists repo-relative paths to copies; **distribution hints are not authority**.

DNS-backed URLs (`intercooperative.network`, `icn.zone`, future mirrors) may still be used as discovery, routing, mirror, or human-facing surfaces for these documents. They must not become the canonical identifier.

Verifiability of a packet's claim to conform to this contract comes from the URN identity, the repository path that holds this schema (`docs/contracts/preview-review.schema.json`), the git history of that path, and (future) receipt and provenance mechanisms when those land.

## How this fits the no-CLI organizer/member workflow

The no-CLI workflow at `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md` describes the human path through a rehearsal. §3 steps 3 and 6 are the explicit **preview** moments — "Preview parsed proposals" and "Preview before mutation." Until this contract landed, the schema for those previews was implicit: every package invented its own shape. This contract names the substrate-level shape every package's preview surface binds to.

Concretely:

- §3 step 3 ("Preview parsed proposals") produces a packet with `preview_kind: meeting_notes_action_items` (or one of the other kinds).
- §3 step 6 ("Preview before mutation") produces a packet with `preview_kind: pending_publish_summary`.
- §3 step 9 ("Export evidence packet") consumes a packet with `preview_kind: evidence_packet_preview` before producing the upstream `urn:icn:contract:rehearsal-evidence-export:v1` artifact.

The contract does not run any of those steps; it just describes the read-model the human review surface renders.

## How this fits the organizer/member accessibility gate

[`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) §3.7 (cognitive load and step complexity), §3.11 (receipts/provenance/evidence access), and §3.12 (governance/action access) all apply to any UI that renders this contract. A surface that renders a `PreviewReview` packet is in scope for the gate; the gate's PR-time checklist runs against any PR that adds or changes such a surface.

The contract's `review_requirements` array lets the producing system declare which categories the reviewer should have considered (privacy, accessibility, repo safety, scope, second-reviewer signoff, fixture-only check, no-mutation check). The gate doc enumerates what each of those checks looks like in practice.

## Stability

The schema sets `"x-icn-status": "rfc"`. Treat the shape as **versioned contract surface**, not frozen public API. The version is encoded in the URN (`...:v1`); the next breaking shape requires a new URN (`...:v2`) and an explicit migration note per `docs/contracts/schema-id-audit.md` §5 migration safety rules. Non-breaking additions (new optional fields, new enum members at the end of a closed taxonomy) may land at the same URN, with a `schema_version` bump in the packet field.

Producers should pin to the URN, not to a file path or a DNS URL.

## Field shape (at-a-glance)

The schema is the source of truth; this section is a summary. See `preview-review.schema.json` for required-vs-optional, types, and closed-enum membership.

| Field | Closed taxonomy? | Notes |
|-------|------------------|-------|
| `schema_version` | — | Semver-style; bumped per shape change at this URN. |
| `preview_id` | — | Stable, locally-unique opaque identifier; no private data encoded. |
| `preview_kind` | yes | `meeting_notes_action_items` / `pending_publish_summary` / `evidence_packet_preview` / `fixture_demo_preview` / `package_artifact_preview`. |
| `created_at` | — | RFC 3339 timestamp; the moment the preview was finalized for review. |
| `created_by_role` | yes | `organizer` / `steward` / `facilitator` / `reviewer` / `system_fixture`. Generic role labels only. |
| `scope` | yes | `entity` / `structure` / `individual` (mirrors the action-card scope axis). |
| `source_material[]` | partial | `kind` is closed (`committed-fixture` / `example-snippet` / `repo-safe-paste`); basename + summary are free text. **No raw external paths.** |
| `proposed_artifact` | partial | `kind` closed (`action_item_set` / `publish_summary` / `evidence_packet` / `fixture_demo_artifact` / `package_artifact`); title + summary + optional detail are plain language. |
| `review_status` | yes | `draft` / `ready_for_review` / `changes_requested` / `approved_for_next_step` / `rejected`. **`approved_for_next_step` is not production deployment.** |
| `review_requirements[]` | yes | Categories the reviewer should have considered. |
| `reviewer_actions[]` | partial | Optional review-trail. `action` and `by_role` closed; `recorded_at` RFC 3339; optional plain-language `notes`. **Not authoritative for governance.** |
| `privacy_review` | partial | Generic role label + closed status enum. |
| `repo_safety` | partial | `classification` closed (`repo-safe` / `partner-internal-only`); the latter must never appear in this repo. |
| `evidence_links[]` | partial | `kind` closed; URLs only to public resources; optional `contract_urn` for related contracts. |
| `non_claims[]` | — | Free text; must include the standing four (no mutation, no production deployment, no formal pilot, no private-data handling). |

## What this schema MUST NOT carry

The schema declares `additionalProperties: false` so unknown fields are rejected at validation time. The following content is **forbidden** by the schema's design and the project's privacy posture, regardless of whether a field could technically hold it:

- real personal names (unless explicitly fictional / example-only)
- real email addresses
- real phone numbers
- sponsor contact details
- accommodation or access needs tied to an identifiable person
- raw attendee or member rolls
- API tokens, secrets, signed URLs, or session credentials
- private Drive / Groups / Sheets paths or content
- private overlay contents
- live partner operational details
- any field that implies production deployment or formal pilot authorization
- DIDs of unconsented participants
- free-form mutation payloads (this is a read-model contract; mutation is performed by separate documented endpoints)

This list is also captured in the schema's `x-icn-must-not-include` extension so machine consumers (linters, partner CI) can read it without parsing this prose.

## Validation guidance for package repos

1. Validate every preview packet that will live in a **public repository** against `preview-review.schema.json` before committing. Use the bundled validator (`docs/scripts/validate-preview-review.py`) or any draft-2020-12 JSON Schema validator. Pin to the URN.
2. Do **not** add partner-specific nouns to this schema. Bind local meaning in package docs.
3. **Partner-internal previews may use the same schema shape and may validate against this contract — but they must set `repo_safety.classification = partner-internal-only` and must not be committed to the ICN repository.** Only previews with `repo_safety.classification = repo-safe` belong in public ICN git. The schema's `partner-internal-only` enum value is a contractual marker that the producing system itself decided NOT to render through the repo-safe path; partner-internal data still belongs in private overlays or partner-private storage, never in this tree.
4. Prefer regulatory-safe vocabulary in human-language fields: **obligation**, **allocation**, **settlement**, **unit**, **position**, **receipt**, **provenance**, **evidence** — not payment / wallet / balance / currency framing for substrate flows.
5. Keep the standing non-claims in every preview. Producers may **add** preview-specific non-claims; producers MUST NOT **remove** the standing four.

## Producing a preview packet

A producing system (a parser, a fixture loader, a publish-staging tool, an evidence-packet drafter) walks its inputs, identifies the proposed artifact, and packages a preview. The reviewer (organizer, steward, facilitator) opens it, decides, and the producing system records the outcome:

1. Set `preview_kind` to the appropriate closed-enum value.
2. Set `created_by_role` to the generic role of the producing actor — never a name, email, or DID.
3. Populate `source_material` by safe basename and category. Do not paste raw cloud-drive paths or private content.
4. Populate `proposed_artifact` with the artifact kind, plain-language title, and summary. Optional `detail` is plain language only; structured payloads belong in per-kind schemas.
5. Set `review_status` initially to `draft` or `ready_for_review`. Update as the reviewer acts.
6. Declare `review_requirements` so the reviewer knows what categories to consider.
7. Run the privacy and accessibility checks. Set `privacy_review.status` accordingly.
8. Set `repo_safety.classification` to `repo-safe` for any preview that will be checked into ICN.
9. Optionally append `reviewer_actions` entries as the reviewer acts.
10. Validate the packet against the schema before committing or rendering it in a public surface.

## Out of scope

- **Mutation API** for emitting preview packets or for applying approved previews. The contract is read-only.
- **Per-artifact-kind schemas.** This contract describes the review-boundary shape; the structured shape of each artifact (action-item-set internals, publish-summary internals) belongs in per-kind schemas if and when those land.
- **Partner overlay format.** Partner-internal previews live in private overlays, outside this schema.
- **Federation export protocol.**
- **UI rendering of preview packets** (organizer / member shell work tracked under [icn#1726](https://github.com/InterCooperative-Network/icn/issues/1726) / [icn#1727](https://github.com/InterCooperative-Network/icn/issues/1727)).
- **Migration of existing contract schemas** (e.g. `action-card.schema.json`) — that is a deliberate follow-up evaluation tracked under [icn#1742](https://github.com/InterCooperative-Network/icn/issues/1742), not part of this PR.

## See also

- [`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md) — repo-safe rehearsal evidence export contract; `evidence_packet_preview` previews are the human review boundary that precedes producing one of those packets
- [`./schema-id-audit.md`](schema-id-audit.md) — audit of every contract `$id` in the repo (this schema is now in §3 of that audit, recommendation: keep)
- [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) — convenience-vs-authority and participation-access checklist (the upstream architecture-review reflex this contract is a deliberate application of)
- [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — surface-side PR-time review gate that runs against any UI rendering this contract
- [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) — generic ICN no-CLI organizer / member rehearsal workflow; §3 steps 3 + 6 + 9 are the preview moments this contract names
- [icn#1728](https://github.com/InterCooperative-Network/icn/issues/1728) — issue this doc closes
- [icn#1724](https://github.com/InterCooperative-Network/icn/issues/1724) — umbrella issue for the no-CLI organizer / member rehearsal UX
- [icn#1726](https://github.com/InterCooperative-Network/icn/issues/1726) / [icn#1727](https://github.com/InterCooperative-Network/icn/issues/1727) — open follow-ups: organizer rehearsal shell + fixture-backed demo mode (separate; this contract is the read-model they will render)
- [icn#1730](https://github.com/InterCooperative-Network/icn/issues/1730) — open follow-up: private-overlay / DID-binding activation flow (separate; the contract's `assignee label not DID` discipline references this work)
- [icn#1740](https://github.com/InterCooperative-Network/icn/issues/1740) — open follow-up: website multilingual / inclusive-access planning (separate; this contract is for organizer/member surfaces, not the public website)
- [icn#1742](https://github.com/InterCooperative-Network/icn/issues/1742) — open follow-up: ActionCard `$id` 2026-06-30 compatibility review (separate; identity-track follow-up)
