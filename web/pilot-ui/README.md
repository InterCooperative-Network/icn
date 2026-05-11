# ICN Pilot UI

The ICN Pilot UI is the current organizer/member demo surface for ICN. It is a static HTML/CSS/JS application used to inspect member standing, action cards, governance activity, action items, receipts, and provenance through the ICN gateway or through bounded demo fixtures.

This is a pilot-stage interface for showing how ICN turns institutional standing and democratic decisions into action cards and receipt-backed evidence. It is not a production product.

## Current status

As of the current Phase 2 framing:

- Phase 0 is complete.
- Phase 1 is complete.
- Phase 2 is in progress and not complete.
- NYCN is the intended first cooperative partner and active partnership track, not a formal pilot.
- Member standing is a real gateway read model.
- Action cards are a real gateway read model.
- `?mode=demo` can render fixture-backed member standing and action cards without requiring a live gateway login.
- Governance, receipts, ledger, members, trust, and federation are not yet fully fixture-backed in demo mode.

Use the current truth anchors before making capability claims:

- `docs/STATE.md`
- `docs/PHASE_PROGRESS.md`
- `docs/reference/project-index/current-truth-map.md`
- `docs/reference/project-index/runtime-surface-map.md`
- `docs/reference/project-index/show-readiness-map.md`
- `docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md`

## What this UI is for

The Pilot UI helps answer a member or organizer's first practical questions:

- Who am I in this institution?
- What domains, roles, and authority scopes are attached to my standing?
- What action cards require my attention?
- Which governance decisions or action items are connected to those cards?
- Which receipts and provenance records prove what happened?

The strongest current story is:

```text
standing -> action cards -> governance action -> receipt -> provenance
```

The demo path currently proves only the first part of that story locally through fixtures:

```text
standing -> action cards
```

The next intended fixture slice is governance proposal/vote coverage, followed by receipt/provenance fixture coverage.

## Running locally

The UI is static. Serve it with any local web server:

```bash
cd web/pilot-ui
python -m http.server 3000
```

Then open:

```text
http://localhost:3000
```

To use the guided fixture-backed demo slice, open:

```text
http://localhost:3000?mode=demo
```

Demo mode currently uses committed fixture JSON under:

```text
web/pilot-ui/fixtures/icn-organizer-demo/
```

Those fixtures are fictional and must not contain real names, real DIDs, real contact data, or private organizer/member material.

## Gateway-backed mode

For live gateway-backed development, run an ICN daemon with the gateway enabled and connect the UI to the gateway URL.

Typical development gateway port:

```text
http://localhost:8080
```

The gateway port is 8080, not 8000.

Authentication and exact gateway startup commands depend on the current daemon configuration and should be checked against the current developer docs and `AGENTS.md` before use.

## Main surfaces

| Surface | Purpose |
|---|---|
| Profile / connection state | Shows the current local connection/session context. |
| My Standing & Action Cards | Member-facing standing plus the action-card queue. |
| Governance | Proposals and voting surfaces when gateway-backed. |
| Action Items | Domain action-item lists and completion flows. Distinct from Action Cards. |
| Receipts | Receipt/provenance viewing and plain-language explanation. |
| Members / domains | Supporting views for institutional context. |

## Action Cards vs Action Items

Action Cards are the per-member queue:

```text
GET /v1/gov/me/action-cards
```

They answer: what needs this member's attention?

Action Items are domain records. They answer: what work exists in this domain, and what is its status?

A good UI should help members move from a personal action card into the underlying domain action or governance object, then back to the receipt/provenance that proves completion.

## Proof-bearing source paths

The currently exercised action-card source paths are:

| Source/action | Receipt |
|---|---|
| `proposal` / `vote` | `GovernanceDecisionReceipt` |
| `action_item` / `complete` | `ActionItemCompletionReceipt` |
| `meeting` / `attend` | `MeetingAttendanceReceipt` |

Other source paths are not complete and must not be presented as finished.

## Tests

Useful local checks:

```bash
cd web/pilot-ui
npm ci
npm run test
npm run test:e2e
npm run test:a11y
```

When gateway or API shapes change, also check the relevant gateway/OpenAPI/SDK instructions in `AGENTS.md` and the project-index maps.

## Non-claims

This UI does not prove production readiness, formal NYCN pilot status, live federation, full fixture-backed demo mode, implemented service hosting, or complete mobile/member UX.

It is a pilot-stage human surface for inspecting and demonstrating the institutional proof loop as it becomes legible.
