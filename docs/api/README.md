# ICN Gateway API (OpenAPI directory)

The ICN Gateway provides a REST and WebSocket API for cooperative applications.

## Quick Links

- [OpenAPI Specification](openapi.yaml) - Full API specification (use with Swagger UI or Redoc)
- [Pilot Coordinator Guide](../internal/pilots/pilot-coordinator-guide.md) - Deployment and operations guide

## Base URL

```
http://localhost:8080/v1
```

## Authentication

The Gateway uses DID-based challenge-response authentication with JWT tokens.

### Getting a Token

```bash
# 1. Request challenge
curl -X POST http://localhost:8080/v1/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did": "did:icn:YOUR_DID"}'

# Response: {"challenge": "abc123...", "expires_at": "..."}

# 2. Sign and verify (using icnctl)
TOKEN=$(icnctl auth token --coop my-coop)

# 3. Use token
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/coops/my-coop
```

## Endpoints Overview

### Health

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/v1/health` | Service health check |

### Authentication

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/auth/challenge` | Request auth challenge |
| POST | `/v1/auth/verify` | Verify signature, get JWT |

### Cooperatives

| Method | Endpoint | Scope Required | Description |
|--------|----------|----------------|-------------|
| GET | `/v1/coops` | `coop:read` | List cooperatives |
| POST | `/v1/coops` | `coop:write` | Create cooperative |
| GET | `/v1/coops/{id}` | `coop:read` | Get cooperative |
| PUT | `/v1/coops/{id}` | `coop:write` | Update cooperative |
| DELETE | `/v1/coops/{id}` | `coop:admin` | Delete cooperative |
| GET | `/v1/coops/{id}/members` | `coop:read` | List members |
| POST | `/v1/coops/{id}/members` | `coop:admin` | Add member |
| PUT | `/v1/coops/{id}/members/{did}` | `coop:admin` | Update member role |
| DELETE | `/v1/coops/{id}/members/{did}` | `coop:admin` | Remove member |

### Ledger

| Method | Endpoint | Scope Required | Description |
|--------|----------|----------------|-------------|
| GET | `/v1/ledger/{coop}/balance/{did}` | `ledger:read` | Get balance |
| POST | `/v1/ledger/{coop}/payment` | `ledger:write` | Create payment |
| GET | `/v1/ledger/{coop}/history` | `ledger:read` | Transaction history |

### Governance

| Method | Endpoint | Scope Required | Description |
|--------|----------|----------------|-------------|
| GET | `/v1/gov/domains` | `gov:read` | List domains |
| POST | `/v1/gov/domains` | `gov:write` | Create domain |
| GET | `/v1/gov/domains/{id}` | `gov:read` | Get domain |
| GET | `/v1/gov/domains/{id}/proposals` | `gov:read` | List proposals |
| POST | `/v1/gov/domains/{id}/proposals` | `gov:write` | Create proposal |
| GET | `/v1/gov/proposals/{id}` | `gov:read` | Get proposal |
| POST | `/v1/gov/proposals/{id}/open` | `gov:write` | Open voting |
| POST | `/v1/gov/proposals/{id}/close` | `gov:write` | Close & tally |
| POST | `/v1/gov/proposals/{id}/vote` | `gov:write` | Cast vote |
| GET | `/v1/gov/proposals/{id}/vote` | `gov:read` | Get vote tally |

### WebSocket

| Endpoint | Description |
|----------|-------------|
| `GET /v1/ws/{coop_id}` | Real-time event stream |

## Common Examples

### Create a Cooperative

```bash
curl -X POST http://localhost:8080/v1/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "food-coop",
    "name": "Downtown Food Cooperative",
    "description": "A community food buying club"
  }'
```

### Make a Payment

```bash
curl -X POST http://localhost:8080/v1/ledger/food-coop/payment \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "to": "did:icn:recipient",
    "amount": 2,
    "currency": "hours",
    "memo": "Garden help - 2 hours weeding"
  }'
```

### Create a Governance Proposal

```bash
curl -X POST http://localhost:8080/v1/gov/domains/coop:food/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Approve new supplier contract",
    "description": "Vote to approve the contract with Green Valley Farms",
    "kind": "text"
  }'
```

### Cast a Vote

```bash
curl -X POST http://localhost:8080/v1/gov/proposals/$PROPOSAL_ID/vote \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"choice": "for"}'
```

### Connect to WebSocket

```javascript
const ws = new WebSocket('ws://localhost:8080/v1/ws/food-coop');

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'Auth', token: TOKEN }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.type === 'AuthOk') {
    console.log('Authenticated as', msg.did);
  } else if (msg.type === 'Event') {
    console.log('Event:', msg);
  }
};
```

## Rate Limiting

All authenticated endpoints are rate-limited per DID:
- **Burst capacity**: 100 requests
- **Refill rate**: 10 tokens/second

When rate limited, the API returns HTTP 429 with a `Retry-After` header.

## Error Responses

All errors return JSON with `error` and `message` fields:

```json
{
  "error": "Forbidden",
  "message": "Missing required scope: ledger:write"
}
```

| Status | Meaning |
|--------|---------|
| 400 | Bad Request - Invalid parameters |
| 401 | Unauthorized - Missing/invalid token |
| 403 | Forbidden - Insufficient permissions |
| 404 | Not Found - Resource doesn't exist |
| 409 | Conflict - Resource already exists |
| 422 | Unprocessable - Business logic error (e.g., insufficient balance) |
| 429 | Too Many Requests - Rate limited |

## WebSocket Events

After authentication, you'll receive events as they occur:

| Event Type | Description |
|------------|-------------|
| `PaymentCreated` | New payment in the ledger |
| `MemberAdded` | Member joined cooperative |
| `MemberRemoved` | Member left cooperative |
| `RoleUpdated` | Member's role changed |
| `SettingsUpdated` | Cooperative settings changed |
| `GovernanceDomainCreated` | New governance domain |
| `GovernanceProposalCreated` | New proposal |
| `GovernanceProposalOpened` | Proposal opened for voting |
| `GovernanceProposalClosed` | Proposal closed with outcome |
| `GovernanceVoteCast` | Vote cast on proposal |

## Running Examples

Run the example script to test all API endpoints:

```bash
# Run health check only (no token needed)
./docs/api/examples.sh

# Run all examples with authentication
export TOKEN=$(icnctl auth token --coop my-coop)
./docs/api/examples.sh
```

The script demonstrates:
- Health checks
- Cooperative management
- Member management
- Ledger operations (balance, payments)
- Governance (domains, proposals, voting)

## Viewing the OpenAPI Spec

You can view the interactive API documentation using:

### Swagger UI

```bash
docker run -p 8081:8080 -e SWAGGER_JSON=/spec/openapi.yaml \
  -v $(pwd)/docs/api:/spec swaggerapi/swagger-ui
```

Then open http://localhost:8081

### Redoc

```bash
docker run -p 8081:80 -e SPEC_URL=/spec/openapi.yaml \
  -v $(pwd)/docs/api:/usr/share/nginx/html/spec redocly/redoc
```

Then open http://localhost:8081

## SDK Support

- **TypeScript**: See `sdk/typescript/` for the official TypeScript client
- **Python**: Coming soon
- **Rust**: Use `icn-gateway` crate directly

## License

MIT OR Apache-2.0
