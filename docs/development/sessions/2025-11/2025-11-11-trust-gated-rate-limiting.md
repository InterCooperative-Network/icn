# Trust-Gated Rate Limiting Implementation

**Date**: 2025-11-11
**Phase**: Production Hardening
**Feature**: PR #3 - Trust-Gated Rate Limiting
**Status**: ✅ Complete

## Overview

Implemented dynamic rate limiting that adjusts message throughput based on peer trust classification. This provides adaptive DoS protection: strict limits for untrusted peers, generous limits for trusted partners.

## Goals

1. **Adaptive Security**: Rate limits should reflect actual trust relationships
2. **DoS Protection**: Prevent untrusted peers from flooding the network
3. **High Throughput**: Enable fast communication with trusted partners
4. **Dynamic Adjustment**: Limits update automatically as trust changes
5. **Backwards Compatibility**: Work without trust graph (fallback mode)

## Implementation

### Architecture

**Trust Classes and Limits**:
```rust
pub struct TrustGatedRateLimitConfig {
    pub isolated: RateLimitConfig,   // score < 0.1: 10 msg/sec, burst 2
    pub known: RateLimitConfig,      // score 0.1-0.4: 50 msg/sec, burst 10
    pub partner: RateLimitConfig,    // score 0.4-0.7: 100 msg/sec, burst 20
    pub federated: RateLimitConfig,  // score 0.7+: 200 msg/sec, burst 50
    pub refill_interval: Duration,   // Shared: 100ms
}
```

**Design Decisions**:

1. **20x Range**: Federated peers get 20x more throughput than isolated (200 vs 10 msg/sec)
   - Rationale: Strong incentive to build trust, severe punishment for untrusted
   - Alternative considered: 10x range felt insufficient for DoS protection

2. **Trust Score Mapping**: TrustGraph computes final score as 70% direct + 30% transitive
   - Direct trust edges need adjustment: `direct_score * 0.7 = desired_final_score`
   - Example: Direct 1.0 → Final 0.7 (Federated class)
   - This caught us in testing initially - scores weren't mapping correctly

3. **Token Bucket Reset on Trust Change**: Full capacity reset when class changes
   ```rust
   fn update_config(&mut self, new_capacity: f64, new_refill_rate: f64, new_trust_class: Option<TrustClass>) {
       if self.trust_class != new_trust_class {
           self.tokens = new_capacity;  // Full reset
           self.last_refill = Instant::now();
       }
   }
   ```
   - Rationale: Immediate benefit for trust upgrades encourages good behavior
   - Alternative: Gradual refill would delay benefits, less incentive

4. **Per-Message Trust Lookup**: Query trust graph on every message
   - Rationale: Ensures rate limits reflect current trust state
   - Cost: One async lock + hash lookup per message
   - Optimization considered: Cache with TTL (deferred - premature)

### Key Components

**`icn-net/src/rate_limit.rs`**:
- `TrustGatedRateLimitConfig` - Configuration for all trust classes
- `RateLimiter::new_trust_gated()` - Constructor with trust graph integration
- `TokenBucket::update_config()` - Dynamic reconfiguration on trust change
- Token bucket tracks `trust_class: Option<TrustClass>` to detect changes

**`icn-core/src/supervisor.rs`**:
- Creates shared `TrustGraph` from persistent store (`~/.icn/trust/`)
- Passes to both `GossipActor` (for access control) and `NetworkActor` (for rate limiting)
- Trust lookup closure bridges sync/async contexts with `block_in_place`

**`icn-net/src/actor.rs`**:
- `NetworkActor::spawn()` accepts optional `trust_graph` parameter
- Conditional construction: trust-gated if graph provided, fallback otherwise
- Logs mode: "Trust-gated rate limiting enabled" vs "Using fallback rate limiting"

## Challenges & Solutions

### Challenge 1: Trust Score Calculation

**Problem**: Initial tests failed because trust scores didn't map to expected classes.

**Investigation**:
```rust
// Test setup (WRONG):
graph.add_edge(TrustEdge::new(alice, bob, 0.5)).unwrap();
// Expected: Partner class (0.4-0.7)
// Actual: Known class (final score: 0.5 * 0.7 = 0.35)
```

**Root Cause**: TrustGraph applies 70% direct + 30% transitive formula. We were setting direct scores assuming 1:1 mapping.

**Solution**: Adjusted test scores to account for formula:
```rust
// CORRECT:
graph.add_edge(TrustEdge::new(alice, bob, 0.7)).unwrap();
// Final score: 0.7 * 0.7 = 0.49 → Partner class ✓
```

**Lesson**: Always understand the scoring algorithm. Added comments in tests explaining the calculation.

### Challenge 2: Token Bucket Behavior on Trust Change

**Problem**: Test expected 50 fresh tokens after trust upgrade (Known→Federated), but bucket only had remaining tokens from previous capacity.

**First Attempt**: Cap tokens to new capacity
```rust
self.tokens = self.tokens.min(new_capacity);  // WRONG
```
Result: No benefit for trust upgrade!

**Second Attempt**: Reset to full capacity
```rust
self.tokens = new_capacity;  // CORRECT
```
Result: Immediate 50 tokens after upgrade ✓

**Rationale**:
- Trust upgrades should have immediate positive effect
- Encourages cooperative behavior
- Models real-world trust: "You've proven yourself, here's more access"

### Challenge 3: Backwards Compatibility

**Problem**: Existing code uses `NetworkActor::spawn()` without trust graph. Adding required parameter would break all call sites.

**Solution**: Optional parameter + fallback mode
```rust
pub async fn spawn(
    // ... other params
    trust_graph: Option<Arc<RwLock<TrustGraph>>>,  // Optional!
) -> Result<NetworkHandle>
```

All existing tests pass `None`, supervisor passes `Some(trust_graph)`.

**Fallback behavior**: Uses `RateLimitConfig::default()` (100 msg/sec, burst 20) for all peers.

## Testing

### Test Coverage

**`test_trust_gated_rate_limiting_different_classes()`**:
- Creates 4 peers with different trust levels
- Verifies each gets correct burst capacity (2, 10, 20, 50)
- Confirms rate limiting at expected thresholds

**`test_trust_gated_rate_limiting_trust_class_change()`**:
- Peer starts as Known (burst 10)
- Consumes all tokens → rate limited
- Trust upgraded to Federated
- Immediately gets 50 fresh tokens ✓
- Consumes all 50 → rate limited at new threshold

**`test_trust_gated_config_for_class()`**:
- Validates configuration mappings
- Ensures all trust classes have correct limits

**Integration**: All 140+ existing tests pass with changes:
- Updated 4 integration test files to pass `None` for trust graph
- Updated 1 unit test in `icn-net/src/actor.rs`
- Fixed 3 test files to use new `handle_message(&sender, msg)` signature

### Test Results

```
test result: ok. 19 passed; 0 failed; 3 ignored (icn-net)
test result: ok. 140+ passed; 0 failed (full suite)
```

All tests passing on first try after fixing trust score calculations.

## Design Patterns

### Pattern 1: Trust-Gated Resources

**Concept**: Resource allocation based on peer trust classification.

**Application**:
- Rate limiting (this PR)
- Future: Bandwidth quotas, storage allocation, computation credits

**Implementation**:
```rust
let config = match trust_class {
    TrustClass::Isolated => &config.isolated,
    TrustClass::Known => &config.known,
    TrustClass::Partner => &config.partner,
    TrustClass::Federated => &config.federated,
};
```

**Benefits**:
- Automatic adaptation to trust changes
- Clear incentives for building trust
- Severe limits for attacks without trust investment

### Pattern 2: Shared Trust Graph

**Concept**: Single trust graph shared across multiple actors.

**Implementation**:
```rust
let trust_graph_handle = Arc::new(RwLock::new(trust_graph));
// Pass to multiple consumers
let gossip = GossipActor::spawn(did, trust_lookup);
let network = NetworkActor::spawn(..., Some(trust_graph_handle));
```

**Benefits**:
- Single source of truth for trust data
- Updates immediately visible to all actors
- Persistent storage (survives restarts)

**Tradeoffs**:
- RwLock contention possible (read-heavy workload mitigates)
- Could optimize with per-actor caches + invalidation

## Metrics & Observability

**Existing Metric** (from earlier work):
- `icn_network_messages_rate_limited_total` - Total messages blocked

**Future Additions** (planned):
- `icn_network_rate_limited_by_class{class="isolated|known|partner|federated"}` - Per-class blocking
- `icn_network_active_peers_by_class{class="..."}` - Trust distribution
- `icn_network_trust_class_changes_total` - Rate limit adjustments
- `icn_trust_graph_lookup_duration_seconds` - Performance monitoring

## Performance Considerations

**Per-Message Overhead**:
1. Trust graph lock acquisition: ~μs (RwLock read)
2. Trust score computation: ~μs (hash lookup + arithmetic)
3. Token bucket operations: ~100ns (in-memory arithmetic)

**Total**: ~1-2μs per message (negligible compared to network I/O)

**Scaling**:
- Trust graph lock is read-heavy (per-message) vs write-light (trust updates)
- RwLock optimized for this pattern
- Could add per-actor caches if contention observed (not needed yet)

**Memory**:
- One TokenBucket per active peer: ~128 bytes
- Trust graph scales with trust edges: ~200 bytes per edge
- Total: Dominated by peer count, not message volume

## Security Analysis

### Threat Model

**Attack: Message Flooding**
- **Without trust**: Attacker can send 10 msg/sec (Isolated class)
- **Impact**: Limited to 0.1% of federated peer capacity
- **Mitigation**: Automatic, no operator intervention needed

**Attack: Trust Grinding**
- **Scenario**: Attacker builds trust slowly to gain higher limits
- **Cost**: Requires actual trust edges (not free)
- **Detection**: Trust graph auditing (future work)

**Attack: Peer Impersonation**
- **Prevention**: TLS certificate verification (existing)
- **Trust**: Based on DID identity, not IP address

### Defense in Depth

1. **TLS Certificate Verification** - Prevents DID spoofing
2. **Trust-Gated Rate Limiting** - Throttles untrusted peers (this PR)
3. **QUIC Stream Limits** - Bounds concurrent streams (existing)
4. **Message Size Limits** - Prevents memory exhaustion (existing)

Layered protection: Each layer independent, failure in one doesn't compromise others.

## Production Readiness

### ✅ Complete
- [x] Core implementation
- [x] Comprehensive testing (3 new tests)
- [x] Integration with supervisor
- [x] Backwards compatibility
- [x] Documentation (CLAUDE.md + CHANGELOG)

### ⏳ Future Work
- [ ] Prometheus metrics for per-class rate limiting
- [ ] Configuration via `icn.toml` (currently hardcoded defaults)
- [ ] Trust graph metrics (edge count, score distribution)
- [ ] Cache optimization if lock contention observed
- [ ] Trust audit logs for security monitoring

## Lessons Learned

1. **Understand Dependent Algorithms**: TrustGraph's 70/30 scoring required test score adjustments. Always verify assumptions about external components.

2. **Test Trust Dynamics**: Testing static trust is easy, testing trust *changes* caught the token bucket reset bug. Dynamic behavior matters.

3. **Immediate Positive Feedback**: Resetting tokens on trust upgrade creates strong incentive for good behavior. UX applies to automated systems too.

4. **Optional Parameters for Evolution**: Making `trust_graph` optional preserved all existing tests while enabling new functionality. Evolution-friendly APIs reduce churn.

5. **Shared State Patterns**: `Arc<RwLock<T>>` for shared trust graph works well for read-heavy workloads. Lock granularity matters less than read/write ratio.

## References

- Trust graph implementation: `icn-trust/src/lib.rs`
- Rate limiting algorithm: Token bucket (standard, RFC-like behavior)
- Trust score formula: 70% direct + 30% transitive (PageRank-inspired)

## Commits

1. `ac8203a` - feat: Implement trust-gated rate limiting (PR #3)
2. `1cad4fb` - feat: Wire trust graph to network actor in supervisor
3. `3644549` - docs: Document trust-gated rate limiting
4. `035a524` - docs: Add trust-gated rate limiting to CHANGELOG

**Lines Changed**: ~350 additions, ~30 modifications across 9 files

## Next Steps

Planned follow-up work:

1. **Metrics** - Per-class rate limiting metrics for observability
2. **Configuration** - Expose rate limit tuning via `icn.toml`
3. **Trust Metrics** - Monitor trust graph health and dynamics
4. **Validation** - Run daemon and observe trust-gated limiting in practice
5. **Audit Logging** - Security monitoring for trust manipulation

This completes the trust-gated rate limiting feature. The system now provides adaptive DoS protection that automatically adjusts to peer trustworthiness.
