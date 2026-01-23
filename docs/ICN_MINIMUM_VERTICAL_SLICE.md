# ICN: Minimum Vertical Slice

**Last Updated**: 2026-01-23
**Status**: Developer Reference
**Authors**: Claude Code, fahertym

---

## Purpose

This document traces 8 fundamental operations end-to-end through the ICN stack. For each operation:

```
Client Call → Gateway Endpoint → Manager/Service → Daemon Action → Gossip Topic → Storage Write
```

This is the bridge between "architecture docs" and "the actual repo."

---

## 1. Create DID in Wallet

**User action**: Install app, first launch

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT (Mobile/Web)                                                          │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. Generate Ed25519 keypair in Secure Enclave                               │
│    - iOS: SecureEnclave.generateKey()                                       │
│    - Android: KeyStore.generateKeyPair()                                    │
│                                                                              │
│ 2. Derive DID from public key                                               │
│    did = "did:icn:" + base58_encode(public_key)                             │
│                                                                              │
│ 3. Store keypair reference locally                                          │
│    - Never exported, hardware-bound                                         │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    │ (No network call - identity is local)
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ LOCAL STORAGE                                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: ~/.icn/identity.json (or platform-specific secure storage)            │
│ {                                                                            │
│   "did": "did:icn:5K7gS...",                                                │
│   "created_at": "2026-01-23T...",                                           │
│   "key_handle": "secure_enclave_ref_123"                                    │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Crate path**: `icn-identity/src/did.rs` (DID formatting), client-side SDK handles key generation

**Key insight**: DID creation is purely local. No server involved until you want to *do* something with the identity.

---

## 2. Register Device

**User action**: Add a second device to existing identity

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL                                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /identity/devices                                                       │
│ Authorization: Bearer <jwt-from-existing-device>                            │
│ {                                                                            │
│   "device_id": "iphone-14",                                                 │
│   "label": "Alice's iPhone 14",                                             │
│   "public_key": "ed25519:ABC123...",                                        │
│   "capabilities": ["sign", "encrypt"],                                      │
│   "authorizing_device_id": "macbook-1"                                      │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/devices.rs                                        │
│                                                                              │
│ #[post("/identity/devices")]                                                │
│ pub async fn register_device(...)                                           │
│   1. require_scope("identity:write")                                        │
│   2. Verify authorizing device is valid for this DID                        │
│   3. Validate device_id, public_key format                                  │
│   4. Call identity_mgr.register_device(...)                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ IDENTITY MANAGER                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/identity_mgr.rs                                       │
│                                                                              │
│ pub async fn register_device(&self, req: RegisterDeviceRequest)             │
│   1. Check device limit (max 10 per identity)                               │
│   2. Verify authorizing device signature                                    │
│   3. Create DeviceCredential { device_id, public_key, capabilities, ... }   │
│   4. Store in device registry                                               │
│   5. Broadcast to gossip (optional, for cross-node sync)                    │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ STORAGE WRITE                                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│ Sled tree: "identity:devices:{did}"                                         │
│ Key: device_id                                                              │
│ Value: DeviceCredential (CBOR encoded)                                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Gossip topic**: `identity:devices` (optional, for multi-node cooperative setups)

---

## 3. Obtain Membership Credential

**User action**: Apply to join a cooperative, get approved

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL (Application)                                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /coops/{coop_id}/members/apply                                         │
│ Authorization: Bearer <jwt>                                                 │
│ {                                                                            │
│   "invite_code": "ABC123",                                                  │
│   "name": "Alice Smith",                                                    │
│   "email": "alice@example.com"                                              │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/membership/mod.rs                                 │
│                                                                              │
│ #[post("/{coop_id}/members/apply")]                                         │
│ pub async fn apply_for_membership(...)                                      │
│   1. Validate invite code                                                   │
│   2. Check applicant not already member                                     │
│   3. Create MembershipApplication { did, status: Pending, ... }             │
│   4. Notify admins via event                                                │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    │ (Later: Admin approves via governance or direct action)
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL (Admin Approval)                                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /coops/{coop_id}/members/{did}/approve                                 │
│ Authorization: Bearer <admin-jwt>                                           │
│ { "roles": ["member"], "initial_credit_limit": 100 }                        │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/members.rs                                        │
│                                                                              │
│ #[post("/{coop_id}/members/{did}/approve")]                                 │
│ pub async fn approve_member(...)                                            │
│   1. require_scope("members:admin")                                         │
│   2. Verify approver has admin role in this coop                            │
│   3. Issue MembershipCredential (VC)                                        │
│   4. Set initial credit limit in ledger                                     │
│   5. Add to trust graph with initial attestation                            │
│   6. Broadcast MemberJoined event                                           │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ DAEMON ACTIONS                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. Trust Graph (icn-trust)                                                  │
│    - Add edge: coop → member with initial score                             │
│    - File: icn-trust/src/graph.rs                                           │
│                                                                              │
│ 2. Ledger (icn-ledger)                                                      │
│    - Create account with credit limit                                       │
│    - File: icn-ledger/src/ledger.rs                                         │
│                                                                              │
│ 3. Entity Registry (icn-entity)                                             │
│    - Add member to entity membership list                                   │
│    - File: icn-entity/src/registry.rs                                       │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GOSSIP TOPICS                                                                │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. "members:{coop_id}" - MemberJoined event                                 │
│ 2. "trust:attestations" - Initial trust edge                                │
│ 3. "ledger:{coop_id}:accounts" - New account creation                       │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ STORAGE WRITES                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. Sled: "members:{coop_id}:{did}" → MembershipCredential                   │
│ 2. Sled: "trust:edges:{from}:{to}" → TrustEdge                              │
│ 3. Sled: "ledger:{coop_id}:accounts:{did}" → Account                        │
│ 4. Sled: "entity:{entity_id}:members" → Vec<Did>                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Make a Payment

**User action**: Send 2 hours to another member

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL                                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /ledger/{coop_id}/payment                                              │
│ Authorization: Bearer <jwt>                                                 │
│ {                                                                            │
│   "from": "did:icn:alice...",                                               │
│   "to": "did:icn:bob...",                                                   │
│   "amount": 2.0,                                                            │
│   "currency": "hours",                                                      │
│   "memo": "Garden help"                                                     │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/ledger.rs:51                                      │
│                                                                              │
│ #[post("/{coop_id}/payment")]                                               │
│ pub async fn create_payment(...)                                            │
│   1. require_scope("ledger:write")                                          │
│   2. require_coop_access(&coop_id)                                          │
│   3. CRITICAL: Verify claims.sub == req.from (can't send from others)       │
│   4. Validate amount, currency, memo                                        │
│   5. Check velocity limits (anti-drain)                                     │
│   6. Call ledger_mgr.create_payment(...)                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ LEDGER MANAGER                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/ledger_mgr.rs                                         │
│                                                                              │
│ pub async fn create_payment(...)                                            │
│   1. Check sender has sufficient balance (balance + credit_limit >= amount) │
│   2. Check budget constraints (if applicable)                               │
│   3. Forward to daemon via ledger handle                                    │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ ICN DAEMON (Ledger Actor)                                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-ledger/src/ledger.rs                                              │
│                                                                              │
│ pub fn transfer(...)                                                        │
│   1. Create double-entry:                                                   │
│      - Debit entry: Alice -2 hours                                          │
│      - Credit entry: Bob +2 hours                                           │
│   2. Sign entry with node key                                               │
│   3. Append to Merkle-DAG                                                   │
│   4. Update account balances                                                │
│   5. Return entry hash                                                      │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GOSSIP BROADCAST                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ Topic: "ledger:{coop_id}:entries"                                           │
│ Message: SignedLedgerEntry { entry, signature, dag_links }                  │
│ Scope: CooperativeOnly (stays within this coop)                             │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ STORAGE WRITES                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. Sled: "ledger:{coop_id}:entries:{hash}" → LedgerEntry                    │
│ 2. Sled: "ledger:{coop_id}:dag:{hash}" → MerkleNode                         │
│ 3. Sled: "ledger:{coop_id}:balances:{alice}" → Balance (updated)            │
│ 4. Sled: "ledger:{coop_id}:balances:{bob}" → Balance (updated)              │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ WEBSOCKET EVENT (to both parties)                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/events.rs                                             │
│                                                                              │
│ GatewayEvent::Payment {                                                     │
│   from: "did:icn:alice...",                                                 │
│   to: "did:icn:bob...",                                                     │
│   amount: 2.0,                                                              │
│   currency: "hours",                                                        │
│   entry_hash: "abc123...",                                                  │
│   timestamp: "2026-01-23T..."                                               │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Total latency**: ~50-200ms (local), ~500ms-2s (cross-node sync)

---

## 5. Subscribe to Events

**User action**: Open app, connect to real-time updates

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL                                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ WebSocket: wss://gateway.mycoop.org/ws/{coop_id}                            │
│ Headers: Authorization: Bearer <jwt>                                        │
│                                                                              │
│ On connect, server sends:                                                   │
│ { "type": "connected", "last_sequence": 12345 }                             │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/websocket.rs                                      │
│                                                                              │
│ pub async fn ws_handler(...)                                                │
│   1. Verify JWT token                                                       │
│   2. require_coop_access(&coop_id)                                          │
│   3. Register connection with EventBroadcaster                              │
│   4. Handle backfill request if client sends last_seen_sequence             │
│   5. Enter event loop: forward events to this connection                    │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ EVENT BROADCASTER                                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/events.rs                                             │
│                                                                              │
│ pub struct EventBroadcaster {                                               │
│   connections: HashMap<CoopId, Vec<WebSocketSender>>,                       │
│   sequence_counter: AtomicU64,                                              │
│ }                                                                            │
│                                                                              │
│ When event occurs (payment, proposal, vote, etc.):                          │
│   1. Wrap in SequencedEvent { sequence, event }                             │
│   2. Store in event log (for backfill)                                      │
│   3. For each connection subscribed to this coop:                           │
│      - Serialize to JSON                                                    │
│      - Send via WebSocket                                                   │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT RECEIVES                                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│ {                                                                            │
│   "sequence": 12346,                                                        │
│   "event": {                                                                │
│     "type": "Payment",                                                      │
│     "payload": { "from": "...", "to": "...", "amount": 2.0, ... }          │
│   }                                                                          │
│ }                                                                            │
│                                                                              │
│ Client SDK:                                                                 │
│   - Detects gap if sequence != last_sequence + 1                            │
│   - Requests backfill: { "type": "backfill", "from_sequence": ... }         │
│   - Calls onEvent callback for each event                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Event types**: `Payment`, `ProposalOpened`, `ProposalClosed`, `VoteCast`, `MemberJoined`, `MemberLeft`, `TrustChanged`, `ComputeTaskCompleted`

---

## 6. Create a Proposal

**User action**: Propose a governance change

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL                                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /gov/domains/{domain_id}/proposals                                     │
│ Authorization: Bearer <jwt>                                                 │
│ {                                                                            │
│   "title": "Increase new member credit limit",                              │
│   "description": "Raise from 50 to 100 hours...",                           │
│   "payload": {                                                              │
│     "type": "ParameterChange",                                              │
│     "parameter": "default_credit_limit",                                    │
│     "new_value": 100                                                        │
│   }                                                                          │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/governance.rs                                     │
│                                                                              │
│ #[post("/domains/{domain_id}/proposals")]                                   │
│ pub async fn create_proposal(...)                                           │
│   1. require_scope("gov:write")                                             │
│   2. Verify proposer is member of domain                                    │
│   3. Verify proposer has "proposer" role (or domain allows all members)     │
│   4. Validate proposal payload                                              │
│   5. Call gov_mgr.create_proposal(...)                                      │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GOVERNANCE MANAGER                                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/governance_mgr.rs                                     │
│                                                                              │
│ pub async fn create_proposal(...)                                           │
│   1. Generate ProposalId                                                    │
│   2. Create Proposal struct with status: Draft                              │
│   3. Store in governance engine                                             │
│   4. Return proposal_id                                                     │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ ICN DAEMON (Governance)                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-governance/src/engine.rs                                          │
│                                                                              │
│ pub fn create_proposal(...)                                                 │
│   1. Validate payload against domain schema                                 │
│   2. Check proposer eligibility                                             │
│   3. Create Proposal {                                                      │
│        id, domain_id, proposer, title, description,                         │
│        payload, status: Draft, created_at, ...                              │
│      }                                                                       │
│   4. Store proposal                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ OPEN PROPOSAL (separate action or automatic)                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /gov/domains/{domain_id}/proposals/{proposal_id}/open                  │
│                                                                              │
│ This:                                                                       │
│   1. Changes status: Draft → Open                                           │
│   2. Sets voting_starts_at, voting_ends_at                                  │
│   3. Broadcasts ProposalOpened event                                        │
│   4. Notifies all domain members                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GOSSIP BROADCAST                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ Topic: "governance:{domain_id}:proposals"                                   │
│ Message: ProposalCreated { proposal, signature }                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ STORAGE WRITES                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. Sled: "gov:domains:{domain_id}:proposals:{proposal_id}" → Proposal       │
│ 2. Sled: "gov:domains:{domain_id}:proposals:index" → Vec<ProposalId>        │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 7. Cast a Vote

**User action**: Vote on an open proposal

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL                                                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ POST /gov/domains/{domain_id}/proposals/{proposal_id}/vote                  │
│ Authorization: Bearer <jwt>                                                 │
│ {                                                                            │
│   "choice": "For",                                                          │
│   "weight": 1,                                                              │
│   "rationale": "This will help new members participate faster"              │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/governance.rs                                     │
│                                                                              │
│ #[post("/domains/{domain_id}/proposals/{proposal_id}/vote")]                │
│ pub async fn cast_vote(...)                                                 │
│   1. require_scope("gov:vote")                                              │
│   2. Verify voter is member of domain                                       │
│   3. Verify proposal is in Open status                                      │
│   4. Verify voting period is active                                         │
│   5. Check voter hasn't already voted (unless change allowed)               │
│   6. Call gov_mgr.cast_vote(...)                                            │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ ICN DAEMON (Governance)                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-governance/src/engine.rs                                          │
│                                                                              │
│ pub fn cast_vote(...)                                                       │
│   1. Calculate voting power based on:                                       │
│      - Base: 1 vote (one-person-one-vote)                                   │
│      - OR: Trust-weighted (if domain configured)                            │
│      - OR: Quadratic (if domain configured)                                 │
│   2. Create Vote { voter, choice, weight, timestamp, signature }            │
│   3. Add to proposal's vote tally                                           │
│   4. Check if quorum reached (trigger early close if threshold met)         │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GOSSIP BROADCAST                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ Topic: "governance:{domain_id}:votes"                                       │
│ Message: VoteCast { proposal_id, voter, choice, signature }                 │
│                                                                              │
│ Note: Actual vote choice may be encrypted until voting closes               │
│       (for anonymous voting domains)                                        │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ STORAGE WRITES                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ 1. Sled: "gov:proposals:{id}:votes:{voter}" → Vote                          │
│ 2. Sled: "gov:proposals:{id}:tally" → VoteTally (updated)                   │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ WEBSOCKET EVENT                                                              │
├─────────────────────────────────────────────────────────────────────────────┤
│ GatewayEvent::VoteCast {                                                    │
│   proposal_id: "...",                                                       │
│   voter: "did:icn:alice...",                                                │
│   // choice may be hidden until close                                       │
│   current_tally: { for: 15, against: 3, abstain: 2 }                        │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 8. Observe Audit Trail

**User action**: View transaction history or governance log

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ CLIENT CALL (Transaction History)                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│ GET /ledger/{coop_id}/history/{did}?limit=50&cursor=abc123                  │
│ Authorization: Bearer <jwt>                                                 │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ GATEWAY ENDPOINT                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│ File: icn-gateway/src/api/ledger.rs                                         │
│                                                                              │
│ #[get("/{coop_id}/history/{did}")]                                          │
│ pub async fn get_history(...)                                               │
│   1. require_scope("ledger:read")                                           │
│   2. require_coop_access(&coop_id)                                          │
│   3. Call ledger_mgr.get_transaction_history(...)                           │
│   4. Return paginated list with Merkle proofs                               │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ LEDGER MANAGER                                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│ Queries:                                                                    │
│   1. Sled: "ledger:{coop_id}:history:{did}" → Vec<EntryHash>                │
│   2. For each hash: "ledger:{coop_id}:entries:{hash}" → LedgerEntry         │
│   3. Optional: Generate Merkle inclusion proof                              │
└─────────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ RESPONSE                                                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│ {                                                                            │
│   "entries": [                                                              │
│     {                                                                        │
│       "hash": "abc123...",                                                  │
│       "type": "transfer",                                                   │
│       "from": "did:icn:alice...",                                           │
│       "to": "did:icn:bob...",                                               │
│       "amount": 2.0,                                                        │
│       "currency": "hours",                                                  │
│       "memo": "Garden help",                                                │
│       "timestamp": "2026-01-23T...",                                        │
│       "dag_parent": "xyz789...",                                            │
│       "merkle_proof": { ... }  // Optional: cryptographic proof             │
│     },                                                                       │
│     ...                                                                      │
│   ],                                                                         │
│   "next_cursor": "def456...",                                               │
│   "has_more": true                                                          │
│ }                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Audit Properties**:

| Property | How It's Achieved |
|----------|-------------------|
| **Immutability** | Merkle-DAG: each entry references parent by hash |
| **Verifiability** | Merkle proofs: any entry can be verified against root |
| **Non-repudiation** | Entries signed by originator's key |
| **Completeness** | DAG structure ensures no entries can be hidden |
| **Transparency** | Any member can request and verify any entry they have access to |

---

## Quick Reference: Endpoint → Crate Mapping

| Endpoint Pattern | Gateway File | Daemon Crate |
|------------------|--------------|--------------|
| `/auth/*` | `api/auth.rs` | `icn-identity` |
| `/identity/*` | `api/identity.rs`, `api/devices.rs` | `icn-identity` |
| `/ledger/{coop_id}/*` | `api/ledger.rs` | `icn-ledger` |
| `/gov/*` | `api/governance.rs` | `icn-governance` |
| `/trust/*` | `api/trust.rs` | `icn-trust` |
| `/compute/*` | `api/compute.rs` | `icn-compute` |
| `/coops/*` | `api/coops.rs`, `api/members.rs` | `icn-coop` |
| `/communities/*` | `api/communities.rs` | `icn-community` |
| `/federation/*` | `api/federation.rs` | `icn-federation` |
| `/ws/{coop_id}` | `api/websocket.rs` | (gateway-only) |

---

## Quick Reference: Gossip Topics

| Topic Pattern | Purpose | Scope |
|---------------|---------|-------|
| `ledger:{coop_id}:entries` | Ledger transactions | CooperativeOnly |
| `ledger:{coop_id}:accounts` | Account changes | CooperativeOnly |
| `governance:{domain_id}:proposals` | Proposal lifecycle | Domain members |
| `governance:{domain_id}:votes` | Vote broadcasts | Domain members |
| `trust:attestations` | Trust graph edges | Global |
| `members:{coop_id}` | Membership changes | CooperativeOnly |
| `identity:devices` | Device registrations | Global |
| `compute:submit` | Task submissions | TrustGated(0.1) |
| `compute:result` | Task results | TrustGated(0.1) |
| `federation:clearing` | Cross-coop settlement | Federation |

---

## Related Documents

- [ICN_COMPLETE_ARCHITECTURE_SYNTHESIS.md](ICN_COMPLETE_ARCHITECTURE_SYNTHESIS.md) - Layer-by-layer architecture
- [ICN_REAL_WORLD_DEPLOYMENT.md](ICN_REAL_WORLD_DEPLOYMENT.md) - Deployment and operations
- [ARCHITECTURE.md](ARCHITECTURE.md) - Technical system design

---

## Document History

- 2026-01-23: Initial creation as developer onboarding reference
