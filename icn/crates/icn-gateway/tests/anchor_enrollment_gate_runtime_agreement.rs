//! The N2-A startup gate and the live SDIS enrollment seam agree about which
//! `commons/anchors/by_did/` states are forbidden (#2627 M4a).
//!
//! The two layers protect the same invariant from opposite ends: the gate
//! refuses to *start over* a store holding two spellings of one principal, and
//! the enrollment constructor refuses to *create* that state at runtime. If
//! they disagreed, one of them would be waving through a state the other calls
//! unsafe — and the runtime would keep minting anchors a restart then refuses
//! to open.
//!
//! The load-bearing agreement here is narrower than M3's, because this
//! namespace holds two rows per healthy anchor. `put_anchor` files an anchor
//! under `Did::from_anchor_id`, a function of the random anchor id;
//! `put_anchor_did_index` files the same anchor under the enrollment spelling.
//! Both layers must call that pair *clear* and call two rows naming one
//! principal *forbidden*. A gate that grouped the two healthy rows together
//! would refuse every enrolled node at startup.
//!
//! Both halves run against one real `commons.sled`, at the data-directory
//! level where `icn_core::supervisor::lifecycle` and `icn_gateway::server` open
//! it, so the gate reads exactly the bytes Commons wrote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::SystemTime;

use icn_commons::store::{CommonsStoreBackend, SledCommonsStore, ANCHOR_BY_DID_PREFIX};
use icn_commons::CommonsHandle;
use icn_identity::Did;
use icn_store::n2a_startup_gate::{enforce, GateRefusal, Verdict};

fn principal_bytes(seed: u8) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes()
}

/// One principal, spelled base58btc — what `Did::from_public_key` produces.
fn spelling_a(seed: u8) -> Did {
    Did::from_public_key(&ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key())
}

/// The same principal, spelled base16-lower. A different string, one identity.
fn spelling_b(seed: u8) -> Did {
    format!("did:icn:f{}", hex::encode(principal_bytes(seed)))
        .parse()
        .unwrap()
}

/// A third spelling, base16-upper.
fn spelling_c(seed: u8) -> Did {
    format!("did:icn:F{}", hex::encode_upper(principal_bytes(seed)))
        .parse()
        .unwrap()
}

fn commons_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("commons.sled")
}

fn anchor_index_row_count(data_dir: &Path) -> usize {
    let store = SledCommonsStore::open(commons_path(data_dir)).unwrap();
    let n = store.scan(ANCHOR_BY_DID_PREFIX).unwrap().len();
    drop(store);
    n
}

#[tokio::test]
async fn the_gate_and_the_enrollment_seam_forbid_the_same_alias_state() {
    let dir = tempfile::tempdir().unwrap();
    let a = spelling_a(40);
    let b = spelling_b(40);
    let c = spelling_c(40);
    assert_eq!(a, b, "the fixture spellings must name one principal");
    assert_ne!(a.as_str(), b.as_str(), "and must be different strings");

    // --- Runtime: the seam enrols once and refuses the alias. --------------
    {
        let handle = CommonsHandle::with_sled_path(commons_path(dir.path())).unwrap();
        handle
            .create_anchor_from_enrollment(&a, None)
            .await
            .expect("the first spelling enrols one anchor");
        let refused = handle
            .create_anchor_from_enrollment(&b, None)
            .await
            .expect_err("the alias must not enrol a second anchor");
        assert_eq!(refused.to_string(), "anchor_principal_already_enrolled");
        handle.flush().await.unwrap();
    }
    // Two rows, one anchor: the enrollment spelling and the anchor's own
    // derived DID. This is the healthy shape, not a collision.
    assert_eq!(
        anchor_index_row_count(dir.path()),
        2,
        "one enrollment leaves two index rows naming two different principals"
    );

    // --- Gate: the state the seam produced is one a restart accepts. --------
    // This is the half that would fail if the descriptor grouped an anchor's
    // two healthy rows together.
    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("a store the enrollment seam wrote is a store the gate opens");
    assert_eq!(receipt.verdict, Verdict::Clear);

    // --- The forbidden state, planted below the seam that refuses it. ------
    // The runtime can no longer create this, so the fixture writes the raw row
    // the way a pre-M4a binary would have.
    {
        let store = SledCommonsStore::open(commons_path(dir.path())).unwrap();
        let mut key = ANCHOR_BY_DID_PREFIX.to_vec();
        key.extend_from_slice(b.to_string().as_bytes());
        store.put(&key, b"00").unwrap();
        store.flush().unwrap();
    }

    // --- Gate: that same state refuses the start, fail-closed. -------------
    let blocked = match enforce(dir.path(), SystemTime::now()) {
        Err(GateRefusal::Blocked { receipt, .. }) => *receipt,
        other => panic!("expected the alias pair to block the start, got {other:?}"),
    };
    let blockers: Vec<String> = blocked
        .stores
        .iter()
        .flat_map(|s| s.blocking.iter().map(|b| b.describe()))
        .collect();
    assert!(
        blockers
            .iter()
            .any(|b| b.contains("icn-commons/anchor_by_did") && b.contains("FAIL-CLOSED")),
        "{blockers:?}"
    );

    // --- Runtime: and the seam still refuses to add to it. -----------------
    {
        let handle = CommonsHandle::with_sled_path(commons_path(dir.path())).unwrap();
        let refused = handle
            .create_anchor_from_enrollment(&c, None)
            .await
            .expect_err("a third spelling of an already-doubled principal must refuse");
        assert_eq!(refused.to_string(), "anchor_principal_already_enrolled");
        handle.flush().await.unwrap();
    }
    assert_eq!(
        anchor_index_row_count(dir.path()),
        3,
        "the refusal added nothing and repaired nothing — M4a chooses no survivor"
    );
}

/// Many distinct principals enrol and the gate still clears.
///
/// The per-anchor row pair is the shape most likely to be mis-registered, and
/// a descriptor that mis-tokenized it would only show up once a store held
/// several anchors. This is the scale control for the `Verdict::Clear` above.
#[tokio::test]
async fn a_store_of_many_distinct_enrollments_still_clears_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    {
        let handle = CommonsHandle::with_sled_path(commons_path(dir.path())).unwrap();
        for seed in 60u8..70 {
            handle
                .create_anchor_from_enrollment(&spelling_a(seed), None)
                .await
                .expect("each distinct principal enrols");
        }
        handle.flush().await.unwrap();
    }
    assert_eq!(anchor_index_row_count(dir.path()), 20);

    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("ten distinct enrollments are not a collision");
    assert_eq!(receipt.verdict, Verdict::Clear);
}
