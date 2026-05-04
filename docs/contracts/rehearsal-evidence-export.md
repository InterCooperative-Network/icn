---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-04
---

# Rehearsal evidence export — contract notes

## Files

| File | Purpose |
|------|---------|
| [`rehearsal-evidence-export.schema.json`](rehearsal-evidence-export.schema.json) | JSON Schema (draft 2020-12) for a single repo-safe rehearsal evidence packet. |
| [`rehearsal-evidence-export.example.json`](rehearsal-evidence-export.example.json) | Fictional sample packet. Validates against the schema. Demonstrates the field shape a rehearsal walk should produce. |
| [`../scripts/validate-rehearsal-evidence.py`](../scripts/validate-rehearsal-evidence.py) | Tiny validator — given a packet path, checks it against the schema. Used by CI / contributors before committing example or partner-side packets that will live in a public repository. |

## Purpose

This schema defines the shape of the **repo-safe rehearsal evidence packet** described by [`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) §3 step 9 and §4 ("Privacy-safe evidence by default"). It is the substrate-level contract that every institution package's evidence checklist binds to. Packages do not extend this schema with partner-specific nouns; they bind their local meaning in package docs and produce packets that conform.

The packet enumerates what a rehearsal did, in plain language, with categories and basenames only.

## Identity — non-DNS contract identifier

The schema `$id` is a non-DNS URN:

```
urn:icn:contract:rehearsal-evidence-export:v1
```

Contract identity for ICN must not depend on centralized DNS, ICANN registrars, registrar renewal, or capitalist hosting/payment structures. Schema `$id` values are machine-facing identifiers — validators and `$ref` consumers read them — but "machine-facing" does not mean "DNS should be authoritative." HTTP URIs like `https://intercooperative.network/contracts/...` quietly make contract identity depend on a single domain registration and a single hosting decision. ICN should not put its contract identity behind a centralized naming choke point.

The companion field `x-icn-contract-name` (`icn.contract.rehearsal_evidence_export.v1`) gives a human-readable handle for the same identity, useful in logs and discovery. It is not a substitute for the URN; the URN is canonical.

The companion field `x-icn-distribution-hints` lists repo-relative paths where copies of the schema and its companion artifacts can be found. **Distribution hints are not authority.** They tell a consumer where to look for a copy. The contract identity remains the URN.

DNS-backed URLs (`intercooperative.network`, `icn.zone`, future mirrors) may be used as **discovery, routing, mirror, or human-facing surfaces** for these documents. They must not become the canonical identifier.

Verifiability of a packet's claim to conform to this contract comes from:

- the URN identity, which is stable across hosts and registrars
- the repository path that holds this schema (`docs/contracts/rehearsal-evidence-export.schema.json`)
- the git history of that path
- future receipt and provenance mechanisms when those land

If a future ICN-side mechanism (for example a content-addressed contract registry) supersedes the URN approach, that is a deliberate contract-identity migration. It is not driven by DNS availability.

## Stability

The schema sets `"x-icn-status": "rfc"`. Treat the shape as **versioned contract surface**, not frozen public API. The version is encoded in the URN (`...:v1`); the next breaking shape requires a new URN (`...:v2`) and an explicit migration note. Non-breaking additions (new optional fields, new enum members at the end of a closed taxonomy) may land at the same URN, with a `schema_version` bump in the packet field.

Producers should pin to the URN, not to a file path or a DNS URL.

## Field shape

The schema is the source of truth; this section is an at-a-glance summary. See `rehearsal-evidence-export.schema.json` for required-vs-optional, types, and closed-enum membership.

| Field | Closed taxonomy? | Notes |
|-------|------------------|-------|
| `schema_version` | — | Semver-style; bumped per shape change at this URN. |
| `recorded_at` | — | RFC 3339 timestamp; the moment of human review-and-export. |
| `rehearsal_label` | — | Short, plain-language title; generic labels only. |
| `rehearsal_mode` | yes | `fixture-only` / `local-dry-run` / `local-execute` / `non-production-gateway`. Production gateways are never a valid value. |
| `audience_categories` | yes | Generic role labels only — never names, emails, or counts that could de-anonymize a small group. |
| `source_material[]` | partial | `kind` is closed (`committed-fixture` / `example-snippet` / `repo-safe-paste`); basename + summary are free text. **No raw external paths.** |
| `workflow_steps_completed[]` | yes | Mirrors §3 of the no-CLI workflow doc. |
| `decision_outcomes[]` | partial | `category` closed; `plain_summary` free text but repo-safe. |
| `preview_review_boundary` | — | `enforced` boolean; document why if not enforced. |
| `mutation_boundary` | partial | `target` closed; `executed` and `target` together describe whether and how mutation happened. |
| `proof_loop_references[]` | yes | `kind` closed; only public ids or category labels. **No private DIDs or holder identifiers.** |
| `warnings[]` | — | Plain-language; not stack traces. |
| `follow_ups[]` | — | URLs only to public issues / PRs. |
| `privacy_review` | partial | Generic role label + closed status enum. |
| `accessibility_review` | partial | Same shape as `privacy_review`; status enum is its own. |
| `export_safety_classification` | yes | `repo-safe` is the only value packets in this repository should carry. |
| `non_claims[]` | — | Free text; must include the standing four (no production deployment, no formal pilot, no live federation/cloud-sync, no private-data handling). |

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
- any field that implies formal pilot approval or production deployment
- DIDs of unconsented participants

This list is also captured in the schema's `x-icn-must-not-include` extension so machine consumers (linters, partner CI) can read it without parsing this prose.

## Validation guidance for package repos

1. Validate every example or evidence packet that will live in a **public repository** against `rehearsal-evidence-export.schema.json` before committing. Use the bundled validator (`docs/scripts/validate-rehearsal-evidence.py`) or any draft-2020-12 JSON Schema validator. Pin to the URN.
2. Do **not** add partner-specific nouns to this schema. Bind local meaning in package docs.
3. **Partner-internal overlays do not validate against this schema and must not be committed.** This schema is for the repo-safe export path only.
4. Prefer regulatory-safe vocabulary in human-language fields: **obligation**, **allocation**, **settlement**, **unit**, **position**, **receipt**, **provenance**, **evidence** — not payment / wallet / balance / currency framing for substrate flows.
5. Keep the standing non-claims in every packet. Producers may **add** packet-specific non-claims; producers MUST NOT **remove** the standing four.

## Producing a packet

A rehearsal facilitator (or a future organizer shell) walks the §3 ladder of the no-CLI workflow doc and records what happened. Producing the packet is the final step (`workflow_steps_completed: export-evidence`). The producer should:

1. Use a generic label for the rehearsal — no partner / organization / person names unless they are fictional examples.
2. Categorize source material by `committed-fixture` / `example-snippet` / `repo-safe-paste`. Do not paste in raw cloud-drive paths or private content.
3. Record decisions in plain language with the closed `category` taxonomy.
4. Honor the **review-first / mutation-last** boundary. If `preview_review_boundary.enforced` was false, document why in `notes` so the failure is visible.
5. Run the privacy review (even on a fictional packet) and record the result with a generic `reviewer_role`.
6. Run the accessibility review of the rehearsal itself; record the result.
7. Validate the packet against the schema before committing or pasting it into a public issue comment.

## Out of scope

- Mutation API for emitting evidence packets (no runtime / gateway code is implied here).
- Partner overlay format — partner-internal data lives in private overlays, outside this schema.
- Federation export protocol.
- UI rendering of evidence packets (organizer / member shell work tracked under [#1726](https://github.com/InterCooperative-Network/icn/issues/1726) / [#1727](https://github.com/InterCooperative-Network/icn/issues/1727)).
- Migration of existing contract schemas (e.g. `action-card.schema.json`) to non-DNS `$id` — that is a deliberate follow-up evaluation, not part of this PR.

## See also

- [`../pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) — generic ICN no-CLI organizer / member rehearsal workflow (§3, §4)
- [`./institution-package/README.md`](institution-package/README.md) — institution-package contract notes (existing schema follows the older HTTP `$id` pattern; migration is a separate evaluation)
- [`../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md`](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) — surface-architecture context for `intercooperative.network` vs `icn.zone` (DNS-backed surfaces are discovery / routing / human-facing, not contract authority)
- ICN [#1729](https://github.com/InterCooperative-Network/icn/issues/1729) — tracking issue for this contract
- ICN [#1724](https://github.com/InterCooperative-Network/icn/issues/1724) — umbrella issue for the no-CLI organizer / member rehearsal UX
- NYCN [#56](https://github.com/InterCooperative-Network/nycn/issues/56) — partner-side example packet to follow once this contract lands
