# ICN Changelog

All notable changes to the ICN project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - End-to-End Payload Encryption (Phase 10) (2025-11-13)

**X25519-ChaCha20-Poly1305 Message Encryption:**
- **MAJOR FEATURE:** End-to-end encrypted messages for payload confidentiality
- **Encryption Scheme:**
  - ✅ **Key Exchange**: X25519 ECDH (static, upgradeable to ephemeral in future)
  - ✅ **Symmetric Cipher**: ChaCha20-Poly1305 AEAD (authenticated encryption)
  - ✅ **Nonce Derivation**: Deterministic from sequence number (no transmission overhead)
  - ✅ **Key Persistence**: X25519 keys stored in keystore v2.1 format

**Three-Layer Security Architecture:**
```
Application:  EncryptedEnvelope (payload confidentiality)
Message:      SignedEnvelope (authentication + replay protection)
Transport:    QUIC/TLS 1.3 (channel encryption)
```

**Why All Three Layers:**
- **QUIC/TLS**: Protects node-to-node connections (per-hop encryption)
- **SignedEnvelope**: Authenticates sender and prevents replay (message integrity)
- **EncryptedEnvelope**: Hides payload from intermediate gossip nodes (end-to-end confidentiality)

**Implementation:**
- New module: `icn-net/src/encryption.rs` with `EncryptedEnvelope` struct
- IdentityBundle extended with X25519 keypair (bundle.rs)
- Keystore v2.1 format with X25519 key persistence (keystore.rs)
- Automatic v2.0 → v2.1 migration on first unlock
- New PayloadType::Encrypted (value 7) for encrypted messages

**Encryption Flow:**
1. Serialize application payload → plaintext bytes
2. Encrypt with X25519 + ChaCha20-Poly1305 → EncryptedEnvelope
3. Serialize EncryptedEnvelope → encrypted bytes
4. Sign with Ed25519 → SignedEnvelope (PayloadType::Encrypted)
5. Wrap in NetworkMessage::Signed → send over network

**Decryption Flow:**
1. Receive NetworkMessage::Signed
2. Verify Ed25519 signature → extract SignedEnvelope
3. Check PayloadType::Encrypted
4. Deserialize → EncryptedEnvelope
5. Decrypt with X25519 keys → plaintext bytes
6. Deserialize → original application payload

**Security Properties:**
- ✅ **Payload confidentiality**: Intermediate nodes cannot read content
- ✅ **Authenticated encryption**: Poly1305 MAC detects tampering
- ✅ **Replay protection**: Inherited from SignedEnvelope sequence numbers
- ✅ **Nonce uniqueness**: Derived from monotonic sequence + DIDs
- ✅ **Key persistence**: X25519 keys survive daemon restarts

**What It Doesn't Provide (Yet):**
- ❌ **Perfect Forward Secrecy**: Static ECDH reuses shared secrets (can add ephemeral keys in Phase 11)
- ❌ **Metadata hiding**: Sender/recipient DIDs still visible
- ❌ **Protection against node compromise**: Attacker with memory access can read keys

**Performance:**
- Encryption overhead: ~0.3-0.7ms per 1KB message
- Memory overhead: 64 bytes per peer (X25519 public key cache)
- Nonce derivation: Zero transmission overhead (computed locally)

**Testing:**
- Unit tests: 8 encryption tests (roundtrip, tampering, nonce uniqueness, edge cases)
- Integration tests: 6 end-to-end tests (encrypt→sign→verify→decrypt flow)
- All 19 icn-identity tests pass (bundle + keystore with X25519)
- All 64 icn-net tests pass (encryption module + integration)

**Keystore Migration:**
- **v2.0 → v2.1 migration**: Automatic on first unlock
- Generates X25519 keypair and saves immediately to disk
- Backward compatible: v1 → v2.1 migration also supported
- Log messages: "Unlocked v2.1+ keystore with X25519 keys" or "Upgrading to v2.1"

**Dependencies Added:**
- `chacha20poly1305 = "0.10"` (workspace)
- `x25519-dalek` already imported (now used)
- `zeroize` for secure memory handling

**Usage Example:**
```rust
// 1. Get identity bundles (contain X25519 keys)
let alice_bundle = keystore.get_identity_bundle()?;
let bob_bundle = /* lookup Bob's bundle */;

// 2. Encrypt message
let plaintext = bincode::serialize(&my_message)?;
let encrypted = EncryptedEnvelope::encrypt(
    alice_bundle.did(),
    bob_bundle.did(),
    sequence_number,
    &alice_bundle.x25519_secret(),
    &bob_bundle.x25519_public(),
    &plaintext,
)?;

// 3. Sign encrypted envelope
let signed = SignedEnvelope::from_payload(
    alice_bundle.did(),
    alice_bundle.keypair(),
    sequence_number,
    PayloadType::Encrypted,
    &encrypted,
)?;

// 4. Send via NetworkMessage::Signed
```

### Added - Gossip Message Authentication (2025-11-13)

**Cryptographically Signed Gossip Messages:**
- **MAJOR CHANGE:** All gossip messages now use SignedEnvelope for authentication
- **Security Properties:**
  - ✅ **Ed25519 authentication**: Every gossip message is cryptographically signed
  - ✅ **Replay protection**: Sequence numbers with Bloom filter detection
  - ✅ **Sender verification**: Impossible to forge messages from other DIDs
  - ✅ **Freshness checking**: Timestamped messages with 300s max age
  - ✅ **Non-repudiation**: Senders cannot deny sending authenticated messages

**Implementation:**
- GossipActor now holds optional keypair for signing outgoing messages
- Sequence counter (AtomicU64) tracks monotonically increasing message numbers
- Send callback creates SignedEnvelope with PayloadType::Gossip
- Receive path decodes and verifies signed gossip messages
- Automatic verification via NetworkActor's ReplayGuard

**Message Flow:**
- **Send:** `GossipActor.publish() → SignedEnvelope::from_payload() → NetworkMessage::signed() → network send`
- **Receive:** `NetworkActor verifies → decode PayloadType::Gossip → handle_message() with authenticated sender`

**Message Size Impact:**
- SignedEnvelope overhead: ~141 bytes per message
  - DID (from): ~60 bytes
  - Sequence number: 8 bytes
  - Timestamp: 8 bytes
  - Payload type: 1 byte
  - Ed25519 signature: 64 bytes
- **Announce messages:** 230B → 371B (+61%)
- **Request messages:** 32B → 173B (+441%, but small absolute size)
- **Response messages (2KB):** 2KB → 2.1KB (+7%)

**Backward Compatibility:**
- ⚠️ **BREAKING CHANGE:** New nodes only send signed messages
- Old MessagePayload::Gossip receive path still exists for compatibility
- Recommended: Coordinate network-wide upgrade or implement dual-mode receiver

**Testing:**
- All 262 library tests pass
- Gossip tests: 52 passing (signed message flow verified)
- Network tests: 53 passing (SignedEnvelope + ReplayGuard)
- Core integration tests: 26 passing

**Impact:**
- First major protocol to use Phase 9 SignedEnvelope infrastructure
- Demonstrates end-to-end message authentication pattern
- **Automatically protects all protocols that use gossip:**
  - ✅ **Ledger sync** - Already authenticated (publishes via gossip topics)
  - ✅ **Trust attestations** - Dual-layer protection (entry + network signatures)
  - ✅ **Contract deployment** - Network-level authentication inherited
- Eliminates trust in "from" field (now cryptographically verified)

### Fixed - Critical: TLS Certificate Persistence (2025-11-13)

**Keystore Migration Bug Fix:**
- **CRITICAL:** Fixed v1-to-v2 keystore migration to persist TLS certificates to disk
  - **Problem:** TLS certificates were regenerated on every daemon restart for v1 keystores
  - **Impact:** Violated Phase 8 requirement that "TLS certificates persist across restarts"
  - **Root Cause:** TODO at line 245 in keystore.rs was never implemented
  - **Fix:** Auto-save upgraded v2 keystore immediately after generating TLS binding
  - **Security Impact:** HIGH - Required for Phase 8 DID-TLS binding integrity

**What Was Broken:**
- When unlocking a v1 keystore (KeyPair-only format), the system generated an `IdentityBundle` with TLS binding in memory
- The TODO comment indicated this should be persisted, but the code only stored the bundle in memory
- The keystore file on disk remained in v1 format
- Every subsequent unlock generated a new TLS certificate with different cryptographic material
- Peers would see different TLS certificates on each daemon restart
- TLS session stability and trust establishment were broken

**How It Was Fixed:**
- Modified `unlock()` method in `icn-identity/src/keystore.rs` (lines 245-260)
- After generating `IdentityBundle` for v1 migration:
  1. Create complete `StoredKey` with all TLS binding fields populated
  2. Call `encrypt_and_save()` to persist immediately to disk
  3. Log success message confirming migration
- This ensures v1 keystores upgrade to v2 format on first unlock
- TLS certificates remain stable across all subsequent unlocks and restarts

**Testing:**
- Added comprehensive test: `test_v1_to_v2_migration_persists_tls()`
- Test verifies:
  - v1 keystore migrates on first unlock
  - TLS certificate is identical on second unlock (not regenerated)
  - TLS certificate persists to disk (verified by new keystore instance)
  - Binding signature remains stable across unlocks
- All 19 icn-identity tests pass

**Security Properties Restored:**
- ✅ TLS certificates persist across daemon restarts
- ✅ DID-TLS binding integrity maintained
- ✅ Peers see consistent TLS certificates
- ✅ Trust establishment stability ensured
- ✅ Phase 8 security requirements met

### Added - Phase 8A: Trust Network Propagation (2025-01-12)

**Trust Attestation System:**
- **Signed trust attestations** with Ed25519 cryptographic signatures
  - `TrustAttestation` message format with issuer, subject, score, TTL, and signature
  - Deterministic signing payload (SHA256 hash of sorted fields)
  - Signature verification extracting verifying key from DIDs
  - TTL-based expiration (default: 30 days) with automatic decay
  - Conversion to/from `TrustEdge` for seamless storage integration
- **`trust:attestations` gossip topic** for network-wide trust propagation
  - Access control: `TrustClass::Known` (requires trust score ≥0.1)
  - Prevents spam from untrusted/isolated nodes
  - Integrates with existing gossip infrastructure
- **Trust propagation module** (`icn-core/src/trust_propagation.rs`)
  - `broadcast_trust_attestation()` - Signs and publishes attestations
  - `handle_trust_attestation_entry()` - Verifies and applies remote attestations
  - Deduplication: only accepts newer attestations (by `created_at` timestamp)
  - Automatic notification callback integration with gossip subscriptions
- **Supervisor wiring** for incoming attestation handling
  - Notification callback processes trust attestations reactively
  - Automatic subscription to `trust:attestations` topic
  - Spawns async tasks for non-blocking attestation processing

**Observability:**
- **Prometheus metrics** for trust propagation:
  - `icn_trust_attestations_broadcasted_total` - Outbound attestations
  - `icn_trust_attestations_received_total` - Inbound attestations
- Enable monitoring of trust graph growth and network health

**Testing:**
- **14 unit tests** for trust attestations (100% pass rate)
  - Signature creation, verification, and tampering detection
  - Expiry checking and TTL management
  - TrustEdge conversion roundtrips
  - Signing payload determinism
- **2 integration tests** for end-to-end trust propagation
  - Two-node trust propagation with full QUIC/TLS stack
  - Three-node transitive trust computation verification
  - Real gossip network with announce/pull cycles

**Architecture:**
- Trust edges now propagate across the network via signed attestations
- Nodes build distributed trust webs automatically
- Transitive trust computation works across remote trust edges
- Foundation for trust-based governance and cooperation

**Security Features:**
- Cryptographic signature verification prevents forgery
- Timestamp monotonic checks mitigate replay attacks
- TTL expiration prevents stale trust information
- Trust-gated topic access prevents spam flooding

**Performance:**
- Average attestation size: ~300 bytes (JSON-serialized)
- Signature overhead: 64 bytes (Ed25519)
- Propagation latency: <1 second for 2-hop networks
- Gossip compression for larger attestations (>1KB)

**Impact:**
- **Closes the biggest gap** in ICN's distributed cooperation infrastructure
- Enables truly distributed trust building (no central authority)
- Foundation for Phase 8B (trust-gated security) and Phase 8C (WAN discovery)
- First step toward federated trust networks

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

### Added - Phase 3 CLI Tools & Production Features (2025-11-11)

**Contract Examples:**
- **`examples/contracts/echo.json`** - Simple test contract demonstrating basic CCL features
  - `echo(message)` - Returns message parameter
  - `add(a, b)` - Adds two numbers using BinOp
- **`examples/contracts/timebank.json`** - Mutual credit time banking contract
  - State variable: `total_hours_exchanged`
  - `record_service(recipient, hours)` - Records service exchange with preconditions
  - `get_stats()` - Returns total hours exchanged
  - Demonstrates: state variables, ledger operations, preconditions, special `sender` variable
- **`examples/contracts/README.md`** - Comprehensive contract development documentation
- **`examples/contracts/test-contracts.sh`** - Automated testing script for contract validation

**Contract Management:**
- Contract listing functionality: `icnctl contract list`
  - Displays installed contracts with metadata (name, participants, currency, rules)
  - Shows state variable count and rule names
  - RPC endpoint: `contract.list`

**Quarantine Management (PR #1):**
- Full operator control over quarantined ledger entries
- **RPC Endpoints:**
  - `ledger.quarantine.list` - List all quarantined entries
  - `ledger.quarantine.get` - Get detailed info about specific entry
  - `ledger.quarantine.release` - Release and retry entry
  - `ledger.quarantine.drop` - Permanently discard entry
  - `ledger.quarantine.purge` - Remove all expired entries
- **CLI Commands:**
  ```bash
  icnctl ledger quarantine list
  icnctl ledger quarantine get <entry_id>
  icnctl ledger quarantine release <entry_id>
  icnctl ledger quarantine drop <entry_id>
  icnctl ledger quarantine purge
  ```
- **RPC Client Methods** in `icn-rpc/src/client.rs`:
  - `quarantine_list()`, `quarantine_get()`, `quarantine_release()`, `quarantine_drop()`, `quarantine_purge()`

**WAN Bootstrap Peers (PR #2):**
- Internet-wide connectivity beyond local mDNS discovery
- Configure bootstrap peers in `icn.toml`:
  ```toml
  bootstrap_peers = [
      "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777"
  ]
  ```
- URL format: `icn://DID@IP:PORT`
- Automatic dialing on daemon startup
- Multiple peers for redundancy (no single point of failure)
- Connection failures are non-fatal (logged as warnings)
- Current limitation: IP addresses only (DNS hostname resolution to be added later)

### Fixed - Phase 3 Error Handling (2025-11-11)

**Quarantine Release Semantics:**
- Fixed incorrect error handling in `ledger.quarantine.release`
  - Operation now returns JSON-RPC error when entry release succeeds but reappend fails
  - Previously returned success response with error flags (violated JSON-RPC 2.0 semantics)
  - Error message format: "Entry released from quarantine but reappend failed: <reason>"
  - Follows standard JSON-RPC pattern: errors in error field, successes in result field
- **Rationale**: Operation name "release" implies "release for retry" - partial success is a failure

**Impact:**
- Operators can now inspect, manage, and resolve quarantined ledger entries
- WAN connectivity enables internet-wide ICN networks
- Contract examples provide learning resources and test cases
- Proper JSON-RPC error handling enables reliable error detection in monitoring tools

### Added - Trust-Gated Rate Limiting (PR #3) (2025-11-11)

**Dynamic Rate Limiting Based on Trust:**
- Different message rate limits for each trust class:
  - **Isolated peers** (trust score < 0.1): 10 messages/sec, burst capacity 2
  - **Known peers** (trust score 0.1-0.4): 50 messages/sec, burst capacity 10
  - **Partner peers** (trust score 0.4-0.7): 100 messages/sec, burst capacity 20
  - **Federated peers** (trust score 0.7+): 200 messages/sec, burst capacity 50
- Rate limits automatically adjust when peer trust changes
- Immediate benefit for trust upgrades (token bucket reset to new capacity)
- Backwards compatible: Falls back to 100 msg/sec when no trust graph available

**Architecture:**
- `TrustGatedRateLimitConfig` in `icn-net/src/rate_limit.rs`
- `RateLimiter::new_trust_gated()` integrates with trust graph
- Token buckets track trust class and detect changes
- Trust graph shared between Gossip and Network actors
- Trust data persisted in `~/.icn/trust/` directory

**Testing:**
- 3 comprehensive unit tests for trust-gated behavior
- Tests verify different limits for each trust class
- Tests verify dynamic adjustment on trust class changes
- All 140+ tests passing

**Impact:**
- Provides robust DoS protection against untrusted peers (10 msg/sec limit)
- Enables high throughput for trusted partners (200 msg/sec for federated peers)
- Adaptive security: protection strengthens/weakens based on actual trust relationships
- No configuration required: works automatically based on trust graph state

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
