# GitHub Copilot Instructions for ICN

This file provides guidance to GitHub Copilot when working with the ICN (Intercooperative Network) codebase.

## Absolute Rules (must follow)

1. **Read `AGENTS.md` first** for operating mode, invariants, and change routing.
2. **Never weaken safety to fix tests**: Do not relax validation, trust gates, signature checks, encoding rules, or determinism requirements.
3. **Verify before claiming**: Do not claim tests passed without showing output. Run the actual commands.
4. **Follow change routing**: Run the right checks for what you touched (see `AGENTS.md`).
5. **Docs must match code**: If you change semantics, update the relevant doc/spec in the same PR.

> Custom agents are available in `.github/agents/` - use `@icn-orchestrator` to route multi-subsystem requests.

> For path-specific instructions (Rust, web, SDK, docs), see `.github/instructions/` directory.

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
- **Cooperative Management**: Lifecycle, membership, and multi-stakeholder governance
- **Federation**: Inter-cooperative coordination and cross-coop settlements
- **Privacy**: Encrypted metadata, onion routing, traffic obfuscation
- **Post-Quantum Security**: Hybrid cryptography for long-term protection
- **SDIS Integration**: Sovereign Digital Identity with VUI and zero-knowledge proofs
- **Byzantine Tolerance**: Misbehavior detection and reputation-based security

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
│   │   ├── icn-coop/      # Cooperative management & lifecycle
│   │   ├── icn-community/ # Community structures & civic engine
│   │   ├── icn-entity/    # Unified entity model (individuals/coops/federations)
│   │   ├── icn-federation/# Inter-cooperative coordination
│   │   ├── icn-privacy/   # Privacy primitives & metadata protection
│   │   ├── icn-security/  # Byzantine fault detection & reputation
│   │   ├── icn-crypto-pq/ # Post-quantum hybrid cryptography
│   │   ├── icn-steward/   # SDIS steward network & VUI computation
│   │   ├── icn-zkp/       # Zero-knowledge proofs for SDIS
│   │   ├── icn-time/      # Clock synchronization (Rough Time Protocol)
│   │   ├── icn-snapshot/  # State snapshots for restarts
│   │   ├── icn-api/       # API types and definitions
│   │   ├── icn-encoding/  # Serialization utilities
│   │   └── icn-testkit/   # Testing utilities
│   └── bins/              # Binaries
│       ├── icnd/          # ICN daemon
│       ├── icnctl/        # CLI management tool
│       └── icn-console/   # Interactive TUI for cooperative management
├── docs/                  # Comprehensive documentation
├── deploy/                # Kubernetes & deployment configs
├── web/                   # Web UIs (pilot-ui, etc.)
├── sdk/                   # Client SDKs (TypeScript, etc.)
└── examples/              # Usage examples
```

## Current Working Context

- See docs/STATE.md and docs/TODO.md for current status, known issues, and priorities.
- Rust toolchain note: wasmtime/cranelift currently require rustc 1.89.0; upgrade toolchain or pin compatible versions before running cargo.

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

### Cooperative & Federation Architecture

**Cooperatives** (`icn-coop`):
- Management and lifecycle (formation, dissolution, membership)
- CoopActor handles coop operations via gossip topic `coop:updates`
- Member roles, tiers, and status tracking
- Asset distribution and capital return methods

**Communities** (`icn-community`):
- Civic structures grouping cooperatives and individuals
- Community types: Geographic, Interest-based, Project-based
- Shared governance, resources, and mutual support
- CommunityActor syncs via `community:updates` topic

**Entities** (`icn-entity`):
- Unified recursive model for individuals, cooperatives, and federations
- EntityId format: `entity:icn:<type>:<identifier>`
- DID interoperability for individuals
- Enables arbitrary organizational hierarchies

**Federation** (`icn-federation`):
- Inter-cooperative coordination and discovery
- Federated trust attestations and DID resolution
- Cross-cooperative credit settlement
- Cooperative-scoped gossip routing
- DID format with cooperative prefix: `did:icn:coop-name:pubkey`

### Security & Privacy

**Byzantine Detection** (`icn-security`):
- MisbehaviorDetector tracks violations and reputation
- Automatic quarantine and banning based on thresholds
- Trust penalty callbacks for network protection
- Violation records with severity scoring

**Privacy Primitives** (`icn-privacy`):
- ChaCha20-Poly1305 encryption for topic metadata
- Bloom filter-based topic discovery (privacy-preserving)
- Onion routing for sender/receiver anonymity (planned)
- Traffic obfuscation and cover traffic (planned)

**Post-Quantum Cryptography** (`icn-crypto-pq`):
- Hybrid classical/PQ constructions for long-term security
- Ed25519 + ML-DSA (Dilithium) for signatures
- X25519 + ML-KEM (Kyber) for key exchange
- Defense-in-depth: attacker must break both algorithms

### SDIS (Sovereign Digital Identity System)

**Steward Network** (`icn-steward`):
- Threshold PRF for Verifiable Unique Identifiers (VUI)
- Proof-of-Personhood enrollment ceremonies
- Blind signatures for privacy-preserving tokens
- Social recovery through steward attestations
- Distributed VUI registry for uniqueness checking

**Zero-Knowledge Proofs** (`icn-zkp`):
- Attribute proofs (age, citizenship, membership) without revealing data
- Non-revocation proofs with RSA or Merkle accumulators
- Compound proofs combining multiple attributes
- PQ-safe Merkle accumulators for future-proof revocation

### Infrastructure

**Clock Synchronization** (`icn-time`):
- Rough Time Protocol (RFC 8915) for cooperative-wide time sync
- Distributed timestamp validation
- Replay attack protection
- Clock skew detection and correction

**State Snapshots** (`icn-snapshot`):
- Local state serialization for graceful restarts
- Distributed snapshots (Chandy-Lamport) for network consistency
- Actor state export/restore for gossip, network, ledger
- Checkpoint-based recovery

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

**Core Infrastructure:**
- **Actor Runtime**: `icn-core/src/supervisor.rs`, `icn-core/src/runtime.rs`
- **Network Protocol**: `icn-net/src/protocol.rs`, `icn-net/src/actor.rs`
- **Gossip Implementation**: `icn-gossip/src/gossip.rs`
- **Storage**: `icn-store/src/lib.rs`
- **Metrics**: `icn-obs/src/metrics.rs`

**Identity & Cryptography:**
- **Identity**: `icn-identity/src/keystore.rs`, `icn-identity/src/did.rs`
- **Post-Quantum Crypto**: `icn-crypto-pq/src/keypair.rs`, `icn-crypto-pq/src/signature.rs`
- **Trust Graph**: `icn-trust/src/graph.rs`, `icn-trust/src/computation.rs`

**Economic & Governance:**
- **Ledger Logic**: `icn-ledger/src/ledger.rs`, `icn-ledger/src/sync.rs`
- **Contract Execution**: `icn-ccl/src/interpreter.rs`, `icn-ccl/src/ast.rs`
- **Governance**: `icn-governance/src/proposal.rs`, `icn-governance/src/domain.rs`, `icn-governance/src/vote.rs`

**Cooperative Structures:**
- **Cooperatives**: `icn-coop/src/actor.rs`, `icn-coop/src/lifecycle.rs`, `icn-coop/src/membership.rs`
- **Communities**: `icn-community/src/actor.rs`, `icn-community/src/types.rs`
- **Entities**: `icn-entity/src/lib.rs`
- **Federation**: `icn-federation/src/registry.rs`, `icn-federation/src/bridge.rs`

**Security & Privacy:**
- **Security**: `icn-security/src/misbehavior.rs`
- **Privacy**: `icn-privacy/src/topic_encryption.rs`, `icn-privacy/src/bloom.rs`

**SDIS (Sovereign Digital Identity):**
- **Steward Network**: `icn-steward/src/vui.rs`, `icn-steward/src/enrollment.rs`
- **Zero-Knowledge Proofs**: `icn-zkp/src/prover.rs`, `icn-zkp/src/verifier.rs`

**Services:**
- **Gateway API**: `icn-gateway/src/server.rs`, `icn-gateway/src/api/`
- **Compute Layer**: `icn-compute/src/actor.rs`, `icn-compute/src/executor.rs`
- **RPC Server**: `icn-rpc/src/server.rs`
- **Clock Sync**: `icn-time/src/sync.rs`
- **Snapshots**: `icn-snapshot/src/local.rs`, `icn-snapshot/src/distributed.rs`

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

**Core Infrastructure:**
- Complete actor runtime with supervisor
- DID-TLS binding with persistent certificates
- Message integrity with Ed25519 signatures
- End-to-end encryption with X25519-ChaCha20-Poly1305
- Multi-device identity support
- Byzantine fault detection with reputation system
- Storage replication with trust-weighted selection
- State snapshots for graceful restarts

**Economic & Governance:**
- Economic safety rails (credit limits, dispute resolution)
- Governance primitives (domains, proposals, voting)
- Mutual credit ledger with Merkle-DAG
- Cooperative lifecycle management (formation, dissolution)
- Multi-stakeholder membership models

**Federation & Coordination:**
- Federation protocol for inter-cooperative coordination
- Cross-cooperative credit settlement
- Federated trust attestations
- Community structures for civic organization
- Unified entity model (individuals/coops/federations)

**Security & Privacy:**
- Post-quantum hybrid cryptography (Ed25519+ML-DSA, X25519+ML-KEM)
- Privacy-preserving topic encryption
- Clock synchronization for replay protection
- Byzantine misbehavior detection

**SDIS Integration:**
- Steward network for VUI computation
- Zero-knowledge attribute proofs
- Enrollment ceremonies and token issuance
- Social recovery mechanisms

**Services:**
- Gateway REST + WebSocket API
- Distributed compute layer with intelligent scheduling
- Comprehensive Prometheus metrics

See `ROADMAP.md` for upcoming features and `CHANGELOG.md` for recent changes.
