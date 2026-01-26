# Phase 2 Trust Extraction - Assessment & Path Forward

## Current PR Status: REJECT

PR #882 contains a fundamental architectural violation that cannot be salvaged through incremental fixes.

## The Critical Flaw: Reverse Meaning Firewall

### What Went Wrong

The implementation maps PolicyOracle `ConstraintSet` back to domain type `TrustClass`:

```rust
// ❌ ARCHITECTURAL VIOLATION
let trust_class = match constraints.max_topics {
    Some(n) if n >= 500 => TrustClass::Federated,
    Some(n) if n >= 100 => TrustClass::Partner,
    Some(n) if n >= 25 => TrustClass::Known,
    _ => TrustClass::Isolated,
};
```

### Why This Is Wrong

The meaning firewall exists to prevent kernel code from understanding domain semantics:

```
Trust App (Domain)          PolicyOracle (Firewall)         Kernel (Mechanics)
------------------          -----------------------         ------------------
TrustGraph                       ↓                                ↓
compute_trust_score()      score_to_constraints()         constraints only
TrustClass semantics       ConstraintSet (generic)        NO domain knowledge
                                ↓
                          [FIREWALL BOUNDARY]
                                ↓
                          Kernel NEVER sees:
                          - TrustClass
                          - Trust scores
                          - Why constraints exist
```

By reconstructing `TrustClass` from `max_topics`, the kernel is reverse-engineering domain semantics, which:
1. Defeats the abstraction
2. Couples kernel to trust app internals
3. Creates hidden dependencies on constraint value ranges
4. Makes it impossible to change trust scoring without breaking kernel code

## Infection Count

Current `TrustClass`/`TrustGraph` references in affected crates:

```bash
icn-gossip:  30+ references (including tests)
icn-net:     ~20 references
icn-gateway: ~15 references  
icn-ledger:  ~5 references
```

## The Correct Approach

### 1. Refactor AccessControl Enum (icn-gossip)

**Current (Wrong)**:
```rust
pub enum AccessControl {
    Open,
    TrustClass(TrustClass),  // ❌ Domain type in kernel
    Allowlist(HashSet<Did>),
}
```

**Correct**:
```rust
pub enum AccessControl {
    Open,
    MinTopics(u32),          // ✅ Generic constraint
    Allowlist(HashSet<Did>),
}
```

### 2. Use Constraints Directly (All Kernel Crates)

**Wrong**:
```rust
let trust_class = map_constraints_to_class(constraints);  // ❌
let limit = TrustResourceLimits::for_trust_class(trust_class);
```

**Correct**:
```rust
let max_topics = constraints.max_topics.unwrap_or(25);  // ✅
if current_topics >= max_topics {
    return Err(LimitExceeded);
}
```

### 3. Eliminate TrustClass from Kernel Types

Rename and refactor:
- `TrustResourceLimits` → `ResourceLimits` (remove TrustClass field)
- `trust_class: TrustClass` → use constraint values directly
- Helper functions take `&ConstraintSet`, not `TrustClass`

## Implementation Order (Correct)

### Phase 2.2a: icn-gossip
**Prerequisite**: Refactor `AccessControl` enum first
- Remove all `TrustClass` references
- Update `can_subscribe()` / `can_publish()` to use `MinTopics(u32)`
- Replace `topics_per_peer_limit(TrustClass)` with `topics_per_peer_limit(&ConstraintSet)`
- Update all tests to use `AllowAllOracle` or mock oracle

### Phase 2.2b: icn-ledger  
- Replace `TrustGraph::compute_trust_score()` with `oracle.evaluate()`
- Extract `credit_multiplier` from `ConstraintSet`
- Never reconstruct trust scores from multiplier

### Phase 2.2c: icn-net
- Replace `trust_graph` field with `policy_oracle`
- Extract `RateLimit` from `ConstraintSet` directly
- Update `RateLimitConfig` to not reference `TrustClass`

### Phase 2.2d: icn-gateway
- Remove hardcoded thresholds (0.7, 0.4, 0.1)
- Use `constraints.rate_limit` directly
- No manual trust-to-rate mapping

### Phase 2.3: icn-core supervisor
- Wire `AllowAllOracle` during bootstrap
- Load trust app and register `TrustPolicyOracle`
- Pass oracle to all actors

## Anti-Patterns to Avoid

### ❌ Never Do This

1. **Reverse mapping**: `constraints → TrustClass`
2. **Temporary bridges**: They become permanent
3. **Hardcoded thresholds**: `if score >= 0.7`
4. **Domain type reconstruction**: Inferring semantics from constraints

### ✅ Always Do This

1. **Use constraints directly**: `constraints.max_topics`
2. **Default gracefully**: `.unwrap_or(default_value)`
3. **Refactor kernel types**: Remove domain type fields
4. **Test with mock oracles**: `AllowAllOracle`, `DenyAllOracle`

## Verification Criteria

After proper implementation:

```bash
# No TrustClass in kernel crates
grep -r 'TrustClass' icn/crates/icn-{gossip,net,gateway,ledger}/src \
  && echo 'FAIL: Domain types in kernel' \
  || echo 'PASS: Meaning firewall intact'

# No TrustGraph in kernel crates  
grep -r 'TrustGraph' icn/crates/icn-{gossip,net,gateway,ledger}/src \
  && echo 'FAIL: Direct graph access' \
  || echo 'PASS: Oracle abstraction enforced'

# Tests compile and pass
cargo test -p icn-gossip -p icn-net -p icn-ledger -p icn-gateway
```

## Lessons Learned

### For Future PRs

Add to project rules:

1. **Code review checklist**: Reject PRs that map constraints to domain types
2. **Architecture doc**: Document the meaning firewall with examples
3. **Test patterns**: Provide templates for oracle-based tests
4. **Incremental refactoring**: Refactor kernel types before replacing trust lookups

### Why This Failed

1. **Incomplete refactoring**: Changed lookup mechanism but not the kernel types
2. **Expedience over correctness**: "Temporary bridge" seemed faster than proper refactor
3. **Missing big picture**: Focused on removing fields, not removing domain knowledge

## Recommendation

**Close PR #882** and restart with proper approach:

1. Create sub-issues (2.2a through 2.3) 
2. Start with icn-gossip AccessControl refactor
3. Ensure each crate passes firewall verification before moving to next
4. Update project rules with anti-patterns
5. Final integration test with supervisor wiring

## References

- Issue #857: Phase 2 - Trust Extraction
- `apps/trust/src/oracle.rs`: Correct oracle implementation
- `icn-kernel-api/src/authz.rs`: PolicyOracle trait definition
