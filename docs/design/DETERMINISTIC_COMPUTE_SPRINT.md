# ICN Deterministic Compute Sprint

**Branch**: `feature/deterministic-compute-core`
**Status**: Planning Complete → Implementation Ready
**Date**: 2026-02-11
**Origin**: Research Sprint #3 (Security & Determinism Spec v1.1)

---

## Executive Summary

This document specifies the implementation of the **ICN Deterministic Core (ICN-DC)** - a hardened WASM execution environment that ensures bit-exact reproducibility across x86_64 and aarch64 architectures. This is required for the Legitimacy Compute (LC) path where governance decisions and ledger mutations must be verifiable by any node.

**Key Insight**: Ethereum replicates execution to get truth. ICN can verify truth without replicating execution.

---

## Table of Contents

1. [Context & Problem Statement](#context--problem-statement)
2. [Architecture Decision](#architecture-decision)
3. [Current State Analysis](#current-state-analysis)
4. [Implementation Plan](#implementation-plan)
5. [Integration Points](#integration-points)
6. [Test Strategy](#test-strategy)
7. [Risks & Mitigations](#risks--mitigations)
8. [File Map](#file-map)
9. [Agent Handoff Checklist](#agent-handoff-checklist)

---

## Context & Problem Statement

### The Problem

Standard WebAssembly (WASM) execution is **not deterministic**:

1. **NaN payloads**: Different CPUs produce different NaN bit patterns for `0.0/0.0`
2. **Relaxed SIMD**: Hardware-specific optimizations vary by CPU generation
3. **Resource exhaustion**: OOM behavior depends on host OS memory pressure
4. **Iteration order**: HashMap/HashSet iterate non-deterministically

When two nodes execute the same WASM contract with the same inputs but get different outputs, they fork into incompatible states. This is a **subjectivity attack vector**.

### Why It Matters

ICN's governance model requires:
- `DecisionReceipt` computed identically on all nodes
- Effects produced deterministically from receipts
- Any node can verify any decision by replay

Without deterministic execution, honest nodes can be tricked into disagreement.

### The Solution

Split compute into two classes:

| Class | Purpose | Determinism | Examples |
|-------|---------|-------------|----------|
| **LC (Legitimacy Compute)** | State-changing operations | Bit-exact required | Governance execution, treasury effects, settlements |
| **UC (Utility Compute)** | Services, queries, analytics | Sandboxed only | APIs, indexing, batch jobs |

**Rule**: UC can propose → LC (in-scope) verifies and records.

---

## Architecture Decision

### North Star Invariants

1. **LC is the legitimacy path.** If it can change authoritative shared state, it must be bit-exact reproducible.
2. **UC is the service substrate.** Sandboxed + metered, but not consensus-replicated.
3. **Commons is never authoritative.** Commons proposes; higher scopes verify.
4. **Scopes are resource contracts.** Priority is real only where capacity is pledged.

### The LC Boundary

LC covers the path:

```
DecisionReceipt → Effect → LedgerEntry / Settlement / Constraint enforcement
```

Specifically:
- Governance execution translator (`translate_payload_to_effects`)
- Effect executor paths (treasury spend, membership changes)
- Receipt hashing + canonical serialization
- Any WASM/CCL execution that can produce effects

**Everything else defaults to UC** unless explicitly promoted.

### Scope Hierarchy

```
Commons < Local < Cell < Org < Federation
```

| Scope | Priority | Preemptible | Authoritative |
|-------|----------|-------------|---------------|
| Commons | Lowest | Always | Never |
| Local | Above Commons | By choice | Local only |
| Cell | Above Local | By pledge | Within cell |
| Org | Above Cell | By pledge | Within org |
| Federation | Highest | By pledge | Cross-org |

---

## Current State Analysis

### WASM Executor (icn-compute/src/wasm_executor.rs)

**Current Implementation**:
```rust
// Line 59 - NO DETERMINISM CONFIG
let engine = Engine::default();
```

**Issues**:
- ❌ No NaN canonicalization
- ❌ No relaxed SIMD control
- ❌ No fuel metering at Wasmtime level (uses flat estimate)
- ❌ No parallel compilation control
- ⚠️ Memory limit 64MB (should be configurable)
- ✅ Wasmtime version pinned to 24.0.5 (Cargo.toml line 52)

**Host Functions Exposed** (lines 228-261):
- `icn::log(ptr, len)` - Log message
- `icn::timestamp() -> i64` - **Non-deterministic** (uses SystemTime::now)

### Trust Graph (icn-trust/src/)

**Current Implementation**:
- 2-hop bounded algorithm (already conservative)
- No explicit MAX_LOCAL_NODES or MAX_LOCAL_EDGES caps
- HashMap used in anomaly.rs (analytics, not consensus)
- Store-based persistence with prefix scans

**Gap**: No hard limits on graph size → adversarial topology can cause unbounded computation.

### Governance Path (icn-governance/)

**Current Implementation**:
- `GovernanceDecisionReceipt` with deterministic `decision_hash`
- `translate_payload_to_effects()` converts proposals to kernel effects
- `EffectDispatcher` executes effects with `decision_receipt_id` linkage
- `DeterminismClass::Canonical` enforced in CCL interpreter

**Gap**: Determinism class not yet enforced in WASM executor.

---

## Implementation Plan

### Phase 0: Documentation (Ready Now)

Create three specification documents:

1. **`docs/design/compute-classes.md`**
   - LC vs UC formal definitions
   - Promotion rules (how UC output becomes LC input)
   - Scope × Class matrix

2. **`docs/design/scope-scheduling.md`**
   - Capacity slices model
   - Preemption rules per scope
   - Federation pledges

3. **`docs/design/deterministic-core.md`**
   - ICN-DC profile specification
   - Wasmtime configuration requirements
   - WASM subset restrictions

### Phase 1: Deterministic Engine (High Priority)

**Create**: `icn-compute/src/deterministic_engine.rs`

```rust
use wasmtime::{Config, Engine};

/// Pinned Wasmtime version for determinism. Upgrading is a hard fork.
pub const WASMTIME_VERSION_PIN: &str = "24.0.5";

#[derive(Debug, Clone, Copy)]
pub enum SimdPolicy {
    Deterministic,  // relaxed_simd_deterministic = true
    Forbidden,      // SIMD opcodes rejected in subset
}

#[derive(Debug, Clone, Copy)]
pub enum FloatPolicy {
    Allowed,        // With NaN canonicalization
    Forbidden,      // Integer-only LC (recommended for v1)
}

pub struct DeterminismManifest {
    pub wasmtime_version: &'static str,
    pub nan_canonicalization: bool,
    pub simd_policy: SimdPolicy,
    pub float_policy: FloatPolicy,
    pub fuel_enabled: bool,
    pub parallel_compilation: bool,
    pub max_memory_bytes: u64,
}

/// Creates a deterministic Wasmtime engine for Legitimacy Compute.
/// This is the ONLY way to create an engine for LC execution paths.
pub fn create_deterministic_engine() -> anyhow::Result<(Engine, DeterminismManifest)> {
    let mut config = Config::new();
    
    // NaN Canonicalization - forces all NaNs to 0x7fc00000
    config.cranelift_nan_canonicalization(true);
    
    // Fuel consumption - instruction counting, not wall-clock
    config.consume_fuel(true);
    
    // Disable parallel compilation for deterministic JIT
    config.parallel_compilation(false);
    
    // Memory configuration (Pi-friendly)
    let max_memory_bytes = 128 * 1024 * 1024; // 128 MiB for LC v1
    config.static_memory_maximum_size(max_memory_bytes);
    config.static_memory_guard_size(2 * 1024 * 1024);
    config.dynamic_memory_reserved_for_growth(max_memory_bytes);
    
    let manifest = DeterminismManifest {
        wasmtime_version: WASMTIME_VERSION_PIN,
        nan_canonicalization: true,
        simd_policy: SimdPolicy::Forbidden, // v1: reject SIMD in subset
        float_policy: FloatPolicy::Forbidden, // v1: integer-only
        fuel_enabled: true,
        parallel_compilation: false,
        max_memory_bytes,
    };
    
    tracing::info!(
        wasmtime_version = %manifest.wasmtime_version,
        nan_canon = %manifest.nan_canonicalization,
        simd = ?manifest.simd_policy,
        floats = ?manifest.float_policy,
        max_mem = %manifest.max_memory_bytes,
        "Deterministic engine initialized"
    );
    
    Ok((Engine::new(&config)?, manifest))
}
```

**Modify**: `icn-compute/src/wasm_executor.rs`

```rust
// Change line 59 from:
let engine = Engine::default();

// To:
use crate::deterministic_engine::{create_deterministic_engine, DeterminismManifest};

// In WasmExecutor struct, add:
manifest: DeterminismManifest,

// In WasmExecutor::new():
let (engine, manifest) = create_deterministic_engine()?;
```

**Add CI Guard** (`.github/workflows/ci.yml`):

```yaml
- name: Ban Engine::default in LC paths
  run: |
    if rg -q "Engine::default\(\)" icn/crates/icn-compute icn/crates/icn-core icn/crates/icn-governance; then
      echo "ERROR: Engine::default() found in consensus-relevant code"
      exit 1
    fi
```

### Phase 2: Nondeterministic Nasties Test Suite

**Create**: `icn-compute/tests/wasm_determinism_nasties.rs`

Test cases:

| Test | Vulnerability | Success Condition |
|------|---------------|-------------------|
| `nan_payload_miner` | NaN bit patterns | `i32.reinterpret_f32(0.0/0.0) == 0x7fc00000` on both archs |
| `simd_fma_diff` | FMA rounding | Bit-exact vector results |
| `mem_grow_boundary` | OOM consensus | `memory.grow` fails at exactly protocol limit |
| `hashmap_chaos` | Iteration order | Output hash constant across 1000 runs |

**Create**: `icn-compute/tests/fixtures/nasties/`
- `nan_payload.wat`
- `simd_fma.wat`
- `mem_grow.wat`
- `hashmap_iter.wat`

**Create**: Cross-arch CI workflow (`.github/workflows/determinism.yml`):

```yaml
name: Determinism Cross-Arch
on: [push, pull_request]

jobs:
  determinism-x86:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cd icn && cargo test -p icn-compute --test wasm_determinism_nasties
      - uses: actions/upload-artifact@v4
        with:
          name: determinism-hashes-x86
          path: icn/target/determinism-hashes.json
  
  determinism-aarch64:
    runs-on: [self-hosted, aarch64]
    steps:
      - uses: actions/checkout@v4
      - run: cd icn && cargo test -p icn-compute --test wasm_determinism_nasties
      - uses: actions/upload-artifact@v4
        with:
          name: determinism-hashes-arm
          path: icn/target/determinism-hashes.json
  
  compare-determinism:
    needs: [determinism-x86, determinism-aarch64]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
      - run: diff determinism-hashes-x86/determinism-hashes.json determinism-hashes-arm/determinism-hashes.json
```

### Phase 3: WASM Subset Validator

**Create**: `icn-compute/src/subset.rs`

```rust
pub struct WasmSubsetValidator {
    max_control_depth: u32,        // 100
    max_memory_copy: usize,        // 1MB
    allowed_imports: HashSet<&'static str>,
    forbid_floats: bool,           // true for v1
    forbid_simd: bool,             // true unless deterministic mode
}

pub enum SubsetViolation {
    ForbiddenImport(String),
    ForbiddenOpcode { opcode: String, reason: &'static str },
    ControlFlowTooDeep(u32),
    MemoryCopyTooLarge(usize),
}

impl WasmSubsetValidator {
    pub fn validate(&self, module_bytes: &[u8]) -> Result<(), Vec<SubsetViolation>>;
}
```

**WASM Subset Rules for LC v1**:

| Category | Allowed | Forbidden |
|----------|---------|-----------|
| Imports | `icn_log`, `icn_timestamp` (mocked) | `clock`, `random`, `fs`, `net` |
| Opcodes | Integer ops, memory, control flow | All f32/f64 ops, SIMD |
| Control | Depth ≤ 100 | Unbounded recursion |
| Memory | `memory.grow` under limit | `memory.copy` > 1MB |

### Phase 4: Trust Graph Hard Caps

**Modify**: `icn-trust/src/graph.rs`

```rust
pub const MAX_LOCAL_NODES: usize = 10_000;
pub const MAX_LOCAL_EDGES: usize = 50_000;

impl TrustGraph {
    pub fn add_edge(&mut self, source: &Did, target: &Did, weight: f64) -> Result<(), TrustError> {
        if self.edge_count() >= MAX_LOCAL_EDGES {
            return Err(TrustError::CapacityExceeded("edge limit"));
        }
        // ... existing logic
    }
}
```

### Phase 5: Service Hosting MVP (UC)

Service manifests + scope-aware scheduling for cooperative cloud. Not blocking determinism work.

### Phase 6: PoM-Lite + Dispute Hooks

**Create**: `icn-compute/src/dispute/pom.rs`

```rust
pub struct ProofOfMisbehavior {
    pub claim_type: ClaimType,
    pub canonical_inputs: CanonicalInputs,
    pub expected_output_hash: [u8; 32],
    pub actual_output_hash: [u8; 32],
    pub witness_bundle: WitnessBundle,
    pub signatures: Vec<Signature>,
}

pub struct WitnessBundle {
    pub wasm_hash: [u8; 32],
    pub input_data: Vec<u8>,
    pub fuel_limit: u64,
}
```

### Phase 7: BoLD/OSP (Future)

Full bisection game + reference interpreter. Only when federation becomes adversarial.

---

## Integration Points

### Where to Hook Deterministic Engine

1. **`WasmExecutor::new()`** (wasm_executor.rs:57)
   - Replace `Engine::default()` with `create_deterministic_engine()`

2. **`LocalExecutor::execute_task()`** (executor.rs:176)
   - Check `task.determinism_class` before execution
   - Use deterministic engine for `Canonical` tasks

3. **`add_host_functions()`** (wasm_executor.rs:228)
   - Mock `icn::timestamp()` for deterministic execution
   - Return `task.created_at` instead of `SystemTime::now()`

4. **Module loading** (wasm_executor.rs:122)
   - Validate module against subset before compilation
   - Reject non-compliant modules at load time

### Where to Add Trust Caps

1. **`TrustGraph::add_edge()`** (graph.rs)
   - Check edge count before insert

2. **`get_all_known_dids()`** (lib.rs:810)
   - Cap during collection

3. **`compute_betweenness_stats()`** (anomaly.rs:698)
   - Already has BETWEENNESS_HARD_LIMIT pattern to follow

---

## Test Strategy

### Unit Tests

| Test | Location | Verifies |
|------|----------|----------|
| `deterministic_engine_manifest` | deterministic_engine.rs | Config flags set correctly |
| `nan_canonicalization` | wasm_executor.rs | NaN → 0x7fc00000 |
| `fuel_consumption` | wasm_executor.rs | Fuel tracked per instruction |
| `subset_rejection` | subset.rs | Invalid modules rejected |
| `trust_cap_enforcement` | graph.rs | Edges rejected at limit |

### Integration Tests

| Test | Location | Verifies |
|------|----------|----------|
| `canonical_execution_replay` | tests/integration/ | Same inputs → same output hash |
| `cross_node_receipt_match` | tests/integration/ | DecisionReceipt identical across nodes |
| `pom_verification` | tests/integration/ | PoM artifact validates misbehavior |

### Cross-Arch CI

- Self-hosted aarch64 runner (preferred)
- QEMU fallback (acceptable but slower)
- Hash comparison gate blocks merge on mismatch

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Wasmtime config knobs vary by version | Policy: "deterministic or forbidden" — if knob unavailable, reject in subset |
| Cross-arch CI hard to set up | Start with manual testing; self-hosted runner later |
| Memory over-allocation | Start with 128 MiB cap (Pi-friendly); make configurable |
| HashMap in state paths | Audit all HashMap usage; replace with BTreeMap where needed |
| Toolchain drift (rustc versions) | Distribute WASM bytecode, not source |
| Witness bloat | Restrict `memory.copy` length in subset |

---

## File Map

### Files to Create

```
icn/crates/icn-compute/src/
├── deterministic_engine.rs    # NEW - deterministic engine wrapper
├── subset.rs                  # NEW - WASM subset validator
└── dispute/
    ├── mod.rs                 # NEW - dispute module
    ├── pom.rs                 # NEW - Proof of Misbehavior
    └── witness.rs             # NEW - witness bundle

icn/crates/icn-compute/tests/
├── wasm_determinism_nasties.rs  # NEW - nasties test suite
└── fixtures/nasties/
    ├── nan_payload.wat        # NEW
    ├── simd_fma.wat           # NEW
    ├── mem_grow.wat           # NEW
    └── hashmap_iter.wat       # NEW

docs/design/
├── compute-classes.md         # NEW - LC/UC specification
├── scope-scheduling.md        # NEW - capacity slices
└── deterministic-core.md      # NEW - ICN-DC profile

.github/workflows/
└── determinism.yml            # NEW - cross-arch CI
```

### Files to Modify

```
icn/crates/icn-compute/src/
├── wasm_executor.rs           # Use deterministic engine
├── executor.rs                # Check determinism class
└── lib.rs                     # Export new modules

icn/crates/icn-trust/src/
├── graph.rs                   # Add caps
└── types.rs                   # Add constants

.github/workflows/
└── ci.yml                     # Add Engine::default ban
```

---

## Agent Handoff Checklist

For any AI agent (Copilot, Claude, Codex, Gemini) picking up this work:

### Before Starting

- [ ] Read this document completely
- [ ] Read `AGENTS.md` for operating rules
- [ ] Read `CLAUDE.md` for project context
- [ ] Check SQL todos: `SELECT * FROM todos WHERE status = 'pending' ORDER BY id`
- [ ] Verify branch: `git branch --show-current` → `feature/deterministic-compute-core`

### Key Files to Understand

1. **Current WASM executor**: `icn/crates/icn-compute/src/wasm_executor.rs`
   - Line 59: `Engine::default()` - the problem
   - Lines 228-261: Host functions
   
2. **Wasmtime version**: `icn/crates/icn-compute/Cargo.toml` line 52
   - Currently 24.0.5 (security-pinned)

3. **Governance path**: `icn/crates/icn-governance-actor/src/handlers/execution.rs`
   - `translate_payload_to_effects()` - the LC boundary

4. **Trust graph**: `icn/crates/icn-trust/src/graph.rs`
   - No caps currently - need to add

### Implementation Order

1. **Start with Phase 1** (deterministic_engine.rs) - highest impact
2. **Then Phase 0** (docs) - can be done in parallel
3. **Then Phase 2** (nasties tests) - proves determinism works
4. **Then Phase 3** (subset validator) - hardens the boundary
5. **Phases 4-7** can follow

### Critical Rules

1. **Never weaken safety to fix tests** - if tests fail, the code is wrong
2. **Run verification**: `cd icn && cargo build && cargo test -p icn-compute`
3. **Update SQL todos**: `UPDATE todos SET status = 'in_progress' WHERE id = 'X'`
4. **Commit atomically**: One logical change per commit
5. **Follow conventional commits**: `feat(compute): add deterministic engine`

### Questions to Ask Before Proceeding

1. Is the aarch64 CI runner available? (Affects Phase 2)
2. Should we enforce integer-only LC? (Recommended yes)
3. Memory cap: 128 MiB or 256 MiB? (Recommend 128 for Pi)

---

## Appendix: Research Sprint #3 Key Corrections

From the original spec (preserved for reference):

1. **"O(1) Amortized" PPR is DoS vector** → Use Bounded Forward Push
2. **Standard WASM is not deterministic** → Hermetic Wasmtime config
3. **One-Step Proofs need canonical semantics** → Reference interpreter (Phase 7)
4. **Enforcement is reputation-scoped** → PoM triggers credit liquidation + bans

These corrections are incorporated into the implementation plan above.
