# ICN Organizer/Member Demo — Fixture Pack

This directory holds the **first fixture slice** for the guided pilot-ui
organizer/member demo (the surface that pilot-ui exposes when opened
with `?mode=demo`). It is **not** a fixture pack for the legacy
tool-library demo, and the tool-library script is not affected by
anything in this directory.

## Status (read first)

- **Frontend-only fixture slice.** When pilot-ui is opened with
  `?mode=demo`, the standing and action-cards sections short-circuit
  to these committed JSON files instead of fetching the gateway. **No
  backend, no Rust, no runtime fixture mode.** The gateway-side
  `--demo-mode` flag described in
  [`docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md`](../../../docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md)
  §5 PR 5 remains **open**, tracked at ICN
  [#1727](https://github.com/InterCooperative-Network/icn/issues/1727).
- **Repo-safe.** All identifiers use the established
  `did:icn:example-*-not-live` convention. Display labels are
  fictional. No real names, real DIDs, real contact data, or real
  participant data appears anywhere.
- **Not the tool-library demo.** This pack drives the guided pilot-ui
  organizer/member demo. The legacy
  `demo/scripts/run-tool-library-demo.sh` flow is dev scaffolding and
  is **not** the user-facing ICN demo.
- **Not production deployment.** No live ICN runtime mutation, Google
  Drive / Groups / Sheets, federation, K3s, DNS, Forgejo,
  private-overlay, or cloud-sync activation. GitHub PR operations
  only.
- **Not real holder-label activation.** All DIDs are fictional and
  not bound to real persons.
- **Not Phase 2 completion.** Phase truth lives in
  [`docs/PHASE_PROGRESS.md`](../../../docs/PHASE_PROGRESS.md).
- **Not a formal pilot** of any kind.

## What this slice covers

| Surface | Endpoint shape | Fixture file |
|---|---|---|
| Member standing read-model | `GET /v1/gov/me/standing` shape (`StandingResponse`) | [`standing.json`](standing.json) |
| Per-member action-cards queue | `GET /v1/gov/me/action-cards` shape (array of `ActionCard`) | [`action-cards.json`](action-cards.json) |

The fixture content matches the field shapes in
[`docs/contracts/institution-package/action-card.schema.json`](../../../docs/contracts/institution-package/action-card.schema.json)
and the response struct definitions for `StandingResponse` /
`StandingDomainMembership` / `StandingRoleAssignment` in
`icn/apps/governance/src/http/models.rs`. Schema field names are used
verbatim — no invented fields.

## What this slice does NOT cover

The following pilot-ui surfaces remain unaffected by this fixture
pack and will continue to fetch the gateway / show empty states in
demo mode:

- Governance proposals / votes (`/v1/gov/...`)
- Receipt chain (`/v1/receipts/chain`) — pilot-ui's `receipts.js`
  still queries the gateway; the plain-language summary from PR 4
  shows its empty state when the chain is empty.
- Ledger / transaction history.
- Trust score.
- Members directory.
- Federation status.

Adding fixture coverage for any of those surfaces would expand this
PR beyond a narrow first slice and is explicitly deferred. ICN
[#1727](https://github.com/InterCooperative-Network/icn/issues/1727)
remains the tracking issue for true backend fixture mode (the
`--demo-mode` flag on icnd that serves committed fixtures from a
locked path at the request layer).

## Identifier convention

All fixture identifiers follow the established repo convention:

| Pattern | Use |
|---|---|
| `did:icn:example-*-not-live` | every DID in this pack |
| `demo.coop.*`, `demo.committee.*`, `demo.federation.*` | every entity / scope id |
| Fictional handles (e.g. "Demo organizer (fictional)") | every display label |

A grep for any pattern that suggests real data (real email domains,
real phone formats, real names) on this directory should return
zero matches; CI runs that grep on the new doc.

## How to use this in pilot-ui

1. Start pilot-ui locally (any path that serves `web/pilot-ui/` over
   HTTP — including the existing
   `demo/scripts/run-tool-library-demo.sh` if you're using it as
   dev scaffolding, but **note that script is not the ICN demo**).
2. Open the pilot-ui URL with `?mode=demo`.
3. Sign in (the auth-known DID populates the "I am" framing on the
   Demo Guide; standing data comes from this fixture).
4. Open the **My Standing & Action Cards** tab — fixture data
   renders in place of the gateway fetch.

## Out of scope (deferred)

- Backend `--demo-mode` flag on icnd.
- Gateway-side handler short-circuits.
- Federation / cross-cooperative coordination.
- Private-overlay / holder-label activation flows.
- Real organizer / sponsor / attendee data import.
- Live cloud sync (Drive / Sheets / Groups / Calendar / mail).
- K3s / DNS / Forgejo / production-deploy mutation.
- Production deployment posture.

## Cross-references

- ICN [#1727](https://github.com/InterCooperative-Network/icn/issues/1727) — fixture-backed demo mode (tracking issue; remains OPEN after this PR).
- [`docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md`](../../../docs/demo/ICN_SYSTEM_DEMO_READINESS_MAP.md) — sequenced demo-readiness PR plan.
- [`docs/contracts/institution-package/action-card.schema.json`](../../../docs/contracts/institution-package/action-card.schema.json) — action-card schema this pack matches.
- `icn/apps/governance/src/http/models.rs` — standing response struct definitions this pack matches.
- ICN [#1768](https://github.com/InterCooperative-Network/icn/pull/1768) — Invariant 6 (opaque receipt storage), referenced by PR 4's receipt summary.
