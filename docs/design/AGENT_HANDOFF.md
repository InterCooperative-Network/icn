# Agent Handoff: Deterministic Compute Sprint

**Branch**: `feature/deterministic-compute-core`  
**Created**: 2026-02-11  
**Purpose**: Enable any AI agent to continue this work

---

## Quick Start

```bash
# Verify you're on the right branch
git branch --show-current  # Should show: feature/deterministic-compute-core

# Read the master plan first
cat docs/design/DETERMINISTIC_COMPUTE_SPRINT.md

# Then read the specifications
cat docs/design/compute-classes.md
cat docs/design/scope-scheduling.md
cat docs/design/deterministic-core.md
```

---

## What This Branch Is About

We're implementing **deterministic WASM execution** for ICN's governance and ledger paths. Without this, nodes running the same code with the same inputs can produce different outputs, causing consensus forks.

**The Problem**: Standard WASM is not deterministic:
- NaN bit patterns vary by CPU
- HashMap iteration order is random
- OOM behavior depends on host OS
- SIMD optimizations vary by CPU generation

**The Solution**: ICN Deterministic Core (ICN-DC) - a hardened Wasmtime configuration that guarantees bit-exact reproducibility across x86_64 and aarch64.

---

## Current State

### Completed ✅

- [x] Created branch `feature/deterministic-compute-core`
- [x] Wrote `docs/design/DETERMINISTIC_COMPUTE_SPRINT.md` (master plan)
- [x] Wrote `docs/design/compute-classes.md` (LC vs UC spec)
- [x] Wrote `docs/design/scope-scheduling.md` (capacity slices)
- [x] Wrote `docs/design/deterministic-core.md` (ICN-DC profile)

### Ready to Implement 🚀

| Task ID | Description | Priority |
|---------|-------------|----------|
| `det-engine` | Create `deterministic_engine.rs` | P0 |
| `trust-caps` | Add MAX_LOCAL_NODES/EDGES | P1 |
| `trust-bench` | Add trust graph benchmarks | P1 |

### Blocked (has dependencies)

| Task ID | Blocked By |
|---------|------------|
| `det-imports` | `det-engine` |
| `det-ci-ban` | `det-engine` |
| `nasties-tests` | `det-engine` |

---

## Key Files to Understand

### Current WASM Executor (THE PROBLEM)

**File**: `icn/crates/icn-compute/src/wasm_executor.rs`

```rust
// Line 59 - THIS IS WHAT WE'RE FIXING
let engine = Engine::default();  // No determinism config!
```

**Issues**:
- No NaN canonicalization
- No fuel metering at Wasmtime level
- No parallel compilation control
- Timestamp host function uses SystemTime::now() (non-deterministic)

### Wasmtime Version

**File**: `icn/crates/icn-compute/Cargo.toml`

```toml
# Line 52 - already pinned for security
wasmtime = { version = "24.0.5", optional = true }
```

### Governance Path (THE LC BOUNDARY)

**File**: `icn/crates/icn-governance-actor/src/handlers/execution.rs`

The function `translate_payload_to_effects()` is the LC boundary - this is where proposals become kernel effects that mutate state.

---

## Implementation Order

### Phase 1: Deterministic Engine (Start Here)

1. **Create**: `icn/crates/icn-compute/src/deterministic_engine.rs`

```rust
use wasmtime::{Config, Engine};

pub const WASMTIME_VERSION_PIN: &str = "24.0.5";

pub fn create_deterministic_engine() -> anyhow::Result<(Engine, DeterminismManifest)> {
    let mut config = Config::new();
    config.cranelift_nan_canonicalization(true);
    config.consume_fuel(true);
    config.parallel_compilation(false);
    // ... see docs/design/deterministic-core.md for full config
}
```

2. **Modify**: `icn/crates/icn-compute/src/wasm_executor.rs`
   - Replace `Engine::default()` with `create_deterministic_engine()`
   - Add `DeterminismManifest` to `WasmExecutor` struct

3. **Test**: `cargo test -p icn-compute`

### Phase 2: Nasties Test Suite

Create tests that verify determinism:
- `nan_payload_miner` - NaN bit patterns
- `mem_grow_boundary` - OOM behavior
- `fuel_exhaustion` - Fuel metering

### Phase 3: WASM Subset Validator

Create `subset.rs` that validates modules at load time:
- Forbid float opcodes (LC v1)
- Forbid SIMD
- Limit control flow depth
- Restrict memory.copy size

---

## Critical Rules

From `AGENTS.md`:

1. **Never weaken safety to fix tests** - if tests fail, the code is wrong
2. **Run verification** after changes:
   ```bash
   cd icn && cargo build && cargo test -p icn-compute
   ```
3. **Commit atomically** - one logical change per commit
4. **Follow conventional commits**: `feat(compute): add deterministic engine`

---

## Verification Commands

```bash
cd icn

# Build
cargo build -p icn-compute

# Test
cargo test -p icn-compute

# Lint
cargo clippy -p icn-compute -- -D warnings

# Specific test
cargo test -p icn-compute test_deterministic_engine
```

---

## Questions That Might Come Up

### Should we use Wasmtime 29.0.0 (from the spec)?

**No.** Current version is 24.0.5, pinned for security reasons (see Cargo.toml comment). Upgrading is a separate effort.

### Should floats be allowed in LC?

**No (for v1).** Integer-only LC massively reduces fork surface. Can enable floats in v2 with proven NaN canonicalization.

### What about SIMD?

**Forbidden in LC v1.** Either `relaxed_simd_deterministic(true)` or reject in subset. Since config knob availability varies by Wasmtime version, safer to reject.

### What about the credential scheme (Coconut)?

**Keep current approach.** ICN uses Ed25519 + STARK + Merkle accumulators which are post-quantum safe. Coconut/BLS12-381 is not.

---

## Contact / Escalation

If stuck, check:
1. `docs/design/DETERMINISTIC_COMPUTE_SPRINT.md` - comprehensive plan
2. `AGENTS.md` - operating rules
3. `CLAUDE.md` - project context

The design docs contain all the context needed to implement.
