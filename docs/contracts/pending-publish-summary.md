---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-09
---

# Pending-publish summary — contract notes

## Files

| File | Purpose |
|------|---------|
| [`pending-publish-summary.schema.json`](pending-publish-summary.schema.json) | JSON Schema (draft 2020-12) for one repo-safe pending-publish summary packet (a set of review rows). |
| [`pending-publish-summary.example.json`](pending-publish-summary.example.json) | Fictional sample packet with action-item, decision, attendance, and allocation rows. Validates against the schema. |
| [`../scripts/validate-preview-review.py`](../scripts/validate-preview-review.py) | Shared draft-2020-12 validator. It accepts `--schema`, so it validates this contract too: `python3 docs/scripts/validate-preview-review.py --schema docs/contracts/pending-publish-summary.schema.json docs/contracts/pending-publish-summary.example.json`. |

## Why this contract exists after `preview-review`

[`preview-review.schema.json`](preview-review.schema.json) (`urn:icn:contract:preview-review:v1`, landed in icn#1745) defines the **review boundary** — one preview, one `review_status`, one privacy posture, one `proposed_artifact`. It names `pending_publish_summary` as one of its `preview_kind` values but does **not** specify the shape of the individual rows inside such a summary.

This contract fills exactly that gap: it is the **row-level read-model** an organizer reviews one item at a time before any documented next step — the data the rehearsal shell needs at the no-CLI workflow's §3 step 3 ("Preview parsed proposals") and step 6 ("Preview before mutation"). Until this contract landed, each package invented its own row shape; this names the substrate-level row shape every package's pending-publish surface binds to.

## How it composes with `preview-review`

```
urn:icn:contract:preview-review:v1            (the review boundary / wrapper)
   preview_kind = "pending_publish_summary"
        └── body specified by ─────────────►  urn:icn:contract:pending-publish-summary:v1
                                                  preview_rows[]  (this contract)
```

- The **wrapper** (`preview-review`) answers: *was this summary reviewed, by what generic role, under what privacy posture, before the next step?*
- The **body** (this contract) answers: *what are the individual pending-publish rows the organizer is reviewing — their kind, plain summary, governing body, scope, provenance, status, and what each would create if approved?*

This contract **does not replace** `preview-review`. The schema records the composition in machine-readable form via the `x-icn-composes-with` companion field (`urn:icn:contract:preview-review:v1`). A producing system renders a `pending-publish-summary` packet **inside** a `preview-review` packet whose `preview_kind` is `pending_publish_summary`.

## What this contract is — and is not

This is a **read-only / read-model contract**. It describes the rows an organizer was shown. It does **not**:

- mutate, publish, apply, create, or export anything;
- implement any HTTP endpoint or production API surface;
- authorize a formal NYCN pilot;
- permit private partner data in public git.

Three fields make the read-only boundary explicit:

- **`review_actions`** are *affordances* — the choices the organizer could make (`approve` / `reject` / `edit` / `request_info` / `defer`). They are not commands and not a record of actions taken. Rendering them performs nothing.
- **`mutation_preview`** *describes* what a separate, named, documented endpoint would create later if the row were approved (`would_create` + a plain-language `summary`). It carries no executable payload (`additionalProperties: false`).
- **`receipt_expected`** describes the expected receipt/evidence *category* (e.g. `governance_receipt`, `attendance_receipt`), never live receipt data.

The per-row `status` value `approved_for_publish` records that a human review boundary was crossed **for that row**. It does **not** mean production deployment, formal pilot authorization, or any actual mutation. Mutation is performed by separate, named, documented endpoints; this contract only describes what a reviewer was shown and what they decided.

## Identity — non-DNS contract identifier

The schema `$id` is a non-DNS URN:

```
urn:icn:contract:pending-publish-summary:v1
```

Same rationale as [`./preview-review.md`](preview-review.md) and [`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md) (Identity sections) and [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md): contract identity for ICN must not depend on centralized DNS, ICANN registrars, registrar renewal, or capitalist hosting/payment structures. The companion field `x-icn-contract-name` (`icn.contract.pending_publish_summary.v1`) gives a human-readable handle; `x-icn-distribution-hints` lists repo-relative copies — **distribution hints are not authority**. Producers should pin to the URN, not a file path or DNS URL. The packet echoes the URN in its required `contract_id` field (a `const`).

## How this fits the no-CLI organizer/member workflow

The no-CLI workflow at [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) describes the human path through a rehearsal. §3 steps 3 and 6 are the explicit **preview** moments. A `preview-review` packet with `preview_kind: pending_publish_summary` wraps the review; the rows inside it conform to this contract. The contract runs none of those steps; it just names the read-model the human review surface renders.

A surface that renders these rows is in scope for [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) (§3.7 cognitive load, §3.11 receipts/provenance/evidence access, §3.12 governance/action access). Each row carries an `accessibility_hint` (ADR-0028 discipline, mirrored from action cards) so the gate has something concrete to check.

## Relationship to action cards, Member standing, receipts, and provenance

- **Action cards** ([`institution-package/action-card.schema.json`](institution-package/action-card.schema.json)) are per-member *derived views* of what a holder can do now. Pending-publish rows are *organizer-facing previews* of what would be created. They share vocabulary deliberately: `authority_basis`, `risk_level` (`low`/`normal`/`elevated`), `accessibility_hint`, and `scope`-style labels mirror the action-card axes so a shell can render both with one design language. Pending-publish `receipt_expected` is an object (`expected` + `category`) rather than the action-card boolean, because a preview describes an *expected category*, not a live outcome.
- **Member standing** is the framing context (memberships, roles, scopes) above any action queue. This contract uses generic `governing_body_label` / `assignee_label` / `target_scope_label` strings, never DIDs — holder-label to DID binding is the separate private-overlay / DID activation flow ([icn#1730](https://github.com/InterCooperative-Network/icn/issues/1730)).
- **Receipts** are described by category only (`receipt_expected`), never carried inline.
- **Provenance** is `source_provenance_ref`: a category plus a safe reference label, optionally a related `contract_urn` (e.g. `urn:icn:contract:rehearsal-evidence-export:v1`). Never a private path or URL.

## Stability

The schema sets `"x-icn-status": "rfc"`. Treat the shape as **versioned contract surface**, not frozen public API. The version is encoded in the URN (`...:v1`); the next breaking shape requires a new URN (`...:v2`) and an explicit migration note per [`schema-id-audit.md`](schema-id-audit.md) §5. Non-breaking additions (new optional fields, new enum members at the end of a closed taxonomy) may land at the same URN with a `schema_version` bump in the packet.

## Field shape (at-a-glance)

The schema is the source of truth; this is a summary.

| Field | Closed taxonomy? | Notes |
|-------|------------------|-------|
| `schema_version` | — | Semver-style; bumped per shape change at this URN. |
| `contract_id` | const | Must equal `urn:icn:contract:pending-publish-summary:v1`. |
| `generated_at` | — | RFC 3339 timestamp. |
| `source_package` | partial | Generic `label` + closed `category`. No partner nouns. |
| `source_material[]` | partial | `kind` closed (`committed-fixture`/`example-snippet`/`repo-safe-paste`); basename + summary free text. **No raw external paths.** |
| `preview_rows[]` | — | One or more rows (see below). |
| `privacy_review` | partial | Generic role label + closed status enum. |
| `non_claims[]` | — | Free text; must include the standing four. |

Each `preview_row`:

| Row field | Closed taxonomy? | Notes |
|-----------|------------------|-------|
| `id` | — | Opaque, locally-unique; no private data encoded. |
| `kind` | yes | `action_item`/`decision`/`attendance`/`obligation`/`allocation`/`settlement`/`evidence_note`/`risk_note`. Economic-adjacent kinds are regulatory-safe vocabulary only. |
| `plain_summary` | — | Plain language; regulatory-safe vocabulary for economic-adjacent rows. |
| `status` | yes | `pending_review`/`approved_for_publish`/`rejected`/`needs_edit`/`needs_more_info`. **`approved_for_publish` is not a mutation.** |
| `target_scope_label` | — | Generic label; not a DID. |
| `governing_body_label` | — | Generic label; not a DID or roster. |
| `assignee_label` | — | Optional generic label; **never a DID**. |
| `source_provenance_ref` | partial | `category` closed + safe `ref_label`; optional related `contract_urn`. **No private paths.** |
| `authority_basis` | — | Plain language (mirrors action-card). |
| `risk_level` | yes | `low`/`normal`/`elevated`. |
| `accessibility_hint` | — | Required on every row (ADR-0028 discipline). |
| `review_actions[]` | yes | `approve`/`reject`/`edit`/`request_info`/`defer`. **Affordances, not commands.** |
| `mutation_preview` | partial | `would_create` closed + plain `summary`. **Description, not a command; no payload.** |
| `receipt_expected` | partial | `expected` bool + closed `category`. **Expected category, not live data.** |
| `privacy_notes` | — | Optional plain-language note; no private fields. |

## What this schema MUST NOT carry

The schema declares `additionalProperties: false` at the root and every nested object, so unknown fields are rejected at validation time. The following content is **forbidden** by the schema's design and the project's privacy posture, regardless of whether a field could technically hold it:

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

1. Validate every pending-publish summary that will live in a **public repository** against `pending-publish-summary.schema.json` before committing. Use the shared validator (`docs/scripts/validate-preview-review.py --schema docs/contracts/pending-publish-summary.schema.json <packet>`) or any draft-2020-12 JSON Schema validator. Pin to the URN.
2. Do **not** add partner-specific nouns to this schema. Bind local meaning in package docs.
3. **Partner-internal summaries may use the same schema shape but must not be committed to the ICN repository.** Partner-internal data belongs in private overlays or partner-private storage, never in this tree.
4. Prefer regulatory-safe vocabulary in human-language fields: **obligation**, **allocation**, **settlement**, **unit**, **position**, **receipt**, **provenance**, **evidence** — not payment / wallet / balance / currency framing for substrate flows.
5. Keep the standing non-claims in every summary. Producers may **add** summary-specific non-claims; producers MUST NOT **remove** the standing four.

## Producing a summary packet

A producing system (a parser, a fixture loader, a publish-staging tool) walks its inputs, identifies each proposed item, and packages one row per item:

1. Set `contract_id` to the URN and `schema_version` to the shape version you authored against.
2. Set `source_package.label`/`category` to generic values — never a partner name.
3. Populate `source_material` by safe basename and category. No raw cloud-drive paths or private content.
4. For each item, build a `preview_row`: pick the closed-enum `kind`, write a plain-language `plain_summary`, set `status` (usually `pending_review`), and set the generic `target_scope_label` / `governing_body_label` (and optional `assignee_label` — never a DID).
5. Set `source_provenance_ref` to a category + safe `ref_label`. Set `authority_basis`, `risk_level`, and a per-row `accessibility_hint`.
6. List the `review_actions` the organizer may take. Fill `mutation_preview` with what *would* be created (no payload) and `receipt_expected` with the expected category.
7. Run the privacy check; set `privacy_review.status`. Add `privacy_notes` per row where helpful.
8. Validate the packet against the schema before committing or rendering it in a public surface.
9. Render the packet inside a `preview-review` packet whose `preview_kind` is `pending_publish_summary`.

## Out of scope

- **Mutation API** for emitting these summaries or for applying approved rows. The contract is read-only.
- **The review boundary itself** — that is [`preview-review.md`](preview-review.md) (`urn:icn:contract:preview-review:v1`). This contract is only the row-level body.
- **Holder-label to DID binding** — the private-overlay / DID activation flow ([icn#1730](https://github.com/InterCooperative-Network/icn/issues/1730)).
- **Partner overlay format.** Partner-internal summaries live in private overlays, outside this schema.
- **UI rendering of these rows** — organizer/member shell work tracked under [icn#1726](https://github.com/InterCooperative-Network/icn/issues/1726) / [icn#1727](https://github.com/InterCooperative-Network/icn/issues/1727).

## See also

- [`./preview-review.md`](preview-review.md) — the review-boundary wrapper this contract composes with; `preview_kind: pending_publish_summary` is the kind whose body this contract specifies
- [`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md) — repo-safe rehearsal evidence export contract; `source_provenance_ref.contract_urn` may point at one of those packets
- [`./institution-package/action-card.schema.json`](institution-package/action-card.schema.json) — per-member action-card contract; shares the `authority_basis` / `risk_level` / `accessibility_hint` / `receipt_expected` vocabulary
- [`./schema-id-audit.md`](schema-id-audit.md) — audit of every contract `$id` in the repo (this schema is in §3, recommendation: keep)
- [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) — convenience-vs-authority and participation-access checklist
- [`../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — surface-side PR-time review gate that runs against any UI rendering these rows
- [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) — no-CLI organizer / member rehearsal workflow; §3 steps 3 + 6 are the preview moments these rows populate
- [icn#1728](https://github.com/InterCooperative-Network/icn/issues/1728) — issue this contract's row-level read-model addresses
- [icn#1724](https://github.com/InterCooperative-Network/icn/issues/1724) — umbrella issue for the no-CLI organizer / member rehearsal UX
- [icn#1726](https://github.com/InterCooperative-Network/icn/issues/1726) / [icn#1727](https://github.com/InterCooperative-Network/icn/issues/1727) — open follow-ups: organizer rehearsal shell + fixture-backed demo mode (separate; this contract is the read-model they will render)
- [icn#1730](https://github.com/InterCooperative-Network/icn/issues/1730) — open follow-up: private-overlay / DID-binding activation flow (separate; the contract's `assignee label not DID` discipline references this work)
