# ICN - Intercooperative Network

A substrate daemon for the cooperative internet.

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

**Phase 0 - Scaffold: Complete ✓**
- [x] Workspace structure, core runtime, supervisor
- [x] Identity/DID generation & verification
- [x] CLI tooling (icnd + icnctl)

**Phase 1 - Identity & Trust: Complete ✓**
- [x] Age-encrypted keystore with passphrase unlock
- [x] Key rotation protocol with transition records
- [x] Trust graph storage & transitive trust computation
- [x] DID import/export

**Phase 2 - Network Transport: Complete ✓**
- [x] QUIC/TLS sessions with DID-based certificates
- [x] mDNS local discovery
- [x] Network actor with session pooling
- [x] Secure passphrase handling (zeroization)

**Phase 3 - Ledger: Complete ✓**
- [x] Double-entry mutual credit accounting
- [x] Merkle-DAG content-addressable structure
- [x] Multi-currency support with credit limits
- [x] Balance queries & integrity verification

**Phase 4 - Cooperative Contracts (CCL): Complete ✓**
- [x] Domain-specific contract language (AST-based)
- [x] Deterministic interpreter with fuel metering
- [x] Capability system (ReadLedger, WriteLedger, etc.)
- [x] Contract runtime with ledger integration
- [x] TimeBank example contract

**Phase 5 - Gossip & Distributed Sync: Complete ✓**
- [x] Topic-based gossip protocol with ACLs
- [x] Vector clocks for causal ordering
- [x] Bloom filter anti-entropy
- [x] Ledger-gossip integration
- [x] Multi-node convergence verification

**Phase 6 - Network Protocol Bridge: Complete ✓**
- [x] Wire protocol for gossip over QUIC
- [x] NetworkMessage envelope with DID routing
- [x] NetworkActor extensions (send/broadcast)
- [x] Gossip-network bridge in supervisor
- [x] Background anti-entropy task
- [x] Two-node integration test structure

**Phase 7 - Polish & Production: In Progress**
- [ ] Metrics exporter (Prometheus)
- [x] Complete pull protocol (Request/Response)
- [ ] Topic subscriptions & routing
- [ ] Production hardening
- [ ] Comprehensive documentation

## Building

```bash
cargo build --release
```

## Usage

Start the daemon:

```bash
./target/release/icnd
```

Generate a DID:

```bash
./target/release/icnctl id generate
```

Check status:

```bash
./target/release/icnctl status
```

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
- `icn-testkit` - Test utilities

### Binaries

- `icnd` - The ICN daemon
- `icnctl` - CLI management tool

## License

MIT OR Apache-2.0
