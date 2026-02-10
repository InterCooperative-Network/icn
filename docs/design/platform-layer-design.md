# ICN Platform Layer Design

**Date**: 2025-01-15
**Status**: Implementation Phase (Phase 14)
**Strategic Shift**: ICN as Cooperative Backend Platform

## Vision

**NOT**: "People run ICN"
**BUT**: "Co-ops build apps that use ICN as backend"

A food co-op builds their Shopper's Club app (React + Node), uses ICN under the hood for members/balances/contracts, and members just log in - never seeing `icnd`.

## What This Platform IS and IS NOT

**This IS:**
- A developer-facing API layer (REST + WebSocket)
- TypeScript SDK for easy integration
- Reference apps as starting points
- Multi-tenant hosting for early pilots

**This is NOT:**
- An app runtime (like AWS Lambda, Cloudflare Workers)
- A place to deploy custom backend code
- A "cooperative cloud" (that's Phase 16+, conditional on pilot validation)

**Why not a runtime now?**
- We have zero pilots - don't know what co-ops need
- 6-12 month investment without user signal
- Gateway + SDK ships in 2-3 months, validates faster
- Runtime can be added later if pilots prove it's needed

**The path:**
1. **Phase 14**: Build gateway + SDK + reference app
2. **Phase 15**: Host reference app for 1-2 pilot co-ops
3. **Phase 16+**: Let pilot feedback drive what's next (runtime, templates, self-hosting tools, etc.)

## Layer Cake Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Apps (what co-ops build)                                   │
│  - Food Co-op Shopper's Club                                │
│  - Housing Maintenance Portal                               │
│  - Timebank Scheduler                                       │
│  - Community Tool Library                                   │
└─────────────────────────────────────────────────────────────┘
                           ↕
┌─────────────────────────────────────────────────────────────┐
│  Platform Layer (what we build now)                         │
│  - REST + WebSocket API (icn-gateway)                       │
│  - App auth & capability tokens                             │
│  - Coop namespaces & isolation                              │
│  - Event streaming                                          │
│  - TypeScript/Python SDKs                                   │
└─────────────────────────────────────────────────────────────┘
                           ↕
┌─────────────────────────────────────────────────────────────┐
│  Substrate (what we have)                                   │
│  - Identity, Trust, Ledger, Gossip, Storage, Contracts      │
│  - icnd daemon, icnctl CLI                                  │
└─────────────────────────────────────────────────────────────┘
```

## What Co-ops Actually Need

### 1. Identity & Membership

**Questions apps ask:**
- "Who is this user?" → DID
- "Which co-op(s) are they part of?" → Coop namespace
- "What role are they in?" → member / worker / admin / treasurer

**API Examples:**
```typescript
// TypeScript SDK
const user = await icn.auth.getCurrentUser();
// → { did: "did:icn:abc123", name: "Alice" }

const memberships = await icn.membership.getCoops(user.did);
// → [{ coopId: "food-coop-pdx", role: "member" }, ...]

const role = await icn.membership.getRole("food-coop-pdx", user.did);
// → "member" | "worker" | "admin" | "treasurer"
```

### 2. Auth from App's POV

App can say: "User X (DID) is logged in, here's a token that lets me:
- Read their balances in this co-op
- Initiate a payment on their behalf
- Read/write certain contract state"

**Auth Flow (Capability Tokens):**
```
User Device                  App Server               icn-gateway
    │                            │                        │
    │ 1. Sign auth challenge     │                        │
    ├───────────────────────────>│                        │
    │                            │ 2. Verify + request    │
    │                            │    scoped token        │
    │                            ├───────────────────────>│
    │                            │                        │
    │                            │ 3. Issue capability    │
    │                            │    token (JWT-ish)     │
    │                            │<───────────────────────┤
    │                            │                        │
    │ 4. Token (to use in API)   │                        │
    │<───────────────────────────┤                        │
    │                            │                        │
    │ 5. API calls with token    │                        │
    ├────────────────────────────┼───────────────────────>│
    │                            │                        │
```

**Token Structure:**
```json
{
  "sub": "did:icn:abc123",
  "coop": "food-coop-pdx",
  "app": "shoppers-club",
  "capabilities": [
    "read:balance",
    "write:payment",
    "read:members"
  ],
  "exp": 1234567890,
  "iat": 1234560000
}
```

### 3. Ledger & Payments

**Simple API:**
```typescript
// Get balance
const balance = await icn.ledger.getBalance({
  coopId: "food-coop-pdx",
  did: user.did,
  currency: "coop-credits"
});
// → { balance: 150, creditLimit: -50 }

// Send payment
const tx = await icn.ledger.sendPayment({
  coopId: "food-coop-pdx",
  from: user.did,
  to: "did:icn:coop-till",
  amount: 25,
  currency: "coop-credits",
  memo: "Purchased groceries"
});
// → { txId: "tx_123...", timestamp: 1234567890 }

// Get transaction history
const history = await icn.ledger.getHistory({
  coopId: "food-coop-pdx",
  did: user.did,
  limit: 50
});
```

All the hard work (mutual credit, Merkle-DAG, signatures) stays inside ICN substrate.

### 4. Groups / Spaces (Coop Namespaces)

Each co-op = a namespace with:
- Members list
- Role bindings
- Trust rules
- Ledger domains (currencies)
- Contract set

**Data Model:**
```rust
pub struct CoopNamespace {
    /// Unique co-op ID
    pub id: CoopId,

    /// Human-readable name
    pub name: String,

    /// Member DIDs
    pub members: Vec<Did>,

    /// Role assignments (did -> roles)
    pub roles: HashMap<Did, Vec<String>>,

    /// Ledger currencies for this co-op
    pub currencies: Vec<String>,

    /// Trust rules (optional, per-coop overrides)
    pub trust_rules: Option<TrustConfig>,

    /// Installed contracts
    pub contracts: Vec<ContentHash>,

    /// Apps authorized for this co-op
    pub apps: Vec<AppRegistration>,

    /// Created timestamp
    pub created_at: u64,
}

pub struct AppRegistration {
    /// Unique app ID
    pub id: AppId,

    /// App name
    pub name: String,

    /// Allowed capabilities
    pub capabilities: Vec<String>,

    /// App public key (for verifying requests)
    pub public_key: Vec<u8>,
}
```

**API Examples:**
```typescript
// Create a new co-op namespace
const coop = await icn.coops.create({
  name: "Portland Food Co-op",
  foundingMembers: [alice.did, bob.did, carol.did],
  currencies: ["coop-credits"],
});
// → { coopId: "food-coop-pdx", ... }

// Add member
await icn.coops.addMember({
  coopId: "food-coop-pdx",
  member: dave.did,
  role: "member"
});

// List co-op members
const members = await icn.coops.getMembers("food-coop-pdx");
// → [{ did: "did:icn:...", role: "member", joinedAt: 123... }, ...]
```

### 5. Events (Real-Time Subscriptions)

App can subscribe to:
- New transactions
- Member joined/left
- Contract state changes
- Governance proposals/votes

**WebSocket API:**
```typescript
// Subscribe to co-op events
const stream = icn.events.subscribe({
  coopId: "food-coop-pdx",
  eventTypes: ["transaction", "member_joined", "proposal_created"]
});

stream.on("transaction", (event) => {
  console.log(`New transaction: ${event.from} → ${event.to}: ${event.amount}`);
  // Update UI live
});

stream.on("member_joined", (event) => {
  console.log(`${event.member} joined as ${event.role}`);
});
```

**Event Types:**
```typescript
type ICNEvent =
  | { type: "transaction", coopId: string, from: DID, to: DID, amount: number, currency: string, timestamp: number }
  | { type: "member_joined", coopId: string, member: DID, role: string, timestamp: number }
  | { type: "member_left", coopId: string, member: DID, timestamp: number }
  | { type: "role_changed", coopId: string, member: DID, oldRole: string, newRole: string, timestamp: number }
  | { type: "proposal_created", coopId: string, proposalId: string, author: DID, subject: string, timestamp: number }
  | { type: "proposal_voted", coopId: string, proposalId: string, voter: DID, vote: Vote, timestamp: number }
  | { type: "contract_executed", coopId: string, contractHash: string, caller: DID, result: any, timestamp: number };
```

---

## Platform Components

### 1. icn-gateway (New Crate)

**Purpose**: Expose stable, language-agnostic API for co-op apps.

**Stack**:
- Actix-web (Rust async HTTP server)
- REST endpoints (JSON)
- WebSocket for event streaming
- Talks to `icnd` via internal RPC

**Directory Structure**:
```
icn/crates/icn-gateway/
├── src/
│   ├── lib.rs           # Gateway server
│   ├── auth.rs          # Capability token handling
│   ├── api/
│   │   ├── mod.rs
│   │   ├── coops.rs     # /coops/* endpoints
│   │   ├── ledger.rs    # /ledger/* endpoints
│   │   ├── members.rs   # /members/* endpoints
│   │   ├── contracts.rs # /contracts/* endpoints
│   │   └── events.rs    # WebSocket event streaming
│   ├── models.rs        # API request/response types
│   └── error.rs         # HTTP error responses
└── Cargo.toml
```

**API Surface** (detailed below)

---

### 2. Coop Namespaces (Extend icn-store)

**Purpose**: Isolate co-ops from each other. Each co-op = separate namespace for members, ledgers, contracts.

**Storage Schema** (in Sled):
```
Prefix: "coop:{coop_id}:*"

coop:{coop_id}:meta          → CoopNamespace (bincode)
coop:{coop_id}:members       → Vec<Did> (bincode)
coop:{coop_id}:roles:{did}   → Vec<String> (roles)
coop:{coop_id}:apps          → Vec<AppRegistration> (bincode)

# Ledger entries scoped per co-op
coop:{coop_id}:ledger:balance:{did}:{currency} → i64
coop:{coop_id}:ledger:tx:{tx_id}               → JournalEntry

# Governance scoped per co-op
coop:{coop_id}:proposals:{prop_id}             → Proposal
```

**Implementation**:
- Add `icn-store/src/coops.rs` for CRUD operations
- Add namespace isolation to ledger operations
- Add per-coop governance storage (proposals, votes)

---

### 3. App Auth & Capability Tokens

**Purpose**: Apps get scoped tokens to act on behalf of users. Not OAuth (no third-party IdP), but similar flow.

**Token Issuance Flow**:
1. User signs a challenge with their DID keypair
2. App verifies signature, requests token from `icn-gateway`
3. Gateway issues short-lived JWT-like token with capabilities
4. App includes token in `Authorization: Bearer <token>` header

**Token Claims**:
```json
{
  "sub": "did:icn:abc123",        // User DID
  "coop": "food-coop-pdx",        // Coop namespace
  "app": "shoppers-club",         // App ID
  "capabilities": [
    "read:balance",
    "write:payment",
    "read:members",
    "read:proposals"
  ],
  "exp": 1234567890,              // Expiration (1 hour default)
  "iat": 1234560000,              // Issued at
  "jti": "token_unique_id"        // Token ID (for revocation)
}
```

**Capability Scopes**:
- `read:balance` - Read user's balance in co-op
- `write:payment` - Initiate payment on user's behalf
- `read:members` - List co-op members
- `write:members` - Add/remove members (admin only)
- `read:proposals` - View governance proposals
- `write:proposals` - Create proposals
- `vote:proposals` - Vote on proposals
- `execute:contract:{hash}` - Execute specific contract

**Revocation**:
- Gateway maintains revocation list (Redis or in-memory)
- On token verification, check if `jti` is revoked
- User can revoke all tokens for an app
- Tokens auto-expire after 1 hour (refresh flow)

**Implementation**:
- `icn-gateway/src/auth.rs` - Token issuance/verification
- Use `jsonwebtoken` crate for JWT handling
- Ed25519 signing (same as DID keypairs)

---

### 4. TypeScript SDK (@icn/client)

**Purpose**: Make ICN trivial to use from JavaScript/TypeScript apps.

**Package Structure**:
```
sdk/typescript/
├── package.json
├── src/
│   ├── index.ts          # Main exports
│   ├── client.ts         # ICNClient class
│   ├── auth.ts           # Authentication
│   ├── coops.ts          # Coop management
│   ├── ledger.ts         # Ledger operations
│   ├── members.ts        # Membership
│   ├── events.ts         # WebSocket event streaming
│   ├── types.ts          # TypeScript types
│   └── utils.ts          # Helpers
└── tsconfig.json
```

**Usage Example**:
```typescript
import { ICNClient } from '@icn/client';

// Initialize client
const icn = new ICNClient({
  gatewayUrl: 'http://localhost:8000',
});

// Authenticate (user signs challenge with wallet/device)
const user = await icn.auth.login({
  did: 'did:icn:abc123',
  signChallenge: async (challenge) => {
    // Sign with device key
    return await wallet.sign(challenge);
  }
});

// Get token for co-op app
const token = await icn.auth.getToken({
  coopId: 'food-coop-pdx',
  appId: 'shoppers-club',
  capabilities: ['read:balance', 'write:payment']
});

// Use token for API calls
icn.setToken(token);

// Get balance
const balance = await icn.ledger.getBalance({
  coopId: 'food-coop-pdx',
  did: user.did,
  currency: 'coop-credits'
});

// Send payment
const tx = await icn.ledger.sendPayment({
  coopId: 'food-coop-pdx',
  from: user.did,
  to: 'did:icn:coop-till',
  amount: 25,
  currency: 'coop-credits',
  memo: 'Groceries'
});

// Subscribe to events
const stream = icn.events.subscribe({
  coopId: 'food-coop-pdx',
  eventTypes: ['transaction', 'member_joined']
});

stream.on('transaction', (event) => {
  console.log('New transaction:', event);
});
```

---

## icn-gateway API Reference

### Base URL
```
http://localhost:8000/api/v1
```

### Authentication

All endpoints except `/auth/*` require `Authorization: Bearer <token>` header.

#### POST /auth/challenge
Generate authentication challenge for DID.

**Request:**
```json
{
  "did": "did:icn:abc123"
}
```

**Response:**
```json
{
  "challenge": "random_base64_string",
  "expiresAt": 1234567890
}
```

#### POST /auth/verify
Verify signed challenge and issue token.

**Request:**
```json
{
  "did": "did:icn:abc123",
  "challenge": "random_base64_string",
  "signature": "ed25519_signature_base64",
  "coopId": "food-coop-pdx",
  "appId": "shoppers-club",
  "capabilities": ["read:balance", "write:payment"]
}
```

**Response:**
```json
{
  "token": "jwt_token_string",
  "expiresAt": 1234567890
}
```

#### POST /auth/revoke
Revoke a token.

**Request:**
```json
{
  "tokenId": "token_unique_id"
}
```

**Response:**
```json
{
  "success": true
}
```

---

### Co-ops

#### POST /coops
Create a new co-op namespace.

**Request:**
```json
{
  "name": "Portland Food Co-op",
  "foundingMembers": ["did:icn:abc", "did:icn:def"],
  "currencies": ["coop-credits"],
  "roles": {
    "did:icn:abc": ["admin", "member"],
    "did:icn:def": ["member"]
  }
}
```

**Response:**
```json
{
  "coopId": "food-coop-pdx",
  "name": "Portland Food Co-op",
  "createdAt": 1234567890
}
```

#### GET /coops/{coopId}
Get co-op details.

**Response:**
```json
{
  "id": "food-coop-pdx",
  "name": "Portland Food Co-op",
  "memberCount": 42,
  "currencies": ["coop-credits"],
  "createdAt": 1234567890
}
```

#### GET /coops/{coopId}/members
List co-op members.

**Response:**
```json
{
  "members": [
    {
      "did": "did:icn:abc123",
      "roles": ["admin", "member"],
      "joinedAt": 1234567890
    },
    ...
  ]
}
```

#### POST /coops/{coopId}/members
Add a member to co-op.

**Request:**
```json
{
  "member": "did:icn:xyz789",
  "role": "member"
}
```

**Response:**
```json
{
  "success": true,
  "member": "did:icn:xyz789",
  "role": "member"
}
```

---

### Ledger

#### GET /ledger/{coopId}/balance/{did}
Get balance for a DID in a co-op.

**Query Parameters:**
- `currency` (optional) - Filter by currency

**Response:**
```json
{
  "balances": [
    {
      "currency": "coop-credits",
      "balance": 150,
      "creditLimit": -50
    }
  ]
}
```

#### POST /ledger/{coopId}/payment
Send a payment.

**Request:**
```json
{
  "from": "did:icn:abc123",
  "to": "did:icn:coop-till",
  "amount": 25,
  "currency": "coop-credits",
  "memo": "Purchased groceries"
}
```

**Response:**
```json
{
  "txId": "tx_1234567890_abc",
  "from": "did:icn:abc123",
  "to": "did:icn:coop-till",
  "amount": 25,
  "currency": "coop-credits",
  "timestamp": 1234567890,
  "newBalance": 125
}
```

#### GET /ledger/{coopId}/history/{did}
Get transaction history.

**Query Parameters:**
- `limit` (optional, default 50)
- `offset` (optional, default 0)
- `currency` (optional)

**Response:**
```json
{
  "transactions": [
    {
      "txId": "tx_...",
      "from": "did:icn:abc",
      "to": "did:icn:def",
      "amount": 25,
      "currency": "coop-credits",
      "memo": "Groceries",
      "timestamp": 1234567890
    },
    ...
  ],
  "total": 234
}
```

---

### Events (WebSocket)

#### WS /events/subscribe

**Connect:**
```
ws://localhost:8000/api/v1/events/subscribe?token=<jwt_token>
```

**Subscribe Message:**
```json
{
  "action": "subscribe",
  "coopId": "food-coop-pdx",
  "eventTypes": ["transaction", "member_joined", "proposal_created"]
}
```

**Event Messages:**
```json
{
  "type": "transaction",
  "coopId": "food-coop-pdx",
  "from": "did:icn:abc",
  "to": "did:icn:def",
  "amount": 25,
  "currency": "coop-credits",
  "timestamp": 1234567890
}
```

---

## Food Co-op Example Walkthrough

### Phase 1: Identity + Anchor

Co-op builds basic web app:

**Member Onboarding:**
1. New member downloads ICN wallet app (or web extension)
2. Generates DID keypair on device
3. Admin adds them to co-op: `POST /coops/food-coop-pdx/members`
4. Member can now log into Shopper's Club app

**App Auth:**
1. User clicks "Sign In" in Shopper's Club
2. App requests challenge: `POST /auth/challenge`
3. Wallet signs challenge with DID key
4. App verifies: `POST /auth/verify` → gets token
5. App uses token for all API calls

At this stage: ICN = identity + event anchor. App uses Postgres for inventory.

### Phase 2: Loyalty Points (Mutual Credit)

Co-op adds "Shopper Points":

**Earning Points:**
```typescript
// When member works a shift
await icn.ledger.sendPayment({
  coopId: 'food-coop-pdx',
  from: 'did:icn:coop-treasury',
  to: member.did,
  amount: 10,
  currency: 'coop-credits',
  memo: 'Worked Friday stocking shift'
});
```

**Displaying Balance:**
```typescript
const balance = await icn.ledger.getBalance({
  coopId: 'food-coop-pdx',
  did: member.did,
  currency: 'coop-credits'
});
// Show: "You have 150 co-op credits"
```

At this stage: ICN = identity + mutual credit ledger. App still uses Postgres for products.

### Phase 3: Payment at Checkout

**Checkout Flow:**
```typescript
// Show payment options
const balance = await icn.ledger.getBalance({ ... });
// Display: "Pay with: [Credit Card] [Co-op Credits (balance: 150)]"

// User chooses co-op credits
await icn.ledger.sendPayment({
  coopId: 'food-coop-pdx',
  from: member.did,
  to: 'did:icn:coop-till',
  amount: 25,
  currency: 'coop-credits',
  memo: 'Purchase #4567'
});
// → Transaction recorded, balance updated
```

Now: ICN is doing real value accounting, but user just sees "payment completed."

---

## Implementation Plan

### Milestone 1: Gateway + Namespaces (2 weeks)
1. Create `icn-gateway` crate with Actix-web
2. Implement `/auth/*` endpoints (challenge, verify, revoke)
3. Extend `icn-store` with coop namespace storage
4. Implement `/coops/*` endpoints (create, get, members)
5. Add capability token verification middleware

### Milestone 2: Ledger API (1 week)
1. Implement `/ledger/*` endpoints (balance, payment, history)
2. Add ledger scoping per co-op namespace
3. Test with manual curl/Postman requests

### Milestone 3: Events (1 week)
1. Implement WebSocket event streaming
2. Hook ledger operations to emit events
3. Hook membership operations to emit events
4. Test real-time updates

### Milestone 4: TypeScript SDK (2 weeks)
1. Set up `sdk/typescript` package
2. Implement `ICNClient` class
3. Implement auth, coops, ledger, events modules
4. Write comprehensive tests
5. Publish to npm as `@icn/client`

### Milestone 5: Reference App (2 weeks)
1. Build "Tiny Food Co-op Shopper's Club" (Next.js)
2. Features:
   - Member login (DID auth)
   - View balance
   - Send payment
   - Transaction history
   - Live event updates
3. Deploy as demo

**Total: ~8 weeks for complete platform layer**

---

## Open Questions

1. **Token Refresh**: How long should tokens last? Refresh flow needed?
2. **Rate Limiting**: Per-app? Per-user? Per-co-op?
3. **Multi-Coop Users**: Can one user be in multiple co-ops? (Yes, but which token structure?)
4. **Contract Execution**: Should gateway expose contract execution API, or only via SDK helpers?
5. **Event Filtering**: Can apps subscribe to events for specific DIDs (privacy concern)?
6. **Governance Integration**: Should governance be in v1 of gateway, or phase 2?

---

## Success Criteria

**For a co-op app developer:**
- Can build a working app in < 1 day
- Doesn't need to understand QUIC, gossip, Merkle-DAG, or vector clocks
- SDK docs answer 90% of questions
- Reference app is copy-paste starting point

**For co-op members:**
- Sign in with wallet (or eventually phone biometric)
- See balance, send payment, view history
- Updates are instant (WebSocket events)
- Never hear the word "daemon" or "CLI"

**For ICN:**
- Platform layer is stable API boundary
- Substrate can evolve underneath without breaking apps
- Multiple co-ops can run on one ICN node (isolation via namespaces)
- Apps "just work" and don't feel blockchain-y or crypto-y

---

**Next Steps:**
1. ✅ Design complete (this doc)
2. ⏭️ Implement icn-gateway Milestone 1 (auth + namespaces)
3. ⏭️ Implement TypeScript SDK
4. ⏭️ Build reference app (Food Co-op Shopper's Club)

---

**Last Updated**: 2025-01-15
**Contributors**: This design is a living document. Propose changes via issues/PRs.
