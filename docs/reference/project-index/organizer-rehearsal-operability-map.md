---
Status: descriptive
Canonical: no
Last Reviewed: 2026-06-26
---

# Organizer Rehearsal Operability Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map is an **orientation aid**, not a source of truth: it reads the already-merged generated evidence surfaces (route inventory, `icnctl` command inventory) and the capability matrix, and translates them into "what can safely be shown to an organizer, what is fixture-backed, what needs a steward/operator, and what must not be claimed yet." It changes no behavior and grants no readiness.

## Purpose

The organizer-rehearsal milestone ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)) frames the gap plainly: *the kernel can prove, the NYCN package can rehearse, and the missing piece is the participation surface between them.* The vertical institutional spine ([#2141](https://github.com/InterCooperative-Network/icn/issues/2141)) names the path that surface must eventually carry:

```text
package → domain → policy → binding → process/action → receipt → surface → evidence/export
```

This map does not build that surface. It uses the two mechanical evidence surfaces that landed in the route/CLI-inventory lane — the generated [route inventory](generated/route-inventory.md) ([#2112](https://github.com/InterCooperative-Network/icn/issues/2112)) and the generated [`icnctl` command inventory](generated/icnctl-command-inventory.md) ([#2113](https://github.com/InterCooperative-Network/icn/issues/2113)) — to answer, for an organizer rehearsal:

1. What can be shown to organizers now?
2. What is fixture-backed / demo-only?
3. What requires a steward/operator rather than a non-technical organizer?
4. Which routes support the surface at L1 only?
5. Which `icnctl` commands are relevant but still `unknown / needs local verification`?
6. What must not be claimed?
7. What are the smallest next slices to make the rehearsal operable?

It is deliberately conservative: where the generated inventories prove only that a declaration exists, this map says so and points at the [capability matrix](proof-level-taxonomy-capability-matrix.md) for the places where stronger, recorded proof exists.

## How to read the evidence (two layers, do not conflate)

This map draws on **two different kinds of evidence**, and keeping them apart is the whole point:

- **Generated inventories (declaration evidence, proof level `L1`).** The [route inventory](generated/route-inventory.md) and [`icnctl` command inventory](generated/icnctl-command-inventory.md) are mechanically generated from source. They prove a route macro / registration site or a clap command **declaration exists at a snapshot commit** — nothing more. Every entry is uniformly `L1` / `unknown / needs local verification`. A declaration is **not** evidence of correctness, auth, mounting, live wiring, tests, or organizer-safety.
- **Capability matrix (recorded-proof evidence, `L0`–`L8`).** The [proof-level taxonomy and capability matrix](proof-level-taxonomy-capability-matrix.md) records, per rehearsal capability, the **strongest honest proof** that has actually been observed (e.g. an L4 one-command local proof loop, an L5 live daemon path), each tied to a merged PR.

**Rule used throughout this map:** a route or command is cited at `L1` (declaration only) **unless** the capability matrix records a higher proof level for that specific path, in which case the higher level is cited *with its matrix row*. When the two disagree, [`docs/STATE.md`](../../STATE.md) wins (per the [source-of-truth map](source-of-truth-map.md)).

All status labels below come from the existing vocabulary in the [source-of-truth map](source-of-truth-map.md) (`implemented`, `implemented but partial`, `feature-gated`, `fixture-backed`, `gateway-backed`, `docs-only / design-direction`, `package-local`, `private-boundary`, `unknown / needs local verification`). No new status vocabulary is introduced.

## Current evidence surfaces

| Surface | What it is | Proof it carries | Where |
|---|---|---|---|
| Generated route inventory | Gateway route-macro + governance-app registration declarations vs OpenAPI | `L1` declarations only; uniform `unknown / needs local verification` | [`generated/route-inventory.md`](generated/route-inventory.md) |
| Generated `icnctl` command inventory | clap command tree (162 default-build, 1 feature-gated, 0 unparsed), curated role grouping | `L1` declarations only; uniform `unknown / needs local verification` | [`generated/icnctl-command-inventory.md`](generated/icnctl-command-inventory.md) |
| Proof-level capability matrix | Per-capability recorded proof for the organizer-rehearsal path | `L0`–`L8`, tied to merged PRs | [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md) |
| Show-readiness map | What is honest to demonstrate vs what must not be shown as finished; red lines | claim boundaries | [`show-readiness-map.md`](show-readiness-map.md) |
| Runtime surface map | Member-facing read models, action-card proof loops, receipt retrieval | routing layer | [`runtime-surface-map.md`](runtime-surface-map.md) |
| Pilot-UI / member-shell fixtures | Fixture-backed rehearsal shell and demo panes | `fixture-backed` (L2 per matrix row 7) | `web/pilot-ui/fixtures/icn-organizer-demo/`, `web/member-shell/` |
| DEV/DEMO appliance image | Single-actor loop bootable VM image | `L5` stranger-runnable local proof (matrix row 11) | [`docs/demo/`](../../demo/), `deploy/appliance/` |

## What is safe to show now

These are real and may be presented honestly to organizers, **bounded to the recorded proof level**. Each row pairs the organizer-facing thing with the generated declaration evidence and the recorded proof.

| Show this | Declaration evidence (L1) | Recorded proof | Honest framing |
|---|---|---|---|
| Decision → action → receipt proof loop (proposal/vote, action_item/complete, meeting/attend) | route `/gov/me/action-cards` (`get_my_action_cards` @ `icn/apps/governance/src/http/configure.rs:849`), completion-receipt route (`:682`); `icnctl audit verify` (`main.rs:330`), `icnctl receipts chain` (`:281`) | `L5` live single-node (matrix rows 1, 8); `L4` one-command local proof loop (matrix row 5) | "live daemon/gateway proof of the governed receipt chain on a local ephemeral node — proof of path, not deployment readiness; three of five action-card source paths." |
| Member standing / action-card / scopes read models (the shapes) | routes `/gov/me/standing` (`:845`), `/me/scopes` (`:844`), `/me/work` (`:846`) — `standing`/`action-cards` are OpenAPI-documented; `scopes`/`work` are not | `L5` live for the emitted action-card paths (matrix row 8) | "declared + (for standing/action-cards) OpenAPI-documented member-facing read models; fixture-backed in the shell's demo panes." Do not assert every `/me/*` route is live. |
| Fixture-backed organizer rehearsal shell | n/a (UI fixtures, not routes/commands) | `L2` fixture-backed (matrix row 7); `web/pilot-ui/fixtures/icn-organizer-demo/` | "a bounded rehearsal artifact in demo mode — no live daemon behind it; fictional fixtures only." |
| DEV/DEMO appliance image — single-actor loop in one VM | n/a (image/profile) | `L5` stranger-runnable local proof (matrix row 11) | "one local DEV/DEMO node instance proving the single-actor loop — unsigned image, fictional data, not production, not a pilot, not federation, not multi-person." |
| The thesis + roadmap posture | n/a | per [show-readiness-map](show-readiness-map.md) | Phase 2 in progress; NYCN the *intended* first partner; never a formal pilot / live federation. |

## What is fixture-backed

`fixture-backed` (matrix row 7, L2) — demonstrated through committed **fictional** fixtures only; no live daemon behind them. Show these as rehearsal artifacts, never as live participant state:

- The organizer rehearsal **shell** in demo mode (`web/pilot-ui/fixtures/icn-organizer-demo/`, `web/member-shell/`).
- The shell's `?mode=demo` standing / action-card panes.
- The fictional NYCN fixture institution used by the appliance image and the rehearsal packet.
- The pending-publish summary **contract example** (`urn:icn:contract:pending-publish-summary:v1`) — a read-model contract with a validating fictional example (matrix row 6, L2); no producing endpoint binds to it yet.

Demo fixtures prove UI rendering and contract shape. They do **not** prove live participant state, live federation, production readiness, or formal pilot status (per the [source-of-truth map](source-of-truth-map.md), "Demo fixtures vs live state").

## What requires steward/operator handling

These are **not** for a non-technical organizer to run. They require a steward/operator with a local build and a terminal, and most are `gateway-backed` (need a running daemon/gateway) — a higher bar than the inventory's `L1` declaration. The commands below are cited at `L1` (declaration exists) from the [command inventory](generated/icnctl-command-inventory.md); their *operation* is `unknown / needs local verification` except where the capability matrix records a proof loop.

| Steward/operator capability | Relevant `icnctl` declarations (L1) | Notes |
|---|---|---|
| Run the local receipt-chain rehearsal + emit a repo-safe evidence packet | `icnctl audit verify` (`main.rs:330`), `icnctl receipts chain` (`:281`), `receipts intent` (`:313`), `receipts allocation` (`:299`) | The *path* is recorded at `L4`/`L5` (matrix rows 1, 5); the individual command entries remain `L1`. Needs a local `icnd`/`icnctl` build. |
| Stand up a governed domain + walk a proposal/vote | `icnctl gov domain create` (`:1127`), `gov proposal create` (`:1191`), `gov proposal open` (`:1242`), `gov vote cast` (`:1288`), `gov proposal close` (`:1271`) | `gateway-backed`; declaration-only proof. Steward-only — not an organizer terminal task. |
| Charter / institution package operations | `icnctl charter create` (`:1614`), `charter validate` (`:1698`), `charter ratify` (`:1684`), `charter deploy` (`:1718`) | `gateway-backed`; declaration-only. |
| Commons enrollment / affiliation | `icnctl commons enroll` (`:1564`), `commons join` (`:1589`), `commons status` (`:1561`) | `gateway-backed`; declaration-only. |

The milestone's definition of done is explicit: a **facilitator** shows the workflow **without** asking organizers to run a terminal command, and a **steward** can reproduce the same path from fixtures with existing local commands ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)). This map keeps the two roles separate by construction.

## What is not organizer-ready

Per the [show-readiness map](show-readiness-map.md) red lines and the capability matrix, the following must **not** be presented to organizers as finished or ready:

- **An operable, integrated organizer participation surface.** The surface joining the live proof loop to the rehearsal shell is the open milestone ([#1746](https://github.com/InterCooperative-Network/icn/issues/1746)); `L7` (partner/organizer rehearsal) is **OPEN** (matrix row 10). The shell is `L2`, not `L7`, precisely because the accessibility/privacy bar and the live participation surface are not yet met.
- **Private organizer/member/sponsor data handling.** Disclosure enforcement is design-only (`L1`, matrix row 9). Rehearsal privacy today is by **exclusion** (fictional fixtures, repo-safe-by-construction packets), not by an enforced boundary — `private-boundary`.
- **Multi-person interaction.** The appliance proves a **single-actor** loop (matrix row 11); two-member action flow and any QR node-claim ceremony are future lanes, not built.
- **The other two action-card source paths.** `signal_rule` and `obligation_lifecycle` are RFC-gated ([#1646](https://github.com/InterCooperative-Network/icn/issues/1646)) and not emitted — show three of five, not five of five.
- **A formal NYCN pilot, live federation, or production readiness.** None of these are claimable (red lines, [show-readiness-map](show-readiness-map.md)).

## Route evidence used (L1 declarations unless matrix cites higher)

From the generated [route inventory](generated/route-inventory.md). Every entry is `L1` / `unknown / needs local verification` **as proven by the inventory**; the "stronger proof" column points to the capability matrix where a recorded proof exists.

| Method + path | Handler @ source | OpenAPI | Inventory proof | Stronger recorded proof |
|---|---|---|---|---|
| `GET /gov/me/standing` | `get_my_standing` @ `configure.rs:845` | yes | L1 | L5 live (matrix row 8) |
| `GET /gov/me/action-cards` | `get_my_action_cards` @ `configure.rs:849` | yes | L1 | L5 live, L6 partial (matrix row 8) |
| `GET /gov/me/scopes` | `get_my_scopes` @ `configure.rs:844` | no | L1 | none recorded — `unknown / needs local verification` |
| `GET /gov/me/work` | `get_my_work` @ `configure.rs:846` | no | L1 | none recorded — `unknown / needs local verification` |
| `GET …/action-items/{item_id}/completion-receipt` | `get_action_item_completion_receipt` @ `configure.rs:682` | yes | L1 | L5 receipt-chain path (matrix rows 1, 8) |

Reminder: OpenAPI presence proves a path is **documented**, not served; a served route does not prove it is public-safe ([runtime-surface-map](runtime-surface-map.md)).

## CLI evidence used (L1 declarations unless matrix cites higher)

From the generated [`icnctl` command inventory](generated/icnctl-command-inventory.md) (162 default-build commands; 1 feature-gated; 0 unparsed). All entries are `L1` / `unknown / needs local verification`; role is a **curated** navigation heuristic, not a safety assertion.

| Command | Source | Inventory proof | Stronger recorded proof |
|---|---|---|---|
| `icnctl audit verify` | `main.rs:330` | L1 | part of L4/L5 receipt-chain path (matrix rows 1, 5, 11) |
| `icnctl receipts chain` | `main.rs:281` | L1 | part of L4/L5 receipt-chain path (matrix rows 1, 5) |
| `icnctl receipts intent` | `main.rs:313` | L1 | none recorded for the command itself |
| `icnctl receipts allocation` | `main.rs:299` | L1 | none recorded for the command itself |
| `icnctl gov proposal create / open / close` | `main.rs:1191 / 1242 / 1271` | L1 | none recorded — steward-only, `gateway-backed` |
| `icnctl gov vote cast` | `main.rs:1288` | L1 | none recorded — steward-only, `gateway-backed` |
| `icnctl gov domain create` | `main.rs:1127` | L1 | none recorded — steward-only, `gateway-backed` |
| `icnctl charter create / validate / ratify / deploy` | `main.rs:1614 / 1698 / 1684 / 1718` | L1 | none recorded — steward-only, `gateway-backed` |
| `icnctl commons enroll / join / status` | `main.rs:1564 / 1589 / 1561` | L1 | none recorded — steward-only, `gateway-backed` |

The 1 feature-gated command (`icnctl id upgrade-pq`, `cfg(feature = "post-quantum")`) is **not in the default build** and is not part of the rehearsal path.

## Nonclaims

This map:

- Does **not** mark the organizer rehearsal ready (`L7` is OPEN, [#1746](https://github.com/InterCooperative-Network/icn/issues/1746)).
- Does **not** claim a formal NYCN pilot.
- Does **not** claim production readiness.
- Does **not** claim live federation.
- Does **not** claim Phase 2 completion.
- Does **not** claim route runtime correctness from route declarations.
- Does **not** claim CLI runtime correctness from clap declarations.
- Does **not** ask non-technical organizers to run terminals.
- Does **not** introduce any private NYCN/organizer/member data (fictional fixtures only).
- Does **not** alter route behavior.
- Does **not** alter `icnctl` behavior.
- Does **not** start A2e enforcement cutover.
- Does **not** affect [#2082](https://github.com/InterCooperative-Network/icn/issues/2082).

## Next slices

The smallest claim-safe steps that move the rehearsal from "evidence exists" toward "operable," each a candidate child of [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) / [#2141](https://github.com/InterCooperative-Network/icn/issues/2141):

1. **Identify the minimal rehearsal path** — pin the single end-to-end story (standing → action card → discharge → receipt → evidence) to the specific routes/commands above, so facilitator and steward read from one path.
2. **Map the steward/operator commands needed** — promote the steward-only `icnctl` set above into a per-command status pass (the [#2113](https://github.com/InterCooperative-Network/icn/issues/2113) follow-up): which are `gateway-backed` and proven, which remain `unknown / needs local verification`.
3. **Map the routes/surfaces rendered to the organizer** — confirm which `/me/*` read models the shell actually renders, and bound each to its recorded proof (not its L1 declaration).
4. **Produce a plain-language preview/review packet** — bind the pending-publish summary contract (matrix row 6) to a facilitator-readable preview, no jargon/URNs/hashes as primary labels (accessibility gate).
5. **Produce a no-terminal facilitator path** — a fixture-backed walkthrough a facilitator can show without a terminal, satisfying the [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) definition of done, with the accessibility/privacy checklist applied as a gate.

## Where to read deeper

| You want… | Read |
|---|---|
| What is real now (per-PR pointer) | [`current-truth-map.md`](current-truth-map.md), [`docs/STATE.md`](../../STATE.md) |
| What is show-ready / red lines | [`show-readiness-map.md`](show-readiness-map.md) |
| Per-capability recorded proof | [`proof-level-taxonomy-capability-matrix.md`](proof-level-taxonomy-capability-matrix.md) |
| Real runtime surfaces | [`runtime-surface-map.md`](runtime-surface-map.md) |
| Generated route declarations | [`generated/route-inventory.md`](generated/route-inventory.md) |
| Generated `icnctl` command declarations | [`generated/icnctl-command-inventory.md`](generated/icnctl-command-inventory.md) |
| The milestone | [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) · [#2141](https://github.com/InterCooperative-Network/icn/issues/2141) |
