---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Summit Ops Run-Stage Facilitator Path (generic ICN)

> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). This is a **generic ICN-side** map: the first concrete child of the [Summit Ops lifecycle package map](summit-ops-lifecycle-package-map.md), taking its **"run the summit"** stage and describing the minimum event-day operating path that can be *rehearsed* — without claiming a node-hosted Summit Ops cockpit exists. It changes no code, mutates no partner repo, syncs no Google surface, and commits no private event-day data.

## Purpose

On event day, organizing is at its most live, most human, and most error-prone. This map describes a **fixture-backed, no-terminal facilitator path** for the run stage: how event-day operations can be turned into package-ready structures, future ICN action cards, and receipt/evidence candidates — so a facilitator can run the day from a coherent story, and a steward can later reproduce the same path from committed fixtures.

It answers, for the run stage:

1. What is happening right now? · 2. What happens next? · 3. Who is responsible? · 4. What room/surface is involved? · 5. What is late or blocked? · 6. What needs escalation? · 7. What changed from the plan? · 8. What was completed? · 9. What needs post-event follow-up? · 10. What can become a future ICN action card / receipt / evidence item?

## Boundary

This map keeps the three planes from the [lifecycle map](summit-ops-lifecycle-package-map.md) apart:

- **Google + in-person (live, today):** real run-of-show, real role assignments, real check-in, real-time decisions. Stays where it lives; never synced or committed.
- **NYCN package repository (repo-safe shapes):** event-day **templates and fixtures** — generic shapes only, fictional placeholders only — primarily in the standalone `InterCooperative-Network/nycn` repo (the in-monorepo `institutions/nycn/` is boundary-sensitive scaffold pending reconciliation).
- **Future ICN node (live runtime):** `ActionCard` → authorized action → `Receipt` rendered on a member shell / steward cockpit `Surface`. Does **not** exist as a Summit Ops cockpit today.

Per [`docs/ATLAS.md`](../ATLAS.md), NYCN private operating detail (real rosters, real schedules, the live sheet's lane inventory) stays in the private `nycn` package and is referenced here only at boundary level. The event-day **lanes** below are generic event-operating vocabulary (every convening has them), not NYCN-specific operating detail.

## Current surfaces today

Event-day coordination today runs on human and Google surfaces: shared run-of-show docs, role/shift sheets, group chats / mailing lists, printed materials, radios/phones, and the people in the room. None of this is in the repo; none of it is synced.

## What this path is for

- Giving a facilitator a coherent, plain-language story to run the day without a terminal.
- Giving a steward a reproducible, fixture-backed rehearsal of the same path.
- Turning event-day work into **package-ready shapes** and **future** ICN action-card / receipt / evidence candidates — so the run stage is legible and auditable later, not improvised from scratch each cycle.

## What this path is not

- Not a new app, not software that exists yet, not a node-hosted Summit Ops cockpit.
- Not a live Google sync, not a NYCN-repo mutation, not a production or event-day guarantee.
- Not a claim that this path has been executed at a real summit.

## Event-day operating lanes

Each lane maps the same seven fields. Status labels reuse the existing vocabulary ([source-of-truth map](../reference/project-index/source-of-truth-map.md)); proof levels follow the [proof taxonomy](../reference/project-index/proof-level-taxonomy-capability-matrix.md) (a declared template/shape is **L1**). The ICN-core target is uniformly an `Activity`/`RoleAssignment` emitting `ActionCard`→`Receipt` (planned, ICN #1608), so the table states it once and notes per-lane nuance.

| Lane (package vocabulary) | Current Google/human source | Repo-safe package shape | Possible future ActionCard | Possible receipt/evidence | Privacy class |
|---|---|---|---|---|---|
| **Registration desk** | check-in list, walk-up handling | check-in/role shape; arrival as an event step | "staff the registration desk" assignment | attendance/role-completion receipt | `attendee-restricted` |
| **Room monitors** | per-room run-of-show, transitions | room/session role shape, shift slots | "monitor room X for block Y" assignment | session-transition completion receipt | `organizer-internal` |
| **Speaker / presenter support** | speaker confirmations, AV needs, green-room | session-intake + presenter-support shape | "support presenter for session Z" assignment | presenter-support completion receipt | `committee-internal` |
| **Accessibility and language justice** | interpretation, captions, accommodations, alt formats | accessibility-intake field shape (request *kinds*, not people) | "fulfill accessibility request kind K" assignment | accommodation-fulfilled receipt | `attendee-restricted` |
| **Food / vendor coordination** | catering windows, dietary needs, vendor timing | vendor/timing shape (no contracts/amounts) | "confirm food service window" assignment | service-window completion receipt | `partner-restricted` |
| **Tech / A/V** | tech list, equipment, setup/teardown | tech-requirements shape (kinds, not credentials) | "set up A/V for room X" assignment | A/V-ready completion receipt | `organizer-internal` |
| **Volunteer coordination** | volunteer shifts, check-in, reassignment | role/shift shape, fictional placeholder ids | "fill open shift S" assignment | shift-completion receipt | `organizer-internal` |
| **Sponsor / tabling support** | table assignments, sponsor logistics | tabling-logistics shape (no sponsor contacts/amounts) | "set up sponsor table T" assignment | tabling-ready completion receipt | `sponsor-restricted` |
| **Incident / escalation log** | radio/phone, on-site judgment | incident **category** shape (no person-identifying detail) | "escalate incident category C" alert card | escalation/repair receipt | `organizer-internal` |
| **Closing / cleanup** | teardown checklist, lost-and-found, sign-out | closing-checklist shape | "complete closing task T" assignment | closing-completion receipt | `organizer-internal` |

**Per-lane nuance (boundary-sensitive lanes):**

- **Accessibility and language justice** carries `attendee-restricted` data (accommodation requests). The repo-safe shape models request *kinds* and fulfillment status — never a person, a medical detail, or a specific accommodation tied to a name.
- **Incident / escalation log** is the highest-risk lane: the repo-safe shape models incident *categories* and escalation/repair status only. No person-identifying detail, no narrative that could identify anyone, ever enters the repo. Real incident handling stays human and off-repo.
- **Sponsor / tabling** and **Food / vendor** model logistics shape only — no sponsor/vendor contacts, no contract amounts.

Meaning firewall: "registration desk," "room monitor," "speaker support," "language justice," "venue," "sponsor table" are **package/event vocabulary**; ICN core stays generic — `Activity`, `Structure`, `ActionItem`, `ActionCard`, `Receipt`, `Evidence`, `RoleAssignment`. Event-ceremony nouns never enter ICN core.

## Minimal no-terminal walkthrough

The facilitator path a steward can rehearse from committed fixtures, no terminal required:

```text
event-day run packet
  → facilitator reviews the lane checklist (plain language, per lane above)
  → steward/operator confirms fixture/example mode (nothing live)
  → action register shows open event-day tasks (from fixtures)
  → lane lead marks completion or escalation
  → evidence register captures repo-safe basenames + status categories
  → post-event follow-up register is produced
```

This is a **shape and rehearsal path**, not running software. The fixture-backed rehearsal shell (`fixture-backed`, L2 — [capability matrix](../reference/project-index/proof-level-taxonomy-capability-matrix.md) row 7) is the closest existing surface; wiring it to a live run is the open milestone ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)).

## Run-stage source packet

The repo-safe inputs a facilitator/steward would assemble (shapes only, fictional placeholders only): the day-of role/shift shape, the run-of-show template, the tech-requirements shape, the accessibility-request-kind shape, and the lane checklist above. These live as `*.example.yaml` package fixtures in the NYCN repo (e.g. day-of roles, tech, accessibility) and as `*.private.yaml` overlay **hooks** the public exporter never reads. Referenced here at boundary level — content stays in the package.

## Run-stage action register

A list of open event-day tasks, each: lane, short description, responsible role id (placeholder), room/surface, status (`open` / `done` / `blocked` / `escalated`). Maps to future ICN `ActionItem`/`ActionCard` instances (planned, #1608). No real names; role ids are placeholders bound via the private overlay at runtime.

## Run-stage decision / escalation register

Event-day decisions and escalations, each: trigger category, decision/owner role, outcome status. Maps to future `Proposal`/`Vote` or escalation receipts. The **incident** path records categories and status only — never person-identifying detail.

## Run-stage evidence register

Repo-safe evidence candidates: basenames + status categories only (e.g. `closing-checklist: complete`), conforming to the rehearsal-evidence-export shape. Maps to the spine's `evidence/export` terminal node. No raw contents, no attendee/sponsor/accessibility/incident specifics.

## Future ICN mapping

```text
event-day lane (Activity sub-step, package vocabulary)
  → RoleAssignment (who is responsible — generic, id-bound at runtime)
  → ActionItem / ActionCard (the open task delivered to the holder; planned, ICN #1608)
  → authorized action (lane lead marks done / escalates)
  → Receipt (durable proof the step occurred)
  → EvidenceExport (repo-safe basenames + status categories)
```

`icn_target_status`: **planned** for the full loop; **partially-supported** only for the generic action-item path. The generic decision→action→receipt loop has live single-node proof on **fictional** fixtures (L5, matrix rows 1/8), but the NYCN package has **not** exercised it end-to-end (per the package's own proof matrix). Action cards carry **ids, not contact info**; the delivery channel comes from the private overlay and never reaches the repo.

## Privacy and safety boundaries

Never committed, in any form: real attendee names; real registration roll; real accessibility/accommodation details; medical details; phone numbers; private emails; sponsor contact details; incident details that identify a person; raw Google Docs/Sheets/Groups exports; credentials/tokens; private Drive URLs. Rehearsal privacy is by **exclusion** (fictional fixtures, repo-safe-by-construction), not by enforced disclosure (`private-boundary`; enforcement is design-only, L1 — matrix row 9). The six privacy classes (`public`, `organizer-internal`, `committee-internal`, `partner-restricted`, `attendee-restricted`, `sponsor-restricted`) are inherited from the NYCN operating-surfaces boundary policy.

## What can be fixture-backed now

- The lane checklist + run packet **shape** (generic, fictional placeholders).
- A no-terminal facilitator walkthrough driven by committed fixtures (the existing fixture-backed shell, L2).
- Action/decision/evidence register **shapes** with fictional rows.

## What remains planned / unknown

- Live action cards / receipts for a real summit run — `planned`; not exercised end-to-end for NYCN.
- A node-hosted Summit Ops cockpit — `docs-only / design-direction`.
- Enforced disclosure for any attendee/accessibility/incident data — design-only (L1); today it is exclusion-only.

## Nonclaims

- Does not claim a node-hosted Summit Ops cockpit exists.
- Does not claim the run-stage path has been executed at a real summit.
- Does not claim organizer readiness, a formal NYCN pilot, production readiness, live federation, or Phase 2 completion.
- Does not sync Google data; does not mutate the NYCN repo.
- Does not commit private attendee, sponsor, accessibility, volunteer, incident, or registration data.
- Does not change route behavior, `icnctl` behavior, authn/authz, or OpenAPI; does not regenerate SDK types.
- Does not affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082); does not start A2e.

## Next slices

1. Exercise one **fictional** NYCN package action-card proof loop end-to-end for a single run-stage lane (move it off `planned`).
2. Build the post-event closeout / continuity packet (the "close the loop" stage's facilitator path).
3. Wire the fixture-backed shell to render one lane's action register (still fixture-backed, no live data).

## Where to read deeper

| You want… | Read |
|---|---|
| The full event lifecycle map | [`summit-ops-lifecycle-package-map.md`](summit-ops-lifecycle-package-map.md) |
| A single lane walked through the ICN proof loop (fictional, registration desk) | [`summit-ops-registration-action-card-proof-loop.md`](summit-ops-registration-action-card-proof-loop.md) |
| What is safe to show an organizer now | [`organizer-rehearsal-operability-map.md`](../reference/project-index/organizer-rehearsal-operability-map.md) |
| Recorded proof per capability | [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| The ICN spine + meaning firewall | [`docs/architecture/ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md) |
| The ICN ↔ NYCN ↔ public boundary | [`docs/ATLAS.md`](../ATLAS.md) |
| The milestone + spine | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
