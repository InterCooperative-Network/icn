# ICN Kernel/App Separation Architecture

> **Status**: In Progress
> **Tracking Issue**: [#856](https://github.com/InterCooperative-Network/icn/issues/856)
> **Last Updated**: 2026-01-26

## Overview

Transform ICN from a tightly-coupled 27-crate system to a clean kernel/app architecture where:

- **Kernel** provides 8 generic primitives (Identity, Authorization, State, Compute, Communication, Time, Coordination, Naming)
- **Apps** implement all domain logic (trust, ledger, governance, membership) using the same APIs as third-party apps

**Breaking changes acceptable** - no production deployment exists.

## The Meaning Firewall

The core architectural principle: **the kernel never understands domain semantics**.

```
CCL Document (constitution / bylaws / treaty)
         ↓
App Interpreter (Governance / Membership / Ledger / Federation)
         ↓
PolicyDecision { constraints }
         ↓
Kernel enforces constraints mechanically
```

The kernel never sees "constitutional amendment" or "supermajority" — only:
- `min_votes = 67`
- `quorum_required = 50`
- `deadline = timestamp`

This separation ensures:
1. Apps can implement any governance model
2. Kernel remains simple and auditable
3. Third-party apps have the same capabilities as first-party apps

## Phase Status

### ✅ Phase 0: PolicyOracle Infrastructure (Complete)

**PR**: [#855](https://github.com/InterCooperative-Network/icn/pull/855)

Added to `icn-kernel-api`:

```rust
// Core authorization trait - apps implement this
pub trait PolicyOracle: Send + Sync {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;
    fn domain(&self) -> Domain;
}

// Registry for swapping oracles atomically
pub struct OracleRegistry {
    oracles: ArcSwap<HashMap<Domain, Arc<dyn PolicyOracle>>>,
    fallback: ArcSwap<Arc<dyn PolicyOracle>>,
    cache: DecisionCache,
    phase: Arc<AtomicU8>,
}

// Bootstrap phases with security guarantees
pub enum BootstrapPhase {
    Genesis,   // AllowAllOracle active, genesis caps available
    CoreApps,  // First-party apps loading
    Running,   // Deny-by-default, all auth through oracles
}
```

Key features:
- Atomic oracle replacement via ArcSwap (no torn reads)
- TTL-based caching with automatic invalidation on oracle swap
- Deny-by-default in Running phase
- Time-limited genesis capabilities that expire after bootstrap

### ✅ Phase 1: App Runtime (Complete)

**PR**: [#855](https://github.com/InterCooperative-Network/icn/pull/855)

Added to `icn-core/src/apps/`:

```rust
// App lifecycle management
pub struct AppRuntime {
    apps: RwLock<HashMap<AppId, AppInstance>>,
    oracle_registry: Arc<OracleRegistry>,
    state_factory: StateFactory,
    genesis_caps: Option<Arc<GenesisCapabilities>>,
}

// Lifecycle: prepare → install → start → stop → uninstall
impl AppRuntime {
    pub async fn prepare(&self, manifest_path: &Path) -> Result<AppBuilder>;
    pub async fn install(&self, builder: AppBuilder, caps: &CapabilitySet) -> Result<AppId>;
    pub async fn start(&self, app_id: &AppId) -> Result<()>;
    pub async fn stop(&self, app_id: &AppId) -> Result<()>;
    pub async fn uninstall(&self, app_id: &AppId) -> Result<()>;
}

// Event/request dispatch with Reducer/Service split
pub struct ComputeDispatcher {
    reducers: HashMap<EventType, BoxedReducer>,  // Pure, synchronous
    services: HashMap<RequestType, BoxedService>, // Async, I/O allowed
}
```

Key features:
- YAML manifest parsing for app configuration
- Scoped capability granting
- Namespace isolation per app
- Reducer (pure) / Service (async) split enforces determinism

### ✅ Phase 1.5: CCL Schema Layer (Complete)

**PR**: [#855](https://github.com/InterCooperative-Network/icn/pull/855)

Added to `icn-ccl/src/schema/`:

```yaml
# Entity definition (cooperative, community, federation)
entity:
  name: "Abundance Food Coop"
  type: cooperative
  subtype: consumer
  membership:
    classes:
      - name: consumer_owner
        criteria:
          all:
            - { field: "equity_paid", op: ">=", value: 100 }
    rights_by_class:
      consumer_owner: [vote, patronage_refund]

# Governance definition
governance:
  bodies:
    - name: general_assembly
      composition: all_members
    - name: board
      seats: 7
      elected_by: general_assembly
  decisions:
    - name: constitutional
      authority: general_assembly
      threshold: { fraction: "2/3" }
      quorum: "0.5 * members"

# Economics definition
economics:
  surplus:
    allocation:
      - { target: reserves, fraction: "0.20" }
      - { target: patronage_refund, fraction: "0.60" }
  credit:
    limit: "min(1000, patronage * 0.5 * trust_score)"

# Federation agreement
agreement:
  parties:
    - { type: federation, id: "did:icn:finger-lakes" }
    - { type: federation, id: "did:icn:rochester" }
  boundary:
    joint_decisions:
      boundary_outcome:
        type: binary  # Internal process sovereign, outcome interoperable
```

Key features:
- Declarative YAML schemas for all entity types
- Deterministic expression evaluator (no loops, bounded depth)
- Schema versioning for migrations
- Federation agreements with binary boundary outcomes

### 🚧 Phase 2: Trust Extraction (In Progress)

**Issue**: [#857](https://github.com/InterCooperative-Network/icn/issues/857)

**Goal**: Replace all direct TrustGraph/TrustClass usage with PolicyOracle calls.

Infection points to fix:

| File | Issue |
|------|-------|
| `icn-net/src/rate_limit.rs` | Direct TrustGraph, hardcoded thresholds |
| `icn-gateway/src/rate_limit.rs` | Duplicate trust mapping |
| `icn-gossip/src/gossip.rs` | THREE trust integration paths |
| `icn-ledger/src/credit_policy.rs` | Direct TrustGraph calls |

Create trust app:
```
apps/trust/
  src/lib.rs
  src/oracle.rs      # PolicyOracle implementation
  src/graph.rs       # Move from icn-trust
  manifest.yaml
```

### ⏳ Phase 3: State Generalization

**Issue**: [#858](https://github.com/InterCooperative-Network/icn/issues/858)

Move domain logic out of icn-store, keep only generic primitives (KV, Log, Blob).

### ⏳ Phase 4: Governance Extraction

**Issue**: [#859](https://github.com/InterCooperative-Network/icn/issues/859)

Move governance from kernel to app. First real CCL consumer.

### ⏳ Phase 5: Membership Consolidation

**Issue**: [#860](https://github.com/InterCooperative-Network/icn/issues/860)

Merge icn-entity, icn-coop, icn-community into single membership app.

### ⏳ Phase 6: Crate Consolidation

**Issue**: [#861](https://github.com/InterCooperative-Network/icn/issues/861)

Reduce kernel crates from 17+ to ~12.

### ⏳ Phase 7: Naming Primitive

**Issue**: [#862](https://github.com/InterCooperative-Network/icn/issues/862)

Implement naming/discovery for app-to-app communication.

## Final Architecture

### Kernel Crates (~12)

```
icn-kernel-api/   # Trait definitions (PolicyOracle, primitives)
icn-identity/     # DID + keystore
icn-store/        # Generic storage (KV, Log, Blob)
icn-protocol/     # Gossip + networking
icn-core/         # Runtime + supervisor + app management
icn-services/     # API surfaces (RPC, Gateway)
icn-security/     # Security primitives
icn-crypto/       # Cryptography
icn-obs/          # Observability
icn-encoding/     # Serialization
icn-time/         # Time primitives
icn-testkit/      # Test utilities
```

### First-Party Apps

```
apps/
  trust/          # Trust graph, attestations, PolicyOracle
  ledger/         # Mutual credit, escrow, budgets
  governance/     # Proposals, voting, CCL-driven rules
  membership/     # Entity management, membership classes
  echo/           # Test app (already implemented)
```

### 8 Kernel Primitives

| Primitive | Trait | Purpose |
|-----------|-------|---------|
| Identity | `IdentityService` | DID management, key operations |
| Authorization | `PolicyOracle` | Capability-based access control |
| State | `KvService`, `LogService`, `BlobService` | Persistent storage |
| Compute | `ComputeDispatcher` | Event/request routing |
| Communication | `CommsService` | Pub/sub messaging |
| Time | `TimeService` | Clocks, timers |
| Coordination | `CoordService` | Consensus, CRDTs |
| Naming | `NamingService` | Name resolution, discovery |

## Key Design Decisions

### 1. Binary Boundary Outcomes

Federation agreements use binary outcomes for joint decisions:

```yaml
boundary_outcome:
  type: binary  # approved | rejected
```

Federation A uses consensus internally. Federation B uses majority vote. The agreement doesn't care — it only asks "Did each party approve?"

**Internal process is sovereign. Boundary outcomes are interoperable.**

### 2. Deny-by-Default in Running Phase

During Genesis/CoreApps, `AllowAllOracle` permits bootstrap operations. Once in Running phase, all authorization goes through registered PolicyOracles. Missing oracle = denial.

### 3. Genesis Capabilities are Time-Limited

Genesis capabilities expire after a configured TTL (default: 60 seconds). They cannot become a permanent backdoor.

### 4. Reducer/Service Split

- **Reducers**: Pure, synchronous, deterministic. State transitions only.
- **Services**: Async, can do I/O. Handle requests that need external data.

This split enforces determinism at the type level.

## Verification Strategy

### Per-Phase Checks

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

### Integration Tests

- Multi-node convergence with trust app
- App lifecycle (install, start, stop)
- Cross-app communication via naming

### Success Criteria

1. No domain logic in kernel crates
2. All apps use same kernel APIs
3. "Meaning firewall" test passes for all PRs

## Timeline

| Phase | Status | Issue |
|-------|--------|-------|
| 0: PolicyOracle Infrastructure | ✅ Complete | PR #855 |
| 1: App Runtime | ✅ Complete | PR #855 |
| 1.5: CCL Schema Layer | ✅ Complete | PR #855 |
| 2: Trust Extraction | 🚧 In Progress | #857 |
| 3: State Generalization | ⏳ Planned | #858 |
| 4: Governance Extraction | ⏳ Planned | #859 |
| 5: Membership Consolidation | ⏳ Planned | #860 |
| 6: Crate Consolidation | ⏳ Planned | #861 |
| 7: Naming Primitive | ⏳ Planned | #862 |

**Estimated remaining: ~25-30 days of focused work**

## Related Documents

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture overview
- [PHASE_HISTORY.md](PHASE_HISTORY.md) - Completed development phases
- [icn-kernel-api README](../icn/crates/icn-kernel-api/README.md) - Kernel API documentation
