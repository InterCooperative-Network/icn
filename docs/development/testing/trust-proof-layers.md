---
Status: layer 1 complete
Last Reviewed: 2026-03-28
---

# Trust Proof Layers

## Architecture: trust persistence is sled-backed

`TrustGraph` stores edges in `Arc<dyn Store>` (backed by `SledStore`).

- Write path: `TrustGraph::add_edge()` → `store.put("{prefix}/edges/{source}:{target}", serde_json(edge))`
- Read path: `TrustGraph::get_edge()` → `store.get(key)` → `serde_json::from_slice`
- Storage prefix: `"trust"` by default; typed graphs use `"trust/social"`, `"trust/economic"`, `"trust/technical"`

**What is persisted:**
- `TrustEdge` — source DID, target DID, score (f64), graph_type, evidence, created_at, expires_at

**In-memory only (not persisted):**
- `TrustCache` (LRU score cache with TTL) — rebuilt from sled on first access after restart
- `ReachabilityFilter` (Bloom filter) — rebuilt from edges on startup

**NOTE:** All existing tests use `SledStore::temporary()` — an ephemeral in-memory
store. None of them test durability. This file closes that gap.

---

## Layer 1 — Direct Sled Write Proof ✅

**What it proves:** A `TrustEdge` written through `TrustGraph::add_edge()` with
a real `SledStore::open(path)` (not `temporary()`) survives a sled drop-and-reopen
boundary with exact field values.

After the drop:
- The sled file lock is released.
- The `TrustCache` LRU is gone — the fresh graph has no cached values.
- `get_edge()` reads exclusively from sled.

**Artifact:** `crates/icn-trust/tests/trust_persistence.rs` →
`test_trust_edge_survives_sled_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-trust --test trust_persistence
```

**What is asserted:**
- Edge is present after reopen (not silently lost).
- Source DID survives exact round-trip.
- Target DID survives exact round-trip.
- Score survives exact round-trip (within f64 epsilon).

**Key note:** `TrustEdge::new()` sets `expires_at: None` — no expiry to manage.
The `is_expired()` check in `get_edge()` is a no-op for these edges.

---

## What Is NOT Yet Proven

| Gap | Layer | Next step |
|-----|-------|-----------|
| Typed graph path (`trust/social`, `trust/economic`, `trust/technical`) | L2 | `TrustGraphFacade::add_edge()` write → drop → reopen → `get_edge_from()` |
| Production handle path (`Arc<parking_lot::RwLock<TrustGraph>>` in supervisor) | L2 | Verify via supervisor's actual handle type |
| Same-runtime lifecycle boundary | L3 | Drop handle, reopen, assert |
| Cross-process restart | L4 | Helper binary + subprocess test |
| Evidence fields survive round-trip | L1 extension | Add `evidence: Vec<TrustEvidence>` to Layer 1 assertions |

---

## Layer 2 — Next Step

The production trust path uses `TrustGraphFacade` wrapping `MultiTrustGraph`
wrapping three `TypedTrustGraph` instances, each with a prefixed `TrustGraph`.

Layer 2 target: a `TrustEdge` written through `TrustGraphFacade::add_edge()`
survives the same sled drop-and-reopen as Layer 1, proving the typed graph path
(with storage prefix `"trust/social"`) writes to sled correctly.

Artifact location: `crates/icn-trust/tests/trust_persistence.rs` (extend file).

---

## Comparison with Other Subsystems

| Layer | Governance | Ledger | Gossip | Trust |
|-------|-----------|--------|--------|-------|
| 1 — Direct persistence | ✅ | ✅ | ✅ | ✅ |
| 2 — Production path | ✅ | ✅ | ✅ | ⏳ |
| 3 — Same-runtime lifecycle | ✅ | ✅ | ✅ | ⏳ |
| 4 — Cross-process restart | ✅ | ✅ | ✅ | ⏳ |
