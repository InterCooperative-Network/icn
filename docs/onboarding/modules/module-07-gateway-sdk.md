# Module 7: Gateway API and SDK

## Learning Objectives

By the end of this module, you will:
- Understand the gateway's role as system boundary
- Master the DID-based authentication flow
- Configure and use JWT scopes for authorization
- Implement rate limiting and security policies
- Use the TypeScript SDK for client applications
- Subscribe to real-time events via WebSocket

## Prerequisites

- Module 6 (Ledger and Contracts)
- Basic understanding of HTTP/REST APIs
- Familiarity with JWT tokens

## Key Reading

- `icn/crates/icn-gateway/src/server.rs`
- `icn/crates/icn-gateway/src/api/auth.rs`
- `icn/crates/icn-gateway/src/api/ledger.rs`
- `sdk/typescript/src/index.ts`

---

## Concepts (Textbook Style)

### The Gateway as System Boundary

The gateway serves as ICN's **integration surface** - the boundary between external applications
and the internal ICN runtime. This architectural decision provides several important guarantees:

```
External World                    ICN Runtime

  Mobile Apps    ─┐               ┌─ Actors
  Web Apps       ─┼── Gateway ────┼─ Ledger
  Third-Party    ─┼── (Actix)     ├─ Governance
  Services       ─┘               └─ Trust Graph

        HTTP/WS          Actor Handles
        JSON             Native Types
        JWT Auth         DID Identity
```

**Why a dedicated gateway?**

1. **Protocol Translation**: External apps speak HTTP/JSON; ICN actors communicate via typed
   messages and channels. The gateway translates between these worlds.

2. **Security Boundary**: All external requests pass through authentication, authorization,
   rate limiting, and validation before touching internal systems.

3. **API Stability**: Internal actor interfaces can evolve independently of the external API.
   The gateway provides a stable contract for application developers.

4. **Observability**: Centralized request handling enables consistent logging, metrics, and
   tracing across all API operations.

### Gateway Architecture

The gateway is built on Actix-web, Rust's high-performance async web framework:

```
                    ┌─────────────────────────────────────────┐
                    │              GatewayServer              │
                    │                                         │
    HTTP Request    │  ┌─────────┐   ┌─────────┐   ┌──────┐ │
    ─────────────── ▶  │ Tracing │──▶│  CORS   │──▶│ Auth │ │
                    │  └─────────┘   └─────────┘   └──────┘ │
                    │       │                          │     │
                    │       ▼                          ▼     │
                    │  ┌─────────┐   ┌─────────┐   ┌──────┐ │
                    │  │Compress │   │Security │   │ Rate │ │
                    │  │         │   │ Headers │   │Limit │ │
                    │  └─────────┘   └─────────┘   └──────┘ │
                    │                      │               │
                    │                      ▼               │
                    │              ┌──────────────┐        │
                    │              │   API Route  │        │
                    │              │   Handler    │        │
                    │              └──────────────┘        │
                    │                      │               │
                    │                      ▼               │
                    │  ┌─────────────────────────────────┐ │
                    │  │          Manager Layer          │ │
                    │  │  Auth │ Coop │ Ledger │ Gov    │ │
                    │  └─────────────────────────────────┘ │
                    │                      │               │
                    │                      ▼               │
                    │  ┌─────────────────────────────────┐ │
                    │  │     Actor Handles (Optional)    │ │
                    │  │  TrustGraph │ Governance │ Coop │ │
                    │  └─────────────────────────────────┘ │
                    └─────────────────────────────────────────┘
```

**Key Components**:

| Component | Purpose |
|-----------|---------|
| **AuthManager** | Challenge/nonce generation, JWT issuance and validation |
| **CoopManager** | Cooperative CRUD, membership management |
| **LedgerManager** | Payment creation, balance queries, history |
| **GovernanceManager** | Proposals, voting, domains |
| **TrustManager** | Trust score queries, attestation management |
| **EventBroadcaster** | WebSocket event distribution |

**Manager Modes**:

Managers can operate in two modes:

1. **Standalone** (in-memory): For testing or isolated deployments
2. **Actor-backed**: Delegates to daemon actors for persistence and gossip sync

```rust
// Actor-backed mode - delegates to daemon's TrustGraph
let trust_manager = if let Some(handle) = self.trust_graph_handle {
    info!("Trust manager connected to daemon (using TrustGraph actor)");
    Arc::new(TrustManager::with_handle(handle))
} else {
    info!("Trust manager running standalone (in-memory only)");
    Arc::new(TrustManager::new())
};
```

---

## Authentication Flow

ICN uses DID-based authentication via challenge-response. This proves identity without
transmitting private keys.

### Step 1: Challenge Request

The client requests a challenge for their DID:

```
POST /v1/auth/challenge
{
  "did": "did:icn:5Kd8xJz..."
}

Response:
{
  "nonce": "a7f3b2c1d4e5...",  // 32-byte hex-encoded random nonce
  "expires_in": 300            // 5 minutes
}
```

**Security considerations**:
- IP-based rate limiting prevents brute-force attacks
- Nonces expire after 5 minutes
- Each nonce can only be used once

```rust
// IP-based rate limiting for unauthenticated endpoints
let client_ip = get_client_ip(&http_req);
ip_limiter.check_rate_limit(&client_ip)?;

let did = req.did.parse()
    .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

let nonce = auth.create_challenge(&did)?;
```

### Step 2: Challenge Verification

The client signs the nonce with their Ed25519 private key and submits for verification:

```
POST /v1/auth/verify
{
  "did": "did:icn:5Kd8xJz...",
  "signature": "3a4b5c6d...",    // hex-encoded Ed25519 signature
  "coop_id": "food-coop",        // cooperative context
  "scopes": ["ledger:read", "ledger:write"]
}

Response:
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "expires_in": 3600             // 1 hour
}
```

**Validation sequence**:

```rust
// 1. Validate DID format
let did = req.did.parse()?;

// 2. Validate requested scopes
validation::validate_scopes(&req.scopes)?;

// 3. Validate cooperative ID format
validation::validate_coop_id(&req.coop_id)?;

// 4. Decode signature from hex
let signature = hex::decode(&req.signature)?;

// 5. Validate signature length (Ed25519 = 64 bytes)
if signature.len() != 64 {
    return Err(GatewayError::BadRequest(
        format!("Invalid signature length: expected 64, got {}", signature.len())
    ));
}

// 6. Verify signature against challenge
let token = auth.verify_challenge(&did, &signature, &req.coop_id, req.scopes)?;
```

### JWT Token Structure

The issued JWT contains claims for authorization:

```json
{
  "sub": "did:icn:5Kd8xJz...",   // Subject (DID)
  "coop_id": "food-coop",        // Cooperative context
  "scopes": ["ledger:read", "ledger:write"],
  "iat": 1704067200,             // Issued at
  "exp": 1704070800              // Expires (1 hour)
}
```

### Step 3: Authenticated Requests

Include the token in subsequent requests:

```
GET /v1/ledger/food-coop/balance/did:icn:5Kd8xJz...
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
```

---

## Scopes and Authorization

Scopes define what operations a token can perform. ICN follows a `resource:action` pattern.

### Available Scopes

| Scope | Permissions |
|-------|-------------|
| `ledger:read` | Query balances and transaction history |
| `ledger:write` | Create payments |
| `coop:read` | View cooperative details and members |
| `coop:write` | Modify cooperative settings, manage members |
| `gov:read` | View proposals, domains, votes |
| `gov:write` | Create proposals, cast votes |
| `trust:read` | Query trust scores and edges |
| `trust:write` | Create/revoke trust attestations |
| `compute:submit` | Submit compute tasks |
| `oracle:read` | Query exchange rates |
| `oracle:write` | Set manual exchange rates |

### Scope Enforcement

Handlers enforce scopes via middleware:

```rust
// In ledger.rs - payment endpoint
pub async fn create_payment(
    http_req: HttpRequest,
    // ...
) -> Result<HttpResponse> {
    // Verify scope
    require_scope(&http_req, "ledger:write")?;

    // Verify cooperative access
    require_coop_access(&http_req, &coop_id)?;

    // Verify sender identity matches authenticated DID
    let claims = get_claims(&http_req)?;
    if claims.sub != req.from {
        return Err(GatewayError::AuthorizationFailed(
            format!("Cannot create payments from other accounts")
        ));
    }

    // ... proceed with payment
}
```

---

## Rate Limiting

ICN implements multiple layers of rate limiting to protect against abuse.

### IP-Based Rate Limiting

Unauthenticated endpoints (challenge, verify) use IP-based limits:

```rust
pub struct IpRateLimiter {
    buckets: DashMap<String, TokenBucket>,
    config: RateLimitConfig,
}

impl IpRateLimiter {
    pub fn check_rate_limit(&self, ip: &str) -> Result<(), GatewayError> {
        let mut bucket = self.buckets
            .entry(ip.to_string())
            .or_insert_with(|| TokenBucket::new(self.config.capacity));

        if !bucket.consume(self.config.cost_per_request) {
            return Err(GatewayError::RateLimited(
                "Too many requests from this IP".to_string()
            ));
        }
        Ok(())
    }
}
```

**Default limits**:
- Burst capacity: 20 requests
- Refill rate: 2 tokens/second
- Cost per request: 1 token

### Trust-Gated Velocity Limiting

Transaction endpoints use trust-based velocity limits:

```
Trust Class      │ Transactions/Hour │ Trust Score Range
─────────────────┼───────────────────┼──────────────────
Isolated         │        10         │    < 0.1
Known            │        50         │  0.1 - 0.4
Partner          │       100         │  0.4 - 0.7
Federated        │       200         │    > 0.7
```

```rust
pub struct VelocityLimiter {
    windows: DashMap<String, VelocityWindow>,
    config: VelocityLimitConfig,
}

impl VelocityLimiter {
    pub fn check_velocity(&self, did: &str, trust_score: f64) -> Result<(), GatewayError> {
        let limit = self.config.limit_for_trust_class(trust_score);
        let mut window = self.windows
            .entry(did.to_string())
            .or_insert_with(VelocityWindow::new);

        if window.count >= limit {
            return Err(GatewayError::RateLimited(
                format!("Velocity limit exceeded: {}/{} tx/hour", window.count, limit)
            ));
        }

        window.increment();
        Ok(())
    }
}
```

### Authenticated Rate Limiting

Authenticated endpoints use DID-based rate limiting:

```rust
// Middleware extracts DID from JWT and applies rate limit
pub async fn rate_limit_middleware(
    req: ServiceRequest,
    next: Next<Body>,
) -> Result<ServiceResponse<Body>, Error> {
    if let Some(claims) = get_claims(&req) {
        let limiter = req.app_data::<Data<Arc<RateLimiter>>>()?;
        limiter.check_rate_limit(&claims.sub)?;
    }
    next.call(req).await
}
```

---

## WebSocket Events

The gateway provides real-time event streaming via WebSocket.

### Connection Flow

```
Client                          Gateway
   │                               │
   │──── Connect /v1/ws/{coop} ───▶│
   │                               │
   │◀────── Connection Open ───────│
   │                               │
   │──── { type: "Auth",      ────▶│
   │       token: "eyJ..." }       │
   │                               │
   │◀────── { type: "AuthOk",  ────│
   │          did: "did:icn:..",   │
   │          current_seq: 42 }    │
   │                               │
   │◀──── { type: "Event",    ─────│  (real-time events)
   │        seq: 43,               │
   │        event: {...} }         │
   │                               │
```

### Event Types

```typescript
type CoopEventType =
  | 'PaymentCreated'
  | 'MemberAdded'
  | 'MemberRemoved'
  | 'RoleUpdated'
  | 'SettingsUpdated'
  | 'GovernanceDomainCreated'
  | 'GovernanceProposalCreated'
  | 'GovernanceProposalOpened'
  | 'GovernanceProposalClosed'
  | 'GovernanceVoteCast'
  | 'ComputeTaskSubmitted'
  | 'ComputeTaskClaimed'
  | 'ComputeTaskCompleted'
  | 'ComputeTaskCancelled'
  | 'Shutdown';
```

### Sequence Numbers and Backfill

Events have sequence numbers for reliable delivery:

```typescript
// AuthOk includes current sequence
interface WsAuthOkMessage {
  type: 'AuthOk';
  did: string;
  current_seq: number;  // Highest sequence number
}

// Events include their sequence
interface WsEventMessage {
  type: 'Event';
  seq: number;          // Event sequence number
  event: GatewayEventPayload;
}

// Request missed events after reconnection
interface WsBackfillMessage {
  type: 'Backfill';
  after_seq: number;    // Get events after this sequence
}
```

### Graceful Shutdown

The gateway notifies clients before shutdown:

```typescript
interface WsShutdownMessage {
  type: 'Shutdown';
  reason: string;
  reconnect_after_ms: number | null;  // Suggested reconnect delay
}
```

---

## TypeScript SDK

The SDK provides a typed client for the gateway API.

### Client Creation

```typescript
import { ICNClient } from '@icn/client';

const client = new ICNClient({
  baseUrl: 'https://gateway.icn.example.com',
  timeout: 30000,        // Request timeout (ms)
  retry: {
    maxRetries: 3,       // Retry failed requests
    initialDelayMs: 1000,
    backoffMultiplier: 2,
  },
  autoRefresh: true,     // Auto-refresh expired tokens
});
```

### Authentication

**Manual flow**:

```typescript
// 1. Get challenge
const challenge = await client.getChallenge('did:icn:abc123');

// 2. Sign with your Ed25519 private key
const signature = await signWithEd25519(challenge.nonce, privateKey);

// 3. Verify and get token
const auth = await client.verify(
  'did:icn:abc123',
  signature,
  'food-coop',
  ['ledger:read', 'ledger:write']
);

// 4. Set token for future requests
client.setToken(auth.token, auth.expires_at);
```

**Using SignatureProvider** (cleaner):

```typescript
// Create a signature provider
const signer: SignatureProvider = {
  async sign(challenge: string): Promise<string> {
    const signature = await ed25519.sign(
      new TextEncoder().encode(challenge),
      privateKey
    );
    return Buffer.from(signature).toString('hex');
  }
};

// Authenticate (handles full flow)
const auth = await client.authenticate(
  'did:icn:abc123',
  signer,
  'food-coop',
  ['ledger:read', 'ledger:write']
);
```

### Ledger Operations

```typescript
// Get balance
const balance = await client.getBalance('food-coop', 'did:icn:alice');
console.log(`Balance: ${balance.balance} ${balance.currency}`);

// Create payment
const payment = await client.pay('food-coop', {
  from: 'did:icn:alice',
  to: 'did:icn:bob',
  amount: 2.5,
  currency: 'hours',
  memo: 'Garden help',
});

// Get transaction history
const history = await client.getHistory('food-coop', {
  offset: 0,
  limit: 50,
});

// Query builder for complex filters
const filtered = await client.queryHistory('food-coop')
  .fromDid('did:icn:alice')
  .lastDays(30)
  .limit(100)
  .execute();
```

### Cross-Currency Payments

```typescript
// Get a quote first
const quote = await client.getCrossPaymentQuote('food-coop', {
  amount: 10,
  from_currency: 'hours',
  to_currency: 'USD',
});

console.log(`10 hours = ${quote.net_target_amount} USD`);
console.log(`Rate: ${quote.rate}, Valid until: ${quote.valid_until}`);

// Execute with slippage protection
const result = await client.crossPay('food-coop', {
  from: 'did:icn:alice',
  to: 'did:icn:bob',
  amount: 10,
  from_currency: 'hours',
  to_currency: 'USD',
  max_target_amount: Math.floor(quote.net_target_amount * 1.01), // 1% slippage
});
```

### Governance

```typescript
// Create a domain
const domain = await client.createDomain({
  domain_id: 'budget-2025',
  name: 'Budget Decisions 2025',
  members: ['did:icn:alice', 'did:icn:bob', 'did:icn:carol'],
});

// Create a proposal
const proposal = await client.createProposal({
  domain_id: 'budget-2025',
  title: 'Allocate funds for community garden',
  description: 'Proposal to allocate 500 hours for garden renovation',
  kind: 'budget',
});

// Open for voting
await client.openProposal(proposal.id);

// Cast vote
await client.vote(proposal.id, { choice: 'for' });

// Get results
const tally = await client.getVotes(proposal.id);
console.log(`For: ${tally.votes_for}, Against: ${tally.votes_against}`);

// Close and get outcome
const outcome = await client.closeProposal(proposal.id);
if (outcome.accepted) {
  console.log('Proposal passed!');
}
```

### Vote Delegation

```typescript
// Blanket delegation - delegate can vote on all proposals
await client.createDelegation({
  delegate: 'did:icn:trusted-delegate',
  scope: 'blanket',
});

// Domain-scoped delegation
await client.createDelegation({
  delegate: 'did:icn:budget-expert',
  scope: 'domain:budget-2025',
  expires_at: Math.floor(Date.now() / 1000) + 86400 * 30, // 30 days
});

// Single-proposal delegation
await client.createDelegation({
  delegate: 'did:icn:advisor',
  scope: 'proposal:prop-123',
});

// Revoke delegation
await client.revokeDelegation('delegation-456');
```

### WebSocket Subscriptions

**Basic connection**:

```typescript
const ws = client.connectWebSocket('food-coop', {
  onOpen: () => console.log('Connected'),
  onMessage: (msg) => console.log('Event:', msg),
  onError: (err) => console.error('Error:', err),
  onClose: () => console.log('Disconnected'),
});
```

**Managed subscription with auto-reconnect**:

```typescript
const subscription = client.subscribe('food-coop', {
  onEvent: (event) => {
    if (event.type === 'Event') {
      console.log('Received event:', event.seq, event.event);
    }
  },
  onAuthOk: (did, seq) => {
    console.log(`Authenticated as ${did}, current seq: ${seq}`);
  },
  onReconnect: (attempt) => {
    console.log(`Reconnecting... attempt ${attempt}`);
  },
  onBackfillComplete: (count) => {
    console.log(`Backfilled ${count} missed events`);
  },
  onStateChange: (state) => {
    console.log(`Connection state: ${state}`);
  },
}, {
  autoReconnect: true,
  maxReconnectAttempts: 10,
  autoBackfill: true,        // Request missed events after reconnect
  gapDetection: true,        // Detect and fill sequence gaps
});

// Later: close subscription
subscription.close();
```

**Event filtering**:

```typescript
import { EventFilter } from '@icn/client';

// Filter by event type
const paymentFilter = EventFilter.payments();

// Filter by DID
const aliceFilter = EventFilter.byDid('did:icn:alice');

// Combine filters
const alicePayments = EventFilter.and(paymentFilter, aliceFilter);

subscription = client.subscribe('food-coop', {
  onEvent: (event) => {
    if (alicePayments(event)) {
      console.log('Alice payment:', event);
    }
  },
});
```

### Batch Operations

```typescript
// Batch payments
const results = await client.batchPay('food-coop', [
  { from: 'did:icn:alice', to: 'did:icn:bob', amount: 2, currency: 'hours' },
  { from: 'did:icn:alice', to: 'did:icn:carol', amount: 1, currency: 'hours' },
  { from: 'did:icn:alice', to: 'did:icn:dave', amount: 3, currency: 'hours' },
]);

console.log(`${results.succeeded} succeeded, ${results.failed} failed`);
for (const r of results.results) {
  if (!r.success) {
    console.error('Failed:', r.error);
  }
}
```

### Error Handling

```typescript
import {
  ICNError,
  AuthenticationError,
  AuthorizationError,
  ValidationError,
  RateLimitError,
  NotFoundError,
} from '@icn/client';

try {
  await client.pay('food-coop', { ... });
} catch (e) {
  if (e instanceof AuthenticationError) {
    // Token expired - re-authenticate
    await client.authenticate(did, signer, coopId, scopes);

  } else if (e instanceof AuthorizationError) {
    console.error('Insufficient permissions:', e.requiredPermissions);

  } else if (e instanceof ValidationError) {
    // Field-level errors
    for (const [field, errors] of Object.entries(e.fields)) {
      console.error(`${field}: ${errors.join(', ')}`);
    }

  } else if (e instanceof RateLimitError) {
    console.error(`Rate limited. Retry after ${e.retryAfter}s`);

  } else if (e instanceof NotFoundError) {
    console.error(`${e.resourceType} not found: ${e.resourceId}`);

  } else if (e instanceof ICNError && e.isRetryable()) {
    // Server error - SDK will auto-retry based on config
    console.error('Temporary error, retrying...');
  }
}
```

---

## Security Configuration

### Security Headers

The gateway adds security headers to all responses:

```rust
pub struct SecurityHeaders {
    config: SecurityConfig,
}

impl SecurityHeaders {
    fn add_headers(&self, resp: &mut Response) {
        resp.headers_mut().insert(
            "X-Content-Type-Options",
            HeaderValue::from_static("nosniff")
        );
        resp.headers_mut().insert(
            "X-Frame-Options",
            HeaderValue::from_static("DENY")
        );
        resp.headers_mut().insert(
            "Content-Security-Policy",
            HeaderValue::from_static("default-src 'self'")
        );
        // ... more headers
    }
}
```

### CORS Configuration

```rust
pub fn configure_cors(config: &SecurityConfig) -> Cors {
    if config.is_development() {
        // Permissive for development
        Cors::permissive()
    } else {
        // Strict for production
        Cors::default()
            .allowed_origin("https://app.icn.example.com")
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
            .allowed_headers(vec!["Authorization", "Content-Type"])
            .max_age(3600)
    }
}
```

### JWT Secret Requirements

```rust
// JWT secret validation at startup
if self.jwt_secret.is_empty() {
    return Err(GatewayError::InternalError(
        "SECURITY: Gateway cannot start with empty JWT secret".to_string()
    ));
}

if self.jwt_secret.len() < 32 {
    warn!(
        "SECURITY WARNING: JWT secret is only {} bytes. \
         Recommended minimum is 32 bytes for HS256.",
        self.jwt_secret.len()
    );
}
```

---

## API Reference

### Public Endpoints (No Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/health` | Health check |
| POST | `/v1/auth/challenge` | Request auth challenge |
| POST | `/v1/auth/verify` | Verify challenge and get token |
| WS | `/v1/ws/{coop_id}` | WebSocket connection |
| GET | `/v1/coops/{id}/stats` | Public cooperative statistics |
| GET | `/v1/identity/resolve/{did}` | Resolve DID |

### Protected Endpoints

| Method | Path | Scope | Description |
|--------|------|-------|-------------|
| GET | `/v1/ledger/{coop}/balance/{did}` | `ledger:read` | Get balance |
| POST | `/v1/ledger/{coop}/payment` | `ledger:write` | Create payment |
| GET | `/v1/ledger/{coop}/history` | `ledger:read` | Transaction history |
| POST | `/v1/gov/proposals` | `gov:write` | Create proposal |
| POST | `/v1/gov/proposals/{id}/vote` | `gov:write` | Cast vote |
| GET | `/v1/trust/{did}/score` | `trust:read` | Get trust score |
| POST | `/v1/compute/submit` | `compute:submit` | Submit task |

### API Documentation

The gateway provides OpenAPI documentation:

- Swagger UI: `http://localhost:8080/swagger-ui/`
- OpenAPI JSON: `http://localhost:8080/api-docs/openapi.json`

---

## Code Map

| File | Purpose |
|------|---------|
| `icn-gateway/src/server.rs` | Main server setup, middleware chain |
| `icn-gateway/src/api/auth.rs` | Challenge/verify endpoints |
| `icn-gateway/src/api/ledger.rs` | Balance, payment, history endpoints |
| `icn-gateway/src/api/governance.rs` | Proposal, voting endpoints |
| `icn-gateway/src/api/websocket.rs` | WebSocket handler |
| `icn-gateway/src/auth.rs` | AuthManager implementation |
| `icn-gateway/src/rate_limit.rs` | Rate limiter implementations |
| `icn-gateway/src/middleware.rs` | JWT auth middleware |
| `sdk/typescript/src/index.ts` | ICNClient implementation |
| `sdk/typescript/src/types.ts` | TypeScript type definitions |

---

## Exercises

### Exercise 1: Implement Custom SignatureProvider

Create a SignatureProvider that uses the `@noble/ed25519` library:

```typescript
import * as ed25519 from '@noble/ed25519';

// Your implementation here:
async function createSigner(privateKey: Uint8Array): Promise<SignatureProvider> {
  // ...
}
```

### Exercise 2: Build Event Dashboard

Create a React component that displays real-time cooperative events:

```typescript
function EventDashboard({ coopId }: { coopId: string }) {
  // Use client.subscribe() to display live events
  // Show payment amounts, new members, proposal updates
}
```

### Exercise 3: Implement Token Refresh

Modify this code to handle token expiration gracefully:

```typescript
async function fetchBalanceWithRetry(client: ICNClient, coopId: string, did: string) {
  // Handle AuthenticationError by re-authenticating
  // Use the autoRefresh feature or manual refresh
}
```

---

## Checkpoints

Before proceeding to Module 8, verify you can:

- [ ] Explain why the gateway serves as system boundary
- [ ] Describe the challenge-response authentication flow
- [ ] List the available JWT scopes and their permissions
- [ ] Explain the three layers of rate limiting (IP, velocity, authenticated)
- [ ] Use the TypeScript SDK to authenticate and make API calls
- [ ] Subscribe to real-time events via WebSocket
- [ ] Handle SDK errors appropriately (retry, re-auth, validation)
- [ ] Configure CORS and security headers for production

---

## Next Steps

In **Module 8: Web UI**, you'll build user interfaces that consume the gateway API,
implementing the cooperative dashboard, governance interfaces, and real-time updates
using the SDK patterns learned here.
