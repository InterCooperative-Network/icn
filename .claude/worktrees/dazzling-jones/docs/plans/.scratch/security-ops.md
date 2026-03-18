# Security / Ops / Resilience — Repo State & Gap Analysis

> Compiled 2026-02-14 by Security/Ops/Resilience Analyst
> Branch: `main` @ `86677ac6`

## 1. Current Security Posture

### 1.1 Transport Security

**QUIC/TLS implementation** — `icn-net/src/session.rs`, `icn-net/src/tls.rs`

| Property | Status | Detail |
|----------|--------|--------|
| QUIC transport | ✅ Implemented | Quinn 0.11 + Rustls 0.23 (memory-safe, no OpenSSL) |
| Mutual TLS | ✅ Implemented | `DidCertificateVerifier` validates client certs (`tls.rs:81-195`) |
| Self-signed certs | ✅ Bound to DID | `IdentityBundle` generates self-signed X.509 with DID in SAN (`icn-identity/src/bundle.rs:93-139`) |
| DID-TLS binding | ✅ Verified | `Signature = Sign_did_key(SHA256(tls_cert))` verified in Hello handler (`handlers/hello.rs:47-53`) |
| Certificate expiry | ✅ Checked | `check_expiration()` in `DidCertificateVerifier` |
| TOFU model | ✅ Deployed | Default `min_trust_threshold = 0.0` for bootstrap; configurable per deployment |
| Length-prefixed framing | ✅ With limits | `MAX_MESSAGE_SIZE` (10MB) validated before allocation (`protocol.rs:143-167`) |
| Version negotiation | ✅ In Hello | `VersionInfo` + `CapabilityFlags` exchanged during handshake (`handlers/hello.rs:78-120`) |

**Key observations**:
- Three critical vulnerabilities (unauthenticated QUIC, unverified DID-TLS binding, gateway scope escalation) were identified and fixed in Dec 2025 security hardening session (`docs/archive/2025/security-hardening-2025-12-18.md`)
- Rate limiting currently checks `message.from` **before** signature verification — attacker can forge `from` to exhaust another peer's rate limit budget (noted in hardening doc as "Low Priority Enhancement")

### 1.2 Message Security

**SignedEnvelope** — `icn-net/src/envelope.rs:61`

| Property | Status | Detail |
|----------|--------|--------|
| Ed25519 signatures | ✅ | 64-byte signature over `(from, sequence, timestamp, payload_type, payload)` |
| Hybrid Ed25519 + ML-DSA | ✅ Feature-gated | `SignatureType::Hybrid` with ~3.4KB combined signature (`envelope.rs:31-42`) |
| Timestamp freshness | ✅ | Millisecond-precision Unix timestamp in envelope |
| Sequence ordering | ✅ | Monotonic per-sender sequence number |
| Payload type discriminator | ✅ | `PayloadType` enum for message classification |

**ReplayGuard** — `icn-net/src/replay_guard.rs`

| Property | Status | Detail |
|----------|--------|--------|
| Per-sender sequence windows | ✅ | `HashMap<Did, SequenceWindow>` with Bloom filters |
| Persistent state | ✅ | Sled-backed `max_seq` and `finalized` sequences survive restarts |
| Restart safety gap | ✅ | +1000 gap on startup prevents edge-case replays (`RESTART_SAFETY_GAP = 1_000`) |
| Bloom filter rotation | ✅ | Rotation at 80% capacity (8000 entries), capacity 10000 |
| Nonce uniqueness (outgoing) | ✅ | `OutgoingSequenceTracker` with +10000 restart gap, Sled persistence, 50K max pairs |

**Remaining concern**: Bloom filter saturation over very long-running sessions (noted in hardening doc).

### 1.3 Identity Security

**Keystore** — `icn-identity/src/keystore.rs`

| Property | Status | Detail |
|----------|--------|--------|
| At-rest encryption | ✅ | Age encryption with scrypt-based passphrase derivation |
| Key zeroization | ✅ | `Zeroizing<[u8; 32]>` for Ed25519 secrets; `#[derive(Zeroize, ZeroizeOnDrop)]` on ML-DSA keys |
| Passphrase handling | ✅ | `SecretString` wrapping, never logged, env var priority |
| DID format | ✅ | `did:icn:<multibase-base58btc-ed25519-pubkey>`, validated on deserialization |
| Key rotation | ✅ | `RotationRequest` with version chain, old-key signs new-key (`keybundle.rs:211-253`) |
| Migration chain | ✅ | v1→v2→v2.1→v3→v4→v5, backward-compatible loading |
| Hardware key support | ⏳ Scaffolding | `DidSigner` trait + PKCS#11/TPM backends stubbed (`backend_factory.rs:68-84`) — NOT functional |

**Social Recovery** — `icn-identity/src/recovery.rs:1-831`

| Property | Status | Detail |
|----------|--------|--------|
| M-of-N threshold | ✅ | Configurable trustees + threshold |
| Delay period | ✅ | Time-locked finalization (fraud detection window) |
| Attestation signatures | ✅ | `"ICN_RECOVERY_ATTESTATION:<old_did>:<new_did>:<timestamp>"` signed by trustees |
| Gossip integration | ✅ | `"identity:recovery"` topic for ceremony coordination |
| Status machine | ✅ | `Pending → Delayed → ReadyToFinalize → Finalized` with cancellation |

### 1.4 Trust-Gated Access Control

**Trust graph** — `icn-trust/src/`

| Property | Status | Detail |
|----------|--------|--------|
| Trust classes | ✅ | Isolated (<0.1), Known (0.1-0.4), Partner (0.4-0.7), Federated (0.7+) |
| Transitive computation | ✅ | Multi-hop trust score derivation |
| Rate limiting per class | ✅ | 10/20/100/200 msg/sec per trust tier |
| PolicyOracle pattern | ✅ | `TrustPolicyOracle` translates scores to `ConstraintSet` across meaning firewall |
| Topic subscription gates | ✅ | `subscribe()` checks oracle + ACL + per-peer limits (`subscriptions.rs:34-218`) |
| Misbehavior recording | ✅ | Policy denials, ACL violations, limit breaches all recorded as `Violation` |

### 1.5 Gateway Security

**Auth flow** — `icn-gateway/src/auth.rs`, `icn-gateway/src/api/auth.rs`

| Property | Status | Detail |
|----------|--------|--------|
| DID challenge-response | ✅ | 32-byte random nonce, 5-min TTL, Ed25519 verification |
| Constant-time auth | ✅ | Timing-attack resistant: dummy verification on parse failures, bitwise OR for decision (`auth.rs:162-236`) |
| JWT tokens | ✅ | HS256, 1-hour expiry, scopes + coop_id claims |
| JWT secret enforcement | ✅ | Minimum 32 bytes required (`server.rs:384-401`) |
| Scope whitelist | ✅ | 16 allowed scopes, rejects unknown (`validation.rs:44-76`) |
| Cross-coop isolation | ✅ | `require_coop_access()` on every protected route (`middleware.rs:88-100`) |
| Sender verification | ✅ | Payment endpoints verify `claims.sub == req.from` (`api/ledger.rs:67-78`) |
| Input validation | ✅ | Length limits on all fields (coop_id: 64, memo: 1024, proposal desc: 10000) |

**Security headers** — `icn-gateway/src/security.rs`

| Header | Value |
|--------|-------|
| CSP | `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; ...` |
| X-Frame-Options | `DENY` |
| X-Content-Type-Options | `nosniff` |
| HSTS | `max-age=31536000; includeSubDomains` |
| Referrer-Policy | `strict-origin-when-cross-origin` |
| Permissions-Policy | Disables geolocation, microphone, camera, payment |

**Rate limiting layers** — `icn-gateway/src/rate_limit.rs`

| Layer | Target | Defaults |
|-------|--------|----------|
| IP-based (`IpRateLimiter`) | Auth endpoints | 20 burst / 2/sec |
| DID-based (`RateLimiter`) | Authenticated | 100 burst / 10/sec |
| Category-based | By endpoint type | Read 200/20, Write 60/6, Governance 30/3, Compute 10/1 |
| Trust-gated | By trust score | PolicyOracle-driven, fallback: Isolated limits |
| Velocity | Transactions/hour | Isolated 10, Known 50, Partner 100, Federated 200 |

**QR Login** — `icn-gateway/src/api/sessions.rs`

| Property | Status | Detail |
|----------|--------|--------|
| IP rate limiting | ✅ | On session creation and polling |
| One-time token delivery | ✅ | Session consumed after first token retrieval |
| Session expiry | ✅ | TTL-based, cleaned up periodically |
| Gateway URL detection | ⚠️ | Trusts `X-Forwarded-*` headers — vulnerable to spoofing if not behind trusted proxy |

### 1.6 Gossip Security

**Protocol** — `icn-gossip/src/`

| Property | Status | Detail |
|----------|--------|--------|
| Bloom filter dedup | ✅ | SHA256 hash, capacity 10000, rotation at 80% |
| Vector clocks | ✅ | Causal ordering prevents duplicate processing |
| Bounded decompression | ✅ | `MAX_DECOMPRESSED_SIZE = 10MB`, bounded reader (`types.rs:15-232`) |
| Per-topic subscriber limits | ✅ | `MAX_SUBSCRIBERS_PER_TOPIC` prevents unbounded growth |
| Per-peer subscription limits | ✅ | Trust-weighted via `ResourceLimits.max_subscriptions` |
| Topic access control | ✅ | `AccessControl` enum: `Open`, `AllowList`, `MinTrustScore` |
| BlobNonceGuard | ✅ | Separate nonce tracking for blob transfers (`handlers/blob_nonce_guard.rs`) |

**Weakness**: Push handler (`handlers/push.rs`) processes announces without per-sender validation — relies entirely on upstream SignedEnvelope verification in NetworkActor. If a message reaches gossip without signature verification (bug), announces could be forged.

### 1.7 Sybil Resistance

**VUI Registry** — `icn-steward/src/vui_registry.rs`

| Property | Status | Detail |
|----------|--------|--------|
| Bloom filter uniqueness | ✅ | Fast probabilistic membership check |
| Exact hash verification | ✅ | `HashMap<[u8; 32], VuiRegistration>` for stewards |
| Witnessing steward | ✅ | Registration records include `witnessing_steward: Did` |
| Enrollment ceremonies | ✅ Scaffolding | `StewardActor` manages enrollment/recovery ceremonies (`actor.rs:4-6`) |

**Gap**: VUI is implemented at the data structure level but the **enrollment ceremony protocol** (in-person verification, M-of-N steward attestation, bond mechanics) is not yet wired into the runtime for production use. Personhood levels are defined conceptually but not enforced in rate limiting or governance.

### 1.8 Post-Quantum Readiness

**icn-crypto-pq** — `icn-crypto-pq/src/`

| Algorithm | Purpose | Status |
|-----------|---------|--------|
| ML-DSA-65 (FIPS 204) | Signatures | ✅ Implemented — 1952B pubkey, 3309B signature, NIST Level 3 |
| ML-KEM-768 (FIPS 203) | Key Encapsulation | ✅ Implemented — 1184B pubkey, 1088B ciphertext |
| Hybrid signatures | Ed25519 + ML-DSA | ✅ Both must verify (`hybrid.rs:74-83`) |
| Hybrid encryption | X25519 + ML-KEM | ⏳ Defined but NOT integrated with `EncryptedEnvelope` |
| PQ binding proof | DID-PQ-BINDING-V1 | ✅ Prevents key substitution, 5-min replay window (`handlers/hello.rs:60-76`) |
| Capability negotiation | HYBRID_SIGNATURES + HYBRID_KEM | ✅ Feature flags in Hello exchange |
| Keystore PQ keys | v5 format | ✅ Feature-gated in `StoredKeyV4` |

**Gap**: Post-quantum encryption (ML-KEM) is not wired into the network layer. Current E2E encryption remains X25519-only even with PQ signatures enabled.

### 1.9 Byzantine Fault Detection

**icn-security** — `icn-security/src/misbehavior.rs`

| Property | Status | Detail |
|----------|--------|--------|
| Violation types | ✅ | 7 types: InvalidSignature, ConflictingLedgerEntries, FailedComputeVerification, ExcessiveResourceUse, TrustGraphSpam, ConflictingSignedStatements, ReplayAttack, FailedStorageChallenge |
| Reputation scoring | ✅ | `ReputationScore` computed from violation history |
| Quarantine/ban | ✅ | Automatic quarantine → ban escalation |
| Evidence limits | ✅ | `MAX_EVIDENCE_SIZE = 64KB`, `MAX_VIOLATIONS_PER_PEER = 100` (bounded memory) |
| Trust penalty callback | ✅ | `TrustPenaltyCallback` propagates to trust graph |
| Gossip integration | ✅ | Violations recorded from gossip subscription/push handlers |
| Monitoring alerts | ✅ | Prometheus alerts for quarantine and auto-ban events |

## 2. Threat Model Assessment

| Threat | Current Mitigation | Residual Risk | Priority |
|--------|-------------------|---------------|----------|
| **T1: State Capture** | No single admin key; governance-based proposals; M-of-N recovery; cooperative democratic structure | Governance primitives incomplete (proposal execution not fully wired); no threshold signing for critical operations | **Medium** — Phase 5 |
| **T2: Capital Capture** | Trust scores gate access (not purchasable); cooperative membership required; no token speculation | Trust graph manipulation via sustained attestation flooding (rate-limited but not proof-of-stake bonded) | **Medium** — Phase 5 |
| **T3: Sybil** | VUI registry (Bloom + exact), steward witnessing, enrollment ceremonies scaffolded | Enrollment ceremonies not production-wired; no in-person verification enforcement; no cost to create DIDs | **High** — Critical for pilot |
| **T4: Coercion** | Age-encrypted keystore (passphrase), social recovery (M-of-N trustees + delay), key rotation chain | No duress key / canary mechanism; no threshold signing to distribute key power; HSM/TPM not functional | **Medium** — Phase 5 |
| **T5: Censorship** | mDNS discovery (LAN), STUN/TURN scaffolded, P2P architecture (no central server) | DNS dependency for STUN servers (Google defaults); no bootstrap node diversity; no relay infrastructure deployed; single K3s cluster | **High** — Blocks non-LAN use |
| **T6: Dependency** | `cargo-deny` (license + source), `cargo-audit` (weekly CI), pinned versions, crates.io-only sources | 4 unmaintained transitive deps (sled ecosystem); no reproducible builds; no SBOM generation | **Medium** — Ongoing |
| **T7: Insider** | Signed envelopes, Byzantine fault detection, quarantine/ban, double-entry ledger consistency checks | No secret rotation for JWT key; single operator can restart nodes; snapshot files unencrypted at rest | **Medium** — Phase 0/5 |

## 3. Operational Readiness

### 3.1 Deployment Model

| Component | Status | Detail |
|-----------|--------|--------|
| K3s single-node cluster | ✅ Running | Deployed since 2025-12-03, non-root pod, read-only rootfs, dropped capabilities |
| Docker multi-stage build | ✅ | `Dockerfile.icnd` — slim runtime image (debian:bookworm-slim), minimal packages |
| Devnet (3-node) | ✅ | Docker Compose with `docker-compose.yml`, automated init via `entrypoint.sh` |
| Network policies | ✅ | Default-deny ingress, namespace-scoped allow, monitoring-only metrics access |
| Health probes | ✅ | Liveness + readiness on `/v1/health` |
| Config validation | ✅ | Startup validation with warnings/errors before accepting connections |

**Concerns**:
- Gateway binds to `0.0.0.0:8080` in K8s/devnet (intentional but requires proxy/firewall)
- Devnet uses hardcoded passphrase `devnet-insecure` and JWT secret (acceptable for dev, dangerous if pattern leaks to prod)
- Metrics endpoint binds to `0.0.0.0:9100` (mitigated by NetworkPolicy)

### 3.2 Monitoring & Observability

| Component | Status | Detail |
|-----------|--------|--------|
| Prometheus metrics | ✅ | All subsystems instrumented: trust, gossip, network, gateway, misbehavior, compute |
| Security events | ✅ | Signature failures, rate limiting, Byzantine violations tracked as counters |
| Tracing (OpenTelemetry) | ✅ | Configurable sampling; security spans always sampled; OTLP export |
| Alerting rules | ✅ | ServiceMonitor with alerts for: Byzantine quarantine/ban, signature failures, network partition, ledger inconsistency |
| Audit logging | ✅ | Auth attempts (success/failure), scope validation failures, QR session lifecycle |
| Cardinality protection | ✅ | Path normalization prevents label explosion; bounded dimensions |

### 3.3 Key Management Operations

| Operation | Status | Detail |
|-----------|--------|--------|
| Key generation | ✅ | `icnctl id init` — Ed25519 + optional PQ via `--features post-quantum` |
| Key backup | ✅ | `icnctl backup` — TAR archive of encrypted keystore + metadata |
| Key restore | ✅ | `icnctl restore` — restores from TAR, safety backup of existing data |
| Key rotation | ✅ | Rotation chain with old→new signing authorization |
| Key export | ⚠️ | Treasury keys CLI-only with explicit flag (not via gateway) |
| Social recovery | ✅ | M-of-N attestation + delay period + gossip coordination |
| PQ upgrade | ✅ | `icnctl id upgrade-pq` adds ML-DSA keys without changing DID |

**Gap**: No automated key rotation schedule; no alerts for aging keys; no operational runbook.

### 3.4 NAT Traversal

**Status**: **Implemented but not production-tested**

| Component | File | Status |
|-----------|------|--------|
| STUN client | `icn-net/src/stun.rs` | ✅ Implemented — DNS resolution, timeout/retry, public IP discovery |
| NAT type detection | `icn-net/src/nat.rs` | ✅ Implemented — Full cone, restricted, port-restricted, symmetric detection |
| TURN relay client | `icn-net/src/turn.rs` | ✅ Implemented — RFC 5766, allocation lifecycle, permission management |
| Unified NAT config | `icn-net/src/nat.rs:36-52` | ✅ — `NatConfig` with STUN/TURN server lists, timeouts |
| Default STUN servers | `nat.rs:57-59` | ⚠️ Google STUN servers (`stun.l.google.com:19302`) — DNS dependency |
| Integration with session establishment | — | ⏳ Not wired into QUIC connection setup |

**Critical gap**: NAT traversal modules exist but are **not integrated into the actual QUIC session establishment flow**. Nodes behind NAT cannot currently connect to each other without manual port forwarding. This blocks any deployment beyond LAN or single-server scenarios.

### 3.5 Naming / Discovery

| Component | Status | Detail |
|-----------|--------|--------|
| mDNS discovery | ✅ LAN only | `icn-net/src/discovery.rs` — automatic peer discovery on local network |
| DNS-based discovery | ❌ | No DNS-SD or DNS bootstrap implementation |
| Bootstrap nodes | ❌ | No configurable bootstrap node list for WAN peer discovery |
| Naming service | ⏳ | `NamingService` / `ScopedDiscovery` traits exist but not implemented |

**Critical gap**: Discovery is **LAN-only via mDNS**. No mechanism for WAN peer discovery without manual IP configuration. Combined with NAT traversal gap, this means the system is effectively LAN-only.

### 3.6 Packaging & Distribution

| Property | Status | Detail |
|----------|--------|--------|
| Docker image | ✅ | `Dockerfile.icnd` — multi-stage, slim runtime |
| Binary build | ✅ | `cargo build --release` produces `icnd`, `icnctl`, `icn-console` |
| .deb/.rpm packages | ❌ | No system packages |
| Install script | ❌ | No one-liner install |
| Systemd unit | ❌ | No service file for node management |
| Auto-update | ❌ | No update mechanism |

**Gap**: Currently requires either Docker or manual Rust compilation. No OS-level packaging.

## 4. Gap Analysis

### Critical Gaps (Security Impact)

| # | Gap | Threat Mitigated | Missing | Where it Belongs | Phase |
|---|-----|-------------------|---------|------------------|-------|
| G1 | **NAT traversal not wired** | T5 Censorship | Integration of STUN/TURN into QUIC session establishment | `icn-net/src/session.rs` | **Phase 1-5** |
| G2 | **Sybil resistance incomplete** | T3 Sybil | Enrollment ceremony protocol not production-wired; no cost to create DIDs | `icn-steward/src/actor.rs` | **Phase 5** |
| G3 | **Snapshot encryption at rest** | T7 Insider | Snapshots contain TLS keys + encryption secrets in plaintext | `icn-snapshot/src/lib.rs:452` | **Phase 0** |
| G4 | **Rate limit before sig verification** | T7 Insider, T3 Sybil | Attacker can exhaust other peer's rate budget by forging `from` | `icn-net/src/` | **Phase 1** |
| G5 | **QR session gateway URL spoofing** | T5 Censorship, T7 Insider | `X-Forwarded-*` headers trusted without validation | `icn-gateway/src/api/sessions.rs:42-84` | **Phase 0** |
| G6 | **LAN-only discovery** | T5 Censorship | No WAN bootstrap or DNS-independent discovery | `icn-net/src/discovery.rs` | **Phase 5** |

### Important Gaps (Operational Impact)

| # | Gap | Threat | Missing | Phase |
|---|-----|--------|---------|-------|
| G7 | No JWT secret rotation | T7 Insider | Mechanism to rotate gateway JWT secret without downtime | Phase 1 |
| G8 | HSM/TPM not functional | T4 Coercion | PKCS#11/TPM backends are scaffolding only | Phase 5 |
| G9 | ML-KEM not integrated | T4 Coercion (quantum) | PQ encryption defined but not used in EncryptedEnvelope | Phase 5 |
| G10 | No reproducible builds | T6 Dependency | Build output not deterministic, no SBOM | Phase 5 |
| G11 | No system packaging | T5 Censorship | No .deb/.rpm, no systemd unit | Phase 5 |
| G12 | Unmaintained deps (sled) | T6 Dependency | 4 transitive RUSTSEC advisories from sled ecosystem | Phase 5 |

## 5. Phase 0 Tasks (2-3 tasks)

These are security contributions needed for the immediate demo:

### P0-S1: LAN Bind Address Safety
**Threat**: T5, T7
**What**: Ensure demo deployment gateway does NOT accidentally bind to WAN.
- Verify `bind_addr` in demo configs defaults to `127.0.0.1:8080` or LAN-only address
- Add startup warning when gateway binds to `0.0.0.0` (already exists: `icnd/src/main.rs:466-483`)
- Document firewall requirements for demo

### P0-S2: QR Session Hardening
**Threat**: T7 (session hijacking)
**What**: Prevent QR login session hijacking in demo environment.
- Validate `X-Forwarded-*` headers come from trusted proxy (or pin `GATEWAY_BASE_URL` env var)
- Consider HMAC-signing QR data to prevent tampering
- Ensure one-time token consumption is atomic (already implemented: `consume_session`)

### P0-S3: Operational Monitoring for Demo
**Threat**: T7
**What**: Basic security monitoring during demo.
- Verify ServiceMonitor alerts are active for: failed signatures, Byzantine quarantine, auth failures
- Ensure metrics endpoint is reachable by monitoring stack
- Test alert firing for a simulated violation

## 6. Phase 5 Tasks (5-8 tasks)

Full resilience for production-grade deployment:

### P5-S1: NAT Traversal End-to-End
**Threat**: T5 Censorship
**What**: Wire STUN/TURN into QUIC session establishment.
- Integrate `NatTraversal::discover_public_address()` into `SessionManager`
- Add TURN fallback path when direct QUIC fails
- Test behind symmetric NAT (most restrictive)
- Replace Google STUN defaults with self-hosted or diverse set
- Add NAT type to peer discovery announcements

### P5-S2: DNS-Independent Naming & Discovery
**Threat**: T5 Censorship
**What**: Bootstrap peers without DNS dependency.
- Implement configurable bootstrap node list (hardcoded fallback)
- DHT-based peer discovery or gossip-over-rendezvous
- `.onion` / Tor hidden service support for censorship resistance
- Remove Google dependency in default STUN config

### P5-S3: Anti-Censorship Relay Infrastructure
**Threat**: T5 Censorship
**What**: Deploy relay nodes for peers behind restrictive NAT/firewalls.
- Deploy TURN relay with ICN DID authentication
- Implement onion routing (scaffolding exists in `icn-privacy/src/onion_routing.rs`)
- Pluggable transport interface for domain fronting

### P5-S4: Key Recovery End-to-End Test
**Threat**: T4 Coercion
**What**: Verify social recovery works under realistic conditions.
- Multi-node test: key loss → attestation gathering → delay period → finalization
- Test cancellation during delay (fraud detection)
- Test with gossip network partitions
- Document recovery operational runbook

### P5-S5: System Packaging
**Threat**: T5 Censorship, T6 Dependency
**What**: Make node installation trivial.
- `.deb` package for Ubuntu/Debian (systemd service unit included)
- `.rpm` package for RHEL/Fedora
- `install.sh` one-liner (curl | sh)
- Auto-update mechanism (version announcements via gossip)

### P5-S6: Supply Chain Security
**Threat**: T6 Dependency
**What**: Protect against compromised dependencies.
- Reproducible builds (deterministic output from same source)
- SBOM generation (SPDX/CycloneDX format)
- Pin exact dependency hashes in `Cargo.lock` verification
- Migrate away from sled (resolve 4 RUSTSEC advisories)
- Binary signing for releases

### P5-S7: Threat Model Documentation
**Threat**: All
**What**: Formal threat model document for security review.
- STRIDE analysis of all subsystems
- Attack trees for T1-T7
- Security boundary diagram
- Penetration test scope document
- Incident response playbook

### P5-S8: Sybil Resistance Production Wiring
**Threat**: T3 Sybil
**What**: Make enrollment ceremonies enforceable.
- Wire steward ceremonies into identity creation flow
- Implement bond mechanics (economic cost to Sybil)
- Personhood levels gate governance participation
- Rate limit DID creation per ceremony window

## 7. Relevant Open Issues

Based on code TODOs and documented follow-ups:

1. **Rate limit ordering**: Move rate limit check after signature verification (noted in `docs/archive/2025/security-hardening-2025-12-18.md:161`)
2. **Bloom filter cleanup**: Periodic rotation task for long-running ReplayGuard sessions (noted in hardening doc:157)
3. **Trust graph TLS integration**: Connect trust scores to TLS certificate validation decisions (noted in hardening doc:145-149)
4. **Phase 2.3 TODO**: Trust score from PolicyOracle for topology decisions (`handlers/hello.rs:231` — currently hardcoded `trust_score = 0.5f32`)
5. **Snapshot encryption**: Caller must encrypt snapshots at rest (not enforced by code — `icn-snapshot/src/lib.rs`)
6. **PQ encryption integration**: ML-KEM not wired into EncryptedEnvelope (noted in `docs/design/post-quantum-crypto.md:234`)
