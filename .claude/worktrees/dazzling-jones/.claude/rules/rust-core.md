---
paths:
  - "icn/crates/**/*.rs"
  - "icn/bins/**/*.rs"
---

# Rust Core Rules

## Workspace

- Rust workspace is in `icn/` (repo root is NOT a Cargo workspace)
- All cargo commands run from `icn/` directory
- Rust edition 2021, toolchain version from `icn/rust-toolchain.toml`

## Error Handling

- Use `Result<T, E>` everywhere; prefer `thiserror` for crate-local error enums
- Use `anyhow` at service/binary boundaries for context chaining
- **Never** `unwrap()` or `expect()` in non-test code (especially protocol/network/actor/deserialization paths)
- Use `ErrCode` enum from `icn-kernel-api` for protocol-level rejections

## Async & Concurrency

- Tokio runtime; no blocking I/O in async code (use `tokio::fs` or `spawn_blocking`)
- Prefer message passing (mpsc/oneshot) over shared mutable state
- If shared state unavoidable: use `tokio::sync`; don't hold locks across `.await`
- `PolicyOracle::evaluate()` is sync - uses `parking_lot::RwLock`, not `tokio::sync::RwLock`

## Actor Pattern

- Actors have: internal message enum, public handle with `mpsc::Sender`, spawn method
- Actor handles are `Clone + Send`
- Actor state is isolated (no shared references)
- Use `oneshot::channel` for request-response patterns

## Naming

- `PascalCase` types/traits/enums
- `snake_case` modules/functions/variables
- `SCREAMING_SNAKE_CASE` constants/statics

## Serialization

- `serde` for all serialization
- `bincode` for internal wire format
- `serde_json` for API boundaries
- JSON structs: `#[serde(rename_all = "camelCase")]`

## Testing

- Unit tests inline with `#[cfg(test)]`
- Integration tests in `crates/*/tests/`
- Use `AllowAllOracle`/`DenyAllOracle` for kernel tests (never mock trust graphs directly)
- Each test node gets unique port and keypair

## Verification (before commit)

```bash
cd icn
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test -p <touched-crate>
```
