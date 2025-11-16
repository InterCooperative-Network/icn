# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ICN (Intercooperative Network) is a substrate daemon for the cooperative internet. It is **not** a blockchain or federation server - it's a P2P coordination layer with:

- **Identity Layer**: Decentralized identifiers (DIDs) with Ed25519 cryptography
- **Trust Graph**: Web-of-participation based trust computation
- **Networking**: QUIC/TLS secure sessions with mDNS discovery
- **Cooperative Contracts**: CCL (Cooperative Contract Language) execution
- **Mutual Credit Ledger**: Double-entry accounting with Merkle-DAG
- **P2P Coordination**: Gossip protocol with trust-gated topics

## Workspace Structure

The Cargo workspace is located in `icn/` subdirectory. All build/test commands must be run from `/home/matt/projects/icn/icn/`.

**Crates** (in `icn/crates/`):
- `icn-core` - Tokio runtime, supervisor, actor lifecycle management
- `icn-identity` - DID generation, Ed25519 keypairs, Age-encrypted keystore
- `icn-trust` - Trust graph storage & transitive trust computation
- `icn-net` - QUIC/TLS sessions, mDNS discovery, NetworkActor
- `icn-gossip` - Topic-based gossip with vector clocks & Bloom filters
- `icn-ledger` - Double-entry mutual credit with Merkle-DAG
- `icn-ccl` - Contract language AST, interpreter, fuel metering
- `icn-store` - Persistent KV storage (Sled)
- `icn-rpc` - gRPC API server
- `icn-obs` - Prometheus metrics, tracing, logging
- `icn-gateway` - REST + WebSocket API for cooperative applications (Phase 14)
- `icn-governance` - Governance primitives for community decision-making (Phase 13)
- `icn-testkit` - Test utilities for multi-node scenarios

**Binaries** (in `icn/bins/`):
- `icnd` - The ICN daemon
- `icnctl` - CLI management tool

## Documentation Structure

**Project root `/home/matt/projects/icn/`:**
- `CLAUDE.md` - This file; guidance for Claude Code when working on the project
- `README.md` - Project overview and quick start guide for users
- `ROADMAP.md` - Strategic roadmap and future development plans (see this for "what's next")
- `CHANGELOG.md` - Formal, user-facing changelog following Keep a Changelog format

**Documentation directory `/home/matt/projects/icn/docs/`:**
- `ARCHITECTURE.md` - System architecture, component design, and implementation details
- `production-hardening.md` - Security hardening measures and vulnerability fixes
- `deployment-guide.md` - Installation, configuration, monitoring, and operations
- `topic-subscriptions-api.md` - API reference for gossip subscriptions
- `governance-primitives.md` - Design spec for governance layer (Phase 13)
- `econ-modeling.md` - Economic modeling research for mutual credit systems
- `dev-journal/` - Detailed development journals (see below)

### Development Journal (`docs/dev-journal/`)

The dev journal contains detailed, chronological records of development sessions. Each journal entry:

**Purpose:**
- Document design decisions and rationale
- Record implementation challenges and solutions
- Provide context for future development
- Track progress within major phases

**When to create a new entry:**
- Starting work on a new phase or major feature
- After completing significant work (e.g., Phase 7 production hardening)
- When making architectural decisions that need documentation
- After resolving complex bugs or challenges

**Naming convention:** `YYYY-MM-DD-phase-N-feature-name.md`

**What to include:**
- Phase/feature overview and goals
- Implementation approach and architecture decisions
- Challenges encountered and how they were solved
- Test results and validation
- Security considerations
- Links to relevant commits
- Next steps or remaining work

**What NOT to include:**
- Routine commits (those go in git log)
- Minor bug fixes (document in commit messages)
- Refactoring details without architectural significance

**Distinction from CHANGELOG:**
- **Dev journal**: Detailed, developer-focused narrative with context and reasoning
- **CHANGELOG**: Concise, user-facing list of changes following semantic versioning

## Build & Test Commands

All commands run from `icn/` directory:

```bash
# Build everything
cargo build

# Build release binaries
cargo build --release

# Run all tests
cargo test

# Run tests for a specific package
cargo test -p icn-gossip

# Run a specific test by name
cargo test test_two_node_convergence

# Run integration tests only
cargo test --test '*'

# Build & run the daemon (debug)
cargo build && ./target/debug/icnd

# Build & run icnctl
cargo build && ./target/debug/icnctl status
```

## Architecture: Actor-Based Runtime

ICNd uses Tokio with an actor pattern. The supervisor (`icn-core/src/supervisor.rs`) spawns and manages actors:

1. **Runtime** (`icn-core/src/runtime.rs`):
   - Entry point that creates Supervisor
   - Manages broadcast shutdown signal via `tokio::sync::broadcast`
   - Loads config and unlocks keystore before spawning actors

2. **Supervisor** (`icn-core/src/supervisor.rs`):
   - Spawns all actors (GossipActor, NetworkActor, Ledger)
   - Initializes metrics server on port 9090
   - Bridges gossip messages over network layer
   - Sets up bidirectional actor communication via callbacks

3. **GossipActor** (`icn-gossip/src/gossip.rs`):
   - Wrapped in `Arc<RwLock<GossipActor>>` for shared access
   - Handles topic subscriptions, vector clocks, anti-entropy
   - Receives messages via `handle_message()` method
   - Sends messages via callback closure to NetworkActor
   - Access control based on trust graph lookups

4. **NetworkActor** (`icn-net/src/actor.rs`):
   - Message-passing via `mpsc::channel` (send `NetworkMsg` enum)
   - Returns `NetworkHandle` for async API
   - Methods: `send_message()`, `broadcast()`, `dial()`, `get_peers()`
   - Spawns background tasks for mDNS discovery and connection management
   - Routes incoming messages to registered `IncomingMessageHandler`

5. **Ledger** (`icn-ledger/src/ledger.rs`):
   - Wrapped in `Arc<RwLock<Ledger>>` for shared access
   - Integrates with GossipActor via `set_gossip()` for distributed sync
   - Publishes journal entries to `ledger:sync` gossip topic
   - Maintains double-entry invariants with quarantine for conflicts

## Actor Communication Pattern

The supervisor sets up bidirectional communication between actors:

```rust
// Network → Gossip: Incoming message handler
let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
    if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
        let mut gossip = gossip_handle.blocking_write();
        gossip.handle_message(gossip_msg)?;
    }
});

// Gossip → Network: Send callback
let send_callback: SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
    let net_msg = NetworkMessage::new(from_did, recipient, MessagePayload::Gossip(gossip_msg));
    network_handle.send_message(recipient, net_msg).await?;
});
gossip.set_send_callback(send_callback);
```

This pattern is used throughout integration tests (see `icn/crates/icn-core/tests/network_gossip_integration.rs`).

## Key Protocols

**Gossip Protocol** (`icn-gossip`):
- Push announcements: Broadcast new content hashes
- Pull requests: Request missing content by hash
- Anti-entropy: Periodic Bloom filter exchange
- Vector clocks: Track causal dependencies per peer
- **Subscription notifications**: Reactive callbacks when new entries arrive in subscribed topics

**Subscription Notifications**:
Subscribers can register a callback to be notified when new entries are published to topics:
```rust
let notification_callback: EntryNotificationCallback = Arc::new(|topic, entry, subscriber_did| {
    println!("New entry in {}: {:?} for {}", topic, entry.hash, subscriber_did);
    // Process the entry...
});
gossip.set_notification_callback(notification_callback);

// Subscribe to topic - will receive notifications for new entries
gossip.subscribe("ledger:sync", my_did)?;
```
Each subscriber receives individual notifications. This enables reactive patterns like UI updates, event-driven workflows, and real-time collaboration.

**Ledger Sync** (`icn-ledger/src/sync.rs`):
- Publishes entries to topic `ledger:sync`
- Serializes `LedgerSyncMessage` to bytes
- Gossip ensures eventual consistency across nodes
- Quarantine mechanism for conflicting entries

**Network Protocol** (`icn-net/src/protocol.rs`):
- `NetworkMessage` envelope with `from_did`, `to_did`, `payload`
- Payload types: `Gossip`, `Rpc`, `Subscribe`, `Hello`, `Signed` (+ others)
- Length-prefixed framing over QUIC streams
- TLS certificates derived from DID Ed25519 keys

**Signed Messages** (`icn-net/src/envelope.rs`, `icn-net/src/replay_guard.rs`):
- **SignedEnvelope**: Application-level signed messages with Ed25519 signatures
- **Security properties**: Authenticity, integrity, freshness, replay protection
- **ReplayGuard**: Per-peer sequence tracking with Bloom filters
- **Automatic verification**: NetworkActor verifies all `Signed` messages before forwarding

Creating signed messages:
```rust
use icn_net::{SignedEnvelope, PayloadType, NetworkMessage};

// Create signed envelope
let envelope = SignedEnvelope::new(
    &sender_did,
    &sender_keypair,
    sequence_number,     // Monotonic per-sender
    PayloadType::Gossip, // Or Ledger, Trust, Contract, etc.
    payload_bytes,
)?;

// Wrap in NetworkMessage and send
let msg = NetworkMessage::signed(Some(recipient_did), envelope);
network_handle.send_message(recipient_did, msg).await?;
```

Verification is automatic:
- NetworkActor checks Ed25519 signature
- Validates timestamp age (default: 300s clock skew)
- Detects replay attacks via sequence number
- Forwards verified messages to handler
- Drops invalid messages (logs warning)

## Cooperative Contract Language (CCL)

CCL (`icn-ccl`) is a domain-specific language for expressing agreements:

- **AST-based** (`ast.rs`): `Contract`, `Rule`, `Stmt`, `Expr`, `Value`
- **Capability system**: `ReadLedger`, `WriteLedger`, `ReadTrust`
- **Fuel metering**: Bounded execution prevents infinite loops
- **Not Turing-complete**: No recursion, fixed iteration bounds
- **Deterministic**: Same inputs always produce same outputs

Example contract invocation:
```rust
let runtime = ContractRuntime::new(ledger_handle, trust_handle);
let result = runtime.invoke_rule(&contract, "record_service", args, &sender_did)?;
```

## Testing Patterns

**Integration Tests**:
- Located in `icn/crates/icn-core/tests/` and `icn/crates/icn-ledger/tests/`
- Use `TestNode` helper pattern to spawn isolated nodes
- Each node gets unique port and keypair
- Nodes dial each other via `network_handle.dial(addr, did)`
- Verify convergence with retries and timeouts

**Test Utilities** (`icn-testkit`):
- Helpers for multi-node test scenarios
- Temporary directory management
- Test keypair generation

## Identity & Keystore

- DIDs are Ed25519-based: `did:icn:<base58-pubkey>`
- Keystore is Age-encrypted with passphrase
- Located at `$DATA_DIR/keystore.age` (default: `~/.icn/keystore.age`)
- **Security**: Passphrase uses `Zeroizing<Vec<u8>>` to prevent memory recovery
- Key rotation supported with transition records

**Keystore Format Migration (v1 → v2.1):**
- **v1 format**: Contains only Ed25519 keypair (legacy)
- **v2 format**: Adds TLS certificate + DID-TLS binding signature
- **v2.1 format**: Adds X25519 keys for end-to-end encryption (current)
- **Auto-migration**: v1/v2 keystores automatically upgrade to v2.1 on first unlock
- **TLS & X25519 persistence**: Certificates and encryption keys persisted to disk
- **Migration behavior**:
  1. v1 → v2.1: Generates `IdentityBundle` with TLS binding + X25519 keys
  2. v2 → v2.1: Reuses TLS binding, generates new X25519 keys
  3. Immediately saves upgraded keystore to disk with `encrypt_and_save()`
  4. Subsequent unlocks use persisted TLS and X25519 keys (stable across restarts)
  5. Log message: "✅ Successfully migrated and saved v2.1 keystore with persistent TLS binding and X25519 keys"
- **Test coverage**: `test_v1_to_v2_migration_persists_tls()` verifies persistence

**icnctl commands**:
```bash
icnctl id init           # Create new identity
icnctl id show           # Display current DID
icnctl id rotate         # Rotate to new keypair
icnctl id export backup.age
icnctl id import backup.age
```

## Metrics & Observability

- **Prometheus metrics** exposed on `http://localhost:9090/metrics`
- Metrics crate: `icn-obs`
- Key metrics:
  - `gossip_announces_sent`, `gossip_requests_sent`, `gossip_responses_sent`
  - `network_connections_active`, `network_connections_total`
  - `ledger_entries_total`, `ledger_entries_quarantined`
- Tracing with structured logging via `tracing` crate
- Initialize in supervisor: `icn_obs::init_metrics()`, `icn_obs::start_metrics_server(9090)`

## Current Phase

**Phase 12 - Economic Safety Rails (Complete ✓)** (2025-01-14):
- [x] Dynamic Credit Limits - Trust + history-based limit calculation
- [x] New Member Protection - Progressive ramping with contribution threshold
- [x] Dispute Resolution - Full lifecycle management (file, mediate, resolve)
- [x] Credit Policy Manager - Conservative/permissive presets
- [x] Dispute Manager - Persistent storage with mediation workflow
- [x] Economic Safety Documentation - Comprehensive guide with examples
- [x] All 10 tests pass (4 credit policy + 6 dispute resolution)

**Economic Safety Features**:
- **Dynamic Limits**: Formula-based limits (baseline + trust_bonus + history_bonus)
- **New Member Throttling**: 10h initial → 90-day ramp → full limit
- **Dispute System**: File disputes, add evidence, assign mediators, resolve
- **Write-offs**: Debt forgiveness mechanism for defaults
- **Multi-currency**: Separate policies per currency (hours, USD, kWh)

**Protection Against:**
- Free riders (low trust = low limits)
- "Grab and run" attacks (new member throttling)
- Credit limit gaming (history-based bonuses)
- Dispute abuse (mediator oversight)

---

**Phase 13 - Governance Primitives v1 (Complete ✓)** (2025-01-15):
- [x] Core Types - GovernanceDomain, Proposal, Vote, VoteTally (39 tests)
- [x] Governance Store - InMemoryGovernanceStore + GovernanceStore trait
- [x] Membership Resolution - StaticMembershipResolver + MembershipResolver trait
- [x] Gossip Protocol - 7 GovernanceMessage types for distributed coordination
- [x] GovernanceProfile - cooperative_default with quorum + approval evaluation
- [x] Documentation - Comprehensive governance.md (706 lines)
- [x] CLI Commands - `icnctl gov` for domain/proposal/vote management
- [x] Integration Test - Multi-node governance lifecycle validation

**Governance Features**:
- **Governance Substrate**: Democratic by default, configurable by communities, extensible via contracts
- **Proposal Types**: Text, Budget, Membership, ConfigChange payloads
- **Voting System**: For/Against/Abstain with optional weighted voting
- **Decision Profiles**: cooperative_default (1-member-1-vote, quorum + majority)
- **Membership Sources**: StaticList (explicit DIDs) or TrustThreshold (future)
- **Gossip Coordination**: DomainCreated, ProposalCreated, ProposalOpened, VoteCast, ProposalClosed messages

**CLI Commands**:
```bash
# Domain management
icnctl gov domain create --domain-id "coop:food" --name "Food Coop" --members "did:icn:alice,did:icn:bob"
icnctl gov domain list
icnctl gov domain show --domain-id "coop:food"

# Proposal lifecycle
icnctl gov proposal create --domain-id "coop:food" --title "Approve supplier" --kind text
icnctl gov proposal open --proposal-id <id>
icnctl gov proposal list --domain-id "coop:food" --state open
icnctl gov proposal close --proposal-id <id>

# Voting
icnctl gov vote cast --proposal-id <id> --choice for
icnctl gov vote show --proposal-id <id>
```

**Integration Test**:
- 3-node setup with gossip protocol
- Full proposal lifecycle: create → open → vote → close
- Distributed voting with convergence validation
- Outcome evaluation: 2 For, 1 Against = 66% = Accepted
- Test: `cargo test --test governance_integration -- --ignored`

**Next Steps:**
- Optional: Daemon Integration (governance actor in icnd)
- Track C1: Pilot Community Selection & Deployment

---

**Phase 14 - Gateway API (Complete ✓)** (2025-01-15):
- [x] REST API server with actix-web framework
- [x] JWT-based authentication with challenge-response flow
- [x] Cooperative namespace management (CRUD operations)
- [x] Ledger API (balances, payments, transaction history)
- [x] WebSocket real-time event streaming
- [x] Event broadcasting system with pub/sub
- [x] JWT middleware protecting all endpoints
- [x] All 30 tests pass

**Gateway Features**:
- **Authentication**: DID-based challenge-response → JWT tokens with configurable TTL
- **Cooperative Management**: Create/read/update/delete coops, member management, role assignments
- **Ledger Operations**: Query balances, create payments, view transaction history
- **Real-time Events**: WebSocket subscriptions to cooperative events (member added/removed, role updated, settings changed)
- **Security**: Bearer token authentication on all protected endpoints, token validation middleware

**API Endpoints**:
- **Public**: `/health`, `/auth/challenge`, `/auth/verify`, `/ws/{coop_id}`
- **Protected**: `/coops/*` (cooperative management), `/ledger/*` (ledger operations)

**Architecture** (`icn-gateway/`):
- **server.rs**: Actix-web HTTP server with middleware stack
- **auth.rs**: JWT token generation and verification, challenge-response protocol
- **middleware.rs**: Bearer token authentication middleware
- **coop.rs**: Cooperative state management (in-memory for Phase 14)
- **ledger_mgr.rs**: Ledger operations wrapper
- **events.rs**: Event broadcasting with tokio mpsc channels
- **websocket.rs**: WebSocket session management with JWT auth
- **api/**: REST endpoint handlers (auth, coops, ledger, websocket, health)
- **models.rs**: Request/response DTOs

**WebSocket Protocol**:
```rust
// Client → Server
{"type": "Auth", "token": "eyJ0eXAi..."}

// Server → Client
{"type": "AuthOk", "did": "did:icn:abc123"}
{"type": "Event", "MemberAdded": {"coop_id": "...", "did": "...", "role": "..."}}
{"type": "Error", "message": "..."}
```

**Security Model**:
- Challenge nonce expires after 5 minutes
- JWT tokens expire after 24 hours (configurable)
- Tokens scoped to cooperative ID + permissions
- WebSocket connections validate coop_id matches token
- All endpoints except auth/health require valid Bearer token

**Gateway Integration with icnd** (2025-01-15):
The gateway is integrated into the main ICN daemon and can be enabled via configuration:

```bash
# Method 1: Using configuration file
cat > icn.toml << EOF
[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
jwt_secret = "your-strong-secret-here"
token_expiry_hours = 24
challenge_ttl_minutes = 5
EOF

icnd --config icn.toml

# Method 2: Using CLI arguments
export ICN_GATEWAY_JWT_SECRET="your-strong-secret"
icnd --gateway-enable --gateway-bind 127.0.0.1:8080

# Method 3: Using environment variable only
export ICN_GATEWAY_JWT_SECRET="your-strong-secret"
icnd --gateway-enable

# Method 4: Standalone gateway (development/testing)
cargo run --bin icn-gateway -- --bind 127.0.0.1:8080 --jwt-secret mysecret
```

**Configuration Priority**: CLI args > Environment variables > Config file

**Security Notes**:
- Gateway disabled by default (opt-in)
- JWT secret must be configured for gateway to start
- Use strong random secrets (32+ characters) in production
- Localhost binding recommended for development
- Use reverse proxy (nginx/caddy) for production deployments

**API Usage**:
```bash
# Get challenge
curl -X POST http://localhost:8080/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did": "did:icn:abc123"}'

# Sign challenge and verify (returns JWT token)
curl -X POST http://localhost:8080/auth/verify \
  -H "Content-Type: application/json" \
  -d '{"did": "did:icn:abc123", "signature": "...", "coop_id": "my-coop", "scopes": ["ledger:read"]}'

# Use token for API calls
curl -H "Authorization: Bearer eyJ0eXAi..." \
  http://localhost:8080/coops/my-coop

# Connect WebSocket
wscat -c ws://localhost:8080/ws/my-coop
> {"type": "Auth", "token": "eyJ0eXAi..."}
```

---

**Track B1 - Operational Hardening (Complete ✓)** (2025-01-14):
- [x] Backup & Restore - `icnctl backup/restore` commands with encrypted tarballs (includes state.snapshot)
- [x] Monitoring Dashboard - Real-time web UI + health check endpoint
- [x] Incident Response Playbook - Comprehensive procedures for 7 major incident types
- [x] Operations Guide - Day-to-day workflows, command reference, troubleshooting
- [x] Protocol Version Validation - Automatic version checks with metrics
- [x] Graceful Restart - Production-ready state persistence (vector clocks, subscriptions, X25519 keys, ACL security, 11 Prometheus metrics)

**Advanced Features:**
- [x] Version Negotiation Handshake - Capability announcements (Complete ✓)
- [ ] Schema Migrations - `icnctl migrate` for data format changes

---

**Track B3 - Economic Modeling (Complete ✓)** (2025-01-14):
- [x] Agent-based simulation framework using Mesa 3.3.1
- [x] 5 behavioral agent types (reciprocator, hoarder, free rider, opportunist, super contributor)
- [x] 5 scenarios testing economic parameters (baseline, dynamic limits, demurrage, free riders, low trust)
- [x] ~13,000 transactions per scenario over 12 months
- [x] Comprehensive results analysis (velocity, defaults, inequality, hoarding)

**Key Findings:**
- **Dynamic Credit Limits**: -33% defaults, -16% velocity (stability vs growth tradeoff validated)
- **Demurrage**: -22% inequality (Gini), no velocity harm (highly effective redistribution)
- **Free-Rider Tolerance**: System stable up to 20% free-riders (4.1% defaults vs 2.7% baseline)
- **Trust Network Density**: Low density (30%) causes 2x hoarding vs high density (60%) - counterintuitive but validated

**Validated Defaults** (now in Phase 12):
- Credit limits: -20 initial → -500 max, +10 per 50 cleared, 2x trust multiplier
- Demurrage: -2% monthly on balances >50
- New member throttling: 3-month ramp, 10 credit contribution requirement

**Deliverables:**
- [sims/mutual-credit/](sims/mutual-credit/) - Complete framework (agents, economy, trust, model)
- [sims/mutual-credit/RESULTS_SUMMARY.md](sims/mutual-credit/RESULTS_SUMMARY.md) - Comprehensive analysis
- [docs/econ-modeling.md](docs/econ-modeling.md) - Updated with findings
- 5 JSON scenario configurations + analysis notebooks

---

**Version Negotiation Features (Complete ✓)** (2025-01-14):
- **VersionInfo Protocol**: Automatic exchange during Hello handshake with current/min/max protocol versions
- **CapabilityFlags**: 8 capability flags (E2E_ENCRYPTION, SIGNED_MESSAGES, GRACEFUL_RESTART, TOPOLOGY_AWARE, TRUST_RATE_LIMITING, GOSSIP_PULL, MULTI_DEVICE, ECONOMIC_SAFETY)
- **Per-Connection Tracking**: PeerConnectionInfo maintains negotiated version and peer capabilities
- **Capability-Based Feature Gating**: NetworkHandle API for querying peer capabilities and conditional feature usage
- **Backward Compatibility**: Legacy nodes (missing version_info) treated as version 1 with empty capabilities
- **Prometheus Metrics**: 5 metrics tracking version negotiation outcomes and capability distribution
- **Graceful Degradation**: Applications can fallback to basic features when advanced ones unavailable
- **Test Coverage**: 16 comprehensive tests covering negotiation, incompatibility detection, capability checking
- **Documentation**: Complete developer guide with patterns for feature gating ([capability-based-features.md](docs/capability-based-features.md))

**Operational Capabilities (Production Ready ✅)**:
- **Backup/Restore**: Encrypted backup bundles (keystore + store + config)
- **Monitoring**: Prometheus metrics + real-time dashboard (`:8080/`)
- **Health Checks**: JSON endpoint for external monitoring (`:8080/health`)
- **Incident Response**: 7 detailed procedures (node compromise, ledger corruption, key theft, etc.)
- **Operations Guide**: 800+ lines covering daily/weekly/monthly tasks, troubleshooting
- **Protocol Versioning**: Version validation prevents incompatible node communication
- **Graceful Restart**: Automatic state snapshots preserve vector clocks, topic subscriptions, and peer X25519 keys

**Graceful Restart Features**:
- **State Snapshot**: JSON snapshots saved to `{data_dir}/state.snapshot` on shutdown
- **Gossip State**: Vector clocks (causal ordering), topic subscriptions, topic metadata with ACL preservation
- **Network State**: Peer X25519 public keys (immediate encrypted communication after restart)
- **Security Hardened**: Fixed AccessControl::Participants data loss (private topics stay private)
- **Monitoring**: 11 Prometheus metrics (duration histograms, counters, gauges for state contents)
- **Backup Integration**: `icnctl backup/restore` includes state.snapshot automatically
- **Automatic**: Restore on startup, save on shutdown (via supervisor lifecycle)
- **Performance**: <10ms startup/shutdown overhead, single snapshot load optimized
- **Gossip Entries**: NOT persisted (fetched from peers via anti-entropy)
- **Network Connections**: NOT persisted (re-established via mDNS discovery within ~5s)
- **crates/icn-snapshot**: Standalone crate with zero dependencies (no circular deps)
- **Test Coverage**: 4 unit tests + 55 gossip tests + 2 integration tests + 5 backup tests

---

**Phase 11 - Multi-Device Identity & Sync (Complete ✓)** (2025-01-14):
- [x] DID Document v2 with multi-device support
- [x] VerificationMethod with capability-based permissions
- [x] RotationEvent chain for device lifecycle audit trail
- [x] Keystore v3 format with DID Document + automatic migration
- [x] `update_did_document()` method for atomic updates
- [x] CLI device management (list, add, approve, revoke)
- [x] Identity sync protocol via gossip (`identity:updates` topic)
- [x] DidDocumentCache for peer identity verification
- [x] All 33 tests pass (30 unit + 2 integration + 1 doc test)
- [x] Complete end-to-end workflow tested
- [x] Comprehensive design doc: `docs/multi-device-identity-design.md`

**Multi-Device Identity Features**:
- Single DID across multiple devices (laptop, phone, etc.)
- Per-device capabilities (Sign, AddDevice, RevokeDevice, RotateKey, Recover, Encrypt)
- Device revocation with timestamps and audit trail
- Gossip-based DID Document synchronization (280 bytes per update)
- Version-ordered cache prevents replay attacks

**Identity Sync Protocol**:
- `IdentityUpdateMessage` broadcasts rotation events
- `DidDocumentCache` maintains peer identity state
- NetworkActor verifies signatures against cached DID Documents
- Automatic version conflict resolution

---

**Phase 14 - Platform Layer (REST API Gateway) (Complete ✓)** (2025-01-15):
- [x] icn-gateway crate - Actix-web HTTP server
- [x] Authentication endpoints - Challenge/verify flow with JWT tokens
- [x] Cooperative namespace management - CRUD + member roles
- [x] Ledger API endpoints - Balance, payment, transaction history
- [x] Per-coop isolation - Separate ledgers per cooperative
- [x] WebSocket event streaming - Real-time updates for ledger/coop events
- [x] All 26 tests passing (9 auth + 5 coop + 5 ledger + 2 integration + 5 events/websocket)

**Gateway API (14 endpoints)**:
- **Authentication**: `POST /auth/challenge`, `POST /auth/verify`
- **Cooperatives**: 7 endpoints (create, get, update, delete, member CRUD)
- **Ledger**: `GET /ledger/:coop/balance/:did`, `POST /ledger/:coop/payment`, `GET /ledger/:coop/history`
- **WebSocket**: `GET /ws/:coop_id` (real-time event streaming)
- **Health**: `GET /health`

**Architecture**:
- **AuthManager**: DID-based challenge/verify with JWT capability tokens
- **CoopManager**: In-memory namespace storage (Owner/Admin/Member roles)
- **LedgerManager**: Per-coop mutual credit ledgers with SledStore backend
- **EventBroadcaster**: Pub/sub event distribution with per-coop isolation
- **WsSession**: WebSocket actor with heartbeat/ping-pong and automatic cleanup
- **Error Handling**: HTTP status mapping, JSON error responses
- **Middleware**: Logging, compression

**Event Types**: PaymentCreated, MemberAdded, MemberRemoved, RoleUpdated, SettingsUpdated

**This is NOT a runtime**: Apps run externally and call this API. See [docs/platform-layer-design.md](docs/platform-layer-design.md).

---

**What's Next**: See [ROADMAP.md](/ROADMAP.md) for strategic roadmap. Critical path:
- **Phase 13**: ✅ Governance Primitives v1 COMPLETE (CLI + integration test ready for pilot)
- **Track C1**: Pilot Community Selection & Deployment - NEXT PRIORITY
- **Track B2**: Legal & Regulatory Radar (lightweight, ongoing)
- **Track B3**: ✅ Economic Modeling COMPLETE (simulation validates Phase 12 defaults)

**Three-Layer Security Architecture (Production Ready ✅)**:
1. **Transport Layer**: QUIC/TLS with DID-TLS binding
2. **Message Layer**: SignedEnvelope with Ed25519 signatures + replay protection
3. **Application Layer**: EncryptedEnvelope with end-to-end encryption

---

**Phase 10 - End-to-End Payload Encryption (Complete ✓)** (2025-01-13):
- [x] EncryptedEnvelope with X25519-ChaCha20-Poly1305 AEAD encryption
- [x] X25519 keys added to IdentityBundle (generation + persistence)
- [x] Keystore v2.1 format with X25519 key storage and auto-migration
- [x] Bidirectional X25519 public key exchange via Hello protocol
- [x] NetworkActor stores and provides peer X25519 keys
- [x] Full encrypt → sign → send → receive → verify → decrypt flow
- [x] Network integration test validating complete message flow
- [x] All 261 tests pass (7 new encryption tests)

**Gossip Message Authentication (Complete ✓)** (2025-11-13):
- [x] Migrated all gossip messages to SignedEnvelope
- [x] Added sequence counter and keypair to GossipActor
- [x] Updated send_callback to create signed envelopes
- [x] Updated receive path to handle PayloadType::Gossip
- [x] All 262 library tests pass
- [x] First major protocol using Phase 9 infrastructure

**Critical Fix - TLS Certificate Persistence (Complete ✓)** (2025-11-13):
- [x] Fixed v1-to-v2 keystore migration to persist TLS certificates
- [x] Auto-save upgraded keystore immediately after generating TLS binding
- [x] Added comprehensive test: `test_v1_to_v2_migration_persists_tls()`
- [x] Restored Phase 8 security requirement: TLS certificates persist across restarts
- [x] All 19 icn-identity tests pass

**Phase 9 - Message & Identity Integrity (Complete ✓)** (2025-01-13):
- [x] SignedEnvelope with Ed25519 signatures (envelope.rs)
- [x] ReplayGuard with sequence tracking and Bloom filters (replay_guard.rs)
- [x] Protocol integration (MessagePayload::Signed)
- [x] NetworkActor automatic verification
- [x] Comprehensive test coverage (16 new tests, 261 total)

**Phase 8 - DID-TLS Binding & Keystore Integration (Complete ✓)** (2025-01-13):
- [x] IdentityBundle with persistent DID-TLS binding
- [x] Keystore v2 format with automatic migration
- [x] Runtime/Supervisor integration
- [x] DID-TLS binding verification tests

**Phase 7 - Polish & Production (Complete ✓)** (2025-01-11):
- [x] Metrics exporter (Prometheus)
- [x] Complete pull protocol (Request/Response)
- [x] Topic subscriptions & routing with notification callbacks
- [x] Production hardening (1 critical security + 7 robustness fixes)
- [x] Comprehensive test coverage (120+ tests)

**Production Hardening Completed (Latest Session - 2025-01-11)**:
1. **Network timeouts** - Added 30s dial, 10s send, 5s broadcast timeouts
2. **DID validation** - Comprehensive validation prevents panic on malformed DIDs
3. **Bounded growth** - Topic entry limits (default 1000) prevent memory exhaustion
4. **Compression** - zstd compression for entries >1KB reduces bandwidth
5. **Input sanitization** - Contract validation enforces limits on names, vars, rules, depth
6. **CRITICAL SECURITY FIX** - Expression depth validation prevents stack overflow bypass
7. **Ledger semantics** - Fixed inverted debit/credit in mutual credit transfers
8. **Test reliability** - Fixed timing-dependent test flakiness

**Earlier Production Hardening (2025-01-11)**:
- Fixed unbounded message allocation DoS
- Fixed blocking operations in async context
- Implemented TLS certificate verification (DID extraction + expiration)
- Fixed integer overflow in timestamp conversion
- Added Bloom filter validation (bounds checking)
- Implemented trust-gated network rate limiting (token bucket, trust-based limits)
- Added bounded QUIC stream limits (10 concurrent, 1MB/stream)

## Common Development Workflows

**Adding a new actor**:
1. Create actor struct with state
2. Implement message enum for actor operations
3. Create handle struct with `mpsc::Sender<Msg>`
4. Implement `spawn()` method that returns handle
5. Register with supervisor in `supervisor.rs`
6. Wire up communication with other actors via callbacks/channels

**Adding a new gossip topic**:
1. Define topic string (convention: `namespace:purpose`)
2. Configure `AccessControl` enum (Public, Private, TrustGated)
3. Subscribe in relevant actor: `gossip.subscribe(topic, access_control)`
4. Implement message serialization (use `bincode` or `serde_json`)
5. Set up notification callback via `set_notification_callback()` to receive new entries
6. Handle incoming messages in gossip actor's message handler

**Adding metrics**:
1. Define metric in `icn-obs/src/metrics/{module}.rs`
2. Register in `init_metrics()` function
3. Increment/observe at instrumentation points
4. Follow naming convention: `{actor}_{metric}_{unit}`

## Security & Production Hardening

**Network-level protections:**
- **Trust-gated rate limiting**: Different limits per trust class (token bucket algorithm)
  - **Isolated peers** (score < 0.1): 10 msg/sec, burst 2
  - **Known peers** (score 0.1-0.4): 50 msg/sec, burst 10
  - **Partner peers** (score 0.4-0.7): 100 msg/sec, burst 20
  - **Federated peers** (score 0.7+): 200 msg/sec, burst 50
  - Implementation: `icn-net/src/rate_limit.rs`
  - Dynamically adjusts when peer trust changes
  - Falls back to 100 msg/sec if trust graph unavailable
  - Metric: `icn_network_messages_rate_limited_total`
- **QUIC stream limits**: 10 concurrent streams, 1MB/stream window
  - Configuration: `icn-net/src/session.rs::create_transport_config()`
- **Message validation**: 10MB max, validated before allocation
  - Implementation: `icn-net/src/protocol.rs::read_message()`

**Protocol-level protections:**
- **Certificate verification**: DID extraction + expiration checks
  - Implementation: `icn-net/src/tls.rs::DidCertificateVerifier`
  - ⚠️ Trust graph integration pending (accepts all valid DIDs)
- **Bloom filter validation**: Bounds checking on deserialization
  - Implementation: `icn-gossip/src/bloom.rs::from_data()`
- **Timestamp overflow protection**: Checked u128 → u64 conversion
  - Applied in ledger entries and gossip messages

**Runtime protections:**
- **Async-safe operations**: No `blocking_*` calls in Tokio runtime
  - All message handlers use `tokio::spawn` for writes
- **Error handling**: Result types with context, no panics in protocol code
- **Graceful degradation**: Malformed data logged and dropped, not panicked

**Security metrics** (Prometheus):
- `icn_network_messages_rate_limited_total` - Attack detection
- `icn_network_connections_total` - Connection monitoring
- `icn_gossip_*_received_total` - Protocol health

**Known limitations:**
- ⚠️ TLS verifier does NOT integrate with trust graph yet
- Currently in "development mode" - accepts all valid DID certificates
- Trust graph integration required before production deployment

See [docs/production-hardening.md](docs/production-hardening.md) for complete details.

## Notes

- The daemon requires an unlocked keystore to spawn actors (passphrase prompt on startup)
- All actor handles use interior mutability (`Arc<RwLock<T>>` or message passing)
- Shutdown propagates via `tokio::sync::broadcast` channel
- Integration tests should use unique ports per node (avoid bind conflicts)
- Vector clocks prevent duplicate processing of gossip messages
