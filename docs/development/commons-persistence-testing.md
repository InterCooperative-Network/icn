---
Status: ready-for-execution
Created: 2026-03-28
Design doc: commons-persistence-design.md
---

# CommonsManager Persistence — Testing Plan

## What must be proven

Commons persistence is sled-backed (same as governance and ledger). The standard
4-layer verification pattern applies (`docs/development/testing/verification-pattern.md`).

For this tranche, **Layer 1 (direct sled drop-and-reopen)** is the minimum bar.
Layers 2-4 are stretch goals if time permits — document what would be needed.

### Invariants that must hold after a drop-and-reopen boundary

For each entity type:
1. The record is present after sled reopen (not silently lost).
2. Key identifying fields survive exact round-trip (DID, anchor_id, charter_id, etc.).
3. Status fields survive exact round-trip.

### What "done" means for this tranche

- `CommonsManager::with_sled_path()` can open an existing sled db and retrieve
  previously written records
- `server.rs` uses sled when `data_dir` is configured
- At least 1 passing drop-and-reopen test per major entity type (anchor, charter, holder)
- All existing `commons_integration.rs` tests still pass (regression check)
- `cargo test -p icn-gateway` clean

---

## Layer 1 — Direct persistence tests

**File:** `icn/crates/icn-gateway/tests/commons_integration.rs`

**Add these tests:**

### Test 1: PersonhoodAnchor survives sled drop-and-reopen

```rust
#[tokio::test]
async fn test_commons_anchor_survives_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("commons.sled");

    let alice = KeyPair::generate().expect("alice").did().clone();

    // Phase 1: write
    let anchor_id = {
        let mgr = CommonsManager::with_sled_path(&sled_path)
            .expect("open sled");
        let anchor_id = mgr.create_anchor_from_enrollment(
            alice.clone(),
            /* ... params ... */
        ).await.expect("create anchor");
        anchor_id
        // mgr drops here — sled lock released
    };

    // Phase 2: reopen fresh
    let mgr2 = CommonsManager::with_sled_path(&sled_path).expect("reopen sled");
    let anchor = mgr2.get_anchor(&anchor_id).await.expect("get_anchor");
    assert!(anchor.is_some(), "anchor must survive sled drop-and-reopen");
    let anchor = anchor.unwrap();
    assert_eq!(anchor.did, alice, "DID must survive round-trip");
}
```

### Test 2: Charter survives sled drop-and-reopen

```rust
#[tokio::test]
async fn test_commons_charter_survives_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("commons.sled");

    let charter_id = {
        let mgr = CommonsManager::with_sled_path(&sled_path).expect("open sled");
        let charter = Charter { /* minimal test charter */ };
        let id = mgr.store_charter(charter).await.expect("store charter");
        id
    };

    let mgr2 = CommonsManager::with_sled_path(&sled_path).expect("reopen sled");
    let retrieved = mgr2.get_charter(&charter_id).await.expect("get_charter");
    assert!(retrieved.is_some(), "charter must survive sled drop-and-reopen");
}
```

### Test 3: CommonsHolderRecord survives sled drop-and-reopen

```rust
#[tokio::test]
async fn test_commons_holder_survives_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("commons.sled");

    let alice = KeyPair::generate().expect("alice").did().clone();

    let holder_id = {
        let mgr = CommonsManager::with_sled_path(&sled_path).expect("open sled");
        // Create anchor first (holder requires anchor)
        let anchor_id = mgr.create_anchor_from_enrollment(alice.clone(), ...).await?;
        let holder_id = mgr.create_holder_from_anchor_with_name(anchor_id, "Alice", alice.clone()).await?;
        holder_id
    };

    let mgr2 = CommonsManager::with_sled_path(&sled_path).expect("reopen sled");
    let holder = mgr2.get_holder(&holder_id).await.expect("get_holder");
    assert!(holder.is_some(), "holder must survive sled drop-and-reopen");
    let holder = holder.unwrap();
    assert_eq!(holder.did, alice);
}
```

---

## Existing tests — regression check

All of these must continue to pass:

```bash
# Runs all tests in icn-gateway including existing commons_integration.rs
cargo test -p icn-gateway 2>&1 | grep -E 'test.*commons|FAILED|ok$' | head -30
```

The existing tests use `CommonsManager::new()` — they get an in-memory backend, same
as before. Behavior must be identical.

---

## Layer 2-4 stretch (for later, not this tranche)

| Layer | What it would prove | Notes |
|-------|-------------------|-------|
| L2 | CommonsManager constructed via server.rs path (with data_dir) writes to sled correctly | Requires a gateway test fixture |
| L3 | Same-runtime drop + recreate (same Arc lifecycle as trust L3) | Simple: CommonsManager has no background task |
| L4 | Cross-process restart via `commons_restart_helper` binary | Pattern identical to `trust_restart_helper` |

L3 is trivial (no background scheduler — same as trust). L4 would follow the exact
pattern in `docs/development/testing/verification-pattern.md`.

---

## Verification sequence

```bash
# 1. Inline store tests
cargo test -p icn-gateway --lib -- commons_store 2>&1

# 2. All gateway tests (includes new persistence tests + existing regression)
cargo test -p icn-gateway 2>&1 | tail -20

# 3. Fmt
cargo fmt --all --check 2>&1

# 4. Clippy
cargo clippy -p icn-gateway --all-targets -- -D warnings 2>&1 | tail -10
```

---

## Regression risks

| Risk | Mitigation |
|------|-----------|
| Existing tests break because `CommonsManager::new()` changed type | Verify: new `new()` still returns usable in-memory manager. Run all tests. |
| `SledCommonsStore::flush()` dropped without replacement | Check callers before removing. Add to trait if needed. |
| RevocationRegistry silently not persisted | This is EXPECTED and in-scope. The warn log is still present. Do NOT add revocation persistence in this tranche. |
| Sled path collision with other stores | Verified: `commons.sled` is a unique path. Double-check in test. |

---

## What this tranche does NOT test

- `RevocationRegistry` persistence (in-memory by design, out of scope)
- CommonsHandle actor pattern (not needed for restart persistence, out of scope)
- Cross-node sync of commons state (out of scope)
- Migration of existing in-memory state (no migration — fresh state on restart, same as today)
