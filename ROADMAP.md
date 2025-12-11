# ICN Roadmap

**Document ID**: ICN-DEP-ROADMAP-01
**Version**: 0.3.0
**Maintainer**: ICN Foundation (Matthew Faherty, lead)
**Canonical Spec**: [ARCHITECTURE.md](docs/ARCHITECTURE.md) (ICN-DEP-01 - Foundation Protocol Specification)

---

**Status**: **PILOT-READY** ✅ - Phase 18 Complete (Pre-Pilot Hardening), Phase 17 ✅ (Storage Replication), Phase 16 ✅ (Scheduler Evolution), Phase 15 ✅ (Compute), Phase 14 ✅ (Gateway), **Federation Layer ✅**, Phases 11-12 ✅, Tracks B1-B3 ✅, Pilot Tooling ✅, **Track S (SDIS) S1-S6 ✅** (2025-12-10)
**Next**: Track C1 Pilot Community Selection → Track C2 Pilot MVP → 3-month Pilot Deployment

## Executive Summary

**Substrate Status**: **PILOT-READY** ✅ - All critical infrastructure complete (Phases 1-18).

**Completed Infrastructure**:
- Three-layer security (transport/message/application encryption)
- Multi-device identity with key rotation
- Economic safety rails (dynamic credit limits, disputes, quotas)
- Gateway API (REST + WebSocket events)
- Distributed compute with intelligent scheduling
- Operational tooling (backup/restore, monitoring, graceful restart)
- Economic validation (agent-based simulation)
- **Data replication with 99.9% durability** (Phase 17 ✅)
- **Byzantine fault detection and quarantine** (Phase 18 ✅)
- **Network partition healing with conflict resolution** (Phase 18 ✅)
- **Storage quotas with priority-based eviction** (Phase 18 ✅)
- **Federation layer with inter-coop coordination** (Federation ✅)
- **SDIS post-quantum identity** (Track S ✅ - 2025-12-10)

**Pre-Pilot Infrastructure: COMPLETE** ✅ (2025-11-27)

**Gap Analysis**:
- [ARCHITECTURE.md Section 12](docs/ARCHITECTURE.md#12-known-limitations--future-work): Complete technical gap analysis (10 critical gaps identified)
- [Strategic Gap Analysis](docs/strategic-gap-analysis.md): 15 structural gaps (substrate→system transition)
- [Implementation Gap Analysis](docs/gap-analysis.md): Documentation vs reality audit

**Critical Path**: Select pilot community (Track C1) → Build pilot MVP (Track C2, 4-6 weeks) → 3-month pilot deployment → Phase 19+ driven by learnings.

---

## Roadmap Structure

ICN's development follows four parallel tracks:

- **Track A: Substrate Evolution** - Core protocol and security features (sequential)
- **Track B: Operational & Legal Backbone** - Production readiness (parallel)
- **Track C: Pilot Community** - Real-world deployment and learning (convergent)
- **Track S: SDIS Identity** - Post-quantum identity with steward network (complete)

**Guiding Principle**: Track C (pilot deployment) drives priorities in Tracks A and B. We build what real communities need, not what the architecture diagram suggests.

## Gap Analysis Summary

**15 Structural Gaps** identified across 4 tiers (see [docs/strategic-gap-analysis.md](docs/strategic-gap-analysis.md)):

**Status Breakdown**:
- ✅ **Closed** (4): Multi-device identity, protective ledger (partial), economic simulation, **Federation** (2025-11-28)
- 🚧 **Partial** (5): Ledger mechanics, security posture, storage sync, observability, NAT traversal (Phases 1-3)
- 🔴 **Critical Path** (6): Client SDK, governance, templates, onboarding, observability UX, cooperation UX

**Key Insight**: The substrate is ready. The missing pieces are *social layer*, *usability*, and *real-world workflows* - all discovered through pilot deployment.

---

## Track A: Substrate Evolution

### Phase 11: Multi-Device Identity & Sync ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Blocker For**: All pilot deployments - NOW UNBLOCKED

**Motivation**: Current identity model (1 keypair = 1 person) fails when devices break, get stolen, or users need multiple access points. A real cooperative substrate must survive hardware failure without losing economic history.

**Scope**:
- DID Document v2 with multiple verification methods
- Device management (add/revoke devices)
- Key rotation with event chains
- Recovery mechanisms (social recovery or backup seeds)

**Technical Design**:
```
DID Document v2:
{
  "id": "did:icn:abc123",
  "verificationMethod": [
    { "id": "device-1", "publicKey": "...", "capabilities": ["sign", "rotate"] },
    { "id": "device-2", "publicKey": "...", "capabilities": ["sign"] }
  ],
  "authentication": ["device-1", "device-2"],
  "recovery": { "method": "social", "threshold": 3, "trustees": [...] }
}

Key Rotation Chain (stored in icn-store):
RotationEvent {
  old_key: PublicKey,
  new_key: PublicKey,
  proof: Signature,
  timestamp: u64,
  reason: RotationReason  // Scheduled, Compromise, DeviceChange
}
```

**Success Criteria**:
- User can add a second device without losing identity
- User can revoke a compromised device
- User can recover identity if all devices are lost (via social recovery)
- All ledger/trust/contract operations work across device rotations
- Tests: Multi-device signing, rotation chain validation, recovery flows

**Deliverables**:
- `icn-identity` crate updates for DID Document v2
- `icnctl device add`, `icnctl device revoke`, `icnctl recover` commands
- Migration path from v2.1 keystore to multi-device format
- Documentation: `docs/multi-device-identity.md`

**Spec Impact**: Updates [ARCHITECTURE.md Section 2.1](docs/ARCHITECTURE.md#21-identity-layer) (Identity Layer - Multi-Device Support)

---

### Phase 12: Economic Safety Rails ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Blocker For**: Pilot deployment in mutual credit scenarios - NOW UNBLOCKED

**Motivation**: Mutual credit systems fail predictably: free riders, credit limit gaming, defaults without recourse. Without guard rails, the first scammer destroys community trust in the entire system.

**Scope**:
- Dynamic credit limits based on trust + history
- New member protective throttling
- Dispute resolution primitives
- Default handling protocols

**Technical Design**:
```rust
// icn-ledger/src/credit_policy.rs
pub struct CreditPolicy {
    baseline: i64,           // Base credit for all members
    trust_multiplier: f64,   // Scale by trust score
    history_bonus: i64,      // Bonus for cleared obligations
}

impl CreditPolicy {
    pub fn calculate_limit(&self, member: &DID, ledger: &Ledger, trust: &TrustGraph) -> i64 {
        let trust_score = trust.compute_trust(member)?;
        let cleared_volume = ledger.total_cleared_by(member)?;

        self.baseline
            + (self.baseline as f64 * trust_score * self.trust_multiplier) as i64
            + (cleared_volume / 10)  // 10% of historical cleared volume
    }
}

// New member throttle
pub struct NewMemberPolicy {
    initial_limit: i64,      // Very low (e.g., 10 hours)
    ramp_period: Duration,   // Time to reach full limit
    contribution_required: u64, // Min contributions before ramp starts
}

// Dispute resolution
pub enum EntryStatus {
    Normal,
    Contested { filed_by: DID, reason: String, filed_at: u64 },
    Resolved { mediator: DID, outcome: Resolution },
}
```

**Success Criteria**:
- New members cannot immediately max out credit and disappear
- Credit limits adapt to demonstrated trustworthiness
- Community can flag disputed entries
- Default handling is explicit and visible (not silent failure)
- Economic simulation shows reduced vulnerability to common attacks

**Deliverables**:
- `icn-ledger` credit policy system
- CCL primitives: `dispute_entry()`, `resolve_dispute()`, `write_off_debt()`
- Economic simulation (see Track B3)
- Documentation: `docs/economic-safety.md`

**Spec Impact**: Updates [ARCHITECTURE.md Section 3](docs/ARCHITECTURE.md#3-ledger-layer) (Ledger Layer - Economic Safety)

---

### Phase 14: Gateway API ✅ COMPLETE
**Status**: Gateway Complete (2025-01-15), SDK & Pilot UI Complete (2025-01-17, see Track C Pilot Tooling)
**Blocker For**: Client applications - NOW UNBLOCKED

**Strategic Shift**: ICN as Cooperative Backend Platform

**Vision**: Co-ops build apps that use ICN under the hood. Members never see `icnd` or `icnctl`.

**What This Is**:
- Developer-facing API layer (REST + WebSocket)
- TypeScript SDK for easy integration
- Reference app (Shopper's Club) as starting point
- **NOT** an app runtime (that's Phase 16+ conditional)

**What This Enables**:
- Co-ops can build custom apps (or fork reference app)
- Early pilots: we host the reference app multi-tenant (Phase 15)
- Later: co-ops can self-host or customize

**Completed Components** ✅:
1. **icn-gateway** - REST + WebSocket API server (Actix-web)
   - 13 REST endpoints (health, auth, coops, ledger, governance, proposals)
   - JWT authentication with challenge-response flow
   - Bearer token middleware protecting sensitive endpoints
   - WebSocket real-time event streaming with post-connection auth
   - Cooperative namespace isolation
2. **Runtime Integration** - Gateway integrated into icnd
   - GatewayConfig in configuration system
   - CLI arguments: `--gateway-enable`, `--gateway-bind`, `--gateway-jwt-secret`
   - Environment variable support: `ICN_GATEWAY_JWT_SECRET`
   - Dedicated thread spawn for Actix-web compatibility
3. **Production Hardening** (2025-11-16) ✅
   - ✅ API versioning (`/v1/` namespacing) - all endpoints versioned
   - ✅ Per-DID rate limiting - token bucket (100 burst, 10/sec refill)
   - ✅ Scope-based authorization - all handlers enforce JWT scope requirements
   - ✅ Authenticated user DID extraction for cooperative ownership
   - 38 passing tests (5 rate limiting + 2 authorization + 1 ownership)

**Client Infrastructure** ✅ (Completed in Track C Pilot Tooling, 2025-01-17):
4. **TypeScript SDK** - `@icn/client` npm package (see [sdk/typescript/](sdk/typescript/))
5. **Pilot Web UI** - Simple browser interface (see [web/pilot-ui/](web/pilot-ui/))
6. **OpenAPI Specification** - Complete API documentation (see [docs/api/openapi.yaml](docs/api/openapi.yaml))

**Recently Completed** (2025-11-21):
- ✅ WebSocket reconnection and event backfill
  - SequencedEvent with global sequence numbers
  - Backfill buffer (100 events per channel)
  - Client can request missed events via `Backfill` message
  - AuthOk includes `current_seq` for reconnection tracking

**Success Criteria**: ✅ ALL COMPLETE
- ✅ Gateway API operational and integrated into icnd
- ✅ JWT authentication working (challenge → token → protected endpoints)
- ✅ WebSocket events streaming to connected clients
- ✅ WebSocket reconnection with event backfill
- ✅ Cooperative namespace isolation functional
- ✅ TypeScript SDK (completed in Track C Pilot Tooling)
- ✅ Pilot web UI (completed in Track C Pilot Tooling)

**Deliverables**: ✅ Gateway Complete + Client Infrastructure
- ✅ `icn-gateway` crate with 38 passing tests (updated 2025-11-16)
- ✅ Integration into icnd supervisor
- ✅ Configuration system and CLI support
- ✅ Documentation: dev journals, CHANGELOG.md, example configs
- ✅ API versioning, rate limiting, scope-based authorization, and authenticated ownership (Phase 14 production hardening)
- ✅ `@icn/client` TypeScript SDK (completed in Track C Pilot Tooling, 2025-01-17)
- ✅ Pilot web UI (completed in Track C Pilot Tooling, 2025-01-17)
- ✅ OpenAPI specification (completed in Track C Pilot Tooling, 2025-01-17)

**Spec Impact**: Updates [ARCHITECTURE.md Section 6](docs/ARCHITECTURE.md#6-gateway-api-layer) (Gateway API Layer)

**Next Steps**:
- Phase 17: Storage hardening (4 weeks)
- Phase 18: Pre-pilot hardening (6 weeks)
- Track C1: Select pilot community (parallel with Phase 17)
- Track C2: Build pilot MVP using existing SDK and UI

---

### Phase 15: Hosted Pilot Deployment (1-2 months)
**Status**: Not Started - Blocked on Phase 14
**Purpose**: Get real signal from actual co-ops

**Approach**:
- Deploy reference app multi-tenant (one instance, namespace-scoped)
- Run on ICN Foundation infrastructure (or similar)
- Onboard 1-2 pilot co-ops: "Here's your app, here's your login"
- **Watch what hurts** - weekly feedback sessions

**Learning Questions**:
- Where do users get stuck? (onboarding, UX, concepts)
- What do they want to customize?
- Do they care about self-hosting?
- Is the ledger model intuitive?
- What governance patterns emerge?

**Success Criteria**:
- 10+ active users logging transactions weekly
- 3+ governance decisions (or attempts at governance)
- Community feedback informs Phase 16+ priorities
- Clear signal on what to build next

**Deliverables**:
- Hosted multi-tenant deployment
- Pilot onboarding guide
- Weekly learnings documented in `docs/pilots/learnings/`

**Design Doc**: [docs/pilots/hosted-approach.md](docs/pilots/hosted-approach.md)

---

### Phase 16: Scheduler Evolution - Intelligent Task Placement (5-phase plan)
**Status**: Phase 16C Locality Awareness Complete ✅ (2025-11-24)
**Philosophy**: Incremental evolution from reactive task claiming to distributed, trust-governed scheduling

**Long-Term Vision**: Transform ICN compute into a multi-tier cooperative fabric (edge/community/regional) with intelligent placement, actor migration, and per-coop policies.

**Design Doc**: [docs/scheduler-evolution-plan.md](docs/scheduler-evolution-plan.md) (8,800+ words)

---

#### Phase 16A: Resource Profiles & Matching ✅ COMPLETE (2025-11-23)
**Duration**: 1 week
**Goal**: Replace vague "capabilities" with concrete resource requirements

**Completed**:
- ✅ `ResourceProfile` type (CPU/RAM/GPU/storage/network requirements)
- ✅ `NodeCapacity` tracking and reservation system
- ✅ `PlacementPolicy` trait for pluggable scoring algorithms
- ✅ `DefaultPlacementPolicy` with multi-factor scoring
- ✅ GPU support with compute capability matching
- ✅ 7 new tests, all passing (47 total in icn-compute)
- ✅ Backward compatible with Phase 15 reactive claiming

**Example**:
```rust
let profile = ResourceProfile::gpu(24, "sm_70".into());
// Requires: 24GB GPU, compute capability sm_70+

let capacity = NodeCapacity { /* ... */ };
if capacity.can_fit(&profile) {
    capacity.reserve(&profile)?;
    // Execute task
    capacity.release(&profile);
}
```

**Deliverables**:
- `icn-compute/src/scheduler.rs` (700+ lines)
- `examples/scheduler_demo.rs` (working placement demo)
- Full test coverage

---

#### Phase 16B: Placement Scoring ✅ COMPLETE (2025-11-23)
**Duration**: 3 sessions (~8 hours)
**Goal**: Replace "first to claim" with "best fit" scoring

**Completed**:
- ✅ `PlacementRequest`/`PlacementOffer` gossip messages
- ✅ Deliberation window (500ms) to prevent race conditions
- ✅ Multi-factor scoring: trust (0.4), capacity (0.3), queue (0.2), jitter (0.1)
- ✅ Prometheus metrics: placement requests/offers, score distribution, duration
- ✅ Automatic protocol selection (resource_profile → PlacementRequest)
- ✅ 48 passing tests, all backward compatible

**Flow**:
```
Submitter → PlacementRequest (broadcast)
    ↓
Executors → Compute score
    ↓
Deliberation (500ms)
    ↓
Highest score → TaskClaimed
```

**Deliverables**:
- Multi-executor placement test validates highest-score wins
- Trust gating prevents low-trust executors from participating
- Random jitter breaks ties and prevents thundering herd

---

#### Phase 16C: Locality Awareness ✅ COMPLETE (2025-11-24)
**Duration**: 4 weeks (compressed: ~7.5 hours)
**Goal**: Network topology and data locality as first-class scheduling inputs

**Completed**:
- ✅ **Week 1**: Network topology measurement (RTT/bandwidth with TTL)
- ✅ **Week 2**: BlobLocationRegistry for data tracking (360 lines, 8 tests)
- ✅ **Week 3**: Enhanced scoring with locality factors (7-factor rebalancing)
- ✅ **Week 4**: Integration test + comprehensive documentation

**Architecture**:
- Network topology: Ping/Pong protocol → RTT measurements with 5-min TTL
- Data registry: Gossip BlobAnnounce → Track blob locations with 24-hour TTL
- Scoring: Trust 25%, Capacity 20%, Queue 15%, **RTT 15%**, **Data 15%**, **Hints 10%**, Jitter 10%
- `LocalityHint` enum: PreferRegion, PreferDid, AvoidDid, ColocateWith

**Technical Achievements**:
- ✅ 50 passing tests (+2 from Phase 16B)
- ✅ ~1,060 lines of production code + tests
- ✅ Performance targets met (RTT <1%, registry 10K+ blobs, scoring <10ms)
- ✅ Validates "compute goes to data" principle

**Impact**:
- Tasks intelligently placed near their data
- Network-aware placement minimizes latency
- Data-aware placement reduces transfer costs
- Locality compensates for lower trust (demonstrated in integration test)

---

#### Phase 16D: Actor State & Migration ✅ COMPLETE
**Status**: Complete (2025-11-24)
**Goal**: Support stateful, long-running actors with fault tolerance

**Key Shift**: Tasks (stateless, one-shot) → Actors (stateful, migratable)

**Completed Implementation** (4 weeks):
- **Week 1**: Core actor model types (ActorId, ActorMode, ActorCheckpoint, MigrationState, ActorEvent)
- **Week 2**: Checkpoint storage with backends (InMemory, Sled) + gossip protocol (compute:checkpoint)
- **Week 3**: Migration manager with policy-based decisions + gossip coordination (compute:migration)
- **Week 4**: Production features (timeout detection, background management, stateful task submission)

**Deliverables**:
- `ActorCheckpoint` with Ed25519 signatures + Blake3 state hashes
- `CheckpointStore` with pluggable backends
- `ActorMigrationManager` with migration protocol (Request/Accept/Reject/Complete)
- `DefaultMigrationPolicy` + `LocalityFirstPolicy` for intelligent placement
- Background timeout detection (60s) and cleanup (5min retention)
- `ComputeTask.actor_mode` field for stateful execution (backward compatible)
- Comprehensive documentation: [Phase 16D Week 4 Dev Journal](docs/dev-journal/2025-01-XX-phase-16d-week4-production-features.md)

**Test Coverage**: 87 tests passing (11 checkpoint + 8 actor model + 11 migration policy + 6 migration manager + 5 timeout detection + 46 existing compute)

**Example**:
```rust
// Submit stateful task with checkpointing
let task = ComputeTask {
    id: "long-running-service".into(),
    code: TaskCode::Ccl(service_contract),
    actor_mode: Some(ActorMode::Stateful {
        checkpoint_interval_secs: 60,     // Checkpoint every minute
        max_state_size_bytes: 10_485_760, // 10MB max state
    }),
    // ... other fields
};
```

---

#### Phase 16E: Cooperative Scheduling Policies ✅ COMPLETE
**Status**: Complete (2025-11-24)
**Duration**: 4 weeks (Weeks 1-4)
**Goal**: Per-coop rules, quotas, and governance integration

**Completed Implementation**:
- ✅ **Week 1**: Policy types and design (CoopSchedulingPolicy, SchedulingRule, MemberQuota, EnforcementMode)
- ✅ **Week 2**: PolicyManager implementation with quota checking and rule evaluation
- ✅ **Week 3**: Integration with ComputeActor (check_submission, usage tracking)
- ✅ **Week 4**: CLI and RPC interface (10 commands, 6 RPC methods, 6 example policies)

**Features**:
- `CoopSchedulingPolicy` with member priorities, resource quotas, scheduling rules
- Policy enforcement: quotas, data sovereignty, time windows, executor filtering
- Usage tracking: CPU hours, concurrent tasks, credits spent per member
- CLI commands: `icnctl policy` and `icnctl quota` management
- RPC backend: `policy.*` and `quota.*` JSON-RPC methods
- 6 example policies with comprehensive documentation (800+ lines)

**Example Policies Delivered**:
- **basic-cooperative.json**: Simple starter with default quotas (50 CPU hrs/mo)
- **gdpr-compliant.json**: Healthcare with data sovereignty (eu-central region)
- **tiered-membership.json**: Multi-tier with building automation + emergency tiers
- **time-restricted.json**: Off-peak scheduling (nights & weekends)
- **executor-filtering.json**: Security-focused whitelist/blacklist
- **permissive-development.json**: Dev sandbox with relaxed limits

**Governance Integration** (Optional - Next Phase):
```rust
// Future: Connect policies to Phase 13 governance proposals
Proposal {
    domain_id: "research:biolab",
    kind: ConfigChange::SchedulingPolicy {
        rules: vec![
            SchedulingRule::DataSovereignty { region: "eu-central" }
        ],
    },
}
```

**Deliverables**:
- `icn-compute/src/policy.rs` - PolicyManager with 8 rule types, quota enforcement
- `bins/icnctl/src/main.rs` - 10 CLI commands (policy + quota management)
- `icn-rpc/src/server.rs` - 6 RPC methods with JSON validation
- `docs/examples/policies/` - 6 example policies + comprehensive README
- Documentation: CHANGELOG.md, dev journal (1,365 lines total)
- **Test Coverage**: 98 tests passing (30 policy tests + 68 existing compute)

**Spec Impact**: Updates [ARCHITECTURE.md Section 5](docs/ARCHITECTURE.md#5-compute-layer) (Compute Layer - Scheduler Evolution & Cooperative Policies)

---

**Success Criteria (Phases 16A-E Complete) ✅**:
- ✅ Resource-aware placement (CPU/RAM/GPU enforcement) - Phase 16A Complete (2025-11-23)
- ✅ Intelligent scoring beats random by 50%+ - Phase 16B Complete (2025-11-23)
- ✅ Locality optimization: Network + data awareness - Phase 16C Complete (2025-11-24)
- ✅ Fault tolerance: Actors survive crashes via checkpoints - Phase 16D Complete (2025-11-24)
- ✅ Policy compliance: 100% enforcement of coop rules - Phase 16E Complete (2025-11-24)

**Scheduler Evolution: COMPLETE ✅** (5 phases completed in 6 months)

**What's Working**:
- Multi-factor placement scoring with trust, capacity, network, and data locality
- Stateful actors with checkpoint-based migration
- Per-cooperative scheduling policies with quotas, rules, and enforcement
- 98 tests passing across all compute features
- Complete CLI and RPC interface for policy management

**Pilot Validation**: Full scheduler stack (16A-E) ready for pilot deployment

---

### Phase 17: Storage Hardening & Replication (4 weeks)
**Status**: Complete ✅ - **PRE-PILOT CRITICAL**
**Completed**: 2025-11-24 (All 4 weeks done)
**Blocker For**: Production deployment with fault tolerance
**Duration**: 4 weeks

**Motivation**: Current ICN relies on implicit gossip-based replication with no guarantees. If a node fails, data loss is probabilistic. Before pilots deploy to real communities, we need explicit replication policies with health monitoring and automatic recovery.

**Critical Gap Addressed**: Data durability and fault tolerance (see [ARCHITECTURE.md Section 7.4](docs/ARCHITECTURE.md#74-data-durability--replication))

**Scope**:
- Trust-weighted replica selection algorithm
- ReplicationManager actor for health monitoring
- Storage layer extensions for replica tracking
- Gossip protocol extensions (ReplicaRequest/Offer/Status)
- Configurable replication policies per data type
- Automatic re-replication when replicas lost

**Technical Design**:
```rust
// icn-storage/src/replication.rs
pub struct ReplicationPolicy {
    data_type: DataType,
    min_replicas: usize,        // Hard minimum (alert if below)
    target_replicas: usize,     // Soft target (continuous optimization)
    strategy: ReplicationStrategy,
}

pub enum ReplicationStrategy {
    TrustWeighted { min_trust: f64 },           // Replicate to high-trust peers
    Participants { dids: Vec<Did> },            // Contract/ledger participants
    GeoDiverse { regions: Vec<String> },        // Regional spread for resilience
    Hybrid(Vec<ReplicationStrategy>),           // Combine strategies
}

pub enum DataType {
    LedgerEntry,      // Critical: all participants + 3 trusted peers
    Contract,         // Critical: all participants + 2 trusted peers
    TrustEdge,        // Personal: local + 2 high-trust backups
    ComputeTask,      // Ephemeral: 2 executors (temporary)
    ComputeResult,    // Important: submitter + 2 trusted peers
}

// icn-core/src/replication_manager.rs
pub struct ReplicationManager {
    policies: HashMap<DataType, ReplicationPolicy>,
    health_checker: Arc<HealthChecker>,
    replica_selector: Arc<ReplicaSelector>,
}

impl ReplicationManager {
    /// Background task: check replication health every 60s
    pub async fn monitor_loop(&self) {
        loop {
            self.check_all_data_types().await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Trigger re-replication if below min_replicas
    async fn check_all_data_types(&self) {
        for (data_type, policy) in &self.policies {
            let status = self.health_checker.check(data_type).await?;
            if status.replica_count < policy.min_replicas {
                self.trigger_replication(data_type, policy).await?;
            }
        }
    }
}
```

**Configuration**:
```toml
[storage.replication]
enabled = true
check_interval_seconds = 60

[storage.replication.ledger]
min_replicas = 3
target_replicas = 5
strategy = "trust_weighted"
min_trust = 0.4

[storage.replication.contracts]
min_replicas = 2
target_replicas = 3
strategy = "participants"  # All contract participants + trusted backups
```

**Implementation Timeline**:
- **Week 1**: Storage layer extensions
  - Add replica tracking metadata to Store trait
  - Implement `list_replicas(content_hash)` and `get_replica_count()`
  - Add replica metadata to gossip entry announcements
- **Week 2**: Gossip protocol extensions
  - New message types: `ReplicaRequest`, `ReplicaOffer`, `ReplicaStatus`
  - Update gossip actor to track replica locations
  - Implement replica transfer protocol
- **Week 3**: ReplicationManager actor
  - Implement health monitoring loop
  - Trust-weighted replica selection algorithm
  - Integrate with supervisor lifecycle
- **Week 4**: Integration testing and tuning
  - Test failure scenarios (node crash, network partition)
  - Performance testing (replication overhead)
  - Documentation and operational guides

**Success Criteria**:
- Ledger entries survive single node failure (99.9% durability)
- Replica count recovers within 5 minutes of node failure
- Replication overhead <10% of network bandwidth
- Clear operational dashboards show replication health
- Prometheus metrics: `icn_storage_replicas_count`, `icn_storage_replication_failures_total`

**Deliverables** (Complete ✅):
- `icn-store/src/lib.rs` - ReplicaMetadata, ReplicaInfo, ReplicaHealth types + Store trait extensions (11 methods)
- `icn-core/src/replication/manager.rs` - ReplicationManager actor with health monitoring (348 lines)
- `icn-core/src/replication/mod.rs` - Public API (ReplicationConfig, ReplicationHandle)
- `icn-gossip/src/types.rs` - Replica gossip messages (ReplicaRequest, ReplicaOffer, ReplicaStatus)
- `icn-gossip/src/gossip.rs` - request_replicas() method for replica coordination
- `icn-core/tests/replication_integration.rs` - 5 comprehensive integration tests (283 lines)
- `docs/replication-operations.md` - Production operational guide (446 lines)
- Updated ARCHITECTURE.md with replication semantics (already complete)

**Spec Impact**: Implements [ARCHITECTURE.md Section 7.4](docs/ARCHITECTURE.md#74-data-durability--replication) (Data Durability & Replication)

**Prometheus Metrics**:
- `icn_storage_replicas_count{data_type, content_hash}` - Current replica count
- `icn_storage_replicas_below_min_total{data_type}` - Count of under-replicated items
- `icn_storage_replication_requests_total` - Replication requests sent
- `icn_storage_replication_transfers_total` - Successful replica transfers
- `icn_storage_replication_failures_total{reason}` - Failed replication attempts

---

### Phase 18: Pre-Pilot Hardening (6 weeks) ✅ COMPLETE
**Status**: Complete (2025-11-27) - All modules built and integrated into production system
**Blocker For**: Production deployment with malicious nodes - NOW UNBLOCKED
**Duration**: 6 weeks (standalone) + integration
**Progress**: All 6 weeks complete ✅

**Motivation**: Phase 17 provides fault tolerance against crashes. Phase 18 provides safety against malicious behavior. Before deploying to real communities with real economic stakes, we need Byzantine fault detection, conflict resolution, and resource protection.

**Critical Gaps Addressed**: See [ARCHITECTURE.md Section 12](docs/ARCHITECTURE.md#12-known-limitations--future-work)
- 12.1 Byzantine Fault Tolerance
- 12.2 Network Partition Healing
- 12.3 Contract Execution Disputes
- 12.4 Ledger Fork Resolution
- 12.5 Storage Exhaustion Protection
- 12.6 Upgrade Coordination

**Scope - Week 1-2: Byzantine Fault Detection**:
```rust
// icn-security/src/misbehavior.rs
pub struct MisbehaviorDetector {
    violations: HashMap<Did, Vec<Violation>>,
    thresholds: MisbehaviorThresholds,
    reputation_scores: HashMap<Did, ReputationScore>,
}

pub enum Violation {
    InvalidSignature { message_hash: ContentHash },
    ConflictingLedgerEntries { entry1: ContentHash, entry2: ContentHash },
    FailedComputeVerification { task_hash: ContentHash, expected: ContentHash, actual: ContentHash },
    ExcessiveResourceUse { metric: String, observed: u64, limit: u64 },
    TrustGraphSpam { rate: f64, threshold: f64 },
}

pub struct MisbehaviorThresholds {
    quarantine_threshold: usize,    // Violations before auto-quarantine
    ban_threshold: usize,           // Violations before auto-ban
    decay_period: Duration,         // Time for violations to decay
}

impl MisbehaviorDetector {
    pub fn record_violation(&mut self, did: &Did, violation: Violation) {
        self.violations.entry(did.clone()).or_default().push(violation);
        self.update_reputation(did);

        if self.should_quarantine(did) {
            self.emit_quarantine_event(did);
        }
    }
}
```

**✅ Integration Complete (2025-11-26)**:
- **NetworkActor** ([crates/icn-net/src/actor.rs](icn/crates/icn-net/src/actor.rs)): Records `InvalidSignature` and `ReplayAttack` violations during message verification
- **GossipActor** ([crates/icn-gossip/src/gossip.rs](icn/crates/icn-gossip/src/gossip.rs)): Records `ExcessiveResourceUse` violations for unauthorized subscriptions and ACL violations
- **Test Coverage**: 108 icn-net tests + 89 icn-gossip tests passing ✅
- **Security Impact**: Automatic Byzantine behavior detection across network and gossip layers with reputation-based quarantine/ban

**Scope - Week 3: Network Partition Healing** ✅ **COMPLETE (2025-11-27)**:
```rust
// icn-gossip/src/partition.rs
pub struct PartitionDetector {
    last_seen: HashMap<Did, Instant>,
    partition_threshold: Duration,  // 5 minutes of no contact = suspected partition
}

pub struct PartitionHealer {
    vector_clock_merger: VectorClockMerger,
    conflict_resolver: ConflictResolver,
}

impl PartitionHealer {
    /// When partition heals, merge vector clocks and resolve conflicts
    pub async fn heal_partition(&self, peer: &Did, their_clock: VectorClock) -> Result<()> {
        // Merge clocks (detect causality violations)
        let conflicts = self.vector_clock_merger.merge(peer, their_clock)?;

        // Resolve conflicts per data type
        for conflict in conflicts {
            match conflict.data_type {
                DataType::LedgerEntry => self.resolve_ledger_conflict(conflict).await?,
                DataType::Contract => self.resolve_contract_conflict(conflict).await?,
                DataType::TrustEdge => self.resolve_trust_conflict(conflict).await?,
                _ => self.log_and_keep_both(conflict).await?,
            }
        }

        Ok(())
    }
}
```

**Week 3 Deliverables**:
- `icn-gossip/src/partition.rs` - PartitionDetector, PartitionHealer, VectorClockMerger, ConflictResolver (already existed)
- `icn-gossip/src/types.rs` - PartitionHealRequest/PartitionHealResponse gossip messages (NEW)
- `icn-gossip/src/gossip.rs` - Handlers for partition heal protocol + `initiate_partition_healing()` method (NEW)
- `icn-core/tests/partition_integration.rs` - 5 integration tests (NEW)
- Gossip-based partition healing: Request → Response → Vector clock merge → PullRequests for diverged topics

**Scope - Week 4: Contract Execution Disputes** ✅ **COMPLETE**:
```rust
// icn-ccl/src/disputes.rs
pub struct DisputeResolutionSystem {
    dispute_store: Arc<dyn Store>,
    mediator_pool: Vec<Did>,  // Elected mediators from governance
}

pub enum DisputeOutcome {
    SubmitterCorrect,           // Original result stands
    ExecutorCorrect,            // Challenger was wrong
    BothWrong { correct_result: Value },  // Third-party re-execution
    Inconclusive { reason: String },      // Human arbitration needed
}

impl DisputeResolutionSystem {
    /// Challenge a compute result
    pub async fn file_dispute(&self,
        task_hash: ContentHash,
        claimed_result: Value,
        evidence: Vec<u8>,
        challenger: Did,
    ) -> Result<DisputeId> {
        // Automatically re-execute contract
        let canonical_result = self.re_execute_contract(task_hash).await?;

        // If mismatch, quarantine executor
        if canonical_result != claimed_result {
            self.record_misbehavior(&executor, Violation::FailedComputeVerification { ... });
        }

        Ok(dispute_id)
    }
}
```

**Week 4 Deliverables**:
- `icn-ccl/src/disputes.rs` - DisputeResolutionSystem with DisputeOutcome, DisputeReason, DisputeStatus (762 lines)
- `file_dispute()`, `investigate_dispute()` methods with automatic re-execution verification
- 5 tests passing

**Scope - Week 5: Ledger Fork Resolution** ✅ **COMPLETE**:
```rust
// icn-ledger/src/fork_resolution.rs
pub enum ForkResolutionStrategy {
    /// Prefer entry with earlier timestamp (first-write-wins)
    TimestampPreference,

    /// Prefer entry from higher-trust participant
    TrustWeighted,

    /// Prefer entry with more co-signatures (participant consensus)
    MajoritySignatures,

    /// Combination strategy (trust-weighted majority)
    Hybrid,
}

impl Ledger {
    pub fn resolve_fork(&mut self, entry1: &JournalEntry, entry2: &JournalEntry) -> Result<JournalEntry> {
        // Both entries reference same parent but different hashes
        match self.fork_strategy {
            ForkResolutionStrategy::TrustWeighted => {
                let trust1 = self.trust_graph.compute_trust(&entry1.author)?;
                let trust2 = self.trust_graph.compute_trust(&entry2.author)?;

                if trust1 > trust2 { entry1.clone() } else { entry2.clone() }
            }
            // ... other strategies
        }
    }
}
```

**Week 5 Deliverables**:
- `icn-ledger/src/fork_resolution.rs` - ForkResolutionStrategy enum with 4 strategies (489 lines)
- ForkResolver and ForkDetector implementations
- TimestampPreference, TrustWeighted, MajoritySignatures, Hybrid strategies
- 4 tests passing

**Scope - Week 6: Storage Exhaustion & Upgrade Coordination** ✅ **COMPLETE**:
```rust
// icn-storage/src/quotas.rs
pub struct StorageQuotaManager {
    quotas: HashMap<Did, StorageQuota>,
    global_limit: u64,
}

pub struct StorageQuota {
    max_bytes: u64,
    current_bytes: u64,
    priority: QuotaPriority,  // Critical, High, Normal, Low
}

impl StorageQuotaManager {
    pub fn can_store(&self, did: &Did, size: u64) -> Result<()> {
        let quota = self.quotas.get(did).ok_or(Error::NoQuota)?;

        if quota.current_bytes + size > quota.max_bytes {
            return Err(Error::QuotaExceeded);
        }

        Ok(())
    }

    /// Automatic cleanup: evict low-priority data when approaching global limit
    pub fn evict_if_needed(&mut self) -> Result<()> {
        if self.total_usage() > 0.9 * self.global_limit {
            self.evict_lowest_priority_data()?;
        }
        Ok(())
    }
}

// Upgrade coordination via governance (Phase 13 integration)
// See ARCHITECTURE.md Section 12.6 for full design
```

**Week 6 Deliverables**:
- `icn-store/src/quotas.rs` - StorageQuotaManager with per-DID quotas and priority-based eviction
- `can_store()`, `evict_if_needed()` methods
- QuotaPriority enum (Critical, High, Normal, Low)
- Automatic cleanup when approaching global limit

**Success Criteria** ✅ **ALL MET**:
- Byzantine node (invalid signatures, conflicting entries) detected within 3 messages
- Network partition heal completes within 60 seconds of reconnection
- Ledger forks resolve deterministically (100% of test cases)
- Storage exhaustion prevented (no crashes from disk full)
- Upgrade proposals can be created and voted on via governance

**Deliverables**:
- `icn-security/src/misbehavior.rs` - Byzantine fault detection
- `icn-gossip/src/partition.rs` - Partition detection and healing
- `icn-ccl/src/disputes.rs` - Contract execution disputes
- `icn-ledger/src/fork_resolution.rs` - Ledger conflict resolution
- `icn-storage/src/quotas.rs` - Storage quota management
- Integration with Phase 13 governance for upgrade coordination
- Comprehensive test suite for all failure modes
- `docs/security-operations.md` - Incident response procedures

**Spec Impact**: Implements [ARCHITECTURE.md Section 12](docs/ARCHITECTURE.md#12-known-limitations--future-work) (Byzantine Fault Tolerance, Partition Healing, Conflict Resolution, Storage Quotas)

**Prometheus Metrics**:
- `icn_security_violations_total{did, violation_type}` - Misbehavior tracking
- `icn_security_quarantined_peers` - Currently quarantined nodes
- `icn_gossip_partitions_detected_total` - Partition events
- `icn_gossip_partition_heal_duration_seconds` - Healing time
- `icn_ledger_forks_resolved_total{strategy}` - Fork resolution stats
- `icn_storage_quota_exceeded_total{did}` - Quota violations
- `icn_storage_evictions_total{priority}` - Automatic cleanup events

---

### Phase 19: Post-Pilot Improvements (4 weeks)
**Status**: Complete ✅ - All 4 weeks done - **POST-PILOT PRIORITY**
**Progress**: 100% (4/4 weeks complete)
**Timing**: After 3-month pilot, driven by real usage patterns
**Duration**: 4 weeks
**Completion Date**: 2025-11-25

**Motivation**: Phases 17-18 address critical safety gaps. Phase 19 addresses scalability and usability issues that pilots reveal. These are important but not deployment-blockers.

**Critical Gaps Addressed**: See [ARCHITECTURE.md Section 12](docs/ARCHITECTURE.md#12-known-limitations--future-work)
- 12.7 Scalability Limits
- 12.8 Clock Synchronization

**Scope - Week 1-2: Scalability Analysis & Optimization**:

**Current Tested Limits** (baseline):
| Dimension | Tested | Target | Breaking Point |
|-----------|--------|--------|----------------|
| Nodes per cooperative | 10 | 100 | ~1,000 (vector clock overhead) |
| Transactions per second | 10/node | 100/node | ~500/node (signature verification CPU) |
| Trust graph size | 100 DIDs | 1,000 DIDs | ~10,000 (computation time) |
| Gossip topics | 10 | 100 | ~1,000 (memory overhead) |
| Concurrent connections | 20 | 100 | ~500 (file descriptor limits) |

**Optimization Plan**:
```rust
// icn-gossip/src/scalability.rs

/// Week 1: Vector clock compression (reduce from 32 bytes/peer to 8 bytes)
pub struct CompressedVectorClock {
    // Store only deltas from baseline, use varint encoding
    deltas: HashMap<Did, VarInt>,
    baseline_version: u64,
}

/// Week 1: Trust graph caching (reduce computation from O(n²) to O(1) lookups)
pub struct TrustGraphCache {
    precomputed_scores: LruCache<(Did, Did), f64>,
    recompute_threshold: Duration,  // Recompute every 5 minutes
}

/// Week 2: Signature verification batching (verify 100 signatures in parallel)
pub struct SignatureBatchVerifier {
    pending: Vec<(PublicKey, Signature, Message)>,
    batch_size: usize,  // 100 signatures
    max_wait: Duration, // 10ms max latency
}

/// Week 2: Gossip topic sharding (partition large topics across peers)
pub struct TopicShard {
    shard_id: usize,
    total_shards: usize,
    responsible_range: (ContentHash, ContentHash),
}
```

**Scope - Week 3-4: Clock Synchronization**:
```rust
// icn-time/src/sync.rs

/// Implement Rough Time Protocol (RFC 8915) for cooperative-wide clock sync
pub struct RoughTimeClient {
    trusted_servers: Vec<RoughTimeServer>,
    max_clock_skew: Duration,  // 300 seconds
}

pub struct ClockSync {
    offset: Duration,           // Local clock offset from network median
    uncertainty: Duration,      // Confidence interval
    last_sync: Instant,
}

impl ClockSync {
    /// Query multiple time servers, use median
    pub async fn sync(&mut self) -> Result<()> {
        let responses = self.query_servers().await?;
        let median_time = self.compute_median(responses)?;

        self.offset = median_time - Instant::now();
        self.uncertainty = self.compute_uncertainty(responses);

        Ok(())
    }

    /// Reject messages with timestamps outside uncertainty window
    pub fn validate_timestamp(&self, timestamp: u64) -> Result<()> {
        let now = self.network_time();
        let acceptable_range = (
            now - self.max_clock_skew,
            now + self.max_clock_skew,
        );

        if timestamp < acceptable_range.0 || timestamp > acceptable_range.1 {
            return Err(Error::TimestampOutOfRange);
        }

        Ok(())
    }
}
```

**Configuration**:
```toml
[time_sync]
enabled = true
sync_interval_seconds = 600  # Sync every 10 minutes
max_clock_skew_seconds = 300
rough_time_servers = [
    "roughtime.cloudflare.com:2003",
    "roughtime.int08h.com:2002",
]

[scalability]
vector_clock_compression = true
trust_cache_size = 10000
signature_batch_size = 100
topic_sharding_threshold = 1000  # Shard topics with >1000 entries
```

**Success Criteria**:
- Support 100-node cooperatives without degradation
- Process 100 tx/sec/node sustained load
- Trust graph queries <10ms for 1,000 DIDs
- Clock skew <300s across all nodes
- Graceful degradation beyond target limits

**Deliverables**:
- `icn-gossip/src/scalability.rs` - Compression and batching optimizations
- `icn-time/src/sync.rs` - Clock synchronization (Rough Time Protocol)
- `icn-trust/src/cache.rs` - Trust graph caching layer
- Performance benchmarks and scaling tests
- `docs/scalability-guide.md` - Capacity planning guide
- Updated metrics dashboards with scaling indicators

**Prometheus Metrics**:
- `icn_gossip_vector_clock_size_bytes` - Clock size tracking
- `icn_trust_cache_hit_ratio` - Cache effectiveness
- `icn_signature_batch_size` - Batching efficiency
- `icn_time_sync_offset_seconds` - Clock offset from network median
- `icn_time_sync_uncertainty_seconds` - Time uncertainty window

---

### Phase 20: Privacy Enhancements (6 weeks) ✅ **COMPLETE**
**Status**: Complete
**Progress**: 100% (6/6 weeks complete)
**Duration**: 6 weeks
**Completion Date**: 2025-11-26

**Motivation**: Network observers can currently see topic subscriptions, connection graphs, and message timing/sizes, revealing privacy-sensitive patterns. Before wide deployment, we need metadata protection to prevent surveillance and correlation attacks.

**Critical Gap Addressed**: ARCHITECTURE.md Section 12.9 (Privacy & Metadata Leakage)

**Scope - Week 1-2: Encrypted Topic Metadata** ✅ **COMPLETE**:
```rust
// icn-privacy/src/topic_encryption.rs

/// Prevent observers from learning subscription patterns
pub struct TopicEncryptor {
    cipher: ChaCha20Poly1305,
}

pub struct EncryptedTopic {
    ciphertext: Vec<u8>,        // Encrypted topic name
    nonce: [u8; 12],            // Random 96-bit nonce
    bloom_hint: [u8; 32],       // SHA256 hash for discovery
}

impl TopicEncryptor {
    /// Encrypt topic name (observer can't see "ledger:sync")
    pub fn encrypt(&self, topic: &str) -> Result<EncryptedTopic>;

    /// Decrypt if you have the key
    pub fn decrypt(&self, encrypted: &EncryptedTopic) -> Result<String>;

    /// Check if topic might match (probabilistic via Bloom)
    pub fn bloom_matches(&self, topic: &str, encrypted: &EncryptedTopic) -> bool;
}
```

**Deliverables (Week 1-2)** ✅:
- `icn-privacy` crate ([crates/icn-privacy/](icn/crates/icn-privacy/))
- `topic_encryption.rs` - TopicEncryptor, EncryptedTopic, TopicBloomFilter (425 lines)
- `error.rs` - PrivacyError types
- 8 Prometheus privacy metrics in `icn-obs`
- 8 tests passing (7 unit + 1 doc)

**Security Properties**:
- **Confidentiality**: Topic names unreadable to network observers
- **Unlinkability**: Can't correlate multiple topics to same subscriber
- **Plausible Deniability**: Bloom filter false positives provide cover

**Scope - Week 3-4: Onion Routing for Gossip** ✅ **COMPLETE**:
```rust
// icn-privacy/src/onion_routing.rs

/// Multi-hop message routing inspired by Tor
pub struct OnionRouter {
    my_did: Did,
    secret_key: StaticSecret,  // X25519 key
    peer_public_keys: HashMap<Did, PublicKey>,
}

pub struct Circuit {
    relays: Vec<Did>,         // Relay path (excluding sender/recipient)
    recipient: Did,           // Final destination
    shared_keys: Vec<[u8; 32]>, // X25519 ECDH shared keys per hop
}

impl OnionRouter {
    /// Build multi-hop routing path with shared keys
    pub fn create_circuit(&self, relays: Vec<Did>, recipient: Did) -> Result<Circuit>;

    /// Wrap message in layered encryption (innermost → outermost)
    pub fn wrap_message(&self, circuit: &Circuit, payload: &[u8]) -> Result<OnionMessage>;

    /// Peel one encryption layer, return next hop or final payload
    pub fn peel_layer(&self, onion: OnionMessage) -> Result<Option<(Did, OnionMessage)>>;
}

/// Select relay nodes based on trust scores
pub fn select_relays(
    candidates: &[Did],
    trust_scores: &HashMap<Did, f64>,
    num_hops: usize,
    min_trust: f64,  // Default: 0.3
) -> Vec<Did>;
```

**Deliverables (Week 3-4)** ✅ **PRODUCTION-READY**:
- `onion_routing.rs` - OnionRouter, Circuit, OnionMessage (530 lines)
- Circuit creation with X25519 ECDH ephemeral key generation
- Layered ChaCha20-Poly1305 encryption with forward secrecy
- Trust-based relay selection with NaN-safe sorting
- 5 comprehensive tests (circuit, relay selection, NaN handling, end-to-end)
- Metrics: `onion_routes_created_total`, `onion_hops_forwarded_total`

**Critical Bug Fixes**:
- Fixed peel_layer structural mismatch (replaced layer instead of push/increment)
- Implemented functional decrypt_layer with ephemeral keys (no longer a stub)
- Fixed NaN panic in select_relays (use total_cmp instead of partial_cmp().unwrap())

**Implementation Status**:
- ✅ **Production-ready**: Full ephemeral key support, functional encryption/decryption
- ✅ **Forward secrecy**: Unique ephemeral keypair per layer, no key reuse
- ✅ **Relay decryption**: ECDH(relay_secret, ephemeral_pubkey) - no pre-shared keys
- ✅ **End-to-end tested**: Sender wraps → relay peels → recipient extracts
- Trust threshold of 0.3 filters out low-trust relays
- NaN trust scores safely handled (sorted to end, never panic)

**Security Properties** (fully implemented):
- **Sender Anonymity**: Recipient doesn't know original sender
- **Receiver Anonymity**: Relays don't know final recipient
- **Unlinkability**: Can't correlate sender → recipient
- **Traffic Analysis Resistance**: Multi-hop routing hides patterns
- **Forward Secrecy**: Ephemeral keys per layer prevent retrospective decryption

**Scope - Week 5-6: Traffic Obfuscation** ✅ **COMPLETE**:
```rust
// icn-privacy/src/traffic_obfuscation.rs

/// Traffic obfuscation for privacy-preserving message transmission
pub struct TrafficObfuscator {
    config: ObfuscationConfig,
}

pub struct ObfuscationConfig {
    enable_delays: bool,
    min_delay_ms: u64,
    max_delay_ms: u64,
    enable_padding: bool,
    padded_size: usize,
    enable_cover_traffic: bool,
    cover_traffic_rate: f64,
}

impl TrafficObfuscator {
    /// Generate random delay for timing resistance
    pub fn random_delay(&self) -> Duration;

    /// Pad message to hide true size
    pub fn pad_message(&self, message: &[u8]) -> Result<ObfuscatedMessage>;

    /// Remove padding from obfuscated message
    pub fn unpad_message(&self, obfuscated: &ObfuscatedMessage) -> Result<Vec<u8>>;

    /// Generate cover traffic (decoy message)
    pub fn generate_cover_traffic(&self) -> Vec<u8>;

    /// Check if cover traffic should be sent (probabilistic)
    pub fn should_send_cover_traffic(&self, time_since_last: Duration) -> bool;
}
```

**Deliverables (Week 5-6)** ✅:
- `traffic_obfuscation.rs` - TrafficObfuscator, ObfuscationConfig, ObfuscatedMessage (394 lines)
- Random message delays (configurable min/max: 0-500ms default)
- Message size padding (configurable size: 1KB default)
- Cover traffic generation with probabilistic scheduling
- Configurable per-feature enable/disable
- 10 comprehensive tests (delay, padding, cover traffic, probability)

**Implementation Notes**:
- Cover traffic disabled by default (bandwidth intensive, opt-in)
- Padding validates message size doesn't exceed target
- Probabilistic scheduling: P(send) = rate × time_delta
- All obfuscation features independently configurable

**Security Properties** (added):
- **Timing Resistance**: Random delays prevent correlation attacks
- **Size Uniformity**: Padding hides message content patterns
- **Traffic Camouflage**: Cover traffic obscures real message count

**Prometheus Metrics** (8 privacy metrics):
- `icn_privacy_topics_encrypted_total` - Topics encrypted
- `icn_privacy_topics_decrypted_total` - Topics decrypted
- `icn_privacy_bloom_filter_hits_total` - Bloom matches
- `icn_privacy_bloom_filter_misses_total` - Bloom misses
- `icn_privacy_onion_routes_created_total` - Onion routes (Week 3-4)
- `icn_privacy_onion_hops_forwarded_total` - Hops forwarded (Week 3-4)
- `icn_privacy_cover_traffic_sent_total` - Cover traffic (Week 5-6)
- `icn_privacy_messages_padded_total` - Messages padded (Week 5-6)

**Testing**:
- **23 tests passing** (22 unit + 1 doc)
- Week 1-2: 8 tests (topic encryption - roundtrip, nonce, Bloom, wrong key, find matches)
- Week 3-4: 5 tests (onion routing - circuit, relay selection, trust filtering, NaN handling, end-to-end)
  - End-to-end test validates full wrap/peel/extract flow with ephemeral keys
- Week 5-6: 10 tests (traffic obfuscation - delays, padding, cover traffic, probability)

---

### Phase 21: Contribution Credits & Infrastructure Incentives (6-8 weeks)
**Status**: Design Document Complete (RFC v0.3.0) - **POST-PILOT**
**Design Doc**: [docs/contribution-credits-design.md](docs/contribution-credits-design.md)
**Glossary**: [docs/glossary.md](docs/glossary.md)

**Motivation**: How do we credit infrastructure contributors (compute, storage, bandwidth) in a non-speculative way? The core insight: **infrastructure provision is labor directed at the network**.

**Key Design Decisions**:
1. **Three-Tier Credit System**: Internal → Federated → Bridge (graduated exchangeability)
2. **Infrastructure as Labor**: Node operators earn same credits as service providers
3. **Anti-Speculation Mechanisms**: Demurrage, provenance tracking, governance approval
4. **Fuel System**: Regenerative rate-limiting (not a token) integrated with contributions
5. **Two Pillars**: Communities (civic) + Cooperatives (economic) as first-class entities
6. **Protocol Contracts**: Economic rules in CCL, governable and auditable

**Implementation Phases**:
- **Phase 21A**: Foundations - Protocol contracts, glossary, terminology cleanup (2 weeks)
- **Phase 21B**: Contribution & Metering - Resource tracking, attestation protocol (4 weeks)
- **Phase 21C**: Fuel System - `icn-fuel` crate, regeneration, operation costs (4 weeks)
- **Phase 21D**: Organizations - `icn-organization` crate, communities, households (4 weeks)
- **Phase 21E**: Exchange - Federation credits, AMM pools, demurrage (6 weeks)
- **Phase 21F**: Marketplace - Listings, trades, multi-currency support (4 weeks)

**Success Criteria**:
- Node operators can earn credits for providing compute/storage/bandwidth
- Credits are spendable within cooperative ecosystem
- No external speculation possible (credits only useful internally)
- Federation allows cross-coop exchange with governance approval
- Fuel system prevents spam without creating economic barriers

**Deliverables**:
- `icn-fuel` crate (regenerative rate-limiting)
- `icn-organization` crate (communities, households)
- `icn-marketplace` crate
- `icnctl contribution`, `icnctl fuel`, `icnctl org`, `icnctl marketplace` commands
- Gateway API endpoints for all new systems
- Protocol contracts in `examples/contracts/protocol/`
- Updated ARCHITECTURE.md with economic layer

**Why Post-Pilot**: Let pilot communities reveal actual infrastructure incentive needs before building. Current design is hypothesis to validate.

---

### Phase 22+: Future Enhancements
**Status**: Not Planned - **DRIVEN BY PILOT LEARNINGS**

**Trust Graph Hardening** (Phase 22?):
- Sybil detection algorithms (graph analysis for fake identities)
- Contribution decay (require ongoing participation to maintain trust)
- Attestation chains (evidence-based trust, not just social links)
- Trust delegation limits (prevent cascading trust exploitation)

**App Runtime** (if co-ops need custom backend logic):
- WASM-based app platform
- Sandboxed execution environment
- Resource quotas per application

**Governance Templates** (if patterns emerge from pilots):
- Consensus-with-fallback contracts
- Sociocracy consent decision templates
- Council delegation patterns

**Better Self-Hosting Tools** (if co-ops struggle with devops):
- One-click deployment (Ansible playbooks)
- Automatic TLS certificate management
- Monitoring stack templates

**Federation** ✅ **COMPLETE** (2025-11-28):
- ✅ Cross-cooperative credit settlement (ClearingManager with bilateral agreements)
- ✅ Governance bridging protocols (scoped gossip, federated attestations)
- ✅ Shared resource pools (cooperative registry, trust bridging)

**Mobile Apps** (if web-on-phone isn't sufficient):
- Native iOS/Android clients
- Offline-first sync
- Push notifications

**Philosophy**: Don't speculate. Build what pilots prove is necessary. These are hypotheses to validate, not a fixed roadmap.

---

### Implementation Priorities

**Pre-Pilot Critical Path** ✅ **COMPLETE**:
1. ✅ **Phases 11-12 Complete**: Multi-device identity, economic safety (2025-01-14)
2. ✅ **Phase 14 Gateway Complete**: REST API, WebSocket events (2025-01-15)
3. ✅ **Track B1 Complete**: Operational hardening (2025-01-14)
4. ✅ **Track B3 Complete**: Economic modeling (2025-01-14)
5. ✅ **Phase 17 Complete**: Storage hardening & replication (2025-11-24)
6. ✅ **Phase 18 Complete**: Pre-pilot hardening (Byzantine, conflicts, quotas) (2025-11-27)

**ICN is now PILOT-READY** with fault-tolerant, Byzantine-resistant infrastructure.

**Post-Pilot Improvements** (4+ weeks):
7. **Phase 19** (4 weeks): Scalability & clock sync - **AFTER 3-MONTH PILOT**
8. **Phase 20+**: Privacy, trust hardening - **AS NEEDED**

**Parallelization Opportunities**:
- Track C1 (pilot selection) can start immediately - **CRITICAL PATH**
- Track B2 (legal docs) continues in background
- Phase 19 scope can be refined based on early pilot feedback

**Critical Milestone ACHIEVED** ✅ (2025-11-27): ICN is production-ready for pilot deployment with:
- ✅ Fault-tolerant data storage (Phase 17 - replication with 99.9% durability)
- ✅ Byzantine fault detection (Phase 18 - misbehavior detection and quarantine)
- ✅ Conflict resolution mechanisms (Phase 18 - partition healing, fork resolution)
- ✅ Resource protection (Phase 18 - storage quotas with priority-based eviction)
- ✅ Economic safety rails (Phase 12 - dynamic limits, disputes, write-offs)
- ✅ Operational tooling (Track B1 - backup/restore, monitoring, graceful restart)

**See Also**:
- [ARCHITECTURE.md Section 7.4](docs/ARCHITECTURE.md#74-data-durability--replication) - Replication design
- [ARCHITECTURE.md Section 12](docs/ARCHITECTURE.md#12-known-limitations--future-work) - Complete gap analysis

---

### Phase 13: Governance Primitives v1 (6-8 weeks)
**Status**: Foundation Started, Full Implementation Deferred
**Driven By**: Platform layer enables governance features in apps

**Motivation**: Cooperatives need to make collective decisions: membership, resource allocation, conflict resolution, rule changes. ICN currently has contracts but no governance patterns. We don't need "the" governance system - we need pluggable primitives that communities can compose.

**Scope**:
- CCL primitives for proposals, quorum, thresholds
- 3-4 governance template contracts
- Role/membership management
- State machine hooks for governance flows

**CCL Extensions Needed**:
```
// Governance primitives (built-in capabilities)
proposal_create(subject: String, payload_ref: Hash) -> ProposalID
proposal_vote(id: ProposalID, vote: Vote) -> Result
proposal_state(id: ProposalID) -> ProposalState
quorum_met(members: Vec<DID>) -> bool
threshold_met(yes: u64, no: u64, abstain: u64, threshold: f64) -> bool
has_role(member: DID, role: String) -> bool
member_count() -> u64

// State machine hooks
on_proposal_open(callback)
on_proposal_consent(callback)
on_proposal_block(callback)
on_proposal_timeout(callback)
on_proposal_execute(callback)
```

**Governance Templates** (shipped as `.ccl` files):
1. **Consensus with Fallback Majority**
   - Try for full consensus (7-day period)
   - Fall back to 2/3 majority if no consensus
2. **Sociocracy-style Consent**
   - Passes unless explicit objection with reason
   - Objection triggers mandatory deliberation
3. **Council Delegation**
   - Elected council makes day-to-day decisions
   - Membership can recall with supermajority
4. **Emergency Lock**
   - Immediate action by designated responders
   - Requires ratification within 48 hours

**Success Criteria**:
- Pilot community can encode their existing governance model in CCL
- Proposals have clear lifecycle (open → deliberation → decision → execution)
- System supports at least 3 distinct governance patterns
- Documentation shows how to create custom governance contracts

**Deliverables**:
- CCL governance primitives (`icn-ccl/src/governance.rs`)
- 4 governance template contracts (`templates/governance/*.ccl`)
- `icnctl governance` subcommands
- Documentation: `docs/governance-primitives.md`

**IMPORTANT**: Do not build this until Phase C2 (pilot community engagement) reveals what they actually need. This scope is a *hypothesis* to be validated by real use.

---

### Intentional Deferments (Pending Pilot Feedback)

These features are **NOT on the roadmap** until pilot communities demonstrate need. Based on gap assessment (2025-01-14):

**Federation/Interoperability** ✅ **COMPLETE** (2025-11-28):
- **Status**: Full federation layer implemented (icn-federation crate)
- **Capabilities**:
  - Cooperative Registry: Register, discover, and vouch for other cooperatives
  - Trust Bridging: Federated attestations with trust context (economic, social, governance)
  - Credit Settlement: Bilateral clearing agreements with position tracking
  - Scoped Gossip: Federation channels with topic routing
  - DID Resolution: Federated DID format (`did:icn:coop-id:pubkey`)
- **API Access**: 14 REST endpoints at `/v1/federation/*` + `icnctl federation` CLI
- **Tests**: 48 federation tests + 104 gateway tests passing
- **Rationale**: Built proactively to enable multi-coop pilots when ready

**Integrated Messaging** (Deferred):
- **Status**: Gossip provides pub/sub bulletin board, not real-time chat
- **Gap**: No Signal Protocol, OMEMO, or private messaging
- **Interim**: Use external tools (Signal, email) for chat; gossip for announcements
- **Decision**: Pilot first, add messaging in Phase 14+ if bulletin board insufficient
- **Rationale**: Tight scope enables pilot success; messaging is scope creep

**Advanced Privacy** (Deferred):
- **Status**: QUIC/TLS transport + X25519 end-to-end encryption for payloads
- **Gap**: No zero-knowledge proofs, selective disclosure, anonymous credentials
- **Decision**: Trust-first communities don't need advanced privacy tech
- **Rationale**: Cooperatives share resources among known members; ZK is solution looking for problem

**NAT Traversal** (Phases 1-3 Complete + Enhancements → 2025-11-17):
- **Status**: STUN discovery + candidate exchange + hole punching + enhancements IMPLEMENTED
- **What's Done**:
  - Phase 1: Manual STUN protocol (RFC 5389) discovers public IP/port
  - Phase 2: ConnectionCandidate exchange via gossip
  - Phase 3: Automatic connection attempts (local addr → public addr priority)
  - CandidateCache with TTL-based expiration (5 min default)
  - **Enhancement**: Parallel STUN queries with majority vote consensus
  - **Enhancement**: Configurable STUN servers via NetworkConfig (privacy + performance)
  - 97 icn-net tests + 4 integration tests passing (460 total)
- **What's Deferred**: Phase 4 (TURN relay) awaiting pilot need
- **Why Now**: Implemented as part of development exploration; validates architecture for when pilots need it
- **Deployment**: Enabled by default with Google STUN; configurable via `icn.toml`

**Cross-Network Standards** (Deferred):
- **Status**: QUIC/TLS works over internet, only discovery is LAN-only (mDNS)
- **Gap**: No standardized discovery protocol for ICN-to-ICN across regions
- **Interim**: Manual peer connection (`icnctl network add-peer <addr> <did>`)
- **Decision**: Add lightweight discovery (DNS TXT records?) in Phase 14+ if pilots demand it
- **Rationale**: Manual peering validates need before building full discovery infrastructure

**Explicitly Out of Scope**:

**Formal Verification** (Never):
- **Status**: CCL has fuel metering, type checking, comprehensive tests (268 passing)
- **Gap**: No formal proofs of contract correctness
- **Decision**: Too expensive for 1-2 developer team; tests + code review sufficient for cooperative-scale (10-1000 members)
- **Rationale**: Formal verification targets financial infrastructure at nation-scale; ICN serves community-scale mutual credit

**Philosophy**: Build what communities need, not what the architecture diagram suggests. Pilot feedback drives roadmap.

---

### Future Phases (Driven by Pilot Learnings)

**Phase 14+: Cooperation Layer**
- Proposals, decisions, signaling, scheduling
- Group identities and working groups
- Role-based permissions
- *Scope TBD based on pilot community workflows*

**Phase 15+: Reputation Layer**
- Structured contribution records
- Signed evidence (not scores)
- Time-based decay
- *Driven by what communities actually track*

**Federation Layer** ✅ **COMPLETE** (2025-11-28)
- ✅ Cross-cooperative boundaries (scoped gossip, DID resolution)
- ✅ Inter-coop credit settlement (ClearingManager, bilateral agreements)
- ✅ Governance bridging (federated attestations, trust context)
- *See lines 1365-1376 for full capabilities*

---

## Track B: Operational & Legal Backbone

### B1: Operational Hardening ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Blocker For**: Production deployment - NOW UNBLOCKED

**Backup & Restore**: ✅
- [x] Document all ICN data locations (`~/.icn/*`)
- [x] Implement `icnctl backup <path>` (encrypted Age bundle with SHA256 checksum)
- [x] Implement `icnctl restore <path>` (with validation and force-restore)
- [x] Best practices doc: daily snapshots, off-site storage, encryption
- [x] State snapshot integration (backup includes `state.snapshot`)

**Monitoring Dashboard**: ✅
- [x] Real-time web UI at `:8080/` with Prometheus metrics
- [x] Key metrics: connections, gossip topics, subscriptions, message rates, snapshot operations
- [x] Health check endpoint (`/health`) for external monitoring (JSON format)
- [x] 11 snapshot-specific metrics for operational visibility

**Upgrade Mechanism**: ✅
- [x] Versioned network protocol with automatic validation
- [x] **Graceful restart semantics** (preserve vector clocks, subscriptions, X25519 keys)
- [x] State snapshot persistence (gossip + network state)
- [x] Signal handling (SIGTERM, SIGINT) for clean shutdown
- [x] Sub-millisecond snapshot save/load performance

**Incident Response Playbook**: ✅
- [x] Document: "Node is compromised - what do?" (7 procedures)
- [x] Document: "Ledger corruption detected - how to recover?"
- [x] Document: "Key suspected stolen - rotation ceremony"
- [x] Document: Network partition, gossip divergence, disk full, protocol mismatch
- [x] Comprehensive troubleshooting guides

**Deliverables**: ✅
- [x] `docs/operations-guide.md` (comprehensive, 800+ lines)
- [x] `docs/incident-response.md` (7 major incident procedures)
- [x] Backup/restore commands in `icnctl` (with test coverage)
- [x] Real-time monitoring dashboard (static HTML + Prometheus integration)
- [x] Graceful restart implementation (snapshot-based state persistence)

---

### B2: Legal & Regulatory Radar ✅ FOUNDATION COMPLETE
**Status**: Foundation Complete (2025-01-17)
**Priority**: Medium (document early, don't block on it)

**Goal**: Not "solve all legal problems" but "know what questions communities will face."

**Completed Deliverables**:
- [x] `docs/legal-considerations.md` (comprehensive guide):
  - Money transmission regulations by jurisdiction (US, EU, UK)
  - Tax reporting requirements (IRS barter exchange rules)
  - Data protection (GDPR, CCPA) with privacy policy template
  - Liability considerations and mitigation strategies
  - Corporate structure options for cooperatives
  - Compliance checklist (before launch and ongoing)
  - Resource links for legal, tax, privacy, and cooperative development
- [x] Privacy/data minimization guidelines included in legal-considerations.md
- [x] Export formats documented (CSV for accountants)

**Ongoing**:
- [ ] Update as real communities raise specific concerns
- [ ] Add jurisdiction-specific appendices as needed

**Non-Goals**:
- We are NOT building a compliance framework
- We are NOT seeking legal opinions yet
- We ARE documenting known questions so communities can consult their own lawyers

---

### B3: Economic Modeling ✅ COMPLETE
**Status**: Complete (2025-01-14)
**Purpose**: Validate economic assumptions before they blow up in production

**Implementation**: Agent-based simulation using Mesa 3.3.1
- **Agents**: 100 per scenario with 5 behavioral types
- **Duration**: 12 months (360 days) per simulation
- **Scenarios**: 5 configurations testing different policy parameters
- **Results**: ~13,000 transactions per scenario, comprehensive metrics

**Key Findings**:
1. ✅ **Dynamic credit limits work**: -33% defaults, -16% velocity (stability vs growth tradeoff)
2. ✅ **Demurrage highly effective**: -22% inequality (Gini) without harming velocity
3. ✅ **System tolerates free-riders**: Up to 20% before serious stress (4.1% defaults)
4. ⚠️ **Sparse trust networks increase hoarding**: 2x hoarding at 30% density vs 60% (counterintuitive)

**Validated Defaults** (now implemented in Phase 12):
- Credit limits: -20 initial → -500 max, +10 per 50 cleared, 2x trust multiplier
- Demurrage: -2% monthly on balances >50
- New member protection: 3-month ramp, 10 credit contribution requirement

**Deliverables**: ✅
- [x] `sims/mutual-credit/` - Complete simulation framework (agents, economy, trust, model)
- [x] 5 JSON scenario configurations (baseline, dynamic limits, demurrage, free riders, low trust)
- [x] `sims/mutual-credit/RESULTS_SUMMARY.md` - Comprehensive analysis
- [x] `docs/econ-modeling.md` - Updated with simulation results
- [x] Analysis notebooks for visualization

**Next**: Calibrate against pilot data (Track C3) to validate real-world applicability

---

## Track C: Pilot Community & Bootstrap

### Pilot Tooling ✅ COMPLETE
**Status**: Complete (2025-01-17)
**Purpose**: Everything needed to deploy to a pilot community

**Completed Components**:
1. **icn-console** - Interactive TUI for cooperative management
   - Dashboard, Members, Ledger, Governance, Trust tabs
   - Real-time stats and navigation
   - Gateway API integration
2. **install.sh** - One-line installer
   - Linux (systemd) and macOS (launchd) support
   - x86_64 and aarch64 architectures
   - Automatic service installation
3. **icnctl init-coop** - Interactive setup wizard
   - Identity creation
   - Trust graph initialization
   - Governance domain setup
4. **Pilot Playbook** - Step-by-step deployment guide
   - Pre-deployment checklist
   - Infrastructure setup
   - Member onboarding
   - Day-2 operations
   - Success metrics
5. **Grafana Dashboard** - Monitoring
   - Network, gossip, ledger panels
   - Security and rate limiting
   - Snapshot and version negotiation metrics
6. **Pilot Proposal Template** - Community outreach
   - Customizable proposal for approaching communities
   - Success criteria and timeline
   - FAQ and technical summary
7. **Pilot Web UI** - Simple browser interface
   - Dashboard with balance and stats
   - Log hours form
   - Transaction history
   - Member list
   - No build step (vanilla HTML/CSS/JS)
8. **TypeScript SDK** - @icn/client npm package
   - Full Gateway API coverage
   - Type definitions
   - WebSocket events
   - Usage examples
9. **OpenAPI Specification** - API documentation
   - All endpoints documented
   - Request/response schemas
   - Can generate clients

**Deliverables**:
- [icn-console](icn/bins/icn-console/) - TUI binary
- [scripts/install.sh](scripts/install.sh) - Installer
- [docs/pilot-playbook.md](docs/pilot-playbook.md) - Deployment guide
- [monitoring/](monitoring/) - Grafana dashboard and setup
- [docs/pilots/pilot-proposal-template.md](docs/pilots/pilot-proposal-template.md) - Outreach template
- [web/pilot-ui/](web/pilot-ui/) - Simple web interface
- [sdk/typescript/](sdk/typescript/) - TypeScript SDK
- [docs/api/openapi.yaml](docs/api/openapi.yaml) - API specification

---

### C1: Community Selection (2-4 weeks, can start immediately)
**Status**: Not Started
**Critical Path**: This drives everything else

**Selection Criteria**:
1. **Existing trust web** (ICN is not solving "everyone hates each other")
2. **Real, recurring coordination problems** (not hypothetical use case)
3. **Openness to experiment** (willing to tolerate rough edges)
4. **Some digital fluency** (can handle CLI tools initially, want better UX)

**Candidate Archetypes** (ranked by simplicity):
1. **Timebank** (RECOMMENDED FIRST)
   - Already mutual-credit-shaped (hours = currency)
   - Simple economic model (1 hour = 1 hour, no pricing complexity)
   - Clear value: "replace our spreadsheet with something that doesn't break"
   - Lower stakes than housing/money
2. **Housing Cooperative**
   - Rich governance needs (maintenance, conflict resolution, membership)
   - Real stakes (people's homes)
   - More complex, but if you have a warm relationship, could work
3. **Community Land Trust**
   - Very high stakes, slower decision cycles
   - Better as second-wave pilot after timebank proves the model

**Action Items**:
- [ ] List 2-3 real organizations you have connections to
- [ ] Draft one-page pilot proposal:
  - "Here's what ICN does"
  - "Here's what we'd pilot (replace X painful workflow)"
  - "Here's what we need from you (weekly feedback, 3-5 active users)"
  - "Here's the timeline (3-month initial pilot)"
- [ ] Start one real conversation this week

---

### C2: Minimum Viable Product for Pilot (scoped after C1 completes)
**Status**: Blocked on C1 (community selection)
**Philosophy**: We are not shipping "ICN the substrate." We are solving 3-5 specific painful workflows for one community.

**Example: Timebank Pilot MVP**

**Jobs to Be Done** (validate with actual community):
1. Log hours worked/received
2. Browse offers and requests
3. See my balance and history
4. Resolve disputes about logged hours
5. View community health (total hours exchanged, active members)

**Technical Scope**:
- **Simple web UI** (not mobile app, not fancy)
  - Login with DID (QR code or key file upload)
  - Dashboard: your balance, recent transactions
  - Log hours form: "I gave 2 hours to Alice for gardening help"
  - Browse: list of open offers/requests (stored as CCL contracts)
  - Dispute: flag an entry as contested
- **Backend**: `icn-rpc` gRPC API (already exists, may need extensions)
- **Interoperability v0**:
  - Email notifications: "You received 2 hours from Alice" (via simple SMTP)
  - Public web page: read-only stats (total hours, active members) as HTML
  - CSV export: for treasurer to hand to accountant

**Non-Goals for MVP**:
- ❌ Mobile app (use web on phone)
- ❌ Real-time collaboration (async is fine)
- ❌ Complex governance (Phase 13)
- ❌ Federation (one community only)

**Deliverables**:
- `icn-web/` - Simple web UI (could be static HTML + JS, or basic Rust/Actix server)
- `docs/pilot-deployment.md` - How to run the pilot stack
- Instrumentation for learning (see C3)

---

### C3: Learning Loop (ongoing during pilot)
**Status**: Not Started
**Purpose**: "This single deployment will teach you more than 6 months of architecture work."

**Weekly Debrief Structure**:
- Meet with 2-3 core pilot community members
- Questions:
  - What worked this week?
  - What broke or confused you?
  - What did you try to do but couldn't?
  - What would you change?
- Document in `docs/pilot-learnings/YYYY-MM-DD.md`

**Instrumentation** (add to pilot MVP):
- Failed transactions: what errors do users hit?
- Abandoned flows: where do people give up?
- Support requests: what questions come up repeatedly?
- Feature requests: what do they ask for that doesn't exist?

**Decision Protocol**:
- **Do NOT over-fit the substrate to one community's quirks**
- Look for patterns across 3+ similar requests
- Validate: "Is this a general cooperative need or specific to this group?"
- Prioritize: Does this unblock adoption or just polish the happy path?

**Success Criteria** (3-month pilot):
- 10+ active users logging hours weekly
- At least 3 governance decisions made using ICN primitives
- Community says: "We'd rather fix this than go back to spreadsheets"
- 2-3 other communities express interest based on pilot results

**Deliverables**:
- `docs/pilot-learnings/` directory with weekly notes
- Quarterly retrospective: what changed in the roadmap based on reality?
- Public case study (with community permission)

---

## Track S: Sovereign Digital Identity System (SDIS)

### Overview
**Status**: Phase S1-S6 Complete ✅ (2025-12-10)
**Purpose**: Post-quantum secure identity with recoverable anchors and zero-knowledge credentials

SDIS extends ICN's identity layer with:
- **Anchor-based identity**: Permanent identity roots that survive key rotation
- **Post-quantum cryptography**: Hybrid Ed25519 + ML-DSA signatures
- **Steward network**: Distributed VUI computation and enrollment
- **Zero-knowledge proofs**: Privacy-preserving attribute verification
- **Three-tier credential presentation**: QR (L1) → NFC/BLE (L2) → Network (L3)

### Phase S1: Cryptographic Foundations ✅ COMPLETE
**Crate**: `icn-crypto-pq`

- ✅ Hybrid signatures (Ed25519 + ML-DSA-65)
- ✅ ML-KEM key encapsulation
- ✅ Threshold PRF for VUI computation
- ✅ Blind signatures for enrollment tokens

### Phase S2: Identity Extensions ✅ COMPLETE
**Crate**: `icn-identity`

- ✅ Anchor types (permanent identity roots)
- ✅ KeyBundle (rotatable key containers)
- ✅ VUI types (Verifiable Unique Identifier)
- ✅ Keystore v4 with SDIS support

### Phase S3: Steward Network ✅ COMPLETE
**Crate**: `icn-steward`

- ✅ StewardProfile and status management
- ✅ EnrollmentToken with blind signatures
- ✅ VuiRegistry (Bloom filter + exact set)
- ✅ EnrollmentCeremony and RecoveryCeremony
- ✅ StewardActor with handle pattern

### Phase S4: Zero-Knowledge Proofs ✅ COMPLETE
**Crate**: `icn-zkp`

- ✅ STARK proof generation/verification
- ✅ ProofType enum (Age, Citizenship, Membership, NonRevocation)
- ✅ 128-bit security with Goldilocks field
- ✅ Configurable proof parameters

### Phase S5: Credential Presentation ✅ COMPLETE
**Crate**: `icn-gateway` (api/sdis module)

- ✅ EphemeralProof with 137-byte QR encoding
- ✅ EphemeralBinding for L2 verification
- ✅ Three-tier verification (L1/L2/L3)
- ✅ Replay protection with nonce cache
- ✅ REST API endpoints (/v1/sdis/*)

### Phase S6: Governance & Polish ✅ COMPLETE
**Crate**: `icn-governance` (sdis module)

- ✅ SdisProposal enum (12 proposal types)
- ✅ Voting requirements per proposal type
- ✅ StewardPenalty and JurisdictionTier types
- ✅ 19 integration tests
- ✅ Security audit materials (threat model, checklist, crypto review)

### Documentation

| Document | Location |
|----------|----------|
| Implementation Plan | [docs/SDIS_IMPLEMENTATION_PLAN.md](docs/SDIS_IMPLEMENTATION_PLAN.md) |
| User Guide | [docs/SDIS_USER_GUIDE.md](docs/SDIS_USER_GUIDE.md) |
| Threat Model | [docs/security/SDIS_THREAT_MODEL.md](docs/security/SDIS_THREAT_MODEL.md) |
| Audit Checklist | [docs/security/SDIS_AUDIT_CHECKLIST.md](docs/security/SDIS_AUDIT_CHECKLIST.md) |
| Crypto Review | [docs/security/SDIS_CRYPTO_REVIEW.md](docs/security/SDIS_CRYPTO_REVIEW.md) |

### Remaining Work

| Item | Status | Notes |
|------|--------|-------|
| Enrollment flow | 🚧 Planned | Requires operational steward network |
| Multi-steward ceremonies | 🚧 Planned | Depends on enrollment |
| Key recovery integration | 🚧 Planned | Social recovery via stewards |

### Security Properties

- **128-bit classical security** (Ed25519)
- **128-bit post-quantum security** (ML-DSA-65 at L2+)
- **Replay protection** via 16-byte nonces
- **Privacy-preserving** via zero-knowledge proofs
- **Recoverable** via steward-assisted ceremonies

---

## Critical Path Summary

**Completed Prerequisites for Pilot Deployment:**
1. ✅ Phase 10: Security hardening, encryption (COMPLETE)
2. ✅ Phase 11: Multi-Device Identity (COMPLETE - 2025-01-14)
3. ✅ Phase 12: Economic Safety Rails (COMPLETE - 2025-01-14)
4. ✅ Track B1: Operational Hardening (COMPLETE - 2025-01-14)
5. ✅ Track B3: Economic Modeling (COMPLETE - 2025-01-14)
6. ✅ Phase 14: Gateway API Core (COMPLETE - 2025-01-15)
7. ✅ Phase 15: Distributed Compute (COMPLETE - 2025-11-21)
8. ✅ Phase 16: Scheduler Evolution (COMPLETE - 2025-11-24)

**Infrastructure Complete - What Must Happen Before Pilot Deployment:**
1. ✅ **Phase 17** (4 weeks): Storage hardening & replication - **COMPLETE** (2025-11-24)
   - ✅ Trust-weighted replica selection
   - ✅ ReplicationManager actor with health monitoring
   - ✅ Gossip protocol extensions for replica coordination
   - ✅ 99.9% durability target achieved
2. ✅ **Phase 18** (6 weeks): Pre-pilot hardening - **COMPLETE** (2025-11-27)
   - ✅ Byzantine fault detection and quarantine
   - ✅ Network partition healing with conflict resolution
   - ✅ Contract execution disputes and ledger fork resolution
   - ✅ Storage quotas with priority-based eviction
3. ⏳ **C1**: Select pilot community (2-4 weeks) - **IMMEDIATE PRIORITY**
4. ⏳ **C2**: Build MVP for that community's workflows (4-6 weeks)
   - TypeScript SDK for pilot needs (already exists)
   - Simple web UI for pilot workflows (already exists)
   - Pilot-specific integrations (email, notifications)

**Pilot-ready infrastructure: COMPLETE** ✅
**Time to pilot deployment: ~6-10 weeks** (community selection + MVP refinement)

**Parallelization**:
- C1 (community selection) can start immediately - **TOP PRIORITY**
- C2 (MVP development) can begin as soon as community is selected
- B2 (legal docs) continues in background

**What Happens After Pilot:**
- **Phase 19** (4 weeks): Scalability & clock sync - **POST-PILOT IMPROVEMENTS**
  - Driven by actual pilot load patterns
  - Vector clock compression, trust caching, signature batching
  - Rough Time Protocol for clock synchronization
- **Phase 20+**: Privacy, trust hardening - **AS NEEDED**
  - Conditional on pilot feedback and real-world requirements
- Phase 13 governance scope prioritized by what pilot communities actually need
- **Federation already complete** (2025-11-28) - ready when 2+ communities want to interconnect

---

## Open Questions

**Technical:**
- Multi-device identity: social recovery vs. backup seeds? (Both? User choice?)
- Economic modeling: what demurrage rate prevents hoarding without punishing savers?
- Governance: should templates be CCL contracts or Rust-level primitives?

**Strategic:**
- Should ICN target existing cooperatives or help form new ones?
- How much interoperability with legacy systems (email, banking) is necessary?
- What's the business model for ongoing ICN development? (Grant-funded? Cooperative membership dues? Service contracts?)

**Operational:**
- Who runs the pilot infrastructure? (Us? Community? Shared?)
- What's the handoff plan when pilot becomes production?
- How do we avoid becoming a single point of failure for the community?

---

## How to Use This Roadmap

**For contributors:**
- Pick a phase, read the scope, build it
- Update status as work progresses
- Add learnings to dev journal (`docs/dev-journal/`)

**For potential pilot communities:**
- Read Track C to understand what we're looking for
- Reach out if your community fits the criteria
- Expect a collaborative design process, not a finished product

**For the broader cooperative movement:**
- This is a living document, not a fixed plan
- Priorities will shift based on pilot learnings
- We're building infrastructure for a civilizational transition, not a product roadmap

---

## Strategic Assessment (2025-11-27)

### What We've Built

**Substrate Infrastructure** (Phases 1-18) ✅ **COMPLETE**:
- Three-layer security architecture (transport, message, application)
- Multi-device identity with key rotation and gossip sync
- Dynamic credit limits with new member protection
- Dispute resolution and write-off mechanisms
- Backup/restore, graceful restart, monitoring
- Economic validation via agent-based simulation
- Gateway API with JWT auth, rate limiting, WebSocket events
- Distributed compute with trust-gated task execution
- Intelligent scheduler with locality awareness and cooperative policies
- **Data replication with 99.9% durability** (Phase 17)
- **Byzantine fault detection and quarantine** (Phase 18)
- **Network partition healing with conflict resolution** (Phase 18)
- **Storage quotas with priority-based eviction** (Phase 18)

**All tests passing** ✅ (1134 tests across all crates)

### What We've Completed (Pre-Pilot Critical) ✅

**Data Durability** (Phase 17) ✅ **COMPLETE** (2025-11-24):
- ✅ Trust-weighted replica selection algorithm
- ✅ ReplicationManager actor with health monitoring
- ✅ Automatic re-replication on node failure
- ✅ 99.9% durability target achieved

**Byzantine Fault Tolerance** (Phase 18) ✅ **COMPLETE** (2025-11-27):
- ✅ Misbehavior detection and quarantine (Week 1-2)
- ✅ Network partition healing with conflict resolution (Week 3)
- ✅ Contract execution disputes (Week 4)
- ✅ Ledger fork resolution (Week 5)
- ✅ Storage quota management (Week 6)

**Pre-Pilot Infrastructure: COMPLETE** - ICN is ready for pilot deployment

See [ARCHITECTURE.md Section 12](docs/ARCHITECTURE.md#12-known-limitations--future-work) for complete gap analysis.

### What We're Missing (Social Layer)

**Usability & Integration**:
- Web/mobile clients (basic pilot UI exists, needs refinement)
- Onboarding workflows for non-technical users
- Visualization tools (trust graph, ledger browser, topology)
- Email/SMS notifications
- Export formats for accountants/treasurers

**Cooperative Workflows**:
- Guided cooperative setup (group creation → governance → ledger)
- Invitation flows, role management, consent mechanisms
- Social protocols (how humans coordinate, not just protocols)

See [Strategic Gap Analysis](docs/strategic-gap-analysis.md) for complete 15-gap assessment.

### The Path Forward

**Infrastructure complete. Now learn from communities.**

**Updated Timeline** (2025-11-27):
1. ✅ **Phase 17** (4 weeks): Storage hardening - **COMPLETE**
2. ✅ **Phase 18** (6 weeks): Byzantine fault tolerance - **COMPLETE**
3. ⏳ **Track C1** (2-4 weeks): Select pilot community - **IMMEDIATE PRIORITY**
4. ⏳ **Track C2** (4-6 weeks): Build MVP for pilot workflows
5. ⏳ **3-month pilot**: Deploy, learn, iterate
6. ⏳ **Phase 19** (4 weeks): Scalability improvements (post-pilot, as needed)

**Success Criteria (3-month pilot)**:
- 10+ active users logging hours/transactions weekly
- 3+ governance decisions made using ICN primitives
- No data loss events (99.9% durability validated)
- No successful Byzantine attacks (detection <3 messages)
- Community prefers ICN over their previous system
- 2-3 other communities express interest

**Next Actions**:
1. Select pilot community (timebank recommended) - **IMMEDIATE PRIORITY**
2. Build minimal MVP for pilot workflows
3. Deploy to pilot community
4. Run weekly learning loop
5. Let pilot needs drive Phase 19+ scope

**Philosophy**: Infrastructure is ready. Now we listen to communities and build what they need. The substrate has fault tolerance and security - time to prove it in the real world.

---

**Last Updated**: 2025-12-10 (Track S SDIS Complete - Post-quantum identity with steward network, ZKP credentials, and three-tier verification)
**Next Review**: After pilot community selection (Track C1) or pilot MVP completion (Track C2)
