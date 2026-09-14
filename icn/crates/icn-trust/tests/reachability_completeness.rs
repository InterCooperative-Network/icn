//! icn#2750 — a reachability filter may only reject a target when it is known
//! to hold the complete authoritative set for that query.
//!
//! ## The defect these tests discriminate against
//!
//! `compute_trust_score*` short-circuited to `0.0` when
//! `!reachability.is_empty() && !reachability.may_be_reachable(target)`. That
//! guard used *non-emptiness* as a proxy for *completeness*. The filter starts
//! empty, is populated only as a side effect of in-process `add_edge`, and was
//! never built from storage — so a single unrelated runtime edge made it
//! non-empty and therefore "trusted", while it still knew nothing about any
//! persisted edge. Every principal reachable only through persisted edges then
//! scored `0.0`.
//!
//! Every test below fails against that implementation and passes only because
//! the completeness proxy was removed. None of them touch scoring weights or the
//! ledger's author-trust threshold.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::KeyPair;
use icn_store::{SledStore, Store};
use icn_trust::{ReachabilityFilter, TrustEdge, TrustGraph, TrustScore};
use std::sync::Arc;
use tempfile::TempDir;

/// The ledger's author-trust gate. Asserting against this rather than an exact
/// score keeps these tests independent of the scoring weights, which icn#2750
/// explicitly does not change.
const AUTHOR_TRUST_GATE: f64 = 0.1;

fn open(path: &std::path::Path) -> Arc<dyn Store> {
    Arc::new(SledStore::open(path).expect("open sled"))
}

fn did() -> icn_identity::Did {
    KeyPair::generate().unwrap().did().clone()
}

fn full() -> TrustScore {
    TrustScore::new(1.0).unwrap()
}

/// Writes `own -> root -> subject` and lets the writing process go away, which
/// is the shape `icnctl institution genesis` (#2744) and `icnctl init-coop`
/// (#2718) leave on disk for a separately-running `icnd` to read.
fn persist_genesis_shape(
    path: &std::path::Path,
) -> (icn_identity::Did, icn_identity::Did, icn_identity::Did) {
    let node = did();
    let root = did();
    let subject = did();
    {
        let mut g = TrustGraph::new(open(path), node.clone());
        g.add_edge(TrustEdge::new(node.clone(), root.clone(), full()))
            .unwrap();
        g.add_edge(TrustEdge::new(root.clone(), subject.clone(), full()))
            .unwrap();
    }
    (node, root, subject)
}

/// Dimension 1: persisted-only state, before any runtime mutation.
///
/// This passed before the fix too — it is the control. Its job is to prove the
/// other tests are isolating the runtime edge and not some unrelated breakage
/// in persistence.
#[test]
fn persisted_edges_score_before_any_runtime_mutation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trust");
    let (node, _root, subject) = persist_genesis_shape(&path);

    let g = TrustGraph::new(open(&path), node);
    assert!(
        g.compute_trust_score(&subject).unwrap() >= AUTHOR_TRUST_GATE,
        "a graph that has added no edge of its own must honour persisted edges"
    );
}

/// Dimension 2: persisted state after an *unrelated* runtime edge.
///
/// The core of icn#2750. The runtime edge touches a stranger and has nothing to
/// do with `subject`, but it is what made the filter non-empty and therefore
/// authoritative under the old guard.
#[test]
fn an_unrelated_runtime_edge_does_not_zero_persisted_edges() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trust");
    let (node, _root, subject) = persist_genesis_shape(&path);

    let mut g = TrustGraph::new(open(&path), node.clone());
    g.add_edge(TrustEdge::new(node, did(), full())).unwrap();

    let score = g.compute_trust_score(&subject).unwrap();
    assert!(
        score >= AUTHOR_TRUST_GATE,
        "an unrelated runtime edge must not zero a persisted-only principal; got {score}"
    );
}

/// Dimension 3: with and without a previously primed score cache.
///
/// The LRU cache is consulted before the filter, so a node that happened to
/// score the DID before its first runtime edge kept serving the correct score
/// until the entry expired. That is what made the defect intermittent rather
/// than absent. Both orders must now agree.
#[test]
fn a_primed_cache_and_a_cold_cache_agree() {
    let dir = TempDir::new().unwrap();

    let primed = {
        let path = dir.path().join("primed");
        let (node, _root, subject) = persist_genesis_shape(&path);
        let mut g = TrustGraph::new(open(&path), node.clone());
        let _ = g.compute_trust_score(&subject).unwrap(); // prime
        g.add_edge(TrustEdge::new(node, did(), full())).unwrap();
        g.compute_trust_score(&subject).unwrap()
    };

    let cold = {
        let path = dir.path().join("cold");
        let (node, _root, subject) = persist_genesis_shape(&path);
        let mut g = TrustGraph::new(open(&path), node.clone());
        g.add_edge(TrustEdge::new(node, did(), full())).unwrap(); // no prime
        g.compute_trust_score(&subject).unwrap()
    };

    assert!(
        cold >= AUTHOR_TRUST_GATE,
        "the cold-cache path is the unmasked defect; got {cold}"
    );
    assert_eq!(
        primed, cold,
        "whether a score was cached earlier must not change the answer"
    );
}

/// Dimension 4: restart.
///
/// A daemon restart drops the in-memory filter, so the first score after
/// restart was correct and the first score after the *next* runtime edge was
/// not. Correctness must not depend on how recently the process started.
#[test]
fn the_answer_survives_a_restart_boundary() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trust");
    let (node, _root, subject) = persist_genesis_shape(&path);

    let before_restart = {
        let mut g = TrustGraph::new(open(&path), node.clone());
        g.add_edge(TrustEdge::new(node.clone(), did(), full()))
            .unwrap();
        g.compute_trust_score(&subject).unwrap()
    };

    // Fresh graph and fresh store handle: the disk is the only bridge.
    let after_restart = {
        let mut g = TrustGraph::new(open(&path), node.clone());
        g.add_edge(TrustEdge::new(node, did(), full())).unwrap();
        g.compute_trust_score(&subject).unwrap()
    };

    assert!(
        before_restart >= AUTHOR_TRUST_GATE && after_restart >= AUTHOR_TRUST_GATE,
        "restart must not change whether persisted edges are honoured; \
         before={before_restart} after={after_restart}"
    );
}

/// Why rebuilding the filter when the graph is opened would NOT have been a
/// sufficient fix.
///
/// Everything here happens in one process, with no persisted state and no
/// restart, so an open-time rebuild would have been complete and would still
/// have answered wrongly. `add_edge` adds `target` to the filter only when it
/// *already* trusts `source`; adding `own -> source` afterwards does not
/// back-fill the targets `source` already reaches. So whether a 2-hop target is
/// in the filter depends on the order the edges arrived in — which is why
/// incremental maintenance cannot re-establish completeness, and why
/// `add_reachable` revokes authoritative status instead of preserving it.
#[test]
fn in_process_edge_ordering_does_not_change_the_answer() {
    let dir = TempDir::new().unwrap();
    let node = did();
    let source = did();
    let target = did();

    let mut g = TrustGraph::new(open(&dir.path().join("trust")), node.clone());
    // Reverse order: the intermediate edge lands while `source` is still a
    // stranger, so `target` is never entered into the filter.
    g.add_edge(TrustEdge::new(source.clone(), target.clone(), full()))
        .unwrap();
    g.add_edge(TrustEdge::new(node, source, full())).unwrap();

    let score = g.compute_trust_score(&target).unwrap();
    assert!(
        score >= AUTHOR_TRUST_GATE,
        "a 2-hop target must score the same regardless of the order its edges \
         were added in; got {score}"
    );
}

/// The optimization must still work when it is legitimately authoritative.
///
/// Without this, deleting the fast path entirely would pass every test above.
#[test]
fn an_authoritative_filter_still_rejects() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trust");
    let (node, _root, subject) = persist_genesis_shape(&path);

    let g = TrustGraph::new(open(&path), node);

    g.rebuild_reachability_filter().unwrap();
    assert!(
        g.reachability_filter().is_authoritative(),
        "an explicit rebuild must establish authority"
    );
    // The filter is probabilistic: at a 1% false-positive rate a single random
    // DID is not guaranteed to be absent, so asserting on one sample would flake
    // about one run in a hundred. Probe a batch and require that the fast path
    // fires for at least one — which is what "the optimization still works"
    // actually means.
    let strangers: Vec<_> = (0..64).map(|_| did()).collect();
    let rejected = strangers
        .iter()
        .filter(|s| g.reachability_filter().is_known_unreachable(s))
        .count();
    assert!(
        rejected > 0,
        "an authoritative filter must still reject DIDs it enumerated away; \
         none of {} probes was rejected",
        strangers.len()
    );
    assert!(
        !g.reachability_filter().is_known_unreachable(&subject),
        "a rebuilt filter must find the persisted 2-hop subject"
    );
    assert!(
        g.compute_trust_score(&subject).unwrap() >= AUTHOR_TRUST_GATE,
        "rebuilding must not zero the subject it just enumerated"
    );
}

/// The invariant at the owning primitive, independent of any graph.
#[test]
fn only_a_rebuild_confers_authority() {
    let f = ReachabilityFilter::new();
    let a = did();

    assert!(
        !f.is_authoritative(),
        "a fresh filter has enumerated nothing"
    );
    assert!(
        !f.is_known_unreachable(&a),
        "a filter that knows nothing may not reject"
    );

    // Non-emptiness is not completeness: this is the exact substitution that
    // caused icn#2750.
    f.add_reachable(&did());
    assert!(!f.is_empty(), "precondition: the filter is now non-empty");
    assert!(
        !f.is_authoritative(),
        "an incremental insert must not confer authority"
    );
    assert!(
        !f.is_known_unreachable(&a),
        "a non-empty but incomplete filter may not reject"
    );

    f.rebuild([a.clone()]);
    assert!(f.is_authoritative(), "a rebuild confers authority");
    assert!(
        f.is_known_unreachable(&did()),
        "an authoritative filter rejects what it enumerated away"
    );

    // ... and any later incremental mutation takes it away again.
    f.add_reachable(&did());
    assert!(
        !f.is_authoritative(),
        "an incremental insert after a rebuild revokes authority"
    );

    f.rebuild([a]);
    f.clear();
    assert!(
        !f.is_authoritative(),
        "an empty filter is the absence of a claim, not a claim that nothing is reachable"
    );
}

/// The second guard, which the issue did not mention.
///
/// `compute_trust_score_with_threshold` carried its own copy of the
/// `!is_empty() && !may_be_reachable()` short-circuit at `lib.rs:792`. It was
/// independently defective and is a separate changed call site, so it needs its
/// own witness — otherwise a regression there passes unnoticed behind the
/// weighted path's coverage.
#[test]
fn the_threshold_path_also_honours_persisted_edges() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("trust");
    let (node, _root, subject) = persist_genesis_shape(&path);

    let mut g = TrustGraph::new(open(&path), node.clone());
    g.add_edge(TrustEdge::new(node, did(), full())).unwrap();

    let score = g
        .compute_trust_score_with_threshold(&subject, Some(AUTHOR_TRUST_GATE))
        .unwrap();
    assert!(
        score >= AUTHOR_TRUST_GATE,
        "the threshold path must not zero a persisted-only principal after an \
         unrelated runtime edge; got {score}"
    );
}

/// A filter may only guard a query whose reachability it actually enumerated.
///
/// `rebuild_reachability_filter` enumerates two edges deep. The pathfinder that
/// `compute_trust_score_with_threshold` uses checks `get_edge(current, target)`
/// for nodes it expanded to `max_hops`, so it can score `own -> a -> b ->
/// target` — three. A rebuilt filter does not contain that target, so guarding
/// the threshold path with it made an authoritative rebuild *cause* a 0.0 that
/// the identical cold query scores positively.
#[test]
fn a_rebuild_does_not_zero_a_three_edge_target_on_the_threshold_path() {
    let dir = TempDir::new().unwrap();
    let node = did();
    let a = did();
    let b = did();
    let target = did();

    let mut g = TrustGraph::new(open(&dir.path().join("trust")), node.clone());
    for (s, t) in [
        (node.clone(), a.clone()),
        (a, b.clone()),
        (b, target.clone()),
    ] {
        g.add_edge(TrustEdge::new(s, t, full())).unwrap();
    }

    let before = g.compute_trust_score_with_threshold(&target, None).unwrap();

    // Make the filter authoritative, then ask the same question again.
    g.rebuild_reachability_filter().unwrap();
    assert!(
        g.reachability_filter().is_authoritative(),
        "precondition: the rebuild must have conferred authority"
    );
    // The rebuilt filter genuinely does not know this target — that is the
    // point. Guarding this path with it would reject a reachable principal.
    assert!(
        !g.reachability_filter().may_be_reachable(&target),
        "precondition: a three-edge target is outside a two-edge enumeration"
    );

    let after = g.compute_trust_score_with_threshold(&target, None).unwrap();
    assert_eq!(
        before, after,
        "rebuilding the filter must not change what the threshold path scores \
         (before={before}, after={after})"
    );
}

/// A refresh that fails partway must not leave the previous snapshot
/// authoritative — a stale snapshot rejects targets storage has since gained.
#[test]
fn authority_is_revoked_before_a_rebuild_re_enumerates() {
    let f = ReachabilityFilter::new();
    let a = did();
    f.rebuild([a.clone()]);
    assert!(
        f.is_authoritative(),
        "precondition: authoritative after rebuild"
    );

    f.revoke_authority();
    assert!(
        !f.is_authoritative(),
        "a revoked filter may not answer for the graph it used to describe"
    );
    assert!(
        !f.is_known_unreachable(&did()),
        "and must not reject while revoked"
    );
    // Contents survive the revocation; only the claim about them is withdrawn.
    assert!(
        f.may_be_reachable(&a),
        "revocation must not discard contents"
    );
}
