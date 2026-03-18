# Lab 04: Firewall Oracle — THE KEYSTONE LAB

## Goal
Prove the Meaning Firewall boundary by building kernel, domain, and oracle crates.

> Note: Code snippets in this lab are intentionally simplified for learning.
> When mapping back to ICN, use `icn/crates/icn-kernel-api/src/authz.rs` as
> the source of truth (`PolicyRequestCore`, `PolicyContext`, sync
> `PolicyOracle::evaluate(&PolicyRequest) -> PolicyDecision`, optional async
> override).

## Structure
```
lab-04-firewall-oracle/
├── Cargo.toml          # Workspace
├── kernel/
│   ├── Cargo.toml      # NO domain dependency
│   └── src/lib.rs      # Enforces constraints
├── domain/
│   ├── Cargo.toml
│   └── src/lib.rs      # Domain semantics (reputation, membership, etc.)
└── oracle/
    ├── Cargo.toml      # Depends on kernel-api AND domain
    └── src/lib.rs      # Implements PolicyOracle
```

## Requirements

### 1. Kernel Crate (`kernel/`)
```rust
pub struct ConstraintSet {
    pub rate_limit: Option<u32>,
    pub quota: Option<u64>,
}

pub struct RateLimiter {
    limits: HashMap<String, (u32, Instant)>,
}

impl RateLimiter {
    pub fn check(&mut self, id: &str, limit: u32) -> Result<(), String> {
        // Enforce rate limit, return error if exceeded
    }
}
```

**CRITICAL**: `kernel/Cargo.toml` has NO dependency on `domain`.

### 2. Domain Crate (`domain/`)
```rust
pub struct ReputationScore {
    pub user_id: String,
    pub score: f64,  // 0.0 to 1.0
}

pub struct ReputationStore {
    scores: HashMap<String, f64>,
}

impl ReputationStore {
    pub fn get_score(&self, user_id: &str) -> f64 {
        self.scores.get(user_id).copied().unwrap_or(0.5)
    }
}
```

**Note**: Kernel NEVER sees this code.

### 3. Oracle Crate (`oracle/`)
```rust
pub trait PolicyOracle {
    fn evaluate(&self, user_id: &str) -> ConstraintSet;
}

pub struct ReputationOracle {
    reputation: ReputationStore,  // Domain type
}

impl PolicyOracle for ReputationOracle {
    fn evaluate(&self, user_id: &str) -> ConstraintSet {
        let score = self.reputation.get_score(user_id);
        
        // THIS IS THE FIREWALL BOUNDARY
        ConstraintSet {
            rate_limit: Some((score * 100.0) as u32),
            quota: Some((score * 1000.0) as u64),
        }
    }
}
```

## Tests to Write

### Test 1: Compile-Time Boundary
```rust
#[test]
fn test_kernel_has_no_domain_dependency() {
    let kernel_toml = std::fs::read_to_string("kernel/Cargo.toml").unwrap();
    assert!(!kernel_toml.contains("domain"));
}
```

### Test 2: Oracle Translation
```rust
#[test]
fn test_oracle_translates_correctly() {
    let mut store = ReputationStore::new();
    store.set_score("alice", 0.8);
    
    let oracle = ReputationOracle::new(store);
    let constraints = oracle.evaluate("alice");
    
    assert_eq!(constraints.rate_limit, Some(80));
    assert_eq!(constraints.quota, Some(800));
}
```

### Test 3: Kernel Enforcement
```rust
#[test]
fn test_kernel_enforces_constraints() {
    let mut limiter = RateLimiter::new();
    
    // Allow 5 requests
    for _ in 0..5 {
        assert!(limiter.check("user1", 5).is_ok());
    }
    
    // 6th request fails
    assert!(limiter.check("user1", 5).is_err());
}
```

### Test 4: Oracle Change Affects Enforcement
```rust
#[test]
fn test_oracle_change_changes_enforcement() {
    let mut store = ReputationStore::new();
    store.set_score("bob", 0.5);
    
    let oracle = ReputationOracle::new(store);
    let constraints_before = oracle.evaluate("bob");
    
    // Change reputation (in real system, via governance or behavior)
    oracle.reputation.set_score("bob", 0.9);
    let constraints_after = oracle.evaluate("bob");
    
    assert!(constraints_after.rate_limit > constraints_before.rate_limit);
}
```

## Done When
- Kernel crate compiles with ZERO domain imports (Test 1 passes)
- Oracle correctly translates domain state to constraints (Test 2 passes)
- Kernel enforces constraints without knowing domain (Test 3 passes)
- Changing oracle changes enforcement without kernel changes (Test 4 passes)

## Why This Matters
This lab proves ICN's central design principle: **apps translate meaning into constraints; the kernel enforces constraints without understanding meaning.**

If you understand this lab, you understand ICN's architecture.

## Mapping to ICN
- Lab `PolicyOracle::evaluate(&str)` maps to ICN `evaluate(&PolicyRequest)`.
- Lab `ConstraintSet { rate_limit, quota }` maps to ICN `ConstraintSet` in `authz.rs`.
- Lab `domain` crate maps to app/domain logic that translates semantics into kernel constraints.

## Resources
- `icn/crates/icn-kernel-api/src/authz.rs`
- `icn/crates/icn-gateway/src/trust_mgr.rs`
- `docs/architecture/KERNEL_APP_SEPARATION.md`
