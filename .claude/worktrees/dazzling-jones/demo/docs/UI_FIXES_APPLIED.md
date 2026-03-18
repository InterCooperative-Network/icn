# UI API Endpoint Alignment Notes

Date: 2026-02-12

This file tracks ledger endpoint alignment between `web/pilot-ui/app.js` and `icn-gateway`.

## Canonical Route Shape

Gateway routes are registered as:

- `GET /v1/ledger/{coop_id}/balance/{did}`
- `GET /v1/ledger/{coop_id}/history`
- `POST /v1/ledger/{coop_id}/payment`

UI uses matching client paths (without the `/v1` prefix because `apiRequest()` prepends it):

- ``/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}``
- ``/ledger/${state.coopId}/history?limit=50``
- ``/ledger/${state.coopId}/payment``

## Verification

Run:

```bash
./demo/scripts/quick-test.sh
```

This now checks the UI for canonical ledger route usage and validates live API health.
