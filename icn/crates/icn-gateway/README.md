# ICN Gateway API

Production-ready REST + WebSocket API server for ICN (Intercooperative Network) cooperative applications.

## Features

- **Three-Layer Security**: Authentication → Rate Limiting → Authorization
- **JWT-Based Auth**: DID-based challenge-response authentication
- **Scope-Based Authorization**: Fine-grained access control (read/write/admin)
- **Per-DID Rate Limiting**: Token bucket algorithm (100 burst, 10/sec refill)
- **API Versioning**: All endpoints under `/v1` namespace
- **Real-time Events**: WebSocket streaming for cooperative updates
- **Per-Coop Isolation**: Separate ledgers and namespaces per cooperative

## Quick Start

### As Part of ICN Daemon

```bash
# Method 1: Configuration file
cat > icn.toml << EOF
[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
jwt_secret = "your-strong-secret-here"
EOF

icnd --config icn.toml

# Method 2: Environment variable + CLI args
export ICN_GATEWAY_JWT_SECRET="your-strong-secret"
icnd --gateway-enable --gateway-bind 127.0.0.1:8080
```

### Standalone (Development/Testing)

```bash
cargo run --bin icn-gateway -- --bind 127.0.0.1:8080 --jwt-secret mysecret
```

## API Reference

### Public Endpoints

#### Health Check
```bash
GET /v1/health
```

Returns server health status.

#### Challenge/Verify Flow

```bash
# 1. Request challenge
POST /v1/auth/challenge
Content-Type: application/json

{
  "did": "did:icn:abc123..."
}

# Response: { "nonce": "hex-encoded-challenge" }

# 2. Sign and verify (returns JWT token)
POST /v1/auth/verify
Content-Type: application/json

{
  "did": "did:icn:abc123...",
  "signature": "hex-encoded-ed25519-signature",
  "coop_id": "my-coop",
  "scopes": ["ledger:read", "ledger:write", "coop:admin"]
}

# Response: { "token": "eyJ0eXAi..." }
```

### Protected Endpoints

All protected endpoints require `Authorization: Bearer <jwt-token>` header.

#### Cooperatives

```bash
# Create cooperative (requires coop:write scope)
POST /v1/coops
Content-Type: application/json
Authorization: Bearer <token>

{
  "id": "my-coop",
  "name": "My Cooperative"
}

# Get cooperative (requires coop:read scope)
GET /v1/coops/{id}
Authorization: Bearer <token>

# Update settings (requires coop:admin scope)
PUT /v1/coops/{id}/settings
Content-Type: application/json
Authorization: Bearer <token>

{
  "governance_model": "consensus",
  "credit_policy": "conservative",
  "currency": "hours"
}

# Delete cooperative (requires coop:admin scope)
DELETE /v1/coops/{id}
Authorization: Bearer <token>

# Add member (requires coop:admin scope)
POST /v1/coops/{id}/members
Content-Type: application/json
Authorization: Bearer <token>

{
  "did": "did:icn:xyz789...",
  "role": "member"  # or "admin", "owner"
}

# Remove member (requires coop:admin scope)
DELETE /v1/coops/{id}/members/{did}
Authorization: Bearer <token>

# Update member role (requires coop:admin scope)
PUT /v1/coops/{id}/members/{did}/role
Content-Type: application/json
Authorization: Bearer <token>

{
  "role": "admin"
}
```

#### Ledger

```bash
# Get balance (requires ledger:read scope)
GET /v1/ledger/{coop_id}/balance/{did}
Authorization: Bearer <token>

# Create payment (requires ledger:write scope)
POST /v1/ledger/{coop_id}/payment
Content-Type: application/json
Authorization: Bearer <token>

{
  "from": "did:icn:abc123...",
  "to": "did:icn:xyz789...",
  "amount": 10,
  "currency": "hours",
  "memo": "Payment for services"
}

# Get transaction history (requires ledger:read scope)
GET /v1/ledger/{coop_id}/history?did={optional_filter}
Authorization: Bearer <token>
```

#### WebSocket Events

```bash
# Connect to WebSocket
GET /v1/ws/{coop_id}
Upgrade: websocket

# After connection, authenticate:
{"type": "Auth", "token": "eyJ0eXAi..."}

# Server responses:
{"type": "AuthOk", "did": "did:icn:abc123..."}
{"type": "Event", "PaymentCreated": {...}}
{"type": "Event", "MemberAdded": {...}}
{"type": "Event", "RoleUpdated": {...}}
{"type": "Event", "SettingsUpdated": {...}}
{"type": "Error", "message": "..."}
```

## Authorization Scopes

| Scope | Description |
|-------|-------------|
| `ledger:read` | Query balances and transaction history |
| `ledger:write` | Create payments |
| `coop:read` | View cooperative information |
| `coop:write` | Create cooperatives |
| `coop:admin` | Member management and settings |

## Rate Limiting

- **Algorithm**: Token bucket
- **Capacity**: 100 tokens (burst)
- **Refill Rate**: 10 tokens/second (600 requests/minute sustained)
- **Tracking**: Independent per authenticated DID
- **Response**: HTTP 429 Too Many Requests when exceeded

## Prometheus Metrics

The gateway exposes comprehensive Prometheus metrics for production monitoring:

### Authentication Metrics
- `icn_gateway_auth_challenges_total` - Total challenges issued
- `icn_gateway_auth_verifications_total` - Total verification attempts
- `icn_gateway_auth_successes_total` - Successful authentications
- `icn_gateway_auth_failures_total` - Failed authentications (by reason)

### Authorization & Rate Limiting
- `icn_gateway_authorization_failures_total` - Authorization failures (by required scope)
- `icn_gateway_rate_limit_exceeded_total` - Rate limit violations (by DID)

### Request Metrics
- `icn_gateway_requests_total` - Total requests (by endpoint and method)
- `icn_gateway_request_duration_seconds` - Request latency histogram (by endpoint and status)

### WebSocket Metrics
- `icn_gateway_websocket_connections_active` - Current active WebSocket connections
- `icn_gateway_websocket_connections_total` - Total WebSocket connections established
- `icn_gateway_websocket_disconnections_total` - Total WebSocket disconnections
- `icn_gateway_websocket_messages_sent_total` - Total messages sent to clients

### Cooperative Metrics
- `icn_gateway_coops_created_total` - Total cooperatives created
- `icn_gateway_coops_deleted_total` - Total cooperatives deleted
- `icn_gateway_members_added_total` - Total members added
- `icn_gateway_members_removed_total` - Total members removed

### Ledger Metrics
- `icn_gateway_payments_created_total` - Total payments created
- `icn_gateway_payment_amount` - Payment amount distribution (by currency)
- `icn_gateway_balance_queries_total` - Total balance queries
- `icn_gateway_history_queries_total` - Total transaction history queries

All metrics include relevant labels for filtering and aggregation (e.g., `endpoint`, `method`, `status`, `currency`, `did`, `required_scope`, `reason`).

## Error Responses

All errors return JSON with error message:

```json
{
  "error": "Missing required scope: ledger:write"
}
```

| Status Code | Error Type | Description |
|-------------|------------|-------------|
| 400 | Bad Request | Invalid request data |
| 401 | Unauthorized | Missing or invalid JWT token |
| 403 | Forbidden | Insufficient permissions (scope check failed) |
| 404 | Not Found | Resource not found |
| 429 | Too Many Requests | Rate limit exceeded |
| 500 | Internal Server Error | Server-side error |

## Security Model

### Three-Layer Architecture

1. **Authentication Layer** (JWT middleware)
   - Verifies bearer token
   - Extracts and validates claims
   - Inserts TokenClaims into request extensions

2. **Rate Limiting Layer** (per-DID middleware)
   - Prevents abuse and resource exhaustion
   - Fair allocation across DIDs
   - Configurable limits per deployment

3. **Authorization Layer** (handler-level)
   - Scope-based access control
   - Fine-grained permissions
   - Prevents privilege escalation

### Request Flow

```
Request → JWT Auth → Rate Limiting → Authorization → Handler
             ↓            ↓              ↓
         Insert       Check DID      Check Scope
         Claims       Limit          Requirement
```

## Configuration

### Environment Variables

- `ICN_GATEWAY_JWT_SECRET`: JWT signing secret (required for gateway to start)

### CLI Arguments

- `--gateway-enable`: Enable gateway server
- `--gateway-bind <addr>`: Bind address (default: 127.0.0.1:8080)
- `--gateway-jwt-secret <secret>`: JWT secret (overrides env var)

### Configuration File (icn.toml)

```toml
[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
jwt_secret = "your-strong-secret-here"
token_expiry_hours = 24
challenge_ttl_minutes = 5
```

## Production Deployment

### Security Best Practices

1. **Use Strong JWT Secrets**: 32+ character random strings
2. **Enable TLS**: Use reverse proxy (nginx/caddy) for HTTPS
3. **Bind to Localhost**: For single-server deployments, bind to 127.0.0.1
4. **Monitor Rate Limits**: Track 429 responses for abuse detection
5. **Regular Rotation**: Rotate JWT secrets periodically

### Example Nginx Configuration

```nginx
server {
    listen 443 ssl http2;
    server_name api.mycoop.org;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # WebSocket support
    location /v1/ws/ {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Development

### Running Tests

```bash
cargo test -p icn-gateway
```

All 38 tests pass, including:
- 5 rate limiting tests
- 2 authorization tests
- 1 ownership verification test
- 30 functional tests (auth, coops, ledger, websocket, events)

Note: Metrics are instrumented throughout the codebase but don't require additional test coverage as they're fire-and-forget counters/histograms.

### Adding New Endpoints

1. Add handler function to appropriate module (`api/coops.rs`, `api/ledger.rs`, etc.)
2. Add scope check: `require_scope(&req, "scope:name")?`
3. Register in `server.rs` under `/v1` protected or public scope
4. Add tests with proper TokenClaims including scopes
5. Document in this README

## Architecture

```
icn-gateway/
├── src/
│   ├── api/           # REST endpoint handlers (with metrics instrumentation)
│   │   ├── auth.rs    # Challenge/verify endpoints
│   │   ├── coops.rs   # Cooperative management
│   │   ├── ledger.rs  # Ledger operations
│   │   ├── health.rs  # Health check
│   │   └── websocket.rs # WebSocket handler
│   ├── auth.rs        # JWT token management
│   ├── middleware.rs  # Authentication & authorization (with metrics)
│   ├── rate_limit.rs  # Token bucket rate limiter (with metrics)
│   ├── coop.rs        # Cooperative state manager
│   ├── ledger_mgr.rs  # Ledger operations wrapper
│   ├── events.rs      # Event broadcasting
│   ├── error.rs       # Error types and HTTP mapping
│   ├── models.rs      # Request/response DTOs
│   ├── websocket.rs   # WebSocket session management (with metrics)
│   └── server.rs      # Actix-web server setup
```

**Observability Integration:**
- Uses `icn-obs` crate for Prometheus metrics
- Metrics tracked at API layer (auth, coops, ledger)
- Middleware layer tracks authorization and rate limit violations
- WebSocket layer tracks connections, disconnections, and messages
- All metrics available via daemon's `/metrics` endpoint (default: `http://localhost:9090/metrics`)

## Known Limitations

- **In-Memory Storage**: Cooperative metadata and ledgers use temporary storage (Phase 14 scope)
- **No Persistent State**: Server restart clears all data (acceptable for Phase 14)
- **WebSocket Reconnection**: Manual reconnection required (deferred to pilot selection)

## Future Enhancements (Deferred)

- Persistent storage for cooperatives and ledgers
- WebSocket reconnection handling
- Event backfill for missed WebSocket messages
- TypeScript SDK (`@icn/client` npm package)
- Reference application for pilot community

## Contributing

See the main [ICN repository](https://github.com/InterCooperative-Network/icn) for contribution guidelines.

## License

Apache 2.0 - See LICENSE file in repository root
