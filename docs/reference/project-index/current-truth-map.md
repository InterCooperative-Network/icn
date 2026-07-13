---
Status: descriptive
Canonical: no
Last Reviewed: 2026-07-13
---

# Current Truth Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map is a fast pointer at those, not a parallel record.

This is a one-screen routing doc for the question *"what is real right now?"*. The per-PR record is in `STATE.md` (with stacked `[sync edit]` annotations) and the phase model is in `PHASE_PROGRESS.md`. This map exists to keep a fresh reader from confusing those with strategy docs, archive docs, or older planning material.

## Phase position

- **Phase 0** (Close the Demo) — ✅ complete (2026-03-18).
- **Phase 1** (Charter Engine) — ✅ complete (2026-03-18). YAML charter documents produce kernel-enforced constraints.
- **Phase 2** (Pilot Launch) — ⏳ **in progress**, not complete.

> **Phase 2 is in progress. NYCN is the intended first cooperative partner — active partnership track, not yet a formally committed pilot.**

The software side of the current wedge — the **Rehearsal Node organizer→member loop** — is merged and witnessed on an assembled image. What remains is human procedure: the real organizer presentation, pilot formalization, and the first operator rehearsal.

## What is real now

These surfaces exist, are merged to `main`, and were exercised end-to-end in the 2026-07-13 assembled-image KVM witness (image built from clean `main` `8c0fe926`, restrict=on boot):

- **Rehearsal organizer review→confirm runtime** (#2406) — build-mode-gated (`ICN_GOVERNANCE_BUILD_MODE=rehearsal`; routes 404 in every other mode), three narrow scopes, BLAKE3 `preview_digest` binding confirm to the exact previewed plan (wrong/stale digest → 409, fail-closed), confirm executes the real ADR-0026 ladder and creates one real action item.
- **Member-shell organizer surface** (#2407) — `web/member-shell` `?surface=organizer`, live-only guided review→confirm in the browser; axe-clean automated a11y (the human/AT pass is still owed under #2041).
- **Appliance wiring + no-paste launcher** (#2408) — `icn-demo-seed --session organizer|member` (least-privilege role JWTs, fresh member session — never a token upgrade), `icn-demo-verify --rehearsal` steward verifier.
- **Committed reproducible walkthrough driver** (#2409) — `deploy/appliance/smoke/smoke-local.sh --demo` drives the full loop + role negatives; this is the harness a recurring assembled-image lane (#2398) will run.
- **Member completion loop** — standing → action card → completion (narrow `governance:action-item:complete` scope, #2402) → durable completion receipt (survives restart).
- **Evidence export + steward verification** (#2394) — `urn:icn:contract:rehearsal-workflow-evidence:v1`, no DIDs/credentials exported; tampered packet rejected fail-closed.
- **Trusted-local appliance issuance** (#2396/#2397) — `icnctl … --local-mint` signs demo-session JWTs in-process with the node's own first-boot secret; `/auth/verify` stays fail-closed (#2075). This is appliance-local operator bootstrap, **not** production trusted issuance (#2080 open).

## What is not yet real

- **The human gates.** No organizer presentation has occurred (#1703/#1746; partner-side nycn #41/#52). No human assistive-technology pass (#2041). These are the project's primary open gates — software polish does not substitute.
- **Production trusted issuance** (#2080) — how institutions issue real positive authority remains open; the appliance's local mint does not generalize.
- **Recurring assembled-image CI** (#2398) — the walkthrough is protected manually (witnessed at `8c0fe926`); no scheduled runner builds and boots a fresh image per main advance yet.
- **Live federation / two-node** — Rehearsal Node v0.2 territory; nothing federates in production.
- **Disclosure enforcement** — rehearsal privacy is by exclusion; `ScopedVault`/`DisclosurePolicy` remain design-only.
- **Provider-boundary slice 3** (#2393) — operational config categories (deploy/, scripts/, workflow literals) still carry concrete values.
- **K3s/devnet operational liveness** — an ops claim needing re-confirmation (`docs/status.toml`); do not present as currently proven.

## Open gates

| Gate | Owner / track | What unblocks it |
|---|---|---|
| NYCN organizer presentation | Matt + NYCN organizers | Schedule it; the facilitator gate package (nycn#100/#101) is steward-operable |
| Pilot formalization | NYCN organizers | Outcome of presentation; explicit cooperative consent |
| First operator rehearsal | NYCN + ICN ops | Recorded run per REHEARSAL-0004 |
| Human assistive-technology pass | Matt (human/AT) | Real screen-reader/keyboard/zoom run against member-shell (#2041) |
| Recurring assembled smoke | Infra | Runner with KVM + image-build capacity (#2398) |
| Production trusted issuance | Design + human review | #2080 architecture decision |

## Active risks

- **Mistaking strategy docs for current state.** `docs/strategy/*` carries long-arc planning; `STATE.md` and `PHASE_PROGRESS.md` carry truth.
- **Mistaking archive material for current state.** Anything under `docs/archive/` or with a snapshot marker is historical.
- **Overclaiming NYCN integration.** NYCN is the *intended* first partner; there is no formal commitment and no live integration.
- **Mistaking the witness for validation.** The 2026-07-13 assembled-image witness is automated evidence at one commit; it closes no human gate.

## Where to go next

| You want... | Read |
|---|---|
| The per-PR record | [`docs/STATE.md`](../../STATE.md) |
| The phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| What surfaces exist today | [`runtime-surface-map.md`](runtime-surface-map.md) |
| What is or isn't show-ready | [`show-readiness-map.md`](show-readiness-map.md) |
| The rehearsal runbook | [`docs/demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md`](../../demo/ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md) |
