//! The N2-A startup gate and the live weak-holder mint seam agree about which
//! `commons/holders/by_did/` states are forbidden (#2627 M3).
//!
//! The two layers protect the same invariant from opposite ends: the gate
//! refuses to *start over* a store holding two spellings of one principal, and
//! the mint seam refuses to *create* that state at runtime. If they disagreed,
//! one of them would be waving through a state the other calls unsafe — and
//! the runtime would keep writing rows a restart then refuses to open.
//!
//! Both halves run against one real `commons.sled`, at the data-directory
//! level where `icn_core::supervisor::lifecycle` and `icn_gateway::server` open
//! it. The two wrappers write sled's default tree, so the gate reads exactly
//! the bytes Commons wrote — the fixture would prove nothing otherwise.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::SystemTime;

use icn_commons::store::{CommonsStoreBackend, SledCommonsStore, HOLDER_BY_DID_PREFIX};
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

fn by_did_row_count(data_dir: &Path) -> usize {
    let store = SledCommonsStore::open(commons_path(data_dir)).unwrap();
    let n = store.scan(HOLDER_BY_DID_PREFIX).unwrap().len();
    drop(store);
    n
}

#[tokio::test]
async fn the_gate_and_the_mint_seam_forbid_the_same_alias_state() {
    let dir = tempfile::tempdir().unwrap();
    let a = spelling_a(40);
    let b = spelling_b(40);
    let c = spelling_c(40);
    assert_eq!(a, b, "the fixture spellings must name one principal");
    assert_ne!(a.as_str(), b.as_str(), "and must be different strings");

    // --- Runtime: the seam creates one holder and refuses the second. -------
    {
        let handle = CommonsHandle::with_sled_path(commons_path(dir.path())).unwrap();
        handle
            .update_display_name(&a, "Alice".to_string())
            .await
            .expect("the first spelling mints one weak holder");
        let refused = handle
            .update_display_name(&b, "Mallory".to_string())
            .await
            .expect_err("the alias must not mint a second holder");
        assert_eq!(refused.to_string(), "holder_principal_already_indexed");
        handle.flush().await.unwrap();
    }
    assert_eq!(
        by_did_row_count(dir.path()),
        1,
        "the seam left exactly one index row"
    );

    // --- Gate: the state the seam produced is one a restart accepts. --------
    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("a store the mint seam wrote is a store the gate opens");
    assert_eq!(receipt.verdict, Verdict::Clear);

    // --- The forbidden state, planted below the seam that refuses it. ------
    // The runtime can no longer create this, so the fixture writes the raw row
    // the way a pre-M3 binary would have.
    {
        let store = SledCommonsStore::open(commons_path(dir.path())).unwrap();
        let mut key = HOLDER_BY_DID_PREFIX.to_vec();
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
            .any(|b| b.contains("icn-commons/holder_by_did") && b.contains("FAIL-CLOSED")),
        "{blockers:?}"
    );

    // --- Runtime: and the seam still refuses to add to it. -----------------
    {
        let handle = CommonsHandle::with_sled_path(commons_path(dir.path())).unwrap();
        let refused = handle
            .update_display_name(&c, "Third".to_string())
            .await
            .expect_err("a third spelling of an already-doubled principal must refuse");
        assert_eq!(refused.to_string(), "holder_principal_already_indexed");
        handle.flush().await.unwrap();
    }
    assert_eq!(
        by_did_row_count(dir.path()),
        2,
        "the refusal added nothing and repaired nothing — M3 chooses no survivor"
    );
}
