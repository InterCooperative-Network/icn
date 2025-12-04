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

## Live Deployment (Faherty Homelab)

**Status**: ICN daemon is running on a K3s cluster deployed 2025-12-03.

| Component | Details |
|-----------|---------|
| **Node Identity** | `did:icn:z3TE1ei6B4L5j6Jp29RmJKt1FYonGaQAXQoYHJL3GULR3` |
| **K3s Control** | `k3s-control` (10.8.10.40) |
| **Workers** | `k3s-worker-1` (10.8.10.41), `k3s-worker-2` (10.8.10.42) |
| **Storage** | NFS from Atlas (10.8.10.25) via `atlas-nfs` StorageClass |
| **Ports** | 7777/UDP (QUIC), 5601/TCP (RPC), 9100/TCP (Prometheus) |

### Quick Access Commands

```bash
# Check cluster and pod status
ssh ubuntu@10.8.10.40 "sudo kubectl get nodes"
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods"

# View ICN daemon logs
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -l app=icn"

# Show identity
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn exec deploy/icn-daemon -- /usr/local/bin/icnctl id show"

# Access metrics (via port-forward)
ssh ubuntu@10.8.10.40 "kubectl -n icn port-forward svc/icn 9100:9100 &" && curl http://localhost:9100/metrics

# Or view in Grafana (deployed 2025-12-04)
# URL: http://10.8.10.40:30300
# Dashboard: ICN Node Dashboard
# Credentials: See K8s secret `prometheus-grafana` in monitoring namespace
```

### Related Homelab Documentation

| Resource | Location |
|----------|----------|
| **Homelab Inventory** | `/home/matt/homelab-inventory` |
| **ICN Launchpad** | `/home/matt/homelab-inventory/projects/icn/ICN_LAUNCHPAD.md` |
| **K3s Cluster Docs** | `/home/matt/homelab-inventory/projects/icn/docs/K3S_CLUSTER.md` |
| **Deployment Plans** | `/home/matt/homelab-inventory/projects/icn/docs/DEPLOYMENT_PLANS.md` |

### Deployment History

The K3s deployment required fixes for several issues discovered during initial bringup:
1. **GLIBC compatibility** - Required Ubuntu 24.04 base image (not Debian)
2. **STUN port conflict** - Disabled STUN to avoid binding same port as QUIC
3. **Governance topic** - Patched `supervisor.rs` to create topic before GovernanceActor spawn
4. **Memory limit** - Increased to 2Gi for age keystore unlock
5. **Health probe** - Changed to use port 9100 (metrics port)

The governance topic fix has been merged upstream (commit `01009e5`).

### Monitoring Stack (deployed 2025-12-04)

| Component | Access | Notes |
|-----------|--------|-------|
| **Grafana** | http://10.8.10.40:30300 | ICN Node Dashboard (creds in K8s secret) |
| **Prometheus** | K3s internal only | Scrapes ICN metrics every 15s |
| **AlertManager** | K3s internal only | 15 ICN-specific alerts configured |

**K8s Resources**:
- `ServiceMonitor` in `icn` namespace → Prometheus scrapes ICN daemon
- `PrometheusRule` in `monitoring` namespace → ICN alert rules

**Files**: [deploy/k8s/monitoring/servicemonitor.yaml](deploy/k8s/monitoring/servicemonitor.yaml)

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
- `icn-compute` - Distributed compute layer with trust-gated task execution (Phase 15)
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

The dev journal contains detailed, chronological records of development sessions.

**Full style guide**: See [docs/DOCUMENTATION_STYLE.md](docs/DOCUMENTATION_STYLE.md) for complete formatting standards.

**Quick reference:**

All entries must include YAML frontmatter:
```yaml
---
date: 2025-12-03
title: "Phase 17: Storage Replication"
type: dev-journal
phase: 17
topics: [storage, replication]
status: complete
duration: ~4 hours
---
```

**When to create a new entry:**
- Starting work on a new phase or major feature
- After completing significant work
- When making architectural decisions
- After resolving complex bugs

**Naming convention:** `YYYY-MM-DD-phase-N-feature-name.md`

**What to include:**
- Phase/feature overview and goals
- Implementation approach with file links (e.g., `[file.rs:L1-L100](path#L1-L100)`)
- Challenges encountered and solutions
- Test results and validation
- Next steps (use `- [ ]` / `- [x]` checkboxes)

**Distinction from CHANGELOG:**
- **Dev journal**: Detailed, developer-focused narrative with context
- **CHANGELOG**: Concise, user-facing list following Keep a Changelog format

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

- **Prometheus metrics** exposed on `http://localhost:9095/metrics`
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

**Phase 14 - Gateway API (Complete ✓)** (2025-01-15, Production Hardening: 2025-11-16, Governance API: 2025-11-17):
- [x] REST API server with actix-web framework
- [x] JWT-based authentication with challenge-response flow
- [x] Cooperative namespace management (CRUD operations)
- [x] Ledger API (balances, payments, transaction history)
- [x] Governance API (domains, proposals, voting) with WebSocket events (2025-11-17)
- [x] WebSocket real-time event streaming
- [x] Event broadcasting system with pub/sub
- [x] JWT middleware protecting all endpoints
- [x] API versioning (/v1 namespacing)
- [x] Per-DID rate limiting (token bucket algorithm)
- [x] Scope-based authorization enforcement
- [x] Authenticated DID extraction for ownership
- [x] All 77 tests pass (19 governance + 58 other) - includes TOCTOU, duplicate ID prevention, overflow protection, whitespace validation tests

**Gateway Features**:
- **Authentication**: DID-based challenge-response → JWT tokens with configurable TTL
- **Authorization**: Scope-based access control (ledger:read, ledger:write, coop:read, coop:write, coop:admin, gov:read, gov:write)
- **Rate Limiting**: Per-DID token bucket (100 burst, 10/sec refill) prevents abuse
- **Cooperative Management**: Create/read/update/delete coops, member management, role assignments
- **Ledger Operations**: Query balances, create payments, view transaction history
- **Governance Operations**: Create domains/proposals, open voting, cast votes, close proposals with outcome calculation
- **Real-time Events**: WebSocket subscriptions to cooperative/governance events (domains, proposals, votes, member changes, settings)
- **API Versioning**: All endpoints under /v1 scope for backward compatibility
- **Security**: Three-layer security (auth → rate limiting → authorization)

**API Endpoints**:
- **Public**: `/health`, `/auth/challenge`, `/auth/verify`, `/ws/{coop_id}`
- **Protected**: `/coops/*` (cooperative management), `/ledger/*` (ledger operations), `/gov/*` (governance operations)

**Architecture** (`icn-gateway/`):
- **server.rs**: Actix-web HTTP server with /v1 public/protected scopes and middleware composition
- **auth.rs**: JWT token generation and verification, challenge-response protocol
- **middleware.rs**: JWT authentication middleware + authorization helpers (require_scope, get_claims)
- **rate_limit.rs**: Token bucket rate limiter with per-DID tracking and automatic cleanup
- **coop.rs**: Cooperative state management (in-memory for Phase 14)
- **ledger_mgr.rs**: Ledger operations wrapper
- **governance_mgr.rs**: Governance operations wrapper (in-memory storage, domains/proposals/votes, proper evaluation with quorum + approval thresholds)
- **events.rs**: Event broadcasting with tokio mpsc channels (cooperative + governance events)
- **websocket.rs**: WebSocket session management with JWT auth
- **api/**: REST endpoint handlers (auth, coops, ledger, governance, websocket, health) with scope enforcement
- **models.rs**: Request/response DTOs
- **validation.rs**: Input validation (domain IDs, names, etc.)
- **error.rs**: GatewayError types with HTTP status mapping (401, 403, 429, etc.)

**WebSocket Protocol**:
```rust
// Client → Server
{"type": "Auth", "token": "eyJ0eXAi..."}

// Server → Client
{"type": "AuthOk", "did": "did:icn:abc123"}
{"type": "Event", "MemberAdded": {"coop_id": "...", "did": "...", "role": "..."}}
{"type": "Error", "message": "..."}
```

**Security Model** (Three-Layer Architecture):
1. **Authentication Layer** (JWT middleware):
   - DID-based challenge-response flow
   - Challenge nonce expires after 5 minutes
   - JWT tokens expire after 24 hours (configurable)
   - Bearer token validation on protected endpoints
   - Inserts TokenClaims into request extensions

2. **Rate Limiting Layer** (per-DID middleware):
   - Token bucket algorithm (100 burst capacity, 10 tokens/sec refill)
   - Independent limits per authenticated DID
   - Prevents API flooding and abuse
   - Returns HTTP 429 when limit exceeded

3. **Authorization Layer** (handler-level):
   - Scope-based access control
   - Fine-grained permissions (read/write/admin)
   - Prevents privilege escalation
   - Returns HTTP 403 when scope missing

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

**Phase 14 - Platform Layer (REST API Gateway) (Complete ✓)** (2025-01-15, Production Hardening: 2025-11-16):
- [x] icn-gateway crate - Actix-web HTTP server
- [x] Authentication endpoints - Challenge/verify flow with JWT tokens
- [x] Cooperative namespace management - CRUD + member roles
- [x] Ledger API endpoints - Balance, payment, transaction history
- [x] Per-coop isolation - Separate ledgers per cooperative
- [x] WebSocket event streaming - Real-time updates for ledger/coop events
- [x] API versioning - /v1 namespacing for backward compatibility
- [x] Per-DID rate limiting - Token bucket algorithm (100 burst, 10/sec refill)
- [x] Scope-based authorization - All protected endpoints enforce JWT scopes
- [x] Authenticated DID extraction - Cooperative owners use real JWT claims
- [x] All 38 tests passing (5 rate limiting + 2 authorization + 1 ownership + 30 existing)

**Gateway API (16 endpoints under /v1)**:
- **Authentication**: `POST /v1/auth/challenge`, `POST /v1/auth/verify`
- **Cooperatives**: 7 endpoints under `/v1/coops/*` (create, get, update, delete, member CRUD)
- **Ledger**: `GET /v1/ledger/:coop/balance/:did`, `POST /v1/ledger/:coop/payment`, `GET /v1/ledger/:coop/history`
- **Compute**: `POST /v1/compute/submit`, `GET /v1/compute/status/:task_hash`
- **WebSocket**: `GET /v1/ws/:coop_id` (real-time event streaming)
- **Health**: `GET /v1/health`

**Architecture**:
- **AuthManager**: DID-based challenge/verify with JWT capability tokens
- **CoopManager**: In-memory namespace storage (Owner/Admin/Member roles)
- **LedgerManager**: Per-coop mutual credit ledgers with SledStore backend
- **ComputeManager**: Distributed task submission and status tracking
- **EventBroadcaster**: Pub/sub event distribution with per-coop isolation
- **RateLimiter**: Token bucket per-DID rate limiting with automatic cleanup
- **WsSession**: WebSocket actor with heartbeat/ping-pong and automatic cleanup
- **Error Handling**: HTTP status mapping (401, 403, 429), JSON error responses
- **Middleware**: JWT auth, rate limiting, logging, compression
- **Authorization**: Scope-based access control (ledger:read/write, coop:read/write/admin, compute:read/write)

**Event Types**: PaymentCreated, MemberAdded, MemberRemoved, RoleUpdated, SettingsUpdated, GovernanceDomainCreated, GovernanceProposalCreated, GovernanceProposalOpened, GovernanceProposalClosed, GovernanceVoteCast

**This is NOT a runtime**: Apps run externally and call this API. See [docs/platform-layer-design.md](docs/platform-layer-design.md).

---

**Phase 15 - Distributed Compute Layer (Complete ✓)** (2025-11-21):
- [x] `icn-compute` crate with trust-gated task execution
- [x] ComputeTask/ComputeResult types for task lifecycle
- [x] TaskManager for tracking states (Pending/Claimed/Completed)
- [x] LocalExecutor with real CCL interpreter integration
- [x] ComputeActor with gossip-based task distribution
- [x] Payment settlement via ledger (auto-pay on success)
- [x] Supervisor integration with trust/gossip/ledger callbacks
- [x] RPC endpoints (compute.submit, compute.status)
- [x] CLI commands (icnctl compute submit/status)
- [x] Gateway REST API (/v1/compute/submit, /v1/compute/status)
- [x] Ed25519 signature signing and verification for all compute results
- [x] Automatic signing in production (configured via supervisor)
- [x] Prometheus metrics for signature verification
- [x] Executor registry tracking (capabilities, trust, last_seen)
- [x] Consensus framework for multi-executor verification (single-executor mode)
- [x] Comprehensive input validation (task ID, DID format, fuel limits, code size, payment rate)
- [x] Gateway-level validation with proper error messages
- [x] Structured logging with tracing (DEBUG/INFO/WARN levels)
- [x] Task cancellation with submitter authorization and gossip propagation
- [x] 41 compute tests + 92 gateway tests + 25 RPC tests passing (11 validation + 6 gateway validation + 4 signature + 2 consensus + 6 cancellation + 1 priority)

**Production Enhancements** (2025-11-21):
- [x] **Cancellation Metrics**: `tasks_cancelled_inc()` tracked at actor/gateway/network levels
- [x] **Executor Load Tracking**: Per-executor gauge `icn_compute_executor_load{executor}` updated at 6 lifecycle points
- [x] **Automatic Timeout Enforcement**: Background task scans every 10s, auto-fails expired tasks, broadcasts timeout results
- [x] **Load-Based Capacity Control**: Configurable `max_concurrent_tasks` (default: 10), capacity checks before claiming, `tasks_rejected_capacity_total` metric
- [x] **Task Priority Levels**: TaskPriority enum (Low=0, Normal=1, High=2, Critical=3) with PartialOrd for natural comparison
- [x] **Priority-Based Claiming**: `TaskManager::pending_by_priority()` sorts by priority desc → created_at asc, executor claims highest-priority task
- [x] **User-Facing Priority API**: Priority support in Gateway REST API, RPC, CLI (`--priority`), and TypeScript SDK
- [x] **WebSocket Event Broadcasting**: Shared EventBroadcaster between supervisor and gateway enables real-time event delivery to WebSocket clients in production
- [x] **Event Flow Integration**: Compute actor → EventBroadcaster → Gateway WebSocket connections → Clients receive TaskClaimed/TaskCompleted events in real-time

**Compute Features**:
- **Trust-Gated Execution**: MIN_TRUST_SUBMIT (0.1), MIN_TRUST_EXECUTE (0.3)
- **Gossip Topics**: `compute:submit`, `compute:claim`, `compute:result`, `compute:cancel`
- **CCL Execution**: Real contract parsing and interpreter execution
- **Payment Settlement**: (fuel_used * payment_rate) / 1000 credits
- **Task Cancellation**: Submitter-only authorization, only pending/claimed tasks cancellable
- **Cryptographic Security**: Ed25519-signed results with DID-based verification
- **Executor Registry**: Tracks available executors with capabilities, trust scores, last_seen timestamps, and current load
- **Consensus Framework**: Multi-executor result verification (currently single-executor mode, extensible to multi-executor)
- **Input Validation**: Comprehensive checks (task ID length, DID format, fuel min/max, code size limits, payment rate caps)
- **Structured Logging**: Tracing spans with structured fields (task_id, task_hash, executor, fuel_used, duration_ms, outcome, priority)
- **Task Prioritization**: Four priority levels with preferential execution (Critical > High > Normal > Low)
- **Automatic Timeouts**: Background checker auto-fails tasks past deadline, prevents stuck tasks
- **Capacity Management**: Per-executor concurrency limits prevent overload

**CLI Commands**:
```bash
# Submit a CCL contract for distributed execution
icnctl compute submit --contract contract.json --fuel 10000

# With priority (low, normal, high, critical)
icnctl compute submit --contract contract.json --priority high

# With payment rate (credits per 1000 fuel) and priority
icnctl compute submit --contract contract.json --payment-rate 100 --priority critical

# Check task status
icnctl compute status <task_hash>

# Cancel a task
icnctl compute cancel <task_hash> --reason "No longer needed"
```

**RPC Methods**:
- `compute.submit` - Submit task, returns task_hash
- `compute.status` - Get task status (pending/claimed/completed/failed/cancelled)
- `compute.cancel` - Cancel a task with optional reason (submitter-only authorization)

**Gateway REST API**:
- `POST /v1/compute/submit` - Submit task (requires compute:write scope)
- `GET /v1/compute/status/{task_hash}` - Get task status (requires compute:read scope)
- `POST /v1/compute/cancel/{task_hash}` - Cancel task with optional reason (requires compute:write scope)

**TypeScript SDK**:
- `client.submitTask(req)` - Submit compute task
- `client.getTaskStatus(taskHash)` - Get task status
- `client.cancelTask(taskHash, req?)` - Cancel task
- `client.waitForTask(taskHash)` - Poll until completed/failed/cancelled

**Task Flow**:
```
Submitter → compute:submit → Executor claims → Executes CCL
                                                    ↓
                         compute:result ← Signed result
                                                    ↓
                                         Payment → Ledger
```

**Architecture** (`icn-compute/`):
- **types.rs**: ComputeTask, ComputeResult, ComputeMessage, TaskCode (CCL/WASM)
- **task.rs**: TaskManager with lifecycle tracking (Pending→Claimed→Completed/Failed/Cancelled)
- **executor.rs**: Executor trait, LocalExecutor (CCL), future WasmExecutor
- **actor.rs**: ComputeActor with gossip/trust/payment/event callbacks + cancellation support
- **error.rs**: ComputeError types

**Event Broadcasting**:
The compute layer broadcasts task status events via the EventCallback:
- **ComputeEvent::TaskClaimed** - When an executor claims a task
- **ComputeEvent::TaskCompleted** - When task execution finishes (with outcome, fuel_used, duration_ms)

Events are broadcast at two points:
1. Local execution: When this node's executor claims and completes tasks
2. Remote execution: When accepting verified results from other executors

The supervisor sets up the event callback for logging and metrics.

**WebSocket Integration Pattern** (Automatically Configured in Production):
The supervisor automatically sets up event broadcasting when both compute actor and gateway are enabled:

1. **Supervisor creates shared EventBroadcaster** when compute actor spawns
2. **Compute event callback** logs + updates metrics + forwards to EventBroadcaster
3. **Gateway receives EventBroadcaster** via `new_with_broadcaster()` constructor
4. **WebSocket clients** connect to `/ws/{coop_id}` and receive real-time events

The `icn-gateway/compute_events.rs` module provides the integration utilities:

```rust
// In supervisor (automatic):
let broadcaster = Arc::new(EventBroadcaster::new());
let callback: EventCallback = Arc::new(move |event| {
    // Log + metrics
    // ...

    // Forward to WebSocket clients
    let b = broadcaster.clone();
    tokio::spawn(async move {
        icn_gateway::forward_compute_event(&b, event).await;
    });
});
compute_actor.set_event_callback(callback);

// Gateway receives shared broadcaster
let gateway = GatewayServer::new_with_broadcaster(addr, jwt, data_dir, broadcaster);
```

**WebSocket clients** subscribe to "compute" channel and receive:
- **ComputeTaskClaimed** - When executors claim tasks
- **ComputeTaskCompleted** - When execution finishes (with outcome, fuel_used, duration_ms)

This enables real-time task monitoring for web/mobile applications via WebSocket connections.

**Prometheus Metrics** (`icn_compute_*`):
- `tasks_submitted_total`, `tasks_claimed_total`, `tasks_completed_total`
- `tasks_pending`, `tasks_executing`, `executors_available` (gauges)
- `task_duration_seconds`, `fuel_used` (histograms)
- `fuel_total`, `payments_settled_total`, `payment_amount_total`
- `tasks_rejected_trust_total`, `tasks_timeout_total`, `tasks_out_of_fuel_total`
- `signatures_verified_total`, `signatures_invalid_total` (by reason: invalid_did, verification_failed)

**WebSocket Events** (subscribe to "compute" channel):
- `ComputeTaskSubmitted` - Task created with task_id, task_hash, submitter, fuel_limit
- `ComputeTaskClaimed` - Executor claimed task
- `ComputeTaskCompleted` - Task finished with outcome, fuel_used, duration_ms
- `ComputeTaskCancelled` - Task cancelled by submitter with reason

---

**Phase 16 - Scheduler Evolution (Complete ✓)** (2025-11-23 to 2025-11-24):
Five-phase incremental evolution from reactive task claiming to intelligent, trust-governed scheduling:

- **Phase 16A**: Resource Profiles & Matching (CPU/RAM/GPU requirements + capacity tracking)
- **Phase 16B**: Placement Scoring (multi-factor scoring: trust 40%, capacity 30%, queue 20%, jitter 10%)
- **Phase 16C**: Locality Awareness (network topology + data locality for "compute goes to data" optimization)
- **Phase 16D**: Actor State & Migration (stateful actors with checkpoint-based fault tolerance)
- **Phase 16E**: Cooperative Policies (per-coop quotas, rules, enforcement modes)

**Deliverables**:
- 98 tests passing across all compute features
- Complete CLI (`icnctl policy`, `icnctl quota`) and RPC interface (`policy.*`, `quota.*` methods)
- 6 example policies with comprehensive documentation (800+ lines)
- PolicyManager with 8 rule types (DataSovereignty, TimeWindow, MemberPriority, RequireCapability, ExecutorFilter, etc.)
- Placement scoring integrates trust, capacity, network latency, data locality, and user hints
- Full scheduler stack ready for pilot deployment

**Spec Impact**: Updates [ARCHITECTURE.md Section 5](docs/ARCHITECTURE.md#5-compute-layer) (Compute Layer)

See [docs/scheduler-evolution-plan.md](docs/scheduler-evolution-plan.md) for complete 8,800+ word design document.

---

**What's Next**: See [ROADMAP.md](/ROADMAP.md) (ICN-DEP-ROADMAP-01) for complete strategic roadmap. Critical path:
- **Phase 16**: ✅ Scheduler Evolution (16A-E) COMPLETE - Intelligent placement, locality awareness, cooperative policies (2025-11-24)
- **Phase 17**: ✅ Storage Hardening & Replication COMPLETE - Trust-weighted replica selection, ReplicationManager actor, 16 Prometheus metrics, 99.9% durability (2025-12-04)
- **Phase 18**: ✅ Pre-Pilot Hardening COMPLETE - Byzantine fault detection (7 violation types), trust graph integration, Grafana dashboard, 16 tests passing, 10 weeks ahead of schedule (2025-12-04)
- **Track C1**: Pilot Community Selection - **NEXT PRIORITY**
  - Select pilot cooperatives, deploy monitoring, collect metrics
- **Phase 19+**: Post-pilot improvements (persistent reputation, cross-node sync, manual moderation)

**Pilot Readiness**: ✅ **READY** - Byzantine fault-tolerant infrastructure operational with comprehensive monitoring

**Three-Layer Security Architecture (Production Ready ✅)**:
1. **Transport Layer**: QUIC/TLS with DID-TLS binding
2. **Message Layer**: SignedEnvelope with Ed25519 signatures + replay protection
3. **Application Layer**: EncryptedEnvelope with end-to-end encryption

---

**Phase 18 - Pre-Pilot Hardening (Complete ✓)** (2025-12-04):
- [x] MisbehaviorDetector with 7 violation types (InvalidSignature, ConflictingLedgerEntries, FailedComputeVerification, ExcessiveResourceUse, TrustGraphSpam, ConflictingSignedStatements, ReplayAttack)
- [x] Reputation system (0.0-1.0 score, 0.05×severity penalty, 0.01/hour decay)
- [x] Automatic quarantine (score < 0.5) and auto-ban (critical violations)
- [x] NetworkActor Byzantine detection (InvalidSignature, ReplayAttack)
- [x] Ledger fork conflict detection (ConflictingLedgerEntries)
- [x] Compute verification failure detection (FailedComputeVerification)
- [x] Trust graph integration (automatic trust penalty on misbehavior)
- [x] Prometheus metrics (7 metrics: violations, quarantines, bans, auto-bans)
- [x] Grafana dashboard (5 panels for Byzantine fault monitoring)
- [x] Comprehensive tests (8 integration + 8 unit tests, all passing)
- [x] All 785 workspace tests passing

**Byzantine Detection Features**:
- **Violation Severity**: Critical (10) → auto-ban, Major (5) → warnings, Minor (1) → tracked
- **Rate Limiting**: Max 10 violations/hour, exceeding triggers quarantine
- **Trust Penalty Mapping**: Reputation < 0.5 → Trust × 0.2 (aggressive penalty)
- **Attack Resistance**: Sybil, fork, replay, signature forgery, Byzantine consensus, DoS, trust manipulation
- **Performance**: <0.1% CPU overhead, 200 KB memory per 1000 peers

**Integration Points**:
- **NetworkActor** (`icn-net/src/actor.rs:1913-1991`): Signature verification + replay detection
- **GossipActor** (`icn-gossip/src/gossip.rs:632,677,712`): ACL violations + subscriber limits
- **Ledger** (`icn-ledger/src/ledger.rs:500-528`): Fork conflict detection
- **ComputeActor** (`icn-compute/src/actor.rs:1501-1523`): Result verification failures
- **Supervisor** (`icn-core/src/supervisor.rs:148-191`): Trust penalty callback

**Operational**:
- Grafana dashboard: `monitoring/grafana-dashboard.json` (5 panels)
- Metrics endpoint: `http://localhost:9095/metrics`
- Alert queries documented in `/tmp/PHASE_18_COMPLETE.md`

**Status**: System is PILOT-READY with Byzantine fault tolerance operational

---

**Internal Testing Infrastructure (Complete ✓)** (2025-12-04):

Phase 18 completion triggered creation of comprehensive internal testing infrastructure for multi-node validation before pilot deployment.

**Infrastructure Components**:
- [x] Docker Compose 4-node test network (3 honest + 1 Byzantine)
- [x] Monitoring stack (Prometheus + Grafana)
- [x] 25 alert rules across 8 categories
- [x] 38 test scenarios documented
- [x] Complete documentation suite (3 guides)
- [x] All Docker configuration issues resolved (5 commits)

**Test Scenarios (38 total)**:
- **Baseline** (10): Network formation, trust graph sync, gossip propagation, ledger transactions, compute tasks, governance (4 scenarios)
- **Byzantine Detection** (10): Invalid signatures, replay attacks, ledger forks, compute forgery, ACL violations, governance attacks (4 scenarios)
- **Performance** (6): Gossip throughput (1000 msg/sec), ledger volume (50 tx/sec), compute queue (500 tasks), governance load (100 proposals/300 votes), 24-hour soak test
- **Resilience** (5): Node crash recovery, partition healing, Byzantine recovery, disk full, monitoring failure
- **Operational** (4): Backup/restore, version upgrade, security incident response, capacity planning

**Governance Testing (9 scenarios integrated)**:
- Domain creation & sync (Baseline 1.6)
- Proposal lifecycle - majority (Baseline 1.7)
- Proposal lifecycle - quorum failure (Baseline 1.8)
- WebSocket events (Baseline 1.9)
- Vote manipulation attack (Byzantine 2.7)
- Double voting attack (Byzantine 2.8)
- Proposal spam (Byzantine 2.9)
- Conflicting outcomes under partition (Byzantine 2.10)
- Load test - 100 proposals, 300 votes (Performance 3.5)

**Docker Services**:
- **node1, node2, node3**: Honest ICN nodes
- **node4**: Byzantine node (optional, `--profile byzantine`)
- **prometheus**: Metrics collection (port 9090)
- **grafana**: Visualization (port 3000)

**Monitoring**:
- **60+ Prometheus metrics** tracked
- **25 alert rules** (Byzantine, network, ledger, gossip, compute, governance, system, monitoring)
- **Grafana dashboard** with auto-provisioning
- **Health checks** for all containers

**Documentation**:
- [`docs/INTERNAL_TESTING_PLAN.md`](docs/INTERNAL_TESTING_PLAN.md) (1,000+ lines) - Complete test plan with success criteria
- [`docs/TESTING_QUICKSTART.md`](docs/TESTING_QUICKSTART.md) (500+ lines) - Quick start guide with manual test procedures
- [`DEPLOY_TEST_NETWORK.md`](DEPLOY_TEST_NETWORK.md) (400+ lines) - Host system deployment instructions

**Quick Start** (on host system):
```bash
# Build image
docker build -t icn:latest -f Dockerfile icn/

# Start 3-node network
docker compose -f docker-compose.test.yml up -d

# Verify
curl http://localhost:9091/metrics | grep icn_network_connections_active
# Expected: icn_network_connections_active 2

# Access monitoring
# Grafana: http://localhost:3000 (dev default creds - change for prod)
# Prometheus: http://localhost:9095
```

**Docker Fixes Applied (5 commits)**:
1. Build context mismatch - Fixed COPY paths
2. Missing keystore passphrase - Added ICN_PASSPHRASE env var
3. Unsupported env vars - Switched to CLI arguments
4. Unsupported --bind argument - Removed from all nodes
5. Kubernetes files cleanup - Added to .gitignore

**Security Warning**:
Test environment uses hardcoded passphrase `test-passphrase-insecure-do-not-use-in-production` **ONLY for internal testing on isolated networks**. Production deployments require secure secrets management.

**Success Criteria** (Go/No-Go):
- [ ] All 38 test scenarios pass
- [ ] No crashes or panics in 24-hour soak test
- [ ] Byzantine nodes detected within 1 min SLA
- [ ] Governance voting works correctly (no vote loss)
- [ ] No false positives (honest nodes never quarantined)
- [ ] Ledger consistency maintained (no undetected forks)
- [ ] Network recovers from partitions <2 min
- [ ] Stable memory usage (<2 GB/node, no leaks)

**Timeline**:
- Week 1 (Days 1-5): Baseline tests + performance baselines
- Week 2 (Days 6-12): Byzantine + governance + resilience + operational tests + 24-hour soak test
- **Go/No-Go Decision** at end of Week 2
- If Go: Proceed to Track C1 (Pilot Community Selection)
- If No-Go: Fix issues and re-test

**Current Status**: Infrastructure complete and ready for deployment on host system

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

**Trust-gated TLS verification:**
- ✅ TLS verifier integrates with trust graph for access control
- Configurable `min_trust_threshold` (default: 0.0 = accept all authenticated DIDs)
- Production recommendation: Set threshold ≥ 0.1 to reject isolated peers
- Metric: `icn_network_connections_rejected_untrusted_total`

See [docs/production-hardening.md](docs/production-hardening.md) for complete details.

## Notes

- The daemon requires an unlocked keystore to spawn actors (passphrase prompt on startup)
- All actor handles use interior mutability (`Arc<RwLock<T>>` or message passing)
- Shutdown propagates via `tokio::sync::broadcast` channel
- Integration tests should use unique ports per node (avoid bind conflicts)
- Vector clocks prevent duplicate processing of gossip messages
