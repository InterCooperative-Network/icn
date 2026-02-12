# ICN Deterministic Core (ICN-DC) Specification

**Status**: Specification  
**Version**: 1.0  
**Date**: 2026-02-11  
**Applies To**: Legitimacy Compute (LC) execution paths

---

## Overview

The ICN Deterministic Core (ICN-DC) is a hardened WASM execution environment that guarantees bit-exact reproducibility across supported architectures (x86_64, aarch64).

ICN-DC is required for all **Legitimacy Compute (LC)** - any computation that can affect authoritative shared state.

---

## Design Goals

1. **Bit-exact reproducibility** - Same inputs → same outputs on any compliant node
2. **Verifiable execution** - Any node can replay and verify any LC execution
3. **Bounded resources** - Guaranteed termination, predictable costs
4. **Defense in depth** - Multiple layers prevent non-determinism

---

## Wasmtime Configuration

### Version Pin

```rust
/// Pinned Wasmtime version. Upgrading is a HARD FORK.
pub const WASMTIME_VERSION_PIN: &str = "24.0.5";
```

**Policy**: All configuration flags are defined against this version. Upgrading requires:
1. Cross-architecture determinism re-certification
2. "Nondeterministic Nasties" test suite pass on both archs
3. Protocol-level coordination (all nodes upgrade together)

**Fail-Closed Rule**: If a config knob specified below is not available in Wasmtime 24.0.5, we DO NOT approximate it. LC execution is forbidden until the knob is available or we redesign. This prevents "imaginary determinism" where we claim a guarantee we can't deliver.

### Engine Configuration

```rust
pub fn create_deterministic_engine() -> anyhow::Result<(Engine, DeterminismManifest)> {
    let mut config = Config::new();
    
    // 1. NaN Canonicalization
    // Forces all NaNs to canonical bit pattern 0x7fc00000
    // Critical for x86 vs ARM consistency
    config.cranelift_nan_canonicalization(true);
    
    // 2. Fuel Consumption
    // Replaces wall-clock timeouts with instruction counting
    // Guarantees same fuel consumption regardless of CPU speed
    config.consume_fuel(true);
    
    // 3. Sequential Compilation
    // Reduces nondeterministic build variance and simplifies audit.
    // NOTE: Determinism must not depend on JIT artifact identity.
    config.parallel_compilation(false);
    
    // 4. Memory Configuration
    // Static limits prevent OOM-dependent behavior
    // NOTE: Verify these APIs exist in Wasmtime 24.0.5 before using
    // If unavailable, fail closed (reject LC execution) until redesign
    let max_memory_bytes = 128 * 1024 * 1024; // 128 MiB
    config.static_memory_maximum_size(max_memory_bytes);
    // These may not exist in 24.0.5 - verify before implementation:
    // config.static_memory_guard_size(2 * 1024 * 1024);
    // config.dynamic_memory_reserved_for_growth(max_memory_bytes);
    
    Ok((Engine::new(&config)?, manifest))
}
```

### Configuration Rationale

| Setting | Value | Reason |
|---------|-------|--------|
| `cranelift_nan_canonicalization` | `true` | Different CPUs produce different NaN bit patterns |
| `consume_fuel` | `true` | Wall-clock timeouts are non-deterministic |
| `parallel_compilation` | `false` | Reduces build variance, simplifies audit (determinism must not depend on JIT identity) |
| `static_memory_maximum_size` | 128 MiB | Prevents OOM-dependent behavior |

---

## WASM Subset Restrictions

### Overview

LC WASM modules are validated against a restricted subset. Modules violating the subset are rejected at load time.

### Import Allowlist

Only these imports are allowed for LC modules:

| Module | Function | Signature | Behavior |
|--------|----------|-----------|----------|
| `icn` | `log` | `(ptr: i32, len: i32)` | Log message (no side effects) |
| `icn` | `timestamp` | `() -> i64` | Returns `task.created_at` (deterministic) |
| `icn` | `entropy_seed` | `(ptr: i32)` | Writes 32-byte seed to ptr (optional) |

**Canonical naming**: All host functions use module `"icn"` with function names like `"log"`, `"timestamp"`. In WASM: `(import "icn" "log" ...)`. In Rust linker: `icn::log`.

### Forbidden Imports

Any import not on the allowlist is forbidden:

- `wasi_*` - WASI filesystem, network, clock
- `env::*` - Environment variables
- Custom host functions returning non-deterministic values

### Opcode Restrictions

**Forbidden in LC v1**:

| Category | Opcodes | Reason |
|----------|---------|--------|
| **Floats** | `f32.*`, `f64.*` | NaN handling varies by architecture |
| **SIMD** | `v128.*`, `*.relaxed_*` | Hardware-specific optimizations |

**Rationale**: Integer-only LC massively reduces the fork surface. Floats can be allowed in v2 with proven NaN canonicalization.

### Control Flow Limits

| Limit | Value | Reason |
|-------|-------|--------|
| Max control depth | 100 | Prevents stack exhaustion attacks |
| Max call depth | 100 | Prevents unbounded recursion |

### Memory Limits

| Limit | Value | Reason |
|-------|-------|--------|
| Max `memory.grow` | 2048 pages (128 MiB) | Protocol-governed limit |
| Max `memory.copy` | 1 MiB | Prevents witness bloat |
| Max `memory.fill` | 1 MiB | Prevents witness bloat |

---

## Deterministic Entropy

### The Problem

WASM contracts may need randomness (e.g., shuffling, sampling). Host `/dev/urandom` is non-deterministic.

### The Solution: Deterministic Seed Injection

```rust
/// Derive deterministic seed from execution context
/// 
/// ICN-native anchors (not blockchain terminology):
/// - scope_root: The Merkle root of the scope's ledger at task creation
/// - decision_hash: The canonical hash of the decision/receipt that triggered this task
pub fn derive_seed(scope_root: &[u8; 32], decision_hash: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"icn:entropy:v1");
    hasher.update(scope_root);
    hasher.update(decision_hash);
    *hasher.finalize().as_bytes()
}
```

**Where these values come from**:
- `scope_root`: Obtained from `Ledger::current_root()` at task creation time
- `decision_hash`: Obtained from `GovernanceDecisionReceipt::canonical_hash()` (for governance LC) or `Task::decision_hash` field (for compute LC)

**Policy**: 
- Host entropy is BANNED
- Contracts receive deterministic seed derived from context
- Seed is reproducible by any verifier

---

## Memory Growth Policy

### The Problem

OOM (Out of Memory) errors dependent on host OS pressure cause consensus forks.

### The Solution

1. **Deterministic Address Space**: Nodes reserve full protocol-max (128 MiB) for LC instances
2. **Protocol-Governed Failure**: `memory.grow` success determined solely by protocol limit
3. **Non-Compliance Handling**: If host OS refuses allocation, host is faulty and must crash rather than fork

### Implementation

```rust
impl ResourceLimiter for LcContext {
    fn memory_growing(&mut self, current: usize, desired: usize, max: Option<usize>) -> bool {
        // Success determined by protocol limit, not host RAM
        let protocol_limit = 128 * 1024 * 1024; // 128 MiB
        (current + desired) <= protocol_limit
    }
}
```

---

## Container Ordering Policy

### The Problem

`HashMap`/`HashSet` use randomized seeding (SipHash), causing non-deterministic iteration order.

### The Solution

**Policy**: `HashMap` is PROHIBITED for state-impacting logic in LC paths.

**Allowed Alternatives**:
- `BTreeMap` - Key-sorted, deterministic
- `IndexMap` - Insertion-sorted, deterministic
- `Vec` - Index-based, deterministic

### CI Enforcement

```bash
# Detect HashMap in LC-relevant crates
rg "HashMap" icn/crates/icn-compute/src/deterministic_engine.rs
# Should return 0 matches
```

---

## Fuel Metering

### Overview

Fuel replaces wall-clock timeouts for deterministic execution bounds.

### Fuel Allocation

```rust
// Convert task fuel_limit to Wasmtime fuel
let wasmtime_fuel = task.fuel_limit.0;
store.set_fuel(wasmtime_fuel)?;
```

### Fuel Consumption Tracking

```rust
// After execution
let fuel_used = initial_fuel - store.get_fuel()?;
result.fuel_used = fuel_used;
```

### Fuel Exhaustion

If fuel runs out mid-execution:
- WASM traps with `FuelExhausted`
- Execution result is `Err(ComputeError::FuelExhausted)`
- No state changes applied
- Task marked as failed (not misbehavior)

---

## Verification Protocol

### For Deterministic Replay

Any node can verify an LC execution:

```
1. Obtain canonical inputs (by inputs_hash)
2. Obtain WASM module (by wasm_hash)
3. Create deterministic engine with ICN-DC config
4. Execute with same fuel_limit
5. Compare output_hash with claimed output_hash
6. If mismatch → Proof of Misbehavior
```

### Canonical Inputs

```rust
pub struct CanonicalInputs {
    pub wasm_hash: [u8; 32],
    pub input_data: Vec<u8>,
    pub fuel_limit: u64,
    pub timestamp: u64,  // task.created_at
    pub entropy_seed: [u8; 32],  // derived from context
}

impl CanonicalInputs {
    pub fn content_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"icn:inputs:v1");
        hasher.update(&self.wasm_hash);
        hasher.update(&self.input_data);
        hasher.update(&self.fuel_limit.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(&self.entropy_seed);
        *hasher.finalize().as_bytes()
    }
}
```

---

## Test Suite: Nondeterministic Nasties

### Overview

The "Nasties" test suite verifies ICN-DC normalizes hardware differences.

### Test Cases

| Test | Vulnerability | Success Condition |
|------|---------------|-------------------|
| `nan_payload_miner` | NaN bit patterns | `i32.reinterpret_f32(0.0/0.0) == 0x7fc00000` on x86 and ARM |
| `simd_fma_diff` | FMA rounding | Bit-exact vector results |
| `mem_grow_boundary` | OOM consensus | `memory.grow` fails at exactly page 2048 |
| `hashmap_chaos` | Iteration order | Output hash constant across 1000 runs |

### Cross-Architecture CI

```yaml
# .github/workflows/determinism.yml
jobs:
  determinism-x86:
    runs-on: ubuntu-latest
    # ... run nasties, upload hashes
  
  determinism-aarch64:
    runs-on: [self-hosted, aarch64]
    # ... run nasties, upload hashes
  
  compare:
    needs: [determinism-x86, determinism-aarch64]
    # diff hashes, fail if mismatch
```

---

## Error Handling

### Deterministic Errors

These errors are deterministic and should produce identical results:

| Error | Handling |
|-------|----------|
| `FuelExhausted` | Trap, return error |
| `MemoryGrowFailed` | Trap, return error |
| `DivisionByZero` | Trap, return error |
| `SubsetViolation` | Reject at load time |

### Non-Deterministic Errors

These errors must NOT occur in LC paths:

| Error | Prevention |
|-------|------------|
| `HostOOM` | Reserve full address space upfront |
| `Timeout` | Use fuel, not wall-clock |
| `IOError` | No IO allowed in LC |

---

## Security Considerations

### Attack Vectors Mitigated

1. **Subjectivity attacks**: Exploiting hardware variance → Mitigated by ICN-DC config
2. **Resource exhaustion**: Infinite loops → Mitigated by fuel metering
3. **Memory bombs**: Excessive allocation → Mitigated by static limits
4. **Witness bloat**: Large memory access → Mitigated by copy/fill limits

### Defense in Depth

```
Layer 1: WASM subset validation (reject bad modules)
Layer 2: Deterministic engine config (normalize execution)
Layer 3: Fuel metering (bound execution)
Layer 4: Verification capability (detect misbehavior)
```

---

## Implementation Checklist

- [ ] Create `deterministic_engine.rs` with `create_deterministic_engine()`
- [ ] Create `subset.rs` with `WasmSubsetValidator`
- [ ] Modify `wasm_executor.rs` to use deterministic engine for LC
- [ ] Add CI guard for `Engine::default()` in LC paths
- [ ] Implement "Nasties" test suite
- [ ] Set up cross-architecture CI
- [ ] Document in `docs/spec/wasm-subset.md`

---

## References

- Research Sprint #3 (Security & Determinism Spec v1.1)
- `docs/design/compute-classes.md` - LC/UC specification
- `docs/design/scope-scheduling.md` - Scope and priority model
- `docs/design/DETERMINISTIC_COMPUTE_SPRINT.md` - Implementation plan
- Wasmtime documentation: https://docs.wasmtime.dev/
