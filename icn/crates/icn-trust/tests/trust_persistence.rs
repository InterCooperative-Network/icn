//! Trust graph persistence proof — Layer 1.
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
//! **All existing tests use `SledStore::temporary()`** — an ephemeral in-memory
//! store that does not test durability. This file proves real sled persistence.
//!
//! ## Layer 1 — what it proves
//!
//! A `TrustEdge` written through the direct `TrustGraph` path (the narrowest
//! canonical write path) survives a real sled drop-and-reopen boundary with
//! exact field values: source DID, target DID, and score.
//!
//! The in-memory `TrustCache` (LRU) is gone after the drop. The fresh
//! `TrustGraph` reads exclusively from sled, not from any cache.
//!
//! ## What is NOT proven by this layer
//!
//! - Production handle path (Layer 2 target)
//! - Same-runtime lifecycle boundary (Layer 3)
//! - Cross-process restart (Layer 4)
//! - Evidence fields (additional invariants for future layers)
//! - Typed/multi-graph path (a separate layer 2 concern)

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::KeyPair;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraph, TrustScore};
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
