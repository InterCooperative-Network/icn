# icn-baseline-lock

Single-node **executable baseline-lock loop** fixture: signed receipt DAG, pure Rust projector, minimal WASM gate guest (wasmtime, zero imports), hostile output validation, test-equivalent gate receipt + `AllocationReceipt` bridge, evidence packet, and Action Card projection.

This is **not** production readiness, live networking, federation, or full governance runtime integration.

## WASM guest (`#![no_std]` + `alloc`)

`icn-baseline-lock-guest` is **excluded** from the root `icn/` workspace (see `icn/Cargo.toml` `exclude`) so `serde`/`postcard` are not unified with workspace `[workspace.dependencies]` entries that would re-enable `std` on `wasm32-unknown-unknown`. The guest has its own `Cargo.lock` at `crates/icn-baseline-lock-guest/Cargo.lock`; `icn-boundary` pins `serde`/`postcard` with explicit `default-features = false` for the same reason.

The wasmtime host still enforces no WASI, no host imports, fuel, and hostile output checks.

## Rebuild the WASM guest

Requires `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`).

From the monorepo root:

```bash
./scripts/build-baseline-lock-guest.sh
```

The committed artifact under `tests/fixtures/icn_baseline_lock_guest.wasm` must stay in sync with guest sources; integration tests `include_bytes!` that file.

**Drift check** (artifact must match a `--locked` rebuild from guest sources):

```bash
./scripts/build-baseline-lock-guest.sh
git diff --exit-code icn/crates/icn-baseline-lock/tests/fixtures/icn_baseline_lock_guest.wasm
```

A clean exit means the checked-in `.wasm` already matches the guest crate.

## Tests

```bash
cd icn
cargo test -p icn-baseline-lock
cargo test -p icn-baseline-lock test_baseline_lock_loop -- --exact
```

Guest-only lint (workspace `exclude`; use `--lib` because `std` pulls in the test harness on `--all-targets`):

```bash
cargo clippy --manifest-path icn/crates/icn-baseline-lock-guest/Cargo.toml \
  --target wasm32-unknown-unknown --lib -- -D warnings
```

## Related docs

- `docs/manual/FOUNDATIONAL_MANUAL_EXECUTABLE_BASELINE_LOOP.md`
- `docs/manual/FOUNDATIONAL_MANUAL_BOUNDARY_TYPES.md`
