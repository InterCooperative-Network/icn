# ICN Gateway API - UI Integration Guide

**Gateway Base URL:** `http://localhost:8080`  
**API Version:** `v1`  
**Full Base:** `http://localhost:8080/v1`

---

## Authentication Flow

### 1. Health Check (No Auth)
```
GET /v1/health
```
**Response:**
```json
{"status":"ok","version":"0.1.0"}
```

### 2. Get Auth Challenge (No Auth)
```
POST /v1/auth/challenge
Content-Type: application/json

{
  "did": "did:icn:...",
  "coop_id": "rochester-tool-library"
}
```

### 3. Verify Challenge (Get JWT)
```
POST /v1/auth/verify
Content-Type: application/json

{
  "did": "did:icn:...",
  "coop_id": "rochester-tool-library",
  "signature": "...",
  "challenge": "..."
}
```
**Response:**
```json
{
  "token": "eyJ...",
  "expires_at": 1234567890
}
```

**Note:** UI currently expects users to get token via `icnctl` and paste it in. This works but could be improved with proper challenge/response flow.

---

## UI Expected Endpoints

### Login Flow (app.js lines 418-450)

**UI calls:**
1. `GET /v1/health` - Test connection
2. `GET /v1/ledger/{coopId}/balance/{did}` - Verify auth

**Backend provides:**
- ✅ `GET /v1/health` (line 426)
- ✅ `GET /v1/ledger/coops/{coop_id}/balances/{did}` (line 473)

**MISMATCH:** UI path vs actual path!
- UI expects: `/v1/ledger/{coopId}/balance/{did}`
- Backend has: `/v1/ledger/coops/{coop_id}/balances/{did}`

**Fix needed:** Update UI to use correct path

---

## Core API Endpoints (All Require JWT)

### Cooperatives

#### Create Cooperative
```
POST /v1/coops
Authorization: Bearer {token}
Content-Type: application/json

{
  "id": "rochester-tool-library",
  "name": "Rochester Tool Library",
  "settings": {
    "governance_model": "consensus",
    "credit_policy": "conservative",
    "currency": "hours"
  }
}
```

#### Get Cooperative
```
GET /v1/coops/{coop_id}
Authorization: Bearer {token}
```

#### Get Stats
```
GET /v1/coops/{coop_id}/stats
Authorization: Bearer {token}
```

#### Add Member
```
POST /v1/coops/{coop_id}/members
Authorization: Bearer {token}
Content-Type: application/json

{
  "did": "did:icn:...",
  "role": "Member",
  "metadata": {
    "name": "Alice Chen",
    "email": "alice@demo.example",
    "skills": ["Woodworking", "Metal Fabrication"]
  }
}
```

#### Remove Member
```
DELETE /v1/coops/{coop_id}/members/{did}
Authorization: Bearer {token}
```

### Ledger

#### Get Balance
```
GET /v1/ledger/coops/{coop_id}/balances/{did}
Authorization: Bearer {token}
```

**Response:**
```json
{
  "did": "did:icn:...",
  "balance": 0.0,
  "currency": "hours"
}
```

#### Create Payment (Transaction)
```
POST /v1/ledger/coops/{coop_id}/payments
Authorization: Bearer {token}
Content-Type: application/json

{
  "from": "did:icn:...",
  "to": "did:icn:...",
  "amount": 2.5,
  "description": "Woodworking instruction - table saw safety"
}
```

#### Get History
```
GET /v1/ledger/coops/{coop_id}/history?did={did}&limit=50
Authorization: Bearer {token}
```

### Governance

#### Create Domain
```
POST /v1/gov/domains
Authorization: Bearer {token}
Content-Type: application/json

{
  "coop_id": "rochester-tool-library",
  "name": "Credit Policy",
  "description": "Policies related to credit limits and lending"
}
```

#### Create Proposal
```
POST /v1/gov/proposals
Authorization: Bearer {token}
Content-Type: application/json

{
  "coop_id": "rochester-tool-library",
  "domain_id": "...",
  "title": "Increase Credit Limit to 20 hours",
  "description": "...",
  "voting_period_days": 7
}
```

#### Cast Vote
```
POST /v1/gov/proposals/{proposal_id}/votes
Authorization: Bearer {token}
Content-Type: application/json

{
  "vote": "approve",
  "comment": "I support this change"
}
```

### Members

#### Get Member Profile
```
GET /v1/members/{did}
Authorization: Bearer {token}
```

### Trust

#### Get Trust Score
```
GET /v1/trust/{from_did}/{to_did}
Authorization: Bearer {token}
```

### Federation

#### List Federated Coops
```
GET /v1/federation/coops
Authorization: Bearer {token}
```

---

## WebSocket (Real-time Updates)

```javascript
const wsUrl = state.gatewayUrl.replace('http', 'ws') + '/v1/ws';
const ws = new WebSocket(wsUrl);

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  // Handle: transaction, balance_update, proposal_created, vote_cast, etc.
};
```

---

## UI Integration Issues Found

### 1. Balance Endpoint Path Mismatch ❌

**UI Code (app.js:436):**
```javascript
await apiRequest('GET', `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`);
```

**Actual Endpoint:**
```
GET /v1/ledger/coops/{coop_id}/balances/{did}
```

**Fix:**
```javascript
await apiRequest('GET', `/ledger/coops/${state.coopId}/balances/${encodeURIComponent(state.did)}`);
```

### 2. API Base Path ✅

**UI correctly uses:** `/v1{path}` (line 208)  
**Backend expects:** `/v1/*`  
**Status:** CORRECT ✅

### 3. Authorization Header ✅

**UI sends:** `Authorization: Bearer {token}` (line 214)  
**Backend expects:** `Authorization: Bearer {token}`  
**Status:** CORRECT ✅

### 4. Content-Type ✅

**UI sends:** `Content-Type: application/json` (line 210)  
**Backend expects:** `application/json`  
**Status:** CORRECT ✅

---

## CORS Configuration

**Current Config (demo.toml):**
```toml
[gateway]
cors_origins = ["http://localhost:3000", "http://localhost:8000"]
```

**Status:** CORRECT ✅

---

## Next Steps

### Immediate Fixes

1. **Fix Balance Endpoint in UI**
   ```javascript
   // In app.js line 436, change:
   `/ledger/${state.coopId}/balance/${encodeURIComponent(state.did)}`
   // To:
   `/ledger/coops/${state.coopId}/balances/${encodeURIComponent(state.did)}`
   ```

2. **Check for other endpoint mismatches**
   - Search UI code for all API calls
   - Compare with gateway routes
   - Update as needed

### Testing Sequence

1. Start gateway (already running)
2. Get JWT token via icnctl
3. Open UI at http://localhost:3000
4. Enter credentials:
   - Gateway: http://localhost:8080
   - Coop ID: rochester-tool-library
   - DID: did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh
   - Token: (from icnctl)
5. Try to login
6. Check browser console for errors

---

## Summary

**Working:**
- ✅ Health check endpoint
- ✅ Authentication flow (via icnctl)
- ✅ CORS configuration
- ✅ JWT authorization
- ✅ Content-Type headers

**Needs Fix:**
- ❌ Balance endpoint path (minor)
- 🟡 May need to check other endpoints

**Confidence:** HIGH - One small path fix should get login working

---

*Analysis Date: 2025-12-18 21:36 UTC*  
*Source: app.js + server.rs route comparison*
