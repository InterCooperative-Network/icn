#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GUEST_MANIFEST="$ROOT/icn/crates/icn-baseline-lock-guest/Cargo.toml"
OUT_WASM="$ROOT/icn/crates/icn-baseline-lock/tests/fixtures/icn_baseline_lock_guest.wasm"

rustup target add wasm32-unknown-unknown 2>/dev/null || true
# Guest is excluded from the root `icn/` workspace so it stays `#![no_std]` on wasm.
cargo build --manifest-path "$GUEST_MANIFEST" --target wasm32-unknown-unknown --release --locked
BUILT="$(dirname "$GUEST_MANIFEST")/target/wasm32-unknown-unknown/release/icn_baseline_lock_guest.wasm"
cp "$BUILT" "$OUT_WASM"
echo "Updated $OUT_WASM (from $BUILT)"
