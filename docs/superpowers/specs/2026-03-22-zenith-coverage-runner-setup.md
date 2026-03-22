# Zenith Temporary Coverage Runner Setup

**Date:** 2026-03-22
**Status:** Ready to execute — 30 min ops task
**Context:** [coverage-ci-decision.md](./2026-03-22-coverage-ci-decision.md)

---

## Why Zenith

ci-runner (3.8GB RAM) is proven insufficient for full-workspace `cargo-llvm-cov`. Zenith (54GB RAM, Ryzen 7 7800X3D) has no capacity constraints and is immediately available. It hosts WSL2 Ubuntu 24.04, which can run a GitHub Actions self-hosted runner natively.

This is a temporary assignment. After Hyperion's RAM RMA completes, a proper always-on runner VM will be provisioned there and Zenith will be deregistered. The workflow label stays stable through that transition.

---

## Step 1 — Get the registration token

From a browser, go to:
```
https://github.com/InterCooperative-Network/icn/settings/actions/runners/new
```

Select **Linux / x64**. Copy the registration token (starts with `A`, expires after 1 hour).

Also note the runner download URL from that page — it shows the current version (e.g. `v2.321.0`).

---

## Step 2 — On Zenith, in WSL2 Ubuntu-24.04

```bash
# Create an isolated runner directory (not inside the ICN repo)
mkdir -p ~/actions-runner-coverage && cd ~/actions-runner-coverage

# Download the runner — use the URL from Step 1 (current version shown on GitHub)
# Example (check GitHub for latest version):
curl -o actions-runner-linux-x64.tar.gz -L \
  https://github.com/actions/runner/releases/download/v2.321.0/actions-runner-linux-x64-2.321.0.tar.gz
tar xzf ./actions-runner-linux-x64.tar.gz

# Configure with coverage label — this is the stable interface
./config.sh \
  --url https://github.com/InterCooperative-Network/icn \
  --token <REGISTRATION_TOKEN_FROM_STEP_1> \
  --name "zentith-coverage" \
  --labels "self-hosted,linux,x64,coverage,zentith"
```

When prompted for runner group: accept default (`Default`).
When prompted for work folder: accept default (`_work`).

---

## Step 3 — Install Rust coverage tooling on Zenith WSL2

```bash
# Install or confirm pinned toolchain
rustup toolchain install 1.88.0
rustup component add llvm-tools-preview --toolchain 1.88.0

# Install cargo-llvm-cov
cargo +1.88.0 install cargo-llvm-cov --locked

# Verify
cargo +1.88.0 llvm-cov --version
# Expected: cargo-llvm-cov <version>
```

If `rustup` is not yet on Zenith WSL2:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
```

---

## Step 4 — Start the runner

```bash
cd ~/actions-runner-coverage
./run.sh
```

For a persistent background service (optional, survives WSL2 restart):
```bash
sudo ./svc.sh install
sudo ./svc.sh start
sudo ./svc.sh status
```

Verify it appears online:
```
https://github.com/InterCooperative-Network/icn/settings/actions/runners
```
Look for `zentith-coverage` with status `Idle`.

---

## Step 5 — Validate before touching ci.yml

On Zenith WSL2, with the ICN repo cloned or accessible:

```bash
cd /path/to/icn/icn  # Rust workspace root (contains Cargo.toml)
cargo +1.88.0 llvm-cov --workspace --lcov --output-path /tmp/lcov-validate.info 2>&1 | tail -20
ls -lh /tmp/lcov-validate.info
# Expected: file exists, > 0 bytes, runtime < 20 min
```

Do not update ci.yml until this completes cleanly.

---

## Step 6 — Update ci.yml (minimal diff)

File: `.github/workflows/ci.yml`, the `coverage:` job.

**Before:**
```yaml
  coverage:
    name: Test Coverage
    needs: [changes]
    if: needs.changes.outputs.docs_only != 'true'
    timeout-minutes: 45
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6

      - name: Free disk space
        uses: ./.github/actions/free-disk-space
        with:
          verbose: 'true'
          aggressive: 'true'

      - name: Install build tools
        uses: ./.github/actions/install-build-tools

      - uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-tarpaulin
        run: cargo install cargo-tarpaulin --locked

      - name: Pre-tarpaulin cleanup
        run: |
          rm -rf ~/.cargo/registry/cache || true
          rm -rf ~/.cargo/git/db || true
          df -h /

      - name: Generate coverage
        run: cargo tarpaulin --workspace --timeout 300 --out Xml --output-dir ./coverage
        working-directory: ./icn
        continue-on-error: ${{ env.GATE_RATCHET_PHASE_COVERAGE != 'blocking' }}

      - name: Upload coverage to Codecov
        if: hashFiles('./icn/coverage/cobertura.xml') != ''
        uses: codecov/codecov-action@v5
        with:
          files: ./icn/coverage/cobertura.xml
          fail_ci_if_error: false
```

**After:**
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

**What changed:**
- `runs-on`: `ubuntu-latest` → `[self-hosted, linux, x64, coverage]`
- `timeout-minutes`: 45 → 30
- Removed: `Free disk space`, `Install build tools`, `dtolnay/rust-toolchain@stable`, `Install cargo-tarpaulin`, `Pre-tarpaulin cleanup`
- Coverage tool: tarpaulin → llvm-cov
- Output format: Cobertura XML → LCOV (Codecov accepts both)

Create a PR from a `chore/` branch:
```bash
git checkout -b chore/coverage-llvm-cov-zenith
# edit the file
git add .github/workflows/ci.yml
git commit -m "chore(ci): migrate coverage to llvm-cov on self-hosted coverage runner"
gh pr create --title "chore(ci): migrate coverage to llvm-cov on self-hosted coverage runner" \
  --body "Switches the coverage job from tarpaulin on GitHub-hosted runners to cargo-llvm-cov on the self-hosted coverage runner (Zenith WSL2 temporarily, Hyperion post-RMA).

**Why:** ci-runner (3.8GB RAM) is insufficient for full-workspace instrumented builds. Zenith (54GB RAM) handles it without constraints.

**Gate:** coverage remains \`observational\` — this PR does not block merges.

**Validation:** Full workspace llvm-cov run completed on Zenith WSL2 before this PR was opened.

**Runner label:** \`[self-hosted, linux, x64, coverage]\` — stable across Zenith → Hyperion migration."
```

---

## Rollback

If Zenith becomes unavailable before the PR merges:
- The workflow is unchanged (still on `ubuntu-latest` with tarpaulin)
- No action needed

If Zenith goes offline after the PR merges and the `coverage` label has no active runner:
- Coverage jobs will queue and eventually time out
- Gate is `observational` — no merges are blocked
- Options: (a) restart Zenith WSL2 runner, (b) temporarily re-point label to a k3s-worker

If k3s-worker becomes the temporary fallback:
```bash
# On k3s-worker-1 (10.8.30.41, 15GB RAM, 28GB disk):
curl -fsSL https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.88.0
source ~/.cargo/env
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov --locked
# Register runner with same labels
mkdir ~/actions-runner-coverage && cd ~/actions-runner-coverage
# download + ./config.sh with same --labels as Zenith
```
Disk concern: if the `_work` directory fills (28GB can be tight for large instrumented builds), add a step to `ci.yml` to clean `target/llvm-cov-target` after the run.

---

## Long-Term Migration to Hyperion (post-RMA)

When Hyperion returns with working RAM:

1. Provision a runner VM on Hyperion — recommended spec: 8+ vCPU, 16GB RAM, 100GB disk
2. Install Rust 1.88.0 + cargo-llvm-cov + sccache on the VM
3. Register the runner with labels: `self-hosted,linux,x64,coverage,hyperion`
4. Verify a full coverage run completes cleanly on Hyperion
5. Deregister Zenith: `cd ~/actions-runner-coverage && ./config.sh remove --token <REMOVAL_TOKEN>`
6. The `ci.yml` workflow label `[self-hosted, linux, x64, coverage]` requires **no change**

The label is the stable interface. The machine behind it changes transparently.

---

## Success Condition

Coverage CI is resolved when:
1. A push to `main` triggers the coverage job
2. The job completes in < 30 minutes
3. `lcov.info` is produced and uploaded to Codecov
4. Codecov dashboard shows a coverage percentage

Gate remains `observational` until a baseline is established after the first successful upload.
