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
- `icn-testkit` - Test utilities for multi-node scenarios

**Binaries** (in `icn/bins/`):
- `icnd` - The ICN daemon
- `icnctl` - CLI management tool

## Documentation Structure

**Project root `/home/matt/projects/icn/`:**
- `CLAUDE.md` - This file; guidance for Claude Code when working on the project
- `README.md` - Project overview and quick start guide for users
- `CHANGELOG.md` - Formal, user-facing changelog following Keep a Changelog format

**Documentation directory `/home/matt/projects/icn/docs/`:**
- `ARCHITECTURE.md` - System architecture, component design, and implementation details
- `production-hardening.md` - Security hardening measures and vulnerability fixes
- `deployment-guide.md` - Installation, configuration, monitoring, and operations
- `topic-subscriptions-api.md` - API reference for gossip subscriptions
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
- Payload types: `Gossip`, `Rpc`, `Custom`
- Length-prefixed framing over QUIC streams
- TLS certificates derived from DID Ed25519 keys

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

**Phase 7 - Polish & Production (Complete ✓)**:
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
