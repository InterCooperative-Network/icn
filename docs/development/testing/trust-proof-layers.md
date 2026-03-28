---
Status: layers 1-3 complete
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

---

## Layer 2 — Facade-backed Sled Write Proof ✅

**What it proves:** A `TrustEdge` written through the full production abstraction
stack (`TrustGraphFacade` → `MultiTrustGraph` → `TypedTrustGraph` →
`TrustGraph::new_with_prefix(store, did, "trust/social")`) survives a sled
drop-and-reopen boundary with exact field values.

Also proves **graph-type namespace isolation**: a Social edge is not retrievable
via the Economic graph prefix (`trust/economic/...`), confirming that the three
typed graphs are truly isolated in sled storage.

**Write path confirmed:**
```
facade.add_edge(edge)                               // edge.graph_type == Social
→ multi.add_edge_to(Social, edge)
→ typed_graph.inner.add_edge(edge)
→ store.put("trust/social/edges/{src}:{tgt}", json)
```

**Artifact:** `crates/icn-trust/tests/trust_persistence.rs` →
`test_trust_edge_survives_facade_sled_drop_and_reopen`

**Run:**
```bash
cargo test -p icn-trust --test trust_persistence
```

**What is asserted:**
- Source DID survives the full facade round-trip (exact match).
- Target DID survives exact round-trip.
- Score survives exact round-trip (within f64 epsilon).
- Social edge is absent from the Economic graph namespace (prefix isolation).

---

---

## Layer 3 — Same-Runtime Facade Drop + Recreate Proof ✅

**What it proves:** Trust state survives a same-runtime lifecycle boundary:
the original `TrustGraphFacade` and `Arc<SledStore>` are fully dropped (all `Arc`
refs released, sled file lock returned to the OS), and a brand-new
`TrustGraphFacade` constructed in the same process restores exact state from disk.

Trust has **no background scheduler and no `JoinHandle`** — dropping the `Arc`
is the complete shutdown. No `shutdown().await` is needed (unlike governance).
The sled file on disk is the sole bridge between Phase 1 and Phase 2.

**Artifact:** `crates/icn-trust/tests/trust_persistence.rs` →
`test_trust_facade_survives_same_runtime_drop_and_recreate`

**Run:**
```bash
cargo test -p icn-trust --test trust_persistence
```

**What is asserted:**
- Source DID survives the lifecycle boundary (exact match).
- Target DID survives (exact match).
- Score survives (exact, within f64 epsilon).
- Graph-type isolation holds through the fresh facade (Social absent from Economic).

---

## What Is NOT Yet Proven

| Gap | Layer | Next step |
|-----|-------|-----------|
| Cross-process restart | L4 | Helper binary + subprocess test |
| Evidence fields survive round-trip | Future | Add `evidence: Vec<TrustEvidence>` assertions |

---

## Comparison with Other Subsystems

| Layer | Governance | Ledger | Gossip | Trust |
|-------|-----------|--------|--------|-------|
| 1 — Direct persistence | ✅ | ✅ | ✅ | ✅ |
| 2 — Production path | ✅ | ✅ | ✅ | ✅ |
| 3 — Same-runtime lifecycle | ✅ | ✅ | ✅ | ✅ |
| 4 — Cross-process restart | ✅ | ✅ | ✅ | ⏳ |
