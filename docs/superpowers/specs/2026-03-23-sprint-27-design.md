# Sprint 27 Design: Demo-Ready + Compute Story

**Date:** 2026-03-23
**Sprint:** 27
**Author:** Claude Code (brainstorming session)
**Status:** Approved — proceeding to implementation plan

---

## Sprint Objective

Make ICN presentable for the NY Cooperative Summit (October 2026) and the May call for
presenters. By sprint end, a presenter can run all five demo flows end-to-end without
manual intervention, the website accurately reflects what ICN can do, and a summit
workshop proposal draft is ready to file.

---

## Narrative Rationale

Sprints 24–26 built a complete commons compute credit system: trust-gated admission,
two-phase credit reservation, cost enforcement, idempotent cancellation. None of this
is visible to any external audience yet.

Meanwhile:

- **Flow 2 (patronage)** and **Flow 4 (reporting)** are broken on K3s. Flow 2 calls
  `/v1/receipts/allocations` without `decision_hash`; `/receipts/chain` is not called
  at all in Flow 2 and needs to be added. Flow 4 calls `/v1/receipts/chain` without
  `decision_hash`. Both need: extract `decision_hash` from the GovernanceReceipt
  response and thread it into downstream receipt calls. Bug #1334's `Option<String>`
  fix is already live in the gateway's `receipts.rs`.
- The **website** hasn't been updated since the federation demo merged (March 13). It
  doesn't reflect the commons compute work, the current demo flows, or the project's
  current capabilities.
- There is **no summit proposal draft**. The May call for presenters is ~6 weeks away.

A sprint that fixes the script bugs, adds a Flow 5 compute narrative, and refreshes the
website transforms three sprints of invisible backend work into a coherent demo story
that cooperative organizers can follow.

---

## Workstreams

### WS-1: Demo Script Fixes (Flows 2 & 4)

**Root cause:** Scripts do not extract `decision_hash` from GovernanceReceipt responses
or thread it into downstream receipt calls. Specific gaps:

- Flow 2: calls `/receipts/allocations` without `decision_hash`; does NOT call
  `/receipts/chain` at all — it needs to be added as a new step.
- Flow 4: calls `/receipts/chain` (line 316) without `decision_hash`.

**Pre-condition:** Before writing fixes, run both scripts live on K3s to confirm current
failure modes match this analysis (the March 19 audit is 4 days old).

Flow 4 additionally has federation API schema mismatch bugs (same as Flow 3 pre-PR
#1344). The federation fix from PR #1344 needs to be ported to Flow 4's script.

**Scope note:** No Rust gateway changes expected. These are shell-script integration fixes.

**Tasks:**
- WS1-T1: Fix `flow-2-patronage.sh` — extract `DECISION_HASH` from Step 7 response,
  pass to `/receipts/allocations?decision_hash=`, add new step calling
  `/receipts/chain?decision_hash=`
- WS1-T2: Fix `flow-4-reporting.sh` — same `decision_hash` threading; port federation
  API schema corrections from PR #1344 (field renames: `did` → `public_did`,
  vouch body restructure, clearing body restructure)
- WS1-T3: `reseed-federation-demo.sh` + re-audit Flows 2 & 4 on K3s; confirm PROVEN

---

### WS-2: Flow 5 — Commons Compute Demo

**Scenario:** Finger Lakes CDN (Delta node, already in the 4-coop cluster) submits a
compute task to the commons pool. The flow shows trust-gated admission, credit
reservation before execution, task completion, and a settlement receipt with provenance
chain linking back to the compute submission.

**Actual gateway routes (confirmed from source):**
- Submit: `POST /v1/compute/submit`
- Status: `GET /v1/compute/status/{task_hash}`
- Cancel: `POST /v1/compute/cancel/{task_hash}`
- WASM upload: `POST /v1/compute/wasm/upload`

**API path:**
1. Authenticate with Delta node (`compute:write compute:read ledger:read` scopes)
2. `POST /v1/compute/submit` — submit task with `scope: "commons"` (e.g., "route optimization")
3. Poll `GET /v1/compute/status/{task_hash}` until status transitions from `Pending`
4. Show pre-execution credit reservation via `GET /v1/ledger/{coop_id}/position/{did}`
   (scope: `ledger:read`)
5. On completion: query receipt chain for full provenance
6. Authorization boundary — confirm `compute:write` scope enforcement

**Target environment:**
- Primary: K3s (consistent with Flows 1–4; `kubectl port-forward` to Delta pod)
- Fallback: devnet (if compute actor not wired in the K3s deployment image)

**Pre-flight:** Before writing the script, probe the Delta pod compute endpoint to
confirm the actor is live. If not: use devnet fallback, document K3s wiring as
Sprint 28 work.

**Tasks:**
- WS2-T1: Probe compute actor on K3s Delta pod — `POST /v1/compute/submit` with minimal
  body; expect 400/422 (actor live, invalid payload) not 404/connection refused
- WS2-T2: Write `demo/scripts/flow-5-compute.sh` with `--present` and `--narrated` modes
- WS2-T3: Write `demo/notes/flow-5-notes.md` with beat-by-beat presenter narrative
- WS2-T4: Extend `demo/scripts/reseed-federation-demo.sh` to seed Delta node with
  compute scopes and any required initial state
- WS2-T5: Test end-to-end on K3s (or devnet fallback); confirm script exits 0

---

### WS-3: Website Update

**Current state:** Pages exist for index, about, architecture, community, roadmap, blog,
docs. Content predates the federation demo merge. Issue #1369 (render roadmap-current.yaml)
is open but the YAML may not be populated.

**Target state:**
- `index.astro`: Update hero and capability bullets to include federation + compute
- `architecture.astro`: Add commons compute layer; show full stack:
  Identity → Trust → Governance → Ledger → Compute → Federation
- `roadmap.astro`: Update static milestones to reflect current phase — Phase 0 ✅,
  Phase 1 ✅ (federation demo), Phase 2 (current: demo-ready), Phase 3 (planned)
  — implement #1369 YAML rendering only if `roadmap-current.yaml` is populated
- `docs/demo.astro` (new): Step-through of all 5 demo flows for a cooperative audience
  with plain-language explanations of what each step proves. Note: `docs/` uses a
  `[...slug].astro` dynamic route — Astro 5 resolves static files before dynamic
  routes, so `demo.astro` is valid, but confirm no slug collision with existing content
  during WS3-T4.

**Out of scope:** Design system changes, new blog posts, SDK docs, #1368 community infra.

**Tasks:**
- WS3-T1: Audit all pages for stale content; produce change list
- WS3-T2: Update `index.astro` + `architecture.astro`
- WS3-T3: Update `roadmap.astro` (static update; #1369 YAML rendering if trivial)
- WS3-T4: Add `docs/demo.astro` — 5-flow walkthrough page; verify build resolves correctly
- WS3-T5: `npm run build` + `npm run lint` + `npm run format -- --check` — all three clean

---

### WS-4: Summit Proposal Draft

**Target audience:** NY Cooperative Summit May call for presenters. Audience is
cooperative organizers, workers, housing co-op members — not developers.

**Format:** 60-minute workshop — 20-min live demo + 20-min cooperative context and
discussion + 20-min design exercise (attendees define governance rules for a
hypothetical cooperative using ICN's parameter system).

**Tasks:**
- WS4-T1: Write `docs/summit/workshop-proposal-draft.md` — title, 200-word description,
  3–5 learning outcomes, format, technical requirements
- WS4-T2: Review for cooperative-organizer register (informal, values-aligned, not
  developer-facing); apply writing-voice rules (no em dashes, declarative rhythm)

---

## Dependencies and Sequencing

```
WS1-T1 ──┐
WS1-T2 ──┴─→ WS1-T3 (re-audit only after fixes)

WS2-T1 → WS2-T2 → WS2-T4 → WS2-T5
                └─→ WS2-T3 (notes can be written alongside script)

WS3-T1 → WS3-T2 ─┐
         WS3-T3 ──┴─→ WS3-T5
         WS3-T4 ──┘

WS4 has no external dependencies — can start any time

WS1 and WS2 can run concurrently (different scripts)
WS3 and WS4 can run concurrently and in parallel with WS1/WS2
```

**Recommended execution order:**
1. WS1-T1 + WS1-T2 (fix demo scripts — highest risk, validate early)
2. WS2-T1 (probe compute on K3s — gates all of WS2)
3. WS1-T3 (re-audit flows 2 & 4 — confirm fixes before proceeding)
4. WS2-T2 + WS2-T3 + WS3-T1 (parallel: write Flow 5 + audit website)
5. WS2-T4 + WS2-T5 + WS3-T2–T5 + WS4-T1 (parallel finalization)

---

## Risks and De-Scoping Rules

| Risk | Probability | Impact | Mitigation / De-scope Rule |
|------|-------------|--------|---------------------------|
| Compute actor not wired in K3s deployment | Medium | High | Fall back to devnet for Flow 5; note K3s wiring as Sprint 28 |
| Flow 2 fix exposes deeper gateway route issues | Low | Medium | Cap WS1 at 2 PRs; if root cause is a gateway bug, scope Flow 2 to "governance PROVEN, ledger narrated as illustrative" |
| Flow 4 federation schema fix introduces regressions | Low | Low | Flow 4 is lower-priority; if complex, drop receipt chain to "illustrative" and ship governance + federation as PROVEN |
| Website scope creep | Medium | Low | Content updates only. No new components, no design-system changes |
| Summit proposal perfectionism | Low | Low | Ship 80%-good draft; it's a draft |
| #1369 YAML not populated | Medium | Low | Update static roadmap content instead; don't build YAML renderer for empty file |

---

## PR Groupings

| PR | Branch | Contents |
|----|--------|----------|
| PR-A | `fix/s27-demo-script-fixes` | flow-2 fix, flow-4 fix, reseed update (WS1) |
| PR-B | `feat/s27-flow-5-compute` | flow-5 script, flow-5 notes, reseed extension (WS2) |
| PR-C | `docs/s27-website-refresh` | all website page updates (WS3) |
| PR-D | `docs/s27-summit-proposal` | summit proposal + any supporting docs (WS4) |

PR-A merges first; PR-B depends on PR-A being green (reseed shared dependency).
PR-C and PR-D are independent and can merge in any order.

---

## Files Expected to Change

**WS-1 (Demo Fixes):**
- `demo/scripts/flow-2-patronage.sh`
- `demo/scripts/flow-4-reporting.sh`
- `demo/scripts/reseed-federation-demo.sh` (minor: ensure scopes for receipt queries)

**WS-2 (Flow 5):**
- `demo/scripts/flow-5-compute.sh` ← new
- `demo/notes/flow-5-notes.md` ← new
- `demo/scripts/reseed-federation-demo.sh` (extend for Delta compute scopes)

**WS-3 (Website):**
- `website/src/pages/index.astro`
- `website/src/pages/architecture.astro`
- `website/src/pages/roadmap.astro`
- `website/src/pages/docs/demo.astro` ← new

**WS-4 (Summit):**
- `docs/summit/workshop-proposal-draft.md` ← new

**No Rust changes expected.** If a gateway bug is discovered during WS1 investigation,
it becomes a separate issue and Sprint 27 narrates around it.

---

## GitHub Issues to Create

| Issue | Title | Links to |
|-------|-------|----------|
| New | fix(demo): flow-2 receipt chain — thread decision_hash through steps 8–11 | Bug #1334 |
| New | fix(demo): flow-4 receipt chain + federation schema | Bug #1334, PR #1344 |
| New | feat(demo): Flow 5 — Commons Compute demo script | Epic #1099 |
| New | chore(website): Sprint 27 content refresh — all 5 flows | Issue #1369 |
| New | docs(summit): workshop proposal draft for May call for presenters | — |

---

## Definition of Done

- [ ] `bash demo/scripts/flow-2-patronage.sh` exits 0 on K3s (post reseed)
- [ ] `bash demo/scripts/flow-4-reporting.sh` exits 0 on K3s (post reseed)
- [ ] `demo/scripts/flow-5-compute.sh` exists with `--present` and `--narrated` modes
- [ ] Flow 5 script exits 0 on K3s or devnet
- [ ] `demo/notes/flow-5-notes.md` committed with beat-by-beat narrative
- [ ] `reseed-federation-demo.sh` supports all 5 flows
- [ ] `cd website && npm run build` passes; `npm run lint` clean; `npm run format -- --check` clean
- [ ] `website/src/pages/index.astro`, `architecture.astro`, `roadmap.astro` updated
- [ ] `website/src/pages/docs/demo.astro` page exists covering all 5 flows
- [ ] `docs/summit/workshop-proposal-draft.md` committed
- [ ] Sprint board: all 5 new issues closed
- [ ] Audit doc updated: all 4 original flows marked PROVEN (or explicitly narrated if
  a de-scope rule was invoked)

---

## Recommended First 3 Moves

1. **Run the broken flows live on K3s right now.** `bash demo/scripts/reseed-federation-demo.sh && bash demo/scripts/flow-2-patronage.sh`. Takes 10 minutes. Confirms the exact failure points against today's deployed binary before writing a single fix — the March 19 audit is 4 days old and CI has run since then.

2. **Fix `flow-2-patronage.sh` decision_hash threading.** Open the script at Step 7 (GovernanceReceipt call), extract `decision_hash` from the JSON response into `DECISION_HASH`, pass it as a query parameter to Steps 8–11. This is ~20 lines of shell, no Rust required. Commit and re-run.

3. **Probe the compute endpoint on K3s before writing Flow 5.** `kubectl -n icn-coop-delta exec -it $(kubectl -n icn-coop-delta get pods -o name | head -1) -- curl -s -X POST http://localhost:8080/v1/compute/submit -H 'Content-Type: application/json' -d '{}'`. If it returns 400/401/422 (any HTTP response): the route exists and the actor is live — Flow 5 targets K3s. If 404 or connection refused: target devnet and file a K3s compute-wiring issue for Sprint 28.
