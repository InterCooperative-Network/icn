---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Summit Ops Registration Action-Card Proof Loop (generic ICN)

> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). This is a **generic ICN-side, fictional, fixture-backed proof-loop map** for one event-day lane — the Registration Desk — taken from the [run-stage facilitator path](summit-ops-run-stage-facilitator-path.md). It is a **proof-loop map / rehearsal-ready shape**, not a runtime proof: no action-card loop has been executed for this lane. It changes no code, syncs no Google surface, mutates no partner repo, and commits no real attendee data.

## Purpose

Take a single run-stage lane and walk it through the minimal ICN proof loop end-to-end, **on fiction only**, so the shape is legible and rehearsal-ready:

```text
fictional run-stage lane
  → source packet shape
  → reviewed action candidate
  → ActionCard candidate
  → authorized completion action
  → completion receipt candidate
  → evidence export candidate
  → post-event follow-up item
```

It answers: 1. What is the fictional source packet? · 2. What does the facilitator review? · 3. What becomes an action-card candidate? · 4. Who/what role can complete it? · 5. What counts as completion? · 6. What receipt/evidence would be produced in future? · 7. What is fixture-only today? · 8. What routes/commands are declarations only? · 9. What proof level is honestly supported? · 10. What must not be claimed?

## Boundary

The three planes from the [lifecycle map](summit-ops-lifecycle-package-map.md) stay apart: **Google + in-person** (live registration, never synced/committed) · **NYCN package repo** (repo-safe shapes, fictional placeholders; standalone `nycn` is the active home, in-monorepo `institutions/nycn/` is scaffold pending reconciliation) · **future ICN node** (live `ActionCard`→`Receipt`; no Summit Ops cockpit today). Per [`docs/ATLAS.md`](../ATLAS.md), real registration operating detail stays in the private package; this doc uses only **fictional, categorical** examples. Meaning firewall: "registration desk," "badge packet," "walk-in question," "attendance count category" are **package/event vocabulary**; ICN core stays generic — `Activity`, `RoleAssignment`, `ActionItem`, `ActionCard`, `Receipt`, `EvidenceExport`, `Capability`, `Standing`.

## Why registration desk first

The Registration Desk is operationally central (it gates arrival and touches every other lane) **and** it can be modeled end-to-end without any real attendee data: its work decomposes into categorical, fictional steps (desk opened, badge-packet check, walk-in escalation, attendance-count *category*, late-arrival support, desk closed) that never require a name, a roll, or a contact. That makes it the safest lane to model a full proof loop on fiction. (The NYCN package models this as a logistics-committee day-of role at boundary level — referenced, not copied.)

## What this path is for

- Showing the minimal ICN proof loop for one lane, on fiction, so a steward can rehearse it.
- Giving the run stage a concrete, fixture-backed shape that maps to future ICN action cards / receipts / evidence.

## What this path is not

- Not a new app, not running software, not a node-hosted Summit Ops cockpit.
- Not a live Google sync, not a NYCN-repo mutation, not a real attendee workflow.
- Not a production/event-day claim; not a claim the loop has been executed.

## Fictional source packet

A repo-safe, fictional registration-desk run packet — categorical steps only, no people:

```text
registration-desk-opened
badge-packet-check-complete
walk-in-question-escalated
attendance-count-category-updated   # a count band/category, never a roll
late-arrival-support-needed
registration-desk-closed
```

This is a **shape with fictional rows**, not a live checklist. Real specifics (rosters, names, accommodation detail) stay in the private package / private overlay and never enter this repo.

## Reviewed action candidate

The facilitator reviews the registration-desk checklist (plain language, the steps above) and the steward/operator confirms **fixture/example mode** (nothing live). A step that needs doing — e.g. `badge-packet-check-complete` not yet done, or `walk-in-question-escalated` — becomes a candidate action. The reviewed candidate is still a fictional shape; no attendee data is consulted.

## Action-card candidate

The candidate maps to the one **already-emitted, schema-valid** ActionCard pair that fits a day-of task — `action_item` / `complete` — per the ICN-core contract [`docs/contracts/institution-package/action-card.schema.json`](../contracts/institution-package/action-card.schema.json). That schema's `source_kind` is a **closed taxonomy** (emitted: `proposal/vote`, `meeting/attend`, `action_item/complete`; RFC-gated forward-compat: `signal_rule`, `obligation_lifecycle`), and `scope` is limited to `entity` / `structure` / `individual`. The registration-desk candidate therefore stays inside the schema:

- **source_kind:** `action_item` · **action_kind:** `complete` (the emitted, schema-valid pair)
- **scope:** `structure` — the logistics `Structure` that owns the registration-desk work (a valid schema scope; the "registration desk" itself is package/event vocabulary for an `Activity` sub-step, but the card's schema `scope` field is `structure`)
- **authority_basis:** `assigned_action_item` / role assignment in the owning `Structure` — i.e. a `RoleAssignment` for the registration/welcome role
- **title (fictional):** "Complete registration-desk step: badge-packet check"
- **carries ids, not contact info** — the delivery channel comes from the private overlay and never reaches this repo

> **Schema-vs-package note.** The NYCN package's `complete-action-item` template (`institution/action-card-templates.example.yaml`) lists richer `source_kind`s (`program_milestone`, `committee_log`). Those are **not** in the current ICN-core emitted `source_kind` enum and are **not** reserved RFC-gated values — modeling this lane against them would point implementers at an invalid ActionCard. Treat any such richer source-kind as a **future schema / source-kind change** (a separate ADR/RFC + `action-card.schema.json` update), not as something emittable today.

The card would surface on the holder's member shell via the declared `GET /gov/me/action-cards` route (declaration only — see proof discipline below).

## Authorized completion action

A holder with the registration/welcome `RoleAssignment` (a `Capability`/`authority_scope` in the logistics `Structure`) marks the step **complete** or **escalated**. Authorization is by role, not by DID ownership alone (per the auth doctrine — a capability token is not a mandate). In this map it is a **fictional** completion: the lane lead marks a fictional step done.

## Receipt candidate

Completion of an `action_item/complete` card maps to the generic, proof-bearing `ActionItemCompletionReceipt` path ([runtime-surface-map](../reference/project-index/runtime-surface-map.md): `action_item / complete → ActionItemCompletionReceipt`, proof loop verified end-to-end **generically**). The receipt is retrievable via the declared completion-receipt route. Honest nuance: the NYCN `complete-action-item` template sets `receipt_expected` per item — not every operational step is receipt-bearing; receipt-bearing registration steps are the ones a future package binds to a milestone.

## Evidence export candidate

A repo-safe evidence-export candidate: basenames + status categories only, e.g. `registration-desk: complete` / `walk-in-questions: 1 escalated (category)`, conforming to the rehearsal-evidence-export shape. Maps to the spine's `evidence/export` terminal node. No roll, no names, no counts tied to identities.

## Post-event follow-up item

A fictional follow-up shape, e.g. "review walk-in escalation handling for next cycle" — a package-side close-stage item (the NYCN package may track these as committee-log / milestone categories; those are package vocabulary, not ICN-core ActionCard `source_kind`s). No person-identifying content.

## Minimal fixture-backed walkthrough

```text
fictional registration source packet
  → facilitator reviews the checklist (categorical steps, plain language)
  → steward/operator confirms fixture/example mode (nothing live)
  → a registration action-card candidate is shown (schema-valid action_item/complete shape)
  → fictional lane lead marks completion or escalation
  → receipt/evidence candidate is described (ActionItemCompletionReceipt + repo-safe export)
  → a post-event follow-up item is produced
```

This is a **shape and rehearsal path**, not running software. The closest existing surface is the fixture-backed rehearsal shell (`fixture-backed`, L2 — [capability matrix](../reference/project-index/proof-level-taxonomy-capability-matrix.md) row 7); wiring it to render this lane's register is the open milestone ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)).

## ICN route / command touchpoints

Declarations only — from the generated inventories, which prove **L1** (a declaration exists in source), not correctness, auth, live wiring, or readiness:

| Touchpoint | Source | Inventory proof | Stronger recorded proof |
|---|---|---|---|
| `GET /gov/me/action-cards` | governance app (`get_my_action_cards`) | L1 | L5 live for emitted source paths (matrix row 8) — generic, not this lane |
| `GET /gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` | governance app (`get_action_item_completion_receipt`) | L1 | L5 receipt-chain path (matrix rows 1, 8) — generic |
| `icnctl audit verify` | `icnctl` (`main.rs:330`) | L1 | part of the L4/L5 receipt-chain path (matrix rows 1, 5) |
| `icnctl receipts chain` | `icnctl` (`main.rs:281`) | L1 | part of the L4/L5 receipt-chain path |

Higher proof exists **only** where the [capability matrix](../reference/project-index/proof-level-taxonomy-capability-matrix.md) records it, and only **generically** (on fictional fixtures) — never for this registration lane specifically.

## Proof level

- This map: **L1** (a declared shape / mapping exists in source).
- The generic `action_item/complete → ActionItemCompletionReceipt` loop: **L5** generic proof on fictional fixtures (matrix rows 1/8).
- The NYCN registration-lane loop end-to-end: **`planned`** — *not exercised*. `icn_target_status: planned` (the action-card path is `partially-supported` only generically).

Because this PR is docs-only, it is a **proof-loop map / rehearsal-ready shape**, not a runtime proof. No fixture/proof command was run for this lane.

## Privacy boundaries

Never committed: real attendee names; real registration roll; real emails; phone numbers; accessibility/accommodation details; medical details; payment/settlement details; raw Google Docs/Sheets/Groups exports; credentials/tokens; private Drive URLs. Registration Desk is `attendee-restricted`. Rehearsal privacy is by **exclusion** (fictional fixtures), not enforced disclosure (`private-boundary`; enforcement is design-only, L1 — matrix row 9). Attendance is modeled as **count categories**, never a roll.

## What can be fixture-backed now

- The fictional registration source packet + checklist shape.
- The action-card candidate shape (schema-valid `action_item`/`complete`), with fictional title and id-only payload.
- The receipt/evidence candidate shapes (basenames + status categories).
- A no-terminal walkthrough driven by committed fixtures (the existing fixture-backed shell, L2).

## What remains planned / unknown

- A live registration action-card → receipt loop for a real (or fictional-in-runtime) summit — `planned`; not exercised end-to-end for NYCN.
- A node-hosted Summit Ops cockpit — `docs-only / design-direction`.
- Enforced disclosure for any attendee/registration data — design-only (L1); today it is exclusion-only.

## Nonclaims

- Does not claim the Registration Desk path ran at a real summit.
- Does not claim a node-hosted Summit Ops cockpit exists.
- Does not claim live NYCN action cards/receipts exist for the summit.
- Does not claim organizer readiness, a formal NYCN pilot, production readiness, live federation, or Phase 2 completion.
- Does not sync Google data; does not mutate the NYCN repo.
- Does not commit private attendee, sponsor, accessibility, volunteer, incident, registration, or payment data.
- Does not change route behavior, `icnctl` behavior, authn/authz, or OpenAPI; does not regenerate SDK types.
- Does not affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082); does not start A2e.

## Next slices

1. Render this lane's action register in the fixture-backed shell (still fixture-only, no live data).
2. Map a second lane's proof loop (e.g. room monitors or closing/cleanup) the same way.
3. Build the post-event closeout / continuity packet (the "close the loop" stage).
4. When a local build is available, exercise the generic `action_item/complete` receipt loop once against fictional inputs and record evidence (moves this lane off `planned`).

## Where to read deeper

| You want… | Read |
|---|---|
| The event-day run-stage facilitator path (parent) | [`summit-ops-run-stage-facilitator-path.md`](summit-ops-run-stage-facilitator-path.md) |
| The full event lifecycle map | [`summit-ops-lifecycle-package-map.md`](summit-ops-lifecycle-package-map.md) |
| Recorded proof per capability | [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| Real runtime surfaces (action cards, receipts) | [`runtime-surface-map.md`](../reference/project-index/runtime-surface-map.md) |
| The ICN spine + meaning firewall | [`docs/architecture/ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md) |
| The ICN ↔ NYCN ↔ public boundary | [`docs/ATLAS.md`](../ATLAS.md) |
| The milestone + spine | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
