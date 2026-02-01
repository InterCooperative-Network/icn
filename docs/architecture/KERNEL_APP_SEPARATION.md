# Kernel/App Separation Architecture

**Status**: Living Document  
**Last Updated**: 2026-02-01
**Related Issues**: Phase 19+ (Trust Extraction, PolicyOracle Migration), [#1007](https://github.com/InterCooperative-Network/icn/issues/1007) (Wave 1 Firewall Contract)

---

## Executive Summary

> **ICN is a constraint engine: apps translate meaning into constraints; the kernel enforces constraints without understanding meaning.**

ICN's kernel/app separation architecture enforces a strict boundary—the **Meaning Firewall**—between infrastructure mechanisms (kernel) and domain semantics (apps). This document describes the architectural principles, implementation patterns, and migration path for this separation.

**Key Insight**: The kernel enforces constraints WITHOUT understanding their semantic origin. Trust scores, governance rules, and membership criteria are domain concepts that apps translate into generic constraints the kernel can enforce blindly.

This is the **constraint engine model**: Policy Oracles (apps) decide constraint values → Kernel enforces constraints without knowing why.

---

## Firewall Contract (Non-Negotiable Invariants)

> **Status**: Enforced via CI  
> **Tracking Issue**: [#1007](https://github.com/InterCooperative-Network/icn/issues/1007) (Wave 1)

These invariants are mechanically enforced to ensure the kernel remains semantically blind.

### Invariant 1: Kernel Must Not Import Domain Crates

**Rule**: Kernel crates (`icn-core`, `icn-kernel-api`, `icn-net`, `icn-gossip`, `icn-store`) must NOT depend on:
- Domain crates: `icn-trust`, `icn-governance`, `icn-ledger` (internals)
- App crates: anything under `apps/`

**Rationale**: Direct imports create compile-time coupling. If kernel code can import `TrustGraph`, it can call domain-specific methods, violating the meaning firewall.

**Enforcement**: CI checks dependency closure via `cargo metadata` (see `.github/scripts/firewall_denylist.py`).

**Example Violation**:
```rust
// ❌ FORBIDDEN in icn-core
use icn_trust::TrustGraph;

fn select_replica(&self, candidates: &[Did]) -> Did {
    let score = self.trust_graph.compute_trust_score(&candidates[0]);
    // Kernel now "knows" what trust scores mean
}
```

**Correct Pattern**:
```rust
// ✅ ALLOWED - kernel receives constraints from PolicyOracle
fn select_replica(&self, candidates: &[Did], constraints: &ConstraintSet) -> Did {
    let min_score = constraints.custom.get("min_trust_score")
        .and_then(|v| v.as_float())
        .unwrap_or(0.0);
    // Kernel enforces the constraint without knowing WHY min_score=0.7
}
```

### Invariant 2: Kernel Only Accepts Pure Data Types

**Rule**: Kernel entrypoints must accept only:
- **Cryptographic proofs**: `IdentityProof`, Ed25519 signatures
- **Pure data**: `ConstraintSet`, `CapabilityToken`, `RateLimitPolicy`
- **Kernel primitives**: `Did`, `Timestamp`, `Hash`

**Rationale**: Pure data types have no behavior. Accepting `&TrustGraph` or `GovernanceConfig` allows kernel code to call semantic methods.

**Example Violation**:
```rust
// ❌ FORBIDDEN - accepting domain type
pub fn authorize_action(
    &self,
    actor: &Did,
    trust_graph: &TrustGraph,  // Domain type with semantic methods
) -> bool {
    trust_graph.compute_trust_score(actor) >= 0.4
}
```

**Correct Pattern**:
```rust
// ✅ ALLOWED - accepting pure data
pub fn authorize_action(
    &self,
    actor: &Did,
    constraints: &ConstraintSet,  // Pure data, no methods
) -> bool {
    constraints.custom.get("trust_score")
        .and_then(|v| v.as_float())
        .map(|score| score >= 0.4)
        .unwrap_or(false)
}
```

### Invariant 3: All Semantic Lookup Confined to PolicyOracle

**Rule**: Any decision based on trust scores, governance rules, membership status, or credit limits MUST happen in a `PolicyOracle` implementation, NOT in kernel code.

**Rationale**: PolicyOracles are the translation boundary. They convert domain semantics into `ConstraintSet` values the kernel can enforce blindly.

**Example Violation**:
```rust
// ❌ FORBIDDEN in icn-core
fn rate_limit_for(&self, actor: &Did) -> RateLimit {
    let score = self.trust_graph.compute_trust_score(actor);
    if score >= 0.7 {  // Kernel "knows" 0.7 means "Federated"
        RateLimit::new(200, 50)
    } else {
        RateLimit::new(20, 5)
    }
}
```

**Correct Pattern**:
```rust
// ✅ In apps/trust/src/oracle.rs (PolicyOracle implementation)
impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let score = self.graph.compute_trust_score(&request.actor);
        
        // Semantic logic ENDS here ═══════════════════════════
        // Convert to pure constraints below the firewall
        
        let rate_limit = if score >= 0.7 {
            RateLimit::new(200, 50)
        } else {
            RateLimit::new(20, 5)
        };
        
        PolicyDecision::allow_with(
            ConstraintSet::new().with_rate_limit(rate_limit)
        )
    }
}

// ✅ In icn-core (kernel code)
fn apply_rate_limit(&self, actor: &Did, constraints: &ConstraintSet) {
    if let Some(limit) = constraints.rate_limit {
        self.rate_limiter.apply(&actor, limit);
        // Kernel doesn't know WHY limit is 200/50 vs 20/5
    }
}
```

### Invariant 4: Kernel Must Not Pattern-Match on `ConstraintSet.custom` Keys

**Rule**: Kernel enforcement logic must ONLY use typed `ConstraintSet` fields:
- `rate_limit: Option<RateLimit>`
- `max_topics: Option<u32>`
- `max_connections: Option<u32>`
- etc.

Kernel code MUST NOT:
- Check for specific `custom` keys (e.g., `constraints.custom.get("trust_score")`)
- Branch on `custom` key presence
- Extract `custom` values to affect enforcement

**Rationale**: `custom` is an escape hatch for app-specific data. If kernel code depends on `custom` keys, it's indirectly depending on app semantics.

**Example Violation**:
```rust
// ❌ FORBIDDEN - kernel branching on custom key
fn should_replicate(&self, constraints: &ConstraintSet) -> bool {
    if let Some(trust_score) = constraints.custom.get("trust_score") {
        trust_score.as_float().unwrap() >= 0.4  // Kernel "knows" trust_score semantics
    } else {
        false
    }
}
```

**Correct Pattern**:
```rust
// ✅ ALLOWED - PolicyOracle sets typed field
impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let score = self.graph.compute_trust_score(&request.actor);
        
        PolicyDecision::allow_with(
            ConstraintSet::new()
                .with_custom("can_replicate", ConstraintValue::Bool(score >= 0.4))
        )
    }
}

// ✅ Kernel checks typed field (if added to ConstraintSet)
// OR kernel remains completely agnostic and lets app handle it
fn should_replicate(&self, constraints: &ConstraintSet) -> bool {
    // Option A: Add typed field to ConstraintSet
    constraints.can_replicate.unwrap_or(false)
    
    // Option B: Don't check at all - let replication be a higher-level decision
}
```

### Invariant 5: Kernel Contains Enforcement Mechanisms, Not Policy Decisions

**Rule**: Kernel provides enforcement infrastructure. Apps provide policy values.

| Component | Kernel Owns (Mechanism) | Apps Own (Value) | Notes |
|-----------|-------------------------|------------------|-------|
| **Rate limiting** | `RateLimiter` struct, token bucket algorithm | `rate_limit: RateLimit(messages_per_second, burst_size)` in `ConstraintSet` | Kernel enforces rate; app decides rate |
| **Credit gating** | `CreditGate` check logic (`balance >= ceiling?`) | `credit_ceiling: 1000` in `ConstraintSet` | Kernel checks ceiling; app computes ceiling |
| **Capability gating** | `CapabilityGate` check logic (`caps.contains(required)?`) | `capabilities: [Read, Write]` in `ConstraintSet` | Kernel checks presence; app grants capabilities |
| **Topic access** | Topic subscription acceptance logic | `allowed_topics: Vec<String>` in `ConstraintSet` | Kernel enforces list; app computes list |
| **Replay guard** | `ReplayGuard` sequence tracking and rejection logic | N/A (no tunable values) | Pure mechanism, no policy knobs |
| **Transport auth** | TLS certificate verification, DID-TLS binding | N/A (no tunable values) | Pure cryptographic check, no policy |

**Example**: Rate limiting mechanism vs value

```rust
// ✅ Kernel provides the mechanism
pub struct RateLimiter {
    limiters: HashMap<Did, TokenBucket>,
}

impl RateLimiter {
    pub fn check(&mut self, actor: &Did, limit: &RateLimit) -> Result<(), RateLimitError> {
        let bucket = self.limiters.entry(*actor).or_insert_with(|| {
            TokenBucket::new(limit.messages_per_second, limit.burst_size)
        });
        bucket.consume(1)  // Mechanism: token bucket algorithm
    }
}

// ✅ App provides the value
impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let score = self.graph.compute_trust_score(&request.actor);
        
        // App decides the rate based on trust class
        let rate = if score >= 0.7 { RateLimit::new(200, 50) }
                   else if score >= 0.4 { RateLimit::new(100, 25) }
                   else { RateLimit::new(20, 5) };
        
        PolicyDecision::allow_with(ConstraintSet::new().with_rate_limit(rate))
    }
}
```

### Violation Examples (What NOT to Do)

**❌ Direct trust-score call in kernel**:
```rust
// icn/crates/icn-core/src/supervisor/init_gossip.rs
fn select_anti_entropy_peers(&self) -> Vec<Did> {
    self.peers.iter()
        .filter(|peer| {
            let score = self.trust_graph.compute_trust_score(peer);  // VIOLATION
            score >= 0.4
        })
        .collect()
}
```

**❌ Importing domain types in kernel**:
```rust
// icn/crates/icn-net/src/rate_limit.rs
use icn_trust::{TrustGraph, TrustClass};  // VIOLATION
```

**❌ Hardcoded policy constants in kernel**:
```rust
// icn/crates/icn-core/src/supervisor/mod.rs
const MIN_TRUST_FOR_REPLICATION: f64 = 0.4;  // VIOLATION - kernel defines policy
```

**❌ Pattern-matching on custom keys**:
```rust
// icn/crates/icn-core/src/supervisor/replication.rs
fn should_replicate(&self, constraints: &ConstraintSet) -> bool {
    if let Some(class) = constraints.custom.get("trust_class") {  // VIOLATION
        class.as_string() == Some("Partner")
    }
}
```

### Migration Path

1. **Wave 1 (This Issue)**: Document contract, add CI enforcement
2. **Waves 2-6**: Migrate existing violations to PolicyOracle pattern
3. **Enforcement**: After Wave 1, CI MUST fail on new violations

---

## Table of Contents

1. [Firewall Contract](#firewall-contract-non-negotiable-invariants)
2. [The Meaning Firewall](#1-the-meaning-firewall)
3. [Architectural Layers](#2-architectural-layers)
4. [Core Abstractions](#3-core-abstractions)
   - [PolicyOracle](#31-policyoracle)
   - [TrustService](#32-trustservice)
   - [ServiceRegistry](#33-serviceregistry)
   - [ConstraintSet](#34-constraintset)
   - [OracleRegistry](#35-oracleregistry)
5. [Request Flow](#4-request-flow)
6. [Implementation Guide](#5-implementation-guide)
7. [Migration Status](#6-migration-status)
8. [Testing Patterns](#7-testing-patterns)
9. [FAQ](#8-faq)

---

## 1. The Meaning Firewall

### 1.1 Core Principle

The **Meaning Firewall** is the conceptual boundary where domain semantics end and kernel enforcement begins. 

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           APPLICATION LAYER                              │
│                                                                          │
│   ┌───────────┐    ┌────────────┐    ┌─────────────┐    ┌───────────┐   │
│   │   Trust   │    │ Governance │    │    Ledger   │    │ Membership│   │
│   │   Graph   │    │   Rules    │    │   Policy    │    │   Roles   │   │
│   └─────┬─────┘    └──────┬─────┘    └──────┬──────┘    └─────┬─────┘   │
│         │                 │                  │                 │         │
│         ▼                 ▼                  ▼                 ▼         │
│   ┌───────────────────────────────────────────────────────────────────┐ │
│   │                        PolicyOracle                                │ │
│   │   (Translates domain semantics → ConstraintSet)                   │ │
│   └───────────────────────────────────────────────────────────────────┘ │
│                                   │                                      │
└───────────────────────────────────┼──────────────────────────────────────┘
                                    │
════════════════════════════════════╪══════════════════════════════════════
                          MEANING FIREWALL
                     (Domain semantics END here)
════════════════════════════════════╪══════════════════════════════════════
                                    │
                                    ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                             KERNEL LAYER                                   │
│                                                                            │
│   ┌─────────────────────────────────────────────────────────────────────┐ │
│   │                         ConstraintSet                                │ │
│   │   - rate_limit: RateLimit                                           │ │
│   │   - credit_multiplier: f64                                          │ │
│   │   - max_topics: u32                                                 │ │
│   │   - custom: HashMap<String, ConstraintValue>                        │ │
│   └─────────────────────────────────────────────────────────────────────┘ │
│                                   │                                        │
│                                   ▼                                        │
│   ┌─────────────┐    ┌────────────┐    ┌─────────────┐    ┌────────────┐  │
│   │    Rate     │    │   Credit   │    │   Topic     │    │   Route    │  │
│   │  Limiting   │    │  Control   │    │   Access    │    │  Priority  │  │
│   └─────────────┘    └────────────┘    └─────────────┘    └────────────┘  │
│                                                                            │
│           The kernel enforces these WITHOUT understanding WHY              │
└───────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Why This Matters

**Before (Tight Coupling)**:
```rust
// ❌ Kernel code directly using domain types
use icn_trust::{TrustGraph, TrustClass};

fn select_replica_candidates(&self) -> Vec<Did> {
    let score = self.trust_graph.compute_trust_score(&peer);
    if TrustClass::from_score(score) >= TrustClass::Partner {
        // Kernel now "knows" what Partner means
        candidates.push(peer);
    }
}
```

**After (Meaning Firewall)**:
```rust
// ✅ Kernel using abstract interfaces
use icn_kernel_api::services::TrustService;

fn select_replica_candidates(&self) -> Vec<Did> {
    let score = self.trust_service.trust_score(&peer);
    if score >= self.config.min_trust_score {  // Just a number!
        candidates.push(peer);
    }
}
```

### 1.3 Decision Criteria

Before adding code to kernel crates, ask:

| Question | If YES → |
|----------|----------|
| Does this interpret domain semantics? | Must be an app |
| Does this hardcode a schema? | Must be an app |
| Does this privilege a specific application? | Must be an app |
| Does this require knowing what a trust score *means*? | Must be an app |

The kernel is deliberately dumb. It provides **pipes, not policies**.

---

## 2. Architectural Layers

### 2.1 Crate Classification

```
KERNEL CRATES (domain-agnostic)              APP CRATES (domain-specific)
────────────────────────────────             ────────────────────────────
icn-kernel-api   ← Trait definitions          icn-trust    ← Trust graph
icn-core         ← Runtime, supervisor        icn-governance ← Voting, proposals  
icn-net          ← QUIC, mDNS, sessions       icn-ledger   ← Credit, accounts
icn-gossip       ← Topics, vector clocks      icn-ccl      ← Contract execution
icn-store        ← Sled persistence           icn-entity   ← Organizations
icn-obs          ← Metrics, tracing           icn-community ← Membership
                                              icn-federation ← Cross-coop
```

### 2.2 Dependency Rules

```
                    ┌─────────────────┐
                    │ icn-kernel-api  │  ← Trait definitions
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              ▼              ▼              ▼
        ┌──────────┐   ┌──────────┐   ┌──────────┐
        │ icn-core │   │ icn-net  │   │icn-gossip│
        └────┬─────┘   └──────────┘   └──────────┘
             │
             │  ❌ FORBIDDEN: Direct imports of domain crates
             │
             │  ✅ ALLOWED: Through TrustService, PolicyOracle, etc.
             │
```

**CI Enforcement**: The `kernel-deps` CI job checks that kernel crates don't depend on forbidden domain crates:

```yaml
# From .github/workflows/ci.yml
kernel-deps:
  name: Kernel Forbidden Dependencies
  steps:
    - name: Check kernel crates for forbidden deps
      run: |
        KERNEL_CRATES=("icn-core" "icn-net" "icn-gossip")
        FORBIDDEN=("icn-trust" "icn-governance" "icn-ledger" ...)
        for crate in "${KERNEL_CRATES[@]}"; do
          for dep in "${FORBIDDEN[@]}"; do
            if cargo tree -p "$crate" | grep -q "${dep}"; then
              echo "Forbidden dependency: ${crate} -> ${dep}"
              exit 1
            fi
          done
        done
```

---

## 3. Core Abstractions

### 3.1 PolicyOracle

The `PolicyOracle` trait is the primary interface for authorization decisions. Apps implement it to translate domain-specific rules into generic constraints.

```rust
/// File: icn/crates/icn-kernel-api/src/authz.rs

pub trait PolicyOracle: Send + Sync {
    /// Evaluate a policy request and return a decision
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;
    
    /// Domain this oracle handles (e.g., "trust", "governance")
    fn domain(&self) -> Domain;
}

pub enum PolicyDecision {
    /// Allow with constraints
    Allow { constraints: ConstraintSet },
    /// Deny with reason
    Deny { reason: String },
    /// Abstain (let other oracles decide)
    Abstain,
}
```

**Example Implementation** (TrustPolicyOracle):

```rust
/// File: icn/crates/icn-gateway/src/trust_policy.rs

impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // 1. Extract actor from request
        let actor = &request.actor;
        
        // 2. Compute trust score (domain logic)
        let score = self.graph.compute_trust_score(actor);
        
        // ════════════════════════════════════════════
        // ▼▼▼ MEANING FIREWALL CROSSING ▼▼▼
        // Trust semantics END here
        // ════════════════════════════════════════════
        
        // 3. Convert to generic constraints (kernel-safe)
        let constraints = ConstraintSet::new()
            .with_rate_limit(score_to_rate_limit(score))
            .with_credit_multiplier(score)
            .with_custom("trust_score", ConstraintValue::Float(score));
        
        PolicyDecision::Allow { constraints }
    }
    
    fn domain(&self) -> Domain {
        Domain::Trust
    }
}

fn score_to_rate_limit(score: f64) -> RateLimit {
    // Trust class boundaries, but kernel sees only numbers
    if score >= 0.7 {      // Federated
        RateLimit { messages_per_sec: 200, burst: 50 }
    } else if score >= 0.4 { // Partner
        RateLimit { messages_per_sec: 100, burst: 25 }
    } else if score >= 0.1 { // Known
        RateLimit { messages_per_sec: 20, burst: 5 }
    } else {                 // Isolated
        RateLimit { messages_per_sec: 10, burst: 2 }
    }
}
```

### 3.2 TrustService

The `TrustService` trait provides trust-related functionality without exposing `TrustGraph`, `TrustClass`, or other domain types.

```rust
/// File: icn/crates/icn-kernel-api/src/services.rs

pub trait TrustService: Send + Sync {
    /// Get the PolicyOracle for this trust service
    fn oracle(&self) -> Arc<dyn PolicyOracle>;
    
    /// Get trust score for an actor (opaque value 0.0-1.0)
    fn trust_score(&self, actor: &Did) -> f64;
    
    /// Check if actor meets minimum trust threshold
    fn meets_trust_threshold(&self, actor: &Did, min_score: f64) -> bool {
        self.trust_score(actor) >= min_score
    }
    
    /// Record a trust-affecting event
    fn record_event(&self, actor: &Did, event: TrustEvent);
}
```

**Kernel-Level TrustClass**:

For convenience, the kernel defines its own `TrustClass` enum that apps can use for boundary conversions:

```rust
/// File: icn/crates/icn-kernel-api/src/services.rs

pub const TRUST_THRESHOLD_KNOWN: f64 = 0.1;
pub const TRUST_THRESHOLD_PARTNER: f64 = 0.4;
pub const TRUST_THRESHOLD_FEDERATED: f64 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrustClass {
    #[default]
    Isolated = 0,   // score < 0.1
    Known = 1,      // 0.1 <= score < 0.4
    Partner = 2,    // 0.4 <= score < 0.7
    Federated = 3,  // score >= 0.7
}

impl TrustClass {
    pub fn from_score(score: f64) -> Self { /* ... */ }
    pub fn min_score(&self) -> f64 { /* ... */ }
}
```

### 3.3 ServiceRegistry

The `ServiceRegistry` allows the daemon to inject domain services into the kernel at startup:

```rust
/// File: icn/crates/icn-kernel-api/src/services.rs

pub struct ServiceRegistry {
    trust: Option<Arc<dyn TrustService>>,
    security: Option<Arc<dyn SecurityService>>,
    governance: Option<Arc<dyn GovernanceService>>,
    ledger: Option<Arc<dyn LedgerService>>,
    
    /// Transition handles for migration period
    raw_handles: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl ServiceRegistry {
    pub fn new() -> Self;
    pub fn with_trust(self, service: Arc<dyn TrustService>) -> Self;
    pub fn with_security(self, service: Arc<dyn SecurityService>) -> Self;
    pub fn with_governance(self, service: Arc<dyn GovernanceService>) -> Self;
    
    /// Transition mechanism for components still needing direct access
    pub fn with_raw_handle<T>(self, key: &str, handle: Arc<T>) -> Self;
    pub fn raw_handle<T>(&self, key: &str) -> Option<Arc<T>>;
}
```

**Key Constants** (prevent typo-induced silent `None`):

```rust
impl ServiceRegistry {
    pub const TRUST_GRAPH_KEY: &str = "trust_graph";
    pub const PROTOCOL_PARAM_STORE_KEY: &str = "protocol_parameter_store";
    pub const LEDGER_KEY: &str = "ledger";
    pub const LEDGER_STORE_KEY: &str = "ledger_store";
}
```

Always use these constants instead of string literals when calling `with_raw_handle()` or `raw_handle()`.

**Usage in icnd** (all three services wired):

```rust
// In icnd main.rs — build_service_registry()
// 1. Trust
let trust_graph_handle = Arc::new(RwLock::new(TrustGraph::new(store, did)));
let trust_service = icn_trust_app::create_service_tokio(trust_graph_handle.clone());
registry = registry.with_trust(trust_service);
registry = registry.with_raw_handle(ServiceRegistry::TRUST_GRAPH_KEY, trust_graph_handle);

// 2. Governance
let parameter_store = Arc::new(SledParameterStore::new(Arc::new(param_db))?);
let governance_service = icn_governance_app::create_service(parameter_store.clone() as _);
registry = registry.with_governance(governance_service);
registry = registry.with_raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY, parameter_store);

// 3. Ledger
let ledger_store = Arc::new(SledStore::open(&ledger_store_path)?);
let ledger = Ledger::new(ledger_store.clone() as _)?;
let ledger_handle = Arc::new(RwLock::new(ledger));
let ledger_service = icn_ledger_app::create_service(ledger_handle.clone());
registry = registry.with_ledger(ledger_service);
registry = registry.with_raw_handle(ServiceRegistry::LEDGER_KEY, ledger_handle);
registry = registry.with_raw_handle(ServiceRegistry::LEDGER_STORE_KEY, ledger_store);
```

#### raw_handle Type Patterns

The `raw_handle<T>` method requires `T: Sized`. This creates two patterns depending on the stored type:

**Pattern A: Concrete type with trait upcast** (for trait objects like `dyn ProtocolParameterStore`):
```rust
// Store concrete type (Sized)
registry.with_raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY, arc_sled_param_store);

// Retrieve as concrete, then upcast
let store: Arc<SledParameterStore> = registry.raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY)?;
let trait_obj: Arc<dyn ProtocolParameterStore> = store;  // upcast
```

**Pattern B: Direct storage** (for wrapped types like `Arc<RwLock<Ledger>>`):
```rust
// Store directly (RwLock<Ledger> is Sized)
registry.with_raw_handle(ServiceRegistry::LEDGER_KEY, ledger_handle);

// Retrieve directly
let handle: Arc<RwLock<Ledger>> = registry.raw_handle(ServiceRegistry::LEDGER_KEY)?;
```

**Why the daemon passes stores separately**: Sled uses exclusive file locking (`flock` on Linux). Opening the same sled path twice in the same process fails. The daemon creates stores once and passes them through `raw_handle` so the supervisor can reuse them for ancillary services (DisputeManager, TreasuryManager) without re-opening.

### 3.4 ConstraintSet

The `ConstraintSet` is the sole output format for `PolicyOracle`. It's designed to be domain-blind:

```rust
/// File: icn/crates/icn-kernel-api/src/authz.rs

pub struct ConstraintSet {
    pub rate_limit: Option<RateLimit>,
    pub credit_multiplier: Option<f64>,
    pub max_topics: Option<u32>,
    pub priority_class: Option<u8>,
    pub custom: HashMap<String, ConstraintValue>,
}

pub struct RateLimit {
    pub messages_per_sec: u32,
    pub burst: u32,
}

pub enum ConstraintValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<ConstraintValue>),
}
```

The kernel enforces these without knowing whether they came from trust scores, governance rules, or membership status.

### 3.5 OracleRegistry

The `OracleRegistry` is the centralized authorization gateway that routes policy requests to domain-specific oracles. It implements `PolicyOracle` so kernel components (GossipActor, NetworkActor, RPC server) can use it as their single authorization endpoint.

**Key features:**
- **Per-domain routing**: Requests are dispatched to the oracle registered for the request's domain (e.g., `trust`, `ledger`, `governance`)
- **Bootstrap phases**: Genesis (allow all) → CoreApps (allow all) → Running (deny unknown domains)
- **Decision caching**: TTL-based cache with automatic invalidation on oracle swap
- **Atomic oracle replacement**: Uses `ArcSwap` for lock-free reads during hot path evaluation

**Supervisor lifecycle integration:**

```
1. Supervisor creates OracleRegistry (Genesis phase - AllowAll)
2. Daemon registers TrustPolicyOracle for "trust" domain
3. Registry transitions to CoreApps phase
4. All actors initialize with OracleRegistry as their PolicyOracle
5. Registry transitions to Running phase (deny-by-default for unknown domains)
```

**Usage in kernel components:**

```rust
// The supervisor passes OracleRegistry as Arc<dyn PolicyOracle>
let oracle_registry = Arc::new(OracleRegistry::new());
oracle_registry.register(Domain::trust(), trust_oracle);
oracle_registry.set_phase(BootstrapPhase::Running);

// Kernel components receive it as a generic PolicyOracle
let oracle: Arc<dyn PolicyOracle> = oracle_registry;
let gossip = GossipActor::new(did, Some(oracle));
```

**Location**: `icn-kernel-api/src/bootstrap.rs`

---

## 4. Request Flow

### 4.1 TrustPolicyOracle Flow Diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│                         API Request Arrives                               │
└───────────────────────────────────┬──────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────────────────┐
│                         TrustPolicyOracle                                 │
│                         ─────────────────                                 │
│   fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision          │
└───────────────────────────────────┬──────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    │                               │
                    ▼                               ▼
            ┌───────────────┐              ┌───────────────────┐
            │ domain check  │              │ domain != "trust" │
            │ domain ==     │              │                   │
            │   "trust"     │              │ Return: Abstain   │
            └───────┬───────┘              └───────────────────┘
                    │
                    ▼
            ┌───────────────────────────────────────────────────┐
            │              Parse Actor DID                       │
            │    ┌─────────────────────────────────────────┐    │
            │    │ Invalid DID? → Return score = 0.0       │    │
            │    └─────────────────────────────────────────┘    │
            └───────────────────────────┬───────────────────────┘
                                        │
                                        ▼
            ┌───────────────────────────────────────────────────┐
            │         Trust Graph Lookup                         │
            │   ────────────────────────────                     │
            │   graph.compute_trust_score(actor) → f64          │
            │                                                    │
            │   ┌─────────────────────────────────────────┐     │
            │   │ Lock Strategy:                          │     │
            │   │   1. Try try_read() first               │     │
            │   │   2. If contention → block_in_place()   │     │
            │   │   3. Increment counter metric           │     │
            │   └─────────────────────────────────────────┘     │
            └───────────────────────────┬───────────────────────┘
                                        │
                                        │ trust_score: f64
                                        │
════════════════════════════════════════╪═══════════════════════════════════
                              MEANING FIREWALL
                         Trust semantics END here
════════════════════════════════════════╪═══════════════════════════════════
                                        │
                                        ▼
            ┌───────────────────────────────────────────────────┐
            │         score_to_constraints(score: f64)          │
            │   ─────────────────────────────────────────────   │
            │                                                    │
            │   ConstraintSet {                                  │
            │       rate_limit: score_to_rate_limit(score),     │
            │       credit_multiplier: score,                    │
            │       max_topics: score_to_max_topics(score),     │
            │       custom: { "trust_score": Float(score) }     │
            │   }                                                │
            └───────────────────────────┬───────────────────────┘
                                        │
                                        ▼
            ┌───────────────────────────────────────────────────┐
            │   Return: PolicyDecision::Allow { constraints }    │
            └───────────────────────────┬───────────────────────┘
                                        │
                                        ▼
            ┌───────────────────────────────────────────────────┐
            │                   KERNEL                           │
            │   ────────────────────────────────                 │
            │   Enforces rate_limit, credit_multiplier, etc.    │
            │   WITHOUT knowing they came from trust scores      │
            └───────────────────────────────────────────────────┘
```

### 4.2 Component Integration

The following components have been migrated to use `TrustService`:

| Component | Location | Before | After |
|-----------|----------|--------|-------|
| ReplicationManager | icn-core/replication | `Arc<RwLock<TrustGraph>>` | `Arc<dyn TrustService>` |
| ForkResolver | icn-ledger | `Arc<RwLock<TrustGraph>>` | `Option<Arc<dyn TrustService>>` |
| TrustManager | icn-gateway | `Arc<RwLock<TrustGraph>>` | `Option<Arc<dyn TrustService>>` |
| ChallengeScheduler | icn-core | `Arc<RwLock<TrustGraph>>` | `Option<Arc<dyn TrustService>>` |
| PolicySource | icn-core/policy | `icn_trust::TrustClass` | `icn_kernel_api::TrustClass` |
| IdentityHandle | icn-core/identity | `icn_trust::TrustClass` | `icn_kernel_api::TrustClass` |

---

## 5. Implementation Guide

### 5.1 Adding a New PolicyOracle

1. **Define your domain** in `icn-kernel-api/src/authz.rs`:

```rust
pub enum Domain {
    Trust,
    Governance,
    Ledger,
    Membership,  // ← Add new domain
}
```

2. **Implement PolicyOracle** in your app crate:

```rust
// In apps/membership/src/oracle.rs
use icn_kernel_api::{PolicyOracle, PolicyRequest, PolicyDecision, ConstraintSet, Domain};

pub struct MembershipOracle {
    membership_db: Arc<MembershipDb>,
}

impl PolicyOracle for MembershipOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        if request.domain != Domain::Membership {
            return PolicyDecision::Abstain;
        }
        
        let actor = &request.actor;
        let role = self.membership_db.get_role(actor);
        
        // ═══ MEANING FIREWALL ═══
        // Convert role → constraints
        let constraints = role_to_constraints(role);
        
        PolicyDecision::Allow { constraints }
    }
    
    fn domain(&self) -> Domain {
        Domain::Membership
    }
}
```

3. **Register in ServiceRegistry**:

```rust
// In icnd or your app's initialization
let membership_oracle = Arc::new(MembershipOracle::new(db));
let registry = registry.with_oracle(membership_oracle);
```

### 5.2 Migrating a Component from TrustGraph to TrustService

**Step 1**: Add optional `TrustService` field:

```rust
pub struct MyComponent {
    // Keep during transition
    trust_graph: Option<Arc<RwLock<TrustGraph>>>,
    // New: prefer this
    trust_service: Option<Arc<dyn TrustService>>,
}
```

**Step 2**: Add helper method that prefers TrustService:

```rust
impl MyComponent {
    fn get_trust_score(&self, did: &Did) -> f64 {
        // Prefer TrustService (kernel/app separation)
        if let Some(ref service) = self.trust_service {
            let kernel_did = icn_kernel_api::types::Did::from(did.to_string());
            return service.trust_score(&kernel_did);
        }
        
        // Fallback to TrustGraph (deprecated path)
        if let Some(ref graph) = self.trust_graph {
            if let Ok(guard) = graph.try_read() {
                return guard.compute_trust_score(did);
            }
        }
        
        0.0  // Default if neither available
    }
}
```

**Step 3**: Update callers to pass `TrustService`:

```rust
// In supervisor/lifecycle.rs
let trust_service = registry.trust().cloned();
let component = MyComponent::new()
    .with_trust_service(trust_service);
```

**Step 4**: Remove TrustGraph field once migration is complete.

### 5.3 Converting TrustClass Types

When migrating from `icn_trust::TrustClass` to `icn_kernel_api::TrustClass`:

```rust
// Before
use icn_trust::TrustClass;

fn check_access(&self, score: f64) -> bool {
    TrustClass::from_score(score) >= TrustClass::Partner
}

// After
use icn_kernel_api::TrustClass;

fn check_access(&self, score: f64) -> bool {
    TrustClass::from_score(score) >= TrustClass::Partner
}
// Or more directly:
fn check_access(&self, score: f64) -> bool {
    score >= icn_kernel_api::TRUST_THRESHOLD_PARTNER  // 0.4
}
```

---

## 6. Migration Status

### 6.1 Completed Migrations

| Component | Migration Date | Notes |
|-----------|---------------|-------|
| ReplicationManager → TrustService | 2026-01-27 | Full migration |
| ForkResolver → TrustService | 2026-01-27 | Optional field, fallback preserved |
| TrustManager (gateway) → TrustService | 2026-01-27 | Optional field |
| ChallengeScheduler → TrustService | 2026-01-28 | Full migration |
| PolicySource → kernel TrustClass | 2026-01-28 | Type migration |
| IdentityHandle → kernel TrustClass | 2026-01-28 | Type migration |
| GossipDeps → PolicyOracle | 2026-01-27 | Removed TrustGraphOracle fallback |
| GovernanceService wired in icnd | 2026-01-30 | Daemon owns SledParameterStore lifecycle |
| LedgerService wired in icnd | 2026-01-30 | Daemon owns Ledger + SledStore lifecycle |
| MisbehaviorDetector → TrustService | 2026-01-30 | Full migration (#910) |
| Ledger → TrustService (no TrustGraph) | 2026-01-30 | icn-trust moved to dev-dep (#867) |
| OracleRegistry wired in supervisor | 2026-01-30 | Bootstrap phases, domain routing (#869) |
| Network rate limiting → OracleRegistry | 2026-01-30 | Wired OracleRegistry as PolicyOracle; functional when trust oracle registered (#869) |
| RPC PolicyOracle → OracleRegistry | 2026-01-30 | Passed to RpcDeps; rate-limiting wiring TBD (#869) |
| AllowAllOracle fallback removed | 2026-01-30 | Gossip uses OracleRegistry (#869) |

### 6.2 Remaining Work

| Component | Location | Status | Notes |
|-----------|----------|--------|-------|
| GatewayServer trust routes | icn-gateway | Partial | Some endpoints still use TrustGraph |
| ContractActor | icn-ccl | Needs TrustService | Still accepts `Option<Arc<RwLock<TrustGraph>>>` |
| icn-trust dep in icn-core | icn-core/Cargo.toml | #912 | Kernel crate still imports domain crate |
| icn-governance dep in icn-core | icn-core/Cargo.toml | #913 | Kernel crate still imports domain crate |
| icn-ledger dep in icn-core | icn-core/Cargo.toml | #914 | Kernel crate still imports domain crate |

### 6.3 Transition Handles

These `raw_handles` exist for components not yet migrated:

| Key | Type | Used By | Remove When |
|-----|------|---------|-------------|
| `TRUST_GRAPH_KEY` | `Arc<RwLock<TrustGraph>>` | MisbehaviorDetector, ContractActor | Components migrated to TrustService |
| `PROTOCOL_PARAM_STORE_KEY` | `Arc<SledParameterStore>` | Supervisor init_governance | Supervisor creates its own (standalone mode only) |
| `LEDGER_KEY` | `Arc<RwLock<Ledger>>` | Supervisor init_ledger | Supervisor creates its own (standalone mode only) |
| `LEDGER_STORE_KEY` | `Arc<SledStore>` | Supervisor init_ledger (DisputeManager, TreasuryManager) | Sled replaced or all stores created by daemon |

**Goal**: Remove all `raw_handles` when migration is complete.

---

## 7. Testing Patterns

### 7.1 Mock TrustService

```rust
/// For tests that don't need trust-gated behavior
pub struct MockTrustService {
    default_score: f64,
}

impl MockTrustService {
    pub fn new(default_score: f64) -> Self {
        Self { default_score }
    }
    
    pub fn fully_trusted() -> Self {
        Self::new(1.0)
    }
    
    pub fn untrusted() -> Self {
        Self::new(0.0)
    }
}

impl TrustService for MockTrustService {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        Arc::new(AllowAllOracle::default())
    }
    
    fn trust_score(&self, _actor: &Did) -> f64 {
        self.default_score
    }
    
    fn record_event(&self, _actor: &Did, _event: TrustEvent) {
        // No-op for tests
    }
}
```

### 7.2 AllowAllOracle

For tests that don't need authorization enforcement:

```rust
use icn_kernel_api::AllowAllOracle;

let deps = GossipDeps {
    policy_oracle: Some(Arc::new(AllowAllOracle::default())),
    trust_service: None,
    // ...
};
```

### 7.3 Integration Test Pattern

```rust
#[tokio::test]
async fn test_component_with_trust_service() {
    // Create mock trust service
    let trust_service = Arc::new(MockTrustService::new(0.5));
    
    // Create component with injected service
    let component = MyComponent::new()
        .with_trust_service(Some(trust_service));
    
    // Test behavior
    let result = component.do_something(&test_did).await;
    assert!(result.is_ok());
}
```

---

## 8. FAQ

### Q: Why not just pass TrustGraph everywhere?

**A**: Direct TrustGraph access creates tight coupling:
- Kernel code "knows" about trust class semantics
- Cannot swap trust implementations
- Testing requires real TrustGraph instances
- Violates separation of concerns

### Q: What if a component needs both TrustService and TrustGraph?

**A**: During transition, use both:
```rust
struct Component {
    trust_service: Option<Arc<dyn TrustService>>,  // Preferred
    trust_graph: Option<Arc<RwLock<TrustGraph>>>,  // Fallback
}
```

When fully migrated, remove `trust_graph`.

### Q: How do I know if my code crosses the Meaning Firewall?

**A**: Your code crosses the firewall when:
1. It converts domain types to kernel types (✅ correct direction)
2. It converts kernel types to domain types (❌ wrong direction)

Example of correct crossing:
```rust
// In app code:
let trust_score = graph.compute_trust_score(&did);
let constraints = ConstraintSet::new()
    .with_rate_limit(score_to_limit(trust_score));
// ↑ trust_score (domain) → rate_limit (kernel)
```

### Q: What's the difference between `icn_trust::TrustClass` and `icn_kernel_api::TrustClass`?

**A**: They have identical values but different purposes:
- `icn_trust::TrustClass` - Domain concept, used in trust graph operations
- `icn_kernel_api::TrustClass` - Kernel abstraction, used for policy decisions

Kernel code should only use `icn_kernel_api::TrustClass`.

### Q: When will raw_handles be removed?

**A**: When all components are migrated to use service traits. Target: Phase 20+.

---

## Appendix A: Store Path Structure

The daemon creates sled-backed stores under `config.store_path()` (default: `~/.icn/store/`):

| Path | Owner | Contents |
|------|-------|----------|
| `store/trust/` | Daemon → TrustGraph | Trust attestations, scores |
| `store/protocol_params/` | Daemon → GovernanceService | Protocol parameter values |
| `store/ledger/` | Daemon → Ledger + DisputeManager + TreasuryManager | Double-entry transactions, disputes |

Each path is opened exactly once by the daemon. The supervisor receives handles via `ServiceRegistry.raw_handle()` to avoid double-opening sled databases.

Helper methods on `Config`:
- `config.store_path()` → base store directory
- `config.protocol_params_path()` → `store/protocol_params/`
- `config.ledger_store_path()` → `store/ledger/`

## Appendix B: Key Files

| File | Purpose |
|------|---------|
| `icn/crates/icn-kernel-api/src/lib.rs` | Kernel API trait exports |
| `icn/crates/icn-kernel-api/src/authz.rs` | PolicyOracle, ConstraintSet |
| `icn/crates/icn-kernel-api/src/services.rs` | TrustService, ServiceRegistry |
| `icn/crates/icn-core/src/supervisor/lifecycle.rs` | Service injection |
| `icn/crates/icn-core/src/supervisor/init_trust.rs` | Trust initialization |
| `icn/crates/icn-core/src/supervisor/init_governance.rs` | Governance initialization |
| `icn/crates/icn-core/src/supervisor/init_ledger.rs` | Ledger initialization |
| `icn/crates/icn-gateway/src/trust_policy.rs` | TrustPolicyOracle implementation |
| `apps/trust/` | TrustService app implementation |
| `apps/governance/` | GovernanceService app implementation |
| `apps/ledger/` | LedgerService app implementation |
| `CLAUDE.md` | Agent guidance (Kernel/App section) |
