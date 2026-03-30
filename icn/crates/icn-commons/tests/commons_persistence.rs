//! Commons substrate persistence proofs — Layers 2 and 3.
//!
//! ## Layer 2 — what it proves
//!
//! Commons state (anchor, charter, holder) written through `CommonsHandle`
//! — the substrate boundary directly below `CommonsManager` — survives a
//! same-runtime drop-and-reopen boundary.
//!
//! Layer 1 (in `icn-gateway/tests/commons_integration.rs`) proved that
//! `CommonsManager::with_sled_path()` round-trips through sled. Layer 2
//! descends one level: it exercises `CommonsHandle` itself, proving the
//! `CommonsHandle → CommonsInner → CommonsStore` path reaches the sled file
//! without any in-memory-only bypass.
//!
//! All Arc refs are dropped in a scoped block before Phase 2 opens the fresh
//! handle. The sled file is the only bridge.
//!
//! ## Layer 3 — what it proves
//!
//! Commons state written through `CommonsHandle` in one OS process is
//! readable by a completely fresh OS process. No shared memory, no shared
//! sled handle, no Arc continuity across the process boundary.
//!
//! Uses `crates/icn-commons/src/bin/commons_restart_helper.rs` and
//! `icn_testkit::subprocess::run_subprocess` for the subprocess invocations.
//!
//! ## What is NOT proven
//!
//! - Layer 4 (daemon restart simulation via CommonsManager + CommonsHandle):
//!   that proof lives in `icn-gateway/tests/commons_integration.rs` because
//!   it requires `CommonsManager` from the gateway crate.
//! - Charter and steward round-trips in Layer 3 (scoped to anchor to keep
//!   the cross-process helper minimal).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_commons::CommonsHandle;
use icn_identity::KeyPair;
use icn_testkit::subprocess::run_subprocess;

// ============================================================================
// Layer 2 — CommonsHandle substrate drop + reopen (same-runtime)
// ============================================================================

/// Layer 2a — PersonhoodAnchor survives CommonsHandle drop + reopen.
///
/// Writes via `CommonsHandle::with_sled_path()`, drops all Arc refs in a
/// scoped block, reopens a fresh handle from the same path, and asserts:
/// 1. Anchor is retrievable by ID.
/// 2. DID reverse-index survives the boundary (anchor retrievable by DID).
#[tokio::test]
async fn test_l2_anchor_survives_handle_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("l2-anchor.sled");

    let keypair = KeyPair::generate().expect("KeyPair::generate");
    let did = keypair.did().clone();

    // ── Phase 1: write through CommonsHandle, then DROP all Arc refs ─────────
    let anchor_id = {
        let handle = CommonsHandle::with_sled_path(&sled_path).expect("open CommonsHandle");
        let anchor = handle
            .create_anchor_from_enrollment(&did, None)
            .await
            .expect("create_anchor_from_enrollment");
        handle.flush().await.expect("flush");
        hex::encode(anchor.id())
        // handle (Arc<RwLock<CommonsInner>>) drops here — sled lock released
    };

    // ── Boundary: disk is the only bridge ────────────────────────────────────

    // ── Phase 2: fresh CommonsHandle, no shared memory ───────────────────────
    let handle2 = CommonsHandle::with_sled_path(&sled_path).expect("reopen CommonsHandle");

    // 1. Anchor present by ID.
    let found = handle2
        .get_anchor(&anchor_id)
        .await
        .expect("get_anchor after reopen");
    assert!(
        found.is_some(),
        "PersonhoodAnchor must survive CommonsHandle drop + reopen (Layer 2)"
    );

    // 2. DID index survives.
    let by_did = handle2
        .get_anchor_by_did(&did)
        .await
        .expect("get_anchor_by_did after reopen");
    assert!(
        by_did.is_some(),
        "DID reverse-index must survive CommonsHandle drop + reopen (Layer 2)"
    );
}

/// Layer 2b — Charter survives CommonsHandle drop + reopen.
///
/// Proves that the charter write path (`CommonsHandle::store_charter`) and
/// read path (`CommonsHandle::get_charter`) are both sled-backed and survive
/// the handle drop-and-reopen boundary with exact field values.
#[tokio::test]
async fn test_l2_charter_survives_handle_drop_and_reopen() {
    use icn_governance::{Charter, DisputePolicy, GovernanceConfig, MembershipPolicy, OrgType};

    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("l2-charter.sled");

    // ── Phase 1: write charter, drop all Arc refs ─────────────────────────────
    let charter_id = {
        let handle = CommonsHandle::with_sled_path(&sled_path).expect("open CommonsHandle");
        let charter = Charter::new(
            OrgType::Cooperative,
            "coop:l2-persist-test".to_string(),
            "Layer 2 Persistence Coop".to_string(),
            GovernanceConfig::cooperative_default(),
            MembershipPolicy::default(),
            DisputePolicy::default(),
        );
        let id = charter.charter_id.to_hex();
        handle.store_charter(charter).await.expect("store_charter");
        handle.flush().await.expect("flush");
        id
        // handle drops here
    };

    // ── Phase 2: fresh CommonsHandle ─────────────────────────────────────────
    let handle2 = CommonsHandle::with_sled_path(&sled_path).expect("reopen CommonsHandle");
    let found = handle2
        .get_charter(&charter_id)
        .await
        .expect("get_charter after reopen");

    assert!(
        found.is_some(),
        "Charter must survive CommonsHandle drop + reopen (Layer 2)"
    );
    assert_eq!(
        found.unwrap().name,
        "Layer 2 Persistence Coop",
        "charter name must survive round-trip"
    );
}

/// Layer 2c — CommonsHolderRecord survives CommonsHandle drop + reopen.
///
/// Proves that the full enrollment path (anchor → holder) survives the
/// CommonsHandle drop-and-reopen boundary. The holder DID must match exactly.
#[tokio::test]
async fn test_l2_holder_survives_handle_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().join("l2-holder.sled");

    let keypair = KeyPair::generate().expect("KeyPair::generate");
    let did = keypair.did().clone();

    // ── Phase 1: enroll anchor + holder, drop ─────────────────────────────────
    let holder_id = {
        let handle = CommonsHandle::with_sled_path(&sled_path).expect("open CommonsHandle");
        let anchor = handle
            .create_anchor_from_enrollment(&did, None)
            .await
            .expect("create_anchor_from_enrollment");
        let anchor_id = hex::encode(anchor.id());
        let holder = handle
            .create_holder_from_anchor(&anchor_id, &did)
            .await
            .expect("create_holder_from_anchor");
        handle.flush().await.expect("flush");
        hex::encode(holder.id())
        // handle drops here
    };

    // ── Phase 2: fresh CommonsHandle ─────────────────────────────────────────
    let handle2 = CommonsHandle::with_sled_path(&sled_path).expect("reopen CommonsHandle");
    let found = handle2
        .get_holder(&holder_id)
        .await
        .expect("get_holder after reopen");

    assert!(
        found.is_some(),
        "CommonsHolderRecord must survive CommonsHandle drop + reopen (Layer 2)"
    );
    assert_eq!(
        found.unwrap().holder_did,
        did,
        "holder_did must survive round-trip"
    );
}

// ============================================================================
// Layer 3 — Cross-process persistence proof (OS process boundary)
// ============================================================================

/// Layer 3 — PersonhoodAnchor survives a true OS process restart.
///
/// A `CommonsHandle` in one OS process writes a `PersonhoodAnchor` to sled
/// and exits. A completely fresh OS process opens the same sled path and
/// verifies the anchor is present — no shared memory, no Arc continuity.
///
/// Uses `commons_restart_helper write/read` subprocesses via
/// `icn_testkit::subprocess::run_subprocess`.
///
/// Invariants asserted across the process boundary:
/// 1. Anchor is present by ID after process restart.
/// 2. DID reverse-index survives (anchor retrievable by DID).
#[test]
fn test_l3_anchor_survives_cross_process_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sled_path = tmp.path().to_str().expect("sled_path to str");
    let helper = env!("CARGO_BIN_EXE_commons_restart_helper");

    // ── Write subprocess: create anchor, flush, print "anchor_id,did" ────────
    let write_stdout = run_subprocess(helper, &["write", sled_path]);

    // stdout format: "anchor_id_hex,did_str"
    let parts: Vec<&str> = write_stdout.splitn(2, ',').collect();
    assert_eq!(
        parts.len(),
        2,
        "write stdout must be 'anchor_id_hex,did_str', got: {write_stdout:?}"
    );
    let (anchor_id_hex, did_str) = (parts[0], parts[1]);

    assert!(
        !anchor_id_hex.is_empty(),
        "anchor_id_hex must be non-empty, got: {anchor_id_hex:?}"
    );
    assert!(
        did_str.starts_with("did:icn:"),
        "did_str must be a valid ICN DID, got: {did_str:?}"
    );

    // ── Read subprocess: fresh process, no shared memory ─────────────────────
    // run_subprocess asserts exit 0; panics with full stdout+stderr otherwise.
    run_subprocess(helper, &["read", sled_path, anchor_id_hex, did_str]);
}
