# ICN - InterCooperative Network

[![CI](https://github.com/InterCooperative-Network/icn/actions/workflows/ci.yml/badge.svg)](https://github.com/InterCooperative-Network/icn/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](LICENSE)

A substrate daemon for the cooperative internet.

**[intercooperative.network](https://intercooperative.network)**

ICN is a P2P coordination layer for cooperatives, communities, and federations. It is not a blockchain or federation server — it's a constraint engine where apps translate cooperative governance into generic constraints, and the kernel enforces them without understanding their meaning.

---

## What ICN Provides

- **Decentralized Identity** — DIDs with Ed25519 cryptography and Age-encrypted keystores
- **Trust Graph** — Web-of-participation trust computation with signed attestations
- **Mutual Credit Ledger** — Double-entry accounting with Merkle-DAG integrity
- **Cooperative Contracts** — CCL (Cooperative Contract Language) for expressing bylaws, agreements, and governance
- **P2P Networking** — QUIC/TLS sessions with mDNS discovery and gossip replication
- **Democratic Governance** — Proposals, voting, and parameter management
- **Distributed Compute** — Trust-gated task execution with receipt settlement

## Architecture

ICN implements a **constraint enforcement architecture**:

```
CCL Document (constitution / bylaws / treaty)
         |
App / Policy Oracle (governance, trust, ledger)
         |
ConstraintSet (rate limits, credit ceilings, voting weights)
         |
Kernel enforces constraints mechanically
```

The kernel never sees "trust scores" or "governance rules" — only generic constraints. This is the **Meaning Firewall**: domain semantics stay in apps, the kernel stays predictable.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full architecture documentation.

## Quick Start

```bash
# Build
cd icn && cargo build --release

# Start node alpha (terminal 1)
./target/release/icnd --config ../config/icn-alpha.toml

# Start node beta (terminal 2)
./target/release/icnd --config ../config/icn-beta.toml

# Check status (terminal 3)
./target/release/icnctl network status
./target/release/icnctl network peers
# Nodes discover each other via mDNS within seconds
```

### Identity Management

```bash
icnctl id init          # Create identity (DID + keystore)
icnctl id show          # Show current DID
icnctl id rotate        # Rotate keys
icnctl id export b.age  # Export encrypted backup
```

### Trust & Networking

```bash
icnctl trust add did:icn:z6Mk... --score 0.8 --label partner
icnctl trust list
icnctl network peers
icnctl network stats
```

### Ports

| Service | Port | Protocol | Purpose |
|---------|------|----------|---------|
| Peer Transport | 7777 | QUIC/UDP | P2P communication |
| RPC API | 5601 | HTTP | CLI control (icnctl) |
| Metrics | 9100 | HTTP | Prometheus exporter |
| Health | 8080 | HTTP | Health checks |

## Project Structure

**28 crates** in `icn/crates/`, **4 app crates** in `apps/`, **3 binaries** in `icn/bins/`.

### Kernel (domain-agnostic)

| Crate | Purpose |
|-------|---------|
| `icn-kernel-api` | Trait definitions (PolicyOracle, State, Compute, Comms) |
| `icn-core` | Tokio runtime, supervisor, actor lifecycle |
| `icn-net` | QUIC/TLS sessions, mDNS discovery |
| `icn-gossip` | Topic-based replication with causal ordering |
| `icn-gateway` | REST + WebSocket API |
| `icn-rpc` | gRPC API server |
| `icn-store` | Persistent KV storage (sled) |
| `icn-obs` | Prometheus metrics, tracing |
| `icn-identity` | DIDs, Ed25519 keypairs, encrypted keystore |
| `icn-encoding` | Serialization utilities |
| `icn-time` | Clock synchronization |
| `icn-snapshot` | State snapshots for graceful restarts |
| `icn-api` | Shared validation and error handling |
| `icn-testkit` | Test utilities |

### Apps (domain-specific, Policy Oracles)

| Crate | Purpose |
|-------|---------|
| `apps/trust` | Trust graph → PolicyOracle (rate limits, credit multipliers) |
| `apps/governance` | GovernanceActor, proposal execution |
| `apps/ledger` | Ledger reduction and settlement |
| `apps/echo` | Example app for testing |

### Domain Crates

| Crate | Purpose |
|-------|---------|
| `icn-trust` | Trust graph storage and computation |
| `icn-governance` | Governance primitives (proposals, voting, parameters) |
| `icn-ledger` | Double-entry mutual credit with Merkle-DAG |
| `icn-ccl` | Contract language AST, interpreter, fuel metering |
| `icn-compute` | Distributed compute with trust-gated execution |
| `icn-federation` | Inter-cooperative coordination |
| `icn-entity` | Unified entity model |
| `icn-coop` | Cooperative management |
| `icn-community` | Community structures |
| `icn-security` | Byzantine fault detection |
| `icn-privacy` | Privacy primitives |
| `icn-crypto-pq` | Post-quantum hybrid cryptography |
| `icn-steward` | SDIS steward network |
| `icn-zkp` | Zero-knowledge proofs |

### Binaries

| Binary | Purpose |
|--------|---------|
| `icnd` | The ICN daemon |
| `icnctl` | CLI management tool |
| `icn-console` | Interactive TUI |

## Development

```bash
cd icn

# Build
cargo build

# Test
cargo test --all-features

# Lint
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for architectural guardrails and the git workflow.

## Security

ICN implements three-layer security:

1. **Transport** — QUIC/TLS with DID-TLS certificate binding
2. **Message** — Ed25519 signed envelopes with replay protection
3. **Application** — E2E encryption with X25519-ChaCha20-Poly1305

Trust-gated rate limiting enforces per-actor throughput bounds based on trust class. See [docs/production-hardening.md](docs/production-hardening.md).

## Documentation

- **[Architecture](docs/ARCHITECTURE.md)** — Constraint engine model, kernel/app separation, Policy Oracle pattern
- **[Getting Started](docs/GETTING_STARTED.md)** — Installation and first steps
- **[FAQ](docs/FAQ.md)** — Common questions
- **[Contributing](CONTRIBUTING.md)** — Development workflow and architectural guardrails
- **[Architecture Index](docs/architecture/)** — Visual diagrams, kernel/app separation details
- **[Onboarding Modules](docs/onboarding/)** — Deep-dive learning modules
- **[Production Hardening](docs/production-hardening.md)** — Security and deployment
- **[Roadmap](docs/dev-journal/ROADMAP.md)** — Current and planned phases
- **[Phase History](docs/PHASE_HISTORY.md)** — Completed development phases

## Status

~362K lines of Rust across 28 crates and 4 app crates. Deployed on a K3s cluster since December 2025.

Active work is tracked in [6 epics](https://github.com/InterCooperative-Network/icn/issues/856):
- **Kernel/App Separation** — Extracting domain logic from kernel (governance ratchet: 83 → 43)
- **Architecture Invariants** — Formalizing the constraint engine boundary
- **Trust Hardening** — Attestation signing, revocations, replay protection
- **Service Discovery** — Gossip-based endpoint propagation
- **Commons/Compute** — Resource pool and credit accounting
- **Kernel Performance** — StateSnapshot COW, rate limiting

## License

[AGPL-3.0](LICENSE)
