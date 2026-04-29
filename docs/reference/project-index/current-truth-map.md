---
Status: descriptive
Canonical: no
Last Reviewed: 2026-04-29
---

# Current Truth Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). This map is a fast pointer at those, not a parallel record.

This is a one-screen routing doc for the question *"what is real right now?"*. The per-PR record is in `STATE.md` (with stacked `[sync edit]` annotations) and the phase model is in `PHASE_PROGRESS.md`. This map exists to keep a fresh reader from confusing those with strategy docs, archive docs, or older planning material.

## Phase position

- **Phase 0** (Close the Demo) — ✅ complete (2026-03-18). Four demo flows pass end-to-end on K3s.
- **Phase 1** (Charter Engine) — ✅ complete (2026-03-18). YAML charter documents produce kernel-enforced constraints; five charter templates in `contracts/templates/`.
- **Phase 2** (Pilot Launch) — ⏳ **in progress**, not complete.

> **Phase 2 is in progress. NYCN is the intended first cooperative partner — active partnership track, not yet a formally committed pilot.**

The Phase 2 *machinery* is in place end-to-end (see [Runtime Surface Map](runtime-surface-map.md)). What remains is human procedure: presenting the merged drive-ingest ladder + ICN proof-loop machinery to NYCN organizers, formalizing the partnership, then running an operator pilot rehearsal against real organizer material.

## What is real now

These surfaces exist and are exercised:

- **Institutional-operability runtime** (live charter activation, person-directory overlay, `/me/standing`, `authority_scope` plumbed end-to-end through `assign_role`, persistent governance domains across gateway restart, bootstrap-apply 409 idempotency, generic institution bootstrap package path).
- **Action-card runtime** (`GET /v1/gov/me/action-cards`) with proof-bearing receipt loops for the three currently emitted source paths:
  - `proposal` / `vote` → `GovernanceDecisionReceipt`
  - `action_item` / `complete` → `ActionItemCompletionReceipt`
  - `meeting` / `attend` → `MeetingAttendanceReceipt`
- **Action-item completion-receipt retrieval** — `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt`. Closes the proof loop on the read side over HTTP.
- **Local HTTP proof loop** documented in `docs/dev/NYCN_ACTION_ITEM_RECEIPT_PATH.md`.
- **K3s smoke proof loop** (operator-authorized, against deployed image `91a63eec`) documented in `docs/dev/NYCN_K3S_PROOF_PATH.md`.
- **NYCN drive-ingest operator ladder** in the separate `fahertym/nycn` repo (NYCN #21–#34): parser → review → decisions → publish dry-run → assignee binding → local publisher → local proof runner → federation surface bridge → operator pilot runbook + ladder checker → organizer briefing + simple summit demo → start-here onboarding pass → one-command local preflight runner → operating-surfaces inventory + Google-Groups boundary policy → communication-groups directory tool → operating-surfaces directory tool. The ladder is pure file-in / file-out or localhost-only operator-gated. K3s mutation is never on the NYCN side.

## What is not yet real

- **Phase 2 is not closed.** The next concrete step is presenting the merged ladder + ICN proof-loop machinery to NYCN organizers. Subsequent gates: partnership formalization, then first operator pilot rehearsal recorded against real (or fixture-equivalent) organizer material.
- **Two action-card source paths are RFC-gated** under issue [#1646](https://github.com/InterCooperative-Network/icn/issues/1646):
  - `signal_rule` — gated on [#1631](https://github.com/InterCooperative-Network/icn/issues/1631)
  - `obligation_lifecycle` — gated on [#1634](https://github.com/InterCooperative-Network/icn/issues/1634)
- **K3s/devnet smoke artifact cleanup / teardown semantics** — tracking issue [#1679](https://github.com/InterCooperative-Network/icn/issues/1679). K3s smoke records remain durable devnet proof artifacts; namespaced teardown semantics are not yet specified.
- **One-command deployment** per cooperative, charter customization workflow doc, pilot onboarding guide, and weekly pilot check-in process — Phase 2 deliverables not yet shipped.

## Open gates

| Gate | Owner / track | What unblocks it |
|---|---|---|
| NYCN organizer presentation | Matt + NYCN organizers | Schedule the conversation; bring the ladder + proof-loop machinery |
| Partnership formalization | NYCN organizers | Outcome of presentation; explicit cooperative consent |
| First operator pilot rehearsal | NYCN + ICN ops | Real or fixture-equivalent organizer material; recorded run |
| Action-card runtime expansion | RFC track | Close [#1631](https://github.com/InterCooperative-Network/icn/issues/1631), [#1634](https://github.com/InterCooperative-Network/icn/issues/1634), then [#1646](https://github.com/InterCooperative-Network/icn/issues/1646) |
| K3s teardown semantics | Operations | Inventory and design under [#1679](https://github.com/InterCooperative-Network/icn/issues/1679) |

## Active risks

- **Mistaking strategy docs for current state.** `docs/strategy/*` carries long-arc planning; `STATE.md` and `PHASE_PROGRESS.md` carry truth. The mapping is set in [`docs-control-map.md`](docs-control-map.md).
- **Mistaking archive material for current state.** Anything under `docs/archive/`, `docs/dev-journal/`, or with a stacked `<details>` "snapshot" marker is historical.
- **Overclaiming NYCN integration.** NYCN is the *intended* first partner. There is no formal commitment yet, and there is no live federation integration.

## Where to go next

| You want... | Read |
|---|---|
| The per-PR record | [`docs/STATE.md`](../../STATE.md) |
| The phase model | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
| What surfaces exist today | [`runtime-surface-map.md`](runtime-surface-map.md) |
| What is or isn't show-ready | [`show-readiness-map.md`](show-readiness-map.md) |
| The NYCN boundary | [`pilot-and-nycn-map.md`](pilot-and-nycn-map.md) |
