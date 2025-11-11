# ICN Architecture

**Status:** Living Document
**Version:** 0.1.0
**Last Updated:** 2025-11-11

This document captures architectural decisions, design tradeoffs, and the reasoning behind ICNd's implementation.

---

## Table of Contents

1. [Identity & Key Management](#1-identity--key-management)
2. [Trust Graph Model](#2-trust-graph-model)
3. [Network Transport](#3-network-transport)
4. [Ledger Design](#4-ledger-design)
5. [Contract Execution (CCL)](#5-contract-execution-ccl)
6. [Gossip & Synchronization](#6-gossip--synchronization)
7. [Data Storage](#7-data-storage)
8. [Security Model](#8-security-model)
9. [Performance & Scalability](#9-performance--scalability)
10. [Operational Considerations](#10-operational-considerations)

---

## 1. Identity & Key Management

### 1.1 DID Format

**Current:** `did:icn:<base58btc-ed25519-pubkey>`

**Decision: Single canonical public key per identity (v1)**

**Rationale:**
- Simplicity: DID = public key = identity (direct mapping)
- No registry: Self-certifying identifiers
- Verifiable: Any peer can verify signatures without infrastructure
- Compatible with existing Ed25519 tooling

**Tradeoffs:**
- ✅ Simple, auditable
- ✅ No central registry dependency
- ❌ Key compromise = identity loss (mitigated by rotation protocol)
- ❌ No key hierarchy (can add later)

**Future:**
- v2: DID documents with multiple keys (signing, encryption, delegation)
- v3: HD wallet support for key derivation

---

### 1.2 Key Derivation

**Options:**

| Approach | Pros | Cons |
|----------|------|------|
| **Single key** | Simple, clear ownership | No sub-keys, rotation is identity change |
| **HD wallet (BIP32-like)** | Derive unlimited keys from seed | Complexity, seed compromise = total loss |
| **Master + derived** | Balance of simplicity and flexibility | Requires rotation protocol |

**Decision: Single master key + rotation protocol (v1)**

**Rationale:**
- Start simple: one identity = one key
- Implement robust key rotation with signed transition records
- Add HD derivation in v2 when use cases emerge (e.g., per-contract keys)

**Implementation:**
```rust
pub struct KeyRotation {
    old_did: Did,
    new_did: Did,
    timestamp: u64,
    reason: RotationReason, // Scheduled, Compromised, Upgrade
    signature_old: Signature, // Signed by old key
    signature_new: Signature, // Signed by new key
}
```

---

### 1.3 Key Storage

**Options:**

| Approach | Pros | Cons | Use Case |
|----------|------|------|----------|
| **Age-encrypted file** | Simple, portable | No hardware protection | Development, personal nodes |
| **OS keychain** | OS-managed, encrypted at rest | Platform-specific | Desktop apps |
| **Hardware security module** | Strongest protection | Requires hardware, expensive | Production, high-value |
| **TPM chip** | Built into modern hardware | Complex setup | Server deployments |

**Decision: Age-encrypted file (v1), pluggable for HSM/TPM (v2)**

**Rationale:**
- Age encryption is simple, auditable (https://age-encryption.org/)
- Passphrase or YubiKey PIV slot for unlock
- File-based is portable across machines
- Clear migration path to HSM for production

**Implementation:**
```rust
pub trait KeyStore: Send + Sync {
    fn unlock(&mut self, passphrase: &[u8]) -> Result<()>;
    fn lock(&mut self);
    fn is_locked(&self) -> bool;
    fn get_keypair(&self) -> Result<&KeyPair>;
    fn rotate(&mut self, new_keypair: KeyPair) -> Result<KeyRotation>;
}

pub struct AgeKeyStore { /* ... */ }
pub struct HsmKeyStore { /* ... */ }  // Future
```

**Storage path:** `$ICN_DATA_DIR/identity/keypair.age`

---

### 1.4 Multi-Device Identity

**Problem:** Same identity across laptop, phone, server?

**Options:**

| Approach | Pros | Cons |
|----------|------|------|
| **Shared key** | Simple, truly same identity | Dangerous key duplication |
| **Delegate keys** | Separate keys per device, revocable | Requires delegation protocol |
| **Multi-sig** | No single point of failure | Complex, requires quorum |

**Decision: Delegate keys via signed capability grants (v2)**

**Rationale:**
- Primary key signs delegation to device keys
- Each device key has limited capabilities (time-bound, scope-bound)
- Revocation: primary key publishes revocation signed statement
- Preserves single canonical identity while allowing safe multi-device

**Future Implementation:**
```rust
pub struct Delegation {
    primary_did: Did,
    delegate_did: Did,
    capabilities: Vec<Capability>, // Sign contracts, read ledger, etc.
    expires_at: u64,
    signature: Signature, // Signed by primary key
}
```

**v1 compromise:** Manual key export/import with clear warnings

---

## 2. Trust Graph Model

### 2.1 Trust Representation

**Decision: Directed labeled trust edges with evidence chains**

**Graph Structure:**
```rust
pub struct TrustEdge {
    source: Did,        // Who trusts
    target: Did,        // Who is trusted
    labels: Vec<String>, // "partner", "supplier", "validator"
    score: f64,         // 0.0 to 1.0
    evidence: Vec<EvidenceRef>, // Links to contracts, attestations
    expires_at: Option<u64>,
    created_at: u64,
}

pub struct Evidence {
    id: ContentHash,
    kind: EvidenceKind, // ContractFulfilled, Attestation, ThirdPartyVouch
    data: Vec<u8>,
    signatures: Vec<(Did, Signature)>,
}
```

**Rationale:**
- **Directed:** Alice trusts Bob ≠ Bob trusts Alice
- **Labeled:** Context matters ("trust as accountant" ≠ "trust as mechanic")
- **Evidence-based:** Trust isn't arbitrary; it's backed by provable interactions
- **Expiring:** Trust decays without refresh

---

### 2.2 Trust Computation

**Decision: Local computation with transitive trust propagation (PageRank-like)**

**Algorithm (v1):**
```
TrustScore(A → B) =
    DirectTrust(A → B) * 0.7 +
    TransitiveTrust(A → C → B) * 0.3

where:
    DirectTrust = weighted avg of A's edges to B (by recency, evidence strength)
    TransitiveTrust = Σ(TrustScore(A → C) * TrustScore(C → B)) / N
```

**Properties:**
- **Local computation:** Each node computes trust from its perspective
- **Transitive:** "Friend of a trusted friend" has some trust
- **Asymmetric:** Different nodes see different trust scores
- **Attack-resistant:** Sybil nodes have low trust unless vouched by existing trusted nodes

**Trust Classes (operational gates):**
```rust
pub enum TrustClass {
    Isolated,    // Score 0.0-0.1: No interaction
    Known,       // Score 0.1-0.4: Seen, but not trusted
    Partner,     // Score 0.4-0.7: Regular interaction
    Federated,   // Score 0.7-1.0: High trust, extended rights
}
```

**Rationale:**
- Not consensus-based (no global truth)
- Resistant to Sybil: new identities start with zero trust
- Contextual: trust for one purpose doesn't imply trust for all

---

### 2.3 Bootstrap Trust

**Problem:** How does a new node establish initial trust?

**Decision: Manual vouching + invite codes + proof-of-work bootstrap**

**Approaches:**

1. **Manual Introduction (primary):**
   - Existing trusted node creates signed `IntroductionVoucher`
   - New node presents voucher to network
   - Voucher grants initial trust score (e.g., 0.3) from introducer's perspective

2. **Invite Codes (secondary):**
   - Pre-generated by community nodes
   - Limited use (1-5 redemptions)
   - Provides minimal trust (0.1) for initial discovery

3. **Proof-of-Work (fallback):**
   - New node can solve computational puzzle
   - Proves Sybil resistance (cost per identity)
   - Grants minimal trust (0.05) for cold start

**Implementation:**
```rust
pub struct IntroductionVoucher {
    introducer: Did,
    introducee: Did,
    initial_trust: f64,
    message: String, // "Alice from the Brooklyn Cooperative"
    expires_at: u64,
    signature: Signature,
}
```

**Anti-patterns:**
- ❌ Open registration (Sybil vulnerability)
- ❌ Pay-to-join (plutocracy)
- ❌ Global reputation (centralization)

---

### 2.4 Attack Resistance

**Threats & Mitigations:**

| Attack | Mitigation |
|--------|------------|
| **Sybil (fake identities)** | Transitive trust; new DIDs start with zero trust |
| **Eclipse (isolate node)** | Multiple discovery mechanisms (mDNS, rendezvous, manual) |
| **Reputation farming** | Evidence-based trust; contracts must be fulfilled |
| **Trust graph poisoning** | Local computation; no global consensus on trust |
| **Key compromise** | Rotation protocol; evidence chains remain verifiable |

**Monitoring:**
- Nodes log trust score changes
- Rapid trust oscillations flag suspicious behavior
- Community visibility into trust edges (opt-in)

---

## 3. Network Transport

### 3.1 Transport Protocol

**Decision: QUIC with TLS 1.3 mutual authentication**

**Rationale:**
- **QUIC advantages:**
  - Multiplexed streams (no head-of-line blocking)
  - Built-in 0-RTT connection resumption
  - Better congestion control than TCP
  - NAT-friendly (UDP-based)

- **TLS 1.3 mutual auth:**
  - Certificate bound to DID (public key)
  - Mutual authentication (both peers verify)
  - Forward secrecy
  - Standard, audited

**Stack:**
```
Application (CCL, Ledger sync, Gossip)
    ↓
QUIC streams (multiplexed, flow-controlled)
    ↓
TLS 1.3 (encryption, authentication)
    ↓
UDP (transport)
```

**Alternatives considered:**
- ❌ TCP: head-of-line blocking, slower handshake
- ❌ Noise Protocol: great, but QUIC+TLS is more mature
- ⚠️ Hybrid: Could add Noise inside QUIC for additional identity binding (v2)

---

### 3.2 Peer Discovery

**Decision: Multi-layered discovery (LAN + WAN + Manual)**

**Mechanisms:**

| Layer | Protocol | Use Case | Trust |
|-------|----------|----------|-------|
| **LAN** | mDNS | Local network discovery | Low (verify via TLS) |
| **WAN** | Rendezvous servers | Internet-wide bootstrap | Medium (signed peer lists) |
| **Manual** | Config file / CLI | Explicit peering | High (admin intent) |
| **Gossip** | Peer exchange (PEX) | Network expansion | Low (verify via trust graph) |

**mDNS (LAN):**
```
Service: _icn._udp.local
TXT records:
  did=did:icn:z...
  version=0.1.0
  capabilities=ledger,contracts
  port=4433
```

**Rendezvous (WAN):**
- Operated by community (anyone can run one)
- Return signed peer list with timestamps
- Clients verify signatures against known rendezvous DID keys
- No single point of failure (multiple rendezvous in config)

**Implementation:**
```rust
pub struct PeerInfo {
    did: Did,
    addrs: Vec<SocketAddr>,
    capabilities: HashSet<String>,
    last_seen: u64,
}

pub enum DiscoverySource {
    Mdns,
    Rendezvous(Did),
    Manual,
    Gossip(Did), // Learned from peer X
}
```

---

### 3.3 NAT Traversal

**Decision: Hole punching + relay fallback**

**Approach:**

1. **Direct connection (best case):**
   - Both peers have public IPs or are LAN-local
   - Direct QUIC connection

2. **Hole punching (common case):**
   - Use STUN-like protocol to discover public IP/port
   - Coordinate simultaneous UDP send (punch through NAT)
   - ~80% success rate

3. **Relay (fallback):**
   - Community-run relay nodes
   - Encrypt end-to-end; relay only forwards packets
   - Relay nodes selected from high-trust peers
   - Temporary (establish, then try hole punch in background)

**Relay incentives:**
- Relay operators gain trust score
- Nodes can donate bandwidth to relay pool
- Relay usage is temporary (encourage direct connections)

**Implementation:**
```rust
pub struct ConnectionPath {
    peer: Did,
    path: ConnectionType,
}

pub enum ConnectionType {
    Direct(SocketAddr),
    Relayed { via: Did, relay_addr: SocketAddr },
    Failed(String),
}
```

---

### 3.4 Connection Management

**Decision: Trust-gated connection limits with backpressure**

**Limits (per node):**
```rust
pub struct ConnectionLimits {
    max_total: usize,      // 500 (adjustable)
    max_per_trust_class: HashMap<TrustClass, usize>,
    // Isolated: 10, Known: 50, Partner: 200, Federated: 240

    max_streams_per_peer: usize, // 32
    max_inflight_bytes: usize,   // 10 MB per peer
}
```

**Rationale:**
- Prevent resource exhaustion
- Prioritize trusted peers
- Shed load under attack (drop Isolated/Known first)
- Backpressure propagates to application layer

**Health checks:**
- Periodic ping/pong (30s interval)
- Idle timeout (5 min for Isolated, 30 min for Partners)
- Detect dead connections, free resources

---

## 4. Ledger Design

### 4.1 Data Model

**Decision: Double-entry append-only ledger with Merkle-DAG structure**

**Why double-entry?**
- Conservation law: Σ debits = Σ credits (every transaction)
- Mutual credit requires balanced books
- Auditable: any peer can verify invariants

**Why Merkle-DAG?**
- Content-addressable: each entry has unique hash
- Tamper-evident: changing history breaks hashes
- Forkable: offline work creates branches, merge later
- Enables efficient sync (exchange hashes, fetch missing)

**Structure:**
```rust
pub struct JournalEntry {
    id: ContentHash,          // SHA-256 of canonical serialization
    timestamp: u64,           // Local timestamp (not consensus)
    author: Did,              // Who created this entry
    contract_ref: Option<ContentHash>, // Which contract authorized this

    accounts: Vec<AccountDelta>,

    parents: Vec<ContentHash>, // Previous entries (Merkle-DAG links)
    signature: Signature,      // Signed by author
}

pub struct AccountDelta {
    account_id: Did,          // Could be person, organization, resource
    currency: String,         // "USD", "hours", "kwh"
    debit: Option<i64>,       // Positive values
    credit: Option<i64>,      // Positive values
}

// Invariant enforced at creation:
// Σ debits == Σ credits (per currency)
```

**Example Transaction:**
```
Alice delivers 10 hours of web design to Bob's coop.

Entry:
  debits:  Alice/hours: 10
  credits: BobCoop/hours: 10

Alice's balance: +10 hours (owed to her)
BobCoop's balance: -10 hours (owes)
```

---

### 4.2 Conflict Resolution

**Problem:** Two nodes create conflicting entries while offline.

**Decision: Deterministic merge with constraint checking**

**Algorithm:**

1. **Detect fork:** Entry has multiple children for same parent
2. **Order canonically:** Sort by `(timestamp, author_did, entry_id)`
3. **Replay both branches:**
   - Compute balance after each branch
   - Check invariants (no overdraft beyond credit limits)
4. **Merge or quarantine:**
   - If both branches valid: merge (both applied)
   - If one invalid: discard invalid branch
   - If both invalid: quarantine, require manual resolution

**Contract-level rules:**
- Contracts can specify stricter merge rules
- E.g., "only one party can create entries" (no conflict possible)
- E.g., "sequential numbering required" (detect gaps)

**Quarantine:**
```rust
pub struct QuarantinedEntry {
    entry: JournalEntry,
    reason: QuarantineReason,
    conflicts_with: Vec<ContentHash>,
    resolution: Option<Resolution>, // Human or contract decision
}

pub enum QuarantineReason {
    InvariantViolation(String), // Overdraft, double-spend
    ConflictingTimestamp,
    InvalidSignature,
}
```

**Rationale:**
- Deterministic: all nodes reach same conclusion
- Conservative: preserve data, don't auto-delete
- Auditable: history of conflicts and resolutions

---

### 4.3 Currency Model

**Decision: Multi-currency with per-contract currency definitions (v1)**

**Rationale:**
- Different cooperatives use different units
  - Time banks: hours
  - Energy coops: kWh
  - Fiat-backed: USD/EUR
  - Resource credits: compute, storage

- Currency is just a string label
- No built-in exchange rates (contracts handle that)
- No global supply (each contract defines issuance)

**Implementation:**
```rust
pub struct Currency {
    symbol: String,       // "hours", "USD", "kwh"
    decimals: u8,         // Precision (e.g., 2 for cents)
    issuer: Option<Did>,  // Who can issue? None = mutual credit
}
```

**Mutual credit currencies:**
- No central issuer
- Created by transaction (balanced debit/credit)
- Can have credit limits per participant

**Asset-backed currencies:**
- Issuer holds reserves
- Can redeem for external asset
- Requires trusted issuer

**v2: Multi-issuer currencies with attestations**

---

### 4.4 Credit Limits

**Decision: Per-participant, per-currency limits set by contract**

**Mechanism:**
```rust
pub struct CreditLimit {
    participant: Did,
    currency: String,
    max_negative_balance: i64, // How much can they owe?
    max_positive_balance: i64, // How much can be owed to them?
    set_by: Did,               // Who set this limit?
    effective_date: u64,
}
```

**Who sets limits?**
- **Mutual agreement:** Both parties sign
- **Contract rules:** Automatic based on participation
- **Community vote:** Governance process

**Dynamic adjustment:**
- Limits can increase with trust score
- Proven track record → higher credit line
- Contract can automate: "10% increase per fulfilled contract"

**Default limits (conservative):**
- New participants: ±100 units
- Increases require explicit action

**Rationale:**
- Prevent runaway debt
- Encourage accountability
- Reflects real-world credit relationships

---

### 4.5 Privacy

**Decision: Semi-private by default, opt-in transparency (v1)**

**Visibility levels:**

| Data | Who can see? | Rationale |
|------|-------------|-----------|
| **Account balances** | Account owner + contract participants | Privacy |
| **Transaction amounts** | Transaction parties + auditors (if designated) | Selective disclosure |
| **Transaction existence** | Contract participants | Provable participation |
| **Merkle roots** | Public | Integrity verification |

**v2: Zero-knowledge proofs**
- Prove balance constraints without revealing amount
- Prove transaction validity without revealing parties
- Requires additional crypto (zk-SNARKs or Bulletproofs)

**Auditor role:**
- Contracts can designate auditors
- Auditors receive encrypted ledger entries
- Can verify compliance without full visibility to everyone

---

## 5. Contract Execution (CCL)

### 5.1 Language Design

**Decision: Domain-specific language (DSL) for v1, WASM for v2**

**v1: CCL DSL (deterministic interpreter)**

**Goals:**
- Express cooperative agreements
- Safe (no arbitrary code execution)
- Deterministic (same inputs → same outputs)
- Auditable (human-readable)

**Features:**
```
contract TimeBank {
  participants: Set<Did>
  currency: "hours"

  rule credit_limit {
    for p in participants:
      limit(p, "hours", -100, +100)
  }

  rule record_service {
    require: sender in participants
    require: recipient in participants
    require: hours > 0

    ledger.transfer(recipient, sender, hours, "hours")
  }

  trigger monthly_report {
    schedule: "0 0 1 * *"  // Cron-like
    action: generate_report()
  }
}
```

**Properties:**
- **Not Turing-complete** (no infinite loops)
- **Bounded execution** (fuel metering)
- **Capability-based** (explicit permissions)
- **Versioned** (contracts include version, can upgrade)

**v2: WASM sandbox**
- Compile Rust/AssemblyScript to WASM
- Run in `wasmtime` with strict gas limits
- Capability injection (contracts request permissions)
- Better for complex logic

---

### 5.2 Determinism

**Critical:** Same contract + same inputs = same outputs on all nodes

**Guarantees:**

| Aspect | Approach |
|--------|----------|
| **Time** | No `now()` access; timestamp passed as input |
| **Randomness** | Deterministic PRNG seeded by contract hash |
| **Ordering** | Inputs sorted canonically before execution |
| **Floating point** | Use fixed-point math (i64 with decimals) |
| **External data** | No network access; oracle data passed as signed inputs |

**Testing:**
- Fuzzing with proptest
- Differential testing (run on multiple platforms, compare outputs)
- Golden files for regression

---

### 5.3 Capabilities

**Decision: Explicit capability model (principle of least privilege)**

**Capabilities:**
```rust
pub enum Capability {
    ReadLedger { accounts: Vec<Did> },
    WriteLedger { accounts: Vec<Did> },
    SendMessage { to: Did },
    ReadState { keys: Vec<String> },
    WriteState { keys: Vec<String> },
    CreateSubContract,
    InvokeContract { contract: ContentHash },
}
```

**Grant mechanism:**
```rust
pub struct ContractInstallation {
    code_hash: ContentHash,
    installed_by: Did,
    capabilities: Vec<Capability>,
    participants: Vec<Did>,
    signatures: Vec<(Did, Signature)>, // All participants must sign
}
```

**Rationale:**
- Participants know exactly what a contract can do
- Sandboxing: contracts can't access data they shouldn't
- Auditable: review capabilities before signing

---

### 5.4 Upgradability

**Decision: Explicit migration with participant consent**

**Process:**
1. Propose new contract version (code_hash_v2)
2. Include migration function: `migrate(old_state) -> new_state`
3. Participants review and sign
4. On consensus: run migration, switch to new version

**Safety:**
- Old contract remains in history (auditable)
- Migration is deterministic (all nodes get same result)
- Rollback: re-instantiate old contract with migrated state

**Implementation:**
```rust
pub struct ContractUpgrade {
    old_version: ContentHash,
    new_version: ContentHash,
    migration: MigrationCode,
    proposed_by: Did,
    approvals: Vec<(Did, Signature)>,
    threshold: usize, // e.g., 2/3 of participants
}
```

**Emergency: Security upgrade without consensus?**
- Risky; prefer participant coordination
- Could allow "security council" capability (opt-in governance)

---

## 6. Gossip & Synchronization

### 6.1 Consistency Model

**Decision: Causal consistency with anti-entropy**

**Properties:**
- **Causal:** If A caused B, all nodes see A before B
- **Eventual:** All nodes converge (no partition forever)
- **Local-first:** Nodes apply changes immediately, sync later

**Not:** Strong consistency (would require consensus, kills availability)

**Mechanism: Vector clocks**
```rust
pub struct VectorClock {
    clock: HashMap<Did, u64>, // Node → sequence number
}

impl VectorClock {
    pub fn happened_before(&self, other: &VectorClock) -> bool {
        // Returns true if self causally precedes other
    }
}
```

**Causality tracking:**
- Each entry includes vector clock
- Nodes merge clocks on sync
- Detect conflicts (concurrent entries)

---

### 6.2 Sync Protocol

**Decision: Hybrid push/pull with bloom filters**

**Protocol:**

1. **Announce (push):**
   - Node creates new entry → send announcement to connected peers
   - Announcement = (entry_hash, vector_clock, author)

2. **Request (pull):**
   - Peer checks: do I have this entry?
   - If not: request full entry

3. **Anti-entropy (periodic):**
   - Exchange bloom filters of recent entries
   - Identify missing entries
   - Fetch missing

**Bloom filters:**
- Compact representation of "entries I have"
- Low false-positive rate
- Efficient sync (don't re-send known entries)

**Rate limiting:**
- Per-peer token bucket
- Prioritize entries from high-trust peers
- Drop announcements under load (fall back to anti-entropy)

---

### 6.3 Topic Model

**Decision: Scoped gossip channels with ACLs**

**Topics:**
```rust
pub struct Topic {
    name: String,              // "global:identity", "contract:abc123"
    acl: AccessControl,        // Who can publish/subscribe?
    retention: Duration,       // How long to keep entries?
    bloom_filter: BloomFilter, // For anti-entropy
}

pub enum AccessControl {
    Public,                    // Anyone (e.g., identity announcements)
    TrustClass(TrustClass),    // Only Partners+
    Participants(Vec<Did>),    // Contract-specific
}
```

**Standard topics:**
- `global:identity` - DID announcements, key rotations
- `global:rendezvous` - Peer discovery hints
- `contract:{hash}` - Per-contract messages
- `ledger:{currency}` - Per-currency ledger sync

**Rationale:**
- Not everything needs global broadcast
- Scoped topics reduce bandwidth
- ACLs prevent spam

---

### 6.4 Bandwidth Management

**Decision: Adaptive rate limiting with QoS**

**Strategy:**

1. **Measure available bandwidth** (periodic speed test to trusted peer)
2. **Allocate budgets:**
   ```
   Critical (identity, ledger): 40%
   High (contracts): 30%
   Normal (gossip): 20%
   Low (discovery): 10%
   ```
3. **Drop low-priority under congestion**
4. **Backpressure to application** (slow down contract execution if can't sync)

**Per-peer limits:**
- Trust-weighted: Federated peers get more bandwidth
- Fairness: No single peer starves others

---

### 6.5 Network Protocol Bridge

**Decision: Length-prefixed bincode over QUIC streams**

**Protocol:**

**Wire format:**
```
[4 bytes: length (big-endian)] [N bytes: bincode-serialized NetworkMessage]
```

**NetworkMessage envelope:**
```rust
pub struct NetworkMessage {
    version: u32,           // Protocol version (current: 1)
    from: Did,              // Source DID
    to: Option<Did>,        // Destination (None = broadcast)
    payload: MessagePayload,
}

pub enum MessagePayload {
    Gossip(GossipMessage),  // Wrapped gossip protocol
    Ping,                   // Keepalive
    Pong,                   // Response to ping
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
    SubscribeAck { topics: Vec<String> },
}
```

**Rationale:**
- **Length-prefixed:** Handles variable-size messages efficiently
- **Bincode:** Fast, compact serialization (5-10% overhead)
- **DID routing:** Enables unicast and broadcast patterns
- **Versioned:** Forward compatibility for protocol evolution
- **Simple:** No complex framing, easy to implement

**Message flow:**

1. **Publishing:**
   ```
   Ledger → GossipActor.publish() → (in-process only in v1)
   ```
   *Network publishing deferred to Phase 7*

2. **Reception:**
   ```
   QUIC connection → NetworkActor.handle_incoming_connections()
   → read_message() → IncomingMessageHandler callback
   → Supervisor extracts GossipMessage
   → GossipActor.handle_message() → process/store
   ```

3. **Anti-entropy:**
   ```
   Background task (30s interval)
   → NetworkHandle.broadcast(RequestBloomFilter)
   → All connected peers receive
   → (Response handling deferred to Phase 7)
   ```

**Implementation details:**

- **NetworkActor extensions:**
  - `send_message(did, message)`: Unicast to specific peer
  - `broadcast(message)`: Multicast to all connected peers
  - `handle_incoming_connections()`: Background acceptor task
  - `handle_connection()`: Per-connection stream processor

- **Gossip routing:**
  - Supervisor creates `IncomingMessageHandler` callback
  - Callback extracts `GossipMessage` from `NetworkMessage`
  - Routes to `GossipActor.handle_message()`

- **Limitations (v1):**
  - Push-only: Can announce entries, cannot request
  - No request/response correlation
  - Broadcast-only anti-entropy (O(n) messages)
  - No topic-based routing

**Performance:**
- Message overhead: ~100 bytes + payload
- Single send latency: 1-2ms (local network)
- Broadcast to 10 peers: 10-20ms
- Max message size: 10MB

**Future enhancements (Phase 7):**
- Complete pull protocol (Request → Response)
- Topic subscriptions (filter by interest)
- Smart peer selection (probabilistic gossip)
- Message batching (multiple per stream)

---

## 7. Data Storage

### 7.1 Storage Backend

**Decision: Pluggable trait with Sled default (v1)**

**Trait:**
```rust
pub trait Store: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;
    fn batch_write(&self, ops: Vec<WriteOp>) -> Result<()>;
}
```

**Implementations:**
- **Sled** (v1): Embedded, pure Rust, transactional
- **RocksDB** (v2): More mature, faster, C++ dependency
- **SQLite** (future): If we need relational queries

**Namespaces:**
```
identity/
  keypair           # Encrypted key
  rotations/        # Key rotation history

trust/
  edges/            # Trust graph edges
  evidence/         # Evidence records

ledger/
  journal/          # All entries (content-addressed)
  balances/         # Cached balances per account
  checkpoints/      # Merkle roots

contracts/
  installed/        # Contract code
  state/            # Contract storage

peers/
  known/            # Discovered peers
  sessions/         # Active connections

config/
  node              # Node config
```

---

### 7.2 Schema Evolution

**Decision: Versioned schemas with migration path**

**Approach:**
```rust
pub struct SchemaVersion {
    version: u32,
    migrate: fn(&Store) -> Result<()>,
}

// On startup:
fn ensure_schema() {
    let current = store.get(b"schema/version")?;
    match current {
        Some(v) if v == LATEST_VERSION => return Ok(()),
        Some(v) => run_migrations(v, LATEST_VERSION)?,
        None => initialize_schema()?,
    }
}
```

**Migration strategy:**
- Backward-compatible when possible
- Breaking changes: copy data to new namespace
- Keep old data until migration verified

**Backup before migration:**
```bash
icnctl backup create pre-migration-v2
```

---

### 7.3 Pruning & Archival

**Decision: Configurable retention with archive export**

**Retention policies:**
```rust
pub struct RetentionPolicy {
    keep_last_entries: usize,     // e.g., 10,000
    keep_duration: Duration,      // e.g., 1 year
    archive_path: Option<PathBuf>,
}
```

**What to prune:**
- Old journal entries (keep Merkle roots)
- Expired trust edges
- Revoked contracts

**What to keep:**
- Active account balances
- Current contract state
- Recent entries (within retention window)

**Archive format:**
- JSON-LD for interop
- Signed by node (provenance)
- Can re-import if needed

---

## 8. Security Model

### 8.1 Threat Model

**Assumptions:**
- **Adversary can:**
  - Create unlimited Sybil identities (but no initial trust)
  - Control network (MITM, partition, delay)
  - Compromise individual nodes
  - Collude with other adversaries

- **Adversary cannot:**
  - Break Ed25519 cryptography
  - Forge signatures
  - Compromise all nodes simultaneously

**Out of scope (v1):**
- Quantum attacks (post-quantum crypto in v2)
- Side-channel attacks (constant-time crypto)
- Physical access (rely on OS security)

---

### 8.2 Attack Surface

**External:**
- Network (QUIC/TLS) - mitigated by mutual auth, rate limits
- RPC (gRPC) - mitigated by auth, capability checks
- Discovery (mDNS, rendezvous) - mitigated by verification

**Internal:**
- Contract execution - mitigated by sandboxing, fuel limits
- Ledger sync - mitigated by signature checks, invariants
- Storage - mitigated by encryption at rest

**Supply chain:**
- Dependencies - use `cargo audit`, lock file
- Build - reproducible builds (future)
- Distribution - signed releases (cosign)

---

### 8.3 Incident Response

**Process:**
1. **Detect:** Monitoring alerts (unusual trust changes, ledger anomalies)
2. **Contain:** Quarantine affected entries, disconnect malicious peers
3. **Analyze:** Review logs, ledger history
4. **Remediate:** Deploy fix, coordinate with network
5. **Disclose:** Public postmortem (if not exploitable)

**Security contacts:**
- security@intercooperative.network (to be set up)
- Encrypted reporting (PGP)

---

## 9. Performance & Scalability

### 9.1 Target Metrics (v1)

| Metric | Target | Rationale |
|--------|--------|-----------|
| Ledger write latency | <100ms | Interactive UX |
| Ledger sync latency | <1s (LAN), <5s (WAN) | Reasonable propagation |
| Contract execution | <50ms | Keep UI responsive |
| Peer connections | 500 concurrent | Medium-sized cooperative |
| Throughput | 100 tx/sec per node | Not high-frequency trading |

**Not optimizing for:**
- Millions of nodes (v1 is cooperative-scale: 100s-1000s)
- High-frequency trading (use CEX)
- Massive contracts (keep contracts focused)

---

### 9.2 Bottlenecks

**Known:**
- Signature verification (CPU-bound) - mitigate with batching
- Ledger sync (network-bound) - mitigate with compression
- Contract execution (interpreter overhead) - v2: WASM is faster

**Monitoring:**
- Prometheus metrics
- Distributed tracing (OpenTelemetry)
- Profiling (perf, flamegraph)

---

### 9.3 Scaling Strategy

**Vertical:**
- Better hardware (more cores, faster disk)
- Tuning (tokio threads, buffer sizes)

**Horizontal:**
- Sharding (per-contract nodes)
- Specialized nodes (relay, rendezvous, archival)

**Not:**
- Global sharding (breaks local-first model)
- Consensus (kills availability)

---

## 10. Operational Considerations

### 10.1 Deployment

**Platforms:**
- **Linux** (primary): systemd service, Docker
- **macOS** (dev): launchd, native binary
- **Windows** (future): Windows Service, WSL2

**Packaging:**
- Debian/Ubuntu: .deb
- Fedora/RHEL: .rpm
- Arch: AUR package
- Nix: flake (deterministic)

**Docker:**
```dockerfile
FROM rust:1.75 AS builder
# Build...

FROM debian:bookworm-slim
COPY --from=builder /app/icnd /usr/local/bin/
ENTRYPOINT ["icnd"]
```

---

### 10.2 Monitoring

**Observability stack:**
- **Logs:** Structured (JSON) to stdout, rotate with logrotate
- **Metrics:** Prometheus exporter on `:9090/metrics`
- **Tracing:** OpenTelemetry (optional export to Jaeger)
- **Health:** `/healthz` endpoint (OK/DEGRADED)

**Key metrics:**
```
icn_peers_connected{trust_class}
icn_ledger_entries_total
icn_ledger_sync_latency_seconds
icn_contract_executions_total{status}
icn_trust_score_changes_total
```

**Alerting:**
- No peers connected (isolation)
- Ledger sync stalled
- Disk space low
- High error rate

---

### 10.3 Backup & Disaster Recovery

**Backup:**
```bash
icnctl backup create --output backup-$(date +%Y%m%d).tar.gz
```

**Contents:**
- Identity keys (encrypted)
- Ledger (all entries)
- Contract state
- Trust graph
- Config

**Restore:**
```bash
icnctl backup restore backup-20251110.tar.gz
```

**Disaster recovery:**
- Identity key lost → use recovery key (created at setup)
- Ledger corrupted → restore from backup + re-sync
- Node destroyed → restore on new machine, peers recognize DID

---

### 10.4 Upgrades

**Process:**
1. Release new version (GitHub releases)
2. Announce in community channels
3. Node operators pull new binary
4. Restart daemon (graceful shutdown)
5. Migration runs on startup (if needed)

**Backward compatibility:**
- Protocol versioned (peers negotiate)
- Old nodes can talk to new nodes (within major version)

**Breaking changes:**
- Major version bump
- Coordination period (e.g., "upgrade by Jan 1")

---

## Appendix

### A. Glossary

- **DID:** Decentralized Identifier
- **CCL:** Cooperative Contract Language
- **DAG:** Directed Acyclic Graph
- **NAT:** Network Address Translation
- **TLS:** Transport Layer Security
- **QUIC:** Quick UDP Internet Connections
- **HSM:** Hardware Security Module

### B. References

- **DIDs:** https://www.w3.org/TR/did-core/
- **Ed25519:** https://ed25519.cr.yp.to/
- **QUIC:** https://datatracker.ietf.org/doc/html/rfc9000
- **Age encryption:** https://age-encryption.org/
- **Mutual credit:** https://www.mutual-credit.org/

### C. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-11-10 | Tokio runtime | Ecosystem maturity |
| 2025-11-10 | Ed25519 keys | Standard, audited |
| 2025-11-10 | QUIC transport | Modern, multiplexed |
| 2025-11-10 | Double-entry ledger | Cooperative finance model |
| 2025-11-10 | Trust-gated everything | Security through relationships |

---

**Document status:** Living - expect updates as we implement and learn.
