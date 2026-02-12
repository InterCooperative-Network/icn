# ICN Gateway API - UI Integration Guide

Gateway base URL: `http://localhost:8080`  
API version: `v1`  
Full base: `http://localhost:8080/v1`

---

## Authentication Flow

1. Health check (no auth): `GET /v1/health`
2. Obtain JWT token (demo flow uses `icnctl auth token`)
3. Call protected endpoints with:

```http
Authorization: Bearer <token>
```

---

## Canonical Ledger Endpoints

The current gateway route shape is:

- `GET /v1/ledger/{coop_id}/balance/{did}`
- `GET /v1/ledger/{coop_id}/history?limit=50`
- `POST /v1/ledger/{coop_id}/payment`

UI calls in `web/pilot-ui/app.js` should stay aligned with these routes.

---

## Core Cooperative Endpoints

- `POST /v1/coops`
- `GET /v1/coops/{coop_id}`
- `GET /v1/coops/{coop_id}/stats`
- `POST /v1/coops/{coop_id}/members`
- `DELETE /v1/coops/{coop_id}/members/{did}`

---

## Governance Endpoints

- `POST /v1/gov/domains`
- `POST /v1/gov/proposals`
- `POST /v1/gov/proposals/{proposal_id}/votes`

---

## Integration Verification Checklist

- `GET /v1/health` returns `{"status":"ok", ...}`
- UI login calls `GET /v1/ledger/{coop_id}/balance/{did}` successfully
- Transaction history loads via `GET /v1/ledger/{coop_id}/history`
- Transaction creation works via `POST /v1/ledger/{coop_id}/payment`

---

## Quick Manual Test

```bash
# 1) Start demo
./demo/scripts/run-tool-library-demo.sh

# 2) Verify health
curl http://localhost:8080/v1/health

# 3) Open UI
# http://localhost:3000
# Sign in using script-provided credentials
```
