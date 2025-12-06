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

## Live Deployment

ICN daemon running on K3s cluster (deployed 2025-12-03). See **[docs/HOMELAB_DEPLOYMENT.md](docs/HOMELAB_DEPLOYMENT.md)** for:
- Cluster details and node identity
- Quick access commands
- CI/CD pipeline and monitoring
- Pilot testing status

**Quick Commands**:
```bash
cd deploy/k8s && make full-deploy-dev  # Deploy new version
make status                              # Check pod status
make logs                                # View logs
```

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
- `icn-gateway` - REST + WebSocket API for cooperative applications
- `icn-governance` - Governance primitives for community decision-making
- `icn-compute` - Distributed compute layer with trust-gated task execution
- `icn-testkit` - Test utilities for multi-node scenarios

**Binaries** (in `icn/bins/`):
- `icnd` - The ICN daemon
- `icnctl` - CLI management tool

## Documentation Structure

**Project root `/home/matt/projects/icn/`:**
- `CLAUDE.md` - This file
- `README.md` - Project overview and quick start
- `ROADMAP.md` - Strategic roadmap (see this for "what's next")
- `CHANGELOG.md` - User-facing changelog

**Documentation directory `/home/matt/projects/icn/docs/`:**
- `ARCHITECTURE.md` - System architecture and component design
- `PHASE_HISTORY.md` - Completed development phases
- `HOMELAB_DEPLOYMENT.md` - K3s deployment details
- `glossary.md` - ICN terminology definitions
- `production-hardening.md` - Security hardening measures
- `trust-multi-graph-migration.md` - Multi-graph trust migration guide
- `dev-journal/` - Development session journals

## Build & Test Commands

All commands run from `icn/` directory:

```bash
cargo build                              # Build everything
cargo build --release                    # Build release binaries
cargo test                               # Run all tests
cargo test -p icn-gossip                 # Test specific package
cargo test test_two_node_convergence     # Test by name
cargo build && ./target/debug/icnd       # Run daemon
cargo build && ./target/debug/icnctl status
```

## Architecture: Actor-Based Runtime

ICNd uses Tokio with an actor pattern. The supervisor (`icn-core/src/supervisor.rs`) spawns and manages actors:

1. **Runtime** (`icn-core/src/runtime.rs`): Entry point, shutdown signal, config loading
2. **Supervisor** (`icn-core/src/supervisor.rs`): Spawns actors, initializes metrics, bridges gossip/network
3. **GossipActor** (`icn-gossip/src/gossip.rs`): Topic subscriptions, vector clocks, anti-entropy
4. **NetworkActor** (`icn-net/src/actor.rs`): QUIC sessions, mDNS discovery, message routing
5. **Ledger** (`icn-ledger/src/ledger.rs`): Double-entry accounting, gossip sync

**Actor Communication**:
```rust
// Network → Gossip: Incoming message handler
let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
    if let MessagePayload::Gossip(gossip_msg) = net_msg.payload {
        gossip_handle.blocking_write().handle_message(gossip_msg)?;
    }
});

// Gossip → Network: Send callback
let send_callback: SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
    network_handle.send_message(recipient, net_msg).await?;
});
```

## Key Protocols

**Gossip Protocol** (`icn-gossip`):
- Push announcements, pull requests, anti-entropy
- Vector clocks for causal ordering
- Subscription notifications via callbacks

**Network Protocol** (`icn-net/src/protocol.rs`):
- `NetworkMessage` envelope with `from_did`, `to_did`, `payload`
- Payload types: `Gossip`, `Rpc`, `Subscribe`, `Hello`, `Signed`
- Length-prefixed framing over QUIC streams

**Signed Messages** (`icn-net/src/envelope.rs`):
- `SignedEnvelope` with Ed25519 signatures
- `ReplayGuard` with sequence tracking
- Automatic verification in NetworkActor

## Cooperative Contract Language (CCL)

CCL (`icn-ccl`) is a domain-specific language for expressing agreements:

- AST-based: `Contract`, `Rule`, `Stmt`, `Expr`, `Value`
- Capability system: `ReadLedger`, `WriteLedger`, `ReadTrust`
- Fuel metering, not Turing-complete, deterministic

## Testing Patterns

**Integration Tests**: Located in `icn/crates/icn-core/tests/` and `icn/crates/icn-ledger/tests/`
- Use `TestNode` helper pattern
- Each node gets unique port and keypair
- Verify convergence with retries

## Identity & Keystore

- DIDs: `did:icn:<base58-pubkey>` (Ed25519)
- Keystore: Age-encrypted at `~/.icn/keystore.age`
- Auto-migration: v1 → v2 → v2.1 (adds TLS binding + X25519 keys)

**icnctl commands**: `id init`, `id show`, `id rotate`, `id export/import`

## Current Development Status

**Pilot Ready**: Byzantine fault-tolerant infrastructure operational with comprehensive monitoring.

**Recent Phases**:
- Phase 18: Pre-Pilot Hardening (Byzantine detection, reputation system)
- Phase 16: Scheduler Evolution (resource profiles, placement scoring, policies)
- Phase 15: Distributed Compute Layer (trust-gated task execution)

See **[docs/PHASE_HISTORY.md](docs/PHASE_HISTORY.md)** for complete phase history.

**What's Next**: See [ROADMAP.md](ROADMAP.md) - Track C1 (Pilot Community Selection) is next priority.

## Common Development Workflows

**Adding a new actor**:
1. Create actor struct with state
2. Implement message enum
3. Create handle struct with `mpsc::Sender<Msg>`
4. Implement `spawn()` method
5. Register with supervisor
6. Wire up callbacks/channels

**Adding a new gossip topic**:
1. Define topic string (`namespace:purpose`)
2. Configure `AccessControl` enum
3. Subscribe in relevant actor
4. Set up notification callback
5. Handle incoming messages

**Adding metrics**:
1. Define in `icn-obs/src/metrics/{module}.rs`
2. Register in `init_metrics()`
3. Follow naming: `{actor}_{metric}_{unit}`

## Security & Production Hardening

**Three-Layer Security**:
1. Transport: QUIC/TLS with DID-TLS binding
2. Message: SignedEnvelope with Ed25519 + replay protection
3. Application: EncryptedEnvelope with E2E encryption

**Trust-gated rate limiting** (per trust class):
- Isolated (< 0.1): 10 msg/sec
- Known (0.1-0.4): 50 msg/sec
- Partner (0.4-0.7): 100 msg/sec
- Federated (0.7+): 200 msg/sec

See [docs/production-hardening.md](docs/production-hardening.md) for complete details.

## Notes

- Daemon requires unlocked keystore (passphrase on startup)
- Actor handles use `Arc<RwLock<T>>` or message passing
- Shutdown via `tokio::sync::broadcast`
- Integration tests need unique ports per node
- Vector clocks prevent duplicate gossip processing
