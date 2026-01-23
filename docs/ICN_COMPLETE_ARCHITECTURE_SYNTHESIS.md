# ICN: Complete Architecture Synthesis

**Last Updated**: 2026-01-23
**Status**: Reference Document
**Authors**: Claude Code, fahertym

---

## The Mission

ICN is infrastructure for a **parallel political economy** where democratic organizations outcompete capitalism through better coordination, not ideology.

**Why capitalism wins**: Fast decisions, easy capital, trivial federation via contracts, price signals aggregate information.

**Why cooperatives lose**: High coordination costs, slow capital formation, bespoke inter-org relationships, siloed information.

**ICN's answer**: Protocol-level infrastructure that makes cooperation cheaper than capitalism.

---

## Implementation Status

Features throughout this document are marked with status indicators:

| Symbol | Meaning |
|--------|---------|
| ✅ | **Implemented** - Merged, tested, deployed |
| 🚧 | **Partial** - Core functionality exists, edge cases pending |
| 📋 | **Designed** - Architecture defined, implementation planned |
| 🔮 | **Future** - Conceptual, not yet designed in detail |

**Current overall status**: ~75% implemented (272K LOC, 2,287 tests across 27 crates)

---

## Invariants (Non-Negotiables)

These are the protocol's hard constraints. They prevent scope drift and define what ICN *is*.

| # | Invariant | Why It Matters |
|---|-----------|----------------|
| 1 | **No global chain** | Eventual consistency via gossip + Merkle-DAG auditability. No single ledger to capture. |
| 2 | **Entity-scoped governance** | No "world government" baked in. Recursion, not centralization. Each entity controls itself. |
| 3 | **Trust gates resources** | Credit, compute, federation routing, rate limits—all flow through the trust graph. |
| 4 | **Deterministic contracts** | Bounded execution, capability-based permissions. No Turing-completeness, no oracles. |
| 5 | **Members own keys** | Hardware + keys belong to individuals. Gateways are infrastructure, not custodians. |
| 6 | **Zero-sum mutual credit** | Every credit creates a matching debit. No monetary expansion without real contribution. |
| 7 | **Portable identity** | DIDs work across cooperatives. Credentials travel; you don't start from zero. |
| 8 | **Auditable by default** | Merkle-DAG ledger, signed messages, governance logs—full traceability without surveillance. |

---

## The Entity Model

**Recursive self-similar structure** at every scale:

```
Individual (did:icn:...)
    ↓ joins
Cooperative/Community (entity:icn:coop:... / entity:icn:community:...)
    ↓ federates
Federation (entity:icn:federation:...)
    ↓ federates
Meta-Federation
    ↓ ultimately governed by
Protocol Commons (ICN itself)
```

**Key insight**: ICN governs itself using ICN. The "Protocol Commons" is the top-level entity that democratically governs protocol parameters.

---

## Identity → Membership → Authorization

**The handshake from "I have a DID" to "I can do things":**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  1. IDENTITY: Create DID                                                     │
│     did:icn:<base58-ed25519-pubkey>                                         │
│     ├── Generate keypair (Secure Enclave / hardware)                        │
│     └── Optionally: complete VUI ceremony for proof-of-personhood           │
└───────────────────────────────────────┬─────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  2. MEMBERSHIP: Obtain Verifiable Credential (VC)                            │
│     Issued by: entity (coop, community, federation)                         │
│     ├── VC contains: issuer, subject (your DID), roles, expiry              │
│     ├── Signed by entity's keypair                                          │
│     └── Stored in wallet, presented on demand                               │
└───────────────────────────────────────┬─────────────────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  3. AUTHORIZATION: Gateway checks at every request                           │
│     ├── DID Auth: Challenge/response proves you control the key             │
│     ├── Membership VC: Proves you belong to this entity                     │
│     ├── Capability scope: VC specifies what roles you have                  │
│     └── Trust threshold: Your trust score gates resource limits             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Roles/Capabilities in Membership VC**:

| Role | Capabilities Granted |
|------|---------------------|
| `member` | View balances, make payments, vote on proposals |
| `proposer` | Create governance proposals |
| `moderator` | Approve/reject pending members, manage listings |
| `treasurer` | Budget operations, credit limit adjustments |
| `steward` | VUI ceremonies, key recovery participation |
| `admin` | Full entity management, role assignment |

**Why this matters**: The entity doesn't become a mini-state because:
1. **You own your keys** - the entity can revoke membership VC, not your identity
2. **Credentials are portable** - other entities can accept your membership proof
3. **Trust is earned** - roles give permissions, but trust gates actual resource access
4. **Appeals exist** - governance can reverse role decisions

---

## Identity Lifecycle

**Key Rotation** (old keys deprecated, new keys linked):
```
1. Generate new keypair
2. Create KeyRotation message signed by OLD key:
   { old_did, new_did, timestamp, reason }
3. Publish to identity gossip topic
4. Grace period: both keys valid for 7 days
5. After grace: old key rejected, new key canonical
6. Membership VCs re-issued to new DID by entities
```

**Device Revocation** (entity revokes device, not person):
```
1. User reports device lost/stolen via another device
2. Entity revokes device credential (not membership)
3. Device credential added to revocation list
4. Gateway rejects auth from revoked device
5. User registers replacement device
6. Membership VC remains valid (tied to person, not device)
```

**Membership Revocation** (entity removes member):
```
1. Governance proposal or admin action
2. Membership VC added to revocation accumulator
3. Member can no longer prove non-revocation
4. Gateway rejects membership proofs
5. Trust graph attestations decay
6. Appeal process available via governance
```

**Compromise Handling** (phone stolen and unlocked):
```
1. IMMEDIATE: Report via any trusted channel
   - Another device, trusted contact, direct to steward
2. Entity revokes all device credentials for that person
3. If VUI compromised: steward network initiates recovery
   - Threshold key shares reconstruct identity anchor
   - New keypair issued after verification ceremony
4. Old DID marked compromised, new DID linked via recovery proof
5. Membership VCs re-issued to new DID
```

---

## Layer-by-Layer Architecture

### Layer 0: Identity (icn-identity, icn-steward, icn-zkp) ✅

**DIDs**: `did:icn:<base58-ed25519-pubkey>`

**SDIS (Self-Sovereign Distributed Identity System)**:

| Level | Purpose | Implementation |
|-------|---------|----------------|
| L0 | Personhood Anchor | VUI via threshold PRF, steward network |
| L1 | Digital Citizenship | Commons membership credentials |
| L2 | Attribute Proofs | Age, location, capability attestations |
| L3 | Institutional Seals | Cooperative endorsements |

**Steward Network** (icn-steward):
- Distributed key generation
- VUI (Verifiable Unique Identity) computation
- Enrollment ceremonies for proof-of-personhood
- Key recovery via threshold shares

**Zero-Knowledge Proofs** (icn-zkp):
- Age proofs (prove >18 without revealing DOB)
- Membership proofs (prove membership without revealing identity)
- Non-revocation proofs (prove credential not revoked)
- RSA and Merkle accumulators for efficient verification

**Threats**:
- **Sybil**: Fake identities → mitigated by VUI (proof-of-personhood via steward ceremonies)
- **Collusion**: Stewards conspire → threshold signatures require k-of-n, steward reputation tracking
- **Coercion**: Forced enrollment → ceremony requires in-person verification, abuse reporting channel

---

### Layer 1: Trust (icn-trust) ✅

**Web-of-participation** with transitive computation:

| Trust Class | Score | Capabilities |
|-------------|-------|-------------|
| Isolated | < 0.1 | Receive only |
| Known | 0.1-0.4 | Basic exchange |
| Partner | 0.4-0.7 | Full participation |
| Federated | 0.7+ | Cross-federation operations |

**Multi-Graph Trust**: Separate graphs for contexts:
- `financial` - Credit relationships
- `governance` - Voting weight
- `identity` - Vouching
- `network` - Connection reliability

**Threats**:
- **Spam attestations**: Inflate trust artificially → rate limiting, attestation cost, decay over time
- **Reputation gaming**: Strategic vouching rings → transitive trust decay, outlier detection
- **Trust graph poisoning**: Coordinated bad actors → cross-context verification, slow trust accrual

---

### Layer 2: Cooperative Contract Language (icn-ccl) ✅

**Purpose**: Express cooperative agreements in deterministic, auditable code.

**Design Principles**:
1. **Deterministic** - Same inputs always produce same outputs
2. **Capability-based** - Explicit permissions required
3. **Fuel-metered** - Bounded execution, no infinite loops
4. **Auditable** - Full traceability

**Contract Structure**:
```rust
Contract {
    name: String,                    // e.g., "MembershipAgreement"
    participants: Vec<Did>,          // 1-100 participants
    currency: Option<String>,        // e.g., "hours"
    state_vars: Vec<StateVar>,       // Max 100 state variables
    rules: Vec<Rule>,                // Max 50 rules (functions)
    triggers: Vec<Trigger>,          // Scheduled actions
}
```

**Capabilities** (explicit permissions):

| Capability | Allows |
|------------|--------|
| `ReadLedger` | Query balances |
| `WriteLedger` | Execute transfers |
| `WriteState` | Persist contract state |
| `ReadTrust` | Query trust scores |
| `GovernancePropose` | Create proposals |

**Safety Bounds**:
- Max 5 loop nesting depth
- Max 50 expression depth
- Max 100 participants per contract
- Max 100 state variables
- Max 50 rules
- Reserved keywords protected

**Statements** (Stmt enum):
```rust
Assign { name, value }           // Local variable
SetState { key, value }          // Persist to storage
LedgerTransfer { from, to, amount, currency }
SetCreditLimit { account, currency, limit }
If { condition, then_block, else_block }
For { var, iterable, body }
Return { value }
```

**Threats**:
- **Resource exhaustion**: Infinite loops → fuel metering, bounded execution
- **Capability escalation**: Unauthorized actions → explicit capability grants, auditable permissions
- **State manipulation**: Corrupted contract state → signed state transitions, Merkle proofs

---

### Layer 3: Distributed Compute (icn-compute) ✅

**The most sophisticated layer** - trust-gated task distribution with BFT verification.

**Architecture**:
```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Submitter  │────▶│   Gossip    │────▶│  Executor   │
│  (Task)     │     │  (Routing)  │     │  (Result)   │
└─────────────┘     └─────────────┘     └─────────────┘
      │                   │                   │
      └───────────────────┴───────────────────┘
                    Trust Graph
                (Access Control)
```

**Trust Thresholds**:
```rust
MIN_TRUST_SUBMIT: 0.1   // Minimum trust to submit tasks
MIN_TRUST_EXECUTE: 0.3  // Minimum trust to execute tasks
```

**Gossip Topics**:

| Topic | Purpose |
|-------|---------|
| `compute:submit` | Task submission |
| `compute:claim` | Executor claims task |
| `compute:result` | Execution results |
| `compute:cancel` | Cancellation |
| `compute:checkpoint` | Actor checkpointing |
| `compute:migration` | Actor migration |
| `compute:dispute` | Dispute resolution |
| `compute:federation` | Cross-coop compute |

#### BFT Verification (result_quorum.rs)

**Task Value Classification**:

| Value Level | Required Executors | Consensus |
|-------------|-------------------|-----------|
| Low (<100 credits) | 1 | N/A |
| Medium (100-1000) | 2 | 2/2 |
| High (1000-10000) | 3 | 2/3 (67%) |
| Critical (>10000) | 5 | 4/5 (80%) |

**Quorum Process**:
1. Task submitted with `verification_config`
2. Scheduler assigns multiple executors based on value
3. Results collected from all executors (30s window)
4. Results grouped by output hash (blake3)
5. Majority determines canonical result
6. Divergent results escalate to dispute

**Quorum Status**:
```rust
enum QuorumStatus {
    Collecting { received, required },
    Consensus { result_hash, agreeing, total },
    Divergent { result_groups, total },
    Timeout { received, required },
}
```

#### Dispute Resolution (dispute.rs)

**Verification Modes**:

| Mode | Description |
|------|-------------|
| SingleExecutor | Default, fastest, cheapest |
| MultiExecutor | N executors, majority consensus |
| Optimistic | One executor, challengeable window |

**Dispute Workflow**:
1. Differing results detected → Create `ComputeDispute`
2. Evidence collection (24h window)
3. Types: ExecutionLogs, InputData, EnvironmentSnapshot, WitnessStatement
4. Re-execution by arbiter (high-trust node, min 0.7 trust)
5. Resolution: Consensus, Reexecution, or Quarantine
6. Correct executors paid, incorrect penalized

#### Scheduler (scheduler.rs)

**Resource Profiles**:
```rust
ResourceProfile {
    cpu_cores: Option<f64>,       // 0.1 = 10% of one core
    memory_mb: Option<u64>,       // RAM required
    storage_mb: Option<u64>,      // Temp storage
    network_mbps: Option<f64>,    // Bandwidth
    gpu_spec: Option<GpuSpec>,    // GPU requirements
    duration_estimate: Option<Duration>,
}
```

**Placement Scoring** (7 factors):
1. Trust (25%) - Higher trust = better
2. Capacity (20%) - Available resources
3. Queue depth (15%) - Shorter queue = better
4. Network latency (15%) - Lower RTT = better
5. Data locality (15%) - Local blobs = better
6. Locality hints (10%) - Match preferences
7. Random jitter (10%) - Break ties

**Federation Policies**:
```rust
enum FederationPolicy {
    PreferLocal,                    // Score bonus for local
    AllowFederated,                 // Equal consideration
    FederatedWhitelist { coops },   // Only specific coops
    LocalOnly,                      // Block all federated
}
```

#### Actor Migration (migration_manager.rs)

**Migration Flow**:
```
Source Executor                     Target Executor
===============                     ===============
1. Evaluate migration
   (policy.should_migrate)
        |
        v
2. Create checkpoint
        |
        v
3. Send MigrationRequest -------->  4. Evaluate capacity
        |                               (policy.should_accept)
        |                                     |
        |                                     v
        |                          5. Send Accept/Reject
        |  <-----------------------------    |
        v
6. Pause actor
        |
        v
7. Final checkpoint
        |
        v
8. Send MigrationComplete ------>  9. Resume actor
        |                               |
        v                               v
10. Cleanup                        11. Acknowledge
```

**Migration Triggers**:
- Load balancing (executor >80% utilized)
- Data locality (inputs on different node)
- Fault tolerance (node going offline)
- Resource availability (better match elsewhere)

#### WASM Execution (wasm_executor.rs)

- Sandboxed WebAssembly execution
- Fuel metering (bounded computation)
- Deterministic execution
- WASM registry with hash verification

**Threats**:
- **Wrong results**: Malicious/buggy executor → BFT quorum verification (67-80% agreement)
- **Bribed executors**: Collusion for false results → random executor selection, trust-weighted placement
- **Resource cheating**: Claiming work not done → result hash verification, re-execution by arbiter

---

### Layer 4: Privacy (icn-privacy) ✅

**Purpose**: Protect metadata, prevent traffic analysis, ensure plausible deniability.

#### Topic Encryption (topic_encryption.rs)

- **Algorithm**: ChaCha20-Poly1305 AEAD
- **Purpose**: Encrypt topic names so observers can't see what you subscribe to
- **Discovery**: Bloom filter-based (privacy-preserving)
- **Property**: False positives provide plausible deniability

```rust
// Create encryptor with shared cooperative key
let encryptor = TopicEncryptor::new(shared_key);

// Encrypt topic name
let encrypted = encryptor.encrypt("ledger:sync")?;

// Subscriber checks match without decrypting
let mut filter = TopicBloomFilter::new();
filter.insert("ledger:sync");
assert!(filter.matches(&encrypted));
```

#### Onion Routing (onion_routing.rs)

- **Purpose**: Multi-hop message routing (Tor-like)
- **Protection**: Hides sender/receiver relationships
- **Architecture**: 3-hop circuits through relay nodes
- **Defense**: Traffic analysis resistance

#### Traffic Obfuscation (traffic_obfuscation.rs)

- **Random delays**: Timing attack resistance
- **Message padding**: Hide content size patterns
- **Cover traffic**: Decoy messages at configurable rate

```rust
ObfuscationConfig {
    min_delay_ms: 50,
    max_delay_ms: 500,
    padding_target: 1024,    // Pad to 1KB
    cover_traffic_rate: 0.1, // 10% decoy messages
}
```

**Threats**:
- **Metadata leakage**: Subscription patterns reveal activity → encrypted topic names, Bloom filter discovery
- **Traffic analysis**: Timing/size correlation → random delays, message padding, cover traffic
- **Relay compromise**: Malicious circuit node → 3-hop routing, no single node sees both endpoints

---

### Layer 5: Security (icn-security) ✅

**Purpose**: Detect Byzantine faults, manage reputation, quarantine bad actors.

#### Violation Types (misbehavior.rs)

| Violation | Severity | Description |
|-----------|----------|-------------|
| ConflictingLedgerEntries | 10 (Critical) | Double-spend attempt |
| ConflictingSignedStatements | 10 | Generic Byzantine |
| ReplayAttack | 10 | Sequence number reuse |
| FailedComputeVerification | 5 (Major) | Wrong compute result |
| InvalidSignature | 5 | Forged message |
| ExcessiveResourceUse | 1 (Minor) | Rate limit violation |
| TrustGraphSpam | 1 | Attestation flooding |
| FailedStorageChallenge | 1-8 | Storage proof failure |

#### Reputation Scoring

```rust
ReputationScore: 0.0 - 1.0

// Automatic actions
score < 0.5 → Auto-quarantine (restricted operations)
score < 0.2 → Auto-ban (blocked entirely)
Critical violation → Immediate ban

// Recovery
Violations decay over time (configurable half-life)
Good behavior gradually restores score
```

**Threats**:
- **False accusations**: Frame innocent nodes → evidence required for violations, appeals process
- **Reputation laundering**: Create new identity after ban → VUI ties reputation to personhood
- **Slow attacks**: Stay below detection thresholds → statistical anomaly detection, long-term tracking

---

### Layer 6: Gossip (icn-gossip) ✅

**Protocol**: Hybrid push/pull with causal ordering.

**Note**: Most gossip topics are dynamically constructed at runtime with parameters (coop_id, domain_id, etc.). Only a few topics like `key:rotation`, `steward:announce`, and `steward:vui-sync` are hardcoded constants.

#### Vector Clocks (vector_clock.rs)

- Causal ordering of events
- 10K entry limit with LRU eviction
- Compressed representation for bandwidth

#### Bloom Filters (bloom.rs)

- Efficient anti-entropy sync
- Configurable false positive rate
- Auto-resizing based on entries

#### Topic Access Control

```rust
enum AccessControl {
    Public,                         // Anyone can publish/subscribe
    TrustClass(TrustClass),         // Requires minimum trust class
    Participants(Vec<Did>),         // Only specific participants (whitelist)
}
```

#### Trust-Gated Resource Limits

| Trust Class | Msg/sec | Max Msg Size | Window Limit |
|-------------|---------|--------------|--------------|
| Isolated | 10 | 1KB | 100KB |
| Known | 50 | 10KB | 1MB |
| Partner | 100 | 100KB | 10MB |
| Federated | 200 | 1MB | 100MB |

**Threats**:
- **Eclipse attack**: Isolate node from honest peers → peer diversity requirements, multiple bootstrap sources
- **Flooding**: Overwhelm with messages → trust-gated rate limits, per-peer quotas
- **Partition attack**: Split network view → vector clocks detect causality, anti-entropy reconciliation

---

### Layer 7: Ledger (icn-ledger) ✅

**Double-entry mutual credit** with cryptographic auditability.

**Properties**:
- Zero-sum: Every credit creates matching debit
- Merkle-DAG: Full history, cryptographic integrity
- Multi-currency: Hours, community tokens, custom units
- Gossip-synced: Eventual consistency across nodes

**Credit Limits**:
```rust
credit_limit = f(
    participation_history,    // How long active
    attestations_received,    // Vouches from others
    contracts_fulfilled,      // Track record
    dispute_record           // History of problems
)
```

**Threats**:
- **Double-spend**: Same credits spent twice → Merkle-DAG with causal ordering, conflict detection
- **Replay attack**: Resubmit old transactions → sequence numbers, signed envelopes
- **Credit inflation**: Spend beyond limit → real-time balance checking, limit enforcement

---

### Layer 8: Governance (icn-governance) ✅

**25,438 LOC, 347 tests** - Comprehensive governance primitives.

**Proposal Types**: Constitutional, Economic, Operational, Membership, Federation

**Voting Methods**:
- Simple majority
- Supermajority (2/3, 3/4)
- Quadratic voting (diminishing marginal voice)
- Liquid democracy (transitive delegation)

**Entity-Scoped**: Governance is scoped to entity level (coop, community, federation).

**Threats**:
- **Vote buying**: Pay for votes → quadratic voting reduces marginal power, anonymous voting
- **Sybil voting**: Fake members vote → VUI-backed membership, one-person-one-vote enforcement
- **Governance capture**: Minority seizes control → quorum requirements, supermajority for critical changes

---

### Layer 9: Federation (icn-federation) 🚧

**Multi-level hierarchy** with trust bridging.

**Features**:
- Arbitrary depth (coop → regional → sector → global)
- Transitive trust calculation across boundaries
- Multi-level settlement with netting
- Proposal escalation rules

**Threats**:
- **Cross-org fraud**: Exploit trust bridges → clearing limits, bond requirements, dispute arbitration
- **Settlement disputes**: Refuse to honor clearing → escrow for large transactions, federation-level governance
- **Federation capture**: One coop dominates → weighted voting by membership, separation of powers

---

## The Economic Vision

**Three-Layer Economy**:

```
┌────────────────────────────────────────────────────────────┐
│  LAYER 3: FIAT BRIDGES                                     │
│  Treasury coops hold reserves, governed gateways,          │
│  rate bands not markets. For: taxes, imports, medical.     │
└────────────────────────────────────────────────────────────┘
                              ↑↓
┌────────────────────────────────────────────────────────────┐
│  LAYER 2: ASSET TOKENS                                     │
│  Real goods with provenance chains, attested creation,     │
│  negotiated exchange within guardrails, optional demurrage │
└────────────────────────────────────────────────────────────┘
                              ↑↓
┌────────────────────────────────────────────────────────────┐
│  LAYER 1: MUTUAL CREDIT                                    │
│  Hours or community units, reputation-gated limits,        │
│  zero-sum coordination, no backing required                │
└────────────────────────────────────────────────────────────┘
```

**Value Retention Flywheel**:
```
More coops → More internal trade → Less leakage →
Capital accumulates → More new coop formation →
More coops → [compound forever]
```

---

## The 27 Crates

| Category | Crates |
|----------|--------|
| **Core** | icn-core |
| **Identity** | icn-identity, icn-steward, icn-zkp, icn-crypto-pq |
| **Trust** | icn-trust |
| **Network** | icn-net, icn-gossip |
| **Storage** | icn-store, icn-encoding, icn-snapshot |
| **Compute** | icn-compute |
| **Economy** | icn-ledger, icn-ccl |
| **Governance** | icn-governance |
| **Organization** | icn-entity, icn-coop, icn-community, icn-federation |
| **Security** | icn-security, icn-privacy |
| **API** | icn-gateway, icn-rpc, icn-api |
| **Observability** | icn-obs |
| **Infrastructure** | icn-time |
| **Testing** | icn-testkit |

---

## Summary

ICN is a **complete civilizational substrate** with:

1. **Recursive Entity Model** - Same patterns at every scale
2. **Self-Governance** - ICN governs itself using ICN
3. **Trust as Infrastructure** - Gates resources, compute, credit, federation
4. **Distributed Compute** - BFT verification, actor migration, WASM execution
5. **Privacy by Design** - Topic encryption, onion routing, traffic obfuscation
6. **Byzantine Fault Tolerance** - Misbehavior detection, reputation, quarantine
7. **Economic Autonomy** - Value circulates internally, fiat bridge minimized
8. **Speculation Resistance** - Demurrage, clearing bands, guardrails

Every layer integrates with every other layer through the trust graph. The compute layer uses trust for scheduling. The ledger uses trust for credit limits. The gossip layer uses trust for rate limiting. The governance uses trust for voting weight. This is **trust-as-infrastructure**, not trust-as-afterthought.

---

## Glossary

| Term | Definition |
|------|------------|
| **DID** | Decentralized Identifier. Format: `did:icn:<base58-ed25519-pubkey>`. Self-sovereign identity anchor. |
| **VC** | Verifiable Credential. Signed attestation (e.g., membership) issued by an entity, stored in wallet. |
| **VUI** | Verifiable Unique Identity. Proof-of-personhood via steward ceremony. Prevents Sybil attacks. |
| **Entity** | Any ICN participant: Individual, Cooperative, Community, Federation. Recursive structure. Not to be confused with Jurisdiction. |
| **EntityId** | Identifier for entities. Format: `entity:icn:<type>:<identifier>` (e.g., `entity:icn:coop:food-coop-123`). |
| **Jurisdiction** | Governance domain within an entity. Not an entity type itself—it's where governance decisions are scoped. |
| **Gateway** | HTTP/WebSocket API server. Bridges client apps to ICN daemon. Not a custodian. |
| **Full Node** | Complete ICN daemon with ledger, gossip, compute. Run by cooperatives. |
| **Light Client** | Wallet app that signs locally, calls gateway APIs. No local ledger copy. |
| **Steward** | Trusted member who conducts VUI ceremonies and participates in key recovery. |
| **Trust Class** | Category based on trust score: Isolated (<0.1), Known (0.1-0.4), Partner (0.4-0.7), Federated (0.7+). |
| **Trust Graph** | Directed weighted graph of trust relationships. Multiple contexts (financial, governance, etc.). |
| **CCL** | Cooperative Contract Language. Deterministic, capability-based, fuel-metered agreement execution. |
| **Capability** | Explicit permission granted to a contract or user (e.g., `ReadLedger`, `WriteLedger`). |
| **Capability Token** | Scoped access token. Subset of full permissions for specific operations. |
| **Mutual Credit** | Zero-sum accounting where credits and debits always balance. No external backing required. |
| **Credit Limit** | Maximum negative balance allowed. Gated by trust, participation history, vouches. |
| **Merkle-DAG** | Directed acyclic graph where each node references parents by hash. Ensures auditability. |
| **Clearing Entry** | Inter-cooperative balance. Settled periodically via clearing house. |
| **Clearing House** | Federation service that nets inter-coop balances and coordinates settlement. |
| **Quorum** | Minimum agreement threshold for BFT verification (e.g., 67% for high-value compute). |
| **Arbiter** | High-trust node (≥0.7) that re-executes disputed compute tasks. |
| **Actor Migration** | Moving a running compute actor from one executor to another with state preservation. |
| **Gossip** | Epidemic protocol for propagating messages. Push announcements, pull requests, anti-entropy. |
| **Vector Clock** | Logical clock for causal ordering of distributed events. |
| **Topic** | Named channel for gossip messages. Access controlled (Open, TrustGated, MembersOnly, CooperativeOnly). |
| **Scope** | Gossip propagation range: LocalCluster, Regional, Global. |
| **Federation** | Network of cooperatives with shared trust, clearing, and governance. |
| **Meta-Federation** | Federation of federations. Enables arbitrary hierarchy depth. |
| **Protocol Commons** | Top-level ICN entity that governs protocol parameters democratically. |
| **Subsidiarity** | Principle: decisions made at lowest appropriate level. Higher levels constrain, not dictate. |
| **Demurrage** | Negative interest on holdings. Encourages circulation, discourages hoarding. |
| **Fuel** | Metering unit for contract execution. Prevents infinite loops. |

---

## Related Documents

- [ICN_REAL_WORLD_DEPLOYMENT.md](ICN_REAL_WORLD_DEPLOYMENT.md) - How ICN works at scale
- [ICN_MINIMUM_VERTICAL_SLICE.md](ICN_MINIMUM_VERTICAL_SLICE.md) - End-to-end operation traces
- [ECONOMIC_ARCHITECTURE.md](ECONOMIC_ARCHITECTURE.md) - Economic system details
- [ECONOMIC_VISION.md](ECONOMIC_VISION.md) - Strategic framing
- [dev-journal/ROADMAP.md](dev-journal/ROADMAP.md) - Implementation roadmap

---

## Document History

- 2026-01-23: Added Invariants, Threat Models, Identity→Membership→Authorization, Lifecycle, Glossary
- 2026-01-23: Initial creation from comprehensive codebase analysis
