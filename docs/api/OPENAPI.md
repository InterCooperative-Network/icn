# ICN Gateway API Documentation

## Overview

The ICN Gateway provides a comprehensive REST API and WebSocket interface for cooperative applications. Interactive API documentation is available via Swagger UI.

## Accessing API Documentation

When the gateway is running, navigate to:

```
http://localhost:3030/swagger-ui/
```

This provides an interactive interface to:
- Browse all available endpoints
- View request/response schemas
- Try API calls directly from the browser
- Download the OpenAPI specification

## OpenAPI Specification

The OpenAPI 3.0 specification is available at:

```
http://localhost:3030/api-docs/openapi.json
```

You can use this specification with:
- Code generators (e.g., `openapi-generator`)
- API testing tools (e.g., Postman, Insomnia)
- Documentation generators
- Client SDK generators

## API Structure

### Base URL

```
http://localhost:3030/v1
```

### Authentication

Most endpoints require JWT authentication:

1. **Get Challenge**: `POST /v1/auth/challenge`
2. **Verify Challenge**: `POST /v1/auth/verify`
3. **Use Token**: Include in `Authorization: Bearer <token>` header

### Endpoint Categories

#### Health & Status
- `GET /v1/health` - Liveness check
- `GET /v1/health/detailed` - Detailed health status
- `GET /v1/health/ready` - Readiness check

#### Authentication
- `POST /v1/auth/challenge` - Request authentication challenge
- `POST /v1/auth/verify` - Verify challenge and get JWT token

#### Identity Management
- `GET /v1/identity/:did` - Resolve DID to public key
- `GET /v1/identity/devices` - List registered devices
- `POST /v1/identity/devices` - Register new device
- `DELETE /v1/identity/devices/:device_id` - Remove device

#### Cooperatives
- `GET /v1/coops` - List cooperatives
- `POST /v1/coops` - Create cooperative
- `GET /v1/coops/:coop_id` - Get cooperative details
- `POST /v1/coops/:coop_id/join` - Join cooperative
- `DELETE /v1/coops/:coop_id/leave` - Leave cooperative
- `GET /v1/coops/:coop_id/members` - List members
- `GET /v1/coops/:coop_id/stats` - Get statistics

#### Ledger Operations
- `GET /v1/coops/:coop_id/ledger/balance` - Get balance
- `POST /v1/coops/:coop_id/ledger/transactions` - Record transaction
- `GET /v1/coops/:coop_id/ledger/transactions` - List transactions
- `GET /v1/coops/:coop_id/ledger/transactions/:tx_id` - Get transaction details
- `GET /v1/coops/:coop_id/ledger/audit` - Audit log

#### Trust Graph
- `GET /v1/trust/score/:did` - Get trust score
- `POST /v1/trust/edge` - Set trust edge
- `GET /v1/trust/network` - Get trust network
- `GET /v1/trust/network/:did` - Get trust network for DID

#### Governance
- `GET /v1/governance/proposals` - List proposals
- `POST /v1/governance/proposals` - Create proposal
- `GET /v1/governance/proposals/:proposal_id` - Get proposal details
- `POST /v1/governance/proposals/:proposal_id/vote` - Vote on proposal
- `GET /v1/governance/proposals/:proposal_id/votes` - List votes
- `GET /v1/governance/proposals/:proposal_id/votes/:did` - Get vote by DID

#### Compute Tasks
- `GET /v1/compute/tasks` - List tasks
- `POST /v1/compute/tasks` - Submit task
- `GET /v1/compute/tasks/:task_id` - Get task status
- `DELETE /v1/compute/tasks/:task_id` - Cancel task
- `GET /v1/compute/tasks/:task_id/result` - Get task result

#### Federation
- `POST /v1/federation/bridges` - Register federation bridge
- `GET /v1/federation/bridges/:federation_id` - Get bridge status
- `POST /v1/federation/route` - Route cross-federation message

#### Notifications
- `GET /v1/notifications` - List notifications
- `PATCH /v1/notifications/:notification_id/read` - Mark as read
- `POST /v1/notifications/devices` - Register device for push notifications
- `DELETE /v1/notifications/devices/:token` - Unregister device

#### WebSocket
- `GET /v1/websocket` - Establish WebSocket connection for real-time events

## Rate Limiting

The API implements rate limiting:
- **Default**: 100 requests per second per user
- **Auth endpoints**: 5 requests per minute per IP address

Rate limit information is included in response headers:
- `X-RateLimit-Limit`: Maximum requests allowed
- `X-RateLimit-Remaining`: Requests remaining in window
- `X-RateLimit-Reset`: Time when limit resets (Unix timestamp)

## Error Responses

All errors follow a consistent format:

```json
{
  "error": "Error message",
  "code": "ERROR_CODE",
  "details": {
    "field": "Additional context"
  }
}
```

Common HTTP status codes:
- `400 Bad Request` - Invalid request parameters
- `401 Unauthorized` - Missing or invalid authentication
- `403 Forbidden` - Insufficient permissions
- `404 Not Found` - Resource not found
- `429 Too Many Requests` - Rate limit exceeded
- `500 Internal Server Error` - Server error

## WebSocket Protocol

### Connection

Connect to `/v1/websocket` and upgrade to WebSocket protocol.

### Authentication

After connecting, send authentication message:

```json
{
  "type": "auth",
  "token": "your-jwt-token"
}
```

### Event Subscription

Subscribe to events:

```json
{
  "type": "subscribe",
  "channel": "ledger",
  "coop_id": "your-coop-id"
}
```

Available channels:
- `ledger` - Ledger transaction events
- `governance` - Governance proposal/vote events
- `compute` - Compute task status updates
- `trust` - Trust graph updates

### Event Messages

Events are delivered as JSON:

```json
{
  "type": "event",
  "channel": "ledger",
  "data": {
    "event_type": "transaction",
    "transaction": { ... }
  }
}
```

## Client SDKs

Official SDKs are available:

- **TypeScript**: `sdk/typescript/`
- **Python**: Coming soon
- **Rust**: Use `icn-rpc` crate directly

## Development Tools

### Generate Client Code

Using OpenAPI Generator:

```bash
# Download the OpenAPI spec
curl http://localhost:3030/api-docs/openapi.json > openapi.json

# Generate TypeScript client
openapi-generator-cli generate \
  -i openapi.json \
  -g typescript-fetch \
  -o ./generated-client
```

### Test with cURL

```bash
# Get challenge
CHALLENGE=$(curl -s -X POST http://localhost:3030/v1/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did":"did:icn:abc123"}' | jq -r '.challenge')

# Sign challenge (using your signing tool)
SIGNATURE="..."

# Verify and get token
TOKEN=$(curl -s -X POST http://localhost:3030/v1/auth/verify \
  -H "Content-Type: application/json" \
  -d "{\"did\":\"did:icn:abc123\",\"challenge\":\"$CHALLENGE\",\"signature\":\"$SIGNATURE\"}" \
  | jq -r '.token')

# Use token for authenticated requests
curl http://localhost:3030/v1/coops \
  -H "Authorization: Bearer $TOKEN"
```

## Monitoring

### Metrics

Prometheus metrics available at:

```
http://localhost:3030/metrics
```

### Health Checks

- **Liveness**: `GET /v1/health` - Returns 200 if server is running
- **Readiness**: `GET /v1/health/ready` - Returns 200 if ready to serve traffic

Use these for Kubernetes health probes.

## Security

### HTTPS

In production, always use HTTPS. The gateway should be deployed behind a reverse proxy (nginx, Traefik) that handles TLS termination.

### CORS

CORS is configured based on environment:
- **Development**: Permissive (all origins)
- **Production**: Strict (configured allowed origins)

Configure via `SecurityConfig` when initializing the gateway.

### Security Headers

The gateway automatically adds security headers:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Strict-Transport-Security` (in production)

## Examples

See `examples/` directory for:
- Mobile app integration
- Web app integration
- CLI usage
- WebSocket streaming

## Support

- **Documentation**: `docs/`
- **Issues**: https://github.com/InterCooperative-Network/icn/issues
- **Discussions**: https://github.com/InterCooperative-Network/icn/discussions
