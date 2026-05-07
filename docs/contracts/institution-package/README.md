---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-07
---

# Institution package — ActionCard contract notes

## Files

| File | Purpose |
|------|---------|
| [`action-card.schema.json`](action-card.schema.json) | JSON Schema for one element of the `cards` array on `GET /v1/gov/me/action-cards`. |
| [`action-card.example.json`](action-card.example.json) | Fictional sample card object. Validates against the schema. Demonstrates the `proposal` / `vote` shape with all required fields plus optional `deadline` and `domain_id`. |
| [`../../scripts/validate-action-card.py`](../../scripts/validate-action-card.py) | Tiny validator — given a card-object path, checks it against the schema. Used by contributors and partner-package CI before committing example cards or rendering generic templates that will be checked into a public repository. |

## Stability

The schema sets `"x-icn-status": "rfc"`. Treat the shape as **versioned contract surface**, not frozen public API. The `rfc` marker is honest: the runtime ships the surface for three currently-emitted source/action pairs, but the contract is still subject to amendment (see [ADR-0027 § Card kind taxonomy](../../adr/ADR-0027-action-card-contract.md), which records that the closed taxonomy is "growable by ADR amendment"). **OpenAPI export and generated TypeScript types** are what CI drift checks guard today (`icnctl api export-openapi`, `sdk/typescript` regen) — those checks protect the **generated** API contract, not this hand-maintained JSON file. **`action-card.schema.json` is edited by hand** alongside contract work; changes to `ActionCard` / `ActionCardsResponse` in `icn/apps/governance/src/http/models.rs` should update this schema in the **same PR** as the API/OpenAPI change. **Mechanical** drift detection for this schema (for example CI comparing schema to serde shapes) is **future work** unless and until it is implemented. Publication and stabilization work is tracked in [icn#1713](https://github.com/InterCooperative-Network/icn/issues/1713).

The schema's `$id` is currently DNS-backed (`https://intercooperative.network/contracts/institution-package/action-card.schema.json`). Per [`../schema-id-audit.md`](../schema-id-audit.md), that identifier is **retained temporarily and reviewed by 2026-06-30**. Migration to a non-DNS URN of the form `urn:icn:contract:action-card:v<N>` will be a separate single-schema PR under the audit's §5 migration safety rules; this README and the audit table will be updated in the same PR as that migration.

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
3. Respect **emitted vs gated** `source_kind` values documented in the schema (`x-icn-emitted-source-kinds`, `x-icn-emitted-source-action-pairs`, and `x-icn-rfc-gated-source-kinds`). Today only `proposal`/`vote`, `meeting`/`attend`, and `action_item`/`complete` are emitted by the runtime; `signal_rule` and `obligation_lifecycle` are reserved for forward compatibility, gated on icn#1631 / icn#1711 and icn#1634 / icn#1712 respectively.
4. Prefer regulatory-safe vocabulary in human docs: **obligation**, **allocation**, **settlement**, **unit**, **position**, **receipt**, **provenance**, **evidence** — not payment/wallet/balance/currency framing for the substrate.
5. Institution-specific semantics belong in **institution packages**, not in ICN core. ICN core knows the generic primitives in this schema; packages translate their local templates (for example NYCN action-card templates) into these generic kinds and validate them against `action-card.schema.json` before committing or rendering.

### Command — validate a single card object

The bundled validator works as a draft-2020-12 JSON Schema validator with a stable CLI. Use it from the repo root, or from any partner package vendoring a copy:

```bash
# Validate the bundled fictional example.
python3 docs/scripts/validate-action-card.py

# Validate a partner-side card object.
python3 docs/scripts/validate-action-card.py path/to/card.json

# Validate against an alternate (pinned-version) schema.
python3 docs/scripts/validate-action-card.py --schema custom.schema.json card.json
```

Exit code 0 means the card validates; exit code 1 means the card or schema failed. The validator prints `OK: <path> validates against <$id>` on success and a per-error path/message report on failure.

Partner packages MAY vendor a copy of `action-card.schema.json` and either vendor or invoke `validate-action-card.py` from their own CI. Vendored copies should track this repo's version and update in the same change set as any contract amendment.

The validator validates a **single card object** (one element of the `cards` array on `GET /v1/gov/me/action-cards`). The HTTP wrapper (`{ did, cards, generated_at }`) is OpenAPI-documented in the gateway and is not described by this hand-maintained schema; if a partner needs to validate a wrapper, they should validate each element of `cards` against this schema and check the wrapper fields separately.

## Runtime source of truth

Rust structs and serde names live in `icn/apps/governance/src/http/models.rs` (`ActionCard`, `ActionCardsResponse`). If they diverge from this schema, update the schema in the same change set as the API change and regenerate OpenAPI / TypeScript types per `AGENTS.md`.
