# Trust Service Performance Characteristics

## Overview

This document describes the performance characteristics of the Trust Service implementation and provides guidance for monitoring and optimization.

## Method Comparison

### `trust_score()` - Fast Path

**Purpose:** Quick trust lookups for authorization decisions

**Performance:** ~5-7 µs regardless of network size (due to caching and bloom filter optimizations)

**Operations:**
1. Acquire read lock on trust graph
2. Compute trust score (may use cached value)
3. Return score value only

**Use cases:**
- Request authorization checks
- Rate limiting decisions
- Access control enforcement
- Any high-frequency trust lookup

### `trust_score_detailed()` - Enriched Path

**Purpose:** Cache validation and debugging with provenance metadata

**Performance:** Scales with network size and input edge count
- 100 nodes: ~11.7 ms
- 1000 nodes: ~121.9 ms
- 5000 nodes: ~617.0 ms

**Operations:**
1. Acquire read lock on trust graph
2. Compute trust score (same as fast path)
3. **Iterate ALL DIDs to find input edges** ⚠️
4. Convert edges to attestations
5. Compute SHA-256 hash over canonicalized inputs
6. Build TrustScoreResult with metadata

**Use cases:**
- Cache validation (check if `inputs_hash` or `epoch` changed)
- Debugging trust computation failures
- Audit logging with full provenance
- Low-frequency administrative queries

**NOT recommended for:**
- Request-path authorization (too slow)
- Real-time decision making
- High-frequency queries

## Performance Bottleneck Analysis

The ~21,700x slowdown in `trust_score_detailed()` (1000-node network) breaks down as:

1. **Edge Collection (~90% of overhead)**
   - `graph.get_all_known_dids()` iterates entire graph
   - For each DID, fetch outgoing edges and filter for target
   - O(N × M) where N = number of DIDs, M = avg edges per DID

2. **Attestation Conversion (~5% of overhead)**
   - Convert TrustEdge → TrustAttestation for each input

3. **SHA-256 Hashing (~5% of overhead)**
   - Hash computation is actually quite fast
   - Minimal impact compared to edge collection

## Optimization Considerations

### 1. Reverse Edge Index

**Problem:** Finding input edges requires iterating all DIDs

**Solution:** Maintain an in-memory reverse index: `target_did → Vec<(source_did, score)>`

**Tradeoffs:**
- **Pro:** O(1) lookup instead of O(N × M)
- **Con:** Memory overhead (~8-16 bytes per edge)
- **Con:** Requires keeping index synchronized with graph mutations

**Impact:** Could reduce detailed query time from ~121 ms → ~1-2 ms (1000 nodes)

### 2. Hash Memoization

**Problem:** Same-epoch queries recompute identical hashes

**Solution:** Cache `(actor_did, epoch) → inputs_hash` mapping

**Tradeoffs:**
- **Pro:** Eliminates redundant hash computation
- **Con:** Cache invalidation on epoch increment
- **Con:** Memory overhead for cache storage

**Impact:** Minimal (~5% of total overhead is hashing)
**Recommendation:** **Low priority** - focus on edge collection first

### 3. Async Refactoring

**Problem:** `block_in_place()` contention on tokio threadpool

**Solution:** Make TrustService trait methods async

**Tradeoffs:**
- **Pro:** Eliminates threadpool contention
- **Pro:** Better integration with async ecosystem
- **Con:** Breaking API change for all callers
- **Con:** Significant refactoring effort

**Impact:** Moderate improvement for concurrent queries
**Recommendation:** Consider after Phase 6 crate consolidation

## Monitoring Metrics

### Key Metrics

1. **`trust_oracle_block_in_place_total`**
   - Counter for `block_in_place()` calls
   - Indicates tokio threadpool pressure
   - **Acceptable threshold:** <100 calls/second per core
   - **Warning threshold:** >500 calls/second per core
   - **Critical threshold:** >1000 calls/second per core

2. **Trust Query Latency** (proposed)
   - Histogram for `trust_score()` duration
   - **Target:** p50 < 10 µs, p99 < 100 µs
   - **Warning:** p99 > 1 ms indicates cache misses

3. **Detailed Query Latency** (proposed)
   - Histogram for `trust_score_detailed()` duration
   - **Target:** p50 < 50 ms, p99 < 500 ms
   - **Warning:** p99 > 1s indicates network growth

### Alert Conditions

1. **High block_in_place contention**
   - Symptom: `trust_oracle_block_in_place_total` rate exceeds threshold
   - Action: Audit callers, implement caching, or async refactor

2. **Slow trust queries**
   - Symptom: `trust_score()` p99 latency > 1 ms
   - Action: Check cache hit rate, bloom filter effectiveness

3. **Excessive detailed queries**
   - Symptom: High `trust_score_detailed()` call rate
   - Action: Audit call sites, implement application-level caching

## Recommendations

### Immediate (Phase 22 - tier:3)

1. ✅ **Add benchmarks** - Completed (#1001)
2. ✅ **Document performance tradeoffs** - This document
3. **Add reverse edge index** - Track in new issue
4. **Add query latency metrics** - Track in new issue

### Future (Post-Phase 6)

1. **Async trait refactoring** - After crate consolidation (#861)
2. **Hash memoization** - Low priority, minimal impact

### Non-Goals

- Making `trust_score_detailed()` as fast as `trust_score()`
- Caching detailed results (epoch changes frequently)
- Optimizing for detailed query throughput (rare use case)

## Related

- Issue #1001: Benchmark trust_score_detailed performance
- PR #987: trust_score_detailed() implementation
- Issue #877: Multi-graph trust implementation
- docs/production-hardening.md: Network protections
