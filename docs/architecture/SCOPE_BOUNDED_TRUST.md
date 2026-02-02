# Scope-Bounded Trust: Structural Centralization Prevention

**Status**: Implemented (Wave 2)  
**Last Updated**: 2026-02-01  
**Related Issues**: [#1008](https://github.com/InterCooperative-Network/icn/issues/1008) (Wave 2)

---

## Executive Summary

> **Trust centralization is a political-economy problem, not just a technical one.**

Scope-bounded trust provides **structural prevention** of trust centralization by limiting trust computation to organizational boundaries. This is NOT a damping mechanism (decay, transitive damping) but a fundamental architectural constraint.

**Key Insight**: By partitioning trust into scopes (cooperatives, federations, domains, topic classes), no single entity can accumulate cross-boundary trust power. This prevents "super-nodes" that dominate multiple organizational contexts.

---

## Design Principle

> **Scope must NOT enter kernel.**

- `scope_id` lives ONLY in `icn-trust` (app crate)
- Kernel receives `ConstraintSet` with opaque constraint values
- Kernel must NOT pattern-match on scope
- Scope selection is performed by apps/PolicyOracles → output is pure `ConstraintSet`

This maintains the **Meaning Firewall**: the kernel enforces constraints without understanding their semantic origin.

---

## Scope Types

Trust edges can be scoped to prevent centralization across organizational boundaries:

### 1. Cooperative Scope (`coop:<id>`)
Trust within a single cooperative.

**Use Case**: Member vouching, governance participation, work distribution within one coop.

**Example**: `coop:food-coop-123`

### 2. Federation Scope (`federation:<id>`)
Trust within a federation of cooperatives.

**Use Case**: Inter-cooperative trade credit, federated dispute resolution, resource sharing.

**Example**: `federation:regional-food-network`

### 3. Domain Scope (`domain:<id>`)
Trust within a governance domain.

**Use Case**: Specialized expertise (dispute mediators, auditors, technical stewards).

**Example**: `domain:dispute-resolution`

### 4. Topic Class Scope (`topic:<class>`)
Trust within a service class (compute, storage, etc.).

**Use Case**: Provider reputation scoped to service type. Storage providers don't automatically gain compute trust.

**Example**: `topic:compute`, `topic:storage`

### 5. Global Scope
Trust without scope boundaries (default for backward compatibility).

**Use Case**: Legacy edges, cross-scope foundational trust (rare).

**Example**: Foundational cooperative members, system architects.

---

## Data Model

### ScopeId Type

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScopeId {
    /// Trust within a single cooperative
    Cooperative(String),
    /// Trust within a federation of cooperatives
    Federation(String),
    /// Trust within a governance domain
    Domain(String),
    /// Trust within a topic class (e.g., compute, storage)
    TopicClass(String),
    /// Trust without scope boundaries (default)
    Global,
}
```

### TrustEdge Extension

```rust
pub struct TrustEdge {
    pub source: Did,
    pub target: Did,
    pub labels: Vec<String>,
    pub score: TrustScore,
    pub evidence: Vec<TrustEvidence>,
    pub expires_at: Option<u64>,
    pub created_at: u64,
    pub graph_type: TrustGraphType,
    
    /// Optional scope identifier (Wave 2)
    /// Defaults to None (global scope) for backward compatibility
    #[serde(default)]
    pub scope_id: Option<ScopeId>,
}
```

**Backward Compatibility**: Existing edges have `scope_id: None`, treated as global scope.

---

## Scope-Aware Trust Computation

### Method: `compute_trust_score_in_scope`

```rust
impl TrustGraph {
    pub fn compute_trust_score_in_scope(
        &self,
        target: &Did,
        scope: &ScopeId,
    ) -> Result<f64> {
        // Filter edges to scope before computing trust
        let scoped_edges: Vec<_> = self
            .get_outgoing_edges(&self.own_did)?
            .into_iter()
            .filter(|e| match &e.scope_id {
                Some(edge_scope) => edge_scope == scope,
                None => scope.is_global(),
            })
            .collect();
        
        // Compute trust using only scoped edges
        // (direct + transitive within scope)
        compute_score_from_edges(target, &scoped_edges)
    }
}
```

### Scope Isolation

Trust computed in one scope DOES NOT affect trust in another scope:

```rust
// Alice has high trust in coop scope
let coop_trust = graph.compute_trust_score_in_scope(
    &alice, 
    &ScopeId::cooperative("food-coop-123")
)?;
// coop_trust = 0.85

// Alice has ZERO trust in federation scope
let fed_trust = graph.compute_trust_score_in_scope(
    &alice,
    &ScopeId::federation("regional-network")
)?;
// fed_trust = 0.0

// Scopes are isolated - no cross-contamination
```

---

## Centralization Metrics (Observational Only)

### Purpose

Metrics detect concentration of trust within a scope. They are **observational only** - they feed dashboards and governance, NOT automatic enforcement.

### Metrics Flow

```
Metrics ─► Governance ─► PolicyOracle ─► ConstraintSet ─► Kernel
            │                              │
       (human vote)                   (pure data)
```

**Never**: `Metrics ─► Kernel` (direct enforcement)

### CentralizationMetrics

```rust
pub struct CentralizationMetrics {
    /// Gini coefficient (0.0 = equal, 1.0 = one node has all trust)
    pub gini_coefficient: f64,
    
    /// Number of nodes in scope
    pub node_count: usize,
    
    /// Total inbound trust across all nodes
    pub total_inbound_trust: f64,
    
    /// Fraction of trust held by top 10% of nodes
    pub top_10_percent_share: f64,
    
    /// Betweenness centrality statistics
    pub betweenness: BetweennessStats,
}
```

### Gini Coefficient

Measures inequality in trust distribution:

- **0.0**: Perfect equality (all nodes have equal trust)
- **1.0**: Perfect inequality (one node has all trust)

**Formula**:
```
G = (2 * Σ(i * x_i)) / (n * Σ(x_i)) - (n + 1) / n
```

Where edges are sorted by inbound trust.

### Top-N Concentration

Measures power concentration in top percentile:

```rust
let top_10 = metrics.top_10_percent_share;
// If top_10 > 0.5, top 10% of nodes hold >50% of trust
```

### Betweenness Centrality

Measures "bridge" nodes that connect clusters:

- **High betweenness**: Node is a bottleneck/bridge between groups
- **Low betweenness**: Node is peripheral or in a dense cluster

```rust
pub struct BetweennessStats {
    pub max: f64,       // Most central node
    pub mean: f64,      // Average centrality
    pub std_dev: f64,   // Variance in centrality
}
```

### Computing Metrics

```rust
use icn_trust::{TrustGraphAnalyzer, ScopeId};

let analyzer = TrustGraphAnalyzer::default();
let scope = ScopeId::cooperative("food-coop-123");

let metrics = analyzer.compute_centralization_metrics(&graph, &scope)?;

println!("Gini coefficient: {:.3}", metrics.gini_coefficient);
println!("Top 10% hold: {:.1}% of trust", metrics.top_10_percent_share * 100.0);
```

---

## Bridge Nodes (Future Work)

**Status**: Not yet implemented. Included here for design reference.

**Bridge nodes** span multiple scopes and could accumulate cross-scope influence. When implemented, bridges will require explicit declaration and stricter constraints.

### Planned Design

- `BridgeDeclaration` struct with DID, scopes, timestamp, and signature
- Gossip topic `bridges:declarations` for discovery
- PolicyOracles apply stricter constraints (reduced rate limits, fewer topics)
- Governance policies can formalize "approved bridges" through voting
- **Firewall compliance**: Bridge status → PolicyOracle → ConstraintSet (kernel never sees bridge semantics)

See [Merge Gate](#merge-gate) for tracking.

---

## Migration Path

### Existing Edges

All existing edges default to `scope_id: None` (global scope):

```rust
// Old edge (pre-Wave 2)
let edge = TrustEdge {
    source: alice,
    target: bob,
    score: 0.8,
    // scope_id is None (global)
    ..
};

// Serde deserializes to scope_id: None
// Global scope queries match these edges
```

### Adding Scope to Edges

Apps can add scope when creating new edges:

```rust
// Coop-scoped trust
let edge = TrustEdge::new_scoped(
    alice,
    bob,
    TrustScore::new(0.8)?,
    TrustGraphType::Social,
    ScopeId::cooperative("food-coop-123")
);

graph.add_edge(edge)?;
```

### Gradual Migration

1. **Phase 1**: All edges global (current state)
2. **Phase 2**: New edges optionally scoped
3. **Phase 3**: Governance policies enforce scope on new edges
4. **Phase 4**: Legacy edges remain global or migrated by governance vote

---

## Firewall Compliance Verification

### Rule 1: Kernel Must Not Import icn-trust

✅ **Verified**: Kernel crates (`icn-core`, `icn-kernel-api`, `icn-net`, `icn-gossip`) do NOT depend on `icn-trust`.

CI enforcement: `.github/scripts/firewall_denylist.py`

### Rule 2: Kernel Only Accepts Pure Data

✅ **Verified**: Kernel entrypoints accept only:
- `ConstraintSet` (pure data)
- `CapabilityToken` (pure data)
- `Did`, `Timestamp`, `Hash` (primitives)

No `ScopeId` in kernel interfaces.

### Rule 3: Semantic Lookup in PolicyOracle Only

✅ **Verified**: Trust score computation by scope happens in:
- `icn-trust::TrustGraph::compute_trust_score_in_scope` (app layer)
- PolicyOracle implementations (app layer)

Kernel NEVER calls scope-aware methods.

### Rule 4: No Pattern-Matching on ConstraintSet.custom

✅ **Verified**: Kernel uses ONLY typed `ConstraintSet` fields:
- `rate_limit: Option<RateLimit>`
- `max_topics: Option<u32>`
- etc.

Kernel does NOT check `custom` keys for scope-related branching.

### Rule 5: Mechanism vs Policy

✅ **Verified**:

| Component | Kernel Owns (Mechanism) | Apps Own (Value) |
|-----------|-------------------------|------------------|
| Rate Limiting | Token bucket algorithm | Messages/sec |
| Topic Admission | Max topics check | Max value (25 vs 100) |
| Connection Limits | Max connections check | Max value |
| Scope Trust | (none - not in kernel) | Scope filtering |

---

## Example: PolicyOracle Integration

```rust
// In apps/trust/src/oracle.rs
impl PolicyOracle for ScopedTrustOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        // Extract scope from request context (app logic)
        let scope = self.extract_scope_from_request(request)?;
        
        // Compute scope-bounded trust (app logic)
        let trust_score = self.graph
            .compute_trust_score_in_scope(&request.actor, &scope)?;
        
        // ═══════════════════════════════════════════════════════
        // Semantic logic ENDS here. Below is pure data.
        // ═══════════════════════════════════════════════════════
        
        // Translate trust score to constraints (pure data)
        let rate_limit = if trust_score >= 0.7 {
            RateLimit::new(200, 50) // Federated
        } else if trust_score >= 0.4 {
            RateLimit::new(100, 25) // Partner
        } else {
            RateLimit::new(20, 5)   // Known
        };
        
        PolicyDecision::allow_with(
            ConstraintSet::new()
                .with_rate_limit(rate_limit),
        )
    }
}
```

**Kernel Enforcement** (mechanism only):

```rust
// In icn-core (kernel)
fn apply_constraints(actor: &Did, constraints: &ConstraintSet) {
    if let Some(limit) = constraints.rate_limit {
        self.rate_limiter.apply(actor, limit);
        // Kernel doesn't know WHY limit is 200/50 vs 20/5
    }
}
```

---

## Governance Integration

### Metrics → Governance

Centralization metrics trigger governance discussions:

1. **Dashboard** displays Gini coefficient per scope
2. **Alert** if `gini > 0.7` (high inequality)
3. **Proposal** created: "Limit top 10% trust share to 40%"
4. **Vote** by cooperative members
5. **PolicyOracle** updated to enforce new limits

### PolicyOracle → Enforcement

```rust
// After governance vote, PolicyOracle enforces limit
impl PolicyOracle for GovernanceTrustOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let scope = self.extract_scope(request)?;
        let metrics = self.analyzer.compute_centralization_metrics(&self.graph, &scope)?;
        
        // Governance policy: reject if Gini > 0.7
        if metrics.gini_coefficient > 0.7 {
            return PolicyDecision::deny(
                "Trust centralization exceeds cooperative policy limit"
            );
        }
        
        // Otherwise, standard constraints
        PolicyDecision::allow()
    }
}
```

**Firewall Holds**: Governance policy → PolicyOracle → ConstraintSet. Kernel sees only opaque allow/deny.

---

## Research Directions (Phase 4)

### Inbound Trust Saturation Cap

Prevent any single node from accumulating unbounded trust:

```
effective_trust = Σ edges[0..N] + Σ edges[N..] * decay^(i-N)
```

Where:
- First N edges count fully
- Subsequent edges decay exponentially
- Prevents "super-nodes" even with many incoming edges

**Status**: Prototype in Phase 4 (optional)

---

## Merge Gate

**Required for Wave 2 completion:**

- [x] `TrustEdge.scope_id` field exists
- [x] Trust tests pass with scope
- [ ] Gossip tests pass with scope-aware admission
- [x] No scope logic in kernel crates (firewall holds)
- [x] Metrics exposed but not auto-enforcing
- [ ] Bridge node declaration exists (observational only)

**Current Status**: 4/6 complete (documentation phase)

---

## References

- **Wave 1**: [#1007](https://github.com/InterCooperative-Network/icn/issues/1007) - Firewall Contract
- **Wave 2**: [#1008](https://github.com/InterCooperative-Network/icn/issues/1008) - Scope-Bounded Trust (this)
- **Firewall Architecture**: `docs/architecture/KERNEL_APP_SEPARATION.md`
- **Trust Graph Design**: `icn/crates/icn-trust/src/lib.rs`

---

## Authors

- Implementation: GitHub Copilot (2026-02-01)
- Design Review: ICN Core Team
