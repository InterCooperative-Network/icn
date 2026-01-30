# ICN Phase History

This document contains detailed history of all completed development phases. For current project status, see [CLAUDE.md](../CLAUDE.md).

## Current Status

**Current Focus**: Kernel/App Separation Architecture
**Tracking Issue**: [#856](https://github.com/InterCooperative-Network/icn/issues/856)

**Architecture Reset** (2026-01-26): Previous roadmap superseded by kernel/app separation initiative. See [KERNEL_APP_SEPARATION.md](KERNEL_APP_SEPARATION.md) for details.

---

## Kernel/App Separation - Phase 2 Progress & Epics (Jan 26–30)

### Phase 2: Trust Extraction (~80% Complete)

**Tracking Issue**: [#857](https://github.com/InterCooperative-Network/icn/issues/857) (closed)
**Remaining sub-issues**: #910, #867, #869, #877

Merged PRs advancing Phase 2:
- [#872](https://github.com/InterCooperative-Network/icn/pull/872), [#883](https://github.com/InterCooperative-Network/icn/pull/883), [#896](https://github.com/InterCooperative-Network/icn/pull/896), [#897](https://github.com/InterCooperative-Network/icn/pull/897)
- [#904](https://github.com/InterCooperative-Network/icn/pull/904), [#906](https://github.com/InterCooperative-Network/icn/pull/906), [#907](https://github.com/InterCooperative-Network/icn/pull/907)
- [#968](https://github.com/InterCooperative-Network/icn/pull/968) — GovernanceService + LedgerService daemon wiring

### Cells & Scopes Epic (#919) — ✅ Complete

ScopeLevel, CellId, CellService, scope-aware placement and replication:
- [#950](https://github.com/InterCooperative-Network/icn/pull/950) — CellId + ScopeLevel types
- [#962](https://github.com/InterCooperative-Network/icn/pull/962) — CellService implementation
- [#961](https://github.com/InterCooperative-Network/icn/pull/961) — Scope-aware placement + replication

### ExecutionReceipt & Settlement — ✅ Complete

Chained Ed25519 execution receipts with clearing:
- [#956](https://github.com/InterCooperative-Network/icn/pull/956) — ExecutionReceipt with chained signatures
- [#960](https://github.com/InterCooperative-Network/icn/pull/960) — ReceiptClearingManager

### Service Discovery — 🚧 ~40%

Endpoint registry landed; health checking and routing remain:
- [#952](https://github.com/InterCooperative-Network/icn/pull/952) — Endpoint registry
- Open: #934-#937, #953, #954

### Commons Resource Pool — 🚧 ~30%

CommonsPool with metrics; security and governance remain:
- [#963](https://github.com/InterCooperative-Network/icn/pull/963) — CommonsPool + metrics
- Open: #947-#949, #964-#967

### Daemon Service Wiring (#908, #909) — ✅ Complete

- [#968](https://github.com/InterCooperative-Network/icn/pull/968) — GovernanceService + LedgerService wired into icnd

---

## Kernel/App Separation - Phase 0-1.5 (Complete) - 2026-01-26

**PR**: [#855](https://github.com/InterCooperative-Network/icn/pull/855)

Major architectural initiative to separate kernel infrastructure from domain-specific apps.

### Phase 0: PolicyOracle Infrastructure

Added core authorization infrastructure to `icn-kernel-api`:

- **OracleRegistry**: Atomic oracle replacement via ArcSwap, per-domain routing, TTL-based caching
- **BootstrapPhase**: Genesis → CoreApps → Running state machine with security guarantees
- **DecisionCache**: High-performance caching with automatic invalidation on oracle swap
- **GenesisCapabilities**: Time-limited bootstrap capabilities that expire after startup

Key files:
- `icn/crates/icn-kernel-api/src/authz.rs` - PolicyOracle trait, PolicyRequest, PolicyDecision
- `icn/crates/icn-kernel-api/src/bootstrap.rs` - OracleRegistry, BootstrapPhase, GenesisCapabilities

### Phase 1: App Runtime

Added app lifecycle management to `icn-core/src/apps/`:

- **AppRuntime**: Lifecycle management (prepare → install → start → stop → uninstall)
- **ComputeDispatcher**: Event/request routing with Reducer (pure) / Service (async) split
- **Manifest**: YAML parsing for app configuration
- **StateFactory**: Per-app isolated state namespaces

Key files:
- `icn/crates/icn-core/src/apps/runtime.rs` - AppRuntime, AppHandle, AppStatus
- `icn/crates/icn-core/src/apps/dispatcher.rs` - ComputeDispatcher, Reducer, Service
- `icn/crates/icn-core/src/apps/manifest.rs` - Manifest, OracleConfig

### Phase 1.5: CCL Schema Layer

Added declarative YAML schema support to `icn-ccl/src/schema/`:

- **Entity Schema**: Cooperatives, communities, federations, membership classes, rights
- **Governance Schema**: Bodies, decisions, voting thresholds, delegation
- **Economics Schema**: Capital structure, surplus allocation, credit policies
- **Agreement Schema**: Federation agreements, boundary protocols, dispute resolution
- **Expression Evaluator**: Deterministic mini-language for computed values

Key insight: Federation agreements use **binary boundary outcomes** — internal process is sovereign, boundary outcomes are interoperable.

Key files:
- `icn/crates/icn-ccl/src/schema/entity.rs` - EntitySchema, MembershipClass, Criteria
- `icn/crates/icn-ccl/src/schema/governance.rs` - GovernanceSchema, DecisionType
- `icn/crates/icn-ccl/src/schema/economics.rs` - EconomicsSchema, SurplusConfig
- `icn/crates/icn-ccl/src/schema/agreement.rs` - AgreementSchema, BoundaryProtocol
- `icn/crates/icn-ccl/src/schema/expr.rs` - SchemaExpr, EvalContext

### Test App: Echo

Created reference implementation in `apps/echo/`:

- Demonstrates full app lifecycle
- Uses Reducer for state mutations
- Uses Service for queries
- 2 passing tests

### Statistics

- **Lines added**: ~5,000
- **New tests**: 40+
- **Files created**: 15+

---

## Phase 19 - Comprehensive Code Review & Remediation (Complete) - 2025-12-31

Comprehensive codebase analysis and remediation addressing critical, high, and process issues identified during architecture review.

### Critical Fixes

- **Vector Clock Memory Bounds**: Added LRU eviction to prevent unbounded HashMap growth in high-churn networks
  - `MAX_ENTRIES = 10,000` limit with `CLEANUP_THRESHOLD = 8,000` for early eviction
  - Tracks `last_seen: Instant` per entry for accurate LRU ordering
  - New `prune_stale(duration)` method for periodic cleanup
  - File: `icn-gossip/src/vector_clock.rs`

- **Bloom Filter Saturation**: Added rotation before false positive saturation
  - Tracks `insertion_count` per `SequenceWindow`
  - Rotates Bloom filter at 8,000 entries (80% of 10,000 capacity)
  - Updates `max_seq` threshold on rotation to maintain replay protection
  - File: `icn-net/src/replay_guard.rs`

### High Priority Fixes

- **Trust Computation Fallback**: Improved error handling in multi-graph trust computation
  - Returns small fallback score (0.05) instead of 0.0 on storage errors
  - Prevents artificially blocking legitimate peers during transient failures
  - Structured logging with error context for debugging
  - Metrics continue to increment for monitoring
  - File: `icn-trust/src/multi_graph.rs`

- **Community Gossip Sync**: Implemented full receive and merge for community state
  - Added `community_store` to `NotificationDeps` for gossip routing
  - Created `handle_community_update()` handler with last-write-wins merge
  - Uses `updated_at` timestamp comparison for conflict resolution
  - Removed TODO markers from `init_community.rs` and `actor.rs`
  - Files: `icn-core/src/supervisor/init_notifications.rs`, `init_community.rs`

- **ZKP Verification**: Confirmed ZKP is safely disabled for pilot
  - All circuits use `#[cfg(feature = "simulated")]` with no cryptographic security
  - Attempting proof generation returns error indicating feature unavailable
  - Tracking issues #196-199 for real implementation

### Process Improvements

- **Issue Label Normalization**: Added `priority:*` labels to 30 issues
  - 3 issues: `priority:high`
  - 10 issues: `priority:medium`
  - 17 issues: `priority:low`
  - All 112 open issues now comply with label policy

### Documentation

- Created `docs/pilot-limitations.md` documenting known constraints
- Updated TODO comments to reference implemented solutions

### Validation

- All 1134+ tests passing
- `cargo clippy -- -D warnings` clean
- No regressions in existing functionality

---

## Phase 18 - Pre-Pilot Hardening (Complete) - 2025-12-04

- MisbehaviorDetector with 7 violation types (InvalidSignature, ConflictingLedgerEntries, FailedComputeVerification, ExcessiveResourceUse, TrustGraphSpam, ConflictingSignedStatements, ReplayAttack)
- Reputation system (0.0-1.0 score, 0.05x severity penalty, 0.01/hour decay)
- Automatic quarantine (score < 0.5) and auto-ban (critical violations)
- Trust graph integration (automatic trust penalty on misbehavior)
- Prometheus metrics and Grafana dashboard
- All 1134 workspace tests passing

**Byzantine Detection Features**:
- Violation Severity: Critical (10) -> auto-ban, Major (5) -> warnings, Minor (1) -> tracked
- Rate Limiting: Max 10 violations/hour
- Attack Resistance: Sybil, fork, replay, signature forgery, Byzantine consensus, DoS

---

## Internal Testing Infrastructure (Complete) - 2025-12-04

- Docker Compose 4-node test network (3 honest + 1 Byzantine)
- Monitoring stack (Prometheus + Grafana)
- 25 alert rules across 8 categories
- 38 test scenarios documented
- Complete documentation suite

---

## Phase 16 - Scheduler Evolution (Complete) - 2025-11-23 to 2025-11-24

Five-phase incremental evolution:
- **16A**: Resource Profiles & Matching
- **16B**: Placement Scoring (trust 40%, capacity 30%, queue 20%, jitter 10%)
- **16C**: Locality Awareness
- **16D**: Actor State & Migration
- **16E**: Cooperative Policies

See [docs/scheduler-evolution-plan.md](scheduler-evolution-plan.md) for complete design.

---

## Phase 15 - Distributed Compute Layer (Complete) - 2025-11-21

- `icn-compute` crate with trust-gated task execution
- ComputeTask/ComputeResult types for task lifecycle
- LocalExecutor with CCL interpreter integration
- Payment settlement via ledger
- Ed25519 signature signing and verification
- Task cancellation with submitter authorization
- 41 compute tests + 92 gateway tests + 25 RPC tests passing

**CLI**: `icnctl compute submit/status/cancel`
**Gateway**: `POST /v1/compute/submit`, `GET /v1/compute/status/{hash}`

---

## Phase 14 - Gateway API (Complete) - 2025-01-15 (Hardened: 2025-11-16)

- REST API server with actix-web framework
- JWT-based authentication with challenge-response flow
- Cooperative namespace management, Ledger API, Governance API
- WebSocket real-time event streaming
- Per-DID rate limiting (token bucket)
- 77 tests pass

**Endpoints**: `/auth/*`, `/coops/*`, `/ledger/*`, `/gov/*`, `/compute/*`, `/ws/*`

---

## Phase 13 - Governance Primitives v1 (Complete) - 2025-01-15

- GovernanceDomain, Proposal, Vote, VoteTally types
- Gossip Protocol with 7 GovernanceMessage types
- GovernanceProfile with cooperative_default
- CLI Commands: `icnctl gov`
- 39 tests pass

---

## Phase 12 - Economic Safety Rails (Complete) - 2025-01-14

- Dynamic Credit Limits (trust + history-based)
- New Member Protection (progressive ramping)
- Dispute Resolution (file, mediate, resolve)
- Credit Policy Manager with presets
- 10 tests pass

---

## Track B1 - Operational Hardening (Complete) - 2025-01-14

- Backup & Restore (`icnctl backup/restore`)
- Monitoring Dashboard + health check endpoint
- Incident Response Playbook (7 procedures)
- Protocol Version Validation
- Graceful Restart (state snapshots)

---

## Track B3 - Economic Modeling (Complete) - 2025-01-14

- Agent-based simulation framework (Mesa 3.3.1)
- 5 behavioral agent types
- 5 scenarios testing economic parameters
- ~13,000 transactions per scenario

Key Findings:
- Dynamic Credit Limits: -33% defaults
- Demurrage: -22% inequality (Gini)
- System stable up to 20% free-riders

See [sims/mutual-credit/RESULTS_SUMMARY.md](../sims/mutual-credit/RESULTS_SUMMARY.md).

---

## Phase 11 - Multi-Device Identity & Sync (Complete) - 2025-01-14

- DID Document v2 with multi-device support
- VerificationMethod with capability-based permissions
- Keystore v3 format with automatic migration
- Identity sync protocol via gossip
- 33 tests pass

See [docs/multi-device-identity-design.md](multi-device-identity-design.md).

---

## Phase 10 - End-to-End Payload Encryption (Complete) - 2025-01-13

- EncryptedEnvelope with X25519-ChaCha20-Poly1305 AEAD
- X25519 keys added to IdentityBundle
- Keystore v2.1 format with auto-migration
- Full encrypt -> sign -> send -> receive -> verify -> decrypt flow
- 261 tests pass

---

## Phase 9 - Message & Identity Integrity (Complete) - 2025-01-13

- SignedEnvelope with Ed25519 signatures
- ReplayGuard with sequence tracking and Bloom filters
- NetworkActor automatic verification
- 16 new tests

---

## Phase 8 - DID-TLS Binding & Keystore Integration (Complete) - 2025-01-13

- IdentityBundle with persistent DID-TLS binding
- Keystore v2 format with automatic migration
- Runtime/Supervisor integration

---

## Phase 7 - Polish & Production (Complete) - 2025-01-11

- Prometheus metrics exporter
- Complete pull protocol (Request/Response)
- Topic subscriptions with notification callbacks
- Production hardening (8 fixes including critical security fix)
- 120+ tests

**Security Fixes**:
- Network timeouts, DID validation, bounded growth
- Compression, input sanitization
- Expression depth validation (critical)
- Ledger semantics fix

---

## Version Negotiation Features (Complete) - 2025-01-14

- VersionInfo Protocol with Hello handshake
- 8 CapabilityFlags
- Per-Connection Tracking
- Backward Compatibility for legacy nodes
- 16 tests

See [docs/capability-based-features.md](capability-based-features.md).

---

## Graceful Restart Features

- State Snapshot: JSON to `{data_dir}/state.snapshot`
- Gossip State: Vector clocks, topic subscriptions, ACL preservation
- Network State: Peer X25519 public keys
- <10ms startup/shutdown overhead
- Included in `icnctl backup/restore`

---

## Security & Production Hardening Summary

**Network-level**:
- Trust-gated rate limiting (token bucket per trust class)
- QUIC stream limits (10 concurrent, 1MB/stream)
- Message validation (10MB max)

**Protocol-level**:
- Certificate verification with DID extraction
- Bloom filter validation
- Timestamp overflow protection

**Runtime**:
- Async-safe operations
- Result types with context
- Graceful degradation

See [docs/production-hardening.md](production-hardening.md) for complete details.
