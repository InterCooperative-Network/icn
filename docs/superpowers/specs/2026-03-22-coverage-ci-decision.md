# Coverage CI Decision — p24-pre-2

**Date:** 2026-03-22
**Status:** Decision made — pending implementation
**Author:** Sprint 23 close session

---

## Files Inspected

- `.github/workflows/ci.yml` (lines 414–466, the `coverage` job)
- `ops/state/ci-exceptions.md` (exception classification from s23-t1)
- ci-runner specs via SSH (10.8.30.46)

---

## Current Setup

**Job:** `Test Coverage` in `ci.yml`
**Gate:** `GATE_RATCHET_PHASE_COVERAGE: observational` — non-blocking
**Runner:** `ubuntu-latest` (GitHub-hosted)
**Timeout:** `timeout-minutes: 45`
**Toolchain:** `dtolnay/rust-toolchain@stable` (NOT the pinned 1.88.0)
**Tool:** `cargo-tarpaulin` — installed fresh each run (`cargo install cargo-tarpaulin --locked`)
**Command:** `cargo tarpaulin --workspace --timeout 300 --out Xml --output-dir ./coverage`

Notable workarounds already in place:
- Aggressive disk space cleanup before tarpaulin runs
- No `rust-cache` on this job (intentional — tarpaulin needs fresh instrumented builds)
- `continue-on-error` active (tarpaulin failures don't block merges)
- Codecov upload skipped if XML doesn't exist

---

## Confirmed Failure Shape

From the s23-t1 diagnosis (CI run logs, 2026-03-22):

> `The runner has received a shutdown signal.` at the ~28-minute mark.

Tarpaulin compiles the entire 34-crate workspace with ptrace instrumentation (`cargo clean` + full recompile). The GitHub-hosted `ubuntu-latest` runner is a spot instance with 2 vCPU / 7GB RAM / ~14GB effective disk. The instrumented build exceeds runner lifetime before tarpaulin reaches test execution — no coverage XML is ever produced, Codecov upload is skipped, job fails.

This is not OOM. It is spot preemption — the runner is killed mid-build because GitHub's spot market reclaimed it. It is also not a tarpaulin correctness bug. It is a resource mismatch between job scope and runner lifetime.

---

## Self-Hosted Runner State (ci-runner, 10.8.30.46)

| Resource | Value |
|----------|-------|
| Disk | 77G total, 46G free |
| RAM | 3.8GB total, ~1.2GB available |
| Swap | 8GB, 223MB used |
| sccache | Installed, 10G cache, `RUSTC_WRAPPER=sccache` |
| Labels | `self-hosted,linux,x64,homelab,k3s` |

The disk is adequate. RAM is tight (3.8GB total) — tarpaulin's ptrace instrumentation is memory-intensive and may OOM during a 34-crate full recompile. sccache does NOT help tarpaulin because tarpaulin bypasses normal cargo compilation.

---

## Option Comparison

### Path A — Keep tarpaulin, move to self-hosted runner

Change `runs-on: ubuntu-latest` → `runs-on: [self-hosted, linux, x64, homelab]`.

| Factor | Assessment |
|--------|-----------|
| Effort | Minimal — one line in ci.yml |
| Spot preemption | Eliminated |
| Disk | Fine (46G free) |
| RAM | Risk: 3.8GB total may OOM during instrumented 34-crate build |
| sccache | Does not help tarpaulin (ptrace requires fresh builds) |
| Toolchain drift | Still using `dtolnay@stable` instead of pinned 1.88.0 |
| Reliability | Better than GitHub-hosted, but RAM OOM is plausible |

### Path B — Switch to llvm-cov, move to self-hosted runner

Replace tarpaulin with `cargo-llvm-cov`. Uses LLVM source-based coverage instrumentation instead of ptrace.

| Factor | Assessment |
|--------|-----------|
| Effort | ~2 hours: install llvm-tools-preview on ci-runner, update ci.yml, test |
| Spot preemption | Eliminated (self-hosted) |
| Disk | Fine |
| RAM | Lower overhead — LLVM instrumentation doesn't force full ptrace recompile |
| sccache | **Works with llvm-cov** — incremental builds are possible |
| Toolchain | Can use the pinned 1.88.0 toolchain (`llvm-tools-preview` is a component, not a toolchain) |
| Reliability | High — llvm-cov + sccache on persistent runner is the standard approach for large workspaces |
| Build time | Estimated 8–15 min vs 28+ min (sccache hits, no ptrace overhead) |

---

## Recommendation: Path B

**Switch to `cargo-llvm-cov` on the self-hosted runner.**

Reasons:
1. sccache works with llvm-cov — on subsequent runs, only changed crates recompile. First run is slower; thereafter fast.
2. RAM constraint is avoided — llvm-cov instrumentation does not require the same ptrace-level recompile footprint as tarpaulin.
3. Toolchain alignment — can use the pinned 1.88.0 toolchain, removing the `dtolnay@stable` drift introduced by the current setup.
4. Reliability — self-hosted + sccache + llvm-cov is a production-standard pattern for large Rust workspaces.

Path A is tempting for its simplicity but the RAM risk on ci-runner is real. An OOM crash on the self-hosted runner is worse than a GitHub spot preemption — it can corrupt the sccache and requires manual cleanup.

---

## Exact Next Actions

### Step 1 — Prepare ci-runner

```bash
ssh ubuntu@10.8.30.46
rustup component add llvm-tools-preview  # may already be present
cargo install cargo-llvm-cov --locked
# Verify:
cargo llvm-cov --version
```

### Step 2 — Update ci.yml coverage job

Replace the coverage job (lines 414–466) with:

```yaml
  coverage:
    name: Test Coverage
    needs: [changes]
    if: needs.changes.outputs.docs_only != 'true'
    timeout-minutes: 30
    runs-on: [self-hosted, linux, x64, homelab]
    steps:
      - uses: actions/checkout@v6

      - name: Set up Rust toolchain
        # Uses the pinned toolchain from rust-toolchain.toml (1.88.0)
        run: rustup show

      - name: Install cargo-llvm-cov
        run: cargo install cargo-llvm-cov --locked

      - name: Generate coverage
        run: cargo llvm-cov --workspace --lcov --output-path ./coverage/lcov.info
        working-directory: ./icn
        continue-on-error: ${{ env.GATE_RATCHET_PHASE_COVERAGE != 'blocking' }}

      - name: Upload coverage to Codecov
        if: hashFiles('./icn/coverage/lcov.info') != ''
        uses: codecov/codecov-action@v5
        with:
          files: ./icn/coverage/lcov.info
          fail_ci_if_error: false
```

Changes from current:
- `runs-on`: `ubuntu-latest` → `[self-hosted, linux, x64, homelab]`
- `timeout-minutes`: 45 → 30
- Removed: `dtolnay/rust-toolchain@stable` (uses pinned toolchain)
- Removed: `Free disk space` step (not needed on ci-runner)
- Removed: `Install build tools` step (ci-runner already has them)
- Removed: `Install cargo-tarpaulin` (replaced by `cargo-llvm-cov`)
- Removed: Pre-tarpaulin disk cleanup
- Output: LCOV format instead of Cobertura XML (Codecov accepts both)

### Step 3 — Test locally before committing

```bash
# On ci-runner:
cd /path/to/icn/icn
cargo llvm-cov --workspace --lcov --output-path /tmp/lcov.info 2>&1 | tail -20
```

If this completes in under 20 minutes and produces `/tmp/lcov.info`, the migration is viable.

### Step 4 — Create PR

The CI workflow is in `.github/workflows/ci.yml`. Branch protection prevents direct push to main.
Create a PR: `chore(ci): migrate coverage to llvm-cov on self-hosted runner`

---

## Definition of "Resolved Enough to Stop Being Sprint Drag"

The Coverage CI is resolved when ONE of the following is true:
1. **Path B implemented:** `cargo llvm-cov` on `ci-runner` completes a full workspace run and uploads to Codecov. Gate remains `observational` until coverage baseline is established.
2. **Path A confirmed safe:** A test run of tarpaulin on ci-runner (with swap available) completes without OOM. This requires a test run, not just theory.

"Acknowledged observational exception" (current state) does not count as resolved — it means the job never produces data.

The gate can remain `observational` during the transition. The metric being resolved is: does coverage data actually reach Codecov on each push? Currently it never does.

---

## What This Does NOT Change

- Sprint 24 spine: #925, #947, #964 — unaffected
- Branch protection: no changes
- Test jobs: unaffected — this is coverage-only
- The gate stays `observational` until there's a coverage baseline to enforce
