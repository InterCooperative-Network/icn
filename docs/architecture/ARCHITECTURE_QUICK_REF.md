# ICN Architecture Quick Reference Card

> Historical quick-reference snapshot (December 2025).
> Verify current defaults/endpoints/operational claims against active docs before use.

## 🎯 What is ICN?

**Substrate daemon for cooperative internet coordination**

- NOT a blockchain
- NOT a federation server  
- P2P coordination layer with built-in trust

## 🏗️ Core Stack (8 Layers)

```
Applications         │ icnd, icnctl, icn-console
────────────────────
Gateway              │ REST + WebSocket + RPC
────────────────────
Coordination         │ Compute, Governance, Contracts
────────────────────
Sync                 │ Gossip (pub/sub)
────────────────────
State                │ Ledger (double-entry)
────────────────────
Trust                │ Graph (transitive scores)
────────────────────
Network              │ QUIC/TLS (DID-TLS binding)
────────────────────
Identity             │ DIDs (Ed25519)
────────────────────
Storage              │ Sled (key-value)
```

## 📦 25 Crates at a Glance

### Core (10)
- `icn-core` - Supervisor, runtime
- `icn-identity` - DIDs, keys
- `icn-trust` - Trust graph
- `icn-net` - QUIC/TLS
- `icn-gossip` - Pub/sub
- `icn-ledger` - Mutual credit
- `icn-ccl` - Contracts
- `icn-compute` - Distributed tasks
- `icn-governance` - Proposals/voting
- `icn-store` - Persistence

### APIs (2)
- `icn-gateway` - REST + WebSocket
- `icn-rpc` - JSON-RPC

### Infrastructure (5)
- `icn-obs` - Metrics
- `icn-security` - Byzantine detection
- `icn-time` - Clock sync
- `icn-privacy` - Metadata protection
- `icn-snapshot` - Backup/restore

### Future (5)
- `icn-federation` - Inter-coop
- `icn-steward` - SDIS enrollment
- `icn-crypto-pq` - Post-quantum
- `icn-zkp` - Zero-knowledge
- `icn-testkit` - Test utilities

### Binaries (3)
- `icnd` - Daemon
- `icnctl` - CLI
- `icn-console` - TUI

## 🔑 Key Concepts

| Concept | Description |
|---------|-------------|
| **DID** | `did:icn:<base58-pubkey>` - Self-certifying ID |
| **Trust Score** | 0.0-1.0 (transitive web-of-participation) |
| **Gossip Topic** | Pub/sub channel (e.g., `ledger:sync`) |
| **Journal Entry** | Ledger transaction (Merkle-DAG) |
| **SignedEnvelope** | Message integrity (Ed25519 + replay guard) |
| **TrustGated** | Access control via trust threshold |
| **Actor** | Tokio task with message passing |
| **Handle** | External API to actor (mpsc wrapper) |

## 🔐 Security Layers

```
Application  │ Capabilities (ReadLedger, WriteLedger)
────────────
Message      │ SignedEnvelope + EncryptedEnvelope
────────────
Transport    │ QUIC/TLS + DID-TLS binding
```

## 📊 Trust Classes

| Class | Score | Capabilities | Rate Limit |
|-------|-------|--------------|------------|
| Unknown | 0.0-0.1 | None | Blocked |
| Known | 0.1-0.3 | Read | 10/min |
| Colleague | 0.3-0.6 | Write | 100/min |
| Close | 0.6-0.8 | Execute | 1K/min |
| Intimate | 0.8-1.0 | Admin | Unlimited |

## 🔄 Core Data Flows

### Ledger Transaction
```
API → Ledger (validate) → Store → Gossip → Peers
```

### Compute Task
```
Submit → Schedule → Execute (WASM) → Result → Payment
```

### Gossip Sync
```
Push (announce hash) → Pull (request entry) → Apply
```

## 🎭 Main Actors

```
Supervisor
  ├─ NetworkActor (connections)
  ├─ GossipActor (pub/sub)
  ├─ Ledger (transactions)
  ├─ GovernanceActor (proposals)
  └─ ComputeActor (tasks)
```

## 🎯 Gossip Topics

| Topic | Purpose | Access |
|-------|---------|--------|
| `ledger:sync` | Journal entries | TrustGated(0.1) |
| `compute:submit` | Task submission | TrustGated(0.3) |
| `compute:result` | Task results | TrustGated(0.3) |
| `governance:proposal` | Proposals | Public |
| `governance:vote` | Votes | Public |
| `network:candidates` | NAT traversal | Public |

## 💻 Quick Commands

### Build
```bash
cd icn/
cargo build --release
cargo test --workspace
cargo clippy --workspace
```

### Run
```bash
# Daemon
./target/release/icnd

# With gateway
./target/release/icnd --gateway-enable

# CLI
./target/release/icnctl status
./target/release/icnctl trust add <did> 0.8

# TUI
./target/release/icn-console
```

## 📈 Performance (Ryzen 5900X)

| Metric | Value |
|--------|-------|
| Gossip | 10K msg/s, 5ms p50 |
| Ledger | 5K tx/s, 2ms p50 |
| Trust | 50K ops/s, 0.5ms p50 |
| Signatures | 20K/s |
| Node Scale | 100+ nodes |

## 📁 Key Files

```
icn/
├── bins/icnd/src/main.rs           # Daemon entry
├── crates/icn-core/src/supervisor/ # Actor spawning
├── crates/icn-gossip/src/gossip.rs # Gossip actor
├── crates/icn-ledger/src/ledger.rs # Ledger logic
├── crates/icn-trust/src/graph.rs   # Trust graph
└── crates/icn-net/src/actor.rs     # Network actor
```

## 🧪 Testing

```bash
# All tests (1,134+)
cargo test --workspace

# Specific test with logs
RUST_LOG=debug cargo test <name> -- --nocapture

# Benchmarks
cargo bench -p icn-gossip
```

## 📚 Documentation Hierarchy

```
ARCHITECTURE_INDEX.md       ← Start here (navigation)
    │
    ├─ ARCHITECTURE_VISUAL.md  ← Diagrams & flows
    ├─ ARCHITECTURE_MAP.md     ← Complete inventory
    └─ ../ARCHITECTURE.md      ← Design rationale
```

## 🚦 Current Status

**PILOT-READY** (December 2025)

- ✅ 1,134+ tests passing
- ✅ 25 crates (40K lines Rust)
- ✅ 198 documentation files
- ✅ 85+ integration tests
- ✅ Byzantine fault tolerance
- ✅ Storage replication
- ✅ Distributed compute
- ✅ Federation layer
- 🔄 Privacy (partial)
- 📋 Mobile SDKs (planned)

## 🔗 Next Steps

1. Read [ARCHITECTURE_INDEX.md](./ARCHITECTURE_INDEX.md)
2. Browse [ARCHITECTURE_VISUAL.md](./ARCHITECTURE_VISUAL.md)
3. Study [ARCHITECTURE_MAP.md](./ARCHITECTURE_MAP.md)
4. Deep-dive [ARCHITECTURE.md](../ARCHITECTURE.md)
5. Get started [GETTING_STARTED.md](../GETTING_STARTED.md)

## 📞 Resources

- Repo: https://github.com/InterCooperative-Network/icn
- Docs: `cargo doc --open --workspace`
- License: MIT OR Apache-2.0

---

**Last Updated:** December 17, 2025  
**System Version:** 0.1.0 - PILOT-READY ✅
