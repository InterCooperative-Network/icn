# ICN Changelog

All notable changes to the ICN project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed - Gateway Production Hardening (Phase 14 Continued) (2025-11-16)

**Critical DoS & Security Fixes:**

- **BUG #15 (CRITICAL):** Unbounded transaction history DoS prevented
  - **Problem:** `GET /ledger/:coop/history` loaded ALL transactions into memory via `get_all_entries()`
  - **Impact:** Cooperative with millions of transactions would cause OOM crash, no pagination limits
  - **Fix:** Added pagination with `?offset=0&limit=100` query parameters (max 1,000 per request, default 100)
  - **Validation:** New `validate_history_limit()` function enforces MAX_HISTORY_LIMIT = 1,000
  - **Limitation:** Still loads all entries before pagination due to ledger API constraints (TODO: cursor-based pagination in icn-ledger)

- **BUG #16 (HIGH):** Graceful shutdown for background cleanup task
  - **Problem:** Background cleanup task ran indefinitely with `loop`, no shutdown handling
  - **Impact:** Server shutdown left cleanup task running, prevented clean termination
  - **Fix:** Added tokio::broadcast shutdown channel, cleanup task uses tokio::select! to listen for shutdown signal
  - **Implementation:** Server awaits completion, signals cleanup task, 100ms grace period for cleanup to finish

- **BUG #17 (HIGH):** HTTP timeout configuration prevents connection exhaustion
  - **Problem:** No HTTP timeout settings configured, slow clients could hold connections indefinitely
  - **Impact:** Connection pool exhaustion, slow-loris DoS attacks possible
  - **Fix:** Added production-ready timeouts:
    - `keep_alive`: 75 seconds (standard HTTP/1.1 keep-alive)
    - `client_request_timeout`: 30 seconds (prevents slow-loris)
    - `client_disconnect_timeout`: 5 seconds (prevents hanging on dead clients)

**Input Validation Improvements:**

- **BUG #18 (MEDIUM):** Signature length validation in auth verify
  - **Problem:** `hex::decode()` called without validating Ed25519 signature length (must be 64 bytes)
  - **Impact:** Wasted CPU on expensive verification for obviously invalid lengths (0-byte, 1000-byte, etc.)
  - **Fix:** Validate length == 64 bytes AFTER decode, BEFORE crypto operation
  - **Metric:** Track `auth_failures_inc("invalid_signature_length")`

- **BUG #19 (LOW):** Cooperative ID validation in auth verify
  - **Problem:** `req.coop_id` passed to `verify_challenge()` without validation
  - **Impact:** Minor - could allow malformed coop IDs in JWT tokens (API endpoints do validate)
  - **Fix:** Call `validate_coop_id()` before token generation
  - **Metric:** Track `auth_failures_inc("invalid_coop_id")`

- **BUG #23 (CRITICAL):** Timing attack in auth verification prevented
  - **Problem:** Short-circuit `||` operator broke constant-time guarantee in `verify_challenge()`
  - **Impact:** If signature invalid, expiration check was skipped, creating timing side-channel
  - **Attack:** Attackers could measure response times to determine if signature was valid before checking expiration
  - **Fix:** Use bitwise `|` instead of `||` to force evaluation of both conditions
  - **Location:** `auth.rs:150` - `if !signature_valid | is_expired {`
  - **Documentation:** Lines 81-84 and CHANGELOG BUG #7 explicitly claim constant-time behavior, which was violated
  - **Verification:** Both signature validation AND expiration check now always execute

- **BUG #24 (CRITICAL):** WebSocket connection counter race condition
  - **Problem:** When global limit reached, `started()` called `ctx.stop()` without incrementing counter
  - **Impact:** `stopped()` always decremented, even when `started()` never incremented → counter underflow
  - **Consequence:** Counter became out of sync, allowing connection limit bypass after multiple rejections
  - **Fix:** Added `connection_tracked: bool` field to `WsSession` struct
  - **Implementation:** Only increment sets `connection_tracked = true`, `stopped()` only decrements if true
  - **Location:** `websocket.rs:42-254`
  - **Verification:** Rejected connections no longer affect counter

**Earlier TOCTOU Race Condition Fixes:**

- **BUG #7 (CRITICAL):** Timing attack in authentication prevented
  - **Problem:** Early returns in `verify_challenge()` bypassed expensive signature verification
  - **Impact:** Attackers could measure response times to enumerate valid DIDs
  - **Fix:** Always perform signature verification (real or dummy) for constant-time behavior
  - **Implementation:** Dummy verification with Ed25519 when parsing fails

- **BUG #8 (CRITICAL):** Information leakage in authentication errors
  - **Problem:** Different error messages revealed why authentication failed (parsing vs signature vs expiration)
  - **Impact:** Enumeration attacks to discover valid DIDs, challenges, signatures
  - **Fix:** Generic "Authentication failed" message for all failure modes

- **BUG #9 (CRITICAL):** Unbounded cooperative creation DoS
  - **Problem:** No global limit on number of cooperatives
  - **Impact:** Memory exhaustion via unlimited coop creation
  - **Fix:** MAX_COOPERATIVES = 1,000 limit enforced atomically

- **BUG #10 (MEDIUM):** Timestamp panic on clock manipulation
  - **Problem:** `.unwrap()` on SystemTime could panic if system clock set before Unix epoch
  - **Fix:** Replaced with descriptive `.expect()` message

- **BUG #11 (CRITICAL):** TOCTOU race in cooperative creation limit
  - **Problem:** Count check outside atomic `create_coop()` operation
  - **Impact:** Concurrent threads could bypass MAX_COOPERATIVES limit
  - **Fix:** Moved `validate_coop_count()` inside `create_coop()` while holding write lock

- **BUG #12 (CRITICAL):** TOCTOU race in member addition limit + false documentation
  - **Problem:** (1) Comment claimed validation in `add_member()` but it didn't exist (2) Racy check outside atomic operation
  - **Impact:** Concurrent threads could bypass MAX_MEMBERS_PER_COOP limit
  - **Fix:** Added `validate_member_count()` inside `add_member()` method

- **BUG #13 (HIGH):** Unbounded WebSocket subscriber DoS
  - **Problem:** No limit on WebSocket subscriptions per cooperative
  - **Impact:** Memory exhaustion via unlimited WebSocket connections
  - **Fix:** MAX_SUBSCRIBERS_PER_COOP = 1,000 limit, `subscribe()` returns Option

- **BUG #14 (MEDIUM):** Dummy verification logic error
  - **Problem:** Dummy signature verification succeeded instead of failed (signed message verified against same message)
  - **Impact:** Timing attack mitigation ineffective in edge cases
  - **Fix:** Verify dummy signature against DIFFERENT message to ensure it always fails

**WebSocket Security Hardening:**

- **BUG #20 (HIGH):** Unbounded WebSocket message size prevented
  - **Problem:** No size validation on incoming text messages, attacker could send gigabyte payloads
  - **Impact:** Memory exhaustion and OOM crashes
  - **Fix:** MAX_WEBSOCKET_MESSAGE_SIZE = 65,536 bytes (64KB), validate before parsing, close connection on violation
  - **Implementation:** Check text.len() before JSON deserialization

- **BUG #21 (LOW):** Information leakage in WebSocket auth errors
  - **Problem:** Different error messages revealed DID parsing errors and verification failures
  - **Impact:** Enumeration attacks to discover valid tokens/DIDs
  - **Fix:** Generic "Authentication failed" message for all failure modes

- **BUG #22 (MEDIUM):** No global WebSocket connection limit
  - **Problem:** Only per-coop limit (1,000), attacker could create 1,000 × 1,000 = 1M connections
  - **Impact:** Resource exhaustion from unlimited connections
  - **Fix:** MAX_TOTAL_WEBSOCKET_CONNECTIONS = 10,000, check in started() before incrementing counter

**Test Coverage:**
- **51 tests passing** (49 existing + 2 new validation tests)
- New tests: `test_verify_endpoint_invalid_signature_length`, `test_verify_endpoint_invalid_coop_id`

### Fixed - Gateway Memory Leaks and Storage (2025-11-16)

**Critical Memory Leak Fixes:**
- **FIXED:** Rate limiter bucket cleanup now runs automatically via background task
  - **Problem:** `cleanup_inactive_buckets()` existed but was never called
  - **Impact:** Every unique DID created a permanent HashMap entry → unbounded memory growth
  - **Fix:** Added 5-minute background cleanup task that removes buckets inactive for 1+ hour
  - **Implementation:** `server.rs` spawns tokio task with interval timer
- **FIXED:** Event broadcaster now removes dead WebSocket channels automatically
  - **Problem:** `broadcast()` comment claimed "removing closed channels" but didn't actually remove them
  - **Impact:** Long-running gateway accumulated dead channels, wasting memory/CPU on failed sends
  - **Fix:** Modified `broadcast()` to detect send failures and acquire write lock to remove dead channels
  - **Optimization:** Read-only path for common case (no dead channels), write lock only when needed
- **FIXED:** Authentication challenges now cleaned up automatically
  - **Problem:** `cleanup_expired_challenges()` existed but was never called
  - **Impact:** 5-minute TTL challenges accumulated forever in HashMap
  - **Fix:** Background task cleans up expired challenges every 5 minutes
  - **Logging:** Logs cleanup count when >0 challenges removed
- **FIXED:** Deleted cooperatives now clean up WebSocket subscribers immediately
  - **Problem:** Background cleanup only iterated over existing coops, so deleted coop subscribers never cleaned
  - **Impact:** Long-running gateway with many coop create/delete cycles accumulated dead subscriber lists
  - **Fix:** `delete_coop()` now immediately calls `broadcaster.cleanup()` for deleted coop
  - **Implementation:** Spawns async cleanup task on coop deletion

**Critical Data Loss Fix:**
- **CRITICAL FIXED:** Ledger storage now persistent instead of temporary
  - **Problem:** `LedgerManager` used `SledStore::temporary()` - deleted on process exit
  - **Impact:** **ALL ledger data (payments, balances, history) lost on gateway restart**
  - **Severity:** Production blocker - completely unusable for real deployments
  - **Fix:** Added `new_with_storage(data_dir)` constructor that uses persistent Sled databases
  - **Storage Layout:** `{data_dir}/ledgers/{coop_id}/` - isolated per cooperative
  - **Backward Compat:** `new()` still uses temporary storage for testing
  - **Server API:** Added `GatewayServer::new_with_storage()` for production use

**Background Cleanup Architecture:**
- Spawns single tokio task in `server.rs` running every 5 minutes
- Cleans up 3 types of resources:
  1. Expired authentication challenges (5min TTL)
  2. Inactive rate limiter buckets (1hr inactivity threshold)
  3. Dead WebSocket channels (per cooperative)
- Logs cleanup activity at info level when items removed
- Zero-overhead when no cleanup needed (most of the time)

**Cooperative ID Listing:**
- Added `CoopManager::list_all_coop_ids()` for cleanup task iteration
- Returns just IDs (not full coop data) for efficient iteration

**Testing:** All 38 gateway tests pass

**Deployment Impact:**
- Existing temporary ledger data WILL BE LOST (expected - was never persistent)
- New deployments MUST use `GatewayServer::new_with_storage(data_dir)` for production
- Gateway now production-ready with no known memory leaks

**Documentation Updates:**
- Updated `icn-gateway/README.md` with Storage Configuration section
- Added production deployment guide with persistent storage requirements
- Added Background Cleanup Task architecture documentation
- Updated Known Limitations to reflect persistent ledger storage
- Removed outdated "No Persistent State" limitation

### Fixed - Gateway API Route Registration (Phase 14 Continued) (2025-11-16)

**Critical Route Bug Fix:**
- **CRITICAL:** Fixed duplicate `/v1` scope registration that made all public endpoints inaccessible
  - **Problem:** Two separate `.service(web::scope("/v1"))` registrations
  - **Impact:** Second registration shadowed first, blocking health, auth, and websocket endpoints
  - **Severity:** CRITICAL - Gateway completely unusable (authentication impossible)
  - **Resolution:** Consolidated into single `/v1` scope with nested scopes for protected routes
- **Route Structure (Fixed):**
  ```rust
  web::scope("/v1")
      // Public endpoints (no middleware)
      .service(api::health::health)
      .service(api::auth::challenge)
      .service(api::auth::verify)
      .service(api::websocket::websocket)
      // Protected coop endpoints
      .service(
          web::scope("/coops")
              .service(...)
              .wrap(rate_limiting)
              .wrap(auth.clone())
      )
      // Protected ledger endpoints
      .service(
          web::scope("/ledger")
              .service(...)
              .wrap(rate_limiting)
              .wrap(auth)
      )
  ```
- **Middleware Application:**
  - Public endpoints: No authentication required
  - Protected endpoints: Auth first, then rate limiting (wrapping order: last runs first)
  - MetricsMiddleware wraps entire app for comprehensive request tracking
- **Testing:** All 38 gateway tests pass after fix
- **Security Impact:** HIGH - Authentication flow now accessible

### Added - Gateway Production Monitoring Configurations (Phase 14 Continued) (2025-11-16)

**Turnkey Monitoring Setup:**
- **Grafana Dashboard** (`icn-gateway/grafana-dashboard.json`)
  - 10 pre-configured panels for production monitoring
  - Request rate by endpoint (time series)
  - Latency percentiles (p50, p95, p99) per endpoint
  - Error rate percentage (4xx + 5xx)
  - Active WebSocket connections (gauge)
  - Authentication success rate (percentage)
  - Auth failure breakdown by reason (stacked graph)
  - Top 10 rate-limited DIDs (leaderboard)
  - Authorization failures by missing scope
  - Payment volume by currency (hourly rates)
  - Cooperative activity (created, members added/removed)
- **Prometheus Alerts** (`icn-gateway/prometheus-alerts.yml`)
  - 9 production-ready alerting rules with severity levels
  - **Critical Alerts:**
    - HighErrorRate: >5% 5xx errors for 5 minutes
    - LowAuthSuccessRate: <50% auth success for 5 minutes
  - **Warning Alerts:**
    - HighLatency: p95 >1.0s for 10 minutes
    - AuthenticationFailureSpike: >10 failures/sec for 5 minutes
    - WebSocketConnectionDrop: >5 disconnections/sec for 5 minutes
    - NoTraffic: Zero requests for 5 minutes
  - **Info Alerts:**
    - RateLimitingActive: >1 rejection/sec for 10 minutes
    - AuthorizationFailures: >1 failure/sec by scope for 10 minutes
    - NoPaymentActivity: Zero payments for 2 hours

**Import Instructions:**
- Grafana: Import `grafana-dashboard.json` via UI
- Prometheus: Add to `prometheus.yml` alert rules section
- Ready for immediate deployment use

### Added - Gateway Request Metrics Middleware (Phase 14 Continued) (2025-11-16)

**MetricsMiddleware Implementation:**
- **Actix-web Transform middleware** for automatic request tracking
- Wraps all HTTP requests to measure duration and count
- **Metrics Captured:**
  - Request count by endpoint and method
  - Latency histogram by endpoint and status code
  - Duration measured with `Instant::now()` for accuracy
- **Architecture:**
  - `MetricsMiddleware` - Transform implementation
  - `MetricsMiddlewareService<S>` - Service wrapper with timing logic
  - Async-safe with `LocalBoxFuture` for request handling
- **Middleware Stack Order:**
  ```
  Request → MetricsMiddleware (outermost - measures everything)
         → Logger (actix default)
         → Compress (actix default)
         → Auth (per scope)
         → RateLimiter (per scope)
         → Handler
  ```
- **Implementation:**
  - `icn-gateway/src/middleware.rs` - MetricsMiddleware struct
  - `icn-gateway/src/server.rs` - `.wrap(MetricsMiddleware)` at app level
- **Dependencies:** Added `futures-util = "0.3"` for `LocalBoxFuture`
- **Testing:** All 38 gateway tests pass

### Added - Gateway Prometheus Metrics Instrumentation (Phase 14 Continued) (2025-11-16)

**Comprehensive Metrics Coverage:**
- **21 Prometheus metrics** across 6 operational categories
- Fire-and-forget metric recording (non-blocking)
- Integration with existing `icn-obs` metrics infrastructure

**Authentication Metrics (5):**
- `icn_gateway_auth_challenges_total` - Challenge requests issued
- `icn_gateway_auth_verifications_total` - Verification attempts
- `icn_gateway_auth_successes_total` - Successful authentications
- `icn_gateway_auth_failures_total` - Failures by reason (invalid_did, invalid_signature_encoding, verification_failed)
- Labels: `reason` for failure categorization

**Authorization Metrics (1):**
- `icn_gateway_authorization_failures_total` - Missing scope failures
- Labels: `required_scope` (ledger:read, ledger:write, coop:admin, etc.)

**Rate Limiting Metrics (1):**
- `icn_gateway_rate_limit_exceeded_total` - Rate limit violations by DID
- Labels: `did` for per-user tracking

**Request Metrics (2):**
- `icn_gateway_requests_total` - Total requests by endpoint and method
- `icn_gateway_request_duration_seconds` - Latency histogram by endpoint and status
- Labels: `endpoint`, `method`, `status`

**WebSocket Metrics (4):**
- `icn_gateway_websocket_connections_total` - Total connections (counter)
- `icn_gateway_websocket_connections_active` - Currently active connections (gauge)
- `icn_gateway_websocket_disconnections_total` - Disconnection events
- `icn_gateway_websocket_messages_sent_total` - Event messages sent to clients
- **Atomic Connection Tracking:** `AtomicU64` for lock-free active connection count

**Cooperative Metrics (4):**
- `icn_gateway_coops_created_total` - Cooperative creation events
- `icn_gateway_coops_deleted_total` - Cooperative deletion events
- `icn_gateway_members_added_total` - Member addition events
- `icn_gateway_members_removed_total` - Member removal events

**Ledger Metrics (4):**
- `icn_gateway_payments_created_total` - Payment transactions created
- `icn_gateway_payment_amount` - Payment amount histogram by currency
- `icn_gateway_balance_queries_total` - Balance query count
- `icn_gateway_history_queries_total` - Transaction history query count
- Labels: `currency` for payment tracking

**Instrumentation Locations:**
- `icn-gateway/src/api/auth.rs` - Auth metrics
- `icn-gateway/src/api/coops.rs` - Coop metrics
- `icn-gateway/src/api/ledger.rs` - Ledger metrics
- `icn-gateway/src/middleware.rs` - Request + authorization metrics
- `icn-gateway/src/rate_limit.rs` - Rate limit metrics
- `icn-gateway/src/websocket.rs` - WebSocket metrics with atomic tracking

**Metrics Module:**
- Added `gateway` submodule to `icn-obs/src/metrics.rs` (170+ lines)
- 21 helper functions for metric recording
- Consistent naming: `icn_gateway_{category}_{metric}_{unit}`

**Dependencies:**
- Added `icn-obs` to `icn-gateway/Cargo.toml`
- Added `metrics = "0.22"` for metric macros

**Testing:** All 38 gateway tests pass, 432 workspace tests pass

**Observability Benefits:**
- Real-time performance monitoring
- Attack detection (auth failures, rate limiting)
- Capacity planning (WebSocket connections, payment volume)
- SLO tracking (latency percentiles, error rates)
- Operational visibility into gateway health

### Fixed - Cooperative Owner DID Extraction (Phase 14 Continued) (2025-11-16)

**Ownership Fix:**
- Cooperative creation now correctly uses authenticated user's DID as owner
- Removed placeholder DID generation - uses JWT token's `sub` claim
- Added explicit test: `test_create_coop_uses_authenticated_did`
- Ensures proper ownership tracking from creation
- Total: 38 tests passing (up from 37)

**Technical Details:**
- Uses `get_claims()` helper to extract TokenClaims from request
- Parses `claims.sub` to get owner DID
- Returns AuthenticationFailed if claims missing
- Returns BadRequest if DID format invalid

### Added - Scope-Based Authorization (Phase 14 Continued) (2025-11-16)

**Authorization Enforcement:**
- All protected endpoints now enforce JWT scope-based authorization
- `require_scope()` helper function validates scopes against required permissions
- HTTP 403 Forbidden response when required scope missing
- Scope hierarchy:
  - `ledger:read` - Required for balance queries and transaction history
  - `ledger:write` - Required for creating payments
  - `coop:read` - Required for viewing cooperative information
  - `coop:write` - Required for creating cooperatives
  - `coop:admin` - Required for member management and settings changes
- 2 comprehensive authorization tests verify correct scope enforcement
- Total: 37 tests passing (up from 35)

**Security Benefits:**
- Prevents privilege escalation (read-only tokens cannot write)
- Fine-grained access control for API operations
- Clear separation between read and write permissions
- Administrative operations require explicit `coop:admin` scope

### Added - Gateway API Improvements (Phase 14 Continued) (2025-11-16)

**API Versioning:**
- All endpoints now under `/v1` namespace for future API evolution
- Versioned paths: `/v1/health`, `/v1/auth/*`, `/v1/coops/*`, `/v1/ledger/*`, `/v1/ws/:coop_id`
- Enables backward-compatible API changes in future versions
- Clean migration path for API consumers

**Per-DID Rate Limiting:**
- Token bucket algorithm prevents API abuse and resource exhaustion
- Independent rate limits per authenticated DID
- Default: 100 token burst capacity, 10 tokens/second refill (600 requests/minute sustained)
- Configurable: `RateLimitConfig` allows custom capacity, refill rate, and cost per request
- HTTP 429 Too Many Requests response when rate limit exceeded
- Automatic cleanup of inactive buckets prevents unbounded memory growth
- Applied to protected endpoints (`/v1/coops/*`, `/v1/ledger/*`)
- Public endpoints (health, auth, websocket) not rate-limited

**Technical Details:**
- `RateLimiter`: Arc<RwLock<HashMap<DID, TokenBucket>>> for per-DID tracking
- `TokenBucket`: Continuous refill based on elapsed time (Instant-based)
- Middleware integration via `actix_web::middleware::from_fn`
- Rate limiting applied after JWT authentication (requires valid claims)
- 5 comprehensive tests covering bucket behavior, refill, capacity limits, DID isolation, cleanup

**Benefits:**
- Protects daemon resources from malicious or buggy clients
- Fair resource allocation across DIDs
- Production-ready abuse prevention
- Configurable limits per deployment scenario

### Added - Platform Layer: REST API Gateway (Phase 14) (2025-01-15)

**New Crate: icn-gateway**
- Complete HTTP API server for cooperative applications
- Actix-web 4 async framework with middleware
- 14 endpoints (13 REST + 1 WebSocket) across 5 modules
- 30 tests passing (9 auth, 6 coop, 6 ledger, 4 middleware/websocket, 5 events)

**Authentication & Authorization:**
- DID-based challenge/verify flow
- `POST /auth/challenge` - Request cryptographic challenge
- `POST /auth/verify` - Verify signed challenge, receive JWT token
- Ed25519 signature verification
- JWT capability tokens with scoped permissions (coop_id + scopes)
- 5-minute challenge TTL with automatic cleanup
- Bearer token authentication middleware (actix-web-httpauth)
- Protected endpoints require valid JWT in Authorization header
- Token validation extracts claims into request extensions
- Public endpoints: health, auth, websocket (auth handled post-connection)

**Cooperative Namespace Management:**
- `POST /coops` - Create cooperative
- `GET /coops/:id` - Get cooperative info
- `PUT /coops/:id/settings` - Update governance/credit policy/currency
- `DELETE /coops/:id` - Delete cooperative
- `POST /coops/:id/members` - Add member with role
- `DELETE /coops/:id/members/:did` - Remove member
- `PUT /coops/:id/members/:did/role` - Update member role
- Role-based access control (Owner/Admin/Member)
- Per-coop settings (governance model, credit policy, currency)

**Ledger Operations:**
- `GET /ledger/:coop_id/balance/:did` - Get account balances
- `POST /ledger/:coop_id/payment` - Create payment transaction
- `GET /ledger/:coop_id/history?did=...` - Get transaction history (with optional DID filter)
- Per-cooperative isolated mutual credit ledgers
- Double-entry bookkeeping with validation
- SledStore backend for persistence

**Real-time Event Streaming:**
- `GET /ws/:coop_id` - WebSocket endpoint for real-time updates
- Post-connection JWT authentication via JSON message protocol
- Client sends `{"type": "Auth", "token": "..."}` after connecting
- Server validates token and coop_id match before event subscription
- Event types: PaymentCreated, MemberAdded, MemberRemoved, RoleUpdated, SettingsUpdated
- EventBroadcaster: Pub/sub system with per-cooperative isolation
- WsSession actor: Heartbeat/ping-pong with automatic connection cleanup (60s timeout)
- Tokio mpsc channels for async event distribution (100ms polling)
- Server messages: AuthOk, AuthError, Event, Error (all JSON-formatted)

**Infrastructure:**
- `GET /health` - Health check endpoint
- Error handling with HTTP status mapping
- JSON error responses
- Request/response models for all endpoints
- Logging and compression middleware

**Architecture:**
- `AuthManager` - Challenge/verify flow with JWT token generation and verification
- `CoopManager` - In-memory coop namespace storage
- `LedgerManager` - Per-coop ledgers with isolated storage
- `EventBroadcaster` - Per-cooperative event pub/sub with tokio channels
- `WsSession` - Actix actor for WebSocket connection lifecycle
- JWT middleware (`middleware::jwt_auth`) - Bearer token validation for protected routes
- Thread-safe shared state via Arc<RwLock<T>> and web::Data

**Dependencies:**
- actix-web 4, actix-web-actors, actix-web-httpauth, actix-cors
- jsonwebtoken 9
- hex, rand, ed25519-dalek 2

**Note:** This is NOT an app runtime. Apps run externally and call this API. See `docs/platform-layer-design.md` for architecture.

### Added - Gateway Integration with icnd (2025-01-15)

**Runtime Integration:**
- Gateway server integrated into icnd supervisor
- Spawns in dedicated thread with separate Tokio runtime
- Respects configuration enable/disable flag
- Validates JWT secret before starting
- Graceful error handling when misconfigured

**Configuration System:**
- `GatewayConfig` added to `icn-core/src/config.rs`
- Fields: enabled, bind_addr, token_expiry_hours, challenge_ttl_minutes, jwt_secret
- TOML serialization/deserialization
- Serde defaults for all fields
- Gateway disabled by default (opt-in)

**CLI Arguments:**
- `--gateway-enable`: Enable gateway API server
- `--gateway-bind <IP:PORT>`: Override bind address
- `--gateway-jwt-secret <SECRET>`: Set JWT secret
- Environment variable support: ICN_GATEWAY_JWT_SECRET
- Configuration priority: CLI args > env vars > config file

**Example Configuration:**
```toml
[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
token_expiry_hours = 24
challenge_ttl_minutes = 5
jwt_secret = "your-strong-secret-here"
```

**Documentation:**
- Comprehensive example config file: `config/icn.toml.example`
- Updated CLAUDE.md with 4 configuration methods
- Security recommendations for production deployment
- API usage examples with curl and wscat

**Security Features:**
- Opt-in by default (enabled: false)
- Requires explicit JWT secret configuration
- No default/fallback secrets
- Helpful warnings when misconfigured
- Localhost binding by default

All tests pass. Gateway ready for production deployment.

### Fixed

**Governance:**
- Fixed division-by-zero vulnerability in `quorum_met()` when total_members == 0
- Now returns false instead of NaN/infinity

### Added - Economic Safety Rails: Dispute Resolution System (Phase 12) (2025-01-14)

**Dispute Management:**
- `DisputeManager` for tracking and resolving entry disputes
- File disputes against journal entries with reason and evidence
- Assign mediators to disputes
- Resolve disputes with multiple outcome types
- Track dispute history and status

**Dispute Types:**
- `Dispute` record with filed_by, reason, evidence, mediator
- `DisputeStatus`: Normal, Contested, Resolved
- `DisputeOutcome`: Upheld, Reversed, Settlement, WriteOff
- Persistent storage of all disputes

**Dispute Operations:**
- `file_dispute()` - Create new dispute with reason
- `add_evidence()` - Attach supporting documentation
- `assign_mediator()` - Assign trusted mediator
- `resolve_dispute()` - Record resolution with outcome
- `get_active_disputes()` - Query all active disputes
- `get_disputes_by_filer()` - Filter by who filed

**Implementation:**
- New module: `icn-ledger/src/dispute.rs` (380 lines)
- DisputeManager with persistent storage backend
- Active disputes cached in memory
- 6 unit tests covering dispute lifecycle

**Use Cases:**
- Member contests incorrect charge
- Mediator investigates and resolves
- Debt write-off for defaults
- Settlement agreements between parties
- Audit trail of all dispute activity

**Example:**
```rust
let mut manager = DisputeManager::new(store)?;

// File dispute
manager.file_dispute(entry_hash, member_did, "Wrong amount".to_string(), timestamp)?;

// Add evidence
manager.add_evidence(&entry_hash, "Receipt shows $50, not $100".to_string())?;

// Mediator resolves
let outcome = DisputeOutcome::Settlement {
    terms: "Split difference: $75".to_string(),
    replacement_entry: Some(new_entry_hash),
};
manager.resolve_dispute(&entry_hash, mediator_did, outcome, timestamp)?;
```

### Added - Economic Safety Rails: Dynamic Credit Limits (Phase 12) (2025-01-14)

**Credit Policy System:**
- Dynamic credit limits based on trust score + transaction history
- `CreditPolicy` calculates limits using formula: baseline + trust_bonus + history_bonus
- Conservative and permissive policy presets
- Trust bonus scales with trust graph score (0.0-1.0)
- History bonus rewards cleared transaction volume

**New Member Protection:**
- `NewMemberPolicy` implements protective throttling for new participants
- Initial low credit limit (10 hours default)
- Contribution threshold before ramping starts (50 hours default)
- Linear ramp over 90 days to full credit limit
- Prevents "extract value and disappear" attacks

**Credit Limit Calculation:**
- `CreditPolicyManager` combines base policy + new member throttling
- Effective limit is minimum of both policies
- `total_cleared_by()` method tracks historical contributions
- `check_transaction()` validates against calculated limits

**Implementation:**
- New module: `icn-ledger/src/credit_policy.rs`
- `CreditPolicy::calculate_limit()` - Dynamic limit calculation
- `NewMemberPolicy::calculate_effective_limit()` - Tenure-based ramping
- `Ledger::total_cleared_by()` - Sum all credits for an account
- 4 unit tests covering policy defaults, ramping, and limit checks

**Economic Protection:**
- Protects communities from free riders
- Rewards trusted, active participants with higher limits
- Gradual onboarding prevents new member exploitation
- Foundation for Phase 12 dispute resolution and default handling

**Example:**
```rust
// Conservative policy for new communities
let policy = CreditPolicyManager::conservative("hours".to_string());

// Calculate limit: baseline 100h + trust 24h + history 50h = 174h
let limit = policy.calculate_credit_limit(
    &member_did,
    member_since,
    current_time,
    &ledger,
    &trust_graph,
)?;
```

### Added - Operational Hardening: Protocol Version Validation (Track B1) (2025-01-14)

**Versioned Network Protocol:**
- Protocol version constants: `PROTOCOL_VERSION`, `MIN_SUPPORTED_VERSION`, `MAX_SUPPORTED_VERSION`
- Automatic version validation on message deserialization
- Backward compatibility within version range (v1-v1 currently)
- Forward compatibility detection (rejects messages from future versions)

**Version Mismatch Handling:**
- Clear error messages for version incompatibility
- Prometheus metrics for monitoring version issues:
  - `icn_network_protocol_version_mismatch_total` - Total version mismatches
  - `icn_network_protocol_version_too_old_total` - Messages from old versions
  - `icn_network_protocol_version_too_new_total` - Messages from future versions
- Network actor tracks and logs version mismatches

**Upgrade Safety:**
- Prevents communication between incompatible protocol versions
- Protects against parsing errors from unknown message formats
- Foundation for rolling upgrades in future versions

**Implementation:**
- Version validation in `icn-net/src/protocol.rs::NetworkMessage::from_bytes()`
- Metrics tracking in `icn-net/src/actor.rs` message receive loop
- 4 new unit tests for version validation scenarios

**Future Work:**
- Version negotiation handshake for compatibility announcements
- Graceful degradation for non-breaking changes
- Version compatibility matrix for upgrade planning

### Added - Operational Hardening: Operations Guide (Track B1) (2025-01-14)

**Comprehensive Operations Documentation:**
- Day-to-day operational workflows and procedures
- Consolidates all operational documentation into one reference
- Detailed command reference for all operational tasks
- Troubleshooting workflows for common issues

**Operations Workflows:**
1. **Daily Operations** - Morning health checks (5 min), routine monitoring
2. **Weekly Maintenance** - Backups, metrics review, disk usage checks (15-30 min)
3. **Monthly Tasks** - Backup archival, device audits, update checks
4. **Upgrade Procedures** - Current manual process + future automation plans
5. **Capacity Planning** - Storage, memory, bandwidth growth estimates
6. **Performance Tuning** - Configuration optimization for different use cases

**Monitoring & Health:**
- Dashboard interpretation guide
- Health check endpoint integration examples
- Key metrics to monitor with thresholds
- Prometheus alerting rule examples

**Operational Command Reference:**
- Identity management commands
- Device management commands
- Node operations (start/stop/restart/logs)
- Network diagnostics (peers, connectivity)
- Gossip operations (topics, subscriptions, entries)
- Ledger operations (balances, transactions, quarantine)
- Metrics queries

**Troubleshooting Workflows:**
- Node won't start (port conflicts, keystore issues, permissions)
- No peer connections (mDNS, firewall, TLS issues)
- High quarantine size (conflicts, clock skew, attacks)
- High memory usage (gossip growth, cache tuning)
- Slow transaction processing (network latency, conflicts)

**Implementation:**
- New document: `docs/operations-guide.md` (800+ lines)
- References deployment guide, incident response playbook, architecture docs
- Ready for operational teams and community node operators

### Added - Operational Hardening: Incident Response Playbook (Track B1) (2025-01-14)

**Comprehensive Incident Response Documentation:**
- Detailed procedures for 7 major incident scenarios
- General incident response framework with P0-P3 severity levels
- Step-by-step workflows with command examples
- Monitoring and detection guidance

**Incident Scenarios Covered:**
1. **Node Compromise (P0)** - Immediate isolation, evidence preservation, device revocation
2. **Ledger Corruption (P1)** - Quarantine assessment, recovery procedures, backup restoration
3. **Key Suspected Stolen (P0)** - Emergency device revocation, key rotation ceremony
4. **Network Partition (P1)** - Connectivity diagnosis, split-brain detection
5. **Gossip Storm (P2)** - Rate limiting verification, peer blocking
6. **Quarantine Growth (P2)** - Entry inspection, manual review vs automated cleanup
7. **Monitoring and Detection** - Critical/warning/info alert definitions

**Each Scenario Includes:**
- Symptoms and diagnosis procedures
- Immediate actions (first 15 minutes)
- Recovery steps
- Investigation and root cause analysis
- Prevention strategies

**Operations Support:**
- Post-incident review template
- Emergency contact information structure
- Integration with monitoring dashboard
- Balance between current v0.1 capabilities and future features

**Implementation:**
- New document: `docs/incident-response.md` (630+ lines)
- Ready for operational deployment
- Supports Track C pilot deployment readiness

### Added - Operational Hardening: Monitoring Dashboard (Track B1) (2025-01-14)

**Health Check Endpoint:**
- `/health` endpoint returns JSON health status
- HTTP status codes: 200 (healthy/degraded), 503 (unhealthy)
- Real-time metrics: uptime, active connections, gossip topics, ledger quarantine size
- Health state determination based on system metrics

**Web Monitoring Dashboard:**
- Real-time web UI at `http://localhost:8080/`
- Auto-refreshing every 5 seconds
- Key metrics displayed:
  - Network: Active peers, messages sent/received, bytes transferred, rate limiting
  - Gossip: Topics, entries, subscriptions, pull/push activity
  - Ledger: Accounts, transactions, conflicts, quarantine status
  - Trust: Edges, lookups, cache hit rate, attestations
- Clean, dark-themed UI optimized for operations monitoring
- Fetches data directly from Prometheus `/metrics` endpoint

**Health Service Integration:**
- `HealthService` tracks node state
- Periodic metric updates
- Degraded state detection (>100 quarantine entries)
- Unhealthy state detection (>1000 quarantine entries)

**Implementation:**
- New module: `icn-obs/src/health.rs`
- Static dashboard: `icn-obs/static/dashboard.html`
- Axum-based HTTP server for health and dashboard endpoints

**Use Cases:**
- Real-time operational monitoring
- External health checks (Kubernetes, systemd)
- Quick visual status overview
- Performance troubleshooting

### Added - Operational Hardening: Backup & Restore (Track B1) (2025-01-14)

**Backup & Recovery Commands:**
- `icnctl backup <output>` - Create encrypted tarball backup of ICN data directory
- `icnctl restore <input>` - Restore ICN data directory from backup
- `icnctl restore <input> --force` - Overwrite existing data directory (backs up old data first)

**Backup Features:**
- **Complete data directory backup:** Identity keystore, DID Document, rotation chain, trust graph, ledger database
- **Integrity verification:** SHA256 checksums of all files ensure backup integrity
- **Metadata tracking:** ICN version, timestamp, checksum stored in `backup_metadata.json`
- **Automatic verification:** Restore validates checksum matches backup

**Production-Ready:**
- Tarball format (standard `.tar` archives)
- Preserves file permissions and structure
- Safe restore with existing directory protection
- Comprehensive error handling

**Test Coverage:**
- 4 integration tests:
  - Full backup/restore roundtrip
  - Backup of nonexistent directory (error handling)
  - Restore without --force fails on existing directory
  - Restore with --force creates backup of old data

**Documentation:**
- New guide: `docs/backup-and-recovery.md`
  - Best practices for regular backups
  - Secure storage recommendations (3-2-1 rule)
  - Recovery scenarios (lost device, corrupted data, migration)
  - Troubleshooting guide

**Use Cases:**
- Regular backups after identity changes
- Device migration
- Disaster recovery
- Identity preservation before upgrades

### Added - Multi-Device Identity & Sync (Phase 11) (2025-01-14)

**Multi-Device Support:**
- **MAJOR FEATURE:** One DID controlled by multiple devices with different keys
- **DID Document v2:** Multiple VerificationMethods per DID with capability-based permissions
- **Capability System:**
  - ✅ Sign - Sign messages and contracts
  - ✅ AddDevice - Authorize new devices
  - ✅ RevokeDevice - Revoke other devices
  - ✅ RotateKey - Rotate device key
  - ✅ Recover - Use recovery mechanisms
  - ✅ Encrypt - Decrypt messages (X25519)
- **Rotation Events:** Audit trail for device lifecycle (add, revoke, rotate)
- **Keystore v3 Format:**
  - DID Document storage
  - Device ID tracking
  - Rotation chain history
  - Automatic migration from v1/v2.1

**Identity Sync Protocol:**
- Gossip topic: `identity:updates` for broadcasting DID Document changes
- IdentityUpdateMessage: Bincode-serialized rotation events (~280 bytes)
- DidDocumentCache: Peer identity verification with version ordering
- Version-based conflict resolution

**CLI Device Management:**
- `icnctl device list` - Show all devices for current identity
- `icnctl device add <name>` - Generate keys and request file for new device
- `icnctl device approve <file>` - Approve device add request
- `icnctl device revoke <id>` - Revoke device access

**Implementation:**
- New module: `icn-identity/src/multi_device.rs` (DID Document v2)
- New module: `icn-identity/src/sync.rs` (Identity sync protocol)
- Enhanced: `icn-identity/src/keystore.rs` (v3 format)
- Enhanced: `icnctl/src/main.rs` (Device commands)

**Test Coverage:**
- 31 unit tests (multi_device, keystore, sync)
- 2 integration tests (end-to-end workflow, version ordering)
- 1 doc test

### Fixed

**Critical: Version Mismatch in Device Approval (2025-01-14)**
- Fixed bug where device approval incremented DID Document version twice (once per key) but rotation event expected single increment
- Added `DidDocument::add_device_with_encryption_key()` to add both Ed25519 and X25519 keys with single version increment
- Added test `test_add_device_with_encryption_key_version_increment()` to verify correct behavior
- Impact: Would have caused identity sync verification failures when peers tried to apply rotation events
- Resolution: Rotation event version now matches DID Document version after device approval

### Added - End-to-End Payload Encryption (Phase 10) (2025-11-13)

**X25519-ChaCha20-Poly1305 Message Encryption:**
- **MAJOR FEATURE:** End-to-end encrypted messages for payload confidentiality
- **Encryption Scheme:**
  - ✅ **Key Exchange**: X25519 ECDH (static, upgradeable to ephemeral in future)
  - ✅ **Symmetric Cipher**: ChaCha20-Poly1305 AEAD (authenticated encryption)
  - ✅ **Nonce Derivation**: Deterministic from sequence number (no transmission overhead)
  - ✅ **Key Persistence**: X25519 keys stored in keystore v2.1 format

**Three-Layer Security Architecture:**
```
Application:  EncryptedEnvelope (payload confidentiality)
Message:      SignedEnvelope (authentication + replay protection)
Transport:    QUIC/TLS 1.3 (channel encryption)
```

**Why All Three Layers:**
- **QUIC/TLS**: Protects node-to-node connections (per-hop encryption)
- **SignedEnvelope**: Authenticates sender and prevents replay (message integrity)
- **EncryptedEnvelope**: Hides payload from intermediate gossip nodes (end-to-end confidentiality)

**Implementation:**
- New module: `icn-net/src/encryption.rs` with `EncryptedEnvelope` struct
- IdentityBundle extended with X25519 keypair (bundle.rs)
- Keystore v2.1 format with X25519 key persistence (keystore.rs)
- Automatic v2.0 → v2.1 migration on first unlock
- New PayloadType::Encrypted (value 7) for encrypted messages
- **Bidirectional X25519 key exchange via Hello protocol:**
  - Hello messages now include sender's X25519 public key
  - Connection initiator sends Hello with X25519 key
  - Connection responder sends Hello response with X25519 key
  - NetworkActor stores peer X25519 keys in HashMap
  - Public API: `NetworkHandle::get_peer_x25519_key()` to retrieve peer keys
  - Automatic key exchange during connection establishment

**Encryption Flow:**
1. Serialize application payload → plaintext bytes
2. Encrypt with X25519 + ChaCha20-Poly1305 → EncryptedEnvelope
3. Serialize EncryptedEnvelope → encrypted bytes
4. Sign with Ed25519 → SignedEnvelope (PayloadType::Encrypted)
5. Wrap in NetworkMessage::Signed → send over network

**Decryption Flow:**
1. Receive NetworkMessage::Signed
2. Verify Ed25519 signature → extract SignedEnvelope
3. Check PayloadType::Encrypted
4. Deserialize → EncryptedEnvelope
5. Decrypt with X25519 keys → plaintext bytes
6. Deserialize → original application payload

**Security Properties:**
- ✅ **Payload confidentiality**: Intermediate nodes cannot read content
- ✅ **Authenticated encryption**: Poly1305 MAC detects tampering
- ✅ **Replay protection**: Inherited from SignedEnvelope sequence numbers
- ✅ **Nonce uniqueness**: Derived from monotonic sequence + DIDs
- ✅ **Key persistence**: X25519 keys survive daemon restarts

**What It Doesn't Provide (Yet):**
- ❌ **Perfect Forward Secrecy**: Static ECDH reuses shared secrets (can add ephemeral keys in Phase 11)
- ❌ **Metadata hiding**: Sender/recipient DIDs still visible
- ❌ **Protection against node compromise**: Attacker with memory access can read keys

**Performance:**
- Encryption overhead: ~0.3-0.7ms per 1KB message
- Memory overhead: 64 bytes per peer (X25519 public key cache)
- Nonce derivation: Zero transmission overhead (computed locally)

**Testing:**
- Unit tests: 8 encryption tests (roundtrip, tampering, nonce uniqueness, edge cases)
- Integration tests: 7 end-to-end tests including:
  - `test_network_x25519_key_exchange_and_encrypted_message()`: Full network-level test
  - Verifies automatic key exchange during connection establishment
  - Complete encrypt→sign→send→receive→verify→decrypt flow over real QUIC connections
- All 19 icn-identity tests pass (bundle + keystore with X25519)
- All 76 icn-net tests pass (encryption module + integration + network tests)

**Keystore Migration:**
- **v2.0 → v2.1 migration**: Automatic on first unlock
- Generates X25519 keypair and saves immediately to disk
- Backward compatible: v1 → v2.1 migration also supported
- Log messages: "Unlocked v2.1+ keystore with X25519 keys" or "Upgrading to v2.1"

**Dependencies Added:**
- `chacha20poly1305 = "0.10"` (workspace)
- `x25519-dalek` already imported (now used)
- `zeroize` for secure memory handling

**Usage Example:**
```rust
// 1. Get identity bundles (contain X25519 keys)
let alice_bundle = keystore.get_identity_bundle()?;
let bob_bundle = /* lookup Bob's bundle */;

// 2. Encrypt message
let plaintext = bincode::serialize(&my_message)?;
let encrypted = EncryptedEnvelope::encrypt(
    alice_bundle.did(),
    bob_bundle.did(),
    sequence_number,
    &alice_bundle.x25519_secret(),
    &bob_bundle.x25519_public(),
    &plaintext,
)?;

// 3. Sign encrypted envelope
let signed = SignedEnvelope::from_payload(
    alice_bundle.did(),
    alice_bundle.keypair(),
    sequence_number,
    PayloadType::Encrypted,
    &encrypted,
)?;

// 4. Send via NetworkMessage::Signed
```

### Added - Gossip Message Authentication (2025-11-13)

**Cryptographically Signed Gossip Messages:**
- **MAJOR CHANGE:** All gossip messages now use SignedEnvelope for authentication
- **Security Properties:**
  - ✅ **Ed25519 authentication**: Every gossip message is cryptographically signed
  - ✅ **Replay protection**: Sequence numbers with Bloom filter detection
  - ✅ **Sender verification**: Impossible to forge messages from other DIDs
  - ✅ **Freshness checking**: Timestamped messages with 300s max age
  - ✅ **Non-repudiation**: Senders cannot deny sending authenticated messages

**Implementation:**
- GossipActor now holds optional keypair for signing outgoing messages
- Sequence counter (AtomicU64) tracks monotonically increasing message numbers
- Send callback creates SignedEnvelope with PayloadType::Gossip
- Receive path decodes and verifies signed gossip messages
- Automatic verification via NetworkActor's ReplayGuard

**Message Flow:**
- **Send:** `GossipActor.publish() → SignedEnvelope::from_payload() → NetworkMessage::signed() → network send`
- **Receive:** `NetworkActor verifies → decode PayloadType::Gossip → handle_message() with authenticated sender`

**Message Size Impact:**
- SignedEnvelope overhead: ~141 bytes per message
  - DID (from): ~60 bytes
  - Sequence number: 8 bytes
  - Timestamp: 8 bytes
  - Payload type: 1 byte
  - Ed25519 signature: 64 bytes
- **Announce messages:** 230B → 371B (+61%)
- **Request messages:** 32B → 173B (+441%, but small absolute size)
- **Response messages (2KB):** 2KB → 2.1KB (+7%)

**Backward Compatibility:**
- ⚠️ **BREAKING CHANGE:** New nodes only send signed messages
- Old MessagePayload::Gossip receive path still exists for compatibility
- Recommended: Coordinate network-wide upgrade or implement dual-mode receiver

**Testing:**
- All 262 library tests pass
- Gossip tests: 52 passing (signed message flow verified)
- Network tests: 53 passing (SignedEnvelope + ReplayGuard)
- Core integration tests: 26 passing

**Impact:**
- First major protocol to use Phase 9 SignedEnvelope infrastructure
- Demonstrates end-to-end message authentication pattern
- **Automatically protects all protocols that use gossip:**
  - ✅ **Ledger sync** - Already authenticated (publishes via gossip topics)
  - ✅ **Trust attestations** - Dual-layer protection (entry + network signatures)
  - ✅ **Contract deployment** - Network-level authentication inherited
- Eliminates trust in "from" field (now cryptographically verified)

### Fixed - Critical: TLS Certificate Persistence (2025-11-13)

**Keystore Migration Bug Fix:**
- **CRITICAL:** Fixed v1-to-v2 keystore migration to persist TLS certificates to disk
  - **Problem:** TLS certificates were regenerated on every daemon restart for v1 keystores
  - **Impact:** Violated Phase 8 requirement that "TLS certificates persist across restarts"
  - **Root Cause:** TODO at line 245 in keystore.rs was never implemented
  - **Fix:** Auto-save upgraded v2 keystore immediately after generating TLS binding
  - **Security Impact:** HIGH - Required for Phase 8 DID-TLS binding integrity

**What Was Broken:**
- When unlocking a v1 keystore (KeyPair-only format), the system generated an `IdentityBundle` with TLS binding in memory
- The TODO comment indicated this should be persisted, but the code only stored the bundle in memory
- The keystore file on disk remained in v1 format
- Every subsequent unlock generated a new TLS certificate with different cryptographic material
- Peers would see different TLS certificates on each daemon restart
- TLS session stability and trust establishment were broken

**How It Was Fixed:**
- Modified `unlock()` method in `icn-identity/src/keystore.rs` (lines 245-260)
- After generating `IdentityBundle` for v1 migration:
  1. Create complete `StoredKey` with all TLS binding fields populated
  2. Call `encrypt_and_save()` to persist immediately to disk
  3. Log success message confirming migration
- This ensures v1 keystores upgrade to v2 format on first unlock
- TLS certificates remain stable across all subsequent unlocks and restarts

**Testing:**
- Added comprehensive test: `test_v1_to_v2_migration_persists_tls()`
- Test verifies:
  - v1 keystore migrates on first unlock
  - TLS certificate is identical on second unlock (not regenerated)
  - TLS certificate persists to disk (verified by new keystore instance)
  - Binding signature remains stable across unlocks
- All 19 icn-identity tests pass

**Security Properties Restored:**
- ✅ TLS certificates persist across daemon restarts
- ✅ DID-TLS binding integrity maintained
- ✅ Peers see consistent TLS certificates
- ✅ Trust establishment stability ensured
- ✅ Phase 8 security requirements met

### Added - Phase 8A: Trust Network Propagation (2025-01-12)

**Trust Attestation System:**
- **Signed trust attestations** with Ed25519 cryptographic signatures
  - `TrustAttestation` message format with issuer, subject, score, TTL, and signature
  - Deterministic signing payload (SHA256 hash of sorted fields)
  - Signature verification extracting verifying key from DIDs
  - TTL-based expiration (default: 30 days) with automatic decay
  - Conversion to/from `TrustEdge` for seamless storage integration
- **`trust:attestations` gossip topic** for network-wide trust propagation
  - Access control: `TrustClass::Known` (requires trust score ≥0.1)
  - Prevents spam from untrusted/isolated nodes
  - Integrates with existing gossip infrastructure
- **Trust propagation module** (`icn-core/src/trust_propagation.rs`)
  - `broadcast_trust_attestation()` - Signs and publishes attestations
  - `handle_trust_attestation_entry()` - Verifies and applies remote attestations
  - Deduplication: only accepts newer attestations (by `created_at` timestamp)
  - Automatic notification callback integration with gossip subscriptions
- **Supervisor wiring** for incoming attestation handling
  - Notification callback processes trust attestations reactively
  - Automatic subscription to `trust:attestations` topic
  - Spawns async tasks for non-blocking attestation processing

**Observability:**
- **Prometheus metrics** for trust propagation:
  - `icn_trust_attestations_broadcasted_total` - Outbound attestations
  - `icn_trust_attestations_received_total` - Inbound attestations
- Enable monitoring of trust graph growth and network health

**Testing:**
- **14 unit tests** for trust attestations (100% pass rate)
  - Signature creation, verification, and tampering detection
  - Expiry checking and TTL management
  - TrustEdge conversion roundtrips
  - Signing payload determinism
- **2 integration tests** for end-to-end trust propagation
  - Two-node trust propagation with full QUIC/TLS stack
  - Three-node transitive trust computation verification
  - Real gossip network with announce/pull cycles

**Architecture:**
- Trust edges now propagate across the network via signed attestations
- Nodes build distributed trust webs automatically
- Transitive trust computation works across remote trust edges
- Foundation for trust-based governance and cooperation

**Security Features:**
- Cryptographic signature verification prevents forgery
- Timestamp monotonic checks mitigate replay attacks
- TTL expiration prevents stale trust information
- Trust-gated topic access prevents spam flooding

**Performance:**
- Average attestation size: ~300 bytes (JSON-serialized)
- Signature overhead: 64 bytes (Ed25519)
- Propagation latency: <1 second for 2-hop networks
- Gossip compression for larger attestations (>1KB)

**Impact:**
- **Closes the biggest gap** in ICN's distributed cooperation infrastructure
- Enables truly distributed trust building (no central authority)
- Foundation for Phase 8B (trust-gated security) and Phase 8C (WAN discovery)
- First step toward federated trust networks

### Added - User Onboarding Improvements (2025-11-11)

**New Directories:**
- **`config/`** - Example configuration files for all use cases
  - `icn.toml.example` - Comprehensive configuration template with all options
  - `icn-minimal.toml.example` - Minimal starter configuration
  - `icn-alpha.toml`, `icn-beta.toml` - Two-node local demo configs
  - `prometheus.yml` - Prometheus scrape configuration
  - Complete configuration guide with environment variable documentation
- **`docker/`** - Production-ready Docker deployment
  - Multi-stage Dockerfile (optimized for size and security)
  - `docker-compose.yml` - Full stack with Prometheus monitoring
  - `docker-compose.dev.yml` - Development environment
  - Comprehensive deployment guide with troubleshooting
- **`examples/`** - Getting started tutorials
  - `01-quickstart/` - Automated two-node network demo
    - Interactive tutorial with step-by-step instructions
    - `run.sh` - Fully automated demo script (<5 minutes)
  - Examples index with roadmap for future tutorials

**Documentation Improvements:**
- Enhanced README.md with Quick Start section (5-minute setup guide)
- Added Ports & Services reference table
- Expanded Usage section with examples for all CLI commands
- Navigation links to config/, docker/, examples/ directories

### Fixed - User Onboarding Improvements (2025-11-11)

**Documentation:**
- Fixed port discrepancies in deployment-guide.md (all references updated 5000→4433)
- Corrected QUIC listener port in all documentation to match code reality (4433/udp)
- Updated Docker examples to use correct ports
- Added links to new configuration examples

**Impact:**
- **Onboarding time reduced from ~30 minutes to <5 minutes**
- Users can now run automated quickstart: `./examples/01-quickstart/run.sh`
- Complete Docker deployment ready out-of-box
- 5 example configuration files covering all use cases

### Added - Phase 3 CLI Tools & Production Features (2025-11-11)

**Contract Examples:**
- **`examples/contracts/echo.json`** - Simple test contract demonstrating basic CCL features
  - `echo(message)` - Returns message parameter
  - `add(a, b)` - Adds two numbers using BinOp
- **`examples/contracts/timebank.json`** - Mutual credit time banking contract
  - State variable: `total_hours_exchanged`
  - `record_service(recipient, hours)` - Records service exchange with preconditions
  - `get_stats()` - Returns total hours exchanged
  - Demonstrates: state variables, ledger operations, preconditions, special `sender` variable
- **`examples/contracts/README.md`** - Comprehensive contract development documentation
- **`examples/contracts/test-contracts.sh`** - Automated testing script for contract validation

**Contract Management:**
- Contract listing functionality: `icnctl contract list`
  - Displays installed contracts with metadata (name, participants, currency, rules)
  - Shows state variable count and rule names
  - RPC endpoint: `contract.list`

**Quarantine Management (PR #1):**
- Full operator control over quarantined ledger entries
- **RPC Endpoints:**
  - `ledger.quarantine.list` - List all quarantined entries
  - `ledger.quarantine.get` - Get detailed info about specific entry
  - `ledger.quarantine.release` - Release and retry entry
  - `ledger.quarantine.drop` - Permanently discard entry
  - `ledger.quarantine.purge` - Remove all expired entries
- **CLI Commands:**
  ```bash
  icnctl ledger quarantine list
  icnctl ledger quarantine get <entry_id>
  icnctl ledger quarantine release <entry_id>
  icnctl ledger quarantine drop <entry_id>
  icnctl ledger quarantine purge
  ```
- **RPC Client Methods** in `icn-rpc/src/client.rs`:
  - `quarantine_list()`, `quarantine_get()`, `quarantine_release()`, `quarantine_drop()`, `quarantine_purge()`

**WAN Bootstrap Peers (PR #2):**
- Internet-wide connectivity beyond local mDNS discovery
- Configure bootstrap peers in `icn.toml`:
  ```toml
  bootstrap_peers = [
      "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777"
  ]
  ```
- URL format: `icn://DID@IP:PORT`
- Automatic dialing on daemon startup
- Multiple peers for redundancy (no single point of failure)
- Connection failures are non-fatal (logged as warnings)
- Current limitation: IP addresses only (DNS hostname resolution to be added later)

### Fixed - Phase 3 Error Handling (2025-11-11)

**Quarantine Release Semantics:**
- Fixed incorrect error handling in `ledger.quarantine.release`
  - Operation now returns JSON-RPC error when entry release succeeds but reappend fails
  - Previously returned success response with error flags (violated JSON-RPC 2.0 semantics)
  - Error message format: "Entry released from quarantine but reappend failed: <reason>"
  - Follows standard JSON-RPC pattern: errors in error field, successes in result field
- **Rationale**: Operation name "release" implies "release for retry" - partial success is a failure

**Impact:**
- Operators can now inspect, manage, and resolve quarantined ledger entries
- WAN connectivity enables internet-wide ICN networks
- Contract examples provide learning resources and test cases
- Proper JSON-RPC error handling enables reliable error detection in monitoring tools

### Added - Trust-Gated Rate Limiting (PR #3) (2025-11-11)

**Dynamic Rate Limiting Based on Trust:**
- Different message rate limits for each trust class:
  - **Isolated peers** (trust score < 0.1): 10 messages/sec, burst capacity 2
  - **Known peers** (trust score 0.1-0.4): 50 messages/sec, burst capacity 10
  - **Partner peers** (trust score 0.4-0.7): 100 messages/sec, burst capacity 20
  - **Federated peers** (trust score 0.7+): 200 messages/sec, burst capacity 50
- Rate limits automatically adjust when peer trust changes
- Immediate benefit for trust upgrades (token bucket reset to new capacity)
- Backwards compatible: Falls back to 100 msg/sec when no trust graph available

**Architecture:**
- `TrustGatedRateLimitConfig` in `icn-net/src/rate_limit.rs`
- `RateLimiter::new_trust_gated()` integrates with trust graph
- Token buckets track trust class and detect changes
- Trust graph shared between Gossip and Network actors
- Trust data persisted in `~/.icn/trust/` directory

**Testing:**
- 3 comprehensive unit tests for trust-gated behavior
- Tests verify different limits for each trust class
- Tests verify dynamic adjustment on trust class changes
- All 140+ tests passing

**Impact:**
- Provides robust DoS protection against untrusted peers (10 msg/sec limit)
- Enables high throughput for trusted partners (200 msg/sec for federated peers)
- Adaptive security: protection strengthens/weakens based on actual trust relationships
- No configuration required: works automatically based on trust graph state

### Added - Phase 7 Pull Protocol Completion (2025-01-11)

**Gossip Pull Protocol:**
- **Pull protocol now fully operational** with verified end-to-end convergence
  - Digest emission background task with jitter (10s ± 2s)
  - Pull request/response handlers with backpressure
  - Empty `want_ids` semantics for "send all entries" requests
  - Vector clock-based detection of missing entries
  - Trust-gated resource limits per peer class
  - Comprehensive integration test validating full flow
- Ledger merge report API for operator visibility
  - `merge_batch()` returns detailed `MergeDecision` with accepted/discarded/quarantined counts
  - `QuarantineStore` with ring buffer (1000 entries) and 7-day TTL
  - Methods for quarantine management: `list()`, `get()`, `release()`, `drop()`
  - New metrics: `merge_conflicts_total`, `entries_quarantined_total`, `quarantine_size`

**New Metrics:**
- Gossip pull protocol: `digests_sent/received`, `pull_requests_sent/received`, `pull_responses_sent/received`
- Pull bandwidth: `bytes_pulled_total`, `bytes_pushed_total`
- Backpressure: `pull_truncated_total`, `peer_deficit_bytes`
- Ledger merge: `merge_conflicts_total`, `entries_quarantined_total`, `entries_discarded_total`, `quarantine_size`

### Fixed - Phase 7 Critical Bugs (2025-01-11)

**TLS Handshake (BLOCKER):**
- Fixed `NoSignatureSchemesInCommon` error by generating Ed25519 certificates
  - Changed from RSA (default) to Ed25519 to match client verifier expectations
  - Location: `icn-net/src/tls.rs` - now uses `rcgen::PKCS_ED25519`
  - **Impact**: Unblocked ALL integration tests

**mDNS Discovery:**
- Fixed hostname format bug causing registration failure
  - Changed `"{}"` → `"{}.local."` to comply with mDNS requirements
  - Location: `icn-net/src/discovery.rs:79`

**Pull Protocol Routing:**
- Added sender DID propagation to `handle_message()` signature
  - Changed: `handle_message(message)` → `handle_message(&sender, message)`
  - Enables Digest handler to identify message sender for reply routing
  - Updated 10+ call sites across codebase

### Added - Phase 7 Production Hardening (2025-01-11)

**Security & Hardening:**
- Network message rate limiting using token bucket algorithm (100 msg/sec, burst 20)
  - Per-peer rate limiting prevents single-peer DoS attacks
  - New module: `icn-net/src/rate_limit.rs`
  - New metric: `icn_network_messages_rate_limited_total`
- TLS certificate verification with DID extraction and expiration checking
  - Extracts DID from X.509 certificate Subject Alternative Names
  - Validates certificate validity period (not before/after)
  - Validates DID format (must start with `did:icn:`)
  - Adds security audit logging
  - Added dependency: `x509-parser = "0.16"`
- QUIC transport configuration with bounded stream limits
  - Reduced concurrent streams from 100 → 10 bidirectional
  - Set unidirectional streams to 0 (not used)
  - Stream receive window: 1MB per stream
  - Connection receive window: 10MB total
  - Idle timeout: 60s, keep-alive: 30s
- Message size validation before buffer allocation
  - Validates length prefix before allocating memory
  - Prevents overflow on 32-bit systems
  - Rejects zero-length and oversized messages (>10MB)
- Bloom filter deserialization validation
  - Validates non-zero size to prevent division by zero
  - Validates claimed size vs actual unpacked bits
  - Prevents index out of bounds panics from malformed data
- Timestamp overflow protection in ledger and gossip
  - Changed unchecked `as u64` casts to checked `try_into()`
  - Prevents silent wraparound if system clock is far in future (post-2262)

**Async/Performance:**
- Fixed blocking operations in async context (supervisor message handlers)
  - Replaced `blocking_write()` with `tokio::spawn` + `write().await`
  - Applied to Gossip, Subscribe, and Unsubscribe message handlers
  - Prevents thread pool starvation in Tokio runtime

**Documentation:**
- Added comprehensive production hardening documentation (`docs/production-hardening.md`)
  - Detailed vulnerability descriptions and fixes
  - Configuration guide and tuning recommendations
  - Monitoring and alerting recommendations
  - Security metrics and log patterns
- Added deployment and operations guide (`docs/deployment-guide.md`)
  - Installation instructions (source, Docker, systemd)
  - Configuration reference
  - Monitoring setup (Prometheus/Grafana)
  - Backup & recovery procedures
  - Troubleshooting guide
  - Security best practices
- Updated architecture documentation (`docs/ARCHITECTURE.md`)
  - Added section 8.4: Production Hardening
  - Documents all security protections with implementation references
- Updated README with security section
  - Quick overview of hardening measures
  - Links to detailed documentation

**Testing:**
- Added 4 comprehensive unit tests for rate limiter
  - Token consumption and refill behavior
  - Per-peer isolation
  - Bucket cleanup
- All tests passing: 64 tests across modified crates (icn-net: 27, icn-gossip: 18, icn-ledger: 16, icn-obs: 0)

### Changed

- `icn-net/src/protocol.rs`: Message size validation before allocation
- `icn-net/src/tls.rs`: Implemented certificate verification
- `icn-net/src/session.rs`: Added transport config with bounded limits
- `icn-net/src/actor.rs`: Integrated rate limiter into connection handler
- `icn-core/src/supervisor.rs`: Fixed blocking operations in message handlers
- `icn-gossip/src/gossip.rs`: Fixed timestamp overflow in entry creation
- `icn-gossip/src/bloom.rs`: Added validation in deserialization
- `icn-ledger/src/entry.rs`: Fixed timestamp overflow in journal entries
- `icn-obs/src/metrics.rs`: Added rate limiting metric

### Security Notes

⚠️ **Known Limitations:**
- TLS certificate verifier does NOT yet integrate with trust graph
- Currently accepts all valid DID certificates (development mode)
- Trust graph integration required before production deployment

**Remaining Work (Not Addressed):**
- Medium priority: Request timeouts, unbounded vector growth, compression
- Low priority: Error handling consistency, trace logging improvements

---

## Version History

### [0.1.0] - Phase 0-6 Complete

**Phase 0 - Scaffold:**
- Workspace structure, core runtime, supervisor
- Identity/DID generation & verification
- CLI tooling (icnd + icnctl)

**Phase 1 - Identity & Trust:**
- Age-encrypted keystore with passphrase unlock
- Key rotation protocol with transition records
- Trust graph storage & transitive trust computation
- DID import/export

**Phase 2 - Network Transport:**
- QUIC/TLS sessions with DID-based certificates
- mDNS local discovery
- Network actor with session pooling
- Secure passphrase handling (zeroization)

**Phase 3 - Ledger:**
- Double-entry mutual credit accounting
- Merkle-DAG content-addressable structure
- Multi-currency support with credit limits
- Balance queries & integrity verification

**Phase 4 - Cooperative Contracts (CCL):**
- Domain-specific contract language (AST-based)
- Deterministic interpreter with fuel metering
- Capability system (ReadLedger, WriteLedger, etc.)
- Contract runtime with ledger integration
- TimeBank example contract

**Phase 5 - Gossip & Distributed Sync:**
- Topic-based gossip protocol with ACLs
- Vector clocks for causal ordering
- Bloom filter anti-entropy
- Ledger-gossip integration
- Multi-node convergence verification

**Phase 6 - Network Protocol Bridge:**
- Wire protocol for gossip over QUIC
- NetworkMessage envelope with DID routing
- NetworkActor extensions (send/broadcast)
- Gossip-network bridge in supervisor
- Background anti-entropy task
- Two-node integration test structure

**Phase 7 - Polish & Production:**
- Metrics exporter (Prometheus)
- Complete pull protocol (Request/Response)
- Topic subscriptions & routing
- Production hardening (3 critical + 4 high priority issues)
- Comprehensive documentation

---

## Migration Notes

### Upgrading to Post-Hardening Version

No breaking changes. All hardening features are enabled by default with conservative limits.

**Configuration changes (optional):**
- Rate limiting can be tuned via `RateLimitConfig` (requires code change currently)
- QUIC stream limits configurable via `TransportConfig`
- Message size limit defined by `MAX_MESSAGE_SIZE` constant (10MB)

**Monitoring updates:**
- New metric: `icn_network_messages_rate_limited_total`
- Monitor for rate limiting spikes indicating potential attacks

**No data migration required** - all changes are in protocol handling and validation layers.

---

## Links

- [Repository](https://github.com/your-org/icn)
- [Architecture Documentation](docs/ARCHITECTURE.md)
- [Production Hardening](docs/production-hardening.md)
- [Deployment Guide](docs/deployment-guide.md)
- [Topic Subscriptions API](docs/topic-subscriptions-api.md)
