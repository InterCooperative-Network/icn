# GitHub Copilot Instructions for ICN

This file provides guidance to GitHub Copilot when working with the ICN (Intercooperative Network) codebase.

## Project Overview

ICN is a substrate daemon for the cooperative internet. It is **not** a blockchain or federation server - it's a P2P coordination layer providing:

- **Identity Layer**: Decentralized identifiers (DIDs) with Ed25519 cryptography
- **Trust Graph**: Web-of-participation based trust computation
- **Networking**: QUIC/TLS secure sessions with mDNS discovery
- **Cooperative Contracts**: CCL (Cooperative Contract Language) execution
- **Mutual Credit Ledger**: Double-entry accounting with Merkle-DAG
- **P2P Coordination**: Gossip protocol with trust-gated topics
- **Distributed Compute**: Trust-gated CCL execution with intelligent scheduling
- **Governance**: Democratic proposals and voting primitives
- **Gateway API**: REST + WebSocket API for cooperative applications

## Repository Structure

```
icn/
├── icn/                    # Main Rust workspace
│   ├── crates/            # Core library crates
│   │   ├── icn-core/      # Actor runtime & supervisor
│   │   ├── icn-identity/  # DID generation & keystore
│   │   ├── icn-trust/     # Trust graph computation
│   │   ├── icn-net/       # QUIC/TLS networking
│   │   ├── icn-gossip/    # Topic-based gossip protocol
│   │   ├── icn-ledger/    # Mutual credit accounting
│   │   ├── icn-ccl/       # Contract language interpreter
│   │   ├── icn-store/     # Persistent storage (Sled)
│   │   ├── icn-rpc/       # gRPC API server
│   │   ├── icn-gateway/   # REST + WebSocket API
│   │   ├── icn-governance/# Governance primitives
│   │   ├── icn-compute/   # Distributed compute layer
│   │   ├── icn-obs/       # Metrics & observability
│   │   └── icn-testkit/   # Testing utilities
│   └── bins/              # Binaries
│       ├── icnd/          # ICN daemon
│       └── icnctl/        # CLI management tool
├── docs/                  # Comprehensive documentation
├── deploy/                # Kubernetes & deployment configs
├── web/                   # Web UIs (pilot-ui, etc.)
├── sdk/                   # Client SDKs (TypeScript, etc.)
└── examples/              # Usage examples
```

## Development Workflow

### Build & Test Commands

All commands must be run from the `icn/` directory:

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

# Linting
cargo clippy -- -D warnings

# Formatting
cargo fmt

# Run the daemon
./target/debug/icnd

# Use the CLI
./target/debug/icnctl status
```

### Code Quality Standards

- **Follow existing patterns**: The codebase has established patterns for actors, handles, and message passing
- **Error handling**: Use `Result<T, E>` types, never panic in protocol code
- **Async operations**: Use Tokio runtime, no blocking operations in async contexts
- **Testing**: Write tests for all new functionality, follow existing test patterns
- **Documentation**: Add rustdoc comments for public APIs
- **Linting**: Code must pass `cargo clippy` and `cargo fmt`

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

Examples:
- `feat(gateway): add WebSocket authentication`
- `fix(ledger): correct double-entry balance calculation`
- `test(compute): add task cancellation tests`

## Architecture Patterns

### Actor-Based Runtime

ICNd uses Tokio with an actor pattern:

1. **Supervisor** (`icn-core/src/supervisor.rs`): Spawns and manages all actors
2. **Actors**: GossipActor, NetworkActor, Ledger, GovernanceActor, ComputeActor
3. **Handles**: Each actor provides a handle for async API access
4. **Message Passing**: Use `mpsc::channel` for actor communication
5. **Shared State**: Use `Arc<RwLock<T>>` for shared access

Example actor handle pattern:
```rust
pub struct ActorHandle {
    tx: mpsc::Sender<ActorMsg>,
}

impl ActorHandle {
    pub async fn do_something(&self, arg: T) -> Result<R> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(ActorMsg::DoSomething { arg, reply: tx }).await?;
        rx.await?
    }
}
```

### Gossip Protocol

- **Push announcements**: Broadcast new content hashes
- **Pull requests**: Request missing content by hash
- **Anti-entropy**: Periodic Bloom filter exchange
- **Vector clocks**: Track causal dependencies per peer
- **Subscription notifications**: Reactive callbacks for new entries
- **Access control**: Public, Private, or TrustGated topics

### Security Architecture

Three-layer security model:

1. **Transport Layer**: QUIC/TLS with DID-TLS binding
2. **Message Layer**: SignedEnvelope with Ed25519 signatures + replay protection
3. **Application Layer**: EncryptedEnvelope with end-to-end encryption

### Trust Graph

- Trust scores between 0.0 and 1.0
- Transitive trust computation using weighted edges
- Used for access control, rate limiting, and resource allocation
- Trust thresholds vary by operation (e.g., MIN_TRUST_EXECUTE = 0.3)

## Common Development Tasks

### Adding a New Actor

1. Create actor struct with state
2. Implement message enum for actor operations
3. Create handle struct with `mpsc::Sender<Msg>`
4. Implement `spawn()` method that returns handle
5. Register with supervisor in `supervisor.rs`
6. Wire up communication with other actors via callbacks/channels

### Adding a New Gossip Topic

1. Define topic string (convention: `namespace:purpose`)
2. Configure `AccessControl` enum (Public, Private, TrustGated)
3. Subscribe in relevant actor: `gossip.subscribe(topic, access_control)`
4. Implement message serialization (use `bincode` or `serde_json`)
5. Set up notification callback to receive new entries
6. Handle incoming messages in gossip actor's message handler

### Adding Metrics

1. Define metric in `icn-obs/src/metrics.rs`
2. Register in `init_descriptions()` function
3. Increment/observe at instrumentation points
4. Follow naming convention: `{actor}_{metric}_{unit}`

### Working with the Ledger

- Double-entry bookkeeping with Merkle-DAG
- Entries are immutable once recorded
- Gossip syncs via `ledger:sync` topic
- Quarantine mechanism for conflicting entries
- All transactions require valid signatures

### Working with Contracts (CCL)

- AST-based language with deterministic execution
- Capability system: `ReadLedger`, `WriteLedger`, `ReadTrust`
- Fuel metering prevents infinite loops
- Not Turing-complete: No recursion, fixed iteration bounds
- Invoked via `ContractRuntime::invoke_rule()`

## Testing Patterns

### Integration Tests

- Use `TestNode` helper pattern to spawn isolated nodes
- Each node gets unique port and keypair
- Nodes dial each other via `network_handle.dial(addr, did)`
- Verify convergence with retries and timeouts
- Located in `icn/crates/icn-core/tests/` and package-specific `tests/` dirs

### Test Utilities

- `icn-testkit`: Helpers for multi-node test scenarios
- Temporary directory management
- Test keypair generation
- Use `#[tokio::test]` for async tests

## Key Files to Reference

When working on specific features, reference these files:

- **Actor Runtime**: `icn-core/src/supervisor.rs`, `icn-core/src/runtime.rs`
- **Network Protocol**: `icn-net/src/protocol.rs`, `icn-net/src/actor.rs`
- **Gossip Implementation**: `icn-gossip/src/gossip.rs`
- **Ledger Logic**: `icn-ledger/src/ledger.rs`, `icn-ledger/src/sync.rs`
- **Contract Execution**: `icn-ccl/src/interpreter.rs`, `icn-ccl/src/ast.rs`
- **Gateway API**: `icn-gateway/src/server.rs`, `icn-gateway/src/api/`
- **Governance**: `icn-governance/src/proposal.rs`, `icn-governance/src/domain.rs`, `icn-governance/src/vote.rs`, `icn-governance/src/store.rs`
- **Compute Layer**: `icn-compute/src/actor.rs`, `icn-compute/src/executor.rs`

## Documentation

Comprehensive documentation is available in the `docs/` directory:

- **ARCHITECTURE.md**: System architecture and design decisions
- **GETTING_STARTED.md**: Quick start guide for new contributors
- **production-hardening.md**: Security hardening measures
- **governance-primitives.md**: Governance system design
- **scheduler-evolution-plan.md**: Distributed compute scheduler design
- **dev-journal/**: Detailed development session notes

## Design Principles

ICN is built on five foundational principles:

1. **Local-first**: Nodes operate independently and reconcile via gossip
2. **Trust-native**: Security derives from social trust edges, not global consensus
3. **Deterministic compute**: Same inputs → same outputs → same ledger state
4. **Capability-based security**: Contracts have explicit permissions
5. **Human-governed**: Democratic and auditable policy changes

## Important Notes

- The daemon requires an unlocked keystore (passphrase prompt on startup)
- All actor handles use interior mutability (`Arc<RwLock<T>>` or message passing)
- Shutdown propagates via `tokio::sync::broadcast` channel
- Integration tests should use unique ports per node (avoid bind conflicts)
- Vector clocks prevent duplicate processing of gossip messages
- Never use blocking operations in Tokio runtime contexts
- DID format: `did:icn:<base58-pubkey>`

## Production Hardening

The codebase includes extensive production hardening:

- **Network protections**: Trust-gated rate limiting, QUIC stream limits, message validation
- **Protocol protections**: Certificate verification, Bloom filter validation, timestamp overflow protection
- **Runtime protections**: Async-safe operations, error handling, graceful degradation
- **Byzantine detection**: MisbehaviorDetector with automatic quarantine and banning
- **Metrics**: Comprehensive Prometheus metrics for monitoring
- **Backup/Restore**: `icnctl backup/restore` commands for disaster recovery

## Current Status

**Status: PILOT-READY** ✅

All core infrastructure is complete (Phases 1-20, 1134+ tests passing). The system includes:
- Complete actor runtime with supervisor
- DID-TLS binding with persistent certificates
- Message integrity with Ed25519 signatures
- End-to-end encryption with X25519-ChaCha20-Poly1305
- Multi-device identity support
- Economic safety rails (credit limits, dispute resolution)
- Governance primitives (domains, proposals, voting)
- Gateway REST + WebSocket API
- Distributed compute layer with intelligent scheduling
- Byzantine fault detection
- Storage replication with trust-weighted selection

See `ROADMAP.md` for upcoming features and `CHANGELOG.md` for recent changes.
