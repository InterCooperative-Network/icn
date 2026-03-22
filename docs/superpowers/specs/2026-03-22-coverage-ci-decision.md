# Coverage CI Decision — p24-pre-2

**Date:** 2026-03-22
**Status:** Revised — ci-runner validated insufficient; estate census complete; new path selected
**Author:** Sprint 23 close session + live validation

---

## Files Inspected

- `.github/workflows/ci.yml` (lines 414–466, the `coverage` job)
- `ops/state/ci-exceptions.md` (exception classification from s23-t1)
- ci-runner (10.8.30.46) — SSH live testing
- k3s-worker-1 (10.8.30.41) — SSH census
- k3s-worker-2 (10.8.30.42) — SSH census
- k3s-control (10.8.30.40) — SSH census

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

This is not OOM. It is spot preemption — the runner is killed mid-build because GitHub's spot market reclaimed it.

---

## Live Validation Results (2026-03-22)

### What was tested

Attempted two runs of `cargo-llvm-cov` on ci-runner (10.8.30.46):

**Run 1 — full workspace:**
```bash
cargo +1.88.0 llvm-cov --workspace --lcov --output-path /tmp/lcov.info
```
- Result: Never completed after 45+ minutes
- Swap: rose to 5.6GB used (8GB total), high I/O wait
- No lcov.info produced
- Killed manually

**Run 2 — lib only (unit tests only, no integration test binaries):**
```bash
cargo +1.88.0 llvm-cov --lib --workspace --lcov --output-path /tmp/lcov-lib.info
```
- Result: Still compiling after 50+ minutes, swap reached 6.8GB
- Final log: `Compiling icnd`, `icn-console`, `icnctl` (bins compiled last despite `--lib`)
- No lcov-lib.info produced
- Killed manually

**Conclusion:** `cargo-llvm-cov` does NOT avoid compiling binaries even with `--lib`. The full dependency graph is compiled. On a 3.8GB RAM machine with 8GB swap, the 34-crate workspace instrumented build saturates swap before compilation completes. The prior estimate ("8–15 min vs 28+ min") was wrong — it assumed sccache would make subsequent runs fast, but the first run cannot complete at all.

The "RAM constraint is avoided" claim in the earlier analysis was incorrect for this hardware.

---

## Infrastructure Census (2026-03-22)

Full inventory of accessible nodes from icn-dev:

| Node | IP | CPU | RAM | RAM Free | Disk Free | Notes |
|------|-----|-----|-----|----------|-----------|-------|
| **ci-runner** | 10.8.30.46 | i7-7700K 4c/4t @ 4.2GHz | 3.8GB | ~0.3GB | 39GB | Hyperion VM 446; current GH runner; **proven insufficient** |
| **k3s-control** | 10.8.30.40 | 4c | 7.8GB | 6.5GB | unknown | Control plane, tainted NoSchedule; not a CI candidate |
| **k3s-worker-1** | 10.8.30.41 | i5-6500 4c/4t @ 3.2GHz | 15GB | **14GB** | 28GB | ICN pods; no Rust installed; disk may be tight |
| **k3s-worker-2** | 10.8.30.42 | i5-6500 4c/4t @ 3.2GHz | 15GB | **14GB** | 29GB | ICN pods; no Rust installed; disk may be tight |
| **Hyperion** | 10.8.10.15 | Ryzen 9 3900X 12c/24t | 15GB (bad) | — | — | Proxmox; RAM RMA in progress; offline for CI |
| **Zentith** | 10.8.10.100 | Ryzen 7 7800X3D 8c/16t | **54GB** | high | — | Matt's workstation; WSL2 Ubuntu 24.04; strongest available |

Proxmox nodes node-1 through node-4 (10.8.10.11–14): SSH not authorized from icn-dev; capacity unknown.

---

## Corrected Problem Statement

The problem is not "tarpaulin vs llvm-cov." It is "the current ci-runner has insufficient RAM (3.8GB) for any full-workspace Rust coverage instrumentation on a 34-crate codebase." Both tarpaulin and llvm-cov fail at compilation before reaching test execution. The tool choice is secondary to the hardware constraint.

---

## Option Comparison (Revised)

### Path A — Increase ci-runner RAM on Hyperion

`qm set 446 --memory 8192` (or higher) from Proxmox.

| Factor | Assessment |
|--------|-----------|
| Effort | One Proxmox command |
| Risk | ci-runner is VM 446 on Hyperion, which has bad RAM pending RMA. Adding memory pressure to a failing host is inadvisable. |
| Viability | Blocked until Hyperion RMA completes |

**Verdict:** Not viable now.

### Path B — Use Zenith as temporary coverage runner

Register Zentith WSL2 (Ubuntu 24.04) as a self-hosted runner with a `coverage` label.

| Factor | Assessment |
|--------|-----------|
| RAM | 54GB — no constraints |
| CPU | Ryzen 7 7800X3D 8c/16t — faster than any other available host |
| Disk | Sufficient |
| Rust | Likely already present; install 1.88.0 + cargo-llvm-cov |
| Always-on | No — requires Windows running. Acceptable during active development phase. |
| Effort | ~30 min: register runner in WSL2, install tooling, update ci.yml label |
| Impact | Coverage starts working immediately |
| Long-term | Move to Hyperion (post-RMA) or dedicated VM; deregister Zentith |

**Verdict:** Best immediate path. Honest about the tradeoff (not always-on).

### Path C — Use k3s-worker-1 as coverage runner

Register k3s-worker-1 as a runner alongside K3s.

| Factor | Assessment |
|--------|-----------|
| RAM | 15GB available — sufficient |
| CPU | i5-6500 4c/4t — adequate |
| Disk | 28GB free — tight; a full instrumented build target dir can reach 20GB+ |
| Conflict | Could compete with K3s pod scheduling during coverage runs |
| Effort | ~1 hr: install Rust, runner agent, update ci.yml |
| Risk | Disk pressure; K3s scheduling conflicts if pods land during coverage run |

**Verdict:** Viable fallback if Zenith is unavailable. Disk is the binding concern.

### Path D — Explicit CI exception, defer to Hyperion post-RMA

Document that coverage is deferred infrastructure work, keep `observational` gate, do nothing now.

| Factor | Assessment |
|--------|-----------|
| Effort | Zero |
| Sprint impact | Sprint 24 opens cleanly; coverage stays non-functional |
| Honesty | Accurate — the gate is already `observational`, exception already documented |
| Risk | Coverage data never reaches Codecov; third sprint of zero coverage data |

**Verdict:** Acceptable if bandwidth is the constraint. Worse for the project than Path B.

---

## Recommendation: Path B (Zentith temporary runner)

**Register Zentith WSL2 as a self-hosted runner for coverage only.**

Steps:

### Step 1 — On Zentith WSL2, register the runner

```bash
# In WSL2 Ubuntu-24.04 on Zentith
mkdir ~/actions-runner-coverage && cd ~/actions-runner-coverage
# Download current runner (check https://github.com/InterCooperative-Network/icn/settings/actions/runners/new)
curl -o actions-runner-linux-x64.tar.gz -L <runner-download-url>
tar xzf ./actions-runner-linux-x64.tar.gz
./config.sh \
  --url https://github.com/InterCooperative-Network/icn \
  --token <REGISTRATION_TOKEN> \
  --name "zentith-coverage" \
  --labels "self-hosted,linux,x64,coverage,zenith"
./run.sh  # or install as service: sudo ./svc.sh install && sudo ./svc.sh start
```

Get the registration token from:
`https://github.com/InterCooperative-Network/icn/settings/actions/runners/new`

### Step 2 — On Zentith WSL2, install Rust coverage tooling

```bash
rustup toolchain install 1.88.0
rustup component add llvm-tools-preview --toolchain 1.88.0
cargo +1.88.0 install cargo-llvm-cov --locked
# Verify:
cargo +1.88.0 llvm-cov --version
```

### Step 3 — Validate on Zentith before changing ci.yml

```bash
cd /path/to/icn/icn  # the Rust workspace root
cargo +1.88.0 llvm-cov --workspace --lcov --output-path /tmp/lcov.info 2>&1 | tail -20
# If lcov.info exists and run completes < 20 min: proceed
```

### Step 4 — Update ci.yml coverage job

Change the `runs-on` label from `ubuntu-latest` to the new runner label:

```yaml
  coverage:
    name: Test Coverage
    needs: [changes]
    if: needs.changes.outputs.docs_only != 'true'
    timeout-minutes: 30
    runs-on: [self-hosted, linux, x64, coverage]
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

Note: `runs-on: [self-hosted, linux, x64, coverage]` uses the `coverage` label. Only Zentith (or a future dedicated runner) carries this label. The existing ci-runner keeps `homelab,k3s` labels and handles other jobs.

### Step 5 — After Hyperion RMA

When Hyperion returns with good RAM:
1. Provision a dedicated coverage runner VM (8+ GB RAM, 60GB+ disk, `ubuntu-latest`-style setup)
2. Register with the `coverage` label
3. Deregister Zentith from GitHub Actions
4. The ci.yml `runs-on: [self-hosted, linux, x64, coverage]` requires no change

---

## Definition of "Resolved Enough to Stop Being Sprint Drag"

The Coverage CI is resolved when:
1. A push to main triggers the coverage job
2. The job completes within 30 minutes
3. `lcov.info` is produced and uploaded to Codecov

"Acknowledged observational exception" does not count as resolved — it means the job never produces data.

The gate remains `observational` until a coverage baseline is established after the first successful run.

---

## Sprint Impact

**Sprint 24 can open cleanly.**

Coverage CI is:
- Not a blocking gate (observational)
- Not part of the #925/#947/#964 commons-compute spine
- Now has a concrete resolution path (Zenith runner, ~30 min setup)

p24-pre-2 is complete as a decision artifact. Implementation is a ~30 min ops task on Zentith that does not block Sprint 24 from starting.

---

## What This Does NOT Change

- Sprint 24 spine: #925, #947, #964 — unaffected
- Branch protection: no changes
- Test jobs: unaffected — this is coverage-only
- The gate stays `observational` until there's a coverage baseline to enforce
- Flow 2 (patronage): validated working 2026-03-22, not reopened
