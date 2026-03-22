---
name: fix-rust-lints
description: Classify clippy/compiler failures by lint family and apply canonical idiomatic fixes.
argument-hint: "[lint output | --scan]"
user-invocable: true
allowed-tools: "Bash, Read, Edit, Grep"
---

Turn recurring Rust lint failures into known remediation classes. Never debug clippy from scratch.

## The Problem This Solves

The transcript discovered two lint patterns mid-run and fixed them ad hoc. Those same patterns
will recur. This skill pre-encodes them as named classes with canonical fix shapes so the model
stops relearning under pressure.

## Steps

1. **Collect failures**: If `$ARGUMENTS` is empty, run:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings 2>&1 | grep "^error"
   ```
   Or read from provided output.

2. **Classify each error** by lint name (appears in brackets after `error:`):
   ```bash
   cargo clippy ... 2>&1 | grep -E "^error\[|^\s+-->"
   ```

3. **Apply canonical fix** from the playbook below.

4. **Scan for recurrences** of the same anti-pattern in nearby code:
   ```bash
   grep -rn "<pattern>" crates/ apps/ bins/
   ```

5. **Verify**: Re-run the scoped clippy command to confirm the fix.

---

## Lint Remediation Playbook

### `field_reassign_with_default`

**Trigger**: `let mut x = T::default(); x.field = value;`

**Canonical fix**: Struct update syntax
```rust
// BEFORE
let mut cfg = FooConfig::default();
cfg.some_field = new_value;

// AFTER
let cfg = FooConfig {
    some_field: new_value,
    ..Default::default()
};
```

**Why**: Removes the `mut` binding, signals intent at construction, eliminates post-init mutation.

**Scan for recurrences**:
```bash
grep -rn "let mut .* = .*::default();" crates/ apps/ bins/ --include="*.rs"
```

---

### Deprecated item in `--all-targets` (test code)

**Trigger**: `use of deprecated ...` in test modules when compiling with `--all-targets -D warnings`

**Root cause**: `#[deprecated]` is transitive. CI uses `--all-targets`, which compiles test code.
A deprecated constant marked in a library still fires as an error wherever test code references it,
even if the non-test code is clean.

**Canonical fix**: Replace deprecated references with the recommended replacement, even in tests.
Do NOT use `#[allow(deprecated)]` unless the replacement doesn't exist yet.

```rust
// BEFORE (in test)
membership_age_secs: MIN_MEMBERSHIP_AGE_SECS + 1,

// AFTER
membership_age_secs: AttestationThresholds::default().min_membership_age_secs + 1,
```

**Scan for recurrences**:
```bash
# Find uses of deprecated constants in test modules
grep -rn "MIN_MEMBERSHIP_AGE_SECS\|MAX_ATTESTATIONS_PER_PERIOD\|MIN_TRUST_TO_ATTEST" \
  crates/ apps/ --include="*.rs" | grep -v "pub const\|#\[deprecated"
```

**Re-export workaround**: When a module re-exports deprecated items for backward compat,
add `#[allow(deprecated)]` on the `pub use` block only, not on callers:
```rust
#[allow(deprecated)]
pub use attestation::{LEGACY_CONST, old_function};
```

---

### `clippy::unwrap_used` / `clippy::expect_used` in tests

**Trigger**: `error: used unwrap() on a Result value` in test code

**Canonical fix**: Add workspace-level test allowance or use `#![cfg_attr(test, allow(...))]`
in the crate root. Do NOT sprinkle `#[allow]` throughout test functions.

```rust
// In lib.rs crate root:
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
```

---

### `clippy::too_many_arguments`

**Trigger**: Function has 8+ parameters.

**Canonical fix**: Either group related params into a config struct, or accept the lint suppression
if the function is a one-off constructor:
```rust
// Group into struct when params are logically related
pub fn build_policy(config: PolicyConfig) -> Policy { ... }

// Or suppress if it's an internal constructor unlikely to be called repeatedly
#[allow(clippy::too_many_arguments)]
pub fn new_internal(a: T, b: U, ...) -> Self { ... }
```

**ICN note**: Prefer the config struct approach for any function that appears in lifecycle.rs,
since those wire multiple values from config objects.

---

### `clippy::missing_errors_doc` / `clippy::missing_panics_doc`

**Trigger**: Public function lacks `# Errors` / `# Panics` doc section.

**Canonical fix**: Add the section, or suppress at crate level with `#![allow(missing_docs)]`
if the crate already has that allowance:
```rust
/// Does the thing.
///
/// # Errors
/// Returns `Err` if the configuration is invalid.
pub fn do_thing(&self) -> Result<(), String> { ... }
```

---

### Overflow / underflow in time arithmetic

**Trigger**: `SystemTime` subtraction that can underflow, or `u64::checked_mul` missing.

**Canonical fix**:
```rust
// BEFORE (panics on underflow in debug mode)
let cutoff = now - duration;

// AFTER
let cutoff = now.checked_sub(duration).unwrap_or(SystemTime::UNIX_EPOCH);

// For u64 multiplication that might overflow:
let secs = days.checked_mul(SECONDS_PER_DAY).unwrap_or(u64::MAX);
```

---

## Guardrails

- Never suppress a lint without understanding its class. Find the canonical fix first.
- Prefer fixing the anti-pattern to suppressing the warning.
- After fixing, scan for the same pattern in nearby code before committing.
- A fix that passes local `cargo clippy -p <crate>` may still fail CI `--workspace --all-targets`.
  Always scope-check with `--all-targets` before declaring victory.
