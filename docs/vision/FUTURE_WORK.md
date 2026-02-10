# ICN Future Work

**Document**: ICN-DOC-FUTURE-01
**Last Updated**: 2025-12-04
**Status**: Living Document

This document captures all planned and potential future work for ICN, organized by priority and timeline.

---

## Table of Contents

1. [Immediate (Pre-Pilot)](#1-immediate-pre-pilot)
2. [Short-Term (Pilot Phase)](#2-short-term-pilot-phase)
3. [Medium-Term (Post-Pilot)](#3-medium-term-post-pilot)
4. [Long-Term (Cooperative Cloud Vision)](#4-long-term-cooperative-cloud-vision)
5. [Technical Debt](#5-technical-debt)
6. [Research Topics](#6-research-topics)

---

## 1. Immediate (Pre-Pilot)

### 1.1 Gap Closure (from GAP_ANALYSIS.md)

**Status**: ✅ **22/23 gaps fixed (96%)** - All pilot blockers resolved!

**Completed Integration Tests** (2025-12-04):

| Gap | Description | Tests Added | Status |
|-----|-------------|-------------|--------|
| #3 | RPC integration tests | 32 tests | ✅ FIXED |
| #4 | Privacy crate integration tests | 27 tests | ✅ FIXED |
| #5 | Federation integration tests | 22 tests | ✅ FIXED |
| #7 | CCL contract integration tests | 24 tests | ✅ FIXED |
| #8 | Time synchronization tests | 32 tests | ✅ FIXED |
| #10 | Compute endpoint tests | 18 tests | ✅ FIXED |
| #12 | WASM executor blob storage | WasmRegistry + 17 tests | ✅ FIXED |

**Remaining TODOs (Gap #6)** - tracked via GitHub Issues:

| Issue | Description | Location |
|-------|-------------|----------|
| #36 | N-way fork resolution | `icn-ledger/src/ledger.rs:411` |
| #37 | TURN relay fallback | `icn-core/src/supervisor.rs:1396` |
| #38 | Cooperative treasury DID | `icn-core/src/supervisor.rs:1868` |
| #39 | Federated DID resolution | `icn-federation/src/resolver.rs:250` |
| #40 | Contract deployment gossip | `icn-ccl/src/actor.rs:125` |
| #41 | Governance operations | `icn-core/src/supervisor.rs:2033` |

These are post-pilot features - not blocking pilot deployment.

### 1.2 Entity Model Evolution (Vision Phase 1)

**Completed**:
- [x] Role migration: Owner→Steward, Admin→Facilitator, Member→Participant (commit `4156b09`)

**Remaining**:

```
[ ] Add Guest role (limited access, probationary)
    - New MemberRole::Guest variant
    - has_role() hierarchy update
    - API validation updates

[ ] Add Member scope (personal context)
    - Profile (name, bio, skills, preferences)
    - Device/agent bindings
    - Cross-scope role summary
    - Recovery configuration

[ ] Add Network scope (minimal protocol governance)
    - Protocol versioning decisions
    - Namespace registries (currencies, service types)
    - Minimum security requirements
    - Dispute resolution of last resort

[ ] Agent binding types (for multi-device)
    - AgentBinding struct with proof signature
    - DelegatedCapability enum
    - Principal-agent relationship storage
```

### 1.3 Documentation

| Item | Description | Location | Status |
|------|-------------|----------|--------|
| Threat model | Security analysis document | `docs/threat-model.md` | ✅ Complete |
| API versioning guide | How to evolve APIs | `docs/api-versioning.md` | ✅ Complete |
| Operator runbook | Day-2 operations guide | `docs/operations-guide.md` | ✅ Complete |

---

## 2. Short-Term (Pilot Phase)

### 2.1 Pilot Infrastructure

```
[ ] Pilot cooperative selection criteria
    - Size (10-50 members recommended)
    - Technical capacity
    - Use case alignment

[ ] Monitoring dashboard customization
    - Per-coop metrics views
    - Usage analytics
    - Cost attribution

[ ] Feedback collection system
    - In-app feedback widget
    - Usage telemetry (opt-in)
    - Monthly survey automation
```

### 2.2 Pilot-Driven Features

These will be prioritized based on pilot feedback:

```
[ ] Mobile-friendly gateway endpoints
    - Optimized payloads
    - Push notification support
    - Offline-first patterns

[ ] Bulk operations
    - Batch member import
    - Bulk transaction creation
    - Mass proposal notifications

[ ] Reporting and export
    - Transaction history CSV/JSON export
    - Governance activity reports
    - Member contribution summaries
```

### 2.3 ServiceOffering Schema (Vision Alignment)

Minimal schema for pilot:

```rust
/// A service offered by a cooperative (stub for pilot)
pub struct ServiceOffering {
    pub id: OfferingId,
    pub provider: CoopId,
    pub service_type: String,        // Free-form for now
    pub description: String,
    pub pricing_hint: Option<String>, // Human-readable
    pub contact: String,              // DID or email
    pub created_at: Timestamp,
}
```

Full schema deferred to post-pilot.

---

## 3. Medium-Term (Post-Pilot)

### 3.1 Low Severity Gaps

| Gap | Description | Phase |
|-----|-------------|-------|
| #16 | Federation actor in supervisor | 19+ |
| #17 | Privacy integration in supervisor | 19+ |
| #18 | Contract gossip completion | 16+ |
| #19 | N-way fork resolution | Future |
| #20 | Task migration completion | 16D |

### 3.2 Identity Evolution

```
[ ] DID Documents with multiple keys
    - Verification methods per-device
    - Capability-based permissions
    - Key rotation protocol

[ ] HD wallet / key hierarchy
    - Deterministic key derivation
    - Backup seed phrases
    - Hardware wallet support

[ ] Cross-device sync
    - Encrypted sync protocol
    - Conflict resolution
    - Device revocation
```

### 3.3 Economic Features

```
[ ] Multi-currency support
    - Per-coop currency definitions
    - Exchange rate mechanisms
    - Cross-currency settlements

[ ] Advanced credit policies
    - Time-decay credit limits
    - Seasonal adjustments
    - Member tier multipliers

[ ] Demurrage implementation
    - Configurable decay rates
    - Balance snapshots
    - Redistribution mechanisms
```

### 3.4 Governance Evolution

```
[ ] Weighted voting
    - Contribution-weighted votes
    - Stake-weighted for economic proposals
    - Configurable per-domain

[ ] Delegation
    - Vote delegation to representatives
    - Time-bounded delegations
    - Delegation chains (with limits)

[ ] Governance templates
    - Pre-configured domain setups
    - "Worker coop", "Consumer coop", "Platform coop" presets
    - Customization wizard
```

---

## 4. Long-Term (Cooperative Cloud Vision)

### 4.1 Service Coordination Layer

ICN as coordination layer for federated cooperative infrastructure:

```
[ ] Full ServiceOffering schema
    - ServiceType enum (Container, Function, Storage, etc.)
    - Capability declarations
    - Locality constraints
    - SLA definitions

[ ] Service registry
    - Publication and discovery
    - Trust-filtered matching
    - Cross-federation search

[ ] Service contracts
    - ServiceContract as governance proposal subtype
    - SLA monitoring
    - Dispute resolution integration

[ ] Settlement automation
    - Usage metering integration
    - Periodic settlement jobs
    - Multi-currency reconciliation
```

### 4.2 Provider Ecosystem

```
[ ] Provider interface trait
    - Standard provisioning API
    - Health and usage reporting
    - Graceful termination

[ ] Reference implementations
    - Docker provider
    - Kubernetes provider
    - Nomad provider

[ ] Provider certification
    - Capability attestation
    - Performance benchmarks
    - Reputation tracking
```

### 4.3 Trust-Gated Access Control

Replace traditional IAM with trust + capabilities:

```
[ ] AccessDecision framework
    - Trust score checks
    - Role-in-scope verification
    - Capability delegation chains

[ ] Resource tagging
    - min_trust requirements per resource
    - Scope-based access policies
    - Audit logging

[ ] Capability tokens
    - Short-lived capability grants
    - Revocation mechanism
    - Cross-federation delegation
```

### 4.4 Federation Features

```
[ ] Cross-federation services
    - Trust bridging via attestations
    - Clearing agreements
    - Capability delegation chains

[ ] Data locality
    - Jurisdiction constraints
    - Provider attestations
    - Compliance policies

[ ] Pricing discovery
    - Reference rates (federation-set)
    - Market-based pricing
    - Reputation-weighted value
```

---

## 5. Technical Debt

### 5.1 Code Quality

```
[x] Reduce clippy warnings
    - Current: 0 warnings ✅
    - Target: 0 warnings ✅

[ ] Increase test coverage
    - Current: 1134 tests (increased from 785)
    - Target: 90%+ line coverage on core crates

[ ] Documentation coverage
    - Add rustdoc to all public APIs
    - Example code in doc comments
```

### 5.2 Architecture Improvements

```
[ ] Actor system formalization
    - Common Actor trait
    - Standardized lifecycle management
    - Health check protocol

[ ] Error handling standardization
    - Unified error types across crates
    - Error code registry
    - User-facing error messages

[ ] Configuration validation
    - Schema-based config validation
    - Startup-time checks
    - Migration tooling
```

### 5.3 Performance

```
[ ] Benchmark suite
    - Gossip throughput tests
    - Ledger transaction rate
    - Compute task latency

[ ] Profiling and optimization
    - Memory usage analysis
    - Hot path optimization
    - Connection pooling

[ ] Caching strategy
    - Trust graph caching improvements
    - Query result caching
    - Invalidation protocol
```

---

## 6. Research Topics

### 6.1 Open Design Questions

**Provider Verification**
- How do we verify providers run what they claim?
- Options: attestation, challenge-response, redundant execution, reputation
- Recommendation: attestation + reputation for MVP, redundancy opt-in

**Pricing Discovery**
- How do markets form without central price-setting?
- Options: federation reference rates, provider competition, reputation weighting

**Data Locality**
- How to express and enforce jurisdiction constraints?
- Options: locality tags, provider attestations, federation policies

**Cross-Federation Services**
- How does Coop A member use services from Federation B?
- Options: trust bridging, clearing agreements, capability delegation

### 6.2 Future Protocol Research

```
[ ] Byzantine fault tolerance improvements
    - Current: Detection and quarantine
    - Future: Recovery and forgiveness protocols

[ ] Sybil resistance at scale
    - Current: Trust graph sufficient at cooperative scale
    - Future: Additional mechanisms for larger networks

[ ] Privacy-preserving computation
    - Zero-knowledge proofs for certain operations
    - Encrypted computation for sensitive data
    - Differential privacy for analytics
```

### 6.3 Economic Research

```
[ ] Mutual credit at scale
    - Clearing circle optimization
    - Default contagion analysis
    - Network topology effects

[ ] Demurrage effectiveness
    - Optimal decay rates
    - Redistribution mechanisms
    - Behavioral impacts

[ ] Cross-currency exchange
    - Decentralized rate discovery
    - Arbitrage prevention
    - Liquidity bootstrapping
```

---

## Tracking

This document should be updated:
- After each major feature completion
- During sprint planning
- After pilot feedback sessions

Related documents:
- [GAP_ANALYSIS.md](GAP_ANALYSIS.md) - Current gap status
- [ROADMAP.md](../ROADMAP.md) - Strategic roadmap
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture

---

## Changelog

| Date | Change |
|------|--------|
| 2025-12-04 | Initial document creation |
| 2025-12-04 | Marked role migration complete (Steward/Facilitator/Participant) |
| 2025-12-04 | Marked documentation section complete (threat-model, api-versioning, operations-guide) |
| 2025-12-04 | Updated Gap Closure section - 22/23 gaps fixed (96%), all pilot blockers resolved |
