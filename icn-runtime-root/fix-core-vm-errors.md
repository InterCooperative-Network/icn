# Core VM Module Fixes

This document outlines the issues in the core-vm module and provides solutions.

## Issue 1: Missing `Clone` trait for `ConcreteHostEnvironment`

The error occurs because we try to clone the host environment in various functions, but the struct doesn't implement `Clone`:

```rust
// Add this derive to the ConcreteHostEnvironment struct:
#[derive(Clone)]
pub struct ConcreteHostEnvironment {
    // ... existing fields
}
```

## Issue 2: Using deprecated `Trap::new` method

In the current wasmtime version, the `Trap::new` method has been removed and replaced with `Trap::throw`. 
All occurrences of `Trap::new` should be replaced with `Trap::throw`:

For example, replace:
```rust
.map_err(|e| Trap::new(format!("Invalid CID: {}", e)))?;
```

with:
```rust
.map_err(|e| Trap::throw(format!("Invalid CID: {}", e)))?;
```

These pattern occurs in multiple places in the file:

1. In `host_storage_get` (around line 773)
2. In `host_storage_put` (around line 814)
3. In `host_blob_put` (around line 844)
4. In `host_blob_get` (around line 866, 874)
5. In `host_get_caller_did` (around line 906)
6. In `host_get_caller_scope` (around line 925)
7. In `host_verify_signature` (around line 956)
8. In `host_check_resource_authorization` (around lines 972, 980, 985)
9. In `host_record_resource_usage` (around lines 995, 1003, 1012)
10. In `host_budget_allocate` (around lines 1022, 1033, 1043)
11. In `host_anchor_to_dag` (around lines 1097, 1110)
12. In `read_memory_string` (around line 1204, 1210, 1220, 1228)
13. In `read_memory_bytes` (around line 1234, 1244)
14. In `write_memory_string` (around line 1256, 1264, 1272)
15. In `allocate_memory` (around line 1288, 1296)

## Issue 3: IntoFunc trait bounds

The error is related to closure type compatibility in the wasmtime 12.0.2 update. 

The error occurs when using `func_wrap` to register host functions. This is a more complex issue that may require updating the function signatures or updating the wasmtime dependency.

For a simpler fix in the interim, we can temporarily comment out the core-vm dependency in crates that only need it for types, like:

```toml
# In crates/governance-kernel/Cargo.toml
[dependencies]
icn-identity = { path = "../identity" }
# Temporarily comment out core-vm dependency while it's being fixed
# icn-core-vm = { path = "../core-vm" }
```

And then add a temporary type alias in those crates:

```rust
// Temporary type alias until core-vm is fixed
type HostResult<T> = Result<T, String>;
```

## Issue 4: Missing `get_export` method in `StoreContextMut`

This error occurs because the `get_export` method is no longer available on `StoreContextMut` in the newer wasmtime version.

We need to update how exports are accessed, for example:

Replace:
```rust
caller.as_context_mut().get_export("memory")
```

With something like:
```rust
caller.get_export("memory")
```

Or find the appropriate accessor method in the newer wasmtime API.

---

These issues need to be addressed in `crates/core-vm/src/lib.rs` to make the governance-kernel module compile properly. 