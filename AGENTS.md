# AGENTS.md

Instructions for agentic coding agents operating in this repo.

**Orient first:** read [`docs/ATLAS.md`](docs/ATLAS.md) — the whole-ecosystem map (what each repo
owns, where each fact's canonical source lives, and what to read first). It is an index; for any
conflict, the canonical owner it points to wins.

---

## Operating mode (must follow)

1. **Plan first.** Before edits, produce:
   - Goal + success criteria
   - Files/crates you will touch
   - Commands you will run to verify
2. **Keep diffs small and reviewable.** Prefer multiple PRs over one mega-PR.
3. **No "fixing" by weakening safety.**
   - Do not relax validation, trust gates, signature checks, or encoding rules to make tests pass.
4. **Verify your root, then read before edit.** Confirm `git rev-parse --show-toplevel` is the
   checkout you intend (on the dev VM: `~/icn-dev/worktrees/icn/<worktree>`; standalone clones such
   as `~/projects/icn` are legacy and can be weeks stale). Read a file from this checkout before
   editing it — never edit from memory or from another checkout's copy.
5. **Run the right checks for the area you touched** (see "Change routing" below).
6. **Docs/specs must match reality.** If you change semantics, update the relevant doc/spec in the same PR,
   or create a blocking issue and reference it in the PR description.
7. **No new tooling.** Do not introduce new linters, build systems, or frameworks unless explicitly requested.

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

For the full source-linked index of all four invariant families (operational, firewall-contract, frozen-core, regulatory), see [`docs/reference/project-index/invariants-catalog.md`](docs/reference/project-index/invariants-catalog.md). That catalog indexes canonical sources; it does not define new invariants.

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
- **Example-only crates must be clearly marked** (a `README.md` in the crate dir saying so),
  so they are not mistaken for an uncovered runtime crate.

**Current state (#2064 complete for runtime crates).** All runtime-integrated top-level app
crates have been migrated under `icn/apps/*` and are now workspace-covered (`cargo
test --workspace` / `cargo clippy --workspace`): `icn-ledger-app` (#2070), `icn-governance-app` (#2071),
`icn-trust-app` (#2072) — each kept distinct from its `icn/apps/*` `*-actor` sibling.
Top-level `apps/` now holds **only** `apps/echo` (`icn-app-echo`), which is **example-only**
(no runtime consumers) and intentionally left outside the workspace — see
[`apps/echo/README.md`](apps/echo/README.md). It is therefore not `--workspace` CI-covered;
if it ever gains a runtime consumer, migrate it like the three above.

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
| **Documentation** (`docs/`) | Run the doc-control commands below. If the change touches `docs/status.toml`, `docs/registry.toml`, or `docs/design-language/concept-map.md`, also run `just website-verify` — the public site projects all three |
| **Website** (`website/`) | `just website-verify` — build, type check, public-state projection, docs publication boundary, internal links, walkthrough fixture safety, claim linting, rendered layout/accessibility audit |

**Doc-control commands (exact forms — run from the monorepo root, not from `docs/`):**

```bash
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml
python3 docs/scripts/freshness-check.py --freshness docs/freshness.toml --status docs/status.toml --repo .
python3 .github/scripts/compliance_linter.py --repo-root .
python3 .github/scripts/readiness_overclaim_linter.py --repo-root .
```

### Website commands

`just website-verify` runs everything CI runs for a website change, in CI's
order, so a CI failure reproduces locally with one command. The individual
steps, when you want to iterate on one:

```bash
just website-install       # once, per checkout
just website-build         # build (runs the five state generators first)
just website-check         # types, public-state, docs boundary, links, fixtures
just website-claims        # readiness overclaim linter, scoped to website/
just website-audit         # rendered audit, 7 pages x 3 widths (needs Chrome)
just website-audit-full    # the deep matrix, 12 pages x 5 widths
```

**What each check protects, so a failure is legible:**

| Check | Invariant |
|-------|-----------|
| `check:state` | The public maturity page is a projection of `docs/status.toml`, not a second tracker. Catches unmapped status values, operator-only fields leaking into public output, a missing claim axis, a build timestamp posing as a verification date, and non-deterministic generation. |
| `check:docs` | The public documentation boundary is a security boundary. Catches withheld material (internal, partner, session logs) reaching the site, published material failing to build, archive pages without `noindex` or a banner, and current pages wrongly carrying `noindex`. Writes `public-docs-manifest.json`. |
| `check:links` | Every internal link resolves and every compatibility redirect still works. Stale fragments inside rendered markdown are reported, not blocking. |
| `check:fixtures` | `/see-it-work` stays fictional, labelled, deterministic, offline, and read-only. |
| `audit` | Rendered pages have no horizontal overflow, one `h1`, an unbroken heading outline, landmarks, no sub-12px text, and labelled images and SVGs. |

**Generated files are never edited.** `website/src/data/*.generated.json` are
projections; each names its source. Change the source and rebuild.

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

On the dev host, agent sessions follow the `~/icn-dev` bare-store/worktree operating model — see `docs/dev/AGENT_WORKTREE_POLICY.md` for worktree allocation, file locks, and the merge queue. The quick reference below covers the older repo-adjacent `../icn-wt/` workflow, not the `~/icn-dev` model.

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
- In this older/legacy layout, worktrees live in `../icn-wt/` (sibling to repo root); on the dev VM
  the canonical location is `~/icn-dev/worktrees/icn/<name>` per
  `ops/state/config/repo-map.json#worktrees.root`
- Override defaults via `ICN_WT_DIR`, `ICN_WT_REMOTE`, `ICN_WT_BASE_REF`
- **Rebase before edits**: Any agent assigned to an older branch must first run `git fetch origin && git rebase origin/main` before making any edits. This prevents CRLF phantom diffs and merge conflicts from stale bases.

---

## Agent handoff protocol

When ending a session or passing work to another agent, write a handoff note using `/handoff`.

**What to capture:**

Use `docs/dev/HANDOFF_TEMPLATE.md` as the canonical structure (it carries the truth-type labels and section ordering). At minimum each handoff records:

- **Current state** — branch, head SHA, base SHA, working-tree status.
- **Open PRs and issues** — which PRs are open, which issues are advanced, what's blocked.
- **Validation results** — which checks ran (and which did not), with their outputs.
- **Unresolved reviewer feedback** — open AI-reviewer threads or human comments, and their disposition (accepted, rejected with rebuttal, or deferred).
- **Unsafe assumptions** — anything this session relied on but did not verify.
- **Next recommended action** — the exact starting move for the next session.

**Rules:**
- Write to `docs/dev/handoff-YYYY-MM-DD-<topic>.md`. Use a descriptive topic suffix (e.g., `handoff-2026-05-15-compute-placement-policy.md`); if multiple handoffs land the same day under the same topic, follow `docs/dev/HANDOFF_TEMPLATE.md` §"Usage Notes" for the suffix convention.
- Do not invent `docs/dev-journal/` — that directory does not exist in this repository. The canonical location is `docs/dev/`.
- Stash must be empty before ending — commit or drop stashes
- If pushing, use `/push` (runs fmt + clippy gates first)
- The handoff file is for context continuity — do not auto-commit it

**Resuming from a handoff:**
1. Read the most recent `docs/dev/handoff-YYYY-MM-DD-<topic>.md`
2. Run `/preflight` to verify environment
3. Run `git fetch origin && git rebase origin/main` if branch is stale
4. Check open threads from the handoff note before starting new work

---

## Repo-provided agent rules (must follow)

- **Copilot instructions**: `.github/copilot-instructions.md`
- **Path-specific rules**: `.github/instructions/` (`rust-core.md`, `sdk.md`, `web-ui.md`, `documentation.md`)
- **Custom agents**: `.github/agents/` (ICN-specific specialists)

---

## Headless / CI / cloud-agent runtime gotchas

This section applies to **any** non-interactive environment: Cursor Cloud, Claude Code, Codex, Copilot agents, CI runners, Docker containers, etc.

### Environment setup

Run `./scripts/bootstrap.sh` from the repo root. It installs system packages (Debian/Ubuntu), ensures the pinned Rust toolchain, installs Node.js if missing, fetches Rust deps, and installs TypeScript SDK deps. It is idempotent.

Flags: `--ci` skips optional cargo dev tools (faster). `--no-sysdeps` skips system package installation (use on non-Debian systems or without root).

After bootstrap, verify with: `cd icn && cargo build && cargo test --workspace --lib`

### Running the ICN daemon without a TTY

Identity init and daemon start both prompt for a passphrase interactively. Set `ICN_PASSPHRASE` to bypass:

```bash
cd icn
ICN_PASSPHRASE=dev ./target/debug/icnctl --data-dir /tmp/icn id init
ICN_PASSPHRASE=dev ICN_GATEWAY_JWT_SECRET=dev-secret-must-be-at-least-32-bytes \
  ./target/debug/icnd --data-dir /tmp/icn --gateway-enable
```

**Gotchas:**
- Gateway is **off by default**. Pass `--gateway-enable` to bind port 8080.
- Gateway requires `ICN_GATEWAY_JWT_SECRET` (minimum 32 bytes for HS256). For a quick local smoke run you can instead pass `--insecure-gateway-no-jwt`: this is a **local-dev-only escape hatch**, NOT a general no-auth mode. It only activates when the gateway is enabled and no JWT secret is configured, and it **fails closed** — the daemon refuses to start unless the gateway binds to a loopback IP literal (`--gateway-bind 127.0.0.1:8080` or `[::1]:8080`). The bind address is parsed as a `SocketAddr`, so hostnames such as `localhost` are not accepted and will be rejected. On a valid loopback bind it logs a loud warning and starts with a well-known insecure dev secret so the challenge/verify flow still works (anyone can mint tokens against it — never use it on a reachable interface). To bind a non-loopback gateway, configure a real `ICN_GATEWAY_JWT_SECRET` instead.
- Metrics always bind port 9100. Health: `GET http://localhost:8080/v1/health` (no auth).
- The daemon also accepts `ICN_KEYSTORE_PASSPHRASE` (checked before `ICN_PASSPHRASE`).

### Obtaining a JWT token

`icnctl auth token` defaults to `governance:read`/`governance:write` (full scope names the gateway accepts). For broader access request the full set:

```bash
ICN_PASSPHRASE=dev ./target/debug/icnctl --data-dir /tmp/icn auth token \
  --coop-id test-coop \
  --scopes "ledger:read,ledger:write,coop:read,governance:read,governance:write"
```

Then: `curl -H "Authorization: Bearer <token>" http://localhost:8080/v1/...`
