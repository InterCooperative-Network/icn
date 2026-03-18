#!/usr/bin/env bash
# ICN Preflight Check — standalone version of /icn-preflight skill
# Run at session start to verify environment state.
# Exit 1 on fatal mismatch, exit 0 on all-clear.
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}  ok${NC}  $1"; }
warn() { echo -e "${YELLOW}  !!${NC}  $1"; }
fail() { echo -e "${RED}  FAIL${NC}  $1"; FATAL=1; }

FATAL=0

echo "icn-preflight"

# 1. Repo + branch
if git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
  BRANCH=$(git branch --show-current 2>/dev/null || echo "(detached)")
  pass "branch: $BRANCH"
else
  fail "not in a git repo"
fi

# 2. gh auth
if gh auth status > /dev/null 2>&1; then
  GH_USER=$(gh api user --jq .login 2>/dev/null || echo "unknown")
  pass "gh: authenticated as $GH_USER"
else
  fail "gh: not authenticated (run gh auth login)"
fi

# 3. Port check
if ss -tlnp 2>/dev/null | grep -q ':8080 '; then
  pass "port 8080: listening"
else
  warn "port 8080: not listening (gateway not running)"
fi
if ss -tlnp 2>/dev/null | grep -q ':8000 '; then
  warn "port 8000: something listening (should be 8080)"
fi

# 4. Toolchain
RUSTC_VER=$(rustc --version 2>/dev/null | awk '{print $2}' || echo "missing")
EXPECTED=""
for toolchain_file in icn/rust-toolchain.toml icn/icn/rust-toolchain.toml rust-toolchain.toml; do
  if [ -f "$toolchain_file" ]; then
    EXPECTED=$(grep 'channel' "$toolchain_file" 2>/dev/null | sed 's/.*= *"\([^"]*\)".*/\1/' || echo "")
    break
  fi
done
if [ -n "$EXPECTED" ] && [ "$RUSTC_VER" != "$EXPECTED" ]; then
  warn "toolchain: $RUSTC_VER (expected $EXPECTED)"
else
  pass "toolchain: $RUSTC_VER"
fi

# 5. Cargo check (only if in ICN repo context)
CARGO_DIR=""
for d in icn/icn icn .; do
  if [ -f "$d/Cargo.toml" ]; then
    CARGO_DIR="$d"
    break
  fi
done
if [ -n "$CARGO_DIR" ]; then
  if cargo check --workspace --manifest-path "$CARGO_DIR/Cargo.toml" 2>&1 | tail -1 | grep -q "Finished"; then
    pass "cargo check: ok"
  else
    fail "cargo check: failed (run from $CARGO_DIR)"
  fi
else
  warn "cargo check: no Cargo.toml found"
fi

exit $FATAL
