---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Summit Ops Registration Fixture Shape (generic ICN)

> **Status update:** the card this doc specifies has since been **committed** to `web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json` and validated (per-card schema + `demo-fixture-preload.spec.js` + `standing-action-cards.spec.js` + the rehearsal-shell bundle validator), so the Registration Desk lane is now **fixture-backed at L2** (committed fictional fixture; not a runtime proof). This doc remains the **spec / rationale** for that fixture.

> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). This is a **fixture-shape map**: it specifies the exact, schema-valid JSON committed so the [Registration Desk proof loop](summit-ops-registration-action-card-proof-loop.md) moved from **fixture-ready** to genuinely **fixture-backed**. It syncs no Google surface, mutates no partner repo, and uses no real attendee data.

## Purpose

When this doc was written, the [registration proof-loop map](summit-ops-registration-action-card-proof-loop.md) was **fixture-ready, not fixture-backed**: the shell's fixture pack had no registration-lane rows. This doc pinned the precise fixture shape, the file it belongs in, and the validation to run. The fixture has **since been committed** (see the status note at the top), so the lane is now **fixture-backed (L2)** and this doc serves as the spec/rationale + validation recipe.

## Why this was specified docs-only first (Option A), before committing the fixture

The fixture *schema* was unambiguous, but the **render/validation path was not fully runnable from a docs-only change** at the time, so the shape was pinned first and the fixture committed in a follow-up where the e2e could run:

- The action-cards fixture's **content** is exercised by a **Playwright e2e** — `web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js`, which fetches `action-cards.json` and asserts the `{did, cards, generated_at}` wrapper plus **per-card required fields** and the `scope` / `risk_level` enums — and by the **"Rehearsal Fixture Bundle"** CI gate. (`standing-action-cards.spec.js` covers the Action-Cards-list **DOM wiring**, not fixture contents.) A committed fixture change should be verified against those, which need a browser/npm environment.
- The Python bundle validator ([`docs/scripts/validate-rehearsal-shell-fixtures.py`](../scripts/validate-rehearsal-shell-fixtures.py)) checks the action-cards read-model **shape-only** (`did`, `cards`, `generated_at`) — it does **not** per-card schema-validate, so passing it is necessary but not sufficient.

So this doc specified the shape and the full validation path; **committing the fixture + running the e2e/bundle gate was the next slice — since done**, which is the moment the lane became fixture-backed.

## Where the fixture lives

The organizer-demo rehearsal-shell bundle:

- **Fixture file to extend:** [`web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json`](../../web/pilot-ui/fixtures/icn-organizer-demo/action-cards.json) — wrapper `{ "did": …, "cards": [ … ], "generated_at": … }`. A registration card is appended to `cards`.
- **Manifest read-model:** `member_action_cards` in [`rehearsal-shell.manifest.json`](../../web/pilot-ui/fixtures/icn-organizer-demo/rehearsal-shell.manifest.json) (surface `GET /v1/gov/me/action-cards`, `validate: shape_only`). No manifest change is required to add a card.
- **Per-card contract:** [`docs/contracts/institution-package/action-card.schema.json`](../contracts/institution-package/action-card.schema.json).
- **Validators / tests:** `docs/scripts/validate-rehearsal-shell-fixtures.py` (bundle shape-only for action-cards), `web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js` (fetches `action-cards.json`; asserts wrapper + per-card required fields + `scope`/`risk_level` enums), `web/pilot-ui/tests/e2e/standing-action-cards.spec.js` (Action-Cards-list DOM wiring).

## The exact registration card shape

Schema-valid (`source_kind: action_item`, `action_kind: complete`, `scope: structure`), fictional ids only, matching the existing accessibility card's field set. Append this object to the `cards` array:

```json
{
  "id": "demo-card-action-item-complete-registration-001",
  "source_kind": "action_item",
  "action_kind": "complete",
  "scope": "structure",
  "title": "Mark a registration-desk step complete (badge-packet check)",
  "summary": "A day-of registration-desk step is assigned to the logistics committee role. Marking it complete produces a completion receipt. Fictional demo data; no real attendee information.",
  "authority_basis": "Logistics Committee role (demo role)",
  "required_authority_scope": ["registration_desk"],
  "deadline": 1730500000,
  "risk_level": "normal",
  "accessibility_hint": "Plain-language step summary before structured fields; status read in order; no color-only signaling.",
  "receipt_expected": true,
  "source_id": "demo-action-item-registration-001",
  "domain_id": "demo.coop.organizing"
}
```

**Field discipline:**

- `source_kind` / `action_kind` — the only emitted, schema-valid day-of pair: `action_item` / `complete`. **Do not** use `committee_log` or `program_milestone` (not in the closed `source_kind` enum) or invent new source kinds; any richer source-kind is a future schema change (separate ADR/RFC + `action-card.schema.json` update).
- `scope` — `structure` (the logistics `Structure` that owns the registration-desk work); must be one of `entity` / `structure` / `individual`.
- `id` / `source_id` — fictional, deterministic, prefixed `demo-…`. Carries ids, not contact info.
- All required fields present: `id, source_kind, action_kind, scope, title, summary, authority_basis, required_authority_scope, risk_level, accessibility_hint, receipt_expected, source_id` (`deadline`, `domain_id` optional).

The fictional, categorical registration steps the lane models (`registration-desk-opened`, `badge-packet-check-complete`, `walk-in-question-escalated`, `attendance-count-category-updated`, `late-arrival-support-needed`, `registration-desk-closed`) map onto one or more such `action_item/complete` cards — each a step a logistics role marks complete. Attendance stays a **count category**, never a roll.

## Validation that was run before claiming fixture-backed

This is the recipe that was run (and that any future change to the card must re-run). From the monorepo root, after appending the card to `action-cards.json`:

1. `python3 docs/scripts/validate-rehearsal-shell-fixtures.py` — must stay `PASS` (action-cards read-model is shape-only: wrapper keys unchanged).
2. The **per-card schema check** against `action-card.schema.json` (e.g. validate the appended object with `jsonschema`).
3. The **fixture-content Playwright e2e** `web/pilot-ui/tests/e2e/demo-fixture-preload.spec.js` — it fetches `action-cards.json` and asserts the wrapper + per-card required fields + `scope`/`risk_level` enums (it checks `cards.length > 0`, not a fixed count, so appending a card is fine) — plus the **"Rehearsal Fixture Bundle"** CI gate; `standing-action-cards.spec.js` additionally covers the Action-Cards-list DOM wiring. Re-run these to be sure.
4. `python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml`, `bash ops/scripts/drift-check.sh`, `python3 scripts/check-state-lag.py`, `git diff --check`.

These passed and the fixture is committed, so the [registration proof-loop map](summit-ops-registration-action-card-proof-loop.md) has been updated from *fixture-ready* to *fixture-backed* (L2). Note: the committed card requires `registration_desk`, so the paired `standing.json` also grants a demo Logistics Committee role with that scope — an ActionCard must be derivable from holder standing.

## Privacy boundaries

Fictional / categorical only. Never commit: real attendee names, registration lists, emails, phone numbers, medical/accessibility details, actual attendance rolls, private Drive URLs, raw Google Sheet rows, or financial/payment details. Registration is `attendee-restricted`; rehearsal privacy is by exclusion.

## Nonclaims

- The committed fixture is L2 (a fictional fixture the shell loads/validates), **not** a runtime proof; it does not make this a live action-card→receipt loop.
- Does not claim live summit registration or a real attendee workflow exists.
- Does not sync Google data; does not mutate the NYCN repo.
- Does not commit private attendee, sponsor, accessibility, volunteer, incident, registration, or payment data.
- Does not claim organizer readiness, a formal NYCN pilot, production readiness, live federation, or Phase 2 completion.
- Does not change route behavior, `icnctl` behavior, authn/authz, or OpenAPI; does not regenerate SDK types.
- Does not affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082); does not start A2e.

## Next slices

1. ~~Commit the registration card above into `action-cards.json`; run the validators + e2e + Rehearsal Fixture Bundle gate; flip the lane to **fixture-backed**.~~ **Done** — card committed + validated; lane is fixture-backed (L2).
2. Wire / confirm the shell renders the full registration action register (not just the single card) from committed fixtures.
3. Add a second lane's fixture shape (room monitors or closing/cleanup) the same way.

## Where to read deeper

| You want… | Read |
|---|---|
| The registration proof-loop map (parent) | [`summit-ops-registration-action-card-proof-loop.md`](summit-ops-registration-action-card-proof-loop.md) |
| The run-stage facilitator path | [`summit-ops-run-stage-facilitator-path.md`](summit-ops-run-stage-facilitator-path.md) |
| The ActionCard contract | [`docs/contracts/institution-package/action-card.schema.json`](../contracts/institution-package/action-card.schema.json) |
| Recorded proof per capability | [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| The milestone + spine | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
