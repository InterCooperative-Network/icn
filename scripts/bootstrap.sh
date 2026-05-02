#!/usr/bin/env bash
# ICN development environment bootstrap.
#
# Installs system packages, ensures the correct Rust toolchain, installs
# Node.js (if missing), fetches Rust dependencies, and installs JS deps.
#
# Designed to be the **single setup entry-point** called by:
#   - Contributors:        ./scripts/bootstrap.sh
#   - Devcontainers:       .devcontainer/devcontainer.json  (postCreateCommand)
#   - Cursor Cloud agents: VM update script
#   - CI runners:          directly in workflow steps
#
# The script is idempotent — safe to re-run at any time.
#
# Options:
#   --ci          Skip optional cargo dev tools (faster for CI / cloud agents)
#   --no-sysdeps  Skip system package installation (use when you can't sudo)

set -euo pipefail

# ─── Defaults ─────────────────────────────────────────────────────────
INSTALL_CARGO_TOOLS=true
INSTALL_SYSPACKAGES=true

for arg in "$@"; do
    case "$arg" in
        --ci)          INSTALL_CARGO_TOOLS=false ;;
        --no-sysdeps)  INSTALL_SYSPACKAGES=false ;;
    esac
done

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

ok()   { printf "${GREEN}✓${NC} %s\n" "$1"; }
warn() { printf "${YELLOW}!${NC} %s\n" "$1"; }
fail() { printf "${RED}✗${NC} %s\n" "$1"; }

has_cmd() { command -v "$1" &>/dev/null; }

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ─── System packages ─────────────────────────────────────────────────

install_sys_packages() {
    echo "=== System Dependencies ==="
    local needed=()
    for pkg in clang mold pkg-config protoc; do
        if ! has_cmd "$pkg"; then
            needed+=("$pkg")
        else
            ok "$pkg found"
        fi
    done

    # libssl-dev has no binary — check via pkg-config
    if ! pkg-config --exists openssl 2>/dev/null; then
        needed+=(libssl-dev)
    else
        ok "libssl-dev found"
    fi

    if [ ${#needed[@]} -eq 0 ]; then
        ok "All system dependencies present"
        echo ""
        return
    fi

    # Map command names to package names
    local pkgs=()
    for n in "${needed[@]}"; do
        case "$n" in
            protoc) pkgs+=(protobuf-compiler) ;;
            *)      pkgs+=("$n") ;;
        esac
    done

    if [ "$(id -u)" -eq 0 ]; then
        apt-get update -qq && apt-get install -y -qq "${pkgs[@]}"
        ok "Installed: ${pkgs[*]}"
    elif has_cmd sudo; then
        sudo apt-get update -qq && sudo apt-get install -y -qq "${pkgs[@]}"
        ok "Installed: ${pkgs[*]}"
    else
        fail "Missing packages: ${pkgs[*]}"
        echo "  Install manually: apt-get install -y ${pkgs[*]}"
    fi
    echo ""
}

# ─── Rust toolchain ───────────────────────────────────────────────────

setup_rust() {
    echo "=== Rust Toolchain ==="
    if ! has_cmd rustup; then
        fail "rustup not found — install from https://rustup.rs"
        exit 1
    fi

    if [ -f "$REPO_ROOT/icn/rust-toolchain.toml" ]; then
        # Just entering the icn/ dir triggers rustup to install the pinned version
        (cd "$REPO_ROOT/icn" && rustup show active-toolchain 2>/dev/null | head -1)
        ok "Toolchain managed by icn/rust-toolchain.toml"
    fi
    echo ""
}

# ─── Node.js ──────────────────────────────────────────────────────────

setup_node() {
    echo "=== Node.js ==="
    if has_cmd node; then
        local ver
        ver=$(node --version)
        local major=${ver#v}
        major=${major%%.*}
        if [ "$major" -ge 18 ]; then
            ok "Node.js $ver (>= 18)"
            echo ""
            return
        else
            warn "Node.js $ver is below 18 — upgrading"
        fi
    fi

    echo "  Installing Node.js 20..."
    if [ "$(id -u)" -eq 0 ] || has_cmd sudo; then
        local sudo_cmd=""
        [ "$(id -u)" -ne 0 ] && sudo_cmd="sudo"
        curl -fsSL https://deb.nodesource.com/setup_20.x | $sudo_cmd bash - >/dev/null 2>&1
        $sudo_cmd apt-get install -y -qq nodejs >/dev/null 2>&1
        ok "Node.js $(node --version) installed"
    elif has_cmd nvm; then
        nvm install 20 && nvm use 20
        ok "Node.js $(node --version) installed via nvm"
    else
        fail "Cannot install Node.js — no root access and nvm not found"
        echo "  Install Node.js >= 18 manually"
    fi
    echo ""
}

# ─── Rust dependencies ───────────────────────────────────────────────

fetch_rust_deps() {
    echo "=== Rust Dependencies ==="
    (cd "$REPO_ROOT/icn" && cargo fetch --quiet)
    ok "cargo fetch complete"
    echo ""
}

# ─── JS/TS dependencies ──────────────────────────────────────────────

install_js_deps() {
    echo "=== JavaScript Dependencies ==="
    if ! has_cmd npm; then
        warn "npm not found — skipping JS dependency install"
        echo ""
        return
    fi

    if [ -f "$REPO_ROOT/sdk/typescript/package.json" ]; then
        (cd "$REPO_ROOT/sdk/typescript" && npm ci --ignore-scripts --no-audit --no-fund --loglevel=error)
        ok "sdk/typescript deps installed"
    fi
    echo ""
}

# ─── Optional cargo dev tools ─────────────────────────────────────────

install_cargo_tool() {
    local name="$1"
    local pkg="${2:-$1}"
    if has_cmd "$name"; then
        ok "$name already installed"
        return
    fi
    if has_cmd cargo-binstall; then
        cargo binstall "$pkg" --locked --no-confirm 2>/dev/null && { ok "$name installed via binstall"; return; }
    fi
    cargo install "$pkg" --locked && ok "$name installed" || warn "Failed to install $name"
}

install_dev_tools() {
    echo "=== Cargo Dev Tools ==="
    if ! has_cmd cargo-binstall; then
        echo "  Installing cargo-binstall..."
        cargo install cargo-binstall --locked 2>/dev/null && ok "cargo-binstall installed" || warn "cargo-binstall failed"
    else
        ok "cargo-binstall already installed"
    fi

    install_cargo_tool cargo-nextest cargo-nextest
    install_cargo_tool cargo-deny cargo-deny
    install_cargo_tool cargo-audit cargo-audit
    install_cargo_tool cargo-llvm-cov cargo-llvm-cov
    install_cargo_tool cargo-machete cargo-machete

    if has_cmd just; then
        ok "just already installed"
    else
        if has_cmd apt-get && ([ "$(id -u)" -eq 0 ] || has_cmd sudo); then
            local sudo_cmd=""
            [ "$(id -u)" -ne 0 ] && sudo_cmd="sudo"
            $sudo_cmd apt-get install -y -qq just 2>/dev/null && ok "just installed via apt" || install_cargo_tool just
        else
            install_cargo_tool just
        fi
    fi
    echo ""
}

# ─── Summary ──────────────────────────────────────────────────────────

print_summary() {
    echo "=== Versions ==="
    echo "  rustc:  $(rustc --version 2>/dev/null || echo 'not found')"
    echo "  cargo:  $(cargo --version 2>/dev/null || echo 'not found')"
    echo "  node:   $(node --version 2>/dev/null || echo 'not found')"
    echo "  npm:    $(npm --version 2>/dev/null || echo 'not found')"
    echo "  mold:   $(mold --version 2>/dev/null || echo 'not found')"
    echo "  clang:  $(clang --version 2>/dev/null | head -1 || echo 'not found')"
    echo ""
    ok "Bootstrap complete. Verify: cd icn && cargo build && cargo test --workspace --lib"
}

# ─── Main ─────────────────────────────────────────────────────────────

main() {
    echo ""
    echo "=== ICN Development Bootstrap ==="
    echo ""

    [ "$INSTALL_SYSPACKAGES" = true ] && install_sys_packages
    setup_rust
    setup_node
    fetch_rust_deps
    install_js_deps
    [ "$INSTALL_CARGO_TOOLS" = true ] && install_dev_tools
    print_summary
}

main
