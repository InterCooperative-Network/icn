# Trust Service Benchmarks

Criterion.rs benchmarks for quantifying the performance characteristics of `trust_score()` vs `trust_score_detailed()`.

## Running Benchmarks

From the repository root:

```bash
# Run all trust service benchmarks
cd apps/trust && cargo bench --bench trust_service_bench

# Run specific benchmark group
cd apps/trust && cargo bench --bench trust_service_bench trust_score_baseline
cd apps/trust && cargo bench --bench trust_service_bench trust_score_detailed
cd apps/trust && cargo bench --bench trust_service_bench trust_score_comparison
cd apps/trust && cargo bench --bench trust_service_bench hash_by_input_count
```

## Benchmark Groups

### 1. `trust_score_baseline`

Tests the fast path (`trust_score()`) at various network sizes:
- 100 nodes
- 1000 nodes
- 5000 nodes

**Expected:** ~5-7 µs regardless of network size (due to optimizations)

### 2. `trust_score_detailed`

Tests the enriched path (`trust_score_detailed()`) with SHA-256 hashing and edge collection:
- 100 nodes: ~12 ms
- 1000 nodes: ~122 ms
- 5000 nodes: ~617 ms

**Expected:** Time scales with network size and input edge count

### 3. `trust_score_comparison`

Direct side-by-side comparison of both methods on the same 1000-node graph:
- `trust_score()`: ~5.5 µs
- `trust_score_detailed()`: ~121 ms
- **Slowdown:** ~21,700x

### 4. `hash_by_input_count`

Tests how `trust_score_detailed()` scales with the number of input edges:
- 1 edge: ~13 ms
- 5 edges: ~60 ms
- 10 edges: ~119 ms
- 20 edges: ~234 ms
- 50 edges: ~565 ms (estimated)

**Expected:** Linear scaling with input edge count

## Interpreting Results

### Criterion Output

Criterion reports three key statistics:
- **Mean time:** Average across all samples
- **Std deviation:** Variability in measurements
- **Outliers:** Measurements significantly different from the mean

Example output:
```
trust_score_comparison/trust_score_1000
                        time:   [5.5452 µs 5.5498 µs 5.5561 µs]
Found 10 outliers among 100 measurements (10.00%)
  3 (3.00%) high mild
  7 (7.00%) high severe
```

### Performance Targets

| Metric | Target | Warning | Critical |
|--------|--------|---------|----------|
| trust_score() p99 | <100 µs | >1 ms | >10 ms |
| trust_score_detailed() p99 | <500 ms | >1 s | >5 s |
| Overhead ratio | <30,000x | >50,000x | >100,000x |

## Troubleshooting

### Benchmarks Take Too Long

The `trust_score_detailed` benchmarks for large networks (5000 nodes) or high edge counts (50 edges) can take 10+ minutes. To speed up:

```bash
# Reduce sample count (default: 100)
CRITERION_SAMPLE_SIZE=20 cargo bench --bench trust_service_bench
```

### Inconsistent Results

If results vary significantly between runs:
- Close other applications to reduce system load
- Disable CPU frequency scaling: `sudo cpupower frequency-set --governor performance`
- Increase sample count for more stable averages

### Build Errors

If the benchmark fails to compile:
```bash
cd apps/trust
cargo clean
cargo build --benches
```

## Related Documentation

- [docs/performance/trust-service-performance.md](../../docs/performance/trust-service-performance.md) - Detailed performance analysis
- [icn/crates/icn-trust/benches/trust_bench.rs](../../icn/crates/icn-trust/benches/trust_bench.rs) - Trust graph benchmarks
- Issue #1001 - Performance investigation tracking
