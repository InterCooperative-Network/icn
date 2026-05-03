---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-03
---

# NYCN organizer and user readiness (ICN side)

ICN-side orientation for **NYCN organizers**, **stewards**, and anyone explaining **member-legible** surfaces. It does not duplicate the NYCN package repo; use [NYCN `docs/ORGANIZER-USER-READINESS.md`](https://github.com/InterCooperative-Network/nycn/blob/main/docs/ORGANIZER-USER-READINESS.md) for ladder commands and fixture policy there.

**Non-claims:** Phase 2 is **not** marked complete here. NYCN is the **intended** first cooperative partner, not a formally committed pilot unless a repo-safe organizer record says so. No production-readiness, live federation, or hosted multi-tenant guarantee.

## What ICN runtime surfaces matter for organizers now

| Surface | HTTP (gateway) | Role |
|---------|----------------|------|
| Standing | `GET /v1/gov/me/standing` | Who the caller is, memberships, roles, capabilities, selected scope — the join point for accessible shells. |
| Action cards | `GET /v1/gov/me/action-cards` | Pending **vote**, **attend**, **complete** work items derived from governance state (see ADR-0027). |
| Action-item completion receipt | `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` | Read-side closure of the action-item proof loop (`governance:read` + domain membership). |

Routing and vocabulary: [runtime-surface-map.md](../reference/project-index/runtime-surface-map.md). Honest demo boundaries: [show-readiness-map.md](../reference/project-index/show-readiness-map.md).

## Path for a future member (standing → cards → receipts → evidence)

1. **Standing** — one coherent view of participatory position (contract: [MEMBER_STANDING.md](../architecture/MEMBER_STANDING.md)); implementation exists per [STATE.md](../STATE.md).
2. **Action cards** — a deterministic, derived list of what needs attention **now**; completing an item uses the normal governance endpoints for that object type, not a separate “card API.”
3. **Receipts** — append-only records close the proof loop, but **HTTP retrieval today is not uniform across kinds**:
   - **Action-item completion:** `GET /v1/gov/domains/{domain_id}/action-items/{item_id}/completion-receipt` returns the completion receipt when permitted (see the table above and [runtime-surface-map.md](../reference/project-index/runtime-surface-map.md)).
   - **Proposal / vote:** governance decision receipts and proof-style reads are exercised through the proposal/vote flow and related surfaces documented in runtime maps and tests — not the same URL shape as the action-item completion-receipt route.
   - **Meeting / attend:** marking attendance (`PUT` on the meeting attendance surface) is part of the loop that **produces** `MeetingAttendanceReceipt` records in governance storage; this doc does **not** claim a dedicated public `GET …/attendance-receipt`-style HTTP mirror analogous to the action-item completion-receipt path unless one is listed in the gateway route map.
4. **Evidence** — for pilot rehearsals, evidence is **human procedure + repo-safe artifacts** (see [NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md](../strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md) and NYCN evidence docs), not private data in the ICN tree.

## Generic substrate vs NYCN package material

- **ICN** ships closed enums, receipt types, and gateway routes that any institution package can use without baking in partner-specific nouns.
- **NYCN** (separate repo) holds charter instances, summit fixtures, drive-ingest ladders, and **package-local** template prose. NYCN maps to ICN primitives in its own docs and `institution/mappings/` — not the other way around in core.

## What remains gated

- Action-card source paths **`signal_rule`** and **`obligation_lifecycle`** are **reserved** in the contract; the runtime does **not** emit them yet ([#1646](https://github.com/InterCooperative-Network/icn/issues/1646), [#1631](https://github.com/InterCooperative-Network/icn/issues/1631), [#1634](https://github.com/InterCooperative-Network/icn/issues/1634)).
- Broader federation, hosted operator defaults, and mobile productization remain phased work — see [PHASE_PROGRESS.md](../PHASE_PROGRESS.md).

## Accessible participation

Non-technical members should only need a **shell** (web, mobile, or assisted kiosk) that reads standing and action cards, guides one action at a time, and surfaces receipts in plain language. ICN keeps policy and meaning in apps and charters; the kernel enforces **constraints**, not social doctrine ([KERNEL_APP_SEPARATION.md](../architecture/KERNEL_APP_SEPARATION.md)).

## ActionCard package contract

Institution packages should validate card-shaped JSON against:

- `docs/contracts/institution-package/action-card.schema.json` (**experimental / RFC** — see `x-icn-status` in file)
- Package validation notes: `docs/contracts/institution-package/README.md`

Tracking: [#1713](https://github.com/InterCooperative-Network/icn/issues/1713). Organizer gate: [#1703](https://github.com/InterCooperative-Network/icn/issues/1703).

## Source of truth

- [STATE.md](../STATE.md), [PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
- [NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md](../strategy/NYCN_PHASE_2_PILOT_REHEARSAL_GATE.md)
- NYCN facilitator prep (checklists only, not a decision record):
  [REHEARSAL-0002 — Organizer gate prep](https://github.com/InterCooperative-Network/nycn/blob/main/docs/rehearsals/REHEARSAL-0002-organizer-gate-prep.md)
