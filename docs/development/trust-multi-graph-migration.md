# Trust Multi-Graph Migration Guide

This guide documents how to migrate ICN components from the single `TrustGraph` to the new multi-graph architecture introduced in Phase 21.

## Overview

ICN now uses three orthogonal trust graphs to prevent any single clique from gaining cross-domain influence:

| Graph Type | Purpose | Scoring Weights | Key Consumers |
|------------|---------|-----------------|---------------|
| **Social** | "I know you, we've worked together" | 60% direct / 40% transitive | icn-net, icn-gossip, icn-governance |
| **EconomicReliability** | "You clear obligations consistently" | 80% direct / 20% transitive | icn-ledger, icn-federation |
| **TechnicalReliability** | "Your node behaves correctly" | 90% direct / 10% transitive | icn-compute, icn-security |

## Migration Strategies

### Strategy 1: Drop-in Replacement (Zero Code Changes)

Replace `TrustGraph` with `TrustGraphFacade`. All existing code continues to work:

```rust
// Before
use icn_trust::TrustGraph;
let trust = TrustGraph::new(store, own_did);

// After (no other changes needed)
use icn_trust::TrustGraphFacade;
let trust = TrustGraphFacade::new(store, own_did);
```

The facade provides:
- `add_edge()` → Routes to appropriate graph based on `edge.graph_type`
- `compute_trust_score()` → Returns combined score (50% social + 30% economic + 20% technical)
- `trust_class()` → Uses combined score

### Strategy 2: Typed Access (Recommended for New Code)

Use typed access for domain-appropriate scoring:

```rust
use icn_trust::{TrustGraphFacade, TrustGraphType};

let facade = TrustGraphFacade::new(store, own_did);

// Domain-specific scoring
let credit_score = facade.economic().compute_trust_score(&member)?;
let node_reliability = facade.technical().compute_trust_score(&executor)?;
let social_trust = facade.social().compute_trust_score(&peer)?;
```

### Strategy 3: Direct MultiTrustGraph (Advanced)

For components that need full control:

```rust
use icn_trust::{MultiTrustGraph, TrustGraphType, TrustEdge};

let mut multi = MultiTrustGraph::new(store, own_did);

// Add typed edges
multi.economic_mut().add_edge(
    TrustEdge::new_typed(alice, bob, 0.8, TrustGraphType::EconomicReliability)
)?;

// Query specific graphs
let score = multi.economic().compute_trust_score(&target)?;
```

---

## Consumer Migration Reference

### icn-ledger (Priority: HIGH)

**Current**: Uses combined trust score for credit limits.

**Target**: Use `EconomicReliability` graph for credit decisions.

```rust
// Before
let trust_score = trust_graph.compute_trust_score(&member)?;
let credit_limit = baseline + (baseline * trust_score * multiplier);

// After
let econ_score = trust.economic().compute_trust_score(&member)?;
let credit_limit = baseline + (baseline * econ_score * multiplier);
```

**Files to update**:
- `icn-ledger/src/credit_policy.rs` - Credit limit calculations
- `icn-ledger/src/dispute.rs` - Dispute weighting

**Evidence sources for EconomicReliability**:
- Cleared transactions count
- On-time payment rate
- Dispute outcomes (wins/losses)
- Debt-to-credit ratio

---

### icn-compute (Priority: HIGH)

**Current**: Uses combined trust for task scheduling thresholds.

**Target**: Use `TechnicalReliability` graph for executor selection.

```rust
// Before
let trust = (self.trust_callback)(&task.submitter);
if trust < MIN_TRUST_SUBMIT { return Err(...) }

// After
let tech_trust = trust.technical().compute_trust_score(&task.submitter)?;
if tech_trust < MIN_TRUST_SUBMIT { return Err(...) }
```

**Files to update**:
- `icn-compute/src/actor.rs` - Task submission/claiming thresholds
- `icn-compute/src/scheduler.rs` - Executor selection scoring

**Evidence sources for TechnicalReliability**:
- Node uptime percentage
- Task completion rate
- Byzantine violation history
- Resource usage accuracy

---

### icn-net (Priority: MEDIUM)

**Current**: Uses trust for rate limiting decisions.

**Target**: Use `Social` graph for connection priority and rate limits.

```rust
// Before
let trust_score = trust_graph.compute_trust_score(&peer_did)?;
let rate_limit = calculate_rate_limit(trust_score);

// After
let social_score = trust.social().compute_trust_score(&peer_did)?;
let rate_limit = calculate_rate_limit(social_score);
```

**Files to update**:
- `icn-net/src/rate_limit.rs` - Trust-based rate limiting
- `icn-net/src/actor.rs` - Connection acceptance

**Evidence sources for Social**:
- Peer endorsements
- Shared governance participation
- Organization membership
- Community activity

---

### icn-gossip (Priority: MEDIUM)

**Current**: Uses trust for topic access control.

**Target**: Use `Social` graph for topic gating.

```rust
// Before
let trust = self.trust_graph.compute_trust_score(&subscriber)?;
if trust < topic.min_trust { return Err(AccessDenied) }

// After
let social_trust = self.trust.social().compute_trust_score(&subscriber)?;
if social_trust < topic.min_trust { return Err(AccessDenied) }
```

**Files to update**:
- `icn-gossip/src/gossip.rs` - Topic subscription access control
- `icn-gossip/src/acl.rs` - Access control list evaluation

---

### icn-security (Priority: MEDIUM)

**Current**: Penalties affect unified trust score.

**Target**: Byzantine violations should penalize `TechnicalReliability`.

```rust
// Before
trust_graph.update_score(&violator, current_score * 0.5)?;

// After
// Penalty affects Technical graph specifically
trust.technical_mut().add_edge(
    TrustEdge::new_typed(own_did, violator, 0.0, TrustGraphType::TechnicalReliability)
)?;
```

**Files to update**:
- `icn-security/src/misbehavior.rs` - Violation penalties

**Evidence sources for TechnicalReliability penalties**:
- Invalid signature attempts
- Replay attacks detected
- Conflicting signed statements
- Excessive resource consumption

---

### icn-governance (Priority: LOW)

**Current**: Uses trust for membership resolution.

**Target**: Use `Social` graph for governance membership.

```rust
// Before
let trusted_dids = trust_graph.get_dids_above_threshold(0.3)?;

// After
let trusted_dids = trust.social().get_dids_above_threshold(0.3)?;
```

**Files to update**:
- `icn-governance/src/membership.rs` - Trust-based membership resolution

---

### icn-federation (Priority: LOW)

**Current**: Federation attestations use single trust context.

**Target**: Map `TrustContext` to appropriate `TrustGraphType`.

```rust
// Mapping from federation TrustContext to TrustGraphType
fn map_context_to_graph(ctx: TrustContext) -> TrustGraphType {
    match ctx {
        TrustContext::Social => TrustGraphType::Social,
        TrustContext::Economic => TrustGraphType::EconomicReliability,
        TrustContext::Governance => TrustGraphType::Social, // Derived from social
        TrustContext::General => TrustGraphType::Social,    // Default to social
    }
}
```

**Files to update**:
- `icn-federation/src/attestation.rs` - Trust attestation mapping

---

### icn-rpc (Priority: LOW)

**Current**: Trust RPC returns single score.

**Target**: Expose all three graph types via RPC.

```rust
// New RPC methods
trust.get_score(did, graph_type: Option<TrustGraphType>) -> f64
trust.get_all_scores(did) -> { social: f64, economic: f64, technical: f64 }
```

**Files to update**:
- `icn-rpc/src/trust.rs` - Trust query handlers

---

### icn-core (Priority: HIGH)

**Current**: Supervisor creates single `TrustGraph`.

**Target**: Supervisor creates `TrustGraphFacade` or `MultiTrustGraph`.

```rust
// Before
let trust_graph = Arc::new(RwLock::new(TrustGraph::new(store.clone(), own_did.clone())));

// After (Option A: Facade for gradual migration)
let trust = Arc::new(RwLock::new(TrustGraphFacade::new(store.clone(), own_did.clone())));

// After (Option B: Direct multi-graph)
let trust = Arc::new(RwLock::new(MultiTrustGraph::new(store.clone(), own_did.clone())));
```

**Files to update**:
- `icn-core/src/supervisor.rs` - Trust graph initialization
- `icn-core/src/trust_propagation.rs` - Attestation routing by graph type

---

## Adding Evidence to Trust Graphs

Trust edges should include evidence for auditability:

```rust
let edge = TrustEdge::new_typed(
    issuer,
    subject,
    0.75,
    TrustGraphType::EconomicReliability
)
.with_label("cleared_transactions")
.with_evidence("tx:abc123")  // Reference to transaction
.with_evidence("tx:def456");

trust.economic_mut().add_edge(edge)?;
```

---

## Attestation Propagation

Trust attestations now include `graph_type` for proper routing:

```rust
let attestation = TrustAttestation::new_typed(
    issuer,
    subject,
    0.8,
    TrustGraphType::EconomicReliability
);

// Sign and broadcast
attestation.sign(&keypair)?;
gossip.publish("trust:attestations", attestation)?;
```

Receiving nodes route attestations to the correct graph:

```rust
fn handle_attestation(att: TrustAttestation) {
    let edge = att.to_trust_edge();
    trust.graph_mut(att.graph_type).add_edge(edge)?;
}
```

---

## Testing Migration

Each component should have tests verifying:

1. **Backward compatibility**: Old code paths still work
2. **Typed access**: Domain-specific graphs return correct scores
3. **Edge routing**: Edges go to the right graph
4. **Scoring weights**: Different weights per graph type

Example test:

```rust
#[test]
fn test_credit_policy_uses_economic_graph() {
    let mut trust = TrustGraphFacade::new(store, alice.clone());

    // Add different scores to each graph
    trust.social_mut().add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.9))?;
    trust.economic_mut().add_edge(TrustEdge::new(alice.clone(), bob.clone(), 0.3))?;

    // Credit policy should use economic score (0.3 * 0.8 = 0.24), not social
    let limit = calculate_credit_limit(&trust, &bob)?;
    assert!(limit < high_trust_limit); // Low economic trust = low limit
}
```

---

## Metrics

New Prometheus metrics for multi-graph monitoring:

```
icn_trust_edges_total{graph_type="social|economic|technical"}
icn_trust_score_computed_total{graph_type="social|economic|technical"}
icn_trust_attestations_received_total{graph_type="social|economic|technical"}
```

---

## Rollout Plan

1. **Week 1**: Deploy `TrustGraphFacade` in supervisor (zero consumer changes)
2. **Week 2**: Migrate high-priority consumers (icn-ledger, icn-compute, icn-core)
3. **Week 3**: Migrate medium-priority consumers (icn-net, icn-gossip, icn-security)
4. **Week 4**: Migrate low-priority consumers (icn-governance, icn-federation, icn-rpc)
5. **Week 5**: Remove facade, use `MultiTrustGraph` directly

---

## FAQ

**Q: What happens to existing trust edges without `graph_type`?**

A: They default to `Social` via `#[serde(default)]`. This maintains backward compatibility.

**Q: Can I query combined score after migration?**

A: Yes, `TrustGraphFacade::compute_trust_score()` returns the combined score. Use typed access (`social()`, `economic()`, `technical()`) for domain-specific queries.

**Q: How do I add edges to a specific graph?**

A: Either use `TrustEdge::new_typed()` with explicit graph type, or access the graph directly via `trust.economic_mut().add_edge()`.

**Q: What if a consumer needs scores from multiple graphs?**

A: Query each graph separately and combine as needed for your use case:

```rust
let social = trust.social().compute_trust_score(&did)?;
let economic = trust.economic().compute_trust_score(&did)?;
let custom_score = social * 0.7 + economic * 0.3;
```
