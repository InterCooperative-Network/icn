---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-03
---

# Institution package — ActionCard contract notes

## Files

| File | Purpose |
|------|---------|
| [`action-card.schema.json`](action-card.schema.json) | JSON Schema for one element of the `cards` array on `GET /v1/gov/me/action-cards`. |

## Stability

The schema sets `"x-icn-status": "rfc"`. Treat the shape as **versioned contract surface**, not frozen public API — gateway field additions go through OpenAPI regeneration and CI drift checks. Publication and stabilization work is tracked in [#1713](https://github.com/InterCooperative-Network/icn/issues/1713).

## Validation guidance for package repos

1. Validate each planned card object against `action-card.schema.json` **only** for keys you intend to align with ICN runtime; package-local extensions belong outside the validated object or in a separate schema owned by the package.
2. Do **not** add institution-specific **nouns** to this schema; bind local meaning in package docs or mappings.
3. Respect **emitted vs gated** `source_kind` values documented in the schema (`x-icn-emitted-source-kinds` vs `x-icn-rfc-gated-source-kinds`).
4. Prefer regulatory-safe vocabulary in human docs: **settlement**, **obligation**, **allocation**, **receipt**, **provenance** — not payment/wallet/balance framing for the substrate.

## Runtime source of truth

Rust structs and serde names live in `icn/apps/governance/src/http/models.rs` (`ActionCard`, `ActionCardsResponse`). If they diverge from this schema, update the schema in the same change set as the API change and regenerate OpenAPI / TypeScript types per `AGENTS.md`.
