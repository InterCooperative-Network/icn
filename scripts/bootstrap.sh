#!/usr/bin/env bash
# ICN development environment bootstrap.
# Installs required tools and checks system dependencies.
# Run: ./scripts/bootstrap.sh

set -euo pipefail

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

ok()   { printf "${GREEN}✓${NC} %s\n" "$1"; }
warn() { printf "${YELLOW}!${NC} %s\n" "$1"; }
fail() { printf "${RED}✗${NC} %s\n" "$1"; }

# ─── Helpers ─────────────────────────────────────────────────────────

has_cmd() { command -v "$1" &>/dev/null; }

# Install a cargo tool: try cargo-binstall first, then cargo install.
install_cargo_tool() {
    local name="$1"
    local pkg="${2:-$1}"
    if has_cmd "$name"; then
        ok "$name already installed ($(command -v "$name"))"
        return
    fi
    if has_cmd cargo-binstall; then
        echo "  Installing $pkg via cargo-binstall..."
        cargo binstall "$pkg" --locked --no-confirm 2>/dev/null && { ok "$name installed via binstall"; return; }
    fi
    echo "  Installing $pkg via cargo install (this may take a while)..."
    cargo install "$pkg" --locked && ok "$name installed via cargo install" || fail "Failed to install $name"
}

# ─── Rust toolchain ──────────────────────────────────────────────────

echo "=== Rust Toolchain ==="
if has_cmd rustup; then
    # Ensure the pinned toolchain is installed
    if [ -f icn/rust-toolchain.toml ]; then
        rustup show active-toolchain 2>/dev/null | head -1
        ok "Toolchain managed by icn/rust-toolchain.toml"
    fi
else
    fail "rustup not found — install from https://rustup.rs"
    exit 1
fi
echo ""

# ─── System dependencies ─────────────────────────────────────────────

echo "=== System Dependencies ==="
for dep in clang mold pkg-config; do
    if has_cmd "$dep"; then
        ok "$dep found"
    else
        fail "$dep not found — install with: sudo apt-get install -y $dep"
    fi
done
echo ""

# ─── cargo-binstall (install first, speeds up everything else) ───────

echo "=== Cargo Tools ==="
if ! has_cmd cargo-binstall; then
    echo "  Installing cargo-binstall (speeds up subsequent installs)..."
    cargo install cargo-binstall --locked 2>/dev/null && ok "cargo-binstall installed" || warn "cargo-binstall failed, will use cargo install for remaining tools"
else
    ok "cargo-binstall already installed"
fi

# ─── just ────────────────────────────────────────────────────────────

if has_cmd just; then
    ok "just already installed ($(just --version 2>/dev/null || echo 'unknown version'))"
else
    # Try apt first (Ubuntu 24.04+ has it)
    if sudo apt-get install -y just 2>/dev/null; then
        ok "just installed via apt"
    else
        install_cargo_tool just
    fi
fi

# ─── Cargo development tools ─────────────────────────────────────────

install_cargo_tool cargo-nextest cargo-nextest
install_cargo_tool cargo-deny cargo-deny
install_cargo_tool cargo-audit cargo-audit
install_cargo_tool cargo-llvm-cov cargo-llvm-cov
install_cargo_tool cargo-machete cargo-machete
echo ""

# ─── sccache detection ───────────────────────────────────────────────

echo "=== Environment Check ==="
if grep -q 'rustc-wrapper.*sccache' "$HOME/.cargo/config.toml" 2>/dev/null; then
    warn "Global sccache detected in ~/.cargo/config.toml"
    if [ -f icn/.cargo/config.user.toml ]; then
        ok "Local override exists (icn/.cargo/config.user.toml)"
    else
        warn "If you see SIGSEGV during compilation, run:"
        echo "    cp icn/.cargo/config.user.toml.example icn/.cargo/config.user.toml"
        echo "  Or use: ICN_DISABLE_SCCACHE=1 just test"
    fi
else
    ok "No global sccache detected"
fi
echo ""

# ─── Summary ─────────────────────────────────────────────────────────

echo "=== Tool Versions ==="
echo "  rustc:        $(rustc --version 2>/dev/null || echo 'not found')"
echo "  cargo:        $(cargo --version 2>/dev/null || echo 'not found')"
echo "  just:         $(just --version 2>/dev/null || echo 'not found')"
echo "  cargo-nextest: $(cargo nextest --version 2>/dev/null || echo 'not found')"
echo "  mold:         $(mold --version 2>/dev/null || echo 'not found')"
echo "  clang:        $(clang --version 2>/dev/null | head -1 || echo 'not found')"
echo ""
ok "Bootstrap complete. Run 'just test' to verify."
