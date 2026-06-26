---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Summit Ops Closeout Continuity Packet (generic ICN)

> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). This is a **generic ICN-side, docs-only** map of the **"close the loop"** lifecycle stage from the [Summit Ops lifecycle package map](summit-ops-lifecycle-package-map.md): how a package turns post-event work into repo-safe structures and **future** ICN evidence candidates. It commits no fixtures, syncs no Google surface, mutates no partner repo, touches no pilot-UI code, and is **L1 declared shapes / rehearsal-ready — not fixture-backed and not runtime proof.**

## Purpose

After the event, organizing has to *close the loop*: reconcile what happened, route follow-ups, thank people, draft the public recap, and hand a continuity packet to next year — all without leaking private participant, sponsor, financial, or incident detail. This packet maps that work, lane by lane, onto repo-safe package shapes and the ICN vertical spine's terminal nodes (`receipt → surface → evidence/export`), so the close stage is legible and rehearsal-ready before any runtime exists.

It answers: 1. What happened? · 2. What needs follow-up? · 3. Who owns each follow-up category? · 4. What evidence can be exported safely? · 5. What must remain private? · 6. What becomes a future `ActionCard`/`Receipt`/`EvidenceExport` candidate? · 7. What can be fixture-shaped now? · 8. What remains planned/unknown? · 9. What proof level is honestly supported? · 10. What must not be claimed?

## Boundary

The three planes from the [lifecycle map](summit-ops-lifecycle-package-map.md) stay apart: **Google + human** (post-event surveys, debriefs, financials, sponsor correspondence — live, never synced/committed) · **NYCN package repo** (repo-safe **shapes**, fictional/categorical placeholders; standalone `nycn` is the active home, in-monorepo `institutions/nycn/` is scaffold pending reconciliation) · **future ICN node** (`Receipt` retrieval + `evidence/export`; no Summit Ops cockpit today). Per [`docs/ATLAS.md`](../ATLAS.md), real closeout operating detail stays in the private package; this doc uses generic event-operating vocabulary + categorical examples only.

## Why closeout next

The registration lane (run stage) is now L2 fixture-backed (#2209), and the [proof-level matrix](../reference/project-index/proof-level-taxonomy-capability-matrix.md) records it. Closeout is the next safe slice because it **continues the event lifecycle (plan → prepare → run → close) without touching pilot-UI behavior** while [#2099](https://github.com/InterCooperative-Network/icn/issues/2099) (CodeQL DOM-XSS / missing-SRI in pilot UI) remains open and gates pilot-UI surface expansion. This packet is docs-only by design.

## What this packet is for

- A coherent, plain-language close-the-loop story a facilitator can run without a terminal.
- Repo-safe **shapes** for post-event work that map to future ICN action cards / receipts / evidence — so closeout is reproducible and auditable later, not improvised each cycle.

## What this packet is not

- Not a new app, not running software, not a node-hosted Summit Ops cockpit.
- Not a live Google sync, not a NYCN-repo mutation, not a production event workflow.
- Not a runtime proof, not fixture-backed (no fixtures committed here), not organizer-ready (L7), not a formal pilot.

## Closeout lanes

Each lane maps the same seven fields. Status labels reuse the existing vocabulary ([source-of-truth map](../reference/project-index/source-of-truth-map.md)); the ICN-core target is uniformly an `Activity`/`RoleAssignment` emitting `ActionItem`→`ActionCard`→`Receipt`→`EvidenceExport` (planned, ICN #1608), stated once with per-lane nuance below.

| Lane (package vocabulary) | Current Google/human source | Repo-safe package shape | Possible future ActionCard | Possible Receipt / EvidenceExport | Privacy class |
|---|---|---|---|---|---|
| **Attendance / registration summary** | registration roll, check-in counts | count **categories/bands** only (never a roll) | "confirm attendance-category summary" | attendance-summary receipt; categorical export | `attendee-restricted` |
| **Speaker / presenter follow-up** | speaker list, thank-you/feedback | follow-up task shape; session→presenter role mapping | "send presenter follow-up for session X" | follow-up-complete receipt | `committee-internal` |
| **Sponsor / vendor follow-up** | sponsor contacts, renewal decisions | follow-up/renewal **category** shape (no contacts) | "record sponsor follow-up outcome (category)" | follow-up receipt | `sponsor-restricted` |
| **Reimbursements / obligations** | receipts, payouts | obligation **category** shape; **no amounts** (fictional only) | "close reimbursement obligation (category)" | obligation-closed receipt; `Obligation`/`Allocation` (categorical) | `committee-internal` |
| **Accessibility / language justice lessons** | accommodation/interpretation notes | lesson **category** shape (no person, no accommodation detail) | "log accessibility lesson (category)" | lesson-recorded receipt | `attendee-restricted` |
| **Incident / escalation closeout** | incident log, escalations | incident **category + status** only (no identifying detail) | "close incident category C" | escalation/repair receipt | `organizer-internal` |
| **Volunteer appreciation / follow-up** | volunteer roster, thank-yous | role/shift follow-up shape, fictional ids | "send volunteer appreciation (role)" | appreciation-sent receipt | `organizer-internal` |
| **Budget reconciliation checklist** | budget vs actuals | checklist shape; **no real amounts** | "complete budget-reconciliation step" | reconciliation-step receipt | `committee-internal` |
| **Public recap draft handoff** | recap doc, comms calendar | recap-draft **template** + handoff step (public-safe) | "hand off public recap draft" | handoff receipt | `public` |
| **Next-year continuity packet** | "what to remember for next time" | continuity-packet **template** (carries shapes, not data) | "assemble next-year continuity packet" | continuity-packet receipt | `organizer-internal` |
| **Evidence export candidate** | scattered closeout artifacts | repo-safe export shape (basenames + status categories) | — | `EvidenceExport` (spine terminal node) | `organizer-internal` |
| **Open follow-up action register** | the running "who-owes-what" list | action-register shape (lane, owner role, status) | "complete open follow-up item" | item-complete receipt | `organizer-internal` |

**Per-lane nuance (highest-privacy lanes):**

- **Attendance / registration summary** and **Accessibility / language justice lessons** carry `attendee-restricted` data. The repo-safe shape models **count categories / lesson categories** only — never a roll, a name, a medical detail, or an accommodation tied to a person.
- **Incident / escalation closeout** models incident **categories and status** only — no person-identifying detail, no narrative that could identify anyone.
- **Reimbursements / obligations** and **Budget reconciliation** model **categories and checklist status** only — **no real amounts** (any figure shown must be fictional). `Obligation`/`Allocation` appear only as categorical, clearly-fictional future mappings.
- **Sponsor / vendor follow-up** models outcome categories only — no sponsor contacts or contract amounts.

Meaning firewall: "speaker thank-you," "sponsor follow-up," "volunteer appreciation," "budget reconciliation," "public recap" are **package/event vocabulary**; ICN core stays generic — `Activity`, `Structure`, `RoleAssignment`, `ActionItem`, `ActionCard`, `Receipt`, `EvidenceExport`, `PrivateOverlay`.

## Minimal no-terminal walkthrough

```text
closeout source packet (categorical, fictional)
  → facilitator reviews the closeout lane checklist (plain language)
  → steward/operator confirms fixture/example mode (nothing live)
  → closeout action register shows open post-event follow-ups
  → lane owner marks a follow-up complete or escalates
  → evidence register captures repo-safe basenames + status categories
  → next-year continuity packet is produced
```

A **shape and rehearsal path**, not running software, and **no closeout fixtures are committed** in this slice (the only committed Summit Ops fixture is the registration card, #2209, L2). Wiring any of this into the shell is the open milestone ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)) and would touch pilot-UI surface — gated on [#2099](https://github.com/InterCooperative-Network/icn/issues/2099).

## Closeout source packet

The repo-safe inputs a facilitator/steward assembles (shapes only, fictional/categorical): the closeout lane checklist above, the post-summit report **template**, the debrief/decision-record template, and the continuity-packet template. Real survey/financial/contact contents stay off-repo (Google + private overlay).

## Closeout action register

Open post-event follow-ups, each: lane, short description, responsible role id (placeholder), status (`open`/`done`/`blocked`/`escalated`). Maps to future ICN `ActionItem`/`ActionCard` instances (planned, #1608). No real names; role ids are placeholders bound via the private overlay at runtime.

## Closeout decision / escalation register

Closeout decisions/escalations, each: trigger category, owner role, outcome status. Maps to future `Proposal`/`Vote` or escalation receipts. The **incident** path records categories + status only — never person-identifying detail.

## Closeout evidence register

Repo-safe evidence candidates: basenames + status categories only (e.g. `budget-reconciliation: complete`, `public-recap: handed-off`), conforming to the rehearsal-evidence-export shape. Maps to the spine's `evidence/export` terminal node. No raw contents, no amounts, no attendee/sponsor/incident specifics.

## Future ICN mapping

```text
closeout lane (Activity sub-step, package vocabulary)
  → RoleAssignment (which committee role owns the follow-up — generic, id-bound at runtime)
  → ActionItem / ActionCard (the open follow-up delivered to the holder; planned, ICN #1608)
  → authorized action (lane owner marks done / escalates)
  → Receipt (durable proof the closeout step occurred)
  → EvidenceExport (repo-safe basenames + status categories)
```

`icn_target_status`: **planned** for the full closeout loop; the generic `action_item/complete → ActionItemCompletionReceipt` path has L5 generic proof on fictional fixtures (matrix rows 1/8), but the closeout lanes have **no committed fixtures and no exercised loop**. Action cards carry **ids, not contact info**; delivery channel comes from the private overlay and never reaches the repo.

## Privacy boundaries

Never committed, in any form: real attendee names; registration rolls; emails; phone numbers; accessibility/accommodation details; medical details; incident details that identify a person; sponsor contact details; real reimbursement/payment details; private Drive URLs; raw Google Docs/Sheets/Groups exports; credentials/tokens. Closeout privacy is by **exclusion** (categorical, fictional shapes), not enforced disclosure (`private-boundary`; enforcement is design-only, L1 — matrix row 9). This mirrors the NYCN follow-up workflow's explicit consent control and its "deliberately excludes" boundary.

## What can be fixture-shaped now

- The closeout lane checklist + register **shapes** (generic, fictional/categorical), as docs.
- The post-summit report / debrief / continuity-packet **templates** (shapes, no data).
- Evidence-export candidate **shape** (basenames + status categories).

These are **doc-level shapes**, not committed JSON fixtures. A future slice could commit a fictional closeout fixture (mirroring the registration card) and validate it — but that is not done here.

## What remains planned / unknown

- Committed closeout-lane fixtures and any shell rendering — `planned`; gated on [#2099](https://github.com/InterCooperative-Network/icn/issues/2099) before pilot-UI surface work.
- A live closeout action-card → receipt loop — `planned`; not exercised.
- Enforced disclosure for attendee/incident/financial closeout data — design-only (L1); today exclusion-only.

## Proof level

- This packet: **L1** (declared shapes / mappings exist in source) — docs-only, **not fixture-backed**.
- The registration card (separate lane, #2209): **L2** fixture/demo — the only committed Summit Ops fixture.
- Generic `action_item/complete → receipt` loop: **L5** generic (matrix rows 1/8) — not this lane.
- The NYCN closeout loop end-to-end: **`planned`** — not exercised.

Bounded: this is a rehearsal-ready shape map, **not** runtime proof, **not** live NYCN, **not** organizer-ready (L7), **not** production.

## Nonclaims

- Does not change route behavior, `icnctl` behavior, authn/authz, or OpenAPI; does not regenerate SDK types.
- Does not touch pilot-UI behavior (no `web/pilot-ui` change); #2099 is unaffected by this PR.
- Does not add runtime proof; does not claim fixture-backed (no fixtures committed here).
- Does not claim organizer readiness, a formal NYCN pilot, production readiness, live federation, or Phase 2 completion.
- Does not sync Google; does not mutate the NYCN repo.
- Does not commit private attendee, sponsor, accessibility, volunteer, incident, registration, reimbursement, or payment data.
- Does not affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082); does not start A2e.

## Next slices

1. Optionally commit one fictional closeout fixture (mirroring the registration card) and validate it → would move a closeout lane to L2.
2. Shell-render a closeout register — **only after [#2099](https://github.com/InterCooperative-Network/icn/issues/2099)** is handled (pilot-UI surface).
3. #2113 — per-command `icnctl` live/partial/fixture/planned status classification.

## Where to read deeper

| You want… | Read |
|---|---|
| The full event lifecycle map (parent) | [`summit-ops-lifecycle-package-map.md`](summit-ops-lifecycle-package-map.md) |
| The run-stage facilitator path | [`summit-ops-run-stage-facilitator-path.md`](summit-ops-run-stage-facilitator-path.md) |
| The registration proof loop + its L2 fixture | [`summit-ops-registration-action-card-proof-loop.md`](summit-ops-registration-action-card-proof-loop.md) |
| The Public Recap Draft Handoff fixture shape (next-slice spec) | [`summit-ops-closeout-recap-fixture-shape.md`](summit-ops-closeout-recap-fixture-shape.md) |
| Recorded proof per capability | [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| The ICN spine + meaning firewall | [`docs/architecture/ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md) |
| The ICN ↔ NYCN ↔ public boundary | [`docs/ATLAS.md`](../ATLAS.md) |
| The milestone + spine | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
