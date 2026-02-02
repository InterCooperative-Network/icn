# Trust Cache Invalidation Monitoring & Optimization

**Issue**: #998  
**Status**: Implemented  
**Date**: 2026-02-02

## Overview

This document describes the monitoring and optimization strategy for high-fanout cache invalidation in the trust system, implemented as part of issue #998.

## Background

The transitive cache invalidation fix (issue #878, PR #988) invalidates cached trust scores when edges change. For nodes with high fanout (many outgoing edges), this could trigger 100+ cache invalidations per edge mutation.

**Performance Characteristic**: O(1 + fanout(target)) per edge mutation

## Monitoring Metrics

### Core Metrics

1. **`icn_trust_cache_transitive_invalidations_total`** (counter)
   - Number of cache entries actually invalidated transitively
   - Incremented only for downstream DIDs that had cached entries
   - Use `rate()` to track invalidations per second

2. **`icn_trust_cache_downstream_count`** (histogram)
   - Distribution of total downstream fanout per edge mutation
   - Records total outgoing edges (regardless of cache state)
   - Percentiles reveal high-fanout "hub" nodes

3. **`icn_trust_cache_max_downstream_count`** (gauge)
   - Most recent downstream fanout count observed
   - Note: this gauge tracks the latest value, not the all-time maximum
   - Use `histogram_quantile(0.99, ...)` on the histogram for reliable hub detection

4. **`icn_trust_cache_selective_skips_total`** (counter)
   - Number of transitive invalidations skipped (selective optimization)
   - Indicates effectiveness of the selective invalidation strategy
   - High skip rate = optimization is working well

### Derived Metrics

**Average Downstream Count** (PromQL):
```promql
rate(icn_trust_cache_transitive_invalidations_total[5m]) / 
rate(icn_trust_cache_invalidations_total[5m])
```

**Selective Invalidation Efficiency** (PromQL):
```promql
rate(icn_trust_cache_selective_skips_total[5m]) / 
(rate(icn_trust_cache_selective_skips_total[5m]) + rate(icn_trust_cache_transitive_invalidations_total[5m]))
```

**Fanout Distribution** (PromQL):
```promql
histogram_quantile(0.95, rate(icn_trust_cache_downstream_count_bucket[5m]))
```

## Optimization Strategy: Selective Invalidation

### Implementation

The selective invalidation optimization checks if a cache entry exists before invalidating it:

```rust
pub fn invalidate_if_cached(&self, did: &Did) -> bool {
    if let Ok(mut cache) = self.cache.lock() {
        if cache.peek(did).is_some() {
            cache.pop(did);
            // ... increment metrics ...
            true
        } else {
            trust_cache_selective_skips_inc();
            false
        }
    } else {
        false
    }
}
```

### Benefits

1. **Reduced Lock Contention**: Fewer unnecessary cache operations
2. **Lower Metric Overhead**: Skip tracking for non-existent entries
3. **Better Performance**: For low cache hit rates (<20%), saves 80% of invalidation work

### When Optimization is Effective

- **Low cache hit rate** (<20%): Most downstream nodes aren't cached
- **High fanout** (>50): Many downstream invalidations per mutation
- **Hub nodes**: Central nodes with 100+ outgoing edges

### Performance Impact

For a hub with 100 outgoing edges and 20% cache hit rate:
- **Without optimization**: 100 cache operations (95 unnecessary)
- **With optimization**: 20 invalidations + 80 skips (5x fewer operations)

## Thresholds & Alerts

### Detection Threshold

**High-fanout logging trigger**: Downstream count > 50

When this threshold is exceeded, the system logs detailed information:
- Total downstream count
- Actual invalidations performed
- Cache hit rate for this invalidation event

### Recommended Alert Rules

**Alert 1: High Fanout Detected**
```yaml
alert: HighFanoutCacheInvalidation
expr: |
  histogram_quantile(
    0.99,
    sum(rate(icn_trust_cache_downstream_count_bucket[5m])) by (le)
  ) > 50
for: 5m
labels:
  severity: info
annotations:
  summary: "High-fanout cache invalidation detected"
  description: "Recent downstream fanout (p99) is {{ $value }}, which may impact performance."
```

**Alert 2: Low Selective Efficiency**
```yaml
alert: LowSelectiveInvalidationEfficiency
expr: |
  (
    rate(icn_trust_cache_selective_skips_total[5m]) / 
    (rate(icn_trust_cache_selective_skips_total[5m]) + rate(icn_trust_cache_transitive_invalidations_total[5m]))
  ) < 0.5
for: 10m
labels:
  severity: warning
annotations:
  summary: "Selective invalidation efficiency is low"
  description: "Skip rate is {{ $value | humanizePercentage }}, indicating high cache hit rates where selective optimization provides less benefit."
```

## Grafana Dashboard Panels

### Panel 1: Downstream Count Distribution (Histogram)
```json
{
  "title": "Cache Invalidation Fanout Distribution",
  "type": "timeseries",
  "targets": [
    {
      "expr": "histogram_quantile(0.50, rate(icn_trust_cache_downstream_count_bucket[5m]))",
      "legendFormat": "p50"
    },
    {
      "expr": "histogram_quantile(0.95, rate(icn_trust_cache_downstream_count_bucket[5m]))",
      "legendFormat": "p95"
    },
    {
      "expr": "histogram_quantile(0.99, rate(icn_trust_cache_downstream_count_bucket[5m]))",
      "legendFormat": "p99"
    }
  ]
}
```

### Panel 2: Max Fanout (Gauge)
```json
{
  "title": "Max Downstream Count (Hub Detection)",
  "type": "stat",
  "targets": [
    {
      "expr": "max(icn_trust_cache_max_downstream_count)"
    }
  ],
  "fieldConfig": {
    "thresholds": {
      "steps": [
        { "color": "green", "value": null },
        { "color": "yellow", "value": 50 },
        { "color": "red", "value": 100 }
      ]
    }
  }
}
```

### Panel 3: Selective Optimization Effectiveness
```json
{
  "title": "Selective Invalidation Efficiency",
  "type": "timeseries",
  "targets": [
    {
      "expr": "rate(icn_trust_cache_selective_skips_total[5m])",
      "legendFormat": "Skips/sec"
    },
    {
      "expr": "rate(icn_trust_cache_transitive_invalidations_total[5m])",
      "legendFormat": "Invalidations/sec"
    }
  ]
}
```

## Interpretation Guide

### Scenario 1: Low Fanout Network (Normal)
- Max downstream count: <10
- p95 downstream count: <5
- Selective skip rate: Low (most nodes are cached)
- **Action**: No optimization needed

### Scenario 2: Moderate Fanout with Low Cache Hit Rate
- Max downstream count: 20-50
- p95 downstream count: 10-20
- Selective skip rate: 60-80%
- **Action**: Selective optimization is working well

### Scenario 3: High Fanout Hub Detected
- Max downstream count: >100
- p95 downstream count: >50
- Selective skip rate: 70-90%
- **Action**: Monitor for performance impact; consider additional optimizations if needed

## Future Optimizations (If Needed)

If selective invalidation proves insufficient, consider:

1. **Batch Invalidation**: Collect all affected DIDs, deduplicate, then invalidate once
   - Reduces lock contention for overlapping invalidations
   - Most effective when multiple edges change rapidly

2. **Lazy Invalidation**: Mark entries as "dirty" without removing them
   - Validate on read (check if trust edges have changed since cache time)
   - Reduces invalidation overhead at the cost of read-time validation

3. **Tiered Caching**: Different TTLs for different fanout levels
   - Hub nodes: Shorter TTL (e.g., 1 minute)
   - Leaf nodes: Longer TTL (e.g., 5 minutes)
   - Reduces invalidation frequency for hubs

## Testing

### Unit Tests
- `test_selective_invalidation`: Verifies skip behavior
- `test_selective_invalidation_skip_count`: Validates metric recording

### Integration Tests
- `test_high_fanout_cache_invalidation`: 100-node fanout scenario
- `test_selective_invalidation_reduces_work`: 50-node fanout with low hit rate
- `test_fanout_threshold_logging`: Verifies >50 threshold logging

## Implementation Details

### Code Locations
- **Metrics definitions**: `icn-obs/src/metrics_legacy.rs`
- **Selective invalidation**: `icn-trust/src/trust_cache.rs`
- **Transitive invalidation**: `icn-trust/src/lib.rs:invalidate_affected()`
- **Tests**: `icn-trust/tests/trust_integration.rs`

### Key Design Decisions

1. **Selective by default**: Always use `invalidate_if_cached()` for transitive targets
2. **Direct invalidation non-selective**: The target node itself is always invalidated (correctness over performance)
3. **Metric granularity**: Track both total downstream count (for monitoring) and actual invalidations (for efficiency measurement)

## Acceptance Criteria Status

✅ Metric dashboard showing invalidation volume exists
✅ Selective invalidation implemented for when avg downstream count > 50
✅ High-fanout logging triggers at >50 downstream count
✅ Tests validate optimization behavior

## Related Issues

- #878: Trust cache invalidation bug (fixed in PR #988)
- #988: Transitive cache invalidation PR
- #996: Fault injection + stress tests (tier:3)
- #1001: Benchmark trust_score_detailed (tier:3)
