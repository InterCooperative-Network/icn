# Gate 2: Becoming a Fixer

> **Status**: Promotion gate to **Fixer** tier  
> **Required**: Complete Layers 04-06  
> **Time estimate**: 2-3 weeks (Foundations track) or 1 week (Accelerated track)

## What You've Learned

In Phase 2, you mastered **ICN's architecture**:
- Actors: ICN's central concurrency model
- **The Meaning Firewall**: The boundary separating kernel from apps
- Persistence: Storage as time-communication with invariants

You can now **fix bugs**, write tests, add observability, and understand architectural constraints.

## Requirements

Complete **all four** artifacts below. Each demonstrates architectural understanding.

### Artifact 1: Lab-04 Complete (Keystone)

**What**: Implement the Meaning Firewall lab with three crates (kernel, domain, oracle).

**Requirements**:
- `kernel/` crate enforces generic constraints (rate limiter, permission gate)
- `domain/` crate defines domain semantics (reputation scores, membership status, etc.)
- `oracle/` crate implements `PolicyOracle` trait, translating domain → constraints
- `kernel/Cargo.toml` has **zero** dependencies on `domain/`
- Tests prove: oracle translation works, kernel enforces constraints, changing oracle changes behavior without kernel changes

**Verification**: Submit your lab code + test output showing all three proofs pass.

---

### Artifact 2: End-to-End Trace Diagram

**What**: Draw a complete request flow from Gateway → Kernel → PolicyOracle → ConstraintSet → enforcement → response.

**Requirements**:
- Show all components: Gateway API, Actor (e.g., ComputeActor), PolicyOracle, Kernel enforcement
- Label the **firewall crossing**: where domain semantics (trust scores) become generic constraints (rate limits)
- Include actual data values (e.g., "trust_score: 0.7" → "rate_limit: 70 req/min")
- Show both success path (Allow with constraints) and failure path (Deny with reason)

**Example structure**:
```
Gateway API
    ↓ POST /compute/tasks
ComputeActor
    ↓ PolicyRequest { requester: Alice, operation: ExecuteTask }
TrustPolicyOracle
    ↓ compute_trust(Alice) → 0.8
    ↓ trust_to_constraints(0.8) → ConstraintSet { rate_limit: 80/min }
    ↓ PolicyDecision::Allow { constraints }
ComputeActor
    ↓ enforce rate_limit (80/min)
    ↓ execute task
    ↓ Response { status: "accepted", task_id: "..." }
Gateway API
```

**Verification**: Can you explain what happens if the oracle changes `trust_to_constraints()` logic?

---

### Artifact 3: Boundary Review Exercise

**What**: Identify and fix a Meaning Firewall violation.

**Given**: This code snippet (hypothetical violation):

```rust
// File: icn/crates/icn-compute/src/actor.rs
use icn_trust::TrustGraph;  // ❌ Import

pub struct ComputeActor {
    trust_graph: Arc<RwLock<TrustGraph>>,  // ❌ Direct access
    // ...
}

impl ComputeActor {
    pub async fn submit_task(&self, task: Task) -> Result<()> {
        let trust_score = self.trust_graph
            .read()
            .await
            .compute_trust(&task.submitter)?;  // ❌ Direct query
        
        if trust_score < 0.5 {  // ❌ Hardcoded threshold
            return Err("Insufficient trust");
        }
        
        self.execute(task).await
    }
}
```

**Task**: Rewrite this to use the PolicyOracle pattern, removing the firewall violation.

**Requirements**:
- Remove `icn_trust` import
- Replace direct trust graph access with PolicyOracle
- Move trust threshold logic to the oracle
- Kernel enforces generic constraints, not trust scores

**Verification**: Submit your fixed code + explanation of what changed and why.

---

### Artifact 4: Lock Choice Rationale

**What**: Explain when to use `parking_lot::RwLock` vs `tokio::sync::RwLock`.

**Questions**:
1. Why does `apps/trust/src/oracle.rs` use `parking_lot::RwLock`?
2. Why does `apps/trust/src/oracle_tokio.rs` use `tokio::sync::RwLock`?
3. What would break if you swapped them?
4. When would you choose each for a new actor?

**Requirements**:
- Explain trade-offs: synchronous vs async, performance vs flexibility
- Give concrete example of when each is appropriate
- Reference ICN code that uses each pattern

**Verification**: Your explanation demonstrates understanding of Rust async concurrency.

---

## After Passing

**Congratulations!** You are now a **Fixer**.

You can:
- ✅ Fix bugs in actors, oracles, and storage layers
- ✅ Write tests for concurrent and distributed behavior
- ✅ Add tracing/metrics to improve observability
- ✅ Ship bugfix PRs that respect architectural boundaries
- ✅ Review PRs for Meaning Firewall violations

**Next step**: Proceed to **Phase 3: Systems** to become a **Builder**.

---

## Submission

Submit your four artifacts to your mentor or in the #onboarding channel:
1. Lab-04 code + test output (proof of firewall enforcement)
2. End-to-end trace diagram (showing firewall crossing)
3. Fixed code from boundary review exercise + explanation
4. Lock choice rationale (written explanation)

Once reviewed and approved, you'll receive the **Fixer** badge and can continue to Phase 3.

---

## Need Help?

- **Actor confusion?** → `reference/module-03-runtime-actors.md`
- **Firewall unclear?** → `docs/architecture/KERNEL_APP_SEPARATION.md`
- **Ledger questions?** → `reference/module-06-ledger-contracts.md`

Ask questions in #onboarding — understanding these concepts deeply is critical for Phase 3!
