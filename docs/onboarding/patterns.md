# ICN Common Patterns Reference

This guide documents recurring patterns used throughout the ICN codebase. Use
it as a quick reference when reading or writing code.

Each pattern includes:
- **Template code** showing the pattern
- **Where to find examples** in the codebase
- **Introduced in** which path layer introduces the pattern
- **Exercised by** which lab practices the pattern
- **Failure mode** what breaks if the pattern is misused

---

## Pattern 1: Actor

ICN uses an actor model for concurrent subsystems. Each actor encapsulates
state and exposes a handle for interaction.

### Structure
```rust
// The actor struct holds state
pub struct MyActor {
    state: InternalState,
    // ...
}

// The handle provides concurrent access
pub struct MyActorHandle {
    inner: Arc<RwLock<MyActor>>,
}

impl MyActorHandle {
    pub async fn do_something(&self) -> Result<()> {
        let actor = self.inner.write().await;
        actor.internal_operation()
    }
}
```

### Where to find examples
- `icn/crates/icn-gossip/src/gossip.rs`: GossipActor
- `icn/crates/icn-net/src/actor/mod.rs`: NetworkActor
- `icn/crates/icn-ledger/src/ledger.rs`: Ledger

### Introduced in
`path/phase-2-architecture/04-actors-and-concurrency.md` (Phase 2, Layer 04)

### Exercised by
`labs/lab-03-mini-actor/` — Build actor with bounded mailbox and sequential message processing

### Failure mode
**Shared mutable state without locks**: If multiple threads access actor state directly (bypassing the handle), you get data races. The actor pattern prevents this by encapsulating state and forcing message-based communication.

---

## Pattern 2: Error Handling

Use thiserror for domain-specific errors, anyhow for error propagation at boundaries.

### Domain errors with thiserror
```rust
#[derive(Debug, thiserror::Error)]
pub enum MyError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid state: {reason}")]
    InvalidState { reason: String },
}
```

### Propagation with anyhow
```rust
use anyhow::{Context, Result};

fn load_config(path: &Path) -> Result<Config> {
    let data = std::fs::read_to_string(path)
        .context("Failed to read config file")?;
    let config: Config = toml::from_str(&data)
        .context("Failed to parse config")?;
    Ok(config)
}
```

### Where to find examples
- `icn/crates/icn-federation/src/error.rs`: Domain errors
- `icn/crates/icn-core/src/runtime.rs`: Error propagation

## Async Callbacks for Inter-Actor Communication

Actors communicate via callbacks instead of direct dependencies.

### Pattern
```rust
pub type SendCallback = Arc<dyn Fn(&str, Vec<u8>) -> Result<()> + Send + Sync>;

impl MyActor {
    pub fn set_send_callback(&mut self, callback: SendCallback) {
        self.send_callback = Some(callback);
    }

    fn send_message(&self, topic: &str, data: Vec<u8>) -> Result<()> {
        if let Some(ref cb) = self.send_callback {
            cb(topic, data)?;
        }
        Ok(())
    }
}
```

### Where to find examples
- `icn/crates/icn-gossip/src/gossip.rs`: Send callbacks
- `icn/crates/icn-federation/src/gossip.rs`: GossipSendCallback

## Store Abstraction for Persistence

Storage is abstracted behind traits, allowing different backends.

### Pattern
```rust
pub trait Store: Send + Sync {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn delete(&self, key: &[u8]) -> Result<()>;
}

// Concrete implementation
pub struct SledStore {
    db: sled::Db,
}
```

### Namespaced storage
```rust
const KEY_PREFIX: &[u8] = b"agreement:";

fn make_key(id: &str) -> Vec<u8> {
    let mut key = KEY_PREFIX.to_vec();
    key.extend(id.as_bytes());
    key
}
```

### Where to find examples
- `icn-store/src/lib.rs`: Store trait
- `icn/crates/icn-federation/src/agreement/store.rs`: Namespaced storage

## Builder Pattern for Complex Configuration

### Pattern
```rust
pub struct ConfigBuilder {
    timeout: Option<Duration>,
    max_retries: Option<u32>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self { timeout: None, max_retries: None }
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = Some(n);
        self
    }

    pub fn build(self) -> Config {
        Config {
            timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
            max_retries: self.max_retries.unwrap_or(3),
        }
    }
}
```

### Where to find examples
- `icn/crates/icn-obs/src/otel.rs`: TracingConfig
- `icn/crates/icn-core/src/config/`: Configuration builders

## Lifecycle State Machines

Many components have explicit lifecycle states.

### Pattern
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Draft,
    Pending,
    Active,
    Terminated,
}

impl MyEntity {
    pub fn transition_to(&mut self, new_status: Status) -> Result<()> {
        match (&self.status, &new_status) {
            (Status::Draft, Status::Pending) => {}
            (Status::Pending, Status::Active) => {}
            (Status::Active, Status::Terminated) => {}
            _ => return Err(Error::InvalidTransition),
        }
        self.status = new_status;
        Ok(())
    }
}
```

### Where to find examples
- `icn/crates/icn-federation/src/agreement/types.rs`: AgreementStatus
- `icn/crates/icn-governance/src/proposal.rs`: ProposalStatus

## Metrics Integration

Metrics follow a consistent naming and registration pattern.

### Pattern
```rust
use metrics::{counter, gauge, histogram};

pub fn init_descriptions() {
    metrics::describe_counter!(
        "icn_my_counter_total",
        "Description of what this counter measures"
    );
}

pub fn my_counter_inc() {
    counter!("icn_my_counter_total").increment(1);
}

pub fn my_gauge_set(value: f64) {
    gauge!("icn_my_gauge").set(value);
}

pub fn my_histogram_observe(value: f64) {
    histogram!("icn_my_duration_seconds").record(value);
}
```

### Naming convention
- `icn_{subsystem}_{metric}_{unit}`
- Counters end with `_total`
- Histograms for durations end with `_seconds`

### Where to find examples
- `icn-obs/src/metrics/`: All metric modules
- `icn/crates/icn-federation/src/metrics.rs`: Federation metrics

## Trust-Gated Operations

Operations may be gated based on trust scores.

### Pattern
```rust
pub enum TrustClass {
    Isolated,   // < 0.1
    Known,      // 0.1 - 0.4
    Partner,    // 0.4 - 0.7
    Federated,  // > 0.7
}

impl TrustClass {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s < 0.1 => Self::Isolated,
            s if s < 0.4 => Self::Known,
            s if s < 0.7 => Self::Partner,
            _ => Self::Federated,
        }
    }

    pub fn rate_limit(&self) -> u32 {
        match self {
            Self::Isolated => 10,
            Self::Known => 50,
            Self::Partner => 100,
            Self::Federated => 200,
        }
    }
}
```

### Where to find examples
- `icn-security/src/rate_limit.rs`: Trust-based rate limiting
- `icn-gossip/src/access.rs`: Topic access control

## Shutdown Coordination

Graceful shutdown uses broadcast channels.

### Pattern
```rust
use tokio::sync::broadcast;

let (shutdown_tx, _) = broadcast::channel::<()>(1);

// In each actor
let mut shutdown_rx = shutdown_tx.subscribe();
tokio::select! {
    _ = normal_work() => {}
    _ = shutdown_rx.recv() => {
        info!("Received shutdown signal");
        break;
    }
}

// To trigger shutdown
drop(shutdown_tx);
// Or explicit send
shutdown_tx.send(())?;
```

### Where to find examples
- `icn/crates/icn-core/src/runtime.rs`: Shutdown channel setup
- `icn/crates/icn-core/src/supervisor/init_federation.rs`: Shutdown receiver in tasks

## Serialization with Postcard

Binary serialization uses postcard for compact encoding.

### Pattern
```rust
use serde::{Serialize, Deserialize};
use postcard;

#[derive(Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub payload: Vec<u8>,
}

// Serialize
let bytes = postcard::to_allocvec(&msg)?;

// Deserialize
let msg: Message = postcard::from_bytes(&bytes)?;
```

### Where to find examples
- `icn-gossip/src/message.rs`: Gossip message encoding
- `icn/crates/icn-federation/src/agreement/types.rs`: Agreement serialization

---

## Pattern 11: PolicyOracle / Meaning Firewall

The central architectural pattern: apps translate domain semantics into generic constraints the kernel enforces.

### Structure
```rust
// In icn/crates/icn-kernel-api/src/authz.rs
#[async_trait]
pub trait PolicyOracle: Send + Sync {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;
}

pub struct PolicyRequest {
    pub core: PolicyRequestCore,
    pub context: PolicyContext,
}

pub enum PolicyDecision {
    Allow { constraints: ConstraintSet },
    Deny { reason: PolicyError },
}

pub struct ConstraintSet {
    pub rate_limit: Option<RateLimit>,
    pub custom: HashMap<String, ConstraintValue>,
}

// In icn/crates/icn-gateway/src/trust_mgr.rs
impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // 1. Query domain-specific state
        let trust_score = self.trust_graph.compute_trust(request.actor(), request.actor());
        
        // 2. Translate to constraints (THIS IS THE FIREWALL BOUNDARY)
        let constraints = self.trust_score_to_constraints(trust_score);
        
        // 3. Return generic decision
        PolicyDecision::allow_with(constraints)
    }
}

fn trust_score_to_constraints(&self, score: f64) -> ConstraintSet {
    ConstraintSet {
        rate_limit: Some(RateLimit {
            max_requests: (score * 100.0) as u32,
            window: Duration::from_secs(60),
        }),
        ..Default::default()
    }
}
```

### Where to find examples
- `icn/crates/icn-kernel-api/src/authz.rs`: PolicyOracle trait definition
- `icn/crates/icn-gateway/src/trust_mgr.rs`: Trust scores → constraints
- `icn/crates/icn-kernel-api/src/bootstrap.rs`: Oracle registry + domain routing

### Introduced in
`path/phase-2-architecture/05-the-meaning-firewall.md` (Phase 2, Layer 05 — **KEYSTONE**)

### Exercised by
`labs/lab-04-firewall-oracle/` — **THE KEYSTONE LAB** — Build kernel, domain, oracle crates with compile-time boundary enforcement

### Failure mode
**Kernel importing domain crates**: If `icn-core` imports `icn-trust` directly, the kernel now understands trust semantics, breaking the firewall. This prevents cooperative governance from changing policy without kernel rewrites. CI tests enforce this via compile-time dependency checks.

---

## Pattern 12: Receipt Pattern

Operations return structured receipts (Accepted, Rejected, Quarantined), not "accepted" lies.

### Structure
```rust
pub enum Receipt<T> {
    Accepted(T),
    Rejected { reason: String },
    Quarantined { id: String, reason: String },
}

impl Ledger {
    pub fn submit_entry(&mut self, entry: JournalEntry) -> Receipt<EntryId> {
        match self.validate(&entry) {
            Ok(()) => {
                let id = self.store_entry(entry)?;
                Receipt::Accepted(id)
            }
            Err(e) if e.is_recoverable() => {
                self.quarantine.insert(entry.id.clone(), QuarantinedEntry {
                    entry,
                    reason: e.to_string(),
                    timestamp: Timestamp::now(),
                });
                Receipt::Quarantined {
                    id: entry.id,
                    reason: e.to_string(),
                }
            }
            Err(e) => {
                Receipt::Rejected {
                    reason: e.to_string(),
                }
            }
        }
    }
}
```

### Where to find examples
- `icn/crates/icn-ledger/src/ledger.rs`: Quarantine system (submit_entry path)
- `icn/crates/icn-core/src/dead_letter.rs`: DeadLetterQueue for failed operations

### Introduced in
`path/phase-1-foundations/03-errors-and-tracing.md` (Phase 1, Layer 03)

### Exercised by
`labs/lab-02-error-receipt/` — Add structured receipts and tracing to workspace

### Failure mode
**Silently dropping failures**: If you return `Ok(())` for entries that failed validation, users think their data was accepted when it wasn't. Quarantine makes failures visible and debuggable.

---

## Pattern 13: OracleRegistry

Phase-aware bootstrap with atomic oracle replacement via ArcSwap.

### Structure
```rust
// In icn/crates/icn-kernel-api/src/bootstrap.rs
pub struct OracleRegistry {
    oracles: Arc<DashMap<Domain, Arc<dyn PolicyOracle>>>,
    phase: Arc<ArcSwap<BootstrapPhase>>,
}

pub enum BootstrapPhase {
    Genesis,        // No oracles, fallback to AllowAllOracle
    CoreApps,       // Trust/Ledger loaded, limited functionality
    Running,        // Full system operational
}

impl OracleRegistry {
    pub fn register(&self, domain: Domain, oracle: Arc<dyn PolicyOracle>) {
        self.oracles.insert(domain, oracle);
    }
    
    pub fn set_phase(&self, phase: BootstrapPhase) {
        self.phase.store(Arc::new(phase));
    }
    
    pub async fn evaluate(&self, domain: Domain, request: PolicyRequest) -> PolicyDecision {
        let phase = self.phase.load();
        
        match **phase {
            BootstrapPhase::Genesis => {
                // Fallback: allow everything during bootstrap
                PolicyDecision::allow()
            }
            _ => {
                let oracle = self.oracles.get(&domain)?;
                oracle.evaluate(request).await
            }
        }
    }
}
```

### Where to find examples
- `icn/crates/icn-kernel-api/src/bootstrap.rs`: OracleRegistry implementation
- `icn/crates/icn-core/src/supervisor/mod.rs`: Oracle registration during startup

### Introduced in
`path/phase-2-architecture/05-the-meaning-firewall.md` (Phase 2, Layer 05)

### Exercised by
`labs/lab-04-firewall-oracle/` — Implement phased oracle loading

### Failure mode
**Oracles not ready during startup**: If the kernel tries to evaluate requests before oracles are registered, it either panics or allows everything (insecure). Phased bootstrap allows graceful degradation.

---

## Pattern 14: Double-Entry Invariant

Enforce balanced entries at validation time, reject unbalanced entries.

### Structure
```rust
impl Ledger {
    fn validate_entry(&self, entry: &JournalEntry) -> Result<(), LedgerError> {
        // Invariant: Sum of all postings must be zero (double-entry)
        let sum: i64 = entry.postings.iter().map(|p| p.amount).sum();
        
        if sum != 0 {
            return Err(LedgerError::UnbalancedEntry { sum });
        }
        
        // Other invariants...
        Ok(())
    }
    
    pub fn submit_entry(&mut self, entry: JournalEntry) -> Result<(), LedgerError> {
        self.validate_entry(&entry)?;
        self.store.put(entry_key(&entry.id), serialize(&entry)?)?;
        self.update_cached_balances(&entry);
        Ok(())
    }
}
```

### Where to find examples
- `icn/crates/icn-ledger/src/ledger.rs`: `validate_entry()` function (lines ~487-520)
- `icn/crates/icn-ledger/tests/dynamic_limits_integration.rs`: Tests proving invariants

### Introduced in
`path/phase-2-architecture/06-persistence-and-ledger.md` (Phase 2, Layer 06)

### Exercised by
`labs/lab-05-mini-ledger/` — Implement double-entry ledger with quarantine

### Failure mode
**Accepting unbalanced entries**: If you skip the sum check, you violate conservation of value. This breaks accounting integrity and causes mysterious balance mismatches. The invariant must be enforced at the storage boundary.

---

## Pattern 15: Handle Holder

Use `Arc<RwLock<Option<Handle>>>` for late-bound actors during phased initialization.

### Structure
```rust
// During supervisor initialization
let gossip_handle_holder = Arc::new(RwLock::new(None));

// Clone for callback setup (before actor spawned)
let gossip_handle_clone = gossip_handle_holder.clone();

// Spawn actor
let gossip_handle = spawn_gossip_actor(config).await?;

// Store handle
*gossip_handle_holder.write().await = Some(gossip_handle);

// Later use (in callback or other actor)
if let Some(ref handle) = *gossip_handle_clone.read().await {
    handle.announce(topic, data).await?;
} else {
    return Err("Gossip actor not initialized yet");
}
```

### Where to find examples
- `icn/crates/icn-core/src/supervisor/init_gossip.rs`: Gossip handle holder
- `icn/crates/icn-core/src/supervisor/init_network.rs`: Network handle holder

### Introduced in
`path/phase-2-architecture/04-actors-and-concurrency.md` (Phase 2, Layer 04)

### Exercised by
`labs/lab-03-mini-actor/` — Implement late-bound handle pattern

### Failure mode
**Circular dependencies**: If Actor A needs Actor B's handle and Actor B needs Actor A's handle, you can't spawn either first. Handle holders break the cycle: spawn A with empty holder, spawn B with A's handle, fill A's holder with B's handle.

---

## Pattern 16: Quarantine

Isolate entries that fail validation instead of dropping them.

### Structure
```rust
pub struct QuarantinedEntry {
    pub entry: JournalEntry,
    pub reason: String,
    pub timestamp: Timestamp,
}

pub struct Ledger {
    store: Arc<dyn Store>,
    quarantine: HashMap<EntryId, QuarantinedEntry>,
    // ...
}

impl Ledger {
    pub fn submit_entry(&mut self, entry: JournalEntry) -> Result<(), LedgerError> {
        match self.validate_entry(&entry) {
            Ok(()) => {
                self.store.put(entry_key(&entry.id), serialize(&entry)?)?;
                Ok(())
            }
            Err(e) => {
                // Don't drop — quarantine with reason
                self.quarantine.insert(entry.id.clone(), QuarantinedEntry {
                    entry,
                    reason: e.to_string(),
                    timestamp: Timestamp::now(),
                });
                
                metrics::counter!("ledger_validation_failures_total",
                    "reason" => error_type(&e)).increment(1);
                
                Err(e)
            }
        }
    }
    
    pub fn get_quarantine(&self) -> &HashMap<EntryId, QuarantinedEntry> {
        &self.quarantine
    }
    
    pub fn retry_quarantined(&mut self, entry_id: &EntryId) -> Result<(), LedgerError> {
        let quarantined = self.quarantine.remove(entry_id)?;
        self.submit_entry(quarantined.entry)  // Retry with current validation rules
    }
}
```

### Where to find examples
- `icn/crates/icn-ledger/src/ledger.rs`: Quarantine system
- `icn/crates/icn-ledger/tests/dynamic_limits_integration.rs`: Quarantine tests

### Introduced in
`path/phase-2-architecture/06-persistence-and-ledger.md` (Phase 2, Layer 06)

### Exercised by
`labs/lab-05-mini-ledger/` — Implement quarantine for unbalanced entries

### Failure mode
**Silent data loss**: If you drop failed entries without recording them, you lose data with no trace. Users can't debug why their entry disappeared. Quarantine makes failures visible and provides a recovery path if validation rules change.

---

## Metadata Updates for Existing Patterns

### Pattern 2: Error Handling
**Introduced in**: `path/phase-1-foundations/03-errors-and-tracing.md` (Phase 1, Layer 03)  
**Exercised by**: `labs/lab-02-error-receipt/`

### Pattern 3: Async Callbacks
**Introduced in**: `path/phase-2-architecture/04-actors-and-concurrency.md` (Phase 2, Layer 04)  
**Exercised by**: `labs/lab-03-mini-actor/`

### Pattern 4: Store Abstraction
**Introduced in**: `path/phase-2-architecture/06-persistence-and-ledger.md` (Phase 2, Layer 06)  
**Exercised by**: `labs/lab-05-mini-ledger/`

### Pattern 5: Builder
**Introduced in**: `path/phase-1-foundations/02-rust-through-icn.md` (Phase 1, Layer 02)  
**Exercised by**: `labs/lab-01-workspace/`

### Pattern 6: Lifecycle State Machine
**Introduced in**: `path/phase-1-foundations/02-rust-through-icn.md` (Phase 1, Layer 02)  
**Exercised by**: `labs/lab-08-governance-flow/` (proposal lifecycle)

### Pattern 7: Metrics Integration
**Introduced in**: `path/phase-1-foundations/03-errors-and-tracing.md` (Phase 1, Layer 03)  
**Exercised by**: `labs/lab-02-error-receipt/`

### Pattern 8: Trust-Gated Operations
**Introduced in**: `path/phase-2-architecture/05-the-meaning-firewall.md` (Phase 2, Layer 05)  
**Exercised by**: `labs/lab-04-firewall-oracle/` (domain concept → constraint translation)

### Pattern 9: Shutdown Coordination
**Introduced in**: `path/phase-2-architecture/04-actors-and-concurrency.md` (Phase 2, Layer 04)  
**Exercised by**: `labs/lab-03-mini-actor/`

### Pattern 10: Serialization
**Introduced in**: `path/phase-3-systems/08-network-and-gossip.md` (Phase 3, Layer 08)  
**Exercised by**: `labs/lab-07-gossip-sync/`
