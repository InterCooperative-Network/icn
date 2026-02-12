# ICN Kernel/App Separation Transition Plan

## Overview

Transform ICN from a tightly-coupled 27-crate system to a clean kernel/app architecture where:
- **Kernel** provides 8 generic primitives (Identity, Authorization, State, Compute, Communication, Time, Coordination, Naming)
- **Apps** implement all domain logic (trust, ledger, governance, membership) using the same APIs as third-party apps

**Breaking changes acceptable** - no production deployment exists.

**Core Principle**: We are not refactoring code. We are **deleting privilege**. Every direct trust import removed is removing implicit authority from kernel.

---

## Phase Ordering

| Phase | Description | Status |
|-------|-------------|--------|
| 0 | PolicyOracle infrastructure (generalized) | ✅ Complete |
| 1 | Minimal App Runtime skeleton | ✅ Complete |
| 1.5 | Echo test app (validates runtime) | ✅ Complete |
| 2 | Trust extraction (first real app) | ⏳ Planned |
| 3 | State generalization (formalize trait contracts) | ⏳ Planned |
| 4 | Governance extraction | ⏳ Planned |
| 5 | Membership consolidation (canonical Entity model) | ⏳ Planned |
| 6 | Naming primitive | ⏳ Planned |
| 7 | Crate consolidation (last) | ⏳ Planned |

**Critical ordering rationale**: Runtime MUST exist before extraction. Otherwise supervisor becomes the new god-object that hardcodes first-party apps.

---

## Phase 0: PolicyOracle Infrastructure ✅

**Status**: Complete

**Goal**: Generalize PolicyDecision beyond rate limiting. Split PolicyRequest for caching performance.

**Implementation**: `icn/crates/icn-kernel-api/src/authz.rs` and `icn/crates/icn-kernel-api/src/bootstrap.rs`

### Design

#### Generalized PolicyDecision

```rust
/// Result of a policy evaluation.
/// Kernel enforces these constraints WITHOUT understanding their semantic origin.
pub enum PolicyDecision {
    /// Request allowed with optional constraints
    Allow { constraints: ConstraintSet },
    /// Request denied
    Deny { reason: PolicyError },
}

/// Kernel-enforced constraints. Kernel does not know WHY these exist.
/// Apps (trust, governance, ledger) set these via PolicyOracle.
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ConstraintSet {
    /// Rate limiting parameters
    pub rate_limit: Option<RateLimit>,
    /// Maximum topics this actor can subscribe to
    pub max_topics: Option<u32>,
    /// Credit multiplier for ledger operations
    pub credit_multiplier: Option<f64>,
    /// Voting weight for governance
    pub voting_weight: Option<f64>,
    /// App-specific constraints (bounded, serializable)
    pub custom: HashMap<String, ConstraintValue>,
}

/// Bounded constraint value - NOT Box<dyn Any>
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConstraintValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<ConstraintValue>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimit {
    pub messages_per_second: u32,
    pub burst_size: u32,
}
```

#### Split PolicyRequest for Caching

```rust
/// Cacheable core - small, hashable, no allocations on hot path
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct PolicyRequestCore {
    pub actor: Did,
    pub action: ActionKind,
    pub domain: Domain,
}

/// Context metadata - inspected only on cache miss
pub struct PolicyContext {
    pub metadata: HashMap<String, String>,
    pub capability: Option<Capability>,
}

pub struct PolicyRequest {
    pub core: PolicyRequestCore,
    pub context: PolicyContext,
}

/// Domain identifier for oracle routing
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct Domain(pub String);

/// Action categories
#[derive(Clone, Hash, Eq, PartialEq)]
pub enum ActionKind {
    Read,
    Write,
    Subscribe,
    Publish,
    Execute,
    Custom(String),
}
```

#### Atomic Oracle Registry

```rust
pub struct OracleRegistry {
    oracles: ArcSwap<HashMap<Domain, Arc<dyn PolicyOracle>>>,
    cache: Cache<PolicyRequestCore, CachedDecision>,
    phase: AtomicU8, // BootstrapPhase as u8
}

struct CachedDecision {
    decision: PolicyDecision,
    expires_at: Instant,
}

impl OracleRegistry {
    /// Atomically replace oracle and invalidate domain cache
    pub fn replace(&self, domain: Domain, oracle: Arc<dyn PolicyOracle>) {
        let mut map = (**self.oracles.load()).clone();
        map.insert(domain.clone(), oracle);
        self.oracles.store(Arc::new(map));
        self.cache.invalidate_domain(&domain);
    }

    /// Evaluate with caching
    pub fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Check cache first (core only)
        if let Some(cached) = self.cache.get(&request.core) {
            if cached.expires_at > Instant::now() {
                return cached.decision.clone();
            }
        }

        // Cache miss - consult oracle
        let oracle = self.oracles.load()
            .get(&request.core.domain)
            .cloned()
            .unwrap_or_else(|| Arc::new(AllowAllOracle));

        let decision = oracle.evaluate(request);
        let ttl = oracle.cache_ttl();

        if !ttl.is_zero() {
            self.cache.insert(request.core.clone(), CachedDecision {
                decision: decision.clone(),
                expires_at: Instant::now() + ttl,
            });
        }

        decision
    }
}
```

#### Expiring Genesis Capabilities

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BootstrapPhase {
    Genesis = 0,    // Initial startup, AllowAllOracle
    CoreApps = 1,   // Loading first-party apps
    Running = 2,    // Normal operation
}

pub struct GenesisCapabilities {
    phase_guard: Arc<AtomicU8>,
    expires_at: Instant,
}

impl GenesisCapabilities {
    pub fn new(phase: Arc<AtomicU8>, ttl: Duration) -> Self {
        Self {
            phase_guard: phase,
            expires_at: Instant::now() + ttl,
        }
    }

    pub fn issue(&self, resource: &str, action: &str) -> Result<Capability, AuthzError> {
        let phase = self.phase_guard.load(Ordering::SeqCst);
        if phase != BootstrapPhase::Genesis as u8 {
            return Err(AuthzError::NotInGenesis);
        }
        if Instant::now() > self.expires_at {
            return Err(AuthzError::GenesisExpired);
        }
        // Issue capability...
    }
}
```

### Files to Modify

- `icn/crates/icn-kernel-api/src/authz.rs` - Generalize PolicyDecision, add ConstraintSet
- `icn/crates/icn-kernel-api/src/lib.rs` - Add bootstrap module
- `icn/crates/icn-kernel-api/src/bootstrap.rs` - New: OracleRegistry, BootstrapPhase, GenesisCapabilities

### Verification

- `cargo test -p icn-kernel-api` passes
- Benchmark: CachingOracle lookup < 1μs (core-only cache key)
- No domain-specific types in authz module

---

## Phase 1: Minimal App Runtime ✅

**Status**: Complete

**Goal**: Runtime that can load ONE app via manifest before extracting trust.

**Implementation**: `icn/crates/icn-core/src/apps/` module with:
- `manifest.rs` - YAML manifest parser with validation
- `state_factory.rs` - Namespaced state handles (KV, Log, Blob)
- `dispatcher.rs` - ComputeDispatcher with Reducer/Service split
- `runtime.rs` - AppRuntime for lifecycle management (install, start, stop, uninstall)

### Manifest Schema

```yaml
# apps/echo/manifest.yaml - Example
name: echo
version: 1.0.0
publisher: did:icn:test

capabilities_required:
  - state:logs:append:self      # Write to own namespace
  - state:kv:read:self          # Read own KV
  - state:kv:write:self         # Write own KV

capabilities_provided: []        # Echo provides no capabilities to others

state:
  logs:
    - name: events
      ordering: total
  kv:
    - name: cache

compute:
  reducers:
    - name: echo_reducer
      event_type: echo:event
  services:
    - name: echo_service
      request_type: echo:query
```

```yaml
# apps/trust/manifest.yaml - Real app
name: trust
version: 1.0.0
publisher: did:icn:icn-foundation

capabilities_required:
  - state:logs:append:self
  - state:kv:read:self
  - state:kv:write:self
  - comms:subscribe:trust:*

capabilities_provided:
  - oracle:trust:*              # Provides trust oracle to kernel

state:
  logs:
    - name: attestations
      ordering: causal
  kv:
    - name: scores
    - name: graph

compute:
  reducers:
    - name: attestation_reducer
      event_type: trust:attestation
  services:
    - name: score_query
      request_type: trust:query

oracle:
  domain: trust
  implementation: TrustOracle
```

### Compute Dispatcher with Event Routing

```rust
pub struct ComputeDispatcher {
    reducers: HashMap<EventType, Box<dyn ErasedReducer>>,
    services: HashMap<RequestType, Box<dyn ErasedService>>,
    event_tx: mpsc::Sender<Event>,
    event_rx: mpsc::Receiver<Event>,
}

impl ComputeDispatcher {
    /// Services emit events here
    pub fn event_sender(&self) -> mpsc::Sender<Event> {
        self.event_tx.clone()
    }

    /// Run event loop - routes events to reducers
    pub async fn run(&mut self, state: &mut AppState) {
        while let Some(event) = self.event_rx.recv().await {
            if let Some(reducer) = self.reducers.get(&event.event_type) {
                // Reducers run in restricted context:
                // - No async runtime access
                // - Receive &State, return new State
                // - No access to Comms, Time, IO
                let new_state = reducer.reduce(&state.snapshot(), &event);
                state.apply(new_state);
            }
        }
    }
}
```

### Reducer Purity Enforcement

Rust cannot enforce purity at compile time, but runtime enforces isolation:

```rust
/// Reducers: pure, deterministic, state-in → state-out
pub trait Reducer: Send + Sync {
    type State: Clone;
    type Action;

    /// MUST be deterministic. Same state + action = same result.
    /// Receives immutable state reference, returns new state.
    fn reduce(&self, state: &Self::State, action: &Self::Action) -> Self::State;
}

/// Type-erased reducer for dispatcher
pub trait ErasedReducer: Send + Sync {
    fn reduce(&self, state: &StateSnapshot, event: &Event) -> StateSnapshot;
}

/// Services: may have side effects, cannot mutate state directly
pub trait Service: Send + Sync {
    type Request;
    type Response;
    type Event;

    /// May call external systems. MUST NOT mutate state directly.
    /// Returns response + events that reducers consume.
    async fn handle(
        &self,
        req: Self::Request,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<Self::Response>;
}
```

### Capability Granting from Manifest

```rust
impl AppRuntime {
    pub fn install(
        &mut self,
        manifest: Manifest,
        installer_caps: &CapabilitySet,  // What the installer can grant
    ) -> Result<AppHandle> {
        // 1. Validate: app cannot request more than installer holds
        for cap_req in &manifest.capabilities_required {
            if !installer_caps.can_delegate(cap_req) {
                return Err(RuntimeError::InsufficientPrivilege(cap_req.clone()));
            }
        }

        // 2. Create namespaced state handles
        let namespace = Namespace::new(&manifest.publisher, &manifest.name);
        let state = self.state_factory.create_for_namespace(&namespace, &manifest.state)?;

        // 3. Register reducers/services
        let dispatcher = self.create_dispatcher(&manifest.compute)?;

        // 4. Issue scoped capabilities
        let caps = self.capability_engine.issue_scoped(
            &manifest.capabilities_required,
            &namespace,
        )?;

        // 5. Register oracle if app provides one
        if let Some(oracle_def) = &manifest.oracle {
            // App must provide oracle implementation
            // Runtime wires it to OracleRegistry
        }

        Ok(AppHandle { namespace, state, dispatcher, caps })
    }
}
```

### Supervisor Constraint

```rust
// icn-core/src/supervisor/mod.rs - THE RULE

impl Supervisor {
    pub fn init(&mut self, config: &Config) -> Result<()> {
        // Phase 1: Genesis - AllowAllOracle active
        self.runtime.set_phase(BootstrapPhase::Genesis);

        // Load manifests from config paths
        // Supervisor has ROOT capabilities during genesis
        let genesis_caps = self.runtime.genesis_capabilities();

        for manifest_path in &config.apps {
            let manifest = Manifest::load(manifest_path)?;
            self.runtime.install(manifest, &genesis_caps)?;
        }

        // Phase 2: CoreApps loaded
        self.runtime.set_phase(BootstrapPhase::CoreApps);

        // Start all apps
        for app in self.runtime.apps() {
            self.runtime.start(&app)?;
        }

        // Phase 3: Running - genesis capabilities expired
        self.runtime.set_phase(BootstrapPhase::Running);

        Ok(())

        // ============================================
        // NO DOMAIN KNOWLEDGE BELOW THIS LINE
        // Supervisor does not know "trust" or "governance"
        // It just loads what config tells it to load
        // ============================================
    }
}
```

### App Isolation (Current + Future)

**Current (namespace isolation)**:
- Each app gets isolated state namespace
- Apps cannot read/write other apps' state directly
- Communication only via Comms primitive (pub/sub, request/response)

**Future (WASM sandbox)**:
- Resource quotas (CPU, memory, storage per app)
- No direct inter-app memory access
- Metered execution

Runtime interface is designed to support WASM sandbox later without API changes.

### Files to Create

- `icn/crates/icn-core/src/runtime/mod.rs` - AppRuntime
- `icn/crates/icn-core/src/runtime/manifest.rs` - Manifest parser
- `icn/crates/icn-core/src/runtime/dispatcher.rs` - ComputeDispatcher
- `icn/crates/icn-core/src/runtime/state_factory.rs` - Namespaced state creation
- `icn/crates/icn-core/src/runtime/capabilities.rs` - Capability granting

### Verification

- Can install echo app from manifest
- Supervisor has zero domain-specific code
- Events flow: Service → event_tx → Dispatcher → Reducer → State

---

## Phase 1.5: Echo Test App ✅

**Status**: Complete

**Goal**: Validate runtime works before extracting trust.

**Implementation**: `apps/echo/` with:
- `manifest.yaml` - App manifest declaring KV state and handlers
- `src/lib.rs` - EchoReducer (pure state updates) and EchoService (async queries)
- Integration test validating full lifecycle: prepare → install → start → dispatch → query → stop → uninstall

### Structure

```
apps/
  echo/
    Cargo.toml
    manifest.yaml
    src/
      lib.rs
      reducer.rs    # Echoes events to state
      service.rs    # Responds to queries
```

### Implementation

```rust
// apps/echo/src/reducer.rs
pub struct EchoReducer;

impl Reducer for EchoReducer {
    type State = EchoState;
    type Action = EchoEvent;

    fn reduce(&self, state: &Self::State, action: &Self::Action) -> Self::State {
        let mut new_state = state.clone();
        new_state.events.push(action.clone());
        new_state.count += 1;
        new_state
    }
}

// apps/echo/src/service.rs
pub struct EchoService;

impl Service for EchoService {
    type Request = EchoQuery;
    type Response = EchoResponse;
    type Event = EchoEvent;

    async fn handle(
        &self,
        req: Self::Request,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<Self::Response> {
        // Emit event for reducer
        event_tx.send(Event::new("echo:event", EchoEvent {
            message: req.message.clone(),
            timestamp: Instant::now(),
        })).await?;

        // Return response
        Ok(EchoResponse {
            echoed: req.message,
        })
    }
}
```

### Verification

- `cargo test -p echo` passes
- Echo app installs via runtime
- Service receives query → emits event → reducer updates state
- State is queryable

---

## Phase 2: Trust Extraction

**Goal**: Replace all direct TrustGraph/TrustClass usage with PolicyOracle calls.

### Infection Points

| File | Lines | Issue |
|------|-------|-------|
| `icn-net/src/rate_limit.rs` | 14, 302, 383-446 | Direct TrustGraph, hardcoded thresholds |
| `icn-gateway/src/rate_limit.rs` | 429-493, 469-479 | Duplicate trust mapping |
| `icn-gossip/src/gossip.rs` | 9, 46, 172-183 | THREE trust integration paths |
| `icn-ledger/src/credit_policy.rs` | 12, 89-111 | Direct TrustGraph calls |

### Order of Changes

1. **Create Trust App using runtime**
   ```
   apps/trust/
     Cargo.toml
     manifest.yaml
     src/
       lib.rs
       oracle.rs      # PolicyOracle implementation
       graph.rs       # Move from icn-trust
       reducer.rs     # Attestation processing
       service.rs     # Score queries
   ```

2. **Fix icn-net** (simplest infection)
   - Remove `use icn_trust::TrustClass`
   - Replace `trust_graph: Option<Arc<RwLock<TrustGraph>>>` with `oracle: Arc<OracleRegistry>`
   - Replace trust lookups with `oracle.evaluate()`
   - Use `ConstraintSet.rate_limit` from decision

3. **Fix icn-gateway** (same pattern)

4. **Fix icn-gossip** - Remove all three trust paths, single oracle reference

5. **Fix icn-ledger** - `credit_policy.rs`: Use `ConstraintSet.credit_multiplier`

6. **Update supervisor** - Load trust app via runtime, not direct wiring

### Trust Oracle Implementation

```rust
// apps/trust/src/oracle.rs
pub struct TrustOracle {
    graph: Arc<RwLock<TrustGraph>>,
}

impl PolicyOracle for TrustOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let score = self.graph.read().compute_trust_score(&request.core.actor);

        let constraints = ConstraintSet {
            rate_limit: Some(score_to_rate_limit(score)),
            max_topics: Some(score_to_max_topics(score)),
            credit_multiplier: Some(score_to_credit_mult(score)),
            voting_weight: Some(score), // Trust score IS voting weight
            ..Default::default()
        };

        PolicyDecision::Allow { constraints }
    }

    fn cache_ttl(&self) -> Duration {
        Duration::from_secs(60)
    }
}

fn score_to_rate_limit(score: f64) -> RateLimit {
    // Kernel does not know these thresholds exist
    // This is app logic
    match score {
        s if s >= 0.7 => RateLimit { messages_per_second: 200, burst_size: 50 },
        s if s >= 0.4 => RateLimit { messages_per_second: 100, burst_size: 25 },
        s if s >= 0.1 => RateLimit { messages_per_second: 50, burst_size: 10 },
        _ => RateLimit { messages_per_second: 10, burst_size: 5 },
    }
}
```

### Verification

- `cargo check --workspace` - no TrustGraph/TrustClass imports in kernel crates
- `cargo test -p icn-net -p icn-gossip -p icn-ledger` passes
- Trust app loads via runtime manifest
- Supervisor contains zero trust-specific code

---

## Phase 3: State Generalization

**Goal**: Formalize state trait contracts. Move domain logic out of icn-store.

### Formalized Trait Contracts

```rust
/// Ordering guarantee for logs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ordering {
    /// Strict total ordering - entries have global sequence
    Total,
    /// Causal ordering - happens-before relationships preserved
    Causal,
    /// Concurrent - CRDTs resolve conflicts, no ordering guarantee
    Concurrent,
}

pub trait LogService: Send + Sync {
    /// Create a new log with specified ordering
    fn create_log(&self, namespace: &Namespace, name: &str, ordering: Ordering) -> Result<LogId>;

    /// Get ordering guarantee for a log
    fn ordering(&self, log: &LogId) -> Result<Ordering>;

    /// Append with optional idempotency key
    /// If idempotency_key matches previous append, this is a no-op
    fn append(
        &self,
        log: &LogId,
        entry: &[u8],
        idempotency_key: Option<&str>,
    ) -> Result<Offset>;

    /// Read entries in order
    fn read(&self, log: &LogId, from: Offset, limit: usize) -> Result<Vec<LogEntry>>;

    /// Get consistent point-in-time snapshot
    fn snapshot(&self, log: &LogId) -> Result<LogSnapshot>;
}

pub struct LogSnapshot {
    pub log_id: LogId,
    pub offset: Offset,
    pub timestamp: LogicalTimestamp,
}
```

### Files to Move

| From | To |
|------|-----|
| `icn-store/src/escrow.rs` | `apps/ledger/src/escrow.rs` |
| `icn-store/src/budgets.rs` | `apps/ledger/src/budgets.rs` |
| `icn-store/src/recurring_payments.rs` | `apps/ledger/src/recurring.rs` |

### icn-store Final Structure

```
icn-store/src/
  lib.rs          # Only generic exports
  kv.rs           # KvService implementation
  log.rs          # LogService implementation
  blob.rs         # BlobService implementation
  namespace.rs    # Namespace isolation
```

### Verification

- icn-store has no domain-specific types
- `cargo test -p icn-store` passes
- Ordering is per-log, not per-service

---

## Phase 4: Governance Extraction

**Goal**: Move governance from kernel to app. Remove WitnessPolicy from supervisor.

### Files to Remove from icn-core

- `icn-core/src/governance/` (entire directory if exists)
- Any governance-specific handler code in supervisor

### Create Governance App

```
apps/governance/
  Cargo.toml
  manifest.yaml
  src/
    lib.rs
    oracle.rs       # Voting weight via PolicyOracle
    proposal.rs     # From icn-governance
    voting.rs
    charter.rs
    reducer.rs      # Vote processing
    service.rs      # Proposal queries
```

### WitnessPolicy Migration

WitnessPolicy must become:
1. A governance rule stored in app state
2. Interpreted by runtime via PolicyOracle or Coordination primitive

```rust
// apps/governance/src/oracle.rs
impl PolicyOracle for GovernanceOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        if request.core.action == ActionKind::Custom("witness".into()) {
            let witness_policy = self.load_witness_policy(&request.core.domain);
            let constraints = ConstraintSet {
                custom: hashmap! {
                    "required_witnesses".into() => ConstraintValue::Int(witness_policy.min_witnesses as i64),
                    "witness_threshold".into() => ConstraintValue::Float(witness_policy.threshold),
                },
                ..Default::default()
            };
            return PolicyDecision::Allow { constraints };
        }
        // ...
    }
}
```

### Verification

- Supervisor contains no governance branching
- `cargo check --workspace` - no governance imports in kernel
- Governance app uses PolicyOracle for all decisions

---

## Phase 5: Membership Consolidation

**Goal**: Merge icn-entity, icn-coop, icn-community into single app with canonical Entity model.

### Canonical Entity Abstraction

```rust
/// Unique entity identifier
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct EntityId(pub String);

/// Entity kinds
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKind {
    Individual,
    Cooperative,
    Federation,
    Community,
}

/// Core entity trait
pub trait Entity {
    fn id(&self) -> &EntityId;
    fn kind(&self) -> EntityKind;
    fn did(&self) -> &Did;
}

/// Generic membership - works for any entity type
pub struct Membership<E: Entity> {
    pub entity: EntityId,
    pub member: EntityId,
    pub role: MembershipRole,
    pub joined_at: LogicalTimestamp,
    pub _phantom: PhantomData<E>,
}
```

### Create Membership App

```
apps/membership/
  Cargo.toml
  manifest.yaml
  src/
    lib.rs
    entity.rs       # Unified EntityId model
    membership.rs   # Generic Membership<E>
    coop.rs         # Cooperative-specific config
    community.rs    # Community-specific config
    federation.rs   # Federation-specific config
    reducer.rs
    service.rs
```

### Verification

- Single canonical Entity model
- Old crates (icn-entity, icn-coop, icn-community) deprecated
- No parallel membership models

---

## Phase 6: Naming Primitive

**Goal**: Implement naming/discovery for app-to-app communication.

### Design Constraints

- Naming depends ONLY on State + Identity
- Do NOT entangle with gossip/protocol layer
- This is capability advertisement + dependency resolution

### Implementation

```rust
pub trait NamingService: Send + Sync {
    /// Register a name with authority signature
    fn register(
        &self,
        name: &Name,
        record: NameRecord,
        authority_sig: &Signature,
    ) -> Result<()>;

    /// Resolve a name to its record
    fn resolve(&self, name: &Name) -> Result<Option<NameRecord>>;

    /// List names matching pattern
    fn list(&self, pattern: &str) -> Result<Vec<Name>>;
}

pub struct NameRecord {
    pub name: Name,
    pub kind: NameKind,
    pub endpoints: Vec<Endpoint>,
    pub capabilities: Vec<CapabilityId>,  // What this service provides
    pub dependencies: Vec<Name>,           // What this service needs
    pub metadata: HashMap<String, String>,
    pub expires_at: LogicalTimestamp,
}

pub enum NameKind {
    App,
    Service,
    Oracle,
    Federation,
}
```

### Files to Create

```
icn-core/src/naming/
  mod.rs
  service.rs        # NamingService impl
  discovery.rs      # Discovery impl (uses NamingService)
  records.rs        # NameRecord storage
```

### Verification

- Naming uses only State + Identity primitives
- No gossip dependencies
- Apps can discover each other by name

---

## Phase 7: Crate Consolidation

**Goal**: Reduce kernel crates from 17+ to ~12. Do this LAST.

### Merge Plan

| Target | Sources |
|--------|---------|
| `icn-protocol` | icn-gossip + icn-net |
| `icn-services` | icn-api + icn-rpc + icn-gateway |
| `icn-crypto` | icn-crypto-pq + crypto from icn-identity |

### Final Kernel Structure (12 crates)

```
icn-kernel-api/   # Trait definitions
icn-identity/     # DID + keystore
icn-store/        # Generic storage
icn-protocol/     # Gossip + networking
icn-core/         # Runtime + supervisor
icn-services/     # API surfaces
icn-security/     # Security primitives
icn-crypto/       # Cryptography
icn-obs/          # Observability
icn-encoding/     # Serialization
icn-time/         # Time primitives
icn-testkit/      # Test utilities
```

### Rules

- Do NOT consolidate during extraction phases
- Complete all extractions first
- Use `pub use` re-exports for migration
- Document all path changes in CHANGELOG

---

## Critical Files Summary

1. **`icn-kernel-api/src/authz.rs`** - Generalize PolicyDecision
2. **`icn-kernel-api/src/bootstrap.rs`** - OracleRegistry, BootstrapPhase
3. **`icn-core/src/runtime/mod.rs`** - AppRuntime
4. **`icn-core/src/runtime/manifest.rs`** - Manifest parser
5. **`icn-core/src/runtime/dispatcher.rs`** - ComputeDispatcher
6. **`icn-net/src/rate_limit.rs`** - First trust extraction (1309 lines)
7. **`icn-gossip/src/gossip.rs`** - Three trust paths → one oracle
8. **`icn-ledger/src/credit_policy.rs`** - TrustGraph → PolicyOracle
9. **`icn-core/src/supervisor/mod.rs`** - Wire runtime, no domain knowledge

---

## Verification Strategy

### Per-Phase Checks

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

### Integration Tests

- Echo app lifecycle (install, start, query, stop)
- Multi-node convergence with trust app
- Cross-app communication via naming
- Capability delegation chain

### Success Criteria

- No domain logic in kernel crates
- All apps use same kernel APIs
- Supervisor has zero domain-specific code
- "Meaning firewall" test passes: search for domain terms in kernel crates

```bash
# Must return zero results
rg "TrustGraph|TrustClass|credit_limit|voting_weight|membership" \
   icn/crates/icn-{kernel-api,core,store,net,gossip,security,crypto,obs,encoding,time,testkit}
```

---

## Timeline

| Phase | Estimate |
|-------|----------|
| 0: PolicyOracle generalization | 2-3 days |
| 1: Minimal App Runtime | 4-5 days |
| 1.5: Echo test app | 1 day |
| 2: Trust extraction | 5-7 days |
| 3: State generalization | 3-4 days |
| 4: Governance extraction | 3-4 days |
| 5: Membership consolidation | 4-5 days |
| 6: Naming primitive | 3-4 days |
| 7: Crate consolidation | 3-4 days |

**Total: ~30-40 days**

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Supervisor becomes god-object | Runtime exists BEFORE extraction |
| PolicyDecision too trust-shaped | ConstraintSet is generic from start |
| Genesis capabilities permanent | Expiring + phase-guarded |
| Reducer purity not enforced | Runtime isolation, no async access |
| State contracts implicit | Formalize Ordering, Idempotency in traits |
| Crate consolidation during extraction | Consolidation is LAST phase |

---

## The Meaning Firewall

Before adding ANY code to kernel crates, ask:

1. Does this interpret domain semantics? → Must be an app
2. Does this hardcode a schema? → Must be an app
3. Does this privilege a specific application? → Must be an app
4. Does this understand "trust" or "governance"? → Must be an app

The kernel is deliberately dumb. It provides pipes, not policies.

**We are not refactoring code. We are deleting privilege.**
