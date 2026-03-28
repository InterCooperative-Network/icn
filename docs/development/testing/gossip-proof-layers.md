---
Status: layers 1-4 complete
Canonical: no
Last Reviewed: 2026-03-28
---

# Gossip Proof Layers

## Architecture: gossip persistence is NOT sled-based

Unlike ledger and governance, gossip state is persisted via the `icn-snapshot`
JSON file mechanism:

- Write path: `GossipActor::export_state()` → `StateSnapshot { gossip_state }` →
  `save_snapshot(&snapshot, data_dir)` → atomic JSON file on disk
- Read path: `load_snapshot(data_dir)` → `GossipActor::restore_state(state)`

**What is persisted:**
- Vector clock (causal ordering continuity across restart)
- Topic metadata (name, ACL, scope, max_entries)
- Topic subscriptions (which DIDs are subscribed to which topics)

**What is NOT persisted by design:**
- Gossip entries — they are re-fetched from peers via anti-entropy after restart

---

## Layer 1 — GossipActor State Snapshot Persistence ✅

**What it proves:** Topic metadata, topic subscriptions, and the vector clock
written through `GossipActor::export_state()` → `save_snapshot()` survive a
drop-and-reload boundary with exact field values when restored via
`restore_state()` into a fresh actor.

**Artifact:** `crates/icn-gossip/tests/gossip_persistence.rs` →
`test_gossip_state_survives_export_snapshot_restore`

**Run:**
```bash
cargo test -p icn-gossip --test gossip_persistence
```

**What is asserted:**
- Topic name survives snapshot round-trip (exact string match)
- Subscriber DID survives in the topic's subscription list
- Vector clock count for own_did is exactly 1 after one publish

**Key notes:**
- No oracle or keypair needed — exercises the pure state serialization path
- `publish()` increments the clock without requiring a send_callback
- `restore_state()` restores subscriptions without re-running ACL checks
  (trusts persisted state, same as production path)

---

---

## Layer 2 — GossipHandle (Arc<RwLock<GossipActor>>) Snapshot Persistence ✅

**What it proves:** Topic metadata, topic subscriptions, and the vector clock
written and exported through the production handle path
(`GossipActor::spawn()` → `Arc<RwLock<GossipActor>>`) survive the same
export/snapshot/restore cycle as Layer 1. This is the real access pattern used
by the supervisor for all gossip mutations and shutdown export.

**Artifact:** `crates/icn-gossip/tests/gossip_persistence.rs` →
`test_gossip_handle_state_survives_snapshot_restore`

**Run:**
```bash
cargo test -p icn-gossip --test gossip_persistence
```

**Production path exercised:**
- Mutations: `gossip_handle.write().await.create_topic()` / `.publish()` / `.subscribe()`
- Export: `gossip_handle.read().await.export_state()` (exactly as `supervisor/shutdown.rs`)
- Persist: `save_snapshot(&snapshot, &data_dir)`
- Reload/restore: `load_snapshot()` → `restore_state()` into fresh actor

**What is asserted:**
- Same three invariants as Layer 1: topic name, subscriber DID, vector clock count
- Proves no divergence between the "direct struct test path" and the "production handle path"

---

---

## Layer 3 — Same-Runtime Handle Drop + Recreate Proof ✅

**What it proves:** Gossip coordination state (vector clock, topic metadata,
subscriptions) survives a same-runtime lifecycle boundary: the original
`GossipHandle` is fully dropped (all `Arc` refs released, actor memory
reclaimed), the snapshot on disk is the only bridge, and a brand-new
`GossipHandle` created in the same Tokio runtime restores exact state.

This is the real daemon restart cycle: shutdown drops the handle, a fresh
`GossipActor::spawn()` is created at next boot, and `restore_gossip_snapshot`
calls `gossip_handle.write().await.restore_state()` before accepting work.

**What is NOT proven by this layer:**
- Entry durability: entries are intentionally not persisted — not a gap
- Cross-process memory isolation: requires subprocess (Layer 4)

**Artifact:** `crates/icn-gossip/tests/gossip_persistence.rs` →
`test_gossip_handle_survives_same_runtime_drop_and_recreate`

**Run:**
```bash
cargo test -p icn-gossip --test gossip_persistence
```

**Lifecycle boundary exercised:**
1. `GossipHandle` mutated and exported (production path)
2. `save_snapshot()` persists to disk
3. All `Arc` refs dropped — actor memory reclaimed
4. `load_snapshot()` reads from disk (no in-memory remnant)
5. `GossipActor::spawn()` creates a completely empty fresh actor
6. `handle2.write().await.restore_state()` — exact supervisor boot path
7. Assertions via `handle2.read().await` — no hidden continuity

**What is asserted:**
- Topic name survives the lifecycle boundary
- Subscriber DID survives
- Vector clock count is exactly preserved

---

---

## Layer 4 — Cross-Process Restart Proof ✅

**What it proves:** Gossip coordination state (vector clock, topic metadata,
subscriptions) written through `GossipHandle` in one OS process is readable
in a completely fresh OS process. No shared memory. No shared runtime. True
process-boundary restart.

**Persistence scope confirmed:** Coordination state persists (clock,
topics, subscriptions). Gossip entries do NOT persist — by design, they are
re-fetched from peers via anti-entropy after restart.

**Implementation:**
- Helper binary: `crates/icn-gossip/src/bin/gossip_restart_helper.rs`
  - `write <data_dir>` — builds state through `GossipHandle`, persists
    snapshot, prints `own_did,subscriber_did` to stdout, exits 0.
  - `read <data_dir> <own_did> <subscriber_did>` — loads snapshot, restores
    into fresh `GossipActor`, asserts exact invariants, exits 0 or 1.
- Integration test: `crates/icn-gossip/tests/gossip_persistence.rs` →
  `test_gossip_state_survives_cross_process_restart`

**Key difference from ledger/governance restart helpers:**
- No sled file lock to release — JSON snapshot is written atomically, closed
  on drop. No sled between processes.
- `new_current_thread()` runtime sufficient — gossip has no `block_in_place`.

**Artifact:** `crates/icn-gossip/tests/gossip_persistence.rs` →
`test_gossip_state_survives_cross_process_restart`

**Run:**
```bash
cargo test -p icn-gossip --test gossip_persistence
```

**What is asserted (in read subprocess):**
- Topic name `"layer-4-cross-process-proof"` survives the OS process boundary
- Subscriber DID (passed from write subprocess via stdout) survives
- Vector clock for `own_did` is exactly 1 after one publish

---

## What Is NOT Proven (by design)

| Gap | Why it matters | Status |
|-----|---------------|--------|
| Gossip entry re-gossip after restart | Entries intentionally not persisted | By design — anti-entropy handles this |
| Anti-entropy resync after restart | Requires multi-node test | Multi-node integration test |
| Snapshot checksum corruption detection | Already tested in `icn-snapshot` unit tests | Already covered |

---

## Run Full Layer 1–4 Suite

```bash
cargo test -p icn-gossip --test gossip_persistence
```

All four proof layers are in a single test file. All use the same `icn-snapshot` JSON mechanism.

---

## Comparison with Ledger/Governance Proof Stacks

| Layer | Governance | Ledger | Gossip |
|-------|-----------|--------|--------|
| 1 — Direct struct write + reopen | ✅ | ✅ | ✅ `gossip_persistence.rs` |
| 2 — Actor/handle-backed path | ✅ | ✅ | ✅ `gossip_persistence.rs` |
| 3 — Same-runtime close+reopen | ✅ | ✅ | ✅ `gossip_persistence.rs` |
| 4 — Cross-process restart | ✅ | ✅ | ✅ `gossip_persistence.rs` |

Gossip is now fully verified across all four proof layers. Governance and ledger have full parity.
