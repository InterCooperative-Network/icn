---
Created: 2026-03-28
For: next session
Branch: feat/commons-persistence (new branch off main)
---

# Start Here Tomorrow — CommonsManager Persistence

## One-paragraph summary

`CommonsManager` in `icn-gateway` defaults to an in-memory backend. All
commons/personhood/charter/enrollment state is lost on gateway restart. The sled
storage infrastructure is fully built (`SledCommonsStore`, `CommonsStoreBackend`
trait, `CommonsStore<S>` caching wrapper) — the gap is only in wiring. The fix is:
add a blanket `impl CommonsStoreBackend for Arc<dyn CommonsStoreBackend>`, change
`CommonsManager`'s default type parameter from `InMemoryCommonsStore` to
`Arc<dyn CommonsStoreBackend>`, update constructors, and add a `data_dir` branch in
`server.rs`. Handler files do not change. Add drop-and-reopen persistence proof
tests. This is a targeted ~100-line change across 4 files.

---

## Read these first (in order)

1. `docs/development/commons-persistence-design.md` — architecture, why it's this
   shape, risks, out-of-scope items
2. `docs/development/commons-persistence-checklist.md` — step-by-step execution
3. `docs/development/commons-persistence-testing.md` — test specs

## Key source files

| File | Why |
|------|-----|
| `icn/crates/icn-gateway/src/commons_store.rs:56` | `CommonsStoreBackend` trait — add blanket impl here |
| `icn/crates/icn-gateway/src/commons_mgr.rs:40` | `CommonsManager<S>` struct — change default type param here |
| `icn/crates/icn-gateway/src/server.rs:692` | The `CommonsManager::new()` call to replace |
| `icn/crates/icn-gateway/tests/commons_integration.rs` | Add drop-and-reopen tests here |

---

## Branch to create

```bash
git checkout main
git pull
git checkout -b feat/commons-persistence
```

---

## First concrete step

Open `icn/crates/icn-gateway/src/commons_store.rs`. After the `CommonsStoreBackend`
trait definition (line ~72), add the blanket impl for `Arc<dyn CommonsStoreBackend>`
(see design doc). Run `cargo check -p icn-gateway`. Then proceed to `commons_mgr.rs`.

---

## Verification sequence after coding

```bash
cd /home/ubuntu/projects/icn/icn
cargo check -p icn-gateway                                    # fast check
cargo test -p icn-gateway --lib -- commons_store              # inline tests
cargo test -p icn-gateway                                     # full integration
cargo fmt --all --check
cargo clippy -p icn-gateway --all-targets -- -D warnings
```

---

## Suggested PR framing

**Branch:** `feat/commons-persistence`
**Title:** `feat(gateway): wire CommonsManager to sled-backed storage`

**Body bullets:**
- Replace `CommonsManager<InMemoryCommonsStore>` default with type-erased
  `Arc<dyn CommonsStoreBackend>` backend
- `server.rs` opens `commons.sled` when `data_dir` is configured; falls back to
  in-memory with warning when not set
- Add drop-and-reopen persistence proof tests for anchor, charter, and holder state
- Handler files unchanged — zero API surface change
- `RevocationRegistry` persistence and CommonsHandle actor pattern are explicitly
  out of scope (noted in design doc)

---

## Watch out for

- **Sled path collision**: `commons.sled` must not be the same path as `gateway_store`
  or any other sled db opened in server.rs
- **`flush()` method**: may exist on `impl CommonsManager<SledCommonsStore>` — check
  if called before removing that impl block
- **Double-Arc**: `CommonsStore<Arc<dyn CommonsStoreBackend>>` wraps `Arc<Arc<dyn ...>>`.
  Valid but slightly wasteful. Acceptable for this tranche.
- **Type errors**: after changing the default param, `CommonsManager::new()` callers
  may need updating if they explicitly typed `CommonsManager<InMemoryCommonsStore>`
  anywhere outside the impl block (grep: `CommonsManager<InMemoryCommonsStore>`)
