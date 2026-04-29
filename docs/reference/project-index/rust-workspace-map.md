---
Status: operational
Canonical: no
Last Reviewed: 2026-04-29
---

# Rust Workspace Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md) and [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md). Authoritative crate boundaries are defined by [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md), not by this map.

The ICN Rust workspace lives in **`icn/`**, not at the repo root. The repo root is not a Cargo workspace. Run all Rust commands from `icn/`.

```bash
cd icn
cargo check
cargo build
cargo test --workspace --lib
```

Workspace size today: **35 library crates** in `icn/crates/`, **4 runtime app crates** in `icn/apps/`, **3 binaries** in `icn/bins/`. Toolchain is pinned in `icn/rust-toolchain.toml` — do not upgrade.

> Crate names below are orientation only. Authority over kernel/app boundaries is set by [`KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md). Domain crates must never be imported into kernel crates; the CI "Meaning Firewall Check" enforces this.

## Crates grouped by rough layer

### Kernel surface and runtime mechanics

| Crate | Role |
|---|---|
| `icn-kernel-api` | The kernel API surface. `PolicyOracle`, `ConstraintSet`, `BlockHeight`, `ErrCode`, invariant IDs. Apps program against this; kernel enforces it. |
| `icn-core` | Tokio runtime, supervisor, actor lifecycle, dispatcher. |
| `icn-encoding` | Canonical serialization utilities. Determinism foundation. |
| `icn-protocol` | Facade re-exporting `icn-gossip` + `icn-net`. |
| `icn-services` | Facade re-exporting `icn-api` + `icn-rpc` + `icn-gateway`. |
| `icn-commons` | Shared kernel-side utilities. |

### Identity and cryptography

| Crate | Role |
|---|---|
| `icn-identity` | DID generation (`did:icn:<base58-pubkey>`), Ed25519 keypairs, Age-encrypted keystore. |
| `icn-crypto` | Facade re-exporting `icn-crypto-pq`. |
| `icn-crypto-pq` | Hybrid post-quantum cryptography. |
| `icn-zkp` | Zero-knowledge proofs (used by SDIS). |
| `icn-authz` | Authorization primitives, capability tokens. |
| `icn-naming` | Cooperative naming service, DID resolution. |

### Networking and gossip

| Crate | Role |
|---|---|
| `icn-net` | QUIC/TLS sessions, mDNS discovery, NetworkActor, `SignedEnvelope` handling. |
| `icn-gossip` | Topic-based gossip with vector clocks, anti-entropy, Bloom filters. |
| `icn-time` | Clock synchronization (Rough Time Protocol). |

### Ledger and economics

| Crate | Role |
|---|---|
| `icn-ledger` | Double-entry mutual credit / state-change journal (Merkle-DAG). |
| `icn-snapshot` | State snapshots for graceful restarts. |

### Governance, entity model, federation

| Crate | Role |
|---|---|
| `icn-governance` | Governance primitives (proposals, decisions, action items, meetings, structures, activities). |
| `icn-ccl` | Cooperative Contract Language: AST, interpreter, fuel metering. Constitutional layer for entities. |
| `icn-entity` | Unified entity model across individuals, cooperatives, federations. |
| `icn-coop` | Cooperative management and lifecycle. |
| `icn-community` | Community structures and civic engine. |
| `icn-federation` | Inter-cooperative coordination protocol. |
| `icn-trust` | Trust graph storage and transitive trust computation → `TrustPolicyOracle`. |

### Compute, security, observability, storage

| Crate | Role |
|---|---|
| `icn-compute` | Distributed compute layer with trust-gated task execution. |
| `icn-security` | Byzantine fault detection, reputation. |
| `icn-privacy` | Privacy primitives, metadata protection. |
| `icn-steward` | SDIS steward network and VUI computation. |
| `icn-obs` | Prometheus metrics, tracing, logging. |
| `icn-store` | Persistent KV storage (Sled). |

### API surface

| Crate | Role |
|---|---|
| `icn-api` | Shared service layer (validation, error handling) for RPC and Gateway. |
| `icn-rpc` | gRPC API server. |
| `icn-gateway` | REST + WebSocket API for cooperative applications. **Binds port 8080.** |
| `icn-http-kit` | HTTP utilities shared across gateway/API layers. |

### Test infrastructure

| Crate | Role |
|---|---|
| `icn-testkit` | Test helpers for multi-node convergence scenarios, `TestNode` pattern. |

## App crates (`icn/apps/`)

Runtime-integrated apps. Apps implement `PolicyOracle` and own domain semantics; the kernel never imports app crates.

| App | Purpose |
|---|---|
| `apps/charter` | `CharterPolicyOracle` — translates ratified CCL charter documents into `ConstraintSet`. Wired into `icnd` startup; ratification flow is type-erased at boundary. |
| `apps/governance` | Governance app — proposals, decisions, action items, meetings, action-card runtime. |
| `apps/ledger` | Ledger / journal app: `PatronageTracker`, settlement engine. |
| `apps/membership` | Membership management. |

## Binaries (`icn/bins/`)

| Binary | Role |
|---|---|
| `icnd` | The ICN daemon. Loads keystore, spawns supervisor, runs actors. |
| `icnctl` | CLI management tool (`audit verify`, identity ops, charter validate/inspect/deploy, etc.). |
| `icn-console` | Interactive TUI for cooperative management. |

## Verification commands

```bash
cd icn

cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --lib
cargo test --workspace --test '*' -- --test-threads=1

# Per-crate
cargo test -p icn-gossip
cargo test -p icn-gateway --features sled-storage

# Specific test
cargo test test_two_node_convergence -- --exact
```

If a build SIGSEGVs or fails mysteriously, run `cargo clean` first — incremental compilation cache corruption is a known issue on this machine. See [`AGENTS.md`](../../../AGENTS.md) for the full change-routing matrix.

## Where to read deeper

| Topic | Doc |
|---|---|
| Kernel/app boundary doctrine | [`docs/architecture/KERNEL_APP_SEPARATION.md`](../../architecture/KERNEL_APP_SEPARATION.md) |
| Constitutional core (invariants) | [`docs/genesis.md`](../../genesis.md), [`docs/ai/ICN_CONSTITUTIONAL_CORE.md`](../../ai/ICN_CONSTITUTIONAL_CORE.md) |
| Architecture deep dive | [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) |
| Phase progress | [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md) |
