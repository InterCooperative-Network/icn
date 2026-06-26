---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Summit Ops Lifecycle Package Map (generic ICN)

> For current project truth, defer to [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md). This is a **generic ICN-side** lifecycle/product map: how an institution package (NYCN being the motivating example) can carry an event's full lifecycle — plan, prepare, run, close — onto the ICN vertical spine while preserving the ICN ↔ package ↔ Google boundary. It changes no code, mutates no partner repo, syncs no Google surface, and claims no formal pilot.

## Purpose

The New York Cooperative Summit is the first real pressure test for ICN's institutional operating model across an entire event lifecycle, not just a single proof loop:

```text
plan the summit → prepare the summit → run the summit → close the loop after the summit
```

This map answers, claim-safely: **for each lifecycle stage, where does the work live today, what shape can a repo-safe package hold, which ICN primitive / spine node would eventually carry it, what proof stands behind that, and what must not be claimed.** It exists so that future Summit Ops work lands against the ICN vertical spine ([#2141](https://github.com/InterCooperative-Network/icn/issues/2141)) and the organizer-rehearsal milestone ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)) instead of drifting into a bespoke app or an overclaim.

It is a sibling of the [organizer rehearsal operability map](../reference/project-index/organizer-rehearsal-operability-map.md): that map asks "what is safe to show an organizer now"; this one asks "how does an event's lifecycle map onto package + runtime over time."

## The three planes (do not collapse them)

The single most important thing this map protects is the boundary between three planes. Each owns different things; conflating them is the overclaim this document exists to prevent.

| Plane | What it is | What it owns | What it is **not** |
|---|---|---|---|
| **Google organizing reality** (live, today) | Google Drive / Docs / Sheets / Groups, the public site + registration, newsletters, meetings, email, committee/steward work | The actual live summit coordination — drafts, rosters, sponsor pipeline, attendee data, decisions-in-progress | Not in the repo. Not synced. Not replaced by git. |
| **NYCN package repository** (repo-safe shapes) | Primarily the standalone `InterCooperative-Network/nycn` repo (the active package home): schemas, fixtures, mappings, validators, templates, runbooks, bootstrap/export artifacts, private-overlay *hooks*, package-local Summit structure. The in-monorepo `institutions/nycn/` directory is boundary-sensitive package-local **scaffold** pending reconciliation — not current operational truth (see [`source-of-truth-map.md`](../reference/project-index/source-of-truth-map.md) and [`source-tree-map.md`](../reference/project-index/source-tree-map.md)). | Repo-safe **shapes** — names, relationships, stewardship, scope, privacy class, ICN object mappings — and the bootstrap/export logic | Not a live organizing workspace. Organizers do **not** coordinate the summit in git. No private planning data lives here. |
| **Future ICN node** (live runtime) | A node that imports the NYCN bootstrap package and runs the substrate | Live state: standing, action cards, receipts, provenance, role/capability-aware surfaces, node-hosted Summit Ops workflows | Does **not** exist as a node-hosted Summit Ops cockpit today. Its arrival is future, not current. |

**The correct pipeline** runs left-to-right; nothing skips the human in the middle:

```text
Google organizing reality
  → workflow extraction (observe the shape, not the data)
  → human review (steward decides what is safe + useful)
  → better organizing process now (the near-term payoff)
  → repo-safe NYCN package shape (schemas / fixtures / mappings)
  → bootstrap/export logic (package → node manifest)
  → future NYCN-on-ICN node (live runtime)
```

The first useful product test is **whether this reduces organizer burden now**, not whether it introduces ICN as a system. (This mirrors the NYCN package's own order-of-work doctrine in its `NYCN_WHOLE_SYSTEM_MODEL.md`.)

## The ICN vertical spine this maps onto

From [`docs/architecture/ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md) ([#2141](https://github.com/InterCooperative-Network/icn/issues/2141)):

```text
package → domain → policy → binding → process/action → receipt → surface → evidence/export
```

Summit Ops is **package-level meaning** (`Package` in the ICN vocabulary). It localizes; ICN core holds the generic grammar. The Summit's committees are `Structure`s; the Summit cycle and its lanes are `Activity`s owned by the NYCN organizing-cooperative `Entity`; participation flows through `ActionCard` → authorized action → `Receipt`. None of that requires inventing new ICN core primitives — it reuses Entity / Structure / Activity / ActionItem / Receipt, exactly as the NYCN object map (`institution/mappings/icn-object-map.yaml`) already declares.

## The meaning firewall (what is package vs ICN core)

The firewall is enforced mechanically as **crate-layering**: `.github/scripts/firewall_denylist.py` verifies that kernel crates do not depend (directly or transitively) on domain/app crates. The vocabulary placement below is the **doctrine** that layering protects — the human-readable form in [`ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md), not a literal word-scan:

- **ICN core may know (generic grammar):** `Entity`, `Structure`, `Activity`, `Program`, `Milestone`, `Meeting`, `ActionItem`, `ActionCard`, `Proposal`, `Vote`, `RoleAssignment`, `Standing`, `AuthorityGrant`, `Obligation`, `Allocation`, `Settlement`, `Artifact`, `Vault`, `Receipt`, `Agreement`.
- **The NYCN package may know (local vocabulary):** *summit*, *sponsor packet*, *sponsor pipeline*, *speaker / session intake*, *venue walkthrough*, *accessibility intake*, *run-of-show*, *day-of roles*, *tech list*, *content committee*, *summit stage gates*.

If a Summit-ceremony noun (e.g. `SponsorPacket`, `VenueLocked`, `RunOfShow`) is ever about to enter generic ICN runtime — stop; it belongs in the package. This map names those words only as examples of what stays package-local.

## The lifecycle, stage by stage

Each stage names: **where the work lives today** (Google plane), **the repo-safe package shape** (NYCN plane), **the eventual ICN spine node / primitive** (future-node plane, with honest `icn_target_status`), and **what must not be claimed**. Proof levels follow the [proof-level taxonomy](../reference/project-index/proof-level-taxonomy-capability-matrix.md) (L0–L8); a declared shape or mapping is **L1** unless the capability matrix records higher.

### 1. Plan the summit

- **Today (Google):** a central planning spreadsheet organized into committee/function lanes, committee Docs, organizer-only Groups, real meetings. (The live spreadsheet's specific lane inventory is NYCN operating detail and stays in the private `nycn` package, referenced here only at boundary level per [`docs/ATLAS.md`](../ATLAS.md).)
- **Repo-safe package shape:** the Summit cycle modeled as an `Activity` with sub-`Activity` lanes; committees as `Structure`s; role/steward shapes; a **column-mapping config** describing what *would* be syncable (the existing `sheets_mapping` shape) — never the sheet contents. Budget/program/logistics/accessibility lanes already have `*.example.yaml` shapes under the package's `summit/2026/`.
- **Eventual ICN node:** `Activity` + `Structure` + `RoleAssignment` under the organizing-cooperative's governance `Domain`. `icn_target_status: planned`.
- **Must not claim:** that planning happens in git; that any sheet is synced; that committee membership or budget figures are in the repo.

### 2. Prepare the summit

- **Today (Google):** content/session confirmations, sponsor outreach pipeline, registration setup, marketing calendar, venue/vendor coordination, accessibility/interpretation requests — all in Docs/Sheets/Groups/email.
- **Repo-safe package shape:** templates and validators (sponsor-tier template, session-intake field model, accessibility-intake field model, day-of-role list, tech-requirements list) plus the **operating-surfaces inventory** (which surface holds which work, who stewards it, which of the six privacy classes applies) — shapes only, fictional placeholders only. Private specifics live behind `*.private.yaml` overlay **hooks** that the public exporter never reads.
- **Eventual ICN node:** `Program`/`Milestone` + `ActionItem` + `Artifact` references under the domain; sponsor/accessibility/registration data would be `private-boundary` and (much later) `Vault`-custodied. `icn_target_status: planned` (`partially-supported` only for the action-item path).
- **Must not claim:** that attendee/sponsor data is repo-safe; that a `Vault` enforces disclosure today (disclosure enforcement is design-only, L1 — see capability matrix row 9); that registration/marketing tools are integrated.

### 3. Run the summit

- **Today (Google) + in-person:** run-of-show coordination, day-of role assignments, check-in, live logistics, real-time decisions.
- **Repo-safe package shape:** the day-of role list and run-of-show **template**; a no-CLI facilitator path that walks preview → approve/edit → show action card → show receipt **from committed fixtures** (the existing fixture-backed rehearsal shell, `fixture-backed`, L2 — capability matrix row 7).
- **Eventual ICN node:** `ActionCard` → authorized action → `Receipt`, rendered on a member shell / steward cockpit `Surface`. The generic decision→action→receipt loop has live single-node proof on **fictional** fixtures (L5, capability matrix rows 1/8), but **NYCN's package has not exercised the action-card proof loop end-to-end** (per the package's own proof matrix). `icn_target_status: partially-supported`.
- **Must not claim:** that a node-hosted Summit Ops cockpit exists; that organizers run terminals on the day; that the NYCN-specific loop is proven; that any of this is production or live federation.

### 4. Close the loop after the summit

- **Today (Google):** post-event survey, debrief notes, sponsor thank-yous/renewal decisions, financial reconciliation, retrospective.
- **Repo-safe package shape:** a post-summit report **template**, a debrief/decision-record template, and **repo-safe evidence-export** logic (the bootstrap/export path emits a manifest + expanded export the node can import; rehearsal evidence is repo-safe-by-construction). Survey/financial contents stay off-repo.
- **Eventual ICN node:** `Receipt` retrieval + `evidence/export` (the spine's terminal node); decisions as `Proposal`/`Vote` receipts; allocations/obligations as their own receipt classes. `icn_target_status: planned`.
- **Must not claim:** that financial/survey data is in the repo; that evidence export equals a formal pilot record; Phase 2 completion.

## What Summit Ops can usefully do now vs later

| Capability | Plane | Status | Proof |
|---|---|---|---|
| Model the summit cycle / committees / lanes as repo-safe shapes | package | `implemented` (shapes exist in NYCN repo) | L1 (declared shapes) |
| Reduce "where is the latest version / who stewards what" burden via the operating-surfaces + asset inventory | package → better-organizing-now | `implemented but partial` | L1–L2 |
| Validate sheet/column mappings without touching Sheets | package | `implemented` (no-network validators) | L2 (unit-tested in package) |
| Fixture-backed, no-terminal facilitator walkthrough | package | `fixture-backed` | L2 (matrix row 7) |
| Bootstrap/export package → node manifest | package | `implemented but partial` (export shape exists) | L1–L2 |
| Live standing / action cards / receipts for the *NYCN* summit | future node | `planned` / `partially-supported` | not yet exercised for NYCN |
| Node-hosted Summit Ops cockpit | future node | `docs-only / design-direction` | L0–L1 |
| Live Google sync of any surface | — | **not built, not on the roadmap** | n/a (forbidden by package boundary) |

## Privacy and boundary discipline (inherited, not invented)

This map adopts the NYCN package's existing boundary policy verbatim; it does not create a new one:

- **Default = leave it out of the repo.** The repo is canonical for *shapes*; real Drive/Docs/Sheets/Groups contents, rosters, sponsor/attendee data, and credentials stay where they live.
- **Hard limits (no exception path):** no live sync; no private URLs; no raw document contents; no attendee/sponsor/member data; no credentials/tokens/exports.
- **Six privacy classes** for surfaces: `public`, `organizer-internal`, `committee-internal`, `partner-restricted`, `attendee-restricted`, `sponsor-restricted`.
- **Future-bridge rule:** any future tool touching a surface must run locally on a steward's machine, require an explicit operator-confirm flag, refuse by default to write surface contents into the repo, have its repo-bound outputs manually reviewed, and wire **no** K3s / production gateway / shared service. Until those hold, surface sync is "not built, not implied, not promised."

Rehearsal privacy today is achieved by **exclusion** (fictional fixtures, repo-safe-by-construction packets), not by an enforced disclosure boundary (`private-boundary`; enforcement is design-only, L1).

## Evidence surfaces used

- [`organizer-rehearsal-operability-map.md`](../reference/project-index/organizer-rehearsal-operability-map.md) — what is safe to show / fixture-backed / steward-only.
- [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) — recorded proof (L0–L8) per rehearsal capability.
- [`generated/route-inventory.md`](../reference/project-index/generated/route-inventory.md) and [`generated/icnctl-command-inventory.md`](../reference/project-index/generated/icnctl-command-inventory.md) — `L1` route/command declarations the runtime stages would eventually use (e.g. `/gov/me/action-cards`, completion-receipt; `icnctl audit verify`, `receipts chain`). Declarations only — not correctness, auth, or live wiring.
- [`show-readiness-map.md`](../reference/project-index/show-readiness-map.md) — red lines for outside-facing material.
- NYCN package docs (partner repo, read-only reference): `NYCN_WHOLE_SYSTEM_MODEL.md`, `OPERATING_SURFACES_BOUNDARY.md`, `DRIVE_IMPORT_POLICY.md`, `COMMUNICATION_GROUPS_BOUNDARY.md`, `ICN-INTEGRATION.md`, `institution/mappings/icn-object-map.yaml`, `summit/2026/CANONICAL_FACTS.md`.

## Nonclaims

This map:

- Does **not** mutate the NYCN repo or any partner material.
- Does **not** sync, read, or import Google Drive / Docs / Sheets / Groups / email / website.
- Does **not** imply the NYCN repo replaces Google planning surfaces, or that organizers coordinate the summit in git.
- Does **not** put private planning, attendee, sponsor, financial, or roster data into the repo.
- Does **not** claim a node-hosted Summit Ops cockpit exists.
- Does **not** claim the NYCN-specific action-card proof loop has been exercised end-to-end.
- Does **not** equate package rehearsal with a formal NYCN pilot.
- Does **not** claim production readiness, live federation, or Phase 2 completion.
- Does **not** change route, `icnctl`, authn/authz, or OpenAPI behavior; does **not** regenerate SDK types.
- Does **not** start A2e enforcement cutover; does **not** affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082).

## Next slices

Smallest claim-safe steps, each a candidate child of [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) / [#2141](https://github.com/InterCooperative-Network/icn/issues/2141):

1. **Pin one lifecycle stage to one runnable path** — pick the "run" stage's decision→action→receipt loop and bind it to specific routes/commands from the generated inventories, fixture-backed only.
2. **Map the workflow-extraction step** — document how a steward turns one Google planning lane (e.g. the Content lane) into a repo-safe package shape under the existing future-bridge rule, no live sync.
3. **Exercise the NYCN package proof loop once** — move the NYCN action-card path from `planned`/`partially-supported` toward a recorded local proof, on fictional fixtures.
4. **A no-terminal facilitator packet for one stage** — the plain-language preview/review path a facilitator can show without a terminal (accessibility gate applied).
5. **Confirm the bootstrap/export → node-import contract** — verify the package export shape against what a future node would consume, without standing up a node.

## Where to read deeper

| You want… | Read |
|---|---|
| The ICN spine + vocabulary + meaning firewall | [`docs/architecture/ICN_OPERATING_MODEL.md`](../architecture/ICN_OPERATING_MODEL.md) |
| What is safe to show an organizer now | [`organizer-rehearsal-operability-map.md`](../reference/project-index/organizer-rehearsal-operability-map.md) |
| Recorded proof per capability | [`proof-level-taxonomy-capability-matrix.md`](../reference/project-index/proof-level-taxonomy-capability-matrix.md) |
| Red lines for outside-facing material | [`show-readiness-map.md`](../reference/project-index/show-readiness-map.md) |
| The milestone + spine control issues | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
