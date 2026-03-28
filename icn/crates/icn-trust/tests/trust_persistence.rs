//! Trust graph persistence proof — Layers 1, 2, and 3.
//!
//! ## Architecture note
//!
//! `TrustGraph` uses `Arc<dyn Store>` backed by `SledStore` for edge persistence.
//! The write path is:
//!
//!   `TrustGraph::add_edge()` → `store.put("{prefix}/edges/{source}:{target}", json)`
//!
//! The read path is:
//!   `TrustGraph::get_edge()` → `store.get(key)` → `serde_json::from_slice`
//!
//! Storage prefixes per graph type:
//!   - Social:    `trust/social/edges/{src}:{tgt}`
//!   - Economic:  `trust/economic/edges/{src}:{tgt}`
//!   - Technical: `trust/technical/edges/{src}:{tgt}`
//!
//! **All existing tests use `SledStore::temporary()`** — an ephemeral in-memory
//! store that does not test durability. This file proves real sled persistence.
//!
//! ## Layer 1 — what it proves
//!
//! A `TrustEdge` written through the direct `TrustGraph` path (the narrowest
//! canonical write path) survives a real sled drop-and-reopen boundary.
//!
//! ## Layer 2 — what it proves
//!
//! A `TrustEdge` written through the full production abstraction stack
//! (`TrustGraphFacade` → `MultiTrustGraph` → `TypedTrustGraph` → `TrustGraph`)
//! survives the same sled drop-and-reopen boundary, proving:
//! - The facade write path reaches sled (no silent in-memory-only writes).
//! - The prefixed key namespace (`trust/social/`) is correct end-to-end.
//! - Graph-type isolation: a Social edge is NOT visible in the Economic namespace.
//!
//! ## Layer 3 — what it proves
//!
//! Same lifecycle boundary as Layer 2, but the original facade and store are
//! fully dropped in a scoped block (all `Arc` refs released, sled file lock
//! returned) before Phase 2 begins. Phase 2 runs in the same process with no
//! shared memory. The disk is the only bridge.
//!
//! Trust has no background scheduler — dropping the `Arc` is the full shutdown.
//! No `shutdown().await` or `JoinHandle` is needed.
//!
//! ## What is NOT proven
//!
//! - Cross-process restart (Layer 4)
//! - Evidence fields (additional invariants for future layers)

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::KeyPair;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustGraphFacade, TrustGraphType, TrustScore};
use std::sync::Arc;

/// Layer 1 — TrustGraph direct sled write persistence proof.
///
/// Proves that a `TrustEdge` written through `TrustGraph::add_edge()` with a
/// real `SledStore::open()` path survives a drop-and-reopen boundary.
///
/// After the drop:
/// - The sled file lock is released.
/// - The `TrustCache` (LRU) is cleared — the fresh graph has no cached values.
/// - `get_edge()` reads exclusively from sled via `store.get()`.
///
/// Invariants asserted:
/// 1. The edge is present after reopen (not silently lost).
/// 2. Source and target DIDs survive exact round-trip.
/// 3. Score survives exact round-trip (within f64 tolerance).
#[test]
fn test_trust_edge_survives_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("trust.sled");

    let alice = KeyPair::generate().expect("alice KeyPair").did().clone();
    let bob = KeyPair::generate().expect("bob KeyPair").did().clone();
    let score_value = 0.75_f64;

    // ── Phase 1: write through the direct TrustGraph path ────────────────────
    {
        let store = Arc::new(SledStore::open(&sled_path).expect("open sled"));
        let mut graph = TrustGraph::new(store, alice.clone());

        let edge = TrustEdge::new(
            alice.clone(),
            bob.clone(),
            TrustScore::unchecked(score_value),
        );
        graph.add_edge(edge).expect("add_edge");

        // store + graph drop here — sled file lock released.
    }

    // ── Phase 2: fresh TrustGraph, no shared memory ──────────────────────────
    let store2 = Arc::new(SledStore::open(&sled_path).expect("reopen sled"));
    let graph2 = TrustGraph::new(store2, alice.clone());

    let retrieved = graph2
        .get_edge(&alice, &bob)
        .expect("get_edge")
        .expect("edge must be present after sled reopen");

    // 1. Source DID survives exact round-trip.
    assert_eq!(
        retrieved.source, alice,
        "source DID must survive sled drop-and-reopen"
    );

    // 2. Target DID survives exact round-trip.
    assert_eq!(
        retrieved.target, bob,
        "target DID must survive sled drop-and-reopen"
    );

    // 3. Score survives exact round-trip.
    assert!(
        (retrieved.score.value() - score_value).abs() < 1e-9,
        "score must survive sled drop-and-reopen: expected {score_value}, got {}",
        retrieved.score.value()
    );
}

/// Layer 2 — TrustGraphFacade (production abstraction stack) persistence proof.
///
/// Proves that a `TrustEdge` written through the full production path
/// (`TrustGraphFacade` → `MultiTrustGraph` → `TypedTrustGraph` →
/// `TrustGraph::new_with_prefix(store, did, "trust/social")`)
/// survives a sled drop-and-reopen boundary with exact field values.
///
/// Also proves graph-type namespace isolation: an edge written to the Social
/// graph is NOT retrievable via the Economic graph's namespace prefix.
///
/// Write path:
///   `facade.add_edge(edge)` (edge.graph_type == Social)
///   → `multi.add_edge_to(Social, edge)`
///   → `typed.inner.add_edge(edge)`
///   → `store.put("trust/social/edges/{src}:{tgt}", json)`
///
/// Read path after reopen:
///   `facade2.get_edge(&src, &tgt)` (defaults to Social)
///   → `multi.social().get_edge(&src, &tgt)`
///   → `store.get("trust/social/edges/{src}:{tgt}")`
///
/// Invariants asserted:
/// 1. Source DID survives the facade write → sled → facade read round-trip.
/// 2. Target DID survives exact round-trip.
/// 3. Score survives exact round-trip (within f64 tolerance).
/// 4. Social edge is NOT present in the Economic graph (prefix isolation).
#[test]
fn test_trust_edge_survives_facade_sled_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("trust-facade.sled");

    let alice = KeyPair::generate().expect("alice KeyPair").did().clone();
    let bob = KeyPair::generate().expect("bob KeyPair").did().clone();
    let score_value = 0.65_f64;

    // ── Phase 1: write through the full facade path ──────────────────────────
    {
        let store = Arc::new(SledStore::open(&sled_path).expect("open sled"));
        let mut facade = TrustGraphFacade::new(store, alice.clone());

        // TrustEdge::new() defaults to Social graph type.
        let edge = TrustEdge::new(
            alice.clone(),
            bob.clone(),
            TrustScore::unchecked(score_value),
        );
        facade.add_edge(edge).expect("facade.add_edge");

        // store + facade drop here — sled file lock released.
    }

    // ── Phase 2: fresh facade, no shared memory ──────────────────────────────
    let store2 = Arc::new(SledStore::open(&sled_path).expect("reopen sled"));
    let facade2 = TrustGraphFacade::new(store2, alice.clone());

    // Read via the backward-compatible get_edge (defaults to Social graph).
    let retrieved = facade2
        .get_edge(&alice, &bob)
        .expect("facade.get_edge")
        .expect("edge must be present after facade write → sled reopen");

    // 1. Source DID survives the facade round-trip.
    assert_eq!(
        retrieved.source, alice,
        "source DID must survive facade sled drop-and-reopen"
    );

    // 2. Target DID survives exact round-trip.
    assert_eq!(
        retrieved.target, bob,
        "target DID must survive facade sled drop-and-reopen"
    );

    // 3. Score survives exact round-trip.
    assert!(
        (retrieved.score.value() - score_value).abs() < 1e-9,
        "score must survive facade sled drop-and-reopen: expected {score_value}, got {}",
        retrieved.score.value()
    );

    // 4. Graph-type isolation: the Social edge is NOT in the Economic namespace.
    //    This proves that prefixed keys ("trust/social/..." vs "trust/economic/...")
    //    are correctly isolated — a Social write does not bleed into Economic.
    let economic_result = facade2
        .get_edge_from(TrustGraphType::EconomicReliability, &alice, &bob)
        .expect("get_edge_from economic");
    assert!(
        economic_result.is_none(),
        "Social edge must NOT appear in Economic graph (prefix isolation violated)"
    );
}

/// Layer 3 — Same-runtime facade drop + fresh facade restore proof.
///
/// Proves that trust state survives a same-runtime lifecycle boundary:
///
/// 1. Write a Social trust edge through `TrustGraphFacade`.
/// 2. Drop ALL references to the facade and store — `Arc` ref count reaches
///    zero, sled file lock is returned to the OS, actor memory is reclaimed.
///    The sled file on disk is the ONLY bridge.
/// 3. In the SAME test process (no new OS process), open a fresh
///    `SledStore::open()` and construct a brand-new `TrustGraphFacade`.
/// 4. Read back the edge through the fresh facade and assert exact invariants.
///
/// Trust has no background scheduler and no `JoinHandle`. Dropping the `Arc`
/// is the complete shutdown. This is simpler than governance Layer 3 but proves
/// the same fundamental property: the disk artifact is self-sufficient.
///
/// The graph-type isolation invariant (Social absent from Economic) is
/// re-asserted to confirm the fresh facade uses the same prefix logic.
#[test]
fn test_trust_facade_survives_same_runtime_drop_and_recreate() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("trust-l3.sled");

    let alice = KeyPair::generate().expect("alice KeyPair").did().clone();
    let bob = KeyPair::generate().expect("bob KeyPair").did().clone();
    let score_value = 0.55_f64;

    // ── Phase 1: write through facade, then DROP all Arc refs ────────────────
    {
        let store = Arc::new(SledStore::open(&sled_path).expect("open sled"));
        let mut facade = TrustGraphFacade::new(store, alice.clone());

        facade
            .add_edge(TrustEdge::new(
                alice.clone(),
                bob.clone(),
                TrustScore::unchecked(score_value),
            ))
            .expect("facade.add_edge");

        // facade and store drop here.
        // Arc ref count → 0. Sled file lock released.
        // No background task to await — dropping is the full shutdown.
    }

    // ── Boundary: disk is the only bridge ────────────────────────────────────
    // No in-memory Arc, no live sled handle, no cached state.

    // ── Phase 2: brand-new store + facade in the SAME process ────────────────
    let store2 = Arc::new(SledStore::open(&sled_path).expect("reopen sled"));
    let facade2 = TrustGraphFacade::new(store2, alice.clone());

    let retrieved = facade2
        .get_edge(&alice, &bob)
        .expect("facade2.get_edge")
        .expect("edge must survive same-runtime drop+recreate boundary");

    // 1. Source DID survives the lifecycle boundary.
    assert_eq!(
        retrieved.source, alice,
        "source DID must survive same-runtime drop+recreate"
    );

    // 2. Target DID survives.
    assert_eq!(
        retrieved.target, bob,
        "target DID must survive same-runtime drop+recreate"
    );

    // 3. Score survives exact round-trip.
    assert!(
        (retrieved.score.value() - score_value).abs() < 1e-9,
        "score must survive same-runtime drop+recreate: expected {score_value}, got {}",
        retrieved.score.value()
    );

    // 4. Graph-type isolation holds through the fresh facade.
    let economic_result = facade2
        .get_edge_from(TrustGraphType::EconomicReliability, &alice, &bob)
        .expect("get_edge_from economic");
    assert!(
        economic_result.is_none(),
        "Social edge must NOT appear in Economic graph after same-runtime recreate"
    );
}
