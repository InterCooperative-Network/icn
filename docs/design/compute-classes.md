# Compute Classes: Legitimacy Compute vs Utility Compute

**Status**: Specification  
**Version**: 1.0  
**Date**: 2026-02-11

---

## Overview

ICN distinguishes between two compute classes based on their relationship to authoritative shared state:

| Class | Abbreviation | Purpose | Determinism Requirement |
|-------|--------------|---------|------------------------|
| **Legitimacy Compute** | LC | State-changing operations | Bit-exact across architectures |
| **Utility Compute** | UC | Services, queries, analytics | Sandboxed and metered only |

This separation enables ICN to provide "AWS-scale compute" without "Ethereum gas economics."

---

## The Ethereum Inversion

> **Ethereum**: execution = legitimacy (global consensus on every instruction)  
> **ICN**: verification = legitimacy (cheap execution, deterministic verification)

Ethereum replicates execution across all nodes to establish truth. ICN allows fast local execution and only requires deterministic replay for verification when disputes arise.

---

## Legitimacy Compute (LC)

### Definition

LC covers any computation on the path:

```
DecisionReceipt → Effect → LedgerEntry / Settlement / Constraint enforcement
```

### Concrete Examples

- **Governance execution translator** - Converting proposals to kernel effects
- **Effect executor paths** - Treasury spend, membership changes, parameter updates
- **Receipt hashing** - Computing `decision_hash` for governance decisions
- **Contract evaluation** - Any CCL/WASM execution that can produce effects
- **Settlement logic** - Cross-cooperative credit settlements

### Requirements

1. **Bit-exact reproducibility** across x86_64 and aarch64
2. **Canonical serialization** for all inputs and outputs
3. **Bounded resource consumption** (fuel, memory, time)
4. **Auditable execution** with deterministic replay capability

### Execution Environment

LC MUST run in the **ICN Deterministic Core (ICN-DC)**:

- Wasmtime with NaN canonicalization enabled
- Fuel-based instruction metering
- No parallel compilation
- Static memory limits
- WASM subset restrictions (no floats in v1, no SIMD)

### DeterminismClass Enum

```rust
pub enum DeterminismClass {
    /// Must be deterministic; outputs can mutate state
    Canonical,
    /// Can be probabilistic; suggestions only
    Advisory,
}
```

- `Canonical` tasks MUST have `inputs_hash` for verification
- `Canonical` tasks MUST use the deterministic engine
- `Advisory` tasks CANNOT mutate ledger state (rejected by interpreter)

---

## Utility Compute (UC)

### Definition

UC covers helpful work that:
- Can be wrong or replayed
- Doesn't directly mutate authoritative state
- May use non-deterministic operations

### Concrete Examples

- **API services** - REST/GraphQL endpoints for cooperatives
- **Indexing** - Building search indices over ledger data
- **Analytics** - Computing statistics, reports, dashboards
- **Batch jobs** - Data processing, ETL pipelines
- **Background workers** - Notifications, async tasks

### Requirements

1. **Sandboxed** - Isolated from other workloads
2. **Metered** - Resource usage tracked and billed
3. **Accountable** - Execution attributed to identity
4. **NOT consensus-replicated** - May differ between nodes

### Execution Environment

UC runs in a standard sandboxed environment:

- Memory limits enforced
- CPU time limits enforced
- Network access allowed (with quotas)
- Filesystem access via virtual FS
- Can use non-deterministic operations

---

## Promotion Rules

UC output can become LC input through explicit promotion:

### The Proposal Pattern

```
1. UC service computes recommendation
2. UC output wrapped in Proposal
3. Proposal submitted to governance
4. Governance executes (LC) with deterministic translator
5. Effects applied to ledger
```

### Example: Price Oracle

```
UC: Fetch prices from external APIs, compute average
UC: Submit PriceUpdate proposal with computed price
LC: Governance evaluates proposal (deterministic)
LC: If approved, effect updates on-chain price
```

### Key Invariant

**UC can propose → LC (in-scope) verifies and records.**

UC never directly mutates authoritative state. It can only suggest changes that LC validates.

---

## Scope × Class Matrix

| Scope | LC Execution | UC Execution |
|-------|--------------|--------------|
| Commons | Never (commons is never authoritative) | Allowed, always preemptible |
| Local | For local-only state | Allowed, local priority |
| Cell | For cell-scoped decisions | Allowed, cell priority |
| Org | For org-wide governance | Allowed, org priority |
| Federation | For cross-org settlements | Allowed, federation priority |

---

## Implementation Notes

### Detecting LC vs UC

The `ComputeTask` struct includes `determinism_class`:

```rust
pub struct ComputeTask {
    pub determinism_class: DeterminismClass,
    pub inputs_hash: Option<ContentHash>,  // Required for Canonical
    // ...
}
```

The executor MUST:

1. Check `determinism_class` before execution
2. Use `create_deterministic_engine()` for `Canonical`
3. Reject `Canonical` tasks without `inputs_hash`
4. Log determinism manifest for audit

### Verification Flow

For any `Canonical` execution:

```
1. Verifier receives (inputs_hash, output_hash, executor_signature)
2. Verifier fetches inputs by hash
3. Verifier re-executes with deterministic engine
4. Compare computed output_hash with claimed output_hash
5. If mismatch → Proof of Misbehavior
```

---

## Rationale

### Why Not All LC?

Global deterministic execution is expensive:
- Requires all nodes to execute all code
- Limits throughput to slowest node
- Creates consensus bottleneck

### Why Not All UC?

Pure utility compute lacks:
- Cross-node agreement on state
- Verifiable governance decisions
- Auditable resource allocation

### The Middle Path

By separating LC and UC:
- LC provides legitimacy through verifiability
- UC provides scale through locality
- Both coexist in the same network
- Cooperatives get real compute + real governance

---

## References

- `docs/design/DETERMINISTIC_COMPUTE_SPRINT.md` - Implementation plan
- `docs/design/scope-scheduling.md` - Scope and priority model
- `docs/design/deterministic-core.md` - ICN-DC specification
- `icn/crates/icn-kernel-api/src/compute.rs` - DeterminismClass enum
