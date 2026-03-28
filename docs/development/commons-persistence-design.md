---
Status: ready-for-implementation
Created: 2026-03-28
Branch: feat/commons-persistence (recommended)
---

# CommonsManager Persistence — Design Doc

## Problem

`CommonsManager` in `crates/icn-gateway/src/commons_mgr.rs` defaults to
`CommonsManager<InMemoryCommonsStore>`. All commons/personhood/charter/enrollment
state lives only in heap memory and is lost on every process restart.

The warning added in PR #1450 (`crates/icn-gateway/src/server.rs:697`) documents
this gap explicitly:

```
CommonsManager running in-memory-only mode: commons/personhood/charter/enrollment
state will NOT survive process restart. Wire a CommonsHandle for production use.
```

**What is lost on restart:**
- `PersonhoodAnchor` records (Layer 0 — identity roots)
- `CommonsHolderRecord` records (Layer 1 — cooperative membership state)
- `Charter` records (Layer 2 — cooperative constitutions + signatures)
- `StewardRecord` + `Amendment` + `Appeal` records
- `EnrollmentSession` records (in-flight enrollment ceremonies)
- `RevocationRegistry` (in-memory only, separate from `CommonsStore`)

**What is NOT affected:**
- `RevocationRegistry` — already in-memory-only by design; noted as out-of-scope
  for this tranche (see below)

---

## Architecture: What Already Exists

This tranche is simpler than it looks. The storage infrastructure is already built.

### Already implemented
- `CommonsStoreBackend` trait — object-safe interface (`get`, `put`, `delete`, `scan`)
  → `crates/icn-gateway/src/commons_store.rs:56`
- `InMemoryCommonsStore` — in-memory BTreeMap backend
  → `commons_store.rs:81`
- `SledCommonsStore` — fully implemented sled backend with `open()` and `temporary()`
  → `commons_store.rs:179`
- `CommonsStore<S>` — generic caching wrapper (LRU per entity type)
  → `commons_store.rs:268`
- `CommonsManager<SledCommonsStore>` impl with `with_sled_path()` and `with_sled_temporary()`
  → `commons_mgr.rs:58-80`
- Existing tests for CommonsStore sled persistence
  → `commons_store.rs:1408-1580` (inline tests)
- Existing integration tests using in-memory manager
  → `crates/icn-gateway/tests/commons_integration.rs`

### What does NOT exist (needs to be built)
- `CommonsManager` is generic (`CommonsManager<S>`) — all 13 handler files use
  `CommonsManager` (defaulting to `CommonsManager<InMemoryCommonsStore>`)
- No type erasure — can't switch backends at runtime without updating handler types
- Server.rs builds `CommonsManager::new()` unconditionally — no `data_dir` branch

---

## The Core Implementation Challenge

`CommonsManager<S>` is parameterized by its storage backend. All actix handler
signatures use `web::Data<Arc<CommonsManager>>` (= `CommonsManager<InMemoryCommonsStore>`).
If we switch to `SledCommonsStore`, all 13 handler files need type param updates.

**The recommended fix: type-erase `CommonsManager`** by removing the generic type
parameter and using `Arc<dyn CommonsStoreBackend>` internally. This touches only
`commons_mgr.rs`, `commons_store.rs`, and `server.rs`. Handler files: zero changes.

---

## Target Architecture

### Step 1 — Add blanket impl for `Arc<dyn CommonsStoreBackend>`

In `commons_store.rs`, add:

```rust
impl CommonsStoreBackend for Arc<dyn CommonsStoreBackend> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> { self.as_ref().get(key) }
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> { self.as_ref().put(key, value) }
    fn delete(&self, key: &[u8]) -> Result<()> { self.as_ref().delete(key) }
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> { self.as_ref().scan(prefix) }
}
```

This makes `CommonsStore<Arc<dyn CommonsStoreBackend>>` valid.

### Step 2 — Change `CommonsManager` default type parameter

```rust
// BEFORE:
pub struct CommonsManager<S: CommonsStoreBackend = InMemoryCommonsStore> {

// AFTER:
pub struct CommonsManager<S: CommonsStoreBackend = Arc<dyn CommonsStoreBackend>> {
```

### Step 3 — Update constructors to box the backend

```rust
impl CommonsManager<Arc<dyn CommonsStoreBackend>> {
    /// In-memory (testing/fallback)
    pub fn new() -> Self {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(InMemoryCommonsStore::new());
        Self::with_store(backend)
    }

    /// Sled-backed (production)
    pub fn with_sled_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(SledCommonsStore::open(path)?);
        Ok(Self::with_store(backend))
    }

    pub fn with_sled_temporary() -> Result<Self> {
        let backend: Arc<dyn CommonsStoreBackend> = Arc::new(SledCommonsStore::temporary()?);
        Ok(Self::with_store(backend))
    }
}
```

This means `CommonsManager::new()` and `CommonsManager::with_sled_path()` both
return `CommonsManager<Arc<dyn CommonsStoreBackend>>` — same type, different runtime
backend. All handler files use `CommonsManager` (defaulting to `Arc<dyn
CommonsStoreBackend>`), so zero changes needed there.

### Step 4 — Update `server.rs` construction

```rust
// BEFORE (lines 692-701):
warn!("CommonsManager running in-memory-only mode ...");
let commons_manager = Arc::new(CommonsManager::new());

// AFTER:
let commons_manager: Arc<CommonsManager> = if let Some(ref data_dir) = self.data_dir {
    let commons_path = data_dir.join("commons.sled");
    info!("CommonsManager: opening sled store at {:?}", commons_path);
    Arc::new(CommonsManager::with_sled_path(&commons_path)
        .context("Failed to open commons sled store")?)
} else {
    warn!("CommonsManager: no data_dir, using in-memory (state will not survive restart)");
    Arc::new(CommonsManager::new())
};
```

Remove the `warn!()` from the unconditional in-memory path — it moves into the
`else` branch above.

### Step 5 — Remove the `impl CommonsManager<SledCommonsStore>` block

The `with_sled_path` / `with_sled_temporary` methods move to the new
`CommonsManager<Arc<dyn CommonsStoreBackend>>` impl. The old
`impl CommonsManager<SledCommonsStore>` block can be removed or kept with a
deprecation note.

### Step 6 — Update `EnrollmentStore` if needed

`EnrollmentStore` holds `Option<Arc<CommonsManager>>`. With the new default type
parameter, `Arc<CommonsManager>` = `Arc<CommonsManager<Arc<dyn CommonsStoreBackend>>>`.
This should compile without changes. Verify.

---

## Out of Scope for This Tranche

- `RevocationRegistry` persistence — already in-memory by design, documented separately
- `CommonsHandle` actor pattern (daemon-backed mode with gossip sync) — the TODO
  comment in server.rs mentions this, but it is a much larger scope. Basic persistence
  (surviving restarts) does NOT require an actor handle. `SledCommonsStore` alone is
  sufficient.
- Migrating existing in-memory state at server startup (no migration needed — fresh
  sled db starts empty, same as today's in-memory start)
- Cross-node gossip synchronization of commons state

---

## Risks and Traps

### Sled lock conflict
If `data_dir` is the same as the path used for the main `gateway_store` sled db,
the sled lock will conflict. **The commons store MUST use a distinct path:**
`data_dir.join("commons.sled")` (not `gateway_store`). Verify no path collision
with `gateway_store` or `entity_audit_store`.

### `CommonsStore<Arc<dyn CommonsStoreBackend>>` double-Arc
`CommonsStore<S>` holds `Arc<S>`. With `S = Arc<dyn CommonsStoreBackend>`, the
field is `Arc<Arc<dyn CommonsStoreBackend>>`. This is valid but slightly wasteful.
Acceptable for this tranche. If it bothers you: refactor `CommonsStore` to hold
`Arc<dyn CommonsStoreBackend>` directly and remove its generic param too.

### Existing tests using `CommonsManager::new()`
Tests in `commons_integration.rs` and inline tests in `commons_store.rs` use
`CommonsManager::new()`. With the new impl, these still compile and still get an
in-memory backend — behavior unchanged. Run them to confirm.

### LRU cache and persistence
The LRU cache in `CommonsStore` is in-memory and warm-starts empty on reopen.
This is correct — the cache fills from sled on first access. No issue.

### `CommonsManager<SledCommonsStore>` impl block removal
The old specific impl block (`impl CommonsManager<SledCommonsStore>`) has `flush()`
and `size_on_disk()` methods. These must either:
1. Move to `CommonsManager<Arc<dyn CommonsStoreBackend>>` (using downcast or adding
   `flush()` to the `CommonsStoreBackend` trait)
2. Or be dropped if not called anywhere outside tests

Check callers of `flush()` before removing.

---

## Implementation Order

1. `commons_store.rs` — Add `impl CommonsStoreBackend for Arc<dyn CommonsStoreBackend>`
2. `commons_mgr.rs` — Change default type param; add new `CommonsManager<Arc<dyn CommonsStoreBackend>>` impl block with `new()`, `with_sled_path()`, `with_sled_temporary()`; handle `flush()` migration
3. Compile check: `cargo check -p icn-gateway` — expect errors in `CommonsManager<SledCommonsStore>` specific sites
4. Fix any type errors in `commons_mgr.rs` and callers
5. `server.rs` — Update `CommonsManager` construction with `data_dir` branch; remove old `warn!`
6. Compile check: `cargo check -p icn-gateway` — should be clean
7. Write persistence proof tests (see testing plan)
8. Full verification: `cargo test -p icn-gateway`
9. fmt/clippy

---

## File Inventory

| File | Change | Lines |
|------|--------|-------|
| `crates/icn-gateway/src/commons_store.rs` | Add blanket impl | ~20 new lines |
| `crates/icn-gateway/src/commons_mgr.rs` | New default type param + constructors | ~30 changed |
| `crates/icn-gateway/src/server.rs` | data_dir branch for CommonsManager | ~15 changed |
| `crates/icn-gateway/tests/commons_integration.rs` | Add sled persistence test | ~60 new lines |

Handler files (`api/charter/mod.rs`, `api/commons/*.rs`, etc.): **zero changes** if
default type param approach is used.
