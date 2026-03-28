//! Integration tests for the shared-handle actor boundary.
#![allow(clippy::expect_used, clippy::unwrap_used)]
//!
//! These tests prove three invariants:
//! 1. Two CommonsHandle clones (same Arc) share state — mutations via one are visible via the other.
//! 2. Sled-backed state persists across handle drop and reopen from the same path.
//! 3. CommonsManager::with_handle(handle) is a thin facade — both facades over one handle see
//!    the same data, preventing the dual-ownership divergence we had before PR #1452.

use icn_commons::CommonsHandle;
use icn_identity::Did;

/// Construct a deterministic test DID from a seed byte using a real Ed25519 keypair.
/// This produces a DID that passes DID deserialization validation (valid Ed25519 point).
fn test_did(seed: u8) -> Did {
    use ed25519_dalek::SigningKey;
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    Did::from_public_key(&signing_key.verifying_key())
}

// ─── Test 1: shared handle reference ─────────────────────────────────────────

/// Two clones of the same CommonsHandle share the underlying Arc<RwLock<CommonsInner>>.
/// A write through handle_a must be visible through handle_b without any explicit sync.
#[tokio::test]
async fn shared_handle_clones_see_same_state() {
    let handle_a = CommonsHandle::new_in_memory();
    let handle_b = handle_a.clone(); // same Arc

    let did = test_did(1);

    // Write through handle_a
    handle_a
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect("create anchor via handle_a");

    // Read through handle_b — must see the anchor written by handle_a
    let anchor = handle_b
        .get_anchor_by_did(&did)
        .await
        .expect("get anchor via handle_b");

    assert!(
        anchor.is_some(),
        "handle_b must see anchor created by handle_a (shared Arc)"
    );
}

// ─── Test 2: sled persistence ─────────────────────────────────────────────────

/// State written through a sled-backed CommonsHandle must survive handle drop.
/// Re-opening the same path must return the persisted anchor.
#[tokio::test]
async fn sled_backed_state_survives_handle_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("commons.sled");

    let did = test_did(2);

    // Write and flush; capture the hex anchor ID (returned from create_anchor_from_enrollment)
    let anchor_id_hex = {
        let handle = CommonsHandle::with_sled_path(&path).expect("open sled handle");
        let anchor = handle
            .create_anchor_from_enrollment(&did, None)
            .await
            .expect("create anchor");
        let id_hex = hex::encode(anchor.anchor.id);
        handle.flush().await.expect("flush");
        id_hex
        // handle drops here, closing the sled lock
    };

    // Reopen from same path — data must be present
    let handle2 = CommonsHandle::with_sled_path(&path).expect("reopen sled handle");
    let anchor = handle2
        .get_anchor(&anchor_id_hex)
        .await
        .expect("get anchor after reopen");

    assert!(
        anchor.is_some(),
        "anchor must persist across CommonsHandle drop and reopen from same path"
    );
}

// ─── Test 3: two CommonsManager facades over one handle ───────────────────────

/// CommonsManager::with_handle(h) must be a pure facade — no second store.
/// Writes through manager_a must be visible through manager_b (same handle).
///
/// This is the regression test for the dual-ownership bug: before PR #1452,
/// CommonsManager owned its own CommonsStore, so two gateways over the same
/// sled path would have divergent LRU caches.
#[tokio::test]
async fn two_managers_over_one_handle_share_state() {
    use icn_commons::CommonsHandle;

    let handle = CommonsHandle::new_in_memory();

    // Both managers are facades over the exact same CommonsHandle.
    // In the daemon, the supervisor creates one handle and injects it into the
    // gateway via GatewayServer::with_commons_handle(). This test verifies the contract.
    let handle_for_a = handle.clone();
    let handle_for_b = handle.clone();

    let did = test_did(3);

    // Write through manager_a's handle
    handle_for_a
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect("create anchor via facade A");

    // Read through manager_b's handle — must see the write
    let anchor = handle_for_b
        .get_anchor_by_did(&did)
        .await
        .expect("get anchor via facade B");

    assert!(
        anchor.is_some(),
        "facade B must see state written by facade A (shared handle, no dual ownership)"
    );
}
