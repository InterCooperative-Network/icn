---
Status: descriptive
Canonical: yes
Last Reviewed: 2026-05-05
---

# Schema `$id` audit

> **This document audits the `$id` values of every JSON Schema under [`docs/contracts/`](.). It is the deliverable for [icn#1737](https://github.com/InterCooperative-Network/icn/issues/1737). It performs no migration. Migration of any DNS-backed `$id` is a separate, deliberate decision per schema, gated on compatibility review.**

## 1. Purpose

Capture the current state of contract-schema identity in this repository, classify each `$id` as DNS-backed or non-DNS, and record a per-schema recommendation. The recommendations are explicit on whether to keep, migrate, add, or investigate — and never recommend blind migration. Anything more than this audit ships in a separate PR.

This audit is the immediate consequence of the architecture due-diligence work in [icn#1738](https://github.com/InterCooperative-Network/icn/issues/1738) ([`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md)) and the rehearsal evidence export schema's identity decision in [icn#1729](https://github.com/InterCooperative-Network/icn/issues/1729) / [icn#1734](https://github.com/InterCooperative-Network/icn/pull/1734) ([`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md), Identity section).

## 2. Architectural rule

> **Convenience surfaces may be centralized. Authority surfaces should not be.**

A schema `$id` is a machine-facing identifier — but "machine-facing" does not mean "DNS should be authoritative." HTTP `$id` URIs make contract identity depend on:

- continued domain registration with a single registrar
- ICANN policy and pricing
- a single hosting decision
- the renewal calendar of whoever owns the registration

ICN should not put its contract identity behind any of those choke points. The repository path, git history, and (future) receipt and provenance mechanisms provide the verification path that survives any DNS event.

DNS-backed URLs (`intercooperative.network`, `icn.zone`, `icn.coop`, future mirrors) **may** be used as discovery, routing, mirror, or human-facing references. They **must not** be canonical contract authority.

The default for new contract schemas is a non-DNS URN of the form `urn:icn:contract:<short-name>:v<N>`. Where validator or tooling constraints rule out a meaningful URN namespace, a stable UUID URN (`urn:uuid:<uuid>`) is the explicitly bounded fallback.

## 3. Audit table

| Schema path | Current `$id` | Classification | Recommendation | Rationale |
|---|---|---|---|---|
| [`./rehearsal-evidence-export.schema.json`](rehearsal-evidence-export.schema.json) | `urn:icn:contract:rehearsal-evidence-export:v1` | non-DNS URN | **keep** | Already on the preferred non-DNS pattern. Companion fields `x-icn-contract-name` (human handle) and `x-icn-distribution-hints` (repo-relative paths) are explicitly not authority; full rationale in [`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md) Identity section. |
| [`./preview-review.schema.json`](preview-review.schema.json) | `urn:icn:contract:preview-review:v1` | non-DNS URN | **keep** | Landed already on the preferred non-DNS pattern (icn#1728). Same companion-field convention as the evidence schema (`x-icn-contract-name`, `x-icn-distribution-hints` as non-authoritative hints). Full rationale in [`./preview-review.md`](preview-review.md) Identity section. Read-only review-boundary contract; explicitly not a mutation API. |
| [`./institution-package/action-card.schema.json`](institution-package/action-card.schema.json) | `https://intercooperative.network/contracts/institution-package/action-card.schema.json` | DNS-backed (HTTPS) | **retain temporarily; review by 2026-06-30; migrate only after compatibility inventory** | The schema is referenced by institution-package validation guidance ([`./institution-package/README.md`](institution-package/README.md), which explicitly notes SDK / OpenAPI regen protects the **generated API contract**, not this hand-maintained JSON file). No in-repo generator path is currently known to consume this hand-maintained schema or its `$id`; migration still requires the §5 compatibility review because docs, examples, package validation, external consumers, or future tooling may have pinned the current identifier. Migration must land in its own PR. |

### 3.1 Out of scope but noted

The following non-`docs/contracts/` JSON Schema also lives in the repository. It is **not** in this audit's scope (this audit covers contract substrate identifiers, not runtime configuration), but it carries a DNS-backed `$id` on a different domain and is named here so a future reader knows it has not been overlooked.

| Schema path | Current `$id` | Classification | Notes |
|---|---|---|---|
| `config/icn-config.schema.json` | `https://icn.coop/schemas/config/v1.json` | DNS-backed (HTTPS), draft-07 | Runtime node-configuration schema (`title: ICN Node Configuration`), not a contract surface. Uses `icn.coop`, not `intercooperative.network`. Whether the runtime-config schema should adopt the same non-DNS pattern is a separate architectural question; if filed, it should be a sibling issue to [icn#1737](https://github.com/InterCooperative-Network/icn/issues/1737), not folded into the contracts audit. |

The Astro-generated content-collection schemas under `website/.astro/collections/*.schema.json` are build artifacts produced by Astro for content-collection typing. They are not authored contract identifiers and are explicitly not in scope.

## 4. Recommendations summary

- **Keep**: `rehearsal-evidence-export.schema.json` (already on the preferred pattern).
- **Retain temporarily; review by 2026-06-30; migrate only after compatibility inventory**: `institution-package/action-card.schema.json`. **Do not migrate in the same PR as this audit.** Open a separate PR scoped to that single schema's `$id` migration, gated on §5 below. If the 2026-06-30 review concludes that migration should not happen, document the decision in this audit and reset a new review date; do not let "temporarily" become "indefinitely."
- **Add**: none — every contract schema in scope has an `$id`.
- **No new schema** should ship in `docs/contracts/` with a DNS-backed `$id`. The default for new contract schemas is `urn:icn:contract:<short-name>:v<N>`. Where a meaningful URN is impractical, a stable UUID URN (`urn:uuid:<uuid>`) is the bounded fallback.

## 5. Migration safety rules

If a future PR migrates a DNS-backed `$id` to a non-DNS URN, that PR must:

1. **Enumerate downstream consumers** of the current `$id` before the change. At minimum: in-repo validators (e.g. `docs/scripts/validate-rehearsal-evidence.py`-shaped tools), TypeScript SDK regeneration paths, OpenAPI export, partner-package validators (NYCN drive-ingest, partner CI), and any external pinned references discovered through repo-wide search.
2. **Check what each consumer actually pins.** A consumer that pins on file path or schema content survives an `$id` change; a consumer that pins on the `$id` URI itself does not. Document the result of this check in the migration PR body.
3. **Prefer additive compatibility windows** when a schema is already referenced externally. Strategies include keeping the old `$id` as an alias entry in a separate `x-icn-aliases` array, dual-publishing during a transition window, or shipping the new URN alongside docs that pin the old URI as a discovery hint with an explicit "moved to" note.
4. **Keep distribution / mirror URLs explicitly non-authoritative**, both before and after the change. The `x-icn-distribution-hints` array convention from `rehearsal-evidence-export.schema.json` is the established pattern.
5. **If a DNS-backed `$id` is retained temporarily**, document the reason in the schema's companion `.md` (Identity section), open a follow-up issue, and set a follow-up review date.
6. **Bump the `$id` version (and the schema's `schema_version` field, where applicable) for any breaking shape change.** A migration that only changes the URI namespace without changing semantics is **not** a version bump; preserve the trailing `:v1` if the shape did not change.
7. **Update the audit table in this document** in the same PR that lands a migration. The audit table is the canonical record of where each schema sits.
8. **Validate every committed example packet** against the new `$id` after migration. The validator's output (`OK: <path> validates against <new $id>`) is the conformance receipt.
9. **Do not change `$id` values across multiple schemas in one PR.** One `$id` migration per PR. The deliberateness is the point; bundling defeats the per-consumer compatibility check.

## 6. Non-goals

- **No schema migration in this PR.** This document is the audit; migration ships separately.
- **No runtime change**, no API change, no OpenAPI regeneration triggered by this PR.
- **No website change** and **no NYCN edit**.
- **No production-readiness claim, no Phase 2 completion claim, no formal NYCN pilot**, no live federation, live Google Drive / Groups / Sheets sync, K3s / DNS / Forgejo mutation, private-data handling, or licensing decision.
- **No accessibility implementation work** in this PR. Surface-specific accessibility for organizer / member shells is [icn#1731](https://github.com/InterCooperative-Network/icn/issues/1731); website multilingual / inclusive-access planning is [icn#1740](https://github.com/InterCooperative-Network/icn/issues/1740). Both remain separate.
- **No preview/review API contract work.** That is [icn#1728](https://github.com/InterCooperative-Network/icn/issues/1728) and remains separate.
- **No CI lint** that mechanically scans for "DNS as authority" patterns. The architecture due-diligence checklist explicitly defers that to a separate, larger discussion.
- **No audit of `config/icn-config.schema.json` runtime-config identity.** It is named in §3.1 so future readers know the boundary, not because this PR addresses it.

## 7. Follow-up path

When (if) the project decides to migrate `institution-package/action-card.schema.json`'s `$id` to a non-DNS URN, the migration PR should:

- Be a single-schema PR titled along the lines of `spec(contracts): migrate action-card schema $id to non-DNS URN`.
- Reference this audit document and section §5 of [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md).
- Demonstrate the §5 downstream-consumer check in the PR body.
- Update §3 of this document in the same PR.
- Coordinate with the institution-package companion docs and any partner repository that vendors a validator for the action-card schema.

When (if) a similar audit for `config/icn-config.schema.json` is filed, it should be a sibling issue rather than a continuation of [icn#1737](https://github.com/InterCooperative-Network/icn/issues/1737), to keep the contracts audit and the runtime-config audit cleanly separable.

## 8. See also

- [`./rehearsal-evidence-export.md`](rehearsal-evidence-export.md) — Identity section: worked rationale for the non-DNS URN pattern (the precedent every recommendation in this audit references).
- [`../architecture/ARCHITECTURE_DUE_DILIGENCE.md`](../architecture/ARCHITECTURE_DUE_DILIGENCE.md) — convenience-vs-authority and participation-access checklist (the upstream architecture-review reflex this audit is a deliberate application of).
- [`../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md`](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) — surface-architecture context: DNS-backed surfaces are discovery / routing / human-facing, not contract authority.
- [`./institution-package/README.md`](institution-package/README.md) — institution-package contract notes; existing HTTP `$id` pattern is documented there, and any migration must update both that doc and this audit table.
- [icn#1737](https://github.com/InterCooperative-Network/icn/issues/1737) — issue this doc closes.
- [icn#1738](https://github.com/InterCooperative-Network/icn/issues/1738) — closed: architecture due-diligence checklist (this audit is the first deliberate application).
- [icn#1729](https://github.com/InterCooperative-Network/icn/issues/1729) — closed: rehearsal evidence export schema (the precedent).
- [icn#1731](https://github.com/InterCooperative-Network/icn/issues/1731), [icn#1728](https://github.com/InterCooperative-Network/icn/issues/1728), [icn#1740](https://github.com/InterCooperative-Network/icn/issues/1740) — open follow-ups; explicitly out of scope here.
