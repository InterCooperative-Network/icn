# 01: Environment Setup & Repository Navigation

> **Phase**: 1 | **Tier**: Reader  
> **Patterns introduced**: None (foundation)  
> **Prerequisite**: None (entry point)

## Why This Matters

Before you can read or change ICN code, you need fluency in two domains: **where things are** (mental map) and **how to build them** (toolchain confidence). This layer transforms "I can clone a repo" into "I can navigate subsystems, trigger builds, and interpret failures."

The ICN codebase uses a kernel/app/domain split that reflects the Meaning Firewall architecture (covered in Phase 2). Understanding this split early prevents confusion about why certain crates can't import others.

→ See `manual.md` § "The ICN Substrate" for the full system narrative.

## What You'll Read

### 1. Entry Point: `icn/bins/icnd/src/main.rs`

This is where the daemon starts. Trace the flow:
- CLI argument parsing (`clap`)
- Config loading from `~/.icn/config.toml`
- Passphrase handling for keystore unlock
- Call to `icn_core::runtime::start_runtime()` — the handoff to the actor system

**Key question**: What happens between `main()` and the supervisor spawning actors?

### 2. Actor Orchestration: `icn/crates/icn-core/src/supervisor/mod.rs`

The supervisor is ICN's "conductor" — it spawns all subsystem actors in dependency order:
- Identity + Trust → Network → Gossip → Ledger → Governance → Compute
- Sets up callbacks between actors (e.g., Gossip → Network for sending messages)
- Registers shutdown signal handlers

**Key question**: How does the supervisor ensure actors start in the correct order?

### 3. Lifecycle Shell: `icn/crates/icn-core/src/runtime.rs`

The runtime provides the lifecycle wrapper:
- `start_runtime()` initializes the Tokio runtime
- `tokio::select!` waits for shutdown signal (SIGINT/SIGTERM) or error
- Graceful shutdown propagates via `broadcast::channel` to all actors

**Rust concepts inline**:
- `tokio::main` macro: expands to `fn main()` + runtime setup
- `tokio::select!`: concurrent await on multiple futures, first wins
- `broadcast::channel`: multi-producer, multi-consumer channel for shutdown coordination

### 4. The Kernel/App/Domain Split

**Kernel crates** (domain-agnostic, enforce constraints blindly):
- `icn-kernel-api` — PolicyOracle trait, ConstraintSet types
- `icn-core` — actor runtime, supervisor, shutdown coordination
- `icn-net` — QUIC transport, mDNS discovery
- `icn-gossip` — topic-based replication
- `icn-store` — KV storage abstraction
- `icn-obs` — metrics, tracing

**App crates** (implement PolicyOracle, translate meaning → constraints):
- `apps/trust` — trust graph → rate limits
- `apps/governance` — protocol parameters → quotas
- `apps/ledger` — credit state → transaction limits

**Domain crates** (internal data structures, never imported by kernel):
- `icn-trust` — trust graph storage
- `icn-ledger` — double-entry ledger
- `icn-governance` — proposal lifecycle
- `icn-ccl` — contract language AST

**The rule**: Kernel crates NEVER import domain crates. Apps bridge the two.

## What You'll Build

→ **Lab**: `labs/lab-01-workspace/`

Build a 2-crate workspace mimicking `icn-core` + `icnctl`:
- `core/` — library crate with actor-like struct
- `cli/` — binary that depends on `core/`
- Wire tracing, thiserror, serde — build/test pass

This teaches workspace organization, dependency management, and basic build flow.

## Checkpoint

You've completed this layer when you can:

1. **Build without friction**: `cd icn/ && cargo build` succeeds, you understand any warnings
2. **Test without friction**: `cargo test --workspace --lib` runs, you can isolate a single test
3. **Lint without friction**: `cargo clippy --workspace --all-targets` passes or you understand failures
4. **Draw the repo mental map**: Sketch kernel/app/domain boundaries on paper/whiteboard, label 5 crates in each category
5. **Trace a lifecycle**: Explain the sequence `main.rs` → `runtime.rs` → `supervisor.rs` → actors spawn

**Artifact**: Submit your mental map sketch (hand-drawn photo or digital diagram) showing the crate split.

## Deep Reference

→ `reference/module-00-setup.md` — Full toolchain installation, IDE setup, build troubleshooting  
→ `AGENTS.md` — Repo conventions, build commands, change routing  
→ `CONTRIBUTING.md` — PR workflow, commit format
