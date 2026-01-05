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

**IMPORTANT: Never save documentation files to the project root.** All documentation (session notes, status reports, guides, etc.) must go in the appropriate `docs/` subdirectory.

**Project root `/home/matt/projects/icn/`** (only these files):
- `README.md` - Project overview and quick start
- `CHANGELOG.md` - User-facing changelog
- `CLAUDE.md` - This file (Claude Code guidance)
- `CODE_OF_CONDUCT.md` - Community guidelines
- `CONTRIBUTING.md` - Contribution guidelines

**Documentation directory `/home/matt/projects/icn/docs/`:**
- `ARCHITECTURE.md` - System architecture and component design
- `PHASE_HISTORY.md` - Completed development phases
- `HOMELAB_DEPLOYMENT.md` - K3s deployment details
- `glossary.md` - ICN terminology definitions
- `production-hardening.md` - Security hardening measures
- `trust-multi-graph-migration.md` - Multi-graph trust migration guide
- `dev-journal/` - Development session journals and technical notes
- `demo/` - Demo session documentation and guides
- `security/` - Security audits, threat models, and fixes
- `ci/` - CI/CD status reports and configurations
- `status/` - System status reports and deployment verification
- `performance/` - Performance benchmarks and optimization docs

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

**What's Next**: See [docs/dev-journal/ROADMAP.md](docs/dev-journal/ROADMAP.md) for the strategic roadmap.

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

## Git Workflow

**Goal**: `main` is always releasable. Work happens on short-lived branches, merged via PR.

### Branch Policy

**Protected Branch: `main`**
- **No direct commits to `main`** - all changes land via Pull Request
- Only merge when CI passes (or local checks pass)

**Branch Naming**:
- `feat/<short-slug>` - new capability
- `fix/<short-slug>` - bug fix
- `refactor/<short-slug>` - structure change, behavior preserved
- `docs/<short-slug>` - documentation only
- `chore/<short-slug>` - build tooling, formatting, deps

Examples: `feat/gossip-compression`, `fix/trust-rate-limit-overflow`, `refactor/actor-message-routing`

### Daily Flow

```bash
# 1) Start from updated main
git checkout main
git pull origin main

# 2) Create a branch
git checkout -b feat/<slug>

# 3) Work in small commits
git add -A
git commit -m "feat(gossip): add message compression header"

# 4) Keep branch synced (if main moved)
git fetch origin
git rebase origin/main
git push --force-with-lease   # Only on YOUR branch

# 5) Push and open PR
git push -u origin feat/<slug>
```

### Commit Message Convention

Format: `<type>(<scope>): <summary>`

**Types**: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`

**Scopes**: `gossip`, `net`, `identity`, `trust`, `ledger`, `ccl`, `gateway`, `cli`, `runtime`, `sdk`, `governance`, `compute`

Examples:
- `feat(ledger): add demurrage scheduler`
- `fix(trust): prevent negative reputation underflow`
- `refactor(runtime): isolate actor mailbox logic`

### PR Requirements

**Size**: One coherent change, reviewable in <20 minutes. Split large changes.

**Description must include**:
- **What**: Summary of changes
- **Why**: Motivation / issue link
- **How**: Implementation notes
- **Risk**: What could break
- **Test plan**: How you verified

### Required Checks (before merge)

```bash
# Rust
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# TypeScript SDK (if changed)
cd sdk/typescript && npm test && npm run build
```

### Agent Behavior

The Claude Code agent **must**:
- Never push directly to `main`
- Always work on feature branches and open PRs
- Run required checks before requesting review
- Explain any skipped checks in the PR description
- **When working on an existing PR**: Always check for new/updated comments before making changes
  ```bash
  gh pr view <PR_NUMBER> --comments  # Check for review comments
  gh pr checks <PR_NUMBER>           # Check CI status
  ```

## Issue Management

See **[.github/ISSUE_POLICY.md](.github/ISSUE_POLICY.md)** for the complete issue taxonomy and triage system.

### Quick Rules for Agents

**Every issue MUST have**:
- Exactly one `priority:*` label (`priority:critical`, `priority:high`, `priority:medium`, `priority:low`)
- Exactly one type label (`bug`, `enhancement`, `design`, `documentation`, `testing`, `refactor`)
- At least one domain label if it touches code (`core`, `identity`, `ledger`, `governance`, `gateway`, etc.)

**Issue Title Format**: `<type>(<domain>): <action>`
- Examples: `feat(ledger): Add demurrage scheduler`, `fix(gossip): Remove blocking operations`

**Before Creating Issues**:
1. Search for existing duplicates
2. If duplicate exists, comment + link instead of creating new
3. For "future ideas," use `priority:low` + `design`

**Issue Hierarchy**:
- **Level 0**: Meta/Roadmap (rare, ≤5 total)
- **Level 1**: Execution Epics (primary control surface, ≤10 sub-issues each)
- **Level 2**: Atomic Work Items (where code happens, single responsibility)

**Deprecated Labels** (do not use):
- `critical`, `high`, `medium`, `low` → use `priority:*` instead
- `P0-critical`, `P1-high` → use `priority:*` instead
- `tech-debt`, `technical-debt` → use `refactor` instead

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
