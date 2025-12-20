# ICN Profiling Guide

This guide covers how to profile ICN components and analyze performance.

## Prerequisites

Install profiling tools:

```bash
# Criterion (benchmarking)
cargo install cargo-criterion

# Flamegraph
cargo install flamegraph

# perf (Linux)
sudo apt install linux-tools-common linux-tools-$(uname -r)

# Instruments (macOS)
# Included with Xcode
```

## Running Benchmarks

### Basic Benchmark Run

```bash
cd icn
cargo bench --workspace
```

### Specific Crate

```bash
cargo bench -p icn-trust
cargo bench -p icn-ledger
cargo bench -p icn-gossip
```

### Specific Benchmark

```bash
cargo bench --bench trust_bench -- "trust_computation"
cargo bench --bench ledger_bench -- "ed25519"
```

### Baseline Comparison

```bash
# Save baseline
cargo bench --workspace -- --save-baseline main

# Compare against baseline
cargo bench --workspace -- --baseline main
```

## Interpreting Results

### Criterion Output

```
trust_computation/10_nodes
                        time:   [43.8 ns 44.2 ns 44.6 ns]
                        change: [-1.2% +0.5% +2.1%] (p = 0.42 > 0.05)
                        No change in performance detected.
```

- **time**: [lower bound, estimate, upper bound] at 95% confidence
- **change**: Comparison to previous run (if available)
- **p-value**: Statistical significance (< 0.05 indicates real change)

### What to Look For

| Result | Meaning | Action |
|--------|---------|--------|
| `No change` | Within noise | No action needed |
| `Performance has improved` | Genuine speedup | Document improvement |
| `Performance has regressed` | Genuine slowdown | Investigate cause |

## CPU Profiling

### Flamegraph (Linux)

```bash
# Enable frame pointers for better traces
export RUSTFLAGS="-C force-frame-pointers=yes"

# Profile a specific benchmark
cargo flamegraph --bench trust_bench -- --bench "trust_computation"

# Profile the daemon
cargo flamegraph --bin icnd -- --config /path/to/config.toml
```

Output: `flamegraph.svg` - open in browser.

### perf (Linux)

```bash
# Record profile
perf record -g cargo bench --bench ledger_bench

# Analyze
perf report

# Top functions
perf top -p $(pgrep icnd)
```

### Instruments (macOS)

```bash
# Build with debug symbols
cargo build --release

# Run with Instruments
xcrun xctrace record --template 'Time Profiler' \
  --launch ./target/release/icnd -- --config config.toml
```

## Memory Profiling

### Heap Profiling with DHAT

```bash
# Add to Cargo.toml
[profile.bench]
debug = true

# Run with DHAT (requires nightly)
cargo +nightly run --features dhat-heap --bin icnd
```

### Valgrind (Linux)

```bash
# Memory usage
valgrind --tool=massif ./target/release/icnd --config config.toml

# Visualize
ms_print massif.out.*

# Memory leaks
valgrind --leak-check=full ./target/release/icnd --config config.toml
```

### heaptrack (Linux)

```bash
heaptrack ./target/release/icnd --config config.toml
heaptrack_gui heaptrack.icnd.*.gz
```

## Async Profiling

### tokio-console

For debugging async runtime issues:

```bash
# Add dependency
cargo add --dev console-subscriber

# In main.rs (dev builds only)
#[cfg(feature = "tokio-console")]
console_subscriber::init();

# Run with tokio-console
RUSTFLAGS="--cfg tokio_unstable" cargo run --features tokio-console

# In another terminal
tokio-console
```

### Tracing

ICN uses `tracing` for observability. Enable detailed spans:

```bash
RUST_LOG=icn_trust=trace cargo run --bin icnd
```

## Network Profiling

### Message Timing

Add timing spans to track message latency:

```rust
#[tracing::instrument(skip(self))]
async fn handle_message(&self, msg: Message) {
    let start = std::time::Instant::now();
    // ... process message
    tracing::info!(elapsed_ms = ?start.elapsed().as_millis(), "message processed");
}
```

### Bandwidth Analysis

Monitor network traffic:

```bash
# Linux: iftop
sudo iftop -i eth0 -f "port 5600"

# Or nethogs for per-process
sudo nethogs
```

## Optimization Workflow

### 1. Identify Bottleneck

```bash
# Run benchmarks
cargo bench --workspace

# Find slowest operations
grep -r "time:" target/criterion/*/new/estimates.json | \
  sort -t: -k4 -n | tail -20
```

### 2. Profile the Bottleneck

```bash
# CPU profile
cargo flamegraph --bench <bench_name> -- --bench "<slow_test>"
```

### 3. Analyze Profile

Look for:
- Unexpected function calls
- Deep call stacks
- Hot loops
- Memory allocations in hot paths

### 4. Implement Fix

Common optimizations:
- Reduce allocations (use `&str` instead of `String`)
- Cache expensive computations
- Use `SmallVec` for small collections
- Batch I/O operations
- Use `rayon` for parallel iteration

### 5. Verify Improvement

```bash
# Compare against baseline
cargo bench -- --baseline before_change
```

## Benchmark Best Practices

### Writing Good Benchmarks

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_example(c: &mut Criterion) {
    // Setup OUTSIDE the benchmark loop
    let data = setup_test_data();

    c.bench_function("operation_name", |b| {
        b.iter(|| {
            // Use black_box to prevent optimization
            black_box(expensive_operation(&data))
        })
    });
}
```

### Avoiding Common Pitfalls

1. **Don't benchmark setup**: Move initialization outside `iter()`
2. **Use black_box**: Prevents dead code elimination
3. **Warm up caches**: Criterion handles this, but be aware
4. **Isolate tests**: Each benchmark should be independent
5. **Run multiple times**: Use CI to catch variance

## CI Integration

### GitHub Actions Benchmark Job

```yaml
benchmark:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4

    - name: Run benchmarks
      run: cargo bench --workspace -- --save-baseline pr

    - name: Compare to main
      run: |
        git fetch origin main
        git checkout origin/main -- target/criterion
        cargo bench --workspace -- --baseline main
```

### Regression Thresholds

| Threshold | Action |
|-----------|--------|
| < 10% regression | Pass with note |
| 10-25% regression | Warning, review required |
| > 25% regression | CI failure |

## Useful Links

- [Criterion.rs User Guide](https://bheisler.github.io/criterion.rs/book/)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Flamegraph](https://github.com/flamegraph-rs/flamegraph)
- [tokio-console](https://github.com/tokio-rs/console)
