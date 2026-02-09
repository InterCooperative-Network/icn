# ICN development commands
# Run `just --list` to see all available recipes.

set dotenv-load := false

# Workspace directory
workspace := "icn"

# Overridable knobs (e.g., JOBS=8 just build)
export JOBS := env("JOBS", "")
export TEST_THREADS := env("TEST_THREADS", "")

# ─── Build ───────────────────────────────────────────────────────────

# Compile the workspace
build *FLAGS:
    cd {{workspace}} && cargo build {{FLAGS}}

# Compile release binaries
build-release *FLAGS:
    cd {{workspace}} && cargo build --release {{FLAGS}}

# Remove build artifacts
clean:
    cd {{workspace}} && cargo clean

# ─── Test ────────────────────────────────────────────────────────────

# Run tests (nextest if available, else cargo test)
test *FLAGS:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{workspace}}
    if command -v cargo-nextest &>/dev/null; then
        cargo nextest run {{FLAGS}}
    else
        cargo test {{FLAGS}}
    fi

# Run tests with reduced parallelism (memory-constrained machines)
test-safe *FLAGS:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{workspace}}
    export CARGO_BUILD_JOBS="${JOBS:-2}"
    export RUST_TEST_THREADS="${TEST_THREADS:-2}"
    if command -v cargo-nextest &>/dev/null; then
        cargo nextest run -j 2 {{FLAGS}}
    else
        cargo test {{FLAGS}}
    fi

# Run tests matching CI exactly (unit parallel, integration serial)
test-ci:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{workspace}}
    echo "=== Unit tests (parallel) ==="
    if command -v cargo-nextest &>/dev/null; then
        cargo nextest run --lib
    else
        cargo test --workspace --lib
    fi
    echo "=== Integration tests (serial) ==="
    cargo test --workspace --test '*' -- --test-threads=1

# ─── Lint ────────────────────────────────────────────────────────────

# Run fmt + clippy
check:
    cd {{workspace}} && cargo fmt --all --check
    cd {{workspace}} && cargo clippy --workspace --all-targets -- -D warnings

# Run fmt only
fmt:
    cd {{workspace}} && cargo fmt --all

# Run clippy only
clippy *FLAGS:
    cd {{workspace}} && cargo clippy --workspace --all-targets {{FLAGS}} -- -D warnings

# ─── Devnet ──────────────────────────────────────────────────────────

# Start 3-node devnet
devnet-up:
    make -C deploy/devnet up

# Stop devnet
devnet-down:
    make -C deploy/devnet down

# Restart devnet
devnet-restart:
    make -C deploy/devnet restart

# Show devnet logs
devnet-logs:
    make -C deploy/devnet logs

# Show devnet status
devnet-status:
    make -C deploy/devnet status

# Run devnet smoke tests
devnet-test:
    make -C deploy/devnet test

# Clean devnet data
devnet-clean:
    make -C deploy/devnet clean

# ─── Utilities ───────────────────────────────────────────────────────

# Run bootstrap script to install dev tools
bootstrap:
    ./scripts/bootstrap.sh

# Run security audits
audit:
    cd {{workspace}} && cargo audit
    cd {{workspace}} && cargo deny check
