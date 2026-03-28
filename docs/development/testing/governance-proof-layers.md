---
Status: operational
Canonical: no
Last Reviewed: 2026-03-28
---

# Governance Proof Layers

Governance is the first ICN subsystem with all four verification layers.
This document describes what each layer proves, which artifacts provide it,
what is not proven, and how other subsystems should follow the same pattern.

---

## Four-Layer Proof Stack

### Layer 1 — HTTP Lifecycle Proof

**What it proves:** The full governance proposal lifecycle (Draft → Open → Accepted)
executes correctly end-to-end through the live HTTP API with real Ed25519 auth signatures.
Gateway routing, JWT middleware, and handler logic are all exercised.

**Artifact:** `crates/icn-gateway/tests/governance_proof.rs`
**Run:**
```bash
cargo test -p icn-gateway --test governance_proof --features sled-storage
```
**Storage:** In-memory (`GovernanceManager::new()`). This layer proves the live HTTP
path, not persistence.

---

### Layer 2 — Actor-backed Sled Write Proof

**What it proves:** State written through `GovernanceActor` + `GovernanceManager::with_handle()`
lands in a real sled database (not in-memory HashMaps). A fresh `SledGovernanceStateStore`
reading from the same `Arc<SledStore>` can retrieve the written state.

**Artifact:** `apps/governance/tests/persistence_proof.rs` → `test_governance_restart_persistence` (Phase 1 write + pre-restart assertion)
**Run:**
```bash
cargo test -p icn-governance-actor --test persistence_proof
```

---

### Layer 3 — Same-Runtime Close-and-Reopen Proof

**What it proves:** After `GovernanceHandle::shutdown().await` (which deterministically
awaits the scheduler task's `JoinHandle`), the sled file lock is fully released.
The same path can be reopened with `SledStore::open`, and a fresh `GovernanceActor`
reads back the previously written state through the standard manager API.

**Artifact:** `apps/governance/tests/persistence_proof.rs` → `test_governance_restart_persistence` (Phase 2 reopen)
**Run:** Same command as Layer 2.

**Key mechanism:** `GovernanceHandle::shutdown()` is async. It sends a `oneshot` shutdown
signal to the background scheduler (biased select — checked first), then awaits the task's
`JoinHandle`. This is deterministic — no sleep or yield_now required.

---

### Layer 4 — Cross-Process Restart Proof

**What it proves:** Governance state survives a true OS-level process boundary.
Process A writes, exits. Process B opens the same sled path in a fresh `GovernanceActor`,
reads domain and proposal through `GovernanceManager`, asserts `ProposalState::Accepted`.

**Artifacts:**
- `apps/governance/tests/persistence_proof.rs` → `test_governance_cross_process_restart`
- `apps/governance/src/bin/governance_restart_helper.rs` (write + read helper binary)

**Run:**
```bash
cargo test -p icn-governance-actor --test persistence_proof -- test_governance_cross_process_restart
```

The test uses `env!("CARGO_BIN_EXE_governance_restart_helper")` — Cargo sets this to the
compiled binary path when building test binaries in the same package.

---

## Run the Full Proof Suite

```bash
# All four layers in two commands:
cargo test -p icn-governance-actor --test persistence_proof -- --nocapture
cargo test -p icn-gateway --test governance_proof --features sled-storage
```

Expected output:
```
running 2 tests
test test_governance_cross_process_restart ... ok
test test_governance_restart_persistence ... ok

running 3 tests
test test_auth_rejects_invalid_signature ... ok
test test_governance_endpoints_require_auth ... ok
test test_governance_proposal_full_lifecycle_with_real_auth ... ok
```

---

## What Is NOT Proven

| Gap | Why it matters | Suggested path |
|-----|---------------|----------------|
| SIGKILL durability | Only clean exit is tested; sled WAL recovery is not exercised | spawn helper, kill -9 mid-write, restart, read |
| Concurrent write safety | No multi-actor or multi-writer test | stress test with N parallel proposals |
| Schema migration | No test that old sled data is readable after type changes | versioned serde roundtrip test |
| Gossip sync across restart | Callbacks re-register but no in-flight message delivery tested | multi-node integration test |
| Cross-coop / federation governance | Federation topic path not exercised | separate federation persistence test |

---

## Pattern for Other Subsystems

To make another ICN subsystem "verified" at the same level, follow this order:

1. **HTTP lifecycle proof** — write an integration test against the real HTTP handlers
   using `actix_web::test`. Use `GovernanceManager::new()` style (in-memory) first. This
   proves routing and handler correctness before storage is involved.

2. **Actor-backed write proof** — write a test that constructs the actor (not the
   standalone manager) and reads back state via a fresh store instance. Proves the actor
   writes to sled, not to in-memory maps.

3. **Same-runtime reopen proof** — add `shutdown().await` (JoinHandle pattern) to the
   actor, then reopen the same sled path and read back through a fresh actor. Proves the
   file lock is cleanly released.

4. **Cross-process restart proof** — add a `src/bin/<subsystem>_restart_helper.rs` binary
   with write/read modes. Invoke via `std::process::Command` in an integration test. Use
   `env!("CARGO_BIN_EXE_<name>")` for the binary path. Proves true OS-level durability.

### The shutdown pattern (copy this)

```rust
// In GovernanceHandle (or equivalent actor handle):
pub async fn shutdown(&self) {
    // 1. Send shutdown signal to the background task's biased select.
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

Background task uses `biased;` select with shutdown arm first:
```rust
tokio::select! {
    biased;
    _ = &mut shutdown_rx => { break; }
    _ = interval.tick() => { /* ... */ }
    // other arms...
}
```

---

## Relevant Source Files

| File | Role |
|------|------|
| `apps/governance/src/actor.rs` | `GovernanceHandle::shutdown()` — deterministic JoinHandle-based shutdown |
| `apps/governance/src/manager.rs` | `GovernanceManager::with_handle()` — actor-backed vs. standalone mode |
| `apps/governance/src/state_store.rs` | `SledGovernanceStateStore` — zero in-memory state, reads exclusively from sled |
| `apps/governance/tests/persistence_proof.rs` | Layers 2, 3, 4 |
| `apps/governance/src/bin/governance_restart_helper.rs` | Layer 4 helper binary (write + read phases) |
| `crates/icn-gateway/tests/governance_proof.rs` | Layer 1 |
