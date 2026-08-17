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

# ─── Website ─────────────────────────────────────────────────────────
# The public site at intercooperative.network. Everything below runs
# from `website/` and needs `npm ci` there once.
#
# Before pushing a website or docs change, run `just website-verify`.
# It is what CI runs, in the same order.

# Install website dependencies (respects the lockfile)
website-install:
    cd website && npm ci

# Regenerate the projections of canonical repo state
website-generate:
    cd website && npm run generate

# Build the static site
website-build:
    cd website && npm run build

# Non-browser checks: types, public-state projection, docs boundary,
# internal links, walkthrough fixture safety. Requires a build first.
website-check:
    cd website && npm run check

# Rendered-page audit: overflow, heading outline, landmarks, text size,
# image/SVG labelling. Representative matrix (7 pages x 3 widths).
# Serves dist/ itself on an ephemeral port — no preview server to manage.
website-audit:
    cd website && npm run audit

# The full audit matrix (12 pages x 5 widths). Slower; used by the
# scheduled workflow.
website-audit-full:
    cd website && npm run audit:full

# Readiness/claim linting over public-facing content
website-claims:
    python3 .github/scripts/readiness_overclaim_linter.py --repo-root . --config .github/claim-lint-website.json

# Everything CI runs for a website change, in CI's order and at CI's depth.
# Use `just website-audit` on its own for a faster loop while iterating.
website-verify: website-build website-check website-claims website-audit-full
    @echo "website-verify: all checks passed"
