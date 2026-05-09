---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Pilot UI Current State Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`runtime-surface-map.md`](runtime-surface-map.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This map routes the current pilot UI surface; it is not a production-readiness claim.

## Purpose

The pilot UI is the current human-facing organizer/member demo surface. It has moved faster than some older docs, so this map records the current boundary between fixture-backed, gateway-backed, partial, and not-yet surfaces.

## Source paths

| Area | Path |
|---|---|
| Pilot UI | `web/pilot-ui/` |
| Demo fixtures | `web/pilot-ui/fixtures/icn-organizer-demo/` |
| Runtime surface map | `docs/reference/project-index/runtime-surface-map.md` |
| Demo readiness map | `docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md` |
| ActionCard contract | `docs/contracts/institution-package/action-card.schema.json` |
| Governance HTTP models | `icn/apps/governance/src/http/` |

## Current status

| Surface | Current posture | Notes |
|---|---|---|
| Static UI shell | implemented | Vanilla HTML/CSS/JS PWA-style surface. |
| Gateway-backed mode | implemented but partial | Depends on running gateway and auth/session setup. |
| `?mode=demo` entry | implemented | Demo mode is a bounded UI mode, not full backend demo mode. |
| Member standing | implemented and fixture-backed | Fixture-backed in guided demo mode. |
| Action cards | implemented and fixture-backed | Fixture-backed in guided demo mode. |
| Governance | gateway-backed / not fully fixture-backed | Next intended fixture slice is proposal/vote. |
| Receipts/provenance | implemented but not fully fixture-backed | Plain-language framing exists; demo fixture coverage remains incomplete. |
| Action items | gateway-backed | Distinct from Action Cards. |
| Ledger/economics | partial / not current demo center | Avoid old product framing. |
| Members/trust/federation | partial / not fixture-backed | Do not present as complete local demo. |
| Accessibility | active requirement | Must be verified through tests/gates, not asserted by copy alone. |

## Demo boundary

Current local demo path:

```text
Standing -> Action Cards
```

Desired next demo path:

```text
Standing -> Action Cards -> Governance proposal/vote fixture
```

Later demo path:

```text
Standing -> Action Cards -> Governance proposal/vote -> Receipt/provenance fixture
```

Do not describe the current demo as full fixture-backed ICN. It is a first narrow slice of the organizer/member path.

## Action Cards vs Action Items

Action Cards are the per-member queue:

```text
GET /v1/gov/me/action-cards
```

Action Items are domain records. They are related, but they are not the same surface.

A good demo should help a member move:

```text
personal action card -> underlying domain action/governance object -> receipt/provenance
```

## Fixture rules

Demo fixtures must:

- use fictional identifiers;
- avoid real names, contact data, private organizer details, and real DIDs;
- match actual Rust/API/schema shapes;
- document which surfaces are covered;
- document which surfaces are not covered;
- preserve the difference between frontend fixture mode and a future backend demo mode.

## Current proof-bearing source paths

| Source/action | Receipt | Status |
|---|---|---|
| `proposal` / `vote` | `GovernanceDecisionReceipt` | implemented proof loop |
| `action_item` / `complete` | `ActionItemCompletionReceipt` | implemented proof loop |
| `meeting` / `attend` | `MeetingAttendanceReceipt` | implemented proof loop |

Other source paths must not be presented as complete unless current runtime evidence changes.

## Public/demo claims to avoid

Avoid claiming:

- production readiness;
- formal NYCN pilot status;
- live federation;
- full backend demo mode;
- all surfaces are fixture-backed;
- complete mobile/member UX;
- service hosting or Tool Commons runtime;
- old product framing as the current pilot UI identity.

## Local verification

When local tooling is available, relevant checks include:

```bash
cd web/pilot-ui
npm ci
npm run test
npm run test:e2e
npm run test:a11y
```

If gateway/API shapes change, also run the relevant OpenAPI/SDK drift checks described in `AGENTS.md` and `runtime-surface-map.md`.

## Related items

| Item | Purpose |
|---|---|
| #1779 | Public/demo-facing truth sync. |
| #1777 | Next governance proposal/vote fixture slice. |
| #1727 | Broader fixture-backed demo mode tracking; do not close without true backend/full demo mode. |
| `runtime-surface-map.md` | Current gateway/member-facing runtime surfaces. |
| `show-readiness-map.md` | What is safe to show externally. |
