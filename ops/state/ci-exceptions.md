# CI Exception Registry

This file classifies non-blocking CI failures on `main` with explicit rationale.
Entries here represent intentional acceptance of observed state, not negligence.

## Active Exceptions

### Test Coverage (`cargo-tarpaulin`)
- **Gate classification**: `GATE_RATCHET_PHASE_COVERAGE: observational`
- **Status**: Non-blocking (observational gate, does not prevent merges)
- **Root cause**: `cargo-tarpaulin` v0.35.2 performs a full clean instrumented build
  (`cargo clean` then recompile all 34+ workspace crates with coverage instrumentation)
  before running any tests. On GitHub-hosted `ubuntu-latest` runners, this compilation
  phase alone takes 28+ minutes, and the runner receives a shutdown signal mid-compilation,
  killing the job before tarpaulin can execute tests or produce the cobertura XML report.
  The job `timeout-minutes` is set to 45, but the runner is killed before that limit is
  reached — this appears to be a GitHub-hosted runner lifecycle event, not a job timeout.
  The `continue-on-error` flag on the "Generate coverage" step causes that step to report
  `success` even when killed, but the Codecov upload is `skipped` because no XML exists.
- **Failure mode**: Runner shutdown signal (`##[error]The runner has received a shutdown
  signal. This can happen when the runner service is stopped, or a manually started runner
  is canceled.`) during tarpaulin's instrumented compilation phase. The cobertura XML
  (`./icn/coverage/cobertura.xml`) is never produced. Codecov upload step is skipped.
  Overall job conclusion: `failure`. Observed consistently across all three most recent
  CI runs (IDs: 23399051170, 23398872803, 23397388128) as of 2026-03-22.
- **Resolution path**: Options ranked by feasibility:
  1. Run tarpaulin only on a subset of crates (e.g., `--packages icn-core icn-gateway`)
     to reduce build time below the runner lifetime threshold.
  2. Add `--skip-clean` flag to reuse existing build artifacts (requires cache warming,
     but tarpaulin instrumentation may not be compatible).
  3. Switch to a self-hosted runner (`ci-runner` at 10.8.30.46) which has persistent
     sccache and avoids runner lifecycle kills — though runner capacity is limited.
  4. Accept as permanently observational given workspace size trajectory.
- **Accepted**: 2026-03-22
- **Owner**: Sprint 23 baseline lock
