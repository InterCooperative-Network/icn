# 05: The Meaning Firewall — ICN's Central Design Principle

> **Phase**: 2 | **Tier**: Fixer  
> **Patterns introduced**: [PolicyOracle / Meaning Firewall](#policyoracle--meaning-firewall), [OracleRegistry](#oracleregistry), [Trust-Gated Operations](#trust-gated-operations)  
> **Prerequisite**: 04-actors-and-concurrency.md

## Why This Matters

**"ICN is a constraint engine: apps translate meaning into constraints; the kernel enforces constraints without understanding meaning."**

This is ICN's **central design principle**. It separates:
- **What to enforce** (kernel) from **why to enforce it** (apps)
- **Generic mechanisms** (rate limits, quotas) from **domain semantics** (trust scores, governance weights)
- **Predictable substrate** (kernel) from **adaptable policy** (cooperative governance)

The **Meaning Firewall** is the boundary between these worlds. Understanding it transforms how you read ICN code — it's not just "good architecture"; it's the invariant that makes cooperative self-governance possible without kernel rewrites.

→ See `manual.md` § "The Constraint Engine" for the full narrative.

## What You'll Read

### 1. The Boundary Contract: `icn-kernel-api/src/authz.rs`

This file defines the **interface** between kernel and apps.

#### **The PolicyOracle Trait**

```rust
#[async_trait]
pub trait PolicyOracle: Send + Sync {
    /// Evaluate a policy request and return a decision.
    async fn evaluate(&self, request: PolicyRequest) -> PolicyDecision;
}
```

**This is the firewall.** Everything on the kernel side of this trait is **domain-agnostic**. Everything on the app side is **domain-aware**.

#### **PolicyRequest: What the Kernel Asks**

```rust
pub struct PolicyRequest {
    pub requester: Did,           // Who is requesting?
    pub operation: Operation,      // What do they want to do?
    pub resource: Option<ResourceId>,  // On what resource?
    pub context: HashMap<String, String>,  // Any metadata
}

pub enum Operation {
    ReadTrust,
    WriteLedger,
    SubmitProposal,
    ExecuteTask,
    Custom(String),
}
```

**Key insight**: The kernel knows **operations** (generic verbs), not **meanings** (why they matter).

#### **PolicyDecision: What the Kernel Gets Back**

```rust
pub enum PolicyDecision {
    Allow { constraints: ConstraintSet },
    Deny { reason: PolicyError },
}

pub struct ConstraintSet {
    pub rate_limit: Option<RateLimit>,
    pub quota: Option<Quota>,
    pub expiry: Option<Timestamp>,
    pub custom: HashMap<String, f64>,
}
```

**Key insight**: The kernel receives **constraints** (numbers, limits, timeouts), not **reasoning** (trust scores, governance weights).

#### **What the Kernel Does NOT Understand**

From the doc comment (lines 18-24):

> The kernel does NOT understand:
> - Trust classes or scores
> - Governance weights
> - Credit multipliers
> - Membership status
>
> It only understands:
> - Allow with constraints
> - Deny with reason

**This is enforced at compile time**: Kernel crates have zero dependencies on domain crates.

### 2. Three Oracle Implementations Side-by-Side

Read these three files **in parallel** to see the pattern:

#### **`apps/trust/src/oracle.rs` — Trust Scores → Rate Limits**

```rust
impl PolicyOracle for TrustPolicyOracle {
    async fn evaluate(&self, request: PolicyRequest) -> PolicyDecision {
        // 1. Query domain-specific state
        let trust_score = self.trust_graph
            .read()
            .compute_trust(&request.requester, &target)?;
        
        // 2. Translate to constraints
        let constraints = self.trust_score_to_constraints(trust_score);
        
        // 3. Return generic decision
        PolicyDecision::allow_with(constraints)
    }
}

fn trust_score_to_constraints(&self, score: f64) -> ConstraintSet {
    // THIS IS THE FIREWALL BOUNDARY
    ConstraintSet {
        rate_limit: Some(RateLimit {
            max_requests: (score * 100.0) as u32,  // Higher trust = higher rate
            window: Duration::from_secs(60),
        }),
        ..Default::default()
    }
}
```

**The firewall**: `trust_score_to_constraints()` is the exact point where domain meaning (trust scores) becomes kernel constraints (rate limits). The kernel never sees the trust score.

#### **`apps/governance/src/oracle.rs` — Protocol Parameters → Quotas**

```rust
impl PolicyOracle for GovernancePolicyOracle {
    async fn evaluate(&self, request: PolicyRequest) -> PolicyDecision {
        // 1. Query governance parameters
        let param = self.params
            .read()
            .get_parameter("max_proposal_size")?;
        
        // 2. Translate to constraints
        let constraints = ConstraintSet {
            quota: Some(Quota {
                max_size: param.value as usize,
            }),
            ..Default::default()
        };
        
        // 3. Return generic decision
        PolicyDecision::allow_with(constraints)
    }
}
```

**The firewall**: The kernel enforces `max_size`, but has no idea it came from a governance vote. If governance changes the parameter, kernel enforcement automatically adapts.

#### **`apps/ledger/src/oracle.rs` — Credit State → Transaction Limits**

```rust
impl PolicyOracle for LedgerPolicyOracle {
    async fn evaluate(&self, request: PolicyRequest) -> PolicyDecision {
        // 1. Query ledger state
        let balance = self.ledger
            .read()
            .get_balance(&request.requester)?;
        
        let credit_limit = self.get_credit_limit(&request.requester)?;
        
        // 2. Translate to constraints
        let max_amount = credit_limit - balance;
        let constraints = ConstraintSet {
            custom: HashMap::from([
                ("max_transaction_amount".to_string(), max_amount as f64),
            ]),
            ..Default::default()
        };
        
        // 3. Return generic decision
        PolicyDecision::allow_with(constraints)
    }
}
```

**The firewall**: The kernel never sees "credit limits" or "balances" — it only enforces `max_transaction_amount` as a numerical constraint.

### 3. Firewall Violations and Correct Patterns

**File**: `docs/architecture/KERNEL_APP_SEPARATION.md`

#### **❌ VIOLATION: Kernel Reading Trust Scores Directly**

```rust
// In icn-net/src/actor.rs (WRONG)
use icn_trust::TrustGraph;  // ❌ Domain import in kernel

let trust_score = trust_graph.compute_trust(&peer_did)?;
if trust_score < 0.5 {
    return Err("Insufficient trust");
}
```

**Why this breaks the firewall**: The kernel (`icn-net`) now understands domain semantics (trust scores), making it impossible to change trust policy without kernel changes.

#### **✅ CORRECT: Oracle Translating Trust to Constraints**

```rust
// In icn-net/src/actor.rs (CORRECT)
let decision = self.policy_oracle.evaluate(PolicyRequest {
    requester: peer_did,
    operation: Operation::Custom("dial_peer".into()),
    ..Default::default()
}).await?;

match decision {
    PolicyDecision::Allow { constraints } => {
        // Enforce rate limit (generic constraint)
        if let Some(rate_limit) = constraints.rate_limit {
            self.rate_limiter.check(&peer_did, &rate_limit)?;
        }
    }
    PolicyDecision::Deny { reason } => {
        return Err(reason);
    }
}
```

**Why this preserves the firewall**: The kernel only sees `rate_limit` (generic). Trust policy can change without touching kernel code.

### 4. Compile-Time Enforcement: `icn-core/src/meaning_firewall.rs`

This file contains **CI tests** that scan kernel crates for forbidden domain imports:

```rust
#[test]
fn test_kernel_does_not_import_domain() {
    let kernel_crates = ["icn-core", "icn-net", "icn-gossip", "icn-store"];
    let domain_crates = ["icn-trust", "icn-ledger", "icn-governance", "icn-ccl"];
    
    for kernel in &kernel_crates {
        let cargo_toml = read_cargo_toml(kernel);
        for domain in &domain_crates {
            assert!(
                !cargo_toml.contains(domain),
                "Kernel crate {} imports domain crate {}",
                kernel, domain
            );
        }
    }
}
```

**Pinned violation counts**: If the test detects a new violation, CI fails. This makes firewall violations **visible and blocking**.

### 5. The OracleRegistry: Phase-Aware Bootstrap

**File**: `icn-kernel-api/src/bootstrap.rs`

The kernel needs oracles to start, but oracles depend on subsystems. The OracleRegistry solves this with **phased bootstrapping**:

```rust
pub struct OracleRegistry {
    oracles: Arc<DashMap<Domain, Arc<dyn PolicyOracle>>>,
    phase: Arc<ArcSwap<BootstrapPhase>>,
}

pub enum BootstrapPhase {
    Genesis,        // No oracles, fallback to AllowAllOracle
    CoreApps,       // Trust/Ledger loaded, limited functionality
    Running,        // Full system operational
}
```

**Atomic oracle replacement via ArcSwap**:

```rust
// During bootstrap
registry.register(Domain::Trust, Arc::new(TrustPolicyOracle::new()));

// Atomic swap to Running phase
registry.set_phase(BootstrapPhase::Running);
```

**Why ArcSwap**: Allows replacing oracles without locking. Old references remain valid (via `Arc`), new requests get the new oracle.

**Rust concepts inline**:
- `Arc<dyn PolicyOracle>`: Trait object for runtime polymorphism (different oracle types behind same interface)
- `ArcSwap`: Atomic replacement of `Arc` pointers without locks (from `arc-swap` crate)
- `DashMap`: Concurrent HashMap (from `dashmap` crate) for lock-free multi-threaded access
- `Domain` enum: Routes requests to the right oracle (Trust vs Governance vs Ledger)

### 6. Example: End-to-End Request Flow

**Trace this sequence**:

1. **Gateway**: User submits task via REST API  
   `POST /v1/compute/tasks`

2. **Gateway → Kernel**: Call `compute_actor.submit_task(task)`

3. **ComputeActor → PolicyOracle**: Request authorization  
   ```rust
   let decision = self.oracle.evaluate(PolicyRequest {
       requester: task.submitter,
       operation: Operation::ExecuteTask,
       context: HashMap::from([("task_type", "wasm")]),
       ..Default::default()
   }).await?;
   ```

4. **TrustPolicyOracle**: Query trust graph  
   ```rust
   let trust_score = self.trust_graph.compute_trust(&requester)?;
   ```

5. **TrustPolicyOracle → Kernel**: Translate to constraints  
   ```rust
   let constraints = ConstraintSet {
       rate_limit: Some(RateLimit {
           max_requests: (trust_score * 50.0) as u32,
           window: Duration::from_secs(60),
       }),
       custom: HashMap::from([
           ("max_cpu_ms", trust_score * 10000.0),
       ]),
   };
   PolicyDecision::allow_with(constraints)
   ```

6. **ComputeActor**: Enforce constraints  
   ```rust
   match decision {
       PolicyDecision::Allow { constraints } => {
           self.rate_limiter.check(&requester, &constraints.rate_limit)?;
           let max_cpu = constraints.custom.get("max_cpu_ms").unwrap();
           self.execute_task(task, *max_cpu as u64).await?;
       }
       PolicyDecision::Deny { reason } => {
           return Err(reason);
       }
   }
   ```

7. **Response**: Task accepted or rejected based on constraints (not trust score)

**At no point** does the kernel see trust scores, trust classes, or governance weights. It only sees constraints and enforces them.

## Patterns Introduced

### PolicyOracle / Meaning Firewall
**The central pattern**: Apps translate domain semantics into generic constraints the kernel enforces.

→ See `patterns.md` #11 for full template.

### OracleRegistry
**Phase-aware bootstrap**: Allows kernel to start before oracles are ready, then swap them in atomically.

→ See `patterns.md` #13 for full template.

### Trust-Gated Operations
**Trust as constraint source**: Trust scores → rate limits, quotas, resource allocations.

→ See `patterns.md` #8 for full template.

## What You'll Build

→ **Lab**: `labs/lab-04-firewall-oracle/` — **THE KEYSTONE LAB**

Build three crates:
1. **kernel/** — enforces constraints (rate limiter, permission gate). Only sees `ConstraintSet`. Has ZERO imports from `domain/`.
2. **domain/** — understands "reputation scores" (or any domain concept). Internal types the kernel never sees.
3. **oracle/** — implements `PolicyOracle` trait that translates domain semantics → generic constraints.

**Tests proving**:
- Kernel crate has zero imports from domain crate (compile-time check)
- Oracle correctly translates domain state to constraints
- Changing oracle behavior changes kernel enforcement without kernel code changes

**Done when**: Kernel has zero domain dependencies, oracle imports both, tests prove boundary holds.

## Checkpoint

You've completed this layer when you can:

1. **Trace end-to-end**: Diagram a request from Gateway → Kernel → PolicyOracle → ConstraintSet → enforcement → response
2. **Identify violations**: Given a code snippet with a Meaning Firewall violation, identify and fix it
3. **Explain the boundary**: Describe why `icn-net` cannot import `icn-trust`, and how it gets trust-related constraints anyway
4. **Lock choice rationale**: Explain why `oracle.rs` uses `parking_lot::RwLock` while `oracle_tokio.rs` uses `tokio::sync::RwLock`

**Artifact**: Complete lab-04 and submit:
1. Proof that `kernel/Cargo.toml` has no `domain` dependency
2. Test output showing oracle translation working
3. Diagram showing request → oracle → constraints → enforcement flow

## Deep Reference

→ `reference/module-02-architecture-overview.md` — Full architecture with diagrams  
→ `docs/architecture/KERNEL_APP_SEPARATION.md` — Detailed firewall examples and anti-patterns  
→ `icn-kernel-api/src/authz.rs` — Source of truth for the PolicyOracle interface  
→ `icn-kernel-api/src/bootstrap.rs` — OracleRegistry implementation
