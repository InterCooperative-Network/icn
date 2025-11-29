# ICN - Intercooperative Network

[![CI](https://github.com/InterCooperative-Network/icn/actions/workflows/ci.yml/badge.svg)](https://github.com/InterCooperative-Network/icn/actions/workflows/ci.yml)

A substrate daemon for the cooperative internet.

## Quick Start

Get a two-node network running in under 5 minutes:

```bash
# 1. Build ICN
cd icn && cargo build --release

# 2. Start node alpha (terminal 1)
./target/release/icnd --config ../config/icn-alpha.toml

# 3. Start node beta (terminal 2)
./target/release/icnd --config ../config/icn-beta.toml

# 4. Check network status (terminal 3)
./target/release/icnctl network status
./target/release/icnctl network peers

# Nodes will discover each other via mDNS within seconds
```

**Next Steps:**
- **[Getting Started Guide](docs/GETTING_STARTED.md)** - Complete onboarding (5-minute quickstart, core concepts, troubleshooting)
- **[FAQ](docs/FAQ.md)** - 30+ common questions answered
- [Configuration Examples](config/) - Customize your node
- [Docker Deployment](docker/) - Run with containers
- [Examples](examples/) - Tutorials and demos

---

## 🌱 For Cooperative Communities

**Want to use ICN for your timebank or mutual credit system?**

Check out the **[Pilot Web UI](web/pilot-ui/)** - a production-ready web interface with:

- 📊 Dashboard with balance tracking and activity charts
- 🕐 Easy time/credit logging interface
- 👥 Member directory with search
- 🗳️ Built-in governance and voting
- 📱 Full mobile support
- 📋 CSV export for treasurer reports

**Quick Deploy** (Complete stack with Docker):
```bash
cd deploy
./quickstart.sh "Your Cooperative Name"
# Visit http://localhost:3000
```

**Resources**:
- [5-Minute Getting Started](web/pilot-ui/GETTING-STARTED.md) - Test locally
- [Production Deployment](web/pilot-ui/PRODUCTION-DEPLOY.md) - Deploy with TLS/HTTPS
- [Complete Summary](web/pilot-ui/SUMMARY.md) - All features documented
- [User Guides](web/pilot-ui/) - Quick start, treasurer, admin, FAQ

---

## What is ICN?

ICN is not a blockchain. It's not a federation server. It's a **substrate daemon** that provides:

- **Identity Layer**: Decentralized identifiers (DIDs) with Ed25519 cryptography
- **Trust Graph**: Web-of-participation based trust computation
- **Networking Layer**: QUIC/TLS secure sessions with mDNS discovery
- **Cooperative Contracts**: CCL (Cooperative Contract Language) execution
- **Mutual Credit Ledger**: Double-entry accounting with Merkle-DAG
- **P2P Coordination**: Gossip protocol with trust-gated topics

## Architecture

ICNd is built on Tokio with an actor-based runtime. The daemon manages:

- Identity & key management
- Peer discovery (LAN + WAN)
- Secure session establishment
- Contract execution
- Ledger state synchronization
- Policy enforcement via trust graph

## Project Status

**Status: PILOT-READY** ✅ - All infrastructure complete (Phases 1-20, 929+ tests passing)

**Core Substrate: Complete ✓**
- [x] Phases 0-7: Identity, trust graph, networking, ledger, contracts, gossip, production hardening
- [x] Phase 8: DID-TLS binding with persistent certificates
- [x] Phase 9: Message integrity with Ed25519 signatures and replay protection
- [x] Phase 10: End-to-end encryption with X25519-ChaCha20-Poly1305
- [x] Phase 11: Multi-device identity with DID Document v2
- [x] Phase 12: Economic safety rails (dynamic credit limits, dispute resolution)
- [x] Phase 13: Governance primitives v1 (domains, proposals, voting with gossip coordination)
- [x] Phase 14: Gateway REST API (JWT auth, WebSocket events, per-DID rate limiting)
- [x] Phase 15: Distributed compute layer (trust-gated CCL execution with task prioritization)
- [x] Phase 16: Intelligent scheduler (resource profiles, locality awareness, cooperative policies)
- [x] Phase 17: Storage hardening & replication (99.9% durability target)
- [x] Phase 18: Pre-pilot hardening (Byzantine detection, partition healing, conflict resolution)
- [x] Phase 19: Scalability optimizations (vector clock compression, trust caching)
- [x] Phase 20: Privacy enhancements (encrypted topics, onion routing, traffic obfuscation)
- [x] Federation layer: Inter-cooperative coordination (registry, trust bridging, credit settlement)

**Operational Readiness: Complete ✓**
- [x] Track B1: Operational hardening (backup/restore, monitoring dashboard, graceful restart)
- [x] Track B3: Economic modeling (agent-based simulation validates dynamic credit limits)

**Production Features:**
- ✅ Three-layer security (transport, message, application)
- ✅ Prometheus metrics and real-time monitoring dashboard
- ✅ Encrypted backup/restore with state snapshots
- ✅ Version negotiation with capability-based feature gating
- ✅ Graceful restart with vector clock and subscription persistence
- ✅ Comprehensive documentation (8,500+ lines)
- ✅ Production-hardened gateway (7 security headers, CORS, request limits)

**Next Milestone: Track C1 - Pilot Community Selection & Deployment**

## Topic Subscriptions

ICN supports topic subscriptions for filtered gossip routing:

```rust
// Subscribe to topics on a peer
let subscribe_msg = NetworkMessage::subscribe(
    my_did.clone(),
    peer_did.clone(),
    vec!["global:identity".to_string(), "ledger:hours".to_string()],
);
network_handle.send_message(peer_did, subscribe_msg).await?;

// Query subscription state
let subscribers = gossip.get_subscribers("global:identity");
let my_subscriptions = gossip.get_subscriptions(&my_did);

// Unsubscribe
let unsubscribe_msg = NetworkMessage::unsubscribe(
    my_did.clone(),
    peer_did.clone(),
    vec!["global:identity".to_string()],
);
network_handle.send_message(peer_did, unsubscribe_msg).await?;
```

Topics enforce access control policies (Public, TrustClass, Participants) during subscription.

See [docs/topic-subscriptions-api.md](docs/topic-subscriptions-api.md) for complete API documentation.

## Security & Production Hardening

ICN includes comprehensive production hardening against DoS attacks and resource exhaustion:

- **Rate limiting**: Per-peer message rate limiting (100 msg/sec, burst 20)
- **QUIC stream limits**: Bounded concurrent streams (10) and receive windows (1MB/stream)
- **Certificate validation**: DID extraction and expiration checking on TLS certificates
- **Message validation**: Size limits and overflow protection
- **Async-safe operations**: No blocking calls in Tokio runtime

See [docs/production-hardening.md](docs/production-hardening.md) for complete security documentation.

## Documentation

### For Users
- **[Getting Started Guide](docs/GETTING_STARTED.md)** - Complete onboarding from installation to first transaction
- **[FAQ](docs/FAQ.md)** - 30+ common questions covering setup, security, usage, and troubleshooting
- **[Migration Guides](docs/migration-guides/)** - Keystore version upgrades and safe version migration procedures

### For Developers
- **[Contributing Guide](CONTRIBUTING.md)** - Developer onboarding, code style, testing philosophy, and PR process
- **[Architecture](docs/ARCHITECTURE.md)** - System design, component architecture, and implementation details
- **[Code of Conduct](CODE_OF_CONDUCT.md)** - Community standards and expectations
- **[API Documentation](docs/)** - Topic subscriptions, governance primitives, and protocol references

### For Operators
- **[Deployment Guide](docs/deployment-guide.md)** - Production deployment, monitoring, and operations
- **[Project Governance](docs/PROJECT_GOVERNANCE.md)** - Decision-making process, roles, and release procedures

## Building

```bash
# From repository root
cd icn
cargo build --release

# Binaries will be in icn/target/release/
```

## Usage

### Starting the Daemon

```bash
# With default config (~/.icn/)
./target/release/icnd

# With custom config
./target/release/icnd --config path/to/config.toml

# Override data directory
./target/release/icnd --data-dir /custom/path --log-level debug
```

### Shell Completions

Generate shell completions for enhanced CLI experience:

```bash
# Bash
./target/release/icnctl completions bash > ~/.local/share/bash-completion/completions/icnctl

# Zsh
./target/release/icnctl completions zsh > ~/.zsh/completion/_icnctl

# Fish
./target/release/icnctl completions fish > ~/.config/fish/completions/icnctl.fish
```

### Identity Management

```bash
# Initialize new identity (creates keystore)
./target/release/icnctl id init

# Show current DID
./target/release/icnctl id show

# Rotate keys
./target/release/icnctl id rotate

# Export backup
./target/release/icnctl id export backup.age

# Import backup
./target/release/icnctl id import backup.age
```

### Trust Management

```bash
# Add trust edge
./target/release/icnctl trust add did:icn:z6Mk... --score 0.8 --label partner

# List trust edges
./target/release/icnctl trust list

# Show computed trust score
./target/release/icnctl trust show did:icn:z6Mk...

# Remove trust edge
./target/release/icnctl trust remove did:icn:z6Mk...
```

### Network Operations

```bash
# Check network status
./target/release/icnctl network status

# List discovered peers
./target/release/icnctl network peers

# Get network statistics
./target/release/icnctl network stats

# Manually dial a peer
./target/release/icnctl network dial did:icn:z6Mk... 192.168.1.100:4433
```

### Ports & Services

By default, ICN exposes these services:

| Service | Port | Protocol | Purpose |
|---------|------|----------|---------|
| **Peer Transport** | 7777 | QUIC/UDP | P2P communication |
| **RPC API** | 5601 | HTTP | CLI control (icnctl) |
| **Metrics** | 9100 | HTTP | Prometheus exporter |
| **Health** | 8080 | HTTP | Health checks |

Access metrics: `curl http://localhost:9100/metrics`

Access health: `curl http://localhost:8080/health`

## Development

### Crates

- `icn-core` - Runtime, supervisor, config
- `icn-identity` - DID, keys, crypto
- `icn-trust` - Trust graph & policy
- `icn-net` - Discovery, sessions, transport
- `icn-gossip` - Topic-based sync
- `icn-ledger` - Mutual credit accounting
- `icn-ccl` - Contract language runtime
- `icn-store` - Persistent KV storage
- `icn-rpc` - gRPC API
- `icn-obs` - Metrics, tracing, logging
- `icn-gateway` - REST API & WebSocket gateway
- `icn-governance` - Governance primitives
- `icn-compute` - Distributed compute layer
- `icn-federation` - Inter-cooperative coordination
- `icn-privacy` - Privacy enhancements (encrypted topics, onion routing)
- `icn-security` - Byzantine fault detection
- `icn-time` - Clock synchronization
- `icn-snapshot` - State persistence
- `icn-testkit` - Test utilities

### Binaries

- `icnd` - The ICN daemon
- `icnctl` - CLI management tool
- `icn-console` - Interactive TUI for cooperative management

### Development Environment

**Using VS Code Dev Containers (Recommended):**
1. Install [VS Code](https://code.visualstudio.com/) and the [Dev Containers extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)
2. Open the repository in VS Code
3. Click "Reopen in Container" when prompted
4. Development environment will be ready with Rust, Node.js, and all extensions pre-configured

## Community & Contributing

We welcome contributions from the cooperative community! ICN is designed for cooperatives, by cooperatives.

**Getting Started:**
1. Read the [Contributing Guide](CONTRIBUTING.md) - setup, code style, testing, and PR process
2. Check our [Code of Conduct](CODE_OF_CONDUCT.md) - community standards
3. Review [Project Governance](docs/PROJECT_GOVERNANCE.md) - decision-making and roles

**Ways to Contribute:**
- **Report bugs** - Use GitHub issues with detailed reproduction steps
- **Improve documentation** - Guides, examples, and clarifications welcome
- **Submit code** - Bug fixes, features, tests, and optimizations
- **Join a pilot** - Help test ICN with your cooperative community
- **Provide feedback** - Share your use case and requirements

**Development Quick Start:**
```bash
# Clone and build
git clone https://github.com/InterCooperative-Network/icn.git
cd icn
cd icn && cargo build

# Run tests
cargo test

# Generate shell completions
./target/debug/icnctl completions bash > icnctl.bash
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed development workflow.

## License

MIT OR Apache-2.0
