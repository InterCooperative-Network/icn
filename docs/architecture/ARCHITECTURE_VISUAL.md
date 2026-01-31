# ICN Visual Architecture Reference
**Quick Reference for System Structure**

> **ICN is a constraint engine: apps translate meaning into constraints; the kernel enforces constraints without understanding meaning.**

## Constraint Engine Model

ICN implements a constraint enforcement architecture where Policy Oracles (apps) translate domain semantics into generic constraints that the kernel enforces blindly:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    CONSTRAINT ENFORCEMENT (Kernel)                   │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │  Transport  │  Replay  │  Rate     │  Capability │  Credit    │ ││
│  │   Auth      │  Guard   │  Limiter  │   Gate      │  Gate      │ ││
│  │  (verify)   │ (reject) │  (apply)  │   (check)   │  (check)   │ ││
│  └─────────────────────────────────────────────────────────────────┘│
│                                                                      │
│   Kernel enforces ConstraintSet values. Kernel does NOT decide them. │
└─────────────────────────────────────────────────────────────────────┘
         ▲              ▲           ▲            ▲           ▲
         │              │           │            │           │
    ConstraintSet {
      rate_limit: 20/s,      // ← value from PolicyOracle
      credit_ceiling: 1000,  // ← value from PolicyOracle
      capabilities: [...]    // ← value from PolicyOracle
    }
         │              │           │            │           │
┌────────┴──────────────┴───────────┴────────────┴───────────┴────────┐
│                      POLICY ORACLES (Apps)                           │
│                                                                      │
│   Apps/Governance DECIDE constraint values.                          │
│   Kernel ENFORCES them without knowing why.                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │  Trust   │  │  Ledger  │  │Governance│  │Membership│            │
│  │  Oracle  │  │  Oracle  │  │  Oracle  │  │  Oracle  │            │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘            │
└─────────────────────────────────────────────────────────────────────┘
```

### Key Distinction: Mechanism vs. Policy

| | Kernel (Hard) | Apps/Governance (Meta) |
|---|---|---|
| **Rate limiting** | Mechanism exists | Values decided (20/s vs 100/s) |
| **Credit gating** | Mechanism exists | Ceiling decided (1000 vs 5000) |
| **Capability gating** | Mechanism exists | Grants decided |
| **Replay guard** | Mechanism exists | N/A (no tunable values) |
| **Transport auth** | Mechanism exists | N/A (no tunable values) |

**Critical Property:**
- Governance can change **what** the rate limit is
- Governance **cannot** remove **that** rate limiting exists

---

## Component Organization

The implementation organizes components into functional areas. Note: "layer" terminology below is descriptive (e.g., "transport layer security") — ICN is **not** an OSI-like strictly layered stack. Instead, policy oracles translate domain semantics into constraints that the kernel enforces.

```
┌────────────────────────────────────────────────────────────────┐
│                      USER APPLICATIONS                          │
│  Web Apps • Mobile Apps • CLI Tools • TUI Console              │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ REST/WebSocket/RPC
┌────────────────────────────────────────────────────────────────┐
│                      API GATEWAY LAYER                          │
│  ┌──────────────────┐              ┌──────────────────┐        │
│  │  icn-gateway     │              │    icn-rpc       │        │
│  │  • REST API      │              │  • JSON-RPC      │        │
│  │  • WebSocket     │              │  • CLI comms     │        │
│  │  • JWT Auth      │              │  • Challenge     │        │
│  └──────────────────┘              └──────────────────┘        │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Handle Messages
┌────────────────────────────────────────────────────────────────┐
│                    APPLICATION ACTORS                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │  Compute     │  │  Governance  │  │  CCL (Smart  │         │
│  │  • Schedule  │  │  • Proposals │  │   Contracts) │         │
│  │  • Execute   │  │  • Voting    │  │  • Rules     │         │
│  │  • Results   │  │  • Actions   │  │  • Interp    │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Gossip Topics
┌────────────────────────────────────────────────────────────────┐
│                   SYNCHRONIZATION LAYER                         │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Gossip Actor (icn-gossip)                   │   │
│  │  • Topics (pub/sub channels)                             │   │
│  │  • Push protocol (announce new content)                  │   │
│  │  • Pull protocol (request missing entries)               │   │
│  │  • Anti-entropy (Bloom filters)                          │   │
│  │  • Vector clocks (detect duplicates)                     │   │
│  │  • Access control (Public/Private/TrustGated)            │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Read/Write State
┌────────────────────────────────────────────────────────────────┐
│                      STATE LAYER                                │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Ledger (icn-ledger)                         │   │
│  │  • Double-entry journal (Merkle-DAG)                     │   │
│  │  • Balances (cached in-memory)                           │   │
│  │  • Credit limits & safety rails                          │   │
│  │  • Fork detection & resolution                           │   │
│  │  • Quarantine (invalid entries)                          │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Trust Queries
┌────────────────────────────────────────────────────────────────┐
│                      TRUST LAYER                                │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Trust Graph (icn-trust)                     │   │
│  │  • Directed weighted graph (Did → Did)                   │   │
│  │  • Trust scores (0.0 to 1.0, transitive)                 │   │
│  │  • Trust classes (Known/Colleague/Close/Intimate)        │   │
│  │  • Multi-graph (separate graphs per context)             │   │
│  │  • Propagation (automatic edge discovery)                │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Send/Receive
┌────────────────────────────────────────────────────────────────┐
│                      NETWORK LAYER                              │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Network Actor (icn-net)                     │   │
│  │  • QUIC transport (UDP multiplexing)                     │   │
│  │  • TLS 1.3 (certificate-based auth)                      │   │
│  │  • DID-TLS binding (cert pubkey = DID)                   │   │
│  │  • SignedEnvelope (message integrity + replay guard)     │   │
│  │  • EncryptedEnvelope (end-to-end encryption)             │   │
│  │  • mDNS discovery (local network peers)                  │   │
│  │  • NAT traversal (ICE-like candidates)                   │   │
│  │  • Rate limiting (trust-gated per peer)                  │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Sign/Verify
┌────────────────────────────────────────────────────────────────┐
│                      IDENTITY LAYER                             │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Identity (icn-identity)                     │   │
│  │  • DID: did:icn:<base58-ed25519-pubkey>                 │   │
│  │  • KeyPair (Ed25519 signing + X25519 encryption)        │   │
│  │  • KeyStore (Age-encrypted file)                         │   │
│  │  • Key rotation (signed transitions)                     │   │
│  │  • Social recovery (threshold decryption)                │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Persist
┌────────────────────────────────────────────────────────────────┐
│                    STORAGE LAYER                                │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │              Store (icn-store)                           │   │
│  │  • Sled embedded database (key-value)                    │   │
│  │  • Transactional updates                                 │   │
│  │  • Range queries (prefix scans)                          │   │
│  │  • Storage quotas (per-DID limits)                       │   │
│  └─────────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────┘
```

## Actor Communication Pattern

```
┌──────────────────────────────────────────────────────────────────┐
│                          Supervisor                               │
│  • Spawns all actors                                             │
│  • Propagates shutdown signal                                    │
│  • Manages restart on failure                                    │
└──────────────────────────────────────────────────────────────────┘
        │
        ├─────────────┬─────────────┬─────────────┬─────────────┐
        ▼             ▼             ▼             ▼             ▼
  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │ Network  │  │  Gossip  │  │  Ledger  │  │Governance│  │ Compute  │
  │  Actor   │  │  Actor   │  │ (struct) │  │  Actor   │  │  Actor   │
  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘
       │             │             │             │             │
       ▼             ▼             ▼             ▼             ▼
  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
  │ Network  │  │  Gossip  │  │  Ledger  │  │Governance│  │ Compute  │
  │  Handle  │  │  Handle  │  │  Handle  │  │  Handle  │  │  Handle  │
  │   (tx)   │  │   (tx)   │  │   (tx)   │  │   (tx)   │  │   (tx)   │
  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘
       ▲             ▲             ▲             ▲             ▲
       │             │             │             │             │
       └─────────────┴─────────────┴─────────────┴─────────────┘
                        External callers
              (Gateway, RPC, other actors, tests)
```

### Message Flow Example: Ledger Transaction

```
1. Gateway API Handler
   POST /api/v1/ledger/transfer
         ▼
2. Ledger Handle
   ledger_handle.transfer(from, to, amount, currency).await
         ▼
3. Ledger Actor (via mpsc::channel)
   LedgerMsg::Transfer { from, to, amount, currency, reply }
         ▼
4. Ledger State Update
   - Create JournalEntry (hash, sign)
   - Validate balances & credit limits
   - Update cached_balances
   - Persist to store
         ▼
5. Gossip Notification
   gossip_handle.announce("ledger:sync", entry_bytes).await
         ▼
6. Gossip Actor
   - Broadcast entry hash to all peers
   - Store entry locally
   - Notify subscribers
         ▼
7. Network Layer
   - Sign entry (SignedEnvelope)
   - Send to all connected peers
   - Rate limit per trust class
         ▼
8. Peer Nodes
   - Receive entry via gossip subscription
   - Validate & apply to local ledger
   - Update balances
         ▼
9. WebSocket Broadcast (Gateway)
   - Notify all connected clients
   - Event: { type: "transfer", from, to, amount }
```

## Data Flow: Complete Lifecycle

```
┌──────────────────────────────────────────────────────────────────┐
│ 1. IDENTITY CREATION                                             │
│    KeyPair::generate() → DID → Age-encrypt → Save to disk        │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ 2. NODE STARTUP                                                   │
│    Unlock keystore → Load identity → Spawn actors                │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ 3. PEER DISCOVERY                                                 │
│    mDNS announce → Peer responds → QUIC connect → DID-TLS verify │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ 4. TRUST ESTABLISHMENT                                            │
│    Manual trust edge creation → Propagate via gossip             │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ 5. GOSSIP SYNC                                                    │
│    Subscribe to topics → Anti-entropy (Bloom) → Pull missing     │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ 6. LEDGER CONVERGENCE                                             │
│    Receive entries → Validate → Apply → Update balances          │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ 7. APPLICATION USE                                                │
│    Submit transactions • Execute contracts • Vote on proposals    │
└──────────────────────────────────────────────────────────────────┘
```

## Security Layers

```
┌──────────────────────────────────────────────────────────────────┐
│ APPLICATION LAYER SECURITY                                        │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ Contract Capabilities                                       │  │
│  │  • ReadLedger (query balances)                              │  │
│  │  • WriteLedger (submit transactions)                        │  │
│  │  • ReadTrust (query trust scores)                           │  │
│  │  • Fuel metering (prevent infinite loops)                   │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ MESSAGE LAYER SECURITY                                            │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ SignedEnvelope (Integrity + Authentication)                 │  │
│  │  • Ed25519 signature (64 bytes)                             │  │
│  │  • Timestamp (replay protection)                            │  │
│  │  • Nonce (uniqueness)                                       │  │
│  │  • Sender DID (attribution)                                 │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ EncryptedEnvelope (Confidentiality)                         │  │
│  │  • X25519 key exchange (32-byte shared secret)              │  │
│  │  • ChaCha20-Poly1305 AEAD (cipher + MAC)                    │  │
│  │  • Nonce (12 bytes, random)                                 │  │
│  │  • Tag (16 bytes, authentication)                           │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│ TRANSPORT LAYER SECURITY                                          │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │ QUIC/TLS 1.3                                                 │  │
│  │  • Certificate verification (peer must present cert)        │  │
│  │  • DID-TLS binding (cert pubkey MUST match DID)             │  │
│  │  • Stream multiplexing (isolate requests)                   │  │
│  │  • 0-RTT resumption (fast reconnect)                        │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

## Gossip Topic Architecture

```
Topic: "ledger:sync"
  ├─ Access Control: TrustGated(0.1)  # Known+ trust required
  ├─ Publishers: Ledger actors
  ├─ Subscribers: Ledger actors
  └─ Payload: Serialized JournalEntry

Topic: "compute:submit"
  ├─ Access Control: TrustGated(0.3)  # Colleague+ trust required
  ├─ Publishers: Compute actors (submitters)
  ├─ Subscribers: Compute actors (executors)
  └─ Payload: Task { id, wasm_hash, inputs, constraints }

Topic: "compute:claim"
  ├─ Access Control: TrustGated(0.3)
  ├─ Publishers: Compute actors (executors)
  ├─ Subscribers: Compute actors (submitters)
  └─ Payload: Claim { task_id, executor_did }

Topic: "compute:result"
  ├─ Access Control: TrustGated(0.3)
  ├─ Publishers: Compute actors (executors)
  ├─ Subscribers: Compute actors (submitters)
  └─ Payload: Result { task_id, output, proof }

Topic: "governance:proposal"
  ├─ Access Control: Public
  ├─ Publishers: Governance actors
  ├─ Subscribers: All nodes
  └─ Payload: Proposal { id, title, actions, status }

Topic: "governance:vote"
  ├─ Access Control: Public
  ├─ Publishers: Governance actors
  ├─ Subscribers: Governance actors
  └─ Payload: Vote { proposal_id, voter, choice }

Topic: "identity:recovery"
  ├─ Access Control: Private
  ├─ Publishers: Identity actors
  ├─ Subscribers: Recovery guardians
  └─ Payload: RecoveryMessage { encrypted_shard }

Topic: "network:candidates"
  ├─ Access Control: Public
  ├─ Publishers: Network actors
  ├─ Subscribers: Network actors
  └─ Payload: Candidate { did, address, connection_id }
```

## Compute Scheduler Decision Tree

```
New Task Submitted
       │
       ▼
┌──────────────────┐
│ Filter Executors │  (trust_score >= MIN_TRUST_EXECUTE)
└──────────────────┘
       │
       ▼
┌──────────────────┐
│ Check Resources  │  (memory, CPU, storage sufficient?)
└──────────────────┘
       │
       ▼
┌──────────────────┐
│ Policy Selection │  (TrustWeighted / FIFO / LoadBalanced / etc.)
└──────────────────┘
       │
       ├─── TrustWeighted ────► Sort by trust score DESC
       │
       ├─── LocalityPreferred ─► Sort by locality score DESC
       │
       ├─── LoadBalanced ─────► Sort by current load ASC
       │
       └─── CooperativeEquity ─► Round-robin per cooperative
              │
              ▼
       ┌──────────────────┐
       │ Assign Executor  │
       └──────────────────┘
              │
              ▼
       ┌──────────────────┐
       │ Publish "claim"  │  (announce executor assignment)
       └──────────────────┘
              │
              ▼
       ┌──────────────────┐
       │ Execute WASM     │  (sandboxed, fuel-limited)
       └──────────────────┘
              │
              ▼
       ┌──────────────────┐
       │ Publish "result" │  (output + proof)
       └──────────────────┘
              │
              ▼
       ┌──────────────────┐
       │ Payment (Ledger) │  (credit executor account)
       └──────────────────┘
```

## Ledger Fork Resolution

```
Fork Detected (two entries with same parent hash)
       │
       ▼
┌────────────────────────────────────────┐
│ ForkDetector collects conflicting     │
│ entries                                 │
└────────────────────────────────────────┘
       │
       ▼
┌────────────────────────────────────────┐
│ ForkResolver applies policy:            │
│  • TimestampBased (earliest wins)       │
│  • TrustBased (highest trust wins)      │
│  • ProposalBased (governance vote)      │
└────────────────────────────────────────┘
       │
       ▼
┌────────────────────────────────────────┐
│ Winning entry accepted                  │
│ Losing entry(s) quarantined             │
└────────────────────────────────────────┘
       │
       ▼
┌────────────────────────────────────────┐
│ Gossip update propagated                │
│ All nodes converge to same resolution   │
└────────────────────────────────────────┘
```

## Directory Structure

```
icn/
├── bins/                    # Binaries
│   ├── icnd/               # Daemon (supervisor + actors)
│   ├── icnctl/             # CLI management tool
│   └── icn-console/        # TUI application
│
├── crates/                  # Library crates
│   ├── icn-core/           # Supervisor, runtime, config
│   ├── icn-identity/       # DIDs, keypairs, keystore
│   ├── icn-trust/          # Trust graph
│   ├── icn-net/            # QUIC/TLS network
│   ├── icn-gossip/         # Pub/sub sync
│   ├── icn-ledger/         # Mutual credit ledger
│   ├── icn-ccl/            # Contract language
│   ├── icn-compute/        # Distributed compute
│   ├── icn-governance/     # Proposals & voting
│   ├── icn-gateway/        # REST + WebSocket API
│   ├── icn-rpc/            # JSON-RPC server
│   ├── icn-store/          # Persistent storage
│   ├── icn-obs/            # Metrics & logging
│   ├── icn-security/       # Byzantine detection
│   ├── icn-time/           # Clock sync
│   ├── icn-privacy/        # Metadata protection
│   ├── icn-federation/     # Inter-coop coordination
│   ├── icn-steward/        # SDIS enrollment
│   ├── icn-crypto-pq/      # Post-quantum crypto
│   ├── icn-zkp/            # Zero-knowledge proofs
│   ├── icn-snapshot/       # Backup/restore
│   └── icn-testkit/        # Test utilities
│
├── docs/                    # Documentation (198 files)
│   ├── ARCHITECTURE.md     # Detailed design doc
│   ├── GETTING_STARTED.md  # New contributor guide
│   ├── api/                # API specifications
│   ├── dev-journal/        # Development notes
│   ├── manual/             # User manual
│   └── ...
│
├── config/                  # Configuration templates
├── deploy/                  # Kubernetes configs
├── monitoring/              # Grafana dashboards
├── scripts/                 # Build & test scripts
├── web/                     # Web UIs
└── sdk/                     # Client SDKs
```

## Key Metrics Dashboard Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ ICN Node Metrics                                    [9090/metrics]│
├─────────────────────────────────────────────────────────────────┤
│ Supervisor State:  [Running]   Uptime: 2h 34m                   │
├─────────────────────────────────────────────────────────────────┤
│ Network                                                           │
│  • Active Sessions: 12                                           │
│  • Bytes Sent:      1.2 GB                                       │
│  • Bytes Received:  980 MB                                       │
│  • Rate Limited:    3 (trust violations)                         │
├─────────────────────────────────────────────────────────────────┤
│ Gossip                                                            │
│  • Topics:          8                                            │
│  • Total Entries:   5,432                                        │
│  • Push/Pull Rate:  120 msg/s                                    │
│  • Convergence:     95% (within 5s)                              │
├─────────────────────────────────────────────────────────────────┤
│ Ledger                                                            │
│  • Journal Size:    2,145 entries                                │
│  • Balance Sum:     0.00 (invariant ✓)                           │
│  • Quarantined:     0                                            │
│  • Fork Rate:       0.01% (rare)                                 │
├─────────────────────────────────────────────────────────────────┤
│ Compute                                                           │
│  • Tasks Submitted: 234                                          │
│  • Tasks Completed: 230                                          │
│  • Tasks Failed:    4 (1.7%)                                     │
│  • Avg Latency:     125ms                                        │
├─────────────────────────────────────────────────────────────────┤
│ Trust                                                             │
│  • Graph Size:      45 nodes, 123 edges                          │
│  • Cache Hits:      98.5%                                        │
│  • Propagations:    67 (auto-discovered)                         │
└─────────────────────────────────────────────────────────────────┘
```

## Trust Class Hierarchy

```
┌──────────────────────────────────────────────────────────────────┐
│ Trust Score Range │ Trust Class  │ Capabilities                  │
├──────────────────────────────────────────────────────────────────┤
│ 0.0 - 0.1         │ Unknown      │ • None (blocked)              │
├──────────────────────────────────────────────────────────────────┤
│ 0.1 - 0.3         │ Known        │ • Read ledger                 │
│                   │              │ • Read trust graph            │
│                   │              │ • Basic rate limit: 10/min    │
├──────────────────────────────────────────────────────────────────┤
│ 0.3 - 0.6         │ Colleague    │ • Write ledger                │
│                   │              │ • Submit compute tasks        │
│                   │              │ • Moderate rate: 100/min      │
├──────────────────────────────────────────────────────────────────┤
│ 0.6 - 0.8         │ Close        │ • Execute compute tasks       │
│                   │              │ • Create proposals            │
│                   │              │ • High rate: 1000/min         │
├──────────────────────────────────────────────────────────────────┤
│ 0.8 - 1.0         │ Intimate     │ • Social recovery guardian    │
│                   │              │ • Governance admin            │
│                   │              │ • Unlimited rate              │
└──────────────────────────────────────────────────────────────────┘
```

---

**For comprehensive details, see ARCHITECTURE_MAP.md**
**For design rationale, see docs/ARCHITECTURE.md**
