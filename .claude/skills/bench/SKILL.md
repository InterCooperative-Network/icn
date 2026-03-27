---
name: bench
description: Run criterion benchmarks locally for one or all ICN crates. CI runs these on ubuntu-latest; local runs on icn-dev are for relative comparison only.
argument-hint: "[crate-name | all]"
user-invocable: true
allowed-tools: "Bash"
---

Run criterion benchmarks for the specified crate, or all six targets.

## Benchmark targets

Pass the short name (without `icn-` prefix) as `$ARGUMENTS`, e.g. `gossip` for `icn-gossip`.

| Short name | Full crate | Bench file |
|-----------|-----------|-----------|
| ledger | icn-ledger | ledger_bench |
| gossip | icn-gossip | gossip_bench |
| trust | icn-trust | trust_bench |
| net | icn-net | net_bench |
| compute | icn-compute | compute_bench |
| gateway | icn-gateway | commons_bench |

All commands run from the Rust workspace root (`icn/` within the monorepo).

## Steps

1. Confirm working directory: `cd "$CLAUDE_PROJECT_DIR/icn"`

2. If `$ARGUMENTS` is a short crate name (e.g. `gossip`), look up its bench file above and run:
   ```bash
   cargo bench -p icn-<short-name> --bench <bench_file>
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
