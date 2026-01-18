# AGENTS.md

Instructions for agentic coding agents operating in this repo.

## Repo layout (critical)

- Repo root: `/home/matt/projects/icn`.
- Rust workspace is in `icn/` (repo root is NOT a Cargo workspace).
- Non-Rust projects:
  - `sdk/typescript/` (TypeScript SDK)
  - `sdk/react-native/` (React Native SDK)
  - `web/pilot-ui/` (vanilla JS PWA)
  - `web/dashboard/` (static dashboard)

## Build / lint / test

### Rust (run from `icn/`)

```bash
cd icn

cargo build
cargo build --release

cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# CI runs unit tests in parallel and integration tests serially
cargo test --workspace --lib
cargo test --workspace --test '*' -- --test-threads=1

# Quick local default
cargo test
```

Run a single Rust test:

```bash
cd icn

# By substring
cargo test test_two_node_convergence

# Exact name
cargo test test_two_node_convergence -- --exact

# In one crate
cargo test -p icn-gossip test_two_node_convergence

# Show stdout/stderr
cargo test -p icn-core test_two_node_convergence -- --nocapture
```

CI note: also runs `cargo test -p icn-gateway --features sled-storage`.

### OpenAPI + generated TS types drift (CI)

If gateway API changes, regenerate and commit the spec/types.

```bash
cd icn
cargo build -p icnctl
./target/debug/icnctl api export-openapi -o ../docs/api/openapi.generated.yaml

cd ../sdk/typescript
npm ci
npm run generate-types
npm run check-types
```

### TypeScript SDK (`sdk/typescript/`)

```bash
cd sdk/typescript
npm ci
npm run build
npm test
npm run lint

# Single test
npm test -- src/foo/bar.test.ts
npm test -- -t "parses gateway error"
```

### React Native SDK (`sdk/react-native/`)

```bash
cd sdk/react-native
npm test
npm run build

# Single test
npm test -- -t "derives keypair"
```

### Pilot UI (`web/pilot-ui/`)

```bash
cd web/pilot-ui
npm ci
npm run test
npm run test:e2e
npm run test:a11y

# Single Playwright spec
npx playwright test tests/e2e/accessibility.spec.js
```

### Dashboard (`web/dashboard/`)

```bash
cd web/dashboard
npm run dev  # python3 -m http.server 8080
```

## Code style and engineering conventions

### General

- Prefer small, reviewable changes; follow existing patterns.
- Do not commit secrets; CI checks deployment manifests for placeholder secrets.
- Do not add documentation files to repo root; docs belong under `docs/`.

### Rust (`icn/`)

- Formatting: let `cargo fmt` handle formatting.
- Imports: prefer explicit imports; avoid glob imports except common test preludes; order as `std`, external crates, `crate`.
- Naming: `PascalCase` types/traits/enums; `snake_case` modules/functions/vars; `SCREAMING_SNAKE_CASE` constants/statics.
- Errors:
  - Use `Result<T, E>`; prefer `thiserror` for crate-local error enums.
  - Use `anyhow` at app/service boundaries; add context (`.context("...")`).
  - Avoid `unwrap()`/`expect()` in non-test code (clippy warns).
  - Never panic in protocol/network/actor runtime/deserialization paths.
- Async/concurrency:
  - Tokio runtime; no blocking I/O in async code (`tokio::fs` or `spawn_blocking`).
  - Prefer message passing (mpsc/oneshot) over shared mutable state.
  - If shared state is unavoidable: use `tokio::sync`; don’t hold locks across `.await`.
- Serialization/API: use `serde`; for JSON structs prefer `#[serde(rename_all = "camelCase")]`.
- Clippy: workspace thresholds are tuned in `icn/clippy.toml`; prefer refactors over broad `#[allow]`.

### TypeScript (SDKs)

- Strict TS (`strict: true`); avoid `any` (use `unknown` + narrowing).
- Prefer `interface` for object shapes; export public types from package entrypoints.
- Naming: `PascalCase` types/classes, `camelCase` values/functions.
- Errors: throw typed errors with stable `code` values for boundary-crossing failures.

### Web UI (`web/`)

- Vanilla JS + HTML + CSS (no framework assumptions).
- Prefer `const` and `async/await`; avoid `var`.
- Handle errors with user-friendly messages; log technical details to console.
- Use semantic HTML and accessible patterns.

## Repo-provided agent rules (must follow)

- Copilot instructions: `.github/copilot-instructions.md`
- Path-specific rules: `.github/instructions/` (`rust-core.md`, `sdk.md`, `web-ui.md`, `documentation.md`)
- Cursor rules: none found in `.cursor/rules/` or `.cursorrules`
