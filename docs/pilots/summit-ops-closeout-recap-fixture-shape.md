---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Summit Ops Closeout Recap Fixture Shape (generic ICN)

> **Status:** this lane is **fixture-ready, not fixture-backed.** This doc pins the exact, schema-valid ActionCard a future contributor could commit to `web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json` for the **Public Recap Draft Handoff** closeout lane, plus the matching demo standing role/scope that commit must also add. **No runtime fixture is committed here** and **no `web/pilot-ui` file is touched.** The lane becomes **fixture-backed (L2)** only when the JSON card is committed, the matching standing exists, and the validation below passes.

> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). This is a **fixture-shape map** (Option A, mirroring [`summit-ops-registration-fixture-shape.md`](summit-ops-registration-fixture-shape.md)): it specifies the precise fixture shape, where it belongs, the standing it needs, and the validation to run — so a later slice can move the lane from **fixture-ready** to **fixture-backed**. It syncs no Google surface, mutates no partner repo, and uses no real event data.

## Purpose

The [closeout continuity packet](summit-ops-closeout-continuity-packet.md) maps the **Public Recap Draft Handoff** lane at **L1 declared shapes**: source = recap doc + comms calendar; repo-safe shape = recap-draft template + handoff step (public-safe); possible future ActionCard = "hand off public recap draft"; possible receipt = handoff receipt; privacy class = `public`. This doc pins that lane's exact, schema-valid `ActionCard` JSON, the file it belongs in, the demo standing scope it depends on, and the full validation path — without committing the fixture. Committing the card + standing and running the validation is the next slice.

## Why Public Recap Draft Handoff first

Of the twelve closeout lanes, Public Recap Draft Handoff is the **lowest-privacy, highest-continuity** lane to fixture-shape first:

- **Lowest privacy risk.** Its data is `public` by design — a recap *draft handoff* step models a communications role confirming a public-facing recap packet is ready for review. It carries no attendee roll, no registration data, no payment/reimbursement figures, no incident detail, no accessibility/accommodation detail, and no sponsor contacts. The card stays categorical and fictional (a handoff *step*, not the recap's contents).
- **Useful continuity output.** A public recap handoff is a concrete, recurring post-event deliverable, so the lane is worth making reproducible/auditable later.
- **Easy to model categorically.** It maps cleanly onto the one emitted, schema-valid completion pair (`action_item` / `complete`) at `scope: structure` — the same shape as the registration card — with a single role-owned step.

This advances the **close** stage of the event lifecycle while staying docs-only: it does **not** expand pilot-UI surface, so [#2099](https://github.com/InterCooperative-Network/icn/issues/2099) (CodeQL DOM-XSS / missing-SRI in pilot UI) is unaffected.

## Fixture-ready vs fixture-backed

- **This PR leaves the lane fixture-ready only.** It is a docs-only spec; nothing under `web/pilot-ui` changes; no card is added to `action-cards.json` and no scope is added to `standing.json`.
- **The lane becomes fixture-backed (L2) only when** the JSON card below is committed to `action-cards.json`, a matching demo standing role/scope (`public_recap`) is committed to `standing.json` so the card is derivable from holder standing, **and** the validation path passes (per-card schema check + bundle validator + the Playwright e2e + the Rehearsal Fixture Bundle CI gate). Until all of that lands, the lane is **fixture-ready, not fixture-backed**, and no L2 claim may be made for it. The only committed Summit Ops fixture today remains the registration card ([#2209](https://github.com/InterCooperative-Network/icn/pull/2209), L2).

## Where the fixture would live

The organizer-demo rehearsal-shell bundle:

- **Fixture file to extend:** [`web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json`](../../web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json) — wrapper `{ "did": …, "cards": [ … ], "generated_at": … }`. The recap-handoff card is appended to `cards`.
- **Standing file to extend:** [`web/pilot-ui/fixtures/icn-organizer-demo/standing.json`](../../web/pilot-ui/fixtures/icn-organizer-demo/standing.json) — the card requires the `public_recap` authority scope, which does **not** exist in standing today (the roles grant `session_scheduling`, `program_review`, `veto_for_cause`, `registration_desk`). The future fixture commit must add a demo Communications Committee role granting `public_recap` and add `public_recap` to the top-level `authority_scopes`, exactly as [#2209](https://github.com/InterCooperative-Network/icn/pull/2209) added the Logistics role + `registration_desk` — an ActionCard must be derivable from holder standing.
- **Manifest read-model:** `member_action_cards` in [`rehearsal-shell.manifest.json`](../../web/pilot-ui/fixtures/icn-organizer-demo/rehearsal-shell.manifest.json) (surface `GET /v1/gov/me/action-cards`, `validate: shape_only`). No manifest change is required to add a card.
- **Per-card contract:** [`docs/contracts/institution-package/action-card.schema.json`](../contracts/institution-package/action-card.schema.json).
- **Validators / tests:** [`docs/scripts/validate-rehearsal-shell-fixtures.py`](../scripts/validate-rehearsal-shell-fixtures.py) (bundle shape-only for action-cards), [`web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js`](../../web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js) (fetches `action-cards.json`; asserts the `{ did, cards, generated_at }` wrapper + per-card required fields + `scope`/`risk_level` enums), [`web/pilot-ui/tests/e2e/standing-action-cards.spec.js`](../../web/pilot-ui/tests/e2e/standing-action-cards.spec.js) (Action-Cards-list DOM wiring).

## The exact recap-handoff card shape

Schema-valid (`source_kind: action_item`, `action_kind: complete`, `scope: structure`), fictional ids only, matching the existing registration card's field set. Append this object to the `cards` array:

```json
{
  "id": "demo-card-action-item-complete-closeout-recap-001",
  "source_kind": "action_item",
  "action_kind": "complete",
  "scope": "structure",
  "title": "Mark public recap draft handoff complete",
  "summary": "A post-event closeout step asks a communications role to confirm a public recap draft handoff packet is ready for review. Marking it complete produces a completion receipt. Fictional demo data; no attendee, sponsor, incident, accessibility, payment, or private planning details.",
  "authority_basis": "Communications Committee role (demo role)",
  "required_authority_scope": ["public_recap"],
  "deadline": 1730500000,
  "risk_level": "normal",
  "accessibility_hint": "Plain-language step summary before structured fields; status read in order; no color-only signaling.",
  "receipt_expected": true,
  "source_id": "demo-action-item-closeout-recap-001",
  "domain_id": "demo.coop.organizing"
}
```

## The matching demo standing the fixture commit must add

The card requires `public_recap`, which standing does not currently grant. The future fixture commit must add a demo Communications Committee role to the `roles` array in [`standing.json`](../../web/pilot-ui/fixtures/icn-organizer-demo/standing.json) and add `public_recap` to the top-level `authority_scopes` (do **not** commit this in this docs-only slice):

```json
{
  "role_assignment_id": "demo-role-004",
  "structure_id": "demo.committee.communications",
  "structure_name": "Communications Committee (demo)",
  "parent_entity_id": "demo.coop.organizing",
  "role": "member",
  "authority_scope": ["public_recap"],
  "start_date": 1716000000
}
```

Without that standing grant, the recap-handoff card would require a scope the demo holder does not hold, and the lane would not be honestly derivable — so committing the card alone is not enough to claim fixture-backed.

**Field discipline:**

- `source_kind` / `action_kind` — the only emitted, schema-valid completion pair: `action_item` / `complete`. **Do not** introduce `public_recap`, `closeout_log`, `committee_log`, `program_milestone`, or any invented source kind (the `source_kind` enum is closed: `proposal`, `meeting`, `action_item`, `signal_rule`, `obligation_lifecycle`, with only the first three emitted). Any richer source-kind is a future schema change (separate ADR/RFC + `action-card.schema.json` update).
- `scope` — `structure` (the Communications `Structure` that owns the recap-handoff step); must be one of `entity` / `structure` / `individual`. **Do not** invent a scope outside those three.
- `required_authority_scope` — `["public_recap"]`. This is an *authority-scope string* in a fixture (the schema notes packages "may use richer strings"), **not** a `source_kind` and not an ICN-core type — it is package vocabulary on the demo plane, consistent with the existing `registration_desk` / `session_scheduling` scope strings.
- `id` / `source_id` — fictional, deterministic, prefixed `demo-…`. Carries ids, not contact info or recap contents.
- `additionalProperties: false` — only schema properties are permitted; `deadline` and `domain_id` are the optional ones used here.
- All required fields present: `id, source_kind, action_kind, scope, title, summary, authority_basis, required_authority_scope, risk_level, accessibility_hint, receipt_expected, source_id`.

## Validation a contributor must pass before claiming fixture-backed

From the monorepo root, after appending the card to `action-cards.json` **and** adding the matching role/scope to `standing.json`:

1. **Per-card schema check** of the appended object against [`action-card.schema.json`](../contracts/institution-package/action-card.schema.json) (e.g. validate with `jsonschema`).
2. `python3 docs/scripts/validate-rehearsal-shell-fixtures.py` — must stay `PASS` (the action-cards read-model is shape-only: the `{ did, cards, generated_at }` wrapper keys are unchanged by appending a card).
3. The **fixture-content Playwright e2e** [`web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js`](../../web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js) — it fetches `action-cards.json` and asserts the wrapper + per-card required fields + `scope`/`risk_level` enums (it checks `cards.length > 0`, not a fixed count, so appending a card is fine).
4. The **DOM-wiring e2e** [`web/pilot-ui/tests/e2e/standing-action-cards.spec.js`](../../web/pilot-ui/tests/e2e/standing-action-cards.spec.js) — Action-Cards-list rendering against standing.
5. The **"Rehearsal Fixture Bundle"** CI gate.
6. `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml`, `bash ops/scripts/drift-check.sh`, `python3 scripts/check-state-lag.py`, `git diff --check`.

Only when all of these pass may the lane be flipped to **fixture-backed (L2)** in the closeout packet and the [proof-level matrix](../reference/project-index/proof-level-taxonomy-capability-matrix.md). L2 means a committed fictional fixture the shell loads/validates — **not** a runtime proof.

## Privacy boundaries

Fictional / categorical only. Never commit, in any form: real attendee names, registration rolls, emails, phone numbers, accessibility/accommodation details, medical details, incident details that identify a person, sponsor contacts, real reimbursement/payment details (no amounts), private Drive URLs, raw Google Docs/Sheets/Groups exports, credentials/tokens. The recap-handoff card models a public-safe handoff **step** (a status), never the recap's contents or any participant data; rehearsal privacy is by exclusion.

## Nonclaims

- Does not commit a runtime fixture; does not make the closeout recap lane fixture-backed.
- Does not touch pilot-UI behavior (no `web/pilot-ui` change); [#2099](https://github.com/InterCooperative-Network/icn/issues/2099) is unaffected by this PR.
- Does not use real event data; does not sync Google; does not mutate the NYCN repo.
- Does not add runtime proof; does not claim a live action-card → receipt loop.
- Does not claim organizer readiness, a formal NYCN pilot, production readiness, live federation, or Phase 2 completion.
- Does not change route behavior, `icnctl` behavior, authn/authz, or OpenAPI; does not regenerate SDK types.
- Does not affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082); does not start A2e.

## Follow-ups

1. Commit the recap-handoff card above into `action-cards.json` **and** the matching `demo-role-004` Communications role + `public_recap` scope into `standing.json`; run the validators + e2e + Rehearsal Fixture Bundle gate; flip the lane to **fixture-backed (L2)** in the closeout packet + proof-level matrix.
2. Add a second low-risk closeout lane's fixture shape the same way (e.g. open follow-up register or next-year continuity packet).
3. Shell-render a closeout register — **only after [#2099](https://github.com/InterCooperative-Network/icn/issues/2099)** is handled (pilot-UI surface).
4. [#2113](https://github.com/InterCooperative-Network/icn/issues/2113) — per-command `icnctl` live/partial/fixture/planned status classification.

## Where to read deeper

| You want… | Read |
|---|---|
| The closeout continuity packet (parent) | [`summit-ops-closeout-continuity-packet.md`](summit-ops-closeout-continuity-packet.md) |
| The registration fixture shape (the Option-A pattern this mirrors) | [`summit-ops-registration-fixture-shape.md`](summit-ops-registration-fixture-shape.md) |
| The registration proof loop + its L2 fixture | [`summit-ops-registration-action-card-proof-loop.md`](summit-ops-registration-action-card-proof-loop.md) |
| The ActionCard contract | [`docs/contracts/institution-package/action-card.schema.json`](../contracts/institution-package/action-card.schema.json) |
| Recorded proof per capability | [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| The ICN ↔ NYCN ↔ public boundary | [`docs/ATLAS.md`](../ATLAS.md) |
| The milestone + spine | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
