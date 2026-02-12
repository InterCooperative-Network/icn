# ICN Gateway API Reference

Quick reference guide for the ICN Gateway REST API.

> Snapshot note: this file contains simplified examples. Authoritative route definitions are in `docs/api/openapi.yaml` and `docs/api/OPENAPI.md`.

## Base URL

```
http://localhost:8080      # Local development
https://gateway.icn.example # Production
```

## Authentication

Most endpoints require Bearer token authentication. Public endpoints include `/v1/health`, `/v1/auth/challenge`, and `/v1/auth/verify`:

```bash
curl -H "Authorization: Bearer YOUR_TOKEN" https://gateway.icn.example/v1/coops/my-coop
```

## Quick Start

### 1. Check Service Health

```bash
curl http://localhost:8080/v1/health
```

### 2. Get Your DID

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/identity/did
```

### 3. Check Your Balance

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/ledger/balance
```

### 4. Send a Transaction

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "to": "did:icn:abc123",
    "amount": 100,
    "description": "Payment for services"
  }' \
  http://localhost:8080/ledger/transaction
```

## API Categories

### Identity & Auth
- `GET /identity/did` - Get your DID
- `GET /identity/public-key` - Get your public key

### Trust Graph
- `GET /trust/score/{did}` - Get trust score for a DID
- `POST /trust/edge` - Create/update trust relationship

### Ledger (Mutual Credit)
- `GET /ledger/balance` - Get your balance
- `POST /ledger/transaction` - Record a transaction
- `GET /ledger/history` - Get transaction history

### Governance
- `GET /governance/proposals` - List proposals
- `POST /governance/proposals` - Create proposal
- `POST /governance/proposals/{id}/vote` - Vote on proposal

### Cooperatives
- `GET /coops` - List cooperatives
- `POST /coops` - Create cooperative
- `GET /coops/{id}` - Get cooperative details
- `GET /coops/{id}/members` - List members
- `POST /coops/{id}/members` - Add member

### Distributed Compute
- `GET /compute/tasks` - List compute tasks
- `POST /compute/tasks` - Submit task
- `GET /compute/tasks/{id}` - Get task details

### Notifications
- `GET /notifications` - List notifications
- `POST /notifications/{id}/read` - Mark as read

### Real-time Events
- `GET /ws` - WebSocket connection

## Common Patterns

### Pagination

Most list endpoints support pagination:

```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/ledger/history?limit=50&offset=0"
```

Parameters:
- `limit`: Number of results (max 100, default 50)
- `offset`: Starting position (default 0)

### Filtering

Many endpoints support filtering:

```bash
# Filter proposals by status
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/governance/proposals?domain=my-coop&status=active"

# Filter tasks by status
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/compute/tasks?status=running"
```

### Error Responses

Standard HTTP status codes:
- `200` - Success
- `201` - Created
- `400` - Bad Request (validation error)
- `401` - Unauthorized (missing/invalid token)
- `403` - Forbidden (insufficient permissions)
- `404` - Not Found
- `500` - Internal Server Error

Error response format:

```json
{
  "error": "Error message",
  "code": "ERROR_CODE",
  "details": {}
}
```

## Examples

### Create a Cooperative

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Tech Workers Coop",
    "coop_type": "worker",
    "description": "A cooperative for tech workers"
  }' \
  http://localhost:8080/coops
```

### Create a Governance Proposal

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain": "my-coop",
    "title": "Increase Credit Limit",
    "description": "Proposal to increase max credit limit to 10000",
    "rule_ccl": "rule \"increase_limit\" { set_limit(10000) }",
    "voting_duration_secs": 604800
  }' \
  http://localhost:8080/governance/proposals
```

### Submit a Compute Task

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "contract_id": "data-analysis-v1",
    "function": "analyze_sales",
    "args": {
      "month": "2025-12",
      "region": "west"
    },
    "min_trust": 0.5
  }' \
  http://localhost:8080/compute/tasks
```

### Cast a Vote

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "choice": "approve"
  }' \
  http://localhost:8080/governance/proposals/prop-123/vote
```

### Create a Trust Edge

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "target_did": "did:icn:abc123",
    "score": 0.8
  }' \
  http://localhost:8080/trust/edge
```

## WebSocket Events

Connect to WebSocket for real-time events:

```javascript
const ws = new WebSocket('ws://localhost:8080/v1/ws/my-coop', {
  headers: {
    'Authorization': `Bearer ${token}`
  }
});

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('Event:', data);
};
```

Event types:
- `transaction` - New transaction recorded
- `proposal_created` - New proposal created
- `vote_cast` - Vote cast on proposal
- `task_completed` - Compute task completed
- `notification` - New notification

## Rate Limiting

API is rate-limited by trust score:
- High trust (0.8+): 1000 req/min
- Medium trust (0.5-0.8): 500 req/min
- Low trust (0.3-0.5): 100 req/min
- Below threshold (<0.3): 10 req/min

Rate limit headers:
```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 995
X-RateLimit-Reset: 1734422400
```

## SDK Usage

### TypeScript

```typescript
import { ICNClient } from '@icn/sdk';

const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
  token: 'YOUR_TOKEN'
});

// Get DID
const { did } = await client.getDID();

// Send transaction
await client.sendTransaction({
  to: 'did:icn:abc123',
  amount: 100,
  description: 'Payment'
});

// Create proposal
const proposal = await client.createProposal({
  domain: 'my-coop',
  title: 'New Policy',
  description: 'Proposal description',
  rule_ccl: 'rule "policy" { approve() }'
});
```

## Security

### TLS
All production endpoints use TLS 1.3.

### Authentication
JWT tokens with DID-based claims.

### Request Signing
Optional Ed25519 request signing for critical operations.

### Trust Gating
Many operations require minimum trust scores:
- Compute execution: 0.3
- High-value transactions: 0.5
- Governance proposals: 0.4

## Documentation

- Full OpenAPI spec: `docs/openapi.yaml`
- Architecture: [../../ARCHITECTURE.md](../../ARCHITECTURE.md)
- Getting Started: [../../GETTING_STARTED.md](../../GETTING_STARTED.md)
- SDK Docs: `sdk/typescript/README.md`

## Support

- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Discussions: https://github.com/InterCooperative-Network/icn/discussions
- Email: dev@icn.coop
