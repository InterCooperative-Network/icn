# ICN Pilot Deployment Limitations

This document describes known limitations and constraints in the current ICN pilot deployment. These are intentional scope boundaries for the initial pilot phase, with plans to address them in future iterations.

## Overview

In the assessment snapshot captured by this document, the ICN pilot was considered deployable with broad infrastructure coverage. The following limitations are documented to set expectations and guide post-pilot priorities.

**Snapshot Date**: 2026-02-11.

---

## Security Limitations

### ZKP Circuits (Stubbed)

**Status**: Feature-gated behind `simulated` flag

**Files**: `icn/crates/icn-zkp/src/circuit/{age,citizenship,membership,non_revocation}.rs`

**Limitation**: All zero-knowledge proof circuits use simulation mode with NO cryptographic security. This means:
- Age proofs do not provide privacy guarantees
- Citizenship verification is not cryptographically sound
- Membership proofs are simulated

**Impact**: Do not rely on ZKP-based privacy for sensitive operations in the pilot.

**Tracking**: Issues #196-199

**Mitigation**: ZKP is disabled by default in pilot configuration. Attempting to generate proofs returns an error indicating the feature is not available.

---

## Scalability Constraints

### Single-Node SDIS Enrollment

**Status**: No distributed locking

**File**: `icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs:530-533`

**Limitation**: VUI (Verifiable Unique Identifier) registration does not use distributed locking, meaning:
- Concurrent registration from multiple nodes could cause conflicts
- Single-node deployment works correctly

**Impact**: Multi-node deployments should coordinate VUI registration through a single gateway node or implement application-level coordination.

**Future Work**: Add optimistic locking for multi-node deployment (sled transactions or distributed lock).

### Governance Pagination

**Status**: In-memory filtering

**File**: `icn/crates/icn-gateway/src/api/governance.rs:208`

**Limitation**: Governance domain queries load all matching domains into memory before filtering/pagination.

**Impact**: Performance may degrade with very large numbers of governance domains (>10,000).

**Future Work**: Implement cursor-based pagination at the storage layer.

---

## Infrastructure Considerations

### Gateway Module Size

**Status**: Architectural debt (non-blocking)

**Path**: `icn/crates/icn-gateway/src/` (75 files, 18+ dependencies)

**Limitation**: The gateway crate has grown to encompass many concerns, making it harder to:
- Understand request flow
- Test components in isolation
- Deploy subset functionality

**Impact**: Development velocity on gateway features may be slower than other crates.

**Future Work**: Document dependency graph, then extract managers into separate crates.

### Supervisor Initialization Complexity

**Status**: Documentation needed

**Path**: `icn/crates/icn-core/src/supervisor/` (20+ init_* modules)

**Limitation**: The supervisor initialization sequence involves many interconnected modules with implicit ordering dependencies.

**Impact**: Adding new actors or modifying initialization order requires careful analysis.

**Future Work**: Add initialization sequence documentation and consider dependency injection patterns.

---

## Test Coverage Notes

### Ignored Tests

**Count**: ~20 tests marked `#[ignore]`

**Distribution**:
- 7 tests: "Requires network interfaces" (CI environment limitations)
- 5 tests: "Uses hardcoded ports" (port conflict in parallel test runs)
- 3 tests: "Stress test" (too slow for CI)
- 5 tests: Various environment-specific reasons

**Impact**: These scenarios are not verified in automated CI.

**Future Work**:
- Use `port_picker` crate for dynamic port allocation
- Add network-aware test fixtures
- Move stress tests to separate profile (`cargo test --profile stress`)

---

## Protocol Limitations

### Community State Sync

**Status**: Implemented with last-write-wins

**Limitation**: Community state synchronization uses last-write-wins based on `updated_at` timestamp. This means:
- Concurrent member operations on different nodes may lose updates
- The node with the most recent timestamp wins

**Impact**: In high-contention scenarios, some member operations may need to be retried.

**Future Work**: Consider CRDT-based merge for member lists if contention becomes problematic.

### Trust Graph Computation Fallback

**Status**: Returns fallback score on errors

**File**: `icn/crates/icn-trust/src/multi_graph.rs`

**Limitation**: When trust computation encounters storage errors, it returns a small fallback score (0.05) rather than failing the request.

**Impact**:
- Temporary storage issues won't completely block peer interactions
- Peers may briefly have lower-than-expected trust during transient failures
- Errors are logged and metrics incremented for monitoring

**Mitigation**: Monitor `trust_computation_errors_total` metric for storage health.

---

## Memory Management

### Vector Clock Bounds

**Status**: Fixed in Phase 19

**Limitation**: Vector clocks are limited to 10,000 entries with LRU eviction at 8,000 entries.

**Impact**: In networks with more than 10,000 active nodes, older node entries will be evicted.

**Mitigation**: The 10,000 limit is suitable for pilot scale. For larger deployments, consider:
- Increasing `MAX_ENTRIES` constant
- Implementing hierarchical vector clocks
- Using epoch-based compaction

### Bloom Filter Rotation

**Status**: Fixed in Phase 19

**Limitation**: Replay protection Bloom filters rotate at 8,000 insertions to prevent false positive saturation.

**Impact**:
- Rotation clears recent message tracking
- `max_seq` threshold still provides replay protection
- Brief window where very old replays might not be detected (mitigated by sequence number checks)

---

## What's NOT Limited

As of this pilot-readiness snapshot, the following systems were assessed as ready for pilot usage:

1. **Identity & Cryptography**: Full Ed25519 DID implementation, Age-encrypted keystore
2. **Trust Graph**: Multi-graph computation (Social/Economic/Technical) with caching
3. **Mutual Credit Ledger**: Double-entry accounting with Merkle-DAG integrity
4. **Gossip Protocol**: Vector clocks, anti-entropy, compression
5. **Byzantine Fault Tolerance**: MisbehaviorDetector, reputation system, quarantine
6. **Network Security**: QUIC/TLS, DID-TLS binding, trust-gated rate limiting
7. **Monitoring**: Prometheus metrics, structured logging, distributed tracing
8. **Deployment**: K3s Helm charts, CI/CD pipeline, backup/recovery

---

## Feedback

Please report any issues encountered during the pilot:
- GitHub Issues: https://github.com/InterCooperative-Network/icn/issues
- Use the `pilot-feedback` label for pilot-specific observations
