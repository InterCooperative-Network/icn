---
Status: ready-for-execution
Created: 2026-03-28
Design doc: commons-persistence-design.md
Branch: feat/commons-persistence
---

# CommonsManager Persistence — Execution Checklist

## Pre-flight (before touching code)

- [ ] `git checkout main && git pull` — confirm at `168653bc` (post PR #1450)
- [ ] `git checkout -b feat/commons-persistence`
- [ ] `git branch --show-current` — must be `feat/commons-persistence`
- [ ] Read `docs/development/commons-persistence-design.md` (the full design)
- [ ] Verify `self.data_dir` is set in production daemon startup:
  ```bash
  grep -n 'data_dir\|with_data_dir' icn/crates/icn-gateway/src/server.rs | head -10
  grep -n 'data_dir\|with_data_dir' icn/crates/icn-core/src/supervisor.rs | head -10
  ```
- [ ] Confirm `flush()` callers (before deciding to keep or move it):
  ```bash
  grep -rn '\.flush()' icn/crates/icn-gateway/src/ --include='*.rs' | grep -v test
  ```

---

## Step 1 — Add blanket impl in `commons_store.rs`

**File:** `icn/crates/icn-gateway/src/commons_store.rs`

After the `CommonsStoreBackend` trait definition (around line 72), add:

```rust
/// Allow `Arc<dyn CommonsStoreBackend>` to be used as a backend directly.
/// This enables type-erased CommonsManager construction without changing handler signatures.
impl CommonsStoreBackend for Arc<dyn CommonsStoreBackend> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.as_ref().get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.as_ref().put(key, value)
    }
    fn delete(&self, key: &[u8]) -> Result<()> {
        self.as_ref().delete(key)
    }
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.as_ref().scan(prefix)
    }
}
```

Verify: `cargo check -p icn-gateway` — should compile (no handler changes needed yet).

---

## Step 2 — Update `CommonsManager` in `commons_mgr.rs`

**File:** `icn/crates/icn-gateway/src/commons_mgr.rs`

### 2a — Change default type parameter (line 40)

```rust
// BEFORE:
pub struct CommonsManager<S: CommonsStoreBackend = InMemoryCommonsStore> {

// AFTER:
pub struct CommonsManager<S: CommonsStoreBackend = Arc<dyn CommonsStoreBackend>> {
```

Add import at top if not present:
```rust
use std::sync::Arc;
// Arc<dyn CommonsStoreBackend> needs this — already imported in most files
```

### 2b — Replace `impl CommonsManager<InMemoryCommonsStore>` block (line 48)

```rust
// REPLACE the old InMemoryCommonsStore impl block with:
impl CommonsManager<Arc<dyn CommonsStoreBackend>> {
    /// In-memory backend (testing / no data_dir configured)
    pub fn new() -> Self {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(InMemoryCommonsStore::new());
        Self::with_store(backend)
    }

    /// Persistent sled backend (production)
    pub fn with_sled_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let backend: Arc<dyn CommonsStoreBackend> =
            Arc::new(SledCommonsStore::open(path).context("Failed to open commons sled store")?);
        Ok(Self::with_store(backend))
    }

    /// Temporary sled backend (tests needing real persistence behavior)
    pub fn with_sled_temporary() -> Result<Self> {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(
            SledCommonsStore::temporary().context("Failed to create temporary commons sled store")?,
        );
        Ok(Self::with_store(backend))
    }
}
```

### 2c — Handle `flush()` from `impl CommonsManager<SledCommonsStore>`

Check if `flush()` is called anywhere:
```bash
grep -rn 'commons_manager.*flush\|\.flush()' icn/crates/icn-gateway/src/ | grep -v test
```

If unused externally, add to the `Arc<dyn CommonsStoreBackend>` impl as a no-op or
move the `SledCommonsStore`-specific impl into a helper. If needed:

```rust
// Add to CommonsStoreBackend trait (optional extension):
fn flush(&self) -> Result<()> { Ok(()) }  // default no-op
```

Then `SledCommonsStore` can override it.

### 2d — Remove or deprecate `impl CommonsManager<SledCommonsStore>` block

The old `with_sled_path` / `with_sled_temporary` / `flush()` methods in the
`SledCommonsStore` specific impl block are superseded. Remove or add `#[deprecated]`.

Verify compile: `cargo check -p icn-gateway` — fix any type errors.

---

## Step 3 — Update `server.rs`

**File:** `icn/crates/icn-gateway/src/server.rs`
**Lines:** 692-701 (current `CommonsManager::new()` block)

Replace:

```rust
// TODO: wire CommonsManager to a daemon handle (CommonsHandle) for cross-restart
// persistence. Currently all commons/personhood/charter/enrollment state is
// in-memory only and will be lost on process restart.
// See GovernanceManager::with_handle() for the pattern to follow.
warn!(
    "CommonsManager running in-memory-only mode: \
     commons/personhood/charter/enrollment state will NOT survive process restart. \
     Wire a CommonsHandle for production use."
);
let commons_manager = Arc::new(CommonsManager::new());
```

With:

```rust
let commons_manager: Arc<CommonsManager> = if let Some(ref data_dir) = self.data_dir {
    let commons_path = data_dir.join("commons.sled");
    info!("CommonsManager: opening sled store at {:?}", commons_path);
    Arc::new(
        CommonsManager::with_sled_path(&commons_path)
            .context("Failed to open commons sled store")?,
    )
} else {
    warn!(
        "CommonsManager: no data_dir configured, running in-memory only — \
         commons/personhood/charter/enrollment state will NOT survive process restart"
    );
    Arc::new(CommonsManager::new())
};
```

**Check:** `data_dir.join("commons.sled")` must NOT collide with any existing sled path.
Confirm no other store uses `commons.sled`:
```bash
grep -rn '"commons' icn/crates/icn-gateway/src/server.rs
grep -rn 'join.*sled\|sled.*join' icn/crates/icn-gateway/src/server.rs
```

---

## Step 4 — Full compile check

```bash
cd /home/ubuntu/projects/icn/icn
cargo check -p icn-gateway 2>&1 | head -50
```

Expect: clean compile. If there are type errors, they will be in:
- `commons_mgr.rs` — type param instantiation sites
- `commons_store.rs` — LRU cache size or `Arc<dyn ...>` constraint issues
- `server.rs` — result handling on `CommonsManager::with_sled_path`

---

## Step 5 — Write persistence proof tests

**File:** `icn/crates/icn-gateway/tests/commons_integration.rs`

Add these tests (see testing plan doc for full spec):
1. `test_commons_anchor_survives_sled_drop_and_reopen` — Layer 1 (direct sled)
2. `test_commons_charter_survives_sled_drop_and_reopen` — Layer 1 variant for charter
3. `test_commons_holder_survives_sled_drop_and_reopen` — Layer 1 variant for holder

These use `CommonsManager::with_sled_temporary()` so they don't need tempfile dirs.
But to prove drop-and-reopen, they need a real path:

```rust
let tmp = tempfile::tempdir().unwrap();
let sled_path = tmp.path().join("commons.sled");

// Phase 1: write
{
    let mgr = CommonsManager::with_sled_path(&sled_path).unwrap();
    mgr.create_anchor_from_enrollment(...).await.unwrap();
    // drop mgr here
}

// Phase 2: reopen
let mgr2 = CommonsManager::with_sled_path(&sled_path).unwrap();
let anchor = mgr2.get_anchor(&anchor_id).await.unwrap();
assert!(anchor.is_some());
```

---

## Step 6 — Run tests

```bash
# Unit tests for commons_store inline
cargo test -p icn-gateway --lib -- commons_store 2>&1

# Full gateway integration tests
cargo test -p icn-gateway 2>&1 | tail -20
```

Fix any failures before proceeding.

---

## Step 7 — Scoped fmt + clippy

```bash
cargo fmt --all --check 2>&1
# if fails: cargo fmt --all

cargo clippy -p icn-gateway --all-targets -- -D warnings 2>&1 | tail -20
```

---

## Step 8 — Commit and push

```bash
git add icn/crates/icn-gateway/src/commons_store.rs \
        icn/crates/icn-gateway/src/commons_mgr.rs \
        icn/crates/icn-gateway/src/server.rs \
        icn/crates/icn-gateway/tests/commons_integration.rs

git commit -m "feat(gateway): wire CommonsManager to sled-backed storage

Replace in-memory CommonsManager default with Arc<dyn CommonsStoreBackend>
type-erased backend. server.rs now opens commons.sled when data_dir is set,
falling back to in-memory when not configured.

Adds persistence proof tests (L1 drop-and-reopen) for anchor, charter,
and holder state. RevocationRegistry remains in-memory (out of scope).
"

# push via /push skill — runs fmt+clippy+tests before pushing
```

---

## Verification sequence summary

```bash
cargo check -p icn-gateway           # fast type check
cargo test -p icn-gateway --lib      # inline tests (commons_store)
cargo test -p icn-gateway            # full integration tests
cargo fmt --all --check
cargo clippy -p icn-gateway --all-targets -- -D warnings
```

---

## Likely failure points

| Failure | Cause | Fix |
|---------|-------|-----|
| `type mismatch` on `CommonsManager::new()` | Old call site expects `InMemoryCommonsStore` | Update to new default |
| `CommonsStore<Arc<dyn ...>>: Unpin` required | Async trait bounds | Add `+ Unpin` or use `Box::pin` |
| sled open error at startup | Sled lock held by test | Use `temporary()` in tests, `commons.sled` in prod |
| `flush()` not found on `CommonsManager` | Moved method | Add to trait or keep SledCommonsStore impl |
| `CommonsManager` in handler not `Sync+Send` | `Arc<dyn ...>` not `Sync` | Add `+ Send + Sync` bound to `CommonsStoreBackend` (already there) |
| Handler compile error | Type default mismatch | Ensure handlers use `CommonsManager` (no explicit param) |
