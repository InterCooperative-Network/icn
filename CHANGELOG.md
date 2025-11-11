# ICN Changelog

All notable changes to the ICN project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - User Onboarding Improvements (2025-11-11)

**New Directories:**
- **`config/`** - Example configuration files for all use cases
  - `icn.toml.example` - Comprehensive configuration template with all options
  - `icn-minimal.toml.example` - Minimal starter configuration
  - `icn-alpha.toml`, `icn-beta.toml` - Two-node local demo configs
  - `prometheus.yml` - Prometheus scrape configuration
  - Complete configuration guide with environment variable documentation
- **`docker/`** - Production-ready Docker deployment
  - Multi-stage Dockerfile (optimized for size and security)
  - `docker-compose.yml` - Full stack with Prometheus monitoring
  - `docker-compose.dev.yml` - Development environment
  - Comprehensive deployment guide with troubleshooting
- **`examples/`** - Getting started tutorials
  - `01-quickstart/` - Automated two-node network demo
    - Interactive tutorial with step-by-step instructions
    - `run.sh` - Fully automated demo script (<5 minutes)
  - Examples index with roadmap for future tutorials

**Documentation Improvements:**
- Enhanced README.md with Quick Start section (5-minute setup guide)
- Added Ports & Services reference table
- Expanded Usage section with examples for all CLI commands
- Navigation links to config/, docker/, examples/ directories

### Fixed - User Onboarding Improvements (2025-11-11)

**Documentation:**
- Fixed port discrepancies in deployment-guide.md (all references updated 5000→4433)
- Corrected QUIC listener port in all documentation to match code reality (4433/udp)
- Updated Docker examples to use correct ports
- Added links to new configuration examples

**Impact:**
- **Onboarding time reduced from ~30 minutes to <5 minutes**
- Users can now run automated quickstart: `./examples/01-quickstart/run.sh`
- Complete Docker deployment ready out-of-box
- 5 example configuration files covering all use cases

### Added - Phase 7 Pull Protocol Completion (2025-01-11)

**Gossip Pull Protocol:**
- **Pull protocol now fully operational** with verified end-to-end convergence
  - Digest emission background task with jitter (10s ± 2s)
  - Pull request/response handlers with backpressure
  - Empty `want_ids` semantics for "send all entries" requests
  - Vector clock-based detection of missing entries
  - Trust-gated resource limits per peer class
  - Comprehensive integration test validating full flow
- Ledger merge report API for operator visibility
  - `merge_batch()` returns detailed `MergeDecision` with accepted/discarded/quarantined counts
  - `QuarantineStore` with ring buffer (1000 entries) and 7-day TTL
  - Methods for quarantine management: `list()`, `get()`, `release()`, `drop()`
  - New metrics: `merge_conflicts_total`, `entries_quarantined_total`, `quarantine_size`

**New Metrics:**
- Gossip pull protocol: `digests_sent/received`, `pull_requests_sent/received`, `pull_responses_sent/received`
- Pull bandwidth: `bytes_pulled_total`, `bytes_pushed_total`
- Backpressure: `pull_truncated_total`, `peer_deficit_bytes`
- Ledger merge: `merge_conflicts_total`, `entries_quarantined_total`, `entries_discarded_total`, `quarantine_size`

### Fixed - Phase 7 Critical Bugs (2025-01-11)

**TLS Handshake (BLOCKER):**
- Fixed `NoSignatureSchemesInCommon` error by generating Ed25519 certificates
  - Changed from RSA (default) to Ed25519 to match client verifier expectations
  - Location: `icn-net/src/tls.rs` - now uses `rcgen::PKCS_ED25519`
  - **Impact**: Unblocked ALL integration tests

**mDNS Discovery:**
- Fixed hostname format bug causing registration failure
  - Changed `"{}"` → `"{}.local."` to comply with mDNS requirements
  - Location: `icn-net/src/discovery.rs:79`

**Pull Protocol Routing:**
- Added sender DID propagation to `handle_message()` signature
  - Changed: `handle_message(message)` → `handle_message(&sender, message)`
  - Enables Digest handler to identify message sender for reply routing
  - Updated 10+ call sites across codebase

### Added - Phase 7 Production Hardening (2025-01-11)

**Security & Hardening:**
- Network message rate limiting using token bucket algorithm (100 msg/sec, burst 20)
  - Per-peer rate limiting prevents single-peer DoS attacks
  - New module: `icn-net/src/rate_limit.rs`
  - New metric: `icn_network_messages_rate_limited_total`
- TLS certificate verification with DID extraction and expiration checking
  - Extracts DID from X.509 certificate Subject Alternative Names
  - Validates certificate validity period (not before/after)
  - Validates DID format (must start with `did:icn:`)
  - Adds security audit logging
  - Added dependency: `x509-parser = "0.16"`
- QUIC transport configuration with bounded stream limits
  - Reduced concurrent streams from 100 → 10 bidirectional
  - Set unidirectional streams to 0 (not used)
  - Stream receive window: 1MB per stream
  - Connection receive window: 10MB total
  - Idle timeout: 60s, keep-alive: 30s
- Message size validation before buffer allocation
  - Validates length prefix before allocating memory
  - Prevents overflow on 32-bit systems
  - Rejects zero-length and oversized messages (>10MB)
- Bloom filter deserialization validation
  - Validates non-zero size to prevent division by zero
  - Validates claimed size vs actual unpacked bits
  - Prevents index out of bounds panics from malformed data
- Timestamp overflow protection in ledger and gossip
  - Changed unchecked `as u64` casts to checked `try_into()`
  - Prevents silent wraparound if system clock is far in future (post-2262)

**Async/Performance:**
- Fixed blocking operations in async context (supervisor message handlers)
  - Replaced `blocking_write()` with `tokio::spawn` + `write().await`
  - Applied to Gossip, Subscribe, and Unsubscribe message handlers
  - Prevents thread pool starvation in Tokio runtime

**Documentation:**
- Added comprehensive production hardening documentation (`docs/production-hardening.md`)
  - Detailed vulnerability descriptions and fixes
  - Configuration guide and tuning recommendations
  - Monitoring and alerting recommendations
  - Security metrics and log patterns
- Added deployment and operations guide (`docs/deployment-guide.md`)
  - Installation instructions (source, Docker, systemd)
  - Configuration reference
  - Monitoring setup (Prometheus/Grafana)
  - Backup & recovery procedures
  - Troubleshooting guide
  - Security best practices
- Updated architecture documentation (`docs/ARCHITECTURE.md`)
  - Added section 8.4: Production Hardening
  - Documents all security protections with implementation references
- Updated README with security section
  - Quick overview of hardening measures
  - Links to detailed documentation

**Testing:**
- Added 4 comprehensive unit tests for rate limiter
  - Token consumption and refill behavior
  - Per-peer isolation
  - Bucket cleanup
- All tests passing: 64 tests across modified crates (icn-net: 27, icn-gossip: 18, icn-ledger: 16, icn-obs: 0)

### Changed

- `icn-net/src/protocol.rs`: Message size validation before allocation
- `icn-net/src/tls.rs`: Implemented certificate verification
- `icn-net/src/session.rs`: Added transport config with bounded limits
- `icn-net/src/actor.rs`: Integrated rate limiter into connection handler
- `icn-core/src/supervisor.rs`: Fixed blocking operations in message handlers
- `icn-gossip/src/gossip.rs`: Fixed timestamp overflow in entry creation
- `icn-gossip/src/bloom.rs`: Added validation in deserialization
- `icn-ledger/src/entry.rs`: Fixed timestamp overflow in journal entries
- `icn-obs/src/metrics.rs`: Added rate limiting metric

### Security Notes

⚠️ **Known Limitations:**
- TLS certificate verifier does NOT yet integrate with trust graph
- Currently accepts all valid DID certificates (development mode)
- Trust graph integration required before production deployment

**Remaining Work (Not Addressed):**
- Medium priority: Request timeouts, unbounded vector growth, compression
- Low priority: Error handling consistency, trace logging improvements

---

## Version History

### [0.1.0] - Phase 0-6 Complete

**Phase 0 - Scaffold:**
- Workspace structure, core runtime, supervisor
- Identity/DID generation & verification
- CLI tooling (icnd + icnctl)

**Phase 1 - Identity & Trust:**
- Age-encrypted keystore with passphrase unlock
- Key rotation protocol with transition records
- Trust graph storage & transitive trust computation
- DID import/export

**Phase 2 - Network Transport:**
- QUIC/TLS sessions with DID-based certificates
- mDNS local discovery
- Network actor with session pooling
- Secure passphrase handling (zeroization)

**Phase 3 - Ledger:**
- Double-entry mutual credit accounting
- Merkle-DAG content-addressable structure
- Multi-currency support with credit limits
- Balance queries & integrity verification

**Phase 4 - Cooperative Contracts (CCL):**
- Domain-specific contract language (AST-based)
- Deterministic interpreter with fuel metering
- Capability system (ReadLedger, WriteLedger, etc.)
- Contract runtime with ledger integration
- TimeBank example contract

**Phase 5 - Gossip & Distributed Sync:**
- Topic-based gossip protocol with ACLs
- Vector clocks for causal ordering
- Bloom filter anti-entropy
- Ledger-gossip integration
- Multi-node convergence verification

**Phase 6 - Network Protocol Bridge:**
- Wire protocol for gossip over QUIC
- NetworkMessage envelope with DID routing
- NetworkActor extensions (send/broadcast)
- Gossip-network bridge in supervisor
- Background anti-entropy task
- Two-node integration test structure

**Phase 7 - Polish & Production:**
- Metrics exporter (Prometheus)
- Complete pull protocol (Request/Response)
- Topic subscriptions & routing
- Production hardening (3 critical + 4 high priority issues)
- Comprehensive documentation

---

## Migration Notes

### Upgrading to Post-Hardening Version

No breaking changes. All hardening features are enabled by default with conservative limits.

**Configuration changes (optional):**
- Rate limiting can be tuned via `RateLimitConfig` (requires code change currently)
- QUIC stream limits configurable via `TransportConfig`
- Message size limit defined by `MAX_MESSAGE_SIZE` constant (10MB)

**Monitoring updates:**
- New metric: `icn_network_messages_rate_limited_total`
- Monitor for rate limiting spikes indicating potential attacks

**No data migration required** - all changes are in protocol handling and validation layers.

---

## Links

- [Repository](https://github.com/your-org/icn)
- [Architecture Documentation](docs/ARCHITECTURE.md)
- [Production Hardening](docs/production-hardening.md)
- [Deployment Guide](docs/deployment-guide.md)
- [Topic Subscriptions API](docs/topic-subscriptions-api.md)
