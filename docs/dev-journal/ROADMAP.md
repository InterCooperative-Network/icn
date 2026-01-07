# ICN Roadmap

**Last Updated**: 2026-01-07
**Current Phase**: 18 Complete, Phase 19 Next
**Target**: Production-ready release followed by pilot deployment

---

## Overview

ICN development follows sequential phases. Each phase must be completed before the next begins. No parallel tracks.

**Implementation Status**: ~75% complete (272K LOC, 2,287 tests, deployed on K3s)

---

## Completed Phases (1-18)

See [PHASE_HISTORY.md](../PHASE_HISTORY.md) for details on completed phases.

| Phase | Name | Completed |
|-------|------|-----------|
| 1-10 | Foundation (Identity, Trust, Ledger, Network, Gossip) | 2025-Q3 |
| 11 | Multi-Device Identity & Sync | 2025-01-14 |
| 12 | Economic Safety Rails | 2025-01-14 |
| 13 | Governance Primitives | 2025-01-15 |
| 14 | Gateway API | 2025-01-17 |
| 15 | Distributed Compute | 2025-11-20 |
| 16 | Scheduler Evolution | 2025-11-24 |
| 17 | Storage Replication | 2025-11-25 |
| 18 | Pre-Pilot Hardening | 2025-11-27 |

---

## Planned Phases (19-34)

### Phase 19: Release Infrastructure
**Status**: ⏳ Planned
**Blockers**: None
**Goal**: CI/CD pipeline ready for production releases

Enable secure, validated releases with automated quality gates.

**Issues**:
- #183: Binary signing and SBOM generation
- #184: Pre-deployment health validation
- #186: Benchmark regression detection in CI
- #223: Horizontal Pod Autoscaling for icnd
- #224: Backup validation tests

**Deliverables**:
- Signed release binaries with software bill of materials
- Health checks run before every deployment
- Performance regressions caught automatically
- Auto-scaling under load
- Backup/restore validated in CI

---

### Phase 20: Testing Foundation
**Status**: ⏳ Planned
**Blockers**: None (can parallel with Phase 19)
**Goal**: Comprehensive testing infrastructure

Build confidence through systematic testing.

**Issues**:
- #226: Chaos engineering tests
- #227: Performance benchmark suite
- #228: Fuzz testing for CCL parser
- #319: Multi-node test harness
- #187: Complete test infrastructure helpers
- #159: Integration tests for Phase 16 features
- #329: Load testing and benchmarking suite

**Deliverables**:
- Fault injection framework
- Reproducible performance benchmarks
- CCL parser fuzzing in CI
- Easy multi-node test setup
- Load testing capability

---

### Phase 21: Network Connectivity
**Status**: ⏳ Planned
**Blockers**: None
**Goal**: ICN works over the internet, not just LAN

Currently mDNS only works on local network. This phase enables global connectivity.

**Issues**:
- #471: NAT traversal with STUN/TURN
- #472: Dynamic Bloom filter sizing
- #154: Bloom filter reset for high-volume peers
- #153: Automatic replay guard cleanup
- #483: Connection pooling for high-throughput
- #484: Adaptive gossip fanout based on network size

**Deliverables**:
- STUN/TURN integration for hole-punching
- Nodes can connect across NATs
- Gossip scales to larger networks
- Connection reuse for efficiency

---

### Phase 22: Security Hardening
**Status**: ⏳ Planned
**Blockers**: Phase 21 (network must work first)
**Goal**: Production-grade security posture

Close security gaps before exposing to internet.

**Issues**:
- #470: Sybil resistance in trust computation
- #473: Strict defaults for gossip topic access control
- #496: Reputation score persistence across restarts
- #490: Rate limiting propagates from trust layer
- #426: Atomic trust-class-aware rate limiting

**Deliverables**:
- Sybil attack mitigation
- Secure-by-default gossip topics
- Reputation survives restarts
- Consistent rate limiting

---

### Phase 23: Identity & Trust Evolution
**Status**: ⏳ Planned
**Blockers**: Phase 22
**Goal**: Identity and trust that reflect reality

Make trust dynamic and identity robust.

**Issues**:
- #469: Key rotation events propagate via gossip
- #248: Multi-device identity synchronization
- #482: LRU cache with TTL for computed trust scores
- #205: Enhanced onion routing circuit selection

**Deliverables**:
- Key rotations broadcast network-wide
- Seamless multi-device experience
- Fast trust score lookups
- Privacy-preserving routing

---

### Phase 24: SDK Completion
**Status**: ⏳ Planned
**Blockers**: Phase 21 (need stable network API)
**Goal**: SDK ready for application developers

TypeScript SDK must be solid before apps can build on ICN.

**Issues**:
- #171: Fix type safety issues in TypeScript SDK
- #172: Economic feature methods (recurring, escrow, budgets)
- #169: Trust and governance analytics methods
- #170: Notification system methods
- #173: Raw API layer for unmapped endpoints

**Deliverables**:
- Type-safe SDK with no `any` types
- Full economic API coverage
- Analytics and insights API
- Notification subscriptions
- Escape hatch for advanced users

---

### Phase 25: Observability
**Status**: ⏳ Planned
**Blockers**: Phase 19 (need CI infrastructure)
**Goal**: Production monitoring and debugging

Operators need visibility into running systems.

**Issues**:
- #188: Distributed tracing with OpenTelemetry
- #219: Operational dashboard for icn-core
- #325: Performance regression CI checks
- #494: Review metric cardinality
- #495: Configurable trace sampling
- #331: Enhanced Grafana dashboards

**Deliverables**:
- Request tracing across services
- Real-time operational dashboard
- Performance trends over time
- Reasonable metric cardinality
- Production-appropriate sampling

---

### Phase 26: Documentation
**Status**: ⏳ Planned
**Blockers**: Phases 21-25 (document what exists)
**Goal**: Operators and developers can use ICN

Can't release without documentation.

**Issues**:
- #230: Production runbooks
- #220: Service Level Objectives (SLOs)
- #189: Secrets rotation runbook
- #161: Merkle-DAG invariants documentation
- #160: Proposal state transitions
- #229: Cooperative setup guide
- #231: API versioning strategy
- #526: Hybrid PQ protocol negotiation flow
- #497: Gateway API versioning strategy

**Deliverables**:
- Runbooks for common operations
- Defined SLOs with alerting
- Security procedures documented
- Technical internals explained
- User-facing setup guides

---

### Phase 27: Ledger & Economics
**Status**: ⏳ Planned
**Blockers**: Phase 22 (security first)
**Goal**: Complete economic functionality

Full mutual credit system with inter-coop settlement.

**Issues**:
- #474: Per-currency-pair oracle rate thresholds
- #318: Bilateral clearing house
- #317: Inter-cooperative agreement framework
- #208: Currency rebalancing policies
- #327: Demurrage scheduler
- #485: Cleared volume index compaction
- #337: Use-based resource access model

**Deliverables**:
- Oracle rate validation
- Bilateral credit clearing
- Inter-coop agreements
- Currency rebalancing
- Time-based value decay

---

### Phase 28: CCL & Governance
**Status**: ⏳ Planned
**Blockers**: Phase 27 (economics for contracts)
**Goal**: Contract ecosystem ready

Contracts people can actually use.

**Issues**:
- #475: State isolation between SetState updates
- #162: Configurable fuel costs
- #491: Proposal expiry cleanup
- #332: Contract template library
- #486: Explicit contract version fields
- #487: Pre-execution gas estimation
- #267: Protocol self-governance

**Deliverables**:
- Verified state isolation
- Tunable execution costs
- Expired proposals cleaned up
- Ready-to-use contract templates
- Contract versioning
- Gas estimation API

---

### Phase 29: Code Quality
**Status**: ⏳ Planned
**Blockers**: None (can parallel)
**Goal**: Clean, maintainable codebase

Technical debt cleanup before release.

**Issues**:
- #158: Split config.rs into domain modules
- #157: Split supervisor/mod.rs into smaller modules
- #155: Remove dead code in find_entries_to_pull
- #156: Remove dead_code fields in compute
- #500: Consider unifying RPC and Gateway API layers

**Deliverables**:
- Modular configuration
- Smaller, focused modules
- No dead code
- Cleaner API surface

---

### Phase 30: Mobile SDK
**Status**: ⏳ Planned
**Blockers**: Phase 24 (TypeScript SDK first)
**Goal**: Mobile applications possible

React Native SDK for iOS/Android apps.

**Issues**:
- #213: Offline-first ledger sync
- #214: Biometric authentication integration
- #215: Mobile-optimized compute task submission
- #175: Document and media handling utilities
- #174: Offline-first architecture documentation

**Deliverables**:
- Works offline, syncs when connected
- Fingerprint/Face ID support
- Mobile-friendly task submission
- Media handling
- Offline patterns documented

---

### Phase 31: Infrastructure Polish
**Status**: ⏳ Planned
**Blockers**: Phase 19
**Goal**: Production Kubernetes deployment

Enterprise-ready deployment.

**Issues**:
- #190: ServiceMonitor and NetworkPolicy refinements
- #225: Multi-region deployment support
- #191: GitOps with Flux/ArgoCD
- #193: Canary/blue-green deployment strategy
- #192: Chaos engineering with fault injection

**Deliverables**:
- Prometheus service discovery
- Network segmentation
- Multi-region capability
- GitOps workflow
- Safe rollouts

---

### Phase 32: Federation
**Status**: ⏳ Planned
**Blockers**: Phases 21, 27 (network + economics)
**Goal**: Cooperatives can interconnect

Multiple ICN networks can coordinate.

**Issues**:
- #270: Recursive federation hierarchy
- #268: Inter-cooperative economics (agreements, clearing, group purchasing)
- #386: Razeto's Four Intercooperative Bodies Integration

**Deliverables**:
- Federations of federations
- Cross-network settlement
- Cooperative economic patterns
- Theoretical foundation implemented

---

### Phase 33: CLI & UX Polish
**Status**: ⏳ Planned
**Blockers**: Phase 28 (features to expose)
**Goal**: Pleasant user experience

Command-line and operational UX.

**Issues**:
- #176: REPL/interactive mode for icnctl
- #177: Data export and reporting commands
- #178: Event inspection commands
- #179: Metrics and network inspection commands
- #314: Entity dissolution workflow

**Deliverables**:
- Interactive CLI mode
- Export/reporting tools
- Event debugging
- Network diagnostics
- Proper entity lifecycle

---

### Phase 34: Release Candidate
**Status**: ⏳ Planned
**Blockers**: Phases 19-33
**Goal**: Releasable software

Final integration, polish, and validation.

**Deliverables**:
- All tests passing
- Documentation complete
- Performance validated
- Security audited
- Release notes written

---

### Phase 35: Pilot Deployment
**Status**: ⏳ Planned
**Blockers**: Phase 34
**Goal**: Real cooperative using ICN

Deploy with actual cooperative, gather feedback.

**Deliverables**:
- Pilot community selected
- Onboarding completed
- 3-month operation
- Feedback collected
- Iteration plan created

---

## Issue Backlog (Not Yet Scheduled)

These issues exist but aren't assigned to phases yet:

**Low Priority / Future**:
- #506: Full algebraic constraints for STARK circuits
- #481: HSM/TPM backend for keystore
- #269: SDIS ZK voting and cooperative anchors
- #265: ICN as Cooperative Middle Layer (epic)
- #328: Chaos engineering framework
- #330: Message batching and compression
- #75-79: Phase 21.x economic features (demurrage, exchange, marketplace, labor credits)
- #412: Encryption integration TODO
- #415: Dev-mode CSP documentation

---

## Summary

| Milestone | Phases | Target |
|-----------|--------|--------|
| **Testing Ready** | 19-20 | CI/CD + test infrastructure |
| **Internet Ready** | 21-22 | NAT traversal + security |
| **Developer Ready** | 23-26 | Identity + SDK + observability + docs |
| **Feature Complete** | 27-29 | Economics + contracts + cleanup |
| **Production Ready** | 30-33 | Mobile + infra + federation + UX |
| **Release** | 34 | RC validation |
| **Pilot** | 35 | Real-world deployment |
