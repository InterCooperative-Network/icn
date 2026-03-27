---
name: bench
description: Run criterion benchmarks locally for one or all ICN crates. CI runs these on ubuntu-latest; local runs on icn-dev are for relative comparison only.
argument-hint: "[crate-name | all]"
user-invocable: true
allowed-tools: "Bash"
---

Run criterion benchmarks for the specified crate, or all six targets.

## Benchmark targets

| Crate | Bench file |
|-------|-----------|
| icn-ledger | ledger_bench |
| icn-gossip | gossip_bench |
| icn-trust | trust_bench |
| icn-net | net_bench |
| icn-compute | compute_bench |
| icn-gateway | commons_bench |

All commands run from `icn/icn/` (the Cargo workspace root).

## Steps

1. Confirm working directory: `cd /home/ubuntu/projects/icn/icn`

2. If `$ARGUMENTS` is a crate name (e.g. `gossip`), look up its bench file above and run:
   ```bash
   cargo bench -p icn-<crate> --bench <bench_file>
   ```
   Example: `cargo bench -p icn-gossip --bench gossip_bench`

3. If `$ARGUMENTS` is `all` or empty, run all six:
   ```bash
   cargo bench --bench compute_bench --bench commons_bench --bench gossip_bench \
     --bench ledger_bench --bench net_bench --bench trust_bench
   ```

4. Criterion prints regression vs previous run automatically. Flag regressions in
   `gossip_bench`, `net_bench`, or `ledger_bench` as high-priority (protocol-critical paths).

## Notes

- Local results reflect icn-dev hardware, not ubuntu-latest CI. Use for relative comparison only.
- Benchmarks are slow. Run scoped to the crate you are touching.
- Do not commit `target/criterion/` artifacts.
- "No samples were collected" means the bench binary failed to build — run
  `cargo check -p <crate>` first.
