---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-04
---

# Institution package — ActionCard contract notes

## Files

| File | Purpose |
|------|---------|
| [`action-card.schema.json`](action-card.schema.json) | JSON Schema for one element of the `cards` array on `GET /v1/gov/me/action-cards`. |

## Stability

The schema sets `"x-icn-status": "rfc"`. Treat the shape as **versioned contract surface**, not frozen public API. **OpenAPI export and generated TypeScript types** are what CI drift checks guard today (`icnctl api export-openapi`, `sdk/typescript` regen) — those checks protect the **generated** API contract, not this hand-maintained JSON file. **`action-card.schema.json` is edited by hand** alongside contract work; changes to `ActionCard` / `ActionCardsResponse` in `icn/apps/governance/src/http/models.rs` should update this schema in the **same PR** as the API/OpenAPI change. **Mechanical** drift detection for this schema (for example CI comparing schema to serde shapes) is **future work** unless and until it is implemented. Publication and stabilization work is tracked in [#1713](https://github.com/InterCooperative-Network/icn/issues/1713).

## Emitted pairs (runtime today)

These `(source_kind, action_kind)` combinations are what `icn/apps/governance` may place on `GET /v1/gov/me/action-cards` when matching governance objects exist. They are also listed in the schema extension `x-icn-emitted-source-action-pairs`.

| source_kind | action_kind | Plain-language ask |
|-------------|---------------|-------------------|
| `proposal` | `vote` | Cast or delegate a vote on an open proposal. |
| `meeting` | `attend` | Confirm or adjust attendance for a scheduled meeting. |
| `action_item` | `complete` | Close an assigned action item via its governance endpoints. |

## Gated source kinds (schema reserved; not emitted)

Enum values exist for forward compatibility; the runtime does **not** return cards for these until their source paths land.

| source_kind | Tracking (implementation + contract) |
|-------------|----------------------------------------|
| `signal_rule` | [#1631](https://github.com/InterCooperative-Network/icn/issues/1631), [#1711](https://github.com/InterCooperative-Network/icn/issues/1711), [#1646](https://github.com/InterCooperative-Network/icn/issues/1646) |
| `obligation_lifecycle` | [#1634](https://github.com/InterCooperative-Network/icn/issues/1634), [#1712](https://github.com/InterCooperative-Network/icn/issues/1712), [#1646](https://github.com/InterCooperative-Network/icn/issues/1646) |

## Organizer-facing chain (how to explain the demo)

**Standing → action cards → authorized action on native endpoints → receipt → provenance/evidence.** Standing (`GET /v1/gov/me/standing`) says who the caller is and what scopes apply. Action cards (`GET /v1/gov/me/action-cards`) list pending work as generic derived rows only. Completing work uses the normal proposal, meeting, or action-item HTTP surfaces for that object type; cards do not define a separate mutation API. Receipts close the proof loop where documented (see runtime maps and ADR-0027).

## Validation guidance for package repos

1. Validate each planned card object against `action-card.schema.json` **only** for keys you intend to align with ICN runtime; package-local extensions belong outside the validated object or in a separate schema owned by the package.
2. Do **not** add institution-specific **nouns** to this schema; bind local meaning in package docs or mappings.
3. Respect **emitted vs gated** `source_kind` values documented in the schema (`x-icn-emitted-source-kinds`, `x-icn-emitted-source-action-pairs`, and `x-icn-rfc-gated-source-kinds`).
4. Prefer regulatory-safe vocabulary in human docs: **settlement**, **obligation**, **allocation**, **receipt**, **provenance** — not payment/wallet/balance framing for the substrate.

## Runtime source of truth

Rust structs and serde names live in `icn/apps/governance/src/http/models.rs` (`ActionCard`, `ActionCardsResponse`). If they diverge from this schema, update the schema in the same change set as the API change and regenerate OpenAPI / TypeScript types per `AGENTS.md`.
