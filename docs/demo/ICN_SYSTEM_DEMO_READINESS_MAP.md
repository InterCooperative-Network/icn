# ICN System Demo Readiness Map

**Status**: Living planning artifact
**Last Updated**: 2026-05-08
**Purpose**: sequence runtime/UI work toward a usable ICN demo without claiming we built one

This map is the canonical planning artifact for the **demo-readiness tranche**. It is **not**:

- a rehearsal-doc cousin to the NYCN side (those land in NYCN);
- a substrate readiness declaration (substrate state lives in [`docs/STATE.md`](../STATE.md) and [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md));
- a presentation gate (the cross-repo audit on 2026-05-08 found ICN+NYCN broadly coherent for the May framing);
- a prescription for new architecture (no new contracts; no new kernel boundaries; runtime work proposed below is composition, not invention).

It **is** the planning seam between the discovery that "ICN already has runtime + UI surfaces" and the work needed to turn those surfaces into a guided, plain-language, honest ICN demo for organizers and members.

## 1. Status (read first)

- **Documents existing state plus a planned PR sequence.** No new architecture, no new contract, no behavior change introduced by this doc.
- **Repo-safe.** No real names / real DIDs / real contact or private participant data. Already-committed fictional fixture labels (e.g. `did:icn:example-*-not-live`) may appear only where existing fixtures reference them.
- **Not an organizer decision.**
- **Not a formal pilot.**
- **Not production deployment.** No live ICN runtime mutation, Google Drive / Groups / Sheets, federation, K3s, DNS, Forgejo, private-overlay, or cloud-sync activation. GitHub PR operations only.
- **Not real holder-label activation.** Activation remains gated by [`docs/spec/private-overlay-did-activation-flow.md`](../spec/private-overlay-did-activation-flow.md) §5–§6 and partner-package policy.
- **Not Phase 2 completion.** Phase truth lives in [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md); this map does not advance phase.
- **Not a presentation-readiness declaration.** The cross-repo audit on 2026-05-08 found ICN+NYCN broadly coherent for the May framing. This map closes a planning gap so subsequent runtime/UI PRs land in sequence; it does not declare anything organizer-ready.

## 2. Diagnosis — composition gap, not GUI absence

The earlier framing assumed: "ICN has no GUI; we need to build one." Discovery on 2026-05-08 contradicted that. The actual state:

| Surface | Present? | Path | Runnable today |
|---|---|---|---|
| Pilot UI (PWA) | yes | [`web/pilot-ui/`](../../web/pilot-ui/) | yes (vanilla JS, Playwright e2e tests) |
| Node Dashboard | yes | [`web/dashboard/`](../../web/dashboard/) | yes (basic, no auth) |
| API docs (Swagger) | yes | [`web/api-docs/`](../../web/api-docs/) | yes |
| One-click demo | yes | [`demo/scripts/run-tool-library-demo.sh`](../../demo/scripts/run-tool-library-demo.sh) | yes (~30 s startup) |
| Devnet (3-node) | yes | [`deploy/devnet/`](../../deploy/devnet/) | yes (`make up`) |
| TUI console | yes | [`icn/bins/icn-console/`](../../icn/bins/icn-console/) | yes (terminal-only) |
| TypeScript SDK | yes | [`sdk/typescript/`](../../sdk/typescript/) | yes (Node scripts) |

The blocker is sharper than "no GUI":

> **The existing GUI and runtime surfaces are not yet composed into a clear, guided, plain-language demo of ICN as imagined.**

Concretely:

- **Pilot UI is timebank-flavored.** It renders members, balances, proposals, votes, ledger, trust score, and an Action Items tab driven by `loadActionItems()` (`web/pilot-ui/app.js:7040`) calling `/gov/domains/{domain_id}/action-items`. It does not walk the §3 Guided workflow nine-step narrative ([`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md)) as a guided flow.
- **Action cards are absent from pilot-ui entirely.** `GET /v1/gov/me/action-cards` returns JSON; **no pilot-ui code consumes that endpoint today** (verified by `grep -rn "action-cards" web/pilot-ui/`). Pilot-ui's "Action Items" tab is a different concept — it queries `/gov/domains/{domain_id}/action-items`, not the per-member action-cards stream. Card-shaped UX surfacing the organizer-facing fields per [`action-card.schema.json`](../contracts/institution-package/action-card.schema.json) (`title`, `summary`, `source_kind`, `action_kind`, `authority_basis`, `required_authority_scope`, `deadline`, `risk_level`, `accessibility_hint`, `receipt_expected`) does not exist in any pilot-ui surface.
- **Receipts already have an HTML render surface; plain-language framing is the gap.** `web/pilot-ui/receipts.js` (~18 KB) queries `/v1/receipts/chain?decision_hash=...` and renders the chain + ledger as HTML via `renderResults()`. The opaque receipt storage stack (PRs #1755–#1759) lands `ProcessGateResultReceipt` durably under that surface. The remaining gap is **plain-language framing on top of the existing render** — status / authority / hash / timestamp summary stated *before* the structured chain — not absence of HTML rendering.
- **No mode boundary.** Nothing on screen tells a viewer whether the data is fixture, demo-seed, devnet, or live. ICN [#1727](https://github.com/InterCooperative-Network/icn/issues/1727) (fixture-backed demo mode) is open and not implemented; no `--demo-mode` flag exists on `icnd`.
- **Two web surfaces share a gateway.** `web/pilot-ui` and `web/dashboard` consume the same routes with different auth postures; there is no curated landing that explains which surface is "the demo."

## 3. Runtime classification per ICN function

Per the user-supplied scheme: **runtime implemented and GUI-visible / runtime implemented but only CLI-visible / runtime implemented but not wired to demo fixtures / runtime implemented but not documented / fixture-only / mocked / spec-only / missing**.

| Function | Status | Evidence |
|---|---|---|
| Identity / DID / standing | **runtime + missing in pilot-ui** | `GET /v1/gov/me/standing` (#1627) returns; **no pilot-ui consumer** (verified by `grep -rn "me/standing" web/pilot-ui/`). Pilot-ui knows the logged-in DID via auth flow; it does not fetch the standing read-model. |
| Governance — proposals / voting / decision | runtime + GUI-visible | gateway routes; pilot-ui Governance tab |
| Action cards | **runtime + missing in pilot-ui** | `GET /v1/gov/me/action-cards` (#1659) returns JSON; **no pilot-ui consumer** (the "Action Items" tab queries a different endpoint, `/gov/domains/{d}/action-items`) |
| Receipts (governance / completion / attendance) | **runtime + GUI-visible (chain), plain-language framing absent** | opaque storage stack #1755–#1759; `web/pilot-ui/receipts.js` renders `/v1/receipts/chain` + ledger as HTML; raw structure shown without plain-language summary on top |
| Provenance / evidence | **runtime + GUI-visible via receipts.js, plain-language framing absent** | same surface as receipts; chain HTML exists, status/authority/hash/timestamp summary not yet stated up front |
| Obligations / allocations / settlement primitives | **runtime + only CLI/JSON-visible** | `/v1/receipts/allocations`, `/v1/receipts/intents` JSON; no UI |
| Federation | runtime + only CLI/JSON-visible (no peers endpoint) | actual federation routes are `/federation/status`, `/federation/coops`, `/federation/coops/{id}/vouches`, `/federation/attestations/{member_did}`, `/federation/init`, `/federation/clearing/...`; **there is no `/v1/federation/peers` route**. Node-dashboard's "peer count" is a proxy computed from `/v1/trust/edges`, not from a federation peers endpoint. Implementers planning federation surface work should not chase the missing peers route. |
| Private overlay / holder-label DID boundary | spec-only on substrate side | [`private-overlay-did-activation-flow.md`](../spec/private-overlay-did-activation-flow.md) is the substrate boundary; partner-package policy is required for activation; no runtime activation surface |
| Organizer/member UX | **runtime + not wired to §3 Guided workflow** | pilot-ui exists but is timebank-flavored, not the 9-step no-CLI organizer narrative |
| Operator/admin UX | runtime + only CLI/HTML-visible (basic) | `web/dashboard` exists, no auth, basic; icn-console TUI for power users |
| Accessibility gates | spec-only at runtime | [`ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) is PR-time review reflex; no runtime enforcement |
| Demo / runtime surfaces | **runtime + not yet a fixture-mode** | `run-tool-library-demo.sh` exists; ICN #1727 fixture-backed demo mode **OPEN, not implemented** |
| Node / dashboard surfaces | runtime + only CLI/HTML-visible (basic) | `web/dashboard`, no auth |
| NYCN package integration | fixture-only on NYCN side | NYCN renders concepts as markdown+JSON in `dist/`; not directly reused by ICN UI |
| Local-first / demo mode | **missing as a first-class runtime mode** | no `--demo-mode` flag, no fixture loader; demo flow uses seeded-but-real state |
| No-CLI path (organizer-visible) | **runtime + UI does not mirror §3 Guided workflow** | endpoints exist; no GUI shell that walks the 9-step story |

The pattern: **most ICN core functions are runtime-implemented and reachable through gateway HTTP routes; the gap is between routes and rendered, plain-language UX.**

## 4. Direct answers to the 17 readiness questions

### Section 4.1 — runtime questions (the 10)

| # | Question | Answer |
|---|---|---|
| 1 | Can a local ICN node/demo process run today? | **Yes.** `./demo/scripts/run-tool-library-demo.sh` boots icnd + gateway + pilot-ui in ~30 s. |
| 2 | Can it serve standing? | **Yes (data); pilot-ui has no consumer.** `GET /v1/gov/me/standing` returns; pilot-ui code does not call this endpoint today (verified by grep). The auth flow knows the logged-in DID; the standing read-model is not fetched. |
| 3 | Can it serve action cards? | **Yes (data); pilot-ui has no consumer.** `GET /v1/gov/me/action-cards` returns JSON. Pilot-ui's "Action Items" tab queries `/gov/domains/{d}/action-items` — a different endpoint and concept. The `action-cards` endpoint is not fetched by any pilot-ui code today. |
| 4 | Can it show governance events / proposals? | **Yes.** Pilot-ui Governance tab; full create/vote flow. |
| 5 | Can it produce or retrieve receipts? | **Yes, with HTML rendering.** Opaque receipt storage stack live (#1755–#1759); `GET /v1/receipts/chain` returns JSON; `web/pilot-ui/receipts.js` already renders the chain + ledger as HTML. |
| 6 | Can it show provenance/evidence in plain language? | **Partial.** `receipts.js` renders chain HTML; **plain-language framing on top** (status / authority / hash / timestamp summary stated before the structured detail) is the remaining gap. |
| 7 | Can it distinguish fixture / local / demo / live state? | **No.** No mode indicator on any UI; gateway has no `--demo-mode` flag. |
| 8 | Is there an organizer/member GUI connected to any of this? | **Partial.** Pilot-ui (timebank-flavored) consumes proposals, votes, transactions, action items, ledger, trust score, and renders receipt chains via `receipts.js`. It does **not** consume the per-member `me/standing` or `me/action-cards` read-models, and does **not** implement the §3 Guided workflow as a guided flow. |
| 9 | What still requires CLI? | Initial keystore (`icnctl id init`), token generation, gateway start, devnet bring-up, `icnctl audit verify`, all icn-console operations. |
| 10 | What is the smallest runtime PR that makes the GUI/demo path more real? | **PR 2 below** — pilot-ui demo-mode boundary/header + guided demo landing screen. Lands on existing pilot-ui without backend work; sets the scaffolding for PRs 3–5. |

### Section 4.2 — sharpened questions (the 7)

| # | Question | Answer |
|---|---|---|
| 1 | Can the existing pilot-ui become the guided organizer/member demo surface? | **Yes**, with composition work — landing screen + guided flow + per-section labels. Pilot-ui's vanilla-JS architecture (no framework) makes targeted additions straightforward. |
| 2 | What is the smallest change that makes pilot-ui show the ICN core loop clearly? | A **guided demo landing screen** that frames the §3 Guided workflow steps and a **mode banner** that tells the viewer what state they're looking at. Both UI-only. |
| 3 | What runtime data is already available through gateway routes? | Standing, action-cards, governance proposals/votes/decisions, ledger postings, trust scores + edges, decision-receipts, completion-receipts, attendance-receipts, allocation/intent receipts, federation status / coops / vouches / attestations / clearing, health/metrics. (No `/v1/federation/peers` route exists; node-dashboard's "peer count" is a proxy computed from `/v1/trust/edges`.) |
| 4 | What needs only UI wiring (no backend change)? | (a) demo-mode banner / boundary labels; (b) guided demo landing screen; (c) action-card UX (cards, not list) — data already in JSON; (d) plain-language receipt / provenance panel — data already in JSON. |
| 5 | What needs fixture/demo-mode support? | Stable, repeatable demo data not derived from a seeded but real state. ICN #1727 is the open scope: a `--demo-mode` flag + committed fixture pack the gateway serves deterministically. |
| 6 | What needs a new read-model route? | None for this tranche. A `/demo/state` summary route would be convenient but is **not** required to render guided demo + action cards + receipts. Keep this off the critical path. |
| 7 | What must remain explicitly labeled as not live / not implemented? | Federation topology beyond the trust-edges-derived peer-count proxy (no real topology view); holder-label DID activation (substrate spec only; partner-package policy required); allocation / settlement at scale; private-overlay activation; cross-cooperative federation flows; production cloud-sync; live K3s / DNS / Forgejo / GitHub-org mutation. Each of these gets a "**Not implemented in demo**" label on the relevant pilot-ui section. |

## 5. Recommended PR sequence

This sequence is the actionable output of the discovery. Each PR is **small** (single-concern, narrow file footprint), **honest** (no new claims, no new contracts), and **composes with the next**. PR 1 is this map. PRs 2–5 are the runtime/UI tranche. Each PR has its own approval gate; no PR is queued behind another.

### PR 1 (this PR) — `docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md` + index update

- **What:** the doc you are reading + a one-line addition to [`docs/demo/README.md`](README.md) Documentation table.
- **Why:** sequences PRs 2–5 with a single source of truth; surfaces discovery findings as repo state; prevents wrong runtime work before priorities are pinned.
- **What runs after this PR:** nothing new at runtime. The map is a planning artifact, not a feature.
- **Non-claims:** documents existing state + a planned sequence; no behavior change; no presentation-readiness claim.
- **Files:**
  - `docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md` (new)
  - `docs/demo/README.md` (one-line index addition)

### PR 2 — pilot-ui demo-mode boundary header + guided demo landing screen

- **What:** add a header banner that reads the runtime mode (one of `DEMO`, `FIXTURE`, `DEVNET`, `LOCAL`, `LIVE`) and renders a colored badge in the pilot-ui chrome; add a landing screen at `#/demo` (or `?mode=demo`) that walks the §3 Guided workflow narrative as a guided flow, with **Next** buttons stepping through the **existing** pilot-ui sections only.
- **Why:** answers the user's "shortest path from existing surfaces to a usable ICN demo" — re-uses 100% of existing pilot-ui rendering and adds the composition layer. **Does not pretend standing or action-cards are already GUI-visible.**
- **What runs after this PR (honest labeling required):** opening pilot-ui at `?mode=demo` shows a guided landing that lists the demo path explicitly:
  - **Available now in pilot-ui:** governance / proposals / votes; action items (from `/gov/domains/{d}/action-items`); ledger; trust score; receipt chain view (via `receipts.js`).
  - **Coming in this demo tranche (linked but labeled "Not yet wired"):** member standing read-model surface (PR 3), per-member action cards surface (PR 3), plain-language receipt framing (PR 4), fixture-backed local demo mode (PR 5).
  - **Not implemented in demo at all:** federation topology, holder-label DID activation, allocation/settlement at scale, private-overlay activation, cross-cooperative federation, cloud-sync, K3s/DNS/Forgejo/production-deploy mutation.
  The "I am [member label]" framing comes from the auth flow's known DID, **not** from a standing fetch — PR 2 must not introduce new endpoint consumers.
- **Mode source for now:** environment / query-param toggle. Does **not** require backend change. (Real `--demo-mode` flag is PR 5.)
- **Hard rule for PR 2 implementers:** **no new endpoint consumers.** PR 2 wires composition over existing fetches only. Adding `me/standing` or `me/action-cards` consumers is PR 3's scope. If the landing needs a member display, use the existing auth-known DID; do not add a fetch.
- **Non-claims (must appear in PR body and in the banner-rendered legend):**
  - Demo mode is a UI labeling convention; the banner reflects what was passed to the page, not authoritative gateway state.
  - Mode `LIVE` does **not** mean production — it means the page is reading real gateway state. ICN is not production-deployed.
  - Sections marked **Not yet wired** are honest gaps for upcoming PRs in this tranche; sections marked **Not implemented in demo** are out of scope for the entire tranche.
- **Files (estimated):**
  - `web/pilot-ui/index.html` — header markup, landing template
  - `web/pilot-ui/app.js` — mode parsing, banner render, landing routing, **Next**-button wiring (no new fetches)
  - `web/pilot-ui/style.css` — banner + landing styling
  - `web/pilot-ui/tests/e2e/` — one Playwright test for landing → next-section flow
- **Out of scope:** member standing fetch + render (PR 3), action-cards fetch + card render (PR 3), receipt/provenance plain-language framing (PR 4), gateway-side fixture mode (PR 5), modifications to the existing Action Items tab.

### PR 3 — pilot-ui member standing + action cards: add the missing fetches and render as a coherent member surface

- **What:** **Add two new pilot-ui consumers** for the per-member read-models that pilot-ui does not yet fetch — `GET /v1/gov/me/standing` (#1627) and `GET /v1/gov/me/action-cards` (#1659) — and render them together as the member's "Who am I, what's my standing, what can I do?" surface. Card-shaped UX for action cards using the actual fields in [`action-card.schema.json`](../contracts/institution-package/action-card.schema.json): `title`, `summary`, `source_kind`, `action_kind`, `authority_basis`, `required_authority_scope`, `deadline` (when present), `risk_level`, `accessibility_hint`, `receipt_expected`. Standing renders as the framing context above the action-cards queue.
- **Why:** standing and action-cards belong together from the organizer/member point of view. The first question a viewer asks is **"Who am I in this institution, what is my standing, and what can/should I do?"** — and the gateway's two read-models that answer this question (`me/standing` + `me/action-cards`) are both not consumed by pilot-ui today (verified by `grep -rn "me/standing\|me/action-cards" web/pilot-ui/` returning zero matches). Splitting them into separate PRs risks two tiny PRs that technically advance the repo but do not give a coherent member surface. Both are missing; both should land together.
- **What runs after this PR:** the new "My Standing & Action Cards" section in pilot-ui shows the logged-in member's standing (memberships, roles, scopes — per the standing read-model contract) as the framing context, with the action-cards queue rendered as cards directly below. Each card surfaces `title`, `summary`, `authority_basis`, `required_authority_scope`, and where present `deadline` / `risk_level` / `accessibility_hint`, with a `receipt_expected` indicator — all in plain language, no invented fields. The existing "Action Items" tab (per-domain action-item ledger, different endpoint, different concept) is unchanged.
- **Backend change required?** **No.** Both endpoints already exist (#1627 + #1659).
- **Naming note:** name the new pilot-ui section to make the conceptual split clear — e.g. **"My Standing & Action Cards"** — and keep it distinct from the existing "Action Items" tab. Implementers should not collapse standing-as-context with action-items-as-ledger.
- **Non-claims:**
  - Adds two new pilot-ui consumers for existing gateway endpoints; does not change gateway contracts or schemas.
  - Does not modify or replace the existing Action Items tab.
  - Does not advance #1748 runtime gates (b)–(d) or any acceptance criterion beyond pilot-ui rendering of already-served data.
  - RFC-gated source kinds (signal_rule, obligation_lifecycle per the action-card schema's `x-icn-rfc-gated-source-kinds`) appear as **Not implemented** placeholders; no false RFC promotion.
- **Files (estimated):**
  - `web/pilot-ui/app.js` — `loadStanding()` + `loadActionCards()` (paralleling `loadActionItems()`) + combined renderer
  - `web/pilot-ui/index.html` — new section + card template + standing block
  - `web/pilot-ui/style.css` — section + card + standing styling (may share patterns with existing action-items / dashboard styles but must not collide)
  - `web/pilot-ui/tests/e2e/` — Playwright tests for both fetches and the combined render
- **Out of scope:** new schema fields, new endpoints, mutation flows, modifications to the existing Action Items tab, plain-language framing of receipts (PR 4), fixture-backed runtime (PR 5).
- **Sequencing note:** if standing turns out to be substantially larger than expected during implementation (e.g. multi-coop / multi-federation aggregation requires additional render shape), **stop and report** rather than fold complexity in silently. The user is willing to split into PR 3a (standing) + PR 3b (action cards) only if discovery shows that's necessary; the default is one PR.

### PR 4 — extend the existing receipts.js with plain-language framing

- **What:** extend the **existing** receipt rendering in `web/pilot-ui/receipts.js` (`renderResults()`, `query-chain` flow) to lead with a plain-language status / authority / hash / timestamp summary **before** the existing chain + ledger HTML; keep the structured chain + ledger view as the supporting detail. Implementers should work from `receipts.js` rather than introduce a parallel surface.
- **Why:** receipts are the audit-chain backbone of every ICN claim. The chain renders as HTML today, which is good — but it leads with structure, not meaning. A non-technical viewer looking at the existing surface sees hashes and field names before they see "Decision X passed on date Y under authority Z." Adding plain-language framing on top of what's there closes that gap without parallel surfaces.
- **What runs after this PR:** the same Receipts tab; same `query-chain` flow; same chain + ledger detail at the bottom — but the top of the rendered result is now a plain-language sentence stating outcome / authority / hash / timestamp. The structured detail remains visible (one scroll, not one click) for stewards.
- **Backend change required?** **No.** Data already exists at `/v1/receipts/chain` and is already fetched by `receipts.js`.
- **Non-claims:**
  - Extends existing rendering; does not replace or duplicate it.
  - Plain-language summaries do not replace the JSON / chain record; both are visible on the same surface.
  - "Pinned in opaque storage" cites the [Invariant 6](../architecture/KERNEL_APP_SEPARATION.md) language verbatim — it is the kernel-blind storage primitive, not a trust claim.
- **Files (estimated):**
  - `web/pilot-ui/receipts.js` — extend `renderResults()` with plain-language summary block
  - `web/pilot-ui/index.html` — minor markup adjustment for the summary block above the existing chain results
  - `web/pilot-ui/style.css` — summary styling
- **Out of scope:** new receipt classes, new endpoints, structured proof verification (different runtime scope), parallel rendering surfaces.

### PR 5 — fixture-backed local demo mode / stable demo data loader (advances ICN #1727)

- **What:** add a `--demo-mode` flag to `icnd` that, when set, serves a committed fixture pack from a locked path instead of seeded-but-real state. The fixture pack contains: members, proposals, votes, action cards, receipts, attendance records, ledger postings — all using the existing `did:icn:example-*-not-live` fixture convention.
- **Why:** today the demo loop conflates "what we seeded" with "what's stored." The PR makes demo mode deterministic and labeled — same data every time, no real keystore initialization, no real DID generation, no real signing. This is the runtime piece that makes the mode banner from PR 2 truthful.
- **What runs after this PR:** `./demo/scripts/run-tool-library-demo.sh --demo` boots icnd in fixture mode; pilot-ui's banner reads `DEMO`; every entity on screen uses fictional fixture labels; nothing touches the real keystore or the real network.
- **Backend change required?** **Yes.** This is the only runtime PR in the sequence. Scope per ICN [#1727](https://github.com/InterCooperative-Network/icn/issues/1727): fixture set + minimal loader + default-to-demo toggle.
- **Non-claims:**
  - Fixture mode is for local demo only. It does **not** alter live-mode behavior, kernel invariants, or production code paths.
  - The fixture pack is committed; it does not generate or store real data.
  - Demo mode does **not** simulate federation, holder-label activation, private overlays, or cloud sync — those remain explicitly **not implemented in demo**.
- **Files (estimated):**
  - `icn/bins/icnd/src/main.rs` — flag wiring
  - `icn/crates/icn-gateway/src/...` — fixture loader (one new module under existing paths)
  - `icn/fixtures/demo/` — committed fixture pack (new; `did:icn:example-*-not-live` convention only)
  - `demo/scripts/run-tool-library-demo.sh` — pass `--demo` flag
- **Out of scope:** anything beyond loading committed fixtures — no synthetic generation, no record/replay, no live-mode toggle from within the running gateway.

## 6. What stays explicitly not-yet-done in this tranche

Per the user's hard rules and the existing CLAUDE.md guard rails, the following are **deferred** and must remain labeled as such on every relevant pilot-ui surface:

- Live federation; cross-cooperative federation flows; federation topology view beyond the trust-edges-derived peer-count proxy used by node-dashboard.
- Private-overlay activation flows; holder-label DID binding from within the demo; any consent-moment UI.
- Cloud sync (Drive / Sheets / Groups / Calendar / mail).
- K3s / DNS / Forgejo / GitHub-org / production-deploy mutation from any demo path.
- Allocation / settlement at scale; multi-cooperative obligations; treasury surfaces.
- Operator dashboards beyond the basic `web/dashboard` and `icn-console` TUI.
- Real-data ingest paths; live import from external sources.
- Production deployment posture; release-candidate gates.

These appear in the demo as **Not implemented in demo** labels (PR 2 banner system) so a viewer can see what is and isn't simulated.

## 7. Cross-references

- ICN [#1746](https://github.com/InterCooperative-Network/icn/issues/1746) — showcase milestone; Workstream A names #1727 / #1726 as required-before-presentation-feels-real (this map advances #1727 in PR 5; #1726 thin shell remains aspirational and is **not** in this tranche).
- ICN [#1727](https://github.com/InterCooperative-Network/icn/issues/1727) — fixture-backed demo mode; PR 5 is the smallest implementation slice.
- ICN [#1726](https://github.com/InterCooperative-Network/icn/issues/1726) — thin organizer rehearsal shell; **out of scope** for this tranche. Pilot-ui composition (PRs 2–4) is the smaller path that preserves the option to do #1726 later.
- ICN [#1748](https://github.com/InterCooperative-Network/icn/issues/1748) — institutional process substrate; Gate (a) runtime-receipt acceptance satisfied by #1755–#1759; Gates (b)–(d) (visibility / privacy redaction / accessibility runtime gate) remain open and are **out of scope** for this tranche.
- NYCN [#65](https://github.com/InterCooperative-Network/cooperative-network/pull/65) — REHEARSAL-0003 fixture-only walkthrough; PR 3's card UX should match the deck's wireframe expectations cited there.
- ICN [#1768](https://github.com/InterCooperative-Network/icn/pull/1768) (merged) — Invariant 6 documents opaque receipt storage; PR 4's receipt panel surfaces the same vocabulary.

No issue checkbox edits in this PR. Status comments may be posted **after** PRs 2–5 land, scoped per the issues' acceptance criteria.

## 8. Validation expectations for PRs 2–5

Each PR in the sequence should pass the same posture gates that #65/#66/#1768 followed:

- `cargo check --workspace` (defensive — for non-runtime PRs the workspace must still compile; for PR 5 the new code paths must compile and pass `cargo test -p icn-gateway`).
- `git diff --check` clean.
- `git status --short` shows only the intended files.
- Pilot-ui Playwright E2E suite passes on PRs 2 / 3 / 4.
- Real-contact leak grep covering common email-provider domains, common phone-number formats, and known team contact patterns returns no matches outside explicit "do not commit" / non-claim contexts.
- Overclaim grep covering production / formal pilot / Phase 2 completion / live federation / live cloud / live infrastructure mutation terms hits only in explicit non-claim / red-line / out-of-scope contexts.
- For PR 5, the fixture pack must use only the committed `did:icn:example-*-not-live` convention; no real names, real DIDs, real contacts, or real keystore initialization.
- Each PR body uses the relevant non-claims block from §5 verbatim and includes the Test plan checklist.

## 9. Related docs

- [README.md](README.md) — demo entry point and runnable surfaces
- [DEMO_SCRIPT.md](DEMO_SCRIPT.md) — 20-min presenter walkthrough (existing)
- [QUICK_START.md](QUICK_START.md) — 5-minute clone-to-running
- [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) — visual diagrams
- [WALKTHROUGH.md](WALKTHROUGH.md) — system tour
- [FAQ.md](FAQ.md) — talking points
- [`docs/STATE.md`](../STATE.md) — substrate state (Phase 2 ⏳ partner-bound)
- [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md) — phase truth
- [`docs/architecture/KERNEL_APP_SEPARATION.md`](../architecture/KERNEL_APP_SEPARATION.md) — kernel/app boundary including Invariant 6 (opaque receipt storage)
- [`docs/contracts/institution-package/action-card.schema.json`](../contracts/institution-package/action-card.schema.json) — action card contract; PR 3 renders against this
- [`docs/spec/private-overlay-did-activation-flow.md`](../spec/private-overlay-did-activation-flow.md) — substrate boundary for partner activation; out of scope for this tranche
- [`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`](../pilots/no-cli-organizer-member-rehearsal-workflow.md) — §3 nine-step narrative the demo composition walks
- [`docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`](../design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md) — PR-time review gate; applied to PRs 2–5 as they land
