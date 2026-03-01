#!/usr/bin/env bash
# scripts/whereami.sh — prints which repo root you're in
# Prevents the two-root confusion class of errors (monorepo root vs Rust workspace).
# See CLAUDE.md "Repo Topology (Two Roots)" section.

set -euo pipefail

echo "git top-level: $(git rev-parse --show-toplevel 2>/dev/null || echo '(not in a git repo)')"

if [ -f Cargo.toml ]; then
    echo "✓ Rust workspace root (Cargo.toml found)"
else
    echo "✗ Not Rust workspace root (no Cargo.toml here)"
fi

if [ -f sdk/typescript/package.json ]; then
    echo "✓ Monorepo root (sdk/typescript/package.json found)"
else
    echo "  sdk/typescript/package.json not found (not monorepo root)"
fi
