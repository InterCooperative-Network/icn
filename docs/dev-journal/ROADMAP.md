# ICN Roadmap

**Last Updated**: 2026-01-20
**Current Phase**: 18 Complete, Phase 19 Next
**Target**: Production-ready release followed by pilot deployment

---

## Strategic Context

ICN is infrastructure for a **parallel political economy** — enabling cooperatives, communities, and federations to deliver better material outcomes than traditional capitalist and captured democratic systems.

**Core Vision**: ICN is not just infrastructure *for* cooperatives — it is **cooperative infrastructure that governs itself democratically**. The protocols are adjustable by the organizations using them. This is the "Cooperative Middle Layer" vision.

**See also**:
- [ECONOMIC_VISION.md](../ECONOMIC_VISION.md) — Strategic framing and value proposition
- [ECONOMIC_ARCHITECTURE.md](../ECONOMIC_ARCHITECTURE.md) — Technical design of the economic system
- [COOPERATIVE_MIDDLE_LAYER_GAP_ANALYSIS.md](COOPERATIVE_MIDDLE_LAYER_GAP_ANALYSIS.md) — Vision gap analysis
- [STRATEGIC_ALIGNMENT_AUDIT_2026-01-20.md](STRATEGIC_ALIGNMENT_AUDIT_2026-01-20.md) — Roadmap reconciliation audit

### The Dual-Track Approach

Development follows two parallel tracks that both serve the core mission:

| Track | Entry Point | Value |
|-------|-------------|-------|
| **Governance & Coordination** | "Your org will function better" | Individual org efficiency |
| **Internal Exchange** | "Keep value in the network" | Network-level capital retention |

Both tracks are built on the same substrate (identity, trust, ledger, governance, federation).

### Economic System Staging

The economic layer builds incrementally:

| Stage | Capability | Enables |
|-------|------------|---------|
| A | Mutual credit (hours, services) | Timebanks, service exchange |
| B | Simple asset tokens | Tool libraries, equipment sharing |
| C | Transformation tracking | Supply chains, manufacturing |
| D | Cross-community exchange | Regional cooperative economies |
| E | Fiat bridges | External interface when needed |

Each stage is independently useful. Build based on user need, not speculation.

---

## Overview

ICN development follows sequential phases. Each phase must be completed before the next begins. Some phases can run in parallel where noted.

**Implementation Status**: ~75% complete (272K LOC, 2,287 tests, deployed on K3s)

**Note**: This roadmap was reconciled on 2026-01-20 to integrate the Cooperative Middle Layer vision with the infrastructure roadmap. See the audit document for details.

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

## Planned Phases (19-35)

### Phase 19: Entity & Coop Integration
**Status**: ⏳ Planned
**Blockers**: None
**Goal**: Wire up completed entity and cooperative subsystems
**Duration**: 2 weeks

The CooperativeEntity model and Coop management code is **already implemented** (icn-entity: 4.6K LOC, icn-coop: 4.1K LOC). This phase completes integration into the runtime.

**Issues**:
- #266: CooperativeEntity integration (NEW - tracking issue)
- Spawn CoopActor in supervisor (plan exists in COOP_INTEGRATION_PLAN.md)
- Add entity gossip topic for multi-node sync
- Wire SledEntityRegistry for persistence
- Update gateway endpoints for full entity/coop CRUD

**Deliverables**:
- CoopActor spawned and persistent across restarts
- Entity gossip synchronization working
- Gateway has complete entity/coop API
- Multi-node entity convergence tested

**Related Epic**: #265 (ICN as Cooperative Middle Layer)

---

### Phase 20: Release & Testing Infrastructure
**Status**: ⏳ Planned
**Blockers**: None (can parallel with Phase 19)
**Goal**: CI/CD pipeline and comprehensive testing

Enable secure, validated releases with automated quality gates.

**Issues**:
- #183: Binary signing and SBOM generation
- #184: Pre-deployment health validation
- #186: Benchmark regression detection in CI
- #223: Horizontal Pod Autoscaling for icnd
- #224: Backup validation tests
- #226: Chaos engineering tests
- #227: Performance benchmark suite
- #228: Fuzz testing for CCL parser
- #319: Multi-node test harness
- #187: Complete test infrastructure helpers
- #159: Integration tests for Phase 16 features
- #329: Load testing and benchmarking suite

**Deliverables**:
- Signed release binaries with software bill of materials
- Health checks run before every deployment
- Performance regressions caught automatically
- Fault injection framework
- CCL parser fuzzing in CI
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

### Phase 22: Protocol Governance
**Status**: ⏳ Planned
**Blockers**: Phase 19 (entity foundation)
**Goal**: ICN governs itself democratically

Enable the network to modify its own parameters through democratic governance.

**Issues**:
- #267: Protocol self-governance
- NEW: ProtocolParameter type with modification scopes
- NEW: Parameter governance domain in icn-governance
- NEW: Change application mechanism with versioning
- NEW: Constraint propagation (higher levels constrain lower)

**Deliverables**:
- ProtocolParameter types defined with scopes
- Protocol Commons as root entity
- Parameters changeable via governance proposals
- Constraint inheritance across federation levels
- Audit trail for all parameter changes

**Related Epic**: #265 (ICN as Cooperative Middle Layer)

---

### Phase 23: Security Hardening
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

### Phase 24: Identity & SDIS Completion
**Status**: ⏳ Planned
**Blockers**: Phase 22 (protocol governance)
**Goal**: Complete SDIS integration with ZK voting

Finish the Sybil-resistant Decentralized Identity System.

**Issues**:
- #469: Key rotation events propagate via gossip
- #248: Multi-device identity synchronization
- #482: LRU cache with TTL for computed trust scores
- #205: Enhanced onion routing circuit selection
- #269: SDIS ZK voting and cooperative anchors
- NEW: ZK voting circuits for anonymous one-person-one-vote
- NEW: Cooperative anchor creation ceremonies
- NEW: Steward network governance (stewards as cooperative)
- NEW: Full L0-L3 credential ecosystem

**Deliverables**:
- Key rotations broadcast network-wide
- Seamless multi-device experience
- Fast trust score lookups
- Privacy-preserving routing
- ZK voting circuits operational
- Steward network is itself a cooperative

**Related Epic**: #265 (ICN as Cooperative Middle Layer)

---

### Phase 25: Inter-Cooperative Economics
**Status**: ⏳ Planned
**Blockers**: Phase 19 (entity integration), Phase 22 (protocol governance)
**Goal**: Complete economic functionality for coop-to-coop coordination

Implement Razeto's intercooperative economic patterns.

**Issues**:
- #268: Inter-cooperative economics (agreements, clearing, group purchasing)
- #386: Razeto's Four Intercooperative Bodies Integration
- #718-722: Labor assignment and credit routing types
- #474: Per-currency-pair oracle rate thresholds
- #318: Bilateral clearing house
- #317: Inter-cooperative agreement framework
- #208: Currency rebalancing policies
- #327: Demurrage scheduler
- #485: Cleared volume index compaction
- #337: Use-based resource access model
- NEW: Labor shares (earning through work)
- NEW: Cooperative bonds (inter-coop financing)
- NEW: Group purchasing coordination
- NEW: Anti-extraction policies

**Deliverables**:
- Labor shares system operational
- Cooperative bonds for inter-coop capital
- Bilateral credit clearing with netting
- Inter-coop agreements as first-class objects
- Group purchasing coordination
- Anti-extraction enforcement (ratio limits, ramp periods)
- Demurrage scheduler running

**Related Epic**: #265, #386

---

### Phase 26: SDK Completion
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

### Phase 27: Observability & Documentation
**Status**: ⏳ Planned
**Blockers**: Phase 20 (need CI infrastructure)
**Goal**: Production monitoring and complete documentation

Operators need visibility into running systems.

**Issues**:
- #188: Distributed tracing with OpenTelemetry
- #219: Operational dashboard for icn-core
- #325: Performance regression CI checks
- #494: Review metric cardinality
- #495: Configurable trace sampling
- #331: Enhanced Grafana dashboards
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
- Request tracing across services
- Real-time operational dashboard
- Reasonable metric cardinality
- Production-appropriate sampling
- Runbooks for common operations
- Defined SLOs with alerting
- User-facing setup guides

---

### Phase 28: Recursive Federation & Subsidiarity
**Status**: ⏳ Planned
**Blockers**: Phase 22 (protocol governance), Phase 25 (inter-coop economics)
**Goal**: Federations of federations with proper decision scoping

Enable arbitrary-depth federation with subsidiarity enforcement.

**Issues**:
- #270: Recursive federation hierarchy
- NEW: Multi-level hierarchy (Coop → Federation → Meta-Federation → Global Commons)
- NEW: Recursive trust calculation across levels
- NEW: Multi-level settlement with netting
- NEW: Proposal escalation mechanism
- NEW: DecisionScope enum (Personal → Local → Regional → Network → Global)
- NEW: Automatic scope detection for proposals
- NEW: Constraint propagation enforcement
- NEW: Override/appeal mechanism

**Deliverables**:
- Federations of federations operational
- Trust propagates through hierarchy
- Settlement at each level
- Governance bubbles up appropriately
- Decisions made at lowest appropriate level
- Clear scope boundaries enforced

**Related Epic**: #265 (ICN as Cooperative Middle Layer)

---

### Phase 29: CCL & Contracts
**Status**: ⏳ Planned
**Blockers**: Phase 25 (economics for contracts)
**Goal**: Contract ecosystem ready

Contracts people can actually use.

**Issues**:
- #475: State isolation between SetState updates
- #162: Configurable fuel costs
- #491: Proposal expiry cleanup
- #332: Contract template library
- #486: Explicit contract version fields
- #487: Pre-execution gas estimation

**Deliverables**:
- Verified state isolation
- Tunable execution costs
- Expired proposals cleaned up
- Ready-to-use contract templates
- Contract versioning
- Gas estimation API

---

### Phase 30: Code Quality
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

### Phase 31: Mobile SDK
**Status**: ⏳ Planned
**Blockers**: Phase 26 (TypeScript SDK first)
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

### Phase 32: Infrastructure Polish
**Status**: ⏳ Planned
**Blockers**: Phase 20
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

### Phase 33: CLI & UX Polish
**Status**: ⏳ Planned
**Blockers**: Phase 29 (features to expose)
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

**Critical**: Issue #5 (Select Pilot Community) should begin immediately in parallel with Phase 19.

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
- #328: Chaos engineering framework (partial in Phase 20)
- #330: Message batching and compression
- #412: Encryption integration TODO
- #415: Dev-mode CSP documentation

**Economic System** (see [ECONOMIC_ARCHITECTURE.md](../ECONOMIC_ARCHITECTURE.md) for staging):
- Asset token listings and exchange (Stage B)
- Transformation tracking and recipes (Stage C)
- Cross-community exchange protocols (Stage D)
- Fiat bridge interfaces (Stage E)

---

## Summary

| Milestone | Phases | Target |
|-----------|--------|--------|
| **Foundation Ready** | 19 | Entity/Coop integration |
| **Testing Ready** | 20 | CI/CD + test infrastructure |
| **Internet Ready** | 21 | NAT traversal |
| **Self-Governing** | 22 | Protocol governance |
| **Secure** | 23 | Security hardening |
| **Identity Complete** | 24 | SDIS + ZK voting |
| **Economics Complete** | 25 | Inter-coop economics |
| **Developer Ready** | 26-27 | SDK + observability + docs |
| **Federated** | 28 | Recursive federation |
| **Feature Complete** | 29-30 | Contracts + cleanup |
| **Production Ready** | 31-33 | Mobile + infra + UX |
| **Release** | 34 | RC validation |
| **Pilot** | 35 | Real-world deployment |

---

## Phase Dependencies

```
Phase 19 (Entity Integration)
    │
    ├──► Phase 22 (Protocol Governance)
    │       │
    │       ├──► Phase 24 (SDIS Completion)
    │       │
    │       └──► Phase 28 (Federation & Subsidiarity)
    │
    └──► Phase 25 (Inter-Coop Economics)
            │
            └──► Phase 29 (CCL & Contracts)

Phase 20 (Testing) ──► Phase 27 (Observability) ──► Phase 32 (Infrastructure)

Phase 21 (Network) ──► Phase 23 (Security) ──► Phase 26 (SDK)

Phase 26 (SDK) ──► Phase 31 (Mobile)

All Phases ──► Phase 34 (RC) ──► Phase 35 (Pilot)
```

---

## Change Log

- **2026-01-20**: Reconciled with Cooperative Middle Layer vision. Added Phases 22 (Protocol Governance), 24 (SDIS Completion), 25 (Inter-Coop Economics), 28 (Federation & Subsidiarity). Renumbered subsequent phases. See STRATEGIC_ALIGNMENT_AUDIT_2026-01-20.md.
- **2026-01-17**: Initial roadmap with Phases 19-35.
