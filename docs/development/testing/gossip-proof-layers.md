---
Status: layers 1-2 complete
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

## What Is NOT Proven

| Gap | Why it matters | Next layer target |
|-----|---------------|-------------------|
| Same-runtime close+reopen via `GossipHandle` | Handle drop + re-create not yet proven | Layer 3 |
| Cross-process restart | Only same-process proven | Layer 4 |
| Gossip entry re-gossip after restart | Entries intentionally not persisted | Multi-node integration test |
| Anti-entropy resync after restart | Requires multi-node test | Multi-node integration test |
| Snapshot checksum corruption detection | Already tested in `icn-snapshot` unit tests | Already covered |

---

## Next Layer: Layer 3 — Same-Runtime Close+Reopen

**Target:** Prove that a `GossipHandle` can be dropped, a new
`GossipActor::spawn()` can be created in the same Tokio runtime, snapshot
loaded, and `restore_state()` called — without exiting the process.

**Pattern:** Mirror governance Layer 3 — drop all Arc references to the
handle, reload snapshot, restore into a fresh handle in the same test runtime.

---

## Comparison with Ledger/Governance Proof Stacks

| Layer | Governance | Ledger | Gossip |
|-------|-----------|--------|--------|
| 1 — Direct struct write + reopen | ✅ | ✅ | ✅ `gossip_persistence.rs` |
| 2 — Actor/handle-backed path | ✅ | ✅ | ✅ `gossip_persistence.rs` |
| 3 — Same-runtime close+reopen | ✅ | ✅ | ⏳ |
| 4 — Cross-process restart | ✅ | ✅ | ⏳ |
