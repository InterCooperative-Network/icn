---
Status: canonical
Last Reviewed: 2026-03-28
---

# ICN 4-Layer Verification Pattern

This document defines the standard verification ladder for ICN subsystems.
Any subsystem that writes persistent state should be verified at all four layers.

**Current status:**

| Subsystem | L1 | L2 | L3 | L4 |
|-----------|----|----|----|----|
| Governance | ✅ | ✅ | ✅ | ✅ |
| Ledger     | ✅ | ✅ | ✅ | ✅ |
| Gossip     | ✅ | ✅ | ✅ | ✅ |

---

## The Four Layers

### Layer 1 — Direct Persistence

**Prove:** State written through the lowest-level struct path survives a
drop-and-reopen boundary.

For sled-backed subsystems: write through the struct's storage adapter, drop the
struct (releases the sled file lock), reopen via `SledStore::open`, read back.

For snapshot-backed subsystems: write through the struct, call
`export_state()` → `save_snapshot()`, drop the struct, call `load_snapshot()` →
`restore_state()`, read back.

**What it proves:** The serialization path is correct end-to-end. No data lives
only in in-memory caches.

**Assertion type:** Exact field values on the read-back struct.

---

### Layer 2 — Production Handle Path

**Prove:** State written through the production access pattern (actor handle,
`Arc<RwLock<T>>`, etc.) produces the same persisted state as Layer 1.

This matters because the production path often goes through intermediate types
(`GossipHandle`, `GovernanceHandle`, store-backed wrapper structs) that could
diverge from the bare struct path.

**What it proves:** No divergence between "direct struct test path" and
"production handle path."

**Assertion type:** Same three invariants as Layer 1, reached via the handle
access pattern (write guard, read guard, etc.).

---

### Layer 3 — Same-Runtime Lifecycle Boundary

**Prove:** State survives a same-runtime lifecycle boundary: the original
handle is fully dropped (all `Arc` refs released, actor memory reclaimed),
the snapshot or sled file on disk is the only bridge, and a brand-new handle
created in the same Tokio runtime restores exact state.

For actors with background tasks (governance scheduler): call
`handle.shutdown().await` before dropping. This must be deterministic — no
`sleep`, no `yield_now`.

For handles without background tasks (gossip, ledger service): dropping the
`Arc` is sufficient.

**What it proves:** The shutdown → restart cycle is clean. File locks are
released. The disk artifact (sled db or JSON snapshot) is sufficient to
reconstruct state.

**Assertion type:** Same invariants as L1/L2, asserted through the fresh
handle's read accessor.

---

### Layer 4 — Cross-Process Boundary

**Prove:** State written in one OS process is readable in a completely fresh
OS process. No shared memory. No shared Tokio runtime. True process-boundary
restart.

**Implementation pattern:**

1. Add a helper binary `src/bin/<subsystem>_restart_helper.rs` with two modes:
   - `write <data_dir>` — builds state through the production path, persists,
     prints identifying tokens to stdout (e.g. hash, DID, CID), exits 0.
   - `read <data_dir> <token...>` — opens fresh storage, restores, asserts
     exact invariants, exits 0 or 1.

2. Write an integration test that:
   - Spawns the write subprocess
   - Asserts exit 0, parses stdout
   - Spawns the read subprocess with the parsed tokens
   - Asserts exit 0

3. Use `env!("CARGO_BIN_EXE_<name>")` for the binary path — Cargo sets this
   automatically when building test binaries in the same package.

**What it proves:** True OS-level durability. The disk representation is
self-contained and portable across processes.

**Assertion type:** Inside the read subprocess, exact field equality after
full deserialization from disk.

---

## Persistence Types

Two persistence mechanisms are used in ICN. They differ in how Layer 3 and 4
work:

### Sled-backed (governance, ledger)

- Storage: `SledStore` → sled B-tree database, one directory per node
- Lock: sled holds an exclusive file lock; ALL `Arc<SledStore>` clones must
  be dropped before reopening
- Layer 3: drop all `Arc` refs and await any background task's `JoinHandle`
  before reopening
- Layer 4: no explicit flush call needed; `drop(rt)` runs tokio cleanup which
  flushes sled's WAL
- Runtime note: `block_in_place` in sled paths requires `new_multi_thread()`
  runtime in helper binaries (not `new_current_thread()`)

### Snapshot-backed (gossip)

- Storage: `icn-snapshot` → atomic JSON file, one file per node
- Lock: no file lock; the JSON file is written atomically and closed on drop
- Layer 3: drop all `Arc` refs; no background task to await
- Layer 4: the JSON file is safe to read from a second process immediately
  after the first process writes it
- Runtime note: no `block_in_place`; `new_current_thread()` is sufficient

---

## Shutdown Pattern (for actors with background tasks)

```rust
pub async fn shutdown(&self) {
    // 1. Send shutdown signal to the background task.
    if let Ok(mut guard) = self.scheduler_shutdown.lock() {
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
    // 2. Take JoinHandle outside the lock — never hold sync Mutex across .await.
    let task = self.scheduler_task.lock().ok().and_then(|mut g| g.take());
    // 3. Await completion — deterministic, no sleep.
    if let Some(t) = task {
        let _ = t.await;
    }
}
```

Background task uses `biased;` select to check shutdown before other arms:

```rust
tokio::select! {
    biased;
    _ = &mut shutdown_rx => { break; }
    _ = interval.tick() => { /* ... */ }
}
```

---

## Helper Binary Pattern

Minimal structure for a Layer 4 helper binary:

```rust
// src/bin/<subsystem>_restart_helper.rs
fn main() {
    let rt = tokio::runtime::Builder::new_current_thread() // or new_multi_thread()
        .enable_all().build().expect("runtime");

    let args: Vec<String> = std::env::args().collect();
    let exit_code = match args.get(1).map(String::as_str) {
        Some("write") => {
            let data_dir = PathBuf::from(args.get(2).expect("missing data_dir"));
            rt.block_on(run_write(data_dir))
        }
        Some("read") => {
            let data_dir = PathBuf::from(args.get(2).expect("missing data_dir"));
            // parse remaining args...
            run_read(data_dir, ...)
        }
        _ => { eprintln!("usage: helper <write|read> <data_dir> [...]"); 1 }
    };

    drop(rt); // flush sled WAL before exit
    std::process::exit(exit_code);
}
```

Key rules:
- `drop(rt)` before `process::exit` — ensures sled flush and async cleanup
- Print identifying tokens to stdout only (not logging output)
- Exit 1 with `eprintln!` on any assertion failure in the read phase
- `#![allow(clippy::expect_used, clippy::unwrap_used)]` — test binary only

---

## Integration Test Pattern (Layer 4)

Use `icn_testkit::subprocess::run_subprocess` to avoid repeated boilerplate:

```rust
// In icn-testkit, `run_subprocess(binary, args)` asserts success and
// returns trimmed stdout. See `crates/icn-testkit/src/subprocess.rs`.

let helper = env!("CARGO_BIN_EXE_<subsystem>_restart_helper");

let write_stdout = run_subprocess(helper, &["write", data_dir]);
let (token_a, token_b) = write_stdout.split_once(',')
    .expect("write stdout must be 'token_a,token_b'");

run_subprocess(helper, &["read", data_dir, token_a, token_b]);
```

---

## What Counts as "Verified"

A subsystem is **verified** when:
- All four layers have passing tests
- Each test asserts exact field values (not just "something was returned")
- The proof-layers doc for the subsystem is marked `Status: layers 1-4 complete`
- The verification-pattern comparison table above is updated

---

## Applying This to a New Subsystem

Work in this order:

1. **Pick the persistence type.** Is it sled-backed or snapshot-backed? This
   determines which Layer 3/4 patterns apply.

2. **Write Layer 1.** Direct struct write → drop → reopen → exact assertions.
   No actors, no handles. If this fails, fix the storage layer first.

3. **Write Layer 2.** Repeat via the production handle path. If it diverges
   from Layer 1, there's a caching or write-path bug.

4. **Write Layer 3.** Add shutdown to the handle (if needed), drop, recreate
   in same runtime, assert. If the file lock isn't released cleanly, Layer 3
   will deadlock or error.

5. **Write Layer 4.** Add the helper binary, write the subprocess test. This
   is mechanical once Layer 3 passes.

6. **Update docs.** Mark the subsystem's proof-layers doc `layers 1-4 complete`
   and add it to the table above.

---

## Reference Implementations

| Subsystem | Proof layers doc | Key test files |
|-----------|-----------------|----------------|
| Governance | [governance-proof-layers.md](governance-proof-layers.md) | `apps/governance/tests/persistence_proof.rs`, `crates/icn-gateway/tests/governance_proof.rs` |
| Ledger | [ledger-proof-layers.md](ledger-proof-layers.md) | `crates/icn-ledger/tests/ledger_persistence.rs`, `apps/ledger/tests/actor_persistence_proof.rs`, `crates/icn-core/tests/ledger_service_persistence.rs` |
| Gossip | [gossip-proof-layers.md](gossip-proof-layers.md) | `crates/icn-gossip/tests/gossip_persistence.rs` |
