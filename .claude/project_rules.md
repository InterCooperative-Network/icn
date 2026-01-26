# ICN Development Rules

## Architectural Rules

### Rule 1: Meaning Firewall
- Kernel crates MUST NOT import `icn-trust`, `icn-governance`, `icn-ccl` or any domain-specific crate
- Apps import domain crates and expose generic `ConstraintSet` to kernel
- Run firewall check: `grep -r 'use icn_trust::' crates/icn-{net,gateway,gossip,ledger}/src && exit 1`

### Rule 2: PolicyOracle is Synchronous
- `PolicyOracle::evaluate()` is sync by design
- Use `parking_lot::RwLock` (not `tokio::sync::RwLock`) for app state accessed in evaluate()
- Tech debt tracked in #874 for async migration

### Rule 3: Reducer Purity
- Reducers receive immutable `StateSnapshot`
- Reducers return state delta, not mutated state
- Reducers have NO access to async runtime, network, or time

### Rule 4: Bootstrap Security
- Genesis capabilities expire after 60 seconds
- Running phase denies requests for unregistered domains
- Never allow permanent backdoors

## Code Review Checklist

- [ ] No domain imports in kernel crates
- [ ] PolicyOracle returns only generic constraints (no trust_score in custom)
- [ ] Reducers are pure (no async, no side effects)
- [ ] Error handling: log before fallback, never silent failures
- [ ] Tests cover error paths, not just happy path
- [ ] TTL/cache values have documented security trade-offs

## Issue Labels

- `kernel-api`: Changes to kernel primitive traits
- `core`: Runtime, supervisor, dispatcher
- `trust`: Trust graph and PolicyOracle
- `ccl`: Cooperative Contract Language
- `meaning-firewall`: Violations of kernel/app separation

## PR Conventions

- Title: `feat|fix|refactor(scope): description`
- Scope: `kernel-api`, `core`, `trust-app`, `ccl`, `net`, `gateway`, `gossip`, `ledger`
- Co-author: Include `Co-Authored-By: claude` for AI-assisted commits
- Squash merge for feature PRs
## Anti-Patterns to Avoid

### Anti-Pattern 1: Reverse Meaning Firewall
**WRONG**: Converting generic constraints back to domain types

```rust
// ❌ NEVER DO THIS - Reconstructing domain semantics from constraints
let trust_class = match constraints.max_topics {
    Some(n) if n >= 500 => TrustClass::Federated,
    Some(n) if n >= 100 => TrustClass::Partner,
    Some(n) if n >= 25 => TrustClass::Known,
    _ => TrustClass::Isolated,
};
```

**CORRECT**: Use constraints directly

```rust
// ✅ CORRECT - Use generic constraint values directly
let max_topics = constraints.max_topics.unwrap_or(25);
if subscription_count >= max_topics {
    return Err(SubscriptionLimitExceeded);
}
```

**Why this matters**: The meaning firewall exists to prevent kernel code from understanding domain semantics. By reconstructing `TrustClass` from `max_topics`, you're:
1. Defeating the abstraction layer
2. Coupling kernel to trust app internals
3. Creating hidden dependencies on constraint value ranges
4. Making it impossible to change trust scoring without breaking kernel

### Anti-Pattern 2: "Temporary Bridge" Patterns

**WRONG**: Creating adapters that map between constraint and domain types

```rust
// ❌ NEVER DO THIS - "Temporary" bridges become permanent technical debt
fn bridge_constraints_to_trust(constraints: &ConstraintSet) -> TrustClass {
    // This will be "temporary" (it won't be)
    match constraints.max_topics {
        Some(n) if n >= 500 => TrustClass::Federated,
        // ...
    }
}
```

**CORRECT**: Refactor kernel code to eliminate domain types

```rust
// ✅ CORRECT - Refactor kernel types to not need domain types
pub enum AccessControl {
    Open,
    MinTopics(u32),  // Generic constraint, not TrustClass
    Allowlist(HashSet<Did>),
}
```

**When you think you need a bridge**: You don't need a bridge, you need to refactor the kernel type. The kernel should never reference domain types, period.

### Anti-Pattern 3: Hardcoded Domain Thresholds

**WRONG**: Hardcoding domain-specific threshold values in kernel code

```rust
// ❌ NEVER DO THIS - Domain knowledge in kernel
if trust_score >= 0.7 {
    rate_limit = RateLimit::unlimited();
} else if trust_score >= 0.4 {
    rate_limit = RateLimit::standard();
} else if trust_score >= 0.1 {
    rate_limit = RateLimit::throttled();
} else {
    rate_limit = RateLimit::restricted();
}
```

**CORRECT**: Extract constraints from oracle decision

```rust
// ✅ CORRECT - Kernel sees only generic constraints
let decision = policy_oracle.evaluate(&request);
let rate_limit = decision
    .constraints()
    .and_then(|c| c.rate_limit)
    .unwrap_or(RateLimit::restricted());
```

**Why this matters**: The values 0.7, 0.4, 0.1 are trust-domain semantics. They should exist ONLY in the trust app's `score_to_constraints()` function, never in kernel code.

### Anti-Pattern 4: Domain Type Fields in Kernel Structs

**WRONG**: Including domain types in kernel data structures

```rust
// ❌ NEVER DO THIS
pub struct TrustResourceLimits {
    trust_class: TrustClass,  // Domain type in kernel
    max_topics: usize,
    rate_limit: RateLimit,
}
```

**CORRECT**: Use only generic constraint values

```rust
// ✅ CORRECT
pub struct ResourceLimits {
    max_topics: usize,
    rate_limit: RateLimit,
    max_message_size: usize,
    // No domain types
}
```

## Code Review Red Flags

**Reject any PR that:**

1. ❌ Maps `ConstraintSet` fields back to domain enums (`TrustClass`, `GovernanceRole`, `MembershipTier`, etc.)
2. ❌ Adds domain crate imports to kernel crates (`use icn_trust::`, `use icn_governance::` in icn-net/gateway/gossip/ledger)
3. ❌ Introduces hardcoded domain-specific thresholds in kernel code (`if score >= 0.7`, `if tier == "premium"`)
4. ❌ Creates "bridge" or "adapter" layers between constraints and domain types
5. ❌ Has domain type fields in kernel struct definitions
6. ❌ Includes comments like "temporary bridge" or "will refactor later"

**The correct pattern is always:**
```rust
let decision = oracle.evaluate(&request);
let value = decision.constraints()
    .and_then(|c| c.specific_constraint)
    .unwrap_or(sensible_default);
```

## Architecture: The Meaning Firewall

```
┌─────────────────┐       ┌──────────────┐       ┌─────────────────┐
│  Domain App     │       │ PolicyOracle │       │  Kernel Crate   │
│  (trust)        │──────▶│  (firewall)  │──────▶│  (gossip/net)   │
└─────────────────┘       └──────────────┘       └─────────────────┘
                                 │
TrustGraph                       │                Constraints only
compute_trust_score()            │                max_topics: u32
TrustClass enum           score_to_constraints()  rate_limit: RateLimit
Domain semantics          ConstraintSet (generic) NO domain knowledge
                                 │
                    ══════════════════════════════
                         MEANING FIREWALL
                    ══════════════════════════════
                                 │
                          Kernel NEVER sees:
                          • TrustClass
                          • Trust scores  
                          • Why constraints exist
                          • Domain-specific logic
```

**Key principle**: Information flows ONE WAY across the firewall. Domain → Constraints. Never Constraints → Domain.

## Testing with Oracles

When writing kernel tests, use mock oracles:

```rust
use icn_kernel_api::authz::{AllowAllOracle, DenyAllOracle, PolicyOracle};

#[test]
fn test_subscription_with_high_limits() {
    let oracle = Arc::new(AllowAllOracle::wildcard());
    let mut gossip = GossipActor::new(did, oracle);
    // Test kernel logic with permissive constraints
}

#[test]  
fn test_subscription_denial() {
    let oracle = Arc::new(DenyAllOracle::new(
        Domain::trust(),
        "test lockdown"
    ));
    let mut gossip = GossipActor::new(did, oracle);
    // Test kernel logic with denial
}

// For custom constraints, create a test oracle:
struct TestOracle {
    max_topics: u32,
}

impl PolicyOracle for TestOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision::allow_with(
            ConstraintSet::new().with_max_topics(self.max_topics)
        )
    }
    
    fn domain(&self) -> Domain { Domain::trust() }
}
```

**Never** create mock trust graphs or trust classes in kernel tests. Always test through the oracle interface.
