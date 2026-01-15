# Module 7: Gateway API and SDK

## Objectives
- Understand gateway auth flow and scopes
- Use the TypeScript SDK to call APIs

## Prerequisites
- Module 6

## Key reading
- `icn/crates/icn-gateway/README.md`
- `sdk/typescript/README.md`
- `docs/api/`

## Walkthrough
The gateway exposes REST and WebSocket APIs with JWT auth. The SDK implements
challenge/verify and common ledger, coop, and governance operations.

## Concepts (textbook style)

### Gateway as system boundary
The gateway is the primary integration surface for apps. It translates HTTP
requests into ICN operations and enforces auth, rate limits, and scopes. This
keeps core runtime concerns isolated from external clients.

### Authentication flow
Authentication is DID-based: a client requests a challenge, signs it with its
private key, and exchanges the signature for a JWT. The JWT carries scopes and
cooperative context.

### SDK role
The TypeScript SDK encapsulates the gateway protocol and makes client usage
consistent across applications.

## Detailed walkthrough (auth flow)

### 1) Challenge request
Client calls `/auth/challenge` with a DID. The gateway:
- validates DID formatting
- applies IP‑based rate limiting for unauthenticated calls
- returns a nonce with a short expiry

### 2) Challenge verification
Client signs the nonce with its Ed25519 private key and posts `/auth/verify`.
The gateway:
- validates scopes and coop ID
- verifies signature length and content
- issues a JWT with requested scopes

### 3) Authenticated requests
Clients include `Authorization: Bearer <token>` to call protected endpoints.

## Detailed walkthrough (ledger payment)

### 1) Authorization
Gateway validates scope and coop context, then verifies the authenticated DID
matches the payment sender.

### 2) Input validation
Payment amount, currency, and memo are validated before touching the ledger.

### 3) Rate limiting and budgets
Velocity limits and optional budget constraints are applied.

### 4) Ledger write
The gateway calls `LedgerManager::create_payment`, which builds a journal entry
and appends it to the ledger.

### 5) Events and notifications
On success, the gateway publishes events to WebSocket subscribers and triggers
notifications.

## Failure modes and safeguards
- **Invalid DID or signature**: rejected before token issuance.
- **Scope violations**: request is denied by middleware.
- **Cross‑coop access**: blocked at request validation.
- **Self‑payment**: explicitly rejected in the ledger API.

## Annotated code excerpts

### Challenge endpoint creates a nonce and enforces rate limits
Source: `icn/crates/icn-gateway/src/api/auth.rs`
```rust
let client_ip = get_client_ip(&http_req);
ip_limiter.check_rate_limit(&client_ip)?;

let did = req
    .did
    .parse()
    .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

let nonce = auth.create_challenge(&did)?;
```
Unauthenticated auth endpoints are guarded with IP‑based rate limiting.

### Payment endpoint enforces identity and scope
Source: `icn/crates/icn-gateway/src/api/ledger.rs`
```rust
require_scope(&http_req, "ledger:write")?;
require_coop_access(&http_req, &coop_id)?;

let claims = get_claims(&http_req).ok_or_else(|| {
    crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
})?;

if claims.sub != req.from {
    return Err(crate::error::GatewayError::AuthorizationFailed(
        format!("Cannot create payments from other accounts (authenticated as {}, attempted to send from {})",
            claims.sub, req.from)
    ));
}
```
The gateway ensures only the authenticated DID can create payments from itself.

### Flow breakdown
1. Client calls `/auth/challenge` to receive a nonce
2. Client signs nonce with Ed25519 private key
3. Client posts `/auth/verify` to receive JWT
4. Client calls protected endpoints with `Authorization: Bearer <token>`

## Code map
- `icn/crates/icn-gateway/src/server.rs`:
  `GatewayServer` builds Actix app and wires routes and middleware.
- `icn/crates/icn-gateway/src/api/auth.rs`:
  `challenge` and `verify` implement auth flow.
- `icn/crates/icn-gateway/src/api/ledger.rs`:
  `create_payment` and `get_balance` implement ledger endpoints.
- `icn/crates/icn-gateway/src/ledger_mgr.rs`:
  `LedgerManager::create_payment` bridges API to ledger.
- `sdk/typescript/README.md`:
  `ICNClient` auth and request patterns.

## Reference files (follow-up)
- `icn/crates/icn-gateway/src/server.rs`
- `icn/crates/icn-gateway/src/api/auth.rs`
- `icn/crates/icn-gateway/src/api/ledger.rs`
- `icn/crates/icn-gateway/src/ledger_mgr.rs`
- `sdk/typescript/README.md`

## Exercises
- Describe the auth flow from challenge to JWT
- Use the SDK to call health and balance endpoints

## Checkpoints
- You can explain the gateway auth flow
- You can call an API using the SDK
