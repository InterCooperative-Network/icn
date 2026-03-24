# AGENTS.md

Instructions for agentic coding agents operating in this repo.

---

## Operating mode (must follow)

1. **Plan first.** Before edits, produce:
   - Goal + success criteria
   - Files/crates you will touch
   - Commands you will run to verify
2. **Keep diffs small and reviewable.** Prefer multiple PRs over one mega-PR.
3. **No "fixing" by weakening safety.**
   - Do not relax validation, trust gates, signature checks, or encoding rules to make tests pass.
4. **Run the right checks for the area you touched** (see "Change routing" below).
5. **Docs/specs must match reality.** If you change semantics, update the relevant doc/spec in the same PR,
   or create a blocking issue and reference it in the PR description.
6. **No new tooling.** Do not introduce new linters, build systems, or frameworks unless explicitly requested.

---

## ICN invariants (non-negotiable)

These are the protocol invariants that must never be violated:

| Invariant | Description |
|-----------|-------------|
| **Adversarial-by-default** | Treat peers as untrusted until trust is established. No implicit trust shortcuts. |
| **Determinism** | Protocol state transitions, proofs, and derived roots must be deterministic. Same inputs → same outputs. |
| **Canonical encodings** | Do not change wire/proof/encoding structures without explicit intent + docs + tests. |
| **No panics in protocol paths** | Never panic in network/protocol/actor runtime/deserialization paths. Use `Result<T, E>`. |
| **Kernel/app boundaries** | Keep crate layering clean; avoid dependency cycles; follow forbidden-deps policy. |

If a change might impact any invariant:
- Call it out explicitly in the plan
- Add tests proving the invariant still holds
- Update the relevant docs/specs

---

## Repo layout (critical)

- **Rust workspace is in `icn/`** (repo root is NOT a Cargo workspace).
- Non-Rust projects:
  - `sdk/typescript/` (TypeScript SDK)
  - `sdk/react-native/` (React Native SDK)
  - `web/pilot-ui/` (vanilla JS PWA)
  - `web/dashboard/` (static dashboard)

## App topology rule (Hard)

- Runtime-integrated app crates live under `icn/apps/`.
- Do not add new runtime-integrated crates under top-level `apps/`.
- If you touch a crate under `apps/`, you must either:
  - migrate it to `icn/apps/`, or
  - add a tracking issue that classifies it as an example/tool with a migration or removal date.

---

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

---

## Change routing (run the right verification)

| If you touch... | Run these checks |
|-----------------|------------------|
| **Rust crates** (`icn/crates/**`) | `cargo fmt --all --check`, `cargo clippy ...`, appropriate `cargo test` scope |
| **Gateway API** (`icn-gateway`) | `cargo test -p icn-gateway --features sled-storage`, regenerate OpenAPI + TS types if API changed |
| **TypeScript SDK** (`sdk/typescript/`) | `npm ci && npm run build && npm test && npm run lint` |
| **React Native SDK** (`sdk/react-native/`) | `npm test && npm run build` |
| **Pilot UI** (`web/pilot-ui/`) | `npm run test && npm run test:e2e && npm run test:a11y` |
| **Deploy manifests** (`deploy/`) | Ensure no secrets committed; keep placeholders; update deploy docs if behavior changes |
| **Documentation** (`docs/`) | Verify links, check terminology consistency |

---

## Code style and engineering conventions

### General

- Prefer small, reviewable changes; follow existing patterns.
- Do not commit secrets; CI checks deployment manifests for placeholder secrets.
- Do not add documentation files to repo root; docs belong under `docs/`.
- Do not introduce new tooling (linters/build systems) unless explicitly requested.

### Rust (`icn/`)

- **Formatting**: let `cargo fmt` handle formatting.
- **Imports**: prefer explicit imports; avoid glob imports except common test preludes; order as `std`, external crates, `crate`.
- **Naming**: `PascalCase` types/traits/enums; `snake_case` modules/functions/vars; `SCREAMING_SNAKE_CASE` constants/statics.
- **Errors**:
  - Use `Result<T, E>`; prefer `thiserror` for crate-local error enums.
  - Use `anyhow` at app/service boundaries; add context (`.context("...")`).
  - Avoid `unwrap()`/`expect()` in non-test code (clippy warns).
  - Never panic in protocol/network/actor runtime/deserialization paths.
- **Async/concurrency**:
  - Tokio runtime; no blocking I/O in async code (`tokio::fs` or `spawn_blocking`).
  - Prefer message passing (mpsc/oneshot) over shared mutable state.
  - If shared state is unavoidable: use `tokio::sync`; don't hold locks across `.await`.
- **Serialization/API**: use `serde`; for JSON structs prefer `#[serde(rename_all = "camelCase")]`.
- **Clippy**: workspace thresholds are tuned in `icn/clippy.toml`; prefer refactors over broad `#[allow]`.
- **Callback ownership model** — mandatory review heuristic:
  - `Option<Arc<dyn Fn(...)>>` (single-slot) is correct when there is **one owner**: one transport layer, one trust oracle, one ledger. Use `set_*_callback`.
  - `Vec<Arc<dyn Fn(...)>>` (fan-out) is required when **multiple independent subsystems** may each legitimately register on the same event surface. Use `add_*_callback`.
  - The bug class is not "single callback bad." It is **single callback + contested ownership = dangerous**. A second subsystem calling `set_*_callback` silently drops the first handler. No error, no warning — just a broken routing path.
  - **Audit question**: can two independently-reasonable subsystems each believe they own this surface? If yes, the surface needs fan-out semantics.
  - Known multi-subscriber surface: `GossipActor::notification_callbacks` — lifecycle dispatcher, governance, and steward all register independently. Use `add_notification_callback`, never `set_notification_callback` (deprecated since 0.1.0, see issue #1416).
  - Known single-owner surfaces (correct as-is): `send_callback` (network transport), `trust_callback` / `balance_callback` (service lookups), `event_callback` (gateway WebSocket broadcaster).

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

---

## Documentation organization

**All documentation goes in `docs/`** - never save docs to project root (except the core files listed in CLAUDE.md).

**Finding documentation:**
- Start with `docs/INDEX.md` for comprehensive navigation
- Use `docs/README.md` for quick overview
- Follow the category structure in `docs/`

**Where to put new docs:**
- Architecture/design decisions → `docs/architecture/` or `docs/design/`
- API documentation → `docs/reference/api/`
- User/developer guides → `docs/guides/user/` or `docs/guides/developer/`
- Security documentation → `docs/security/`
- Historical/completed work → `docs/archive/YYYY/`

**See `docs/DOCUMENTATION_MAINTENANCE.md` for complete guidelines.**

---

## Custom agents

This repo provides specialized Copilot agents in `.github/agents/`. The orchestrator (`icn-orchestrator`) auto-selects and routes to specialists.

See `.github/agents/README.md` for the full list and usage instructions.

---

## Multi-agent worktree workflow

When running multiple agents in parallel, each agent gets its own Git worktree with an isolated branch and working directory. See `docs/dev/WORKTREES.md` for full documentation.

**Quick reference:**

```bash
# Create an agent worktree
./scripts/worktrees.sh create agent-d

# List all worktrees
./scripts/worktrees.sh list

# Remove when done
./scripts/worktrees.sh remove agent-d
```

**Rules:**
- One agent = one branch = one worktree
- Never commit to `main` — all work on feature branches
- Worktrees live in `../icn-wt/` (sibling to repo root)
- Override defaults via `ICN_WT_DIR`, `ICN_WT_REMOTE`, `ICN_WT_BASE_REF`
- **Rebase before edits**: Any agent assigned to an older branch must first run `git fetch origin && git rebase origin/main` before making any edits. This prevents CRLF phantom diffs and merge conflicts from stale bases.

---

## Agent handoff protocol

When ending a session or passing work to another agent, write a handoff note using `/handoff`.

**What to capture:**

```markdown
# Session Handoff — YYYY-MM-DD

## Branch
`feat/<slug>` — base: main

## Commits this session
- <sha> <message>

## Open PRs
- #<N>: <title> (state)

## Open threads
- [ ] <unfinished work or decision needed>

## TODOs added this session
- `<file>:<line>` — <text>

## Next steps
1. <first action for next session>
```

**Rules:**
- Write to `docs/dev-journal/session-YYYY-MM-DD.md` (append if file exists today)
- Stash must be empty before ending — commit or drop stashes
- If pushing, use `/push` (runs fmt + clippy gates first)
- The handoff file is for context continuity — do not auto-commit it

**Resuming from a handoff:**
1. Read `docs/dev-journal/session-YYYY-MM-DD.md` (most recent date)
2. Run `/preflight` to verify environment
3. Run `git fetch origin && git rebase origin/main` if branch is stale
4. Check open threads from the handoff note before starting new work

---

## Repo-provided agent rules (must follow)

- **Copilot instructions**: `.github/copilot-instructions.md`
- **Path-specific rules**: `.github/instructions/` (`rust-core.md`, `sdk.md`, `web-ui.md`, `documentation.md`)
- **Custom agents**: `.github/agents/` (ICN-specific specialists)
