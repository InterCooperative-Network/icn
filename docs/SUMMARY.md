# ICN Architecture Summary

Quick reference for the comprehensive [ARCHITECTURE.md](./ARCHITECTURE.md).

## Core Decisions

### Identity
- **Format:** `did:icn:<base58btc-ed25519-pubkey>`
- **Keys:** Ed25519 (signing), X25519 (encryption, future)
- **Storage:** Age-encrypted files, pluggable for HSM
- **Rotation:** Signed transition records, preserves trust history

### Trust
- **Model:** Directed labeled edges with evidence chains
- **Computation:** Local PageRank-like (transitive trust)
- **Bootstrap:** Manual vouching + invite codes
- **Attack resistance:** Sybil-resistant via transitive trust

### Network
- **Transport:** QUIC + TLS 1.3 mutual auth
- **Discovery:** mDNS (LAN) + Rendezvous (WAN) + Manual
- **NAT:** Hole punching + relay fallback
- **Limits:** Trust-gated connection limits

### Ledger
- **Model:** Double-entry append-only
- **Structure:** Merkle-DAG (content-addressed)
- **Conflict resolution:** Deterministic merge with constraint checking
- **Currency:** Multi-currency, per-contract definitions
- **Credit limits:** Per-participant, dynamically adjustable

### Contracts (CCL)
- **v1:** Domain-specific language (DSL), deterministic interpreter
- **v2:** WASM sandbox with gas metering
- **Capabilities:** Explicit permissions, least privilege
- **Upgrade:** Migration with participant consent

### Sync
- **Consistency:** Causal (vector clocks)
- **Protocol:** Hybrid push/pull with bloom filters
- **Topics:** Scoped gossip channels with ACLs
- **Bandwidth:** Adaptive rate limiting, QoS

### Storage
- **Backend:** Pluggable (Sled v1, RocksDB v2)
- **Schema:** Versioned with migrations
- **Retention:** Configurable pruning + archive

## Phase Roadmap

### Phase 0: Scaffold ✓
- Tokio runtime, supervisor, actors
- Identity: DID generation, sign/verify
- CLI: icnd + icnctl
- Storage: trait + Sled impl

### Phase 1: Identity & Trust
- Persistent key storage (Age encryption)
- Key rotation protocol
- Trust graph storage + computation
- DID import/export

### Phase 2: Networking
- mDNS discovery
- QUIC/TLS sessions
- Peer lifecycle management
- NAT traversal

### Phase 3: Ledger
- Double-entry journal
- Merkle-DAG structure
- Balance queries
- Anti-entropy sync

### Phase 4: Contracts (CCL)
- DSL parser + interpreter
- Capability system
- Contract installation + invocation
- Basic mutual credit example

### Phase 5: Gossip
- Topic bus with ACLs
- Bloom filter anti-entropy
- Rate limiting + QoS

### Phase 6: Polish
- Metrics exporter
- Snapshots + backup
- Systemd hardening
- Documentation

## Security Principles

1. **Fail closed:** Deny by default
2. **Trust is earned:** New nodes start with zero trust
3. **Verify everything:** Signatures, invariants, constraints
4. **Explicit capabilities:** Least privilege
5. **Auditable:** All actions logged, traceable

## Performance Targets (v1)

| Metric | Target |
|--------|--------|
| Ledger write latency | <100ms |
| Ledger sync latency | <1s (LAN), <5s (WAN) |
| Contract execution | <50ms |
| Concurrent peers | 500 |
| Throughput | 100 tx/sec per node |

## Key Files

- [ARCHITECTURE.md](./ARCHITECTURE.md) - Full architectural design
- [dev-journal/](./dev-journal/) - Development narrative
- [README.md](./README.md) - Documentation guide

## Quick Links

- **Repository:** https://github.com/InterCooperative-Network/icn
- **Issues:** https://github.com/InterCooperative-Network/icn/issues
- **Crates:** [icn/crates/](../icn/crates/)
- **Binaries:** [icn/bins/](../icn/bins/)

---

For comprehensive details, see [ARCHITECTURE.md](./ARCHITECTURE.md).
