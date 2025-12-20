# ICN Performance Documentation

This directory contains performance benchmarks, profiling guides, and optimization documentation.

## Contents

- [BENCHMARKS.md](BENCHMARKS.md) - Detailed benchmark results and baseline metrics
- [PROFILING.md](PROFILING.md) - Guide to profiling and performance analysis
- [TARGETS.md](TARGETS.md) - Performance targets and SLOs

## Quick Start

### Run Benchmarks

```bash
cd icn
cargo bench --workspace
```

### View Benchmark Reports

After running benchmarks, open the HTML report:

```bash
open target/criterion/report/index.html
```

### Performance Targets

| Operation | Target (P95) | Current |
|-----------|--------------|---------|
| Trust computation | < 1 µs | 44 ns |
| Transaction signing | < 20 µs | 14 µs |
| Transaction verification | < 50 µs | 25 µs |
| Gossip propagation (10 nodes) | < 1 sec | 380 ms |
| Balance query | < 100 µs | < 1 µs |

## CI Integration

Benchmarks run on each PR. Regressions > 10% trigger warnings; > 25% fail CI.

See [GitHub Actions workflow](../../.github/workflows/ci.yml) for details.
