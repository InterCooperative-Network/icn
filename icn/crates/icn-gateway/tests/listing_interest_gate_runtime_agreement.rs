//! The N2-A startup gate and the live express-interest writer agree about which
//! `v1:interest_idx:` states are forbidden (#2627 M4b).
//!
//! The two layers protect one invariant from opposite ends: the gate refuses to
//! *start over* a store holding two spellings of one principal under one
//! listing, and `add_interest_if_not_duplicate` refuses to *create* that state
//! at runtime. If they disagreed, one would be waving through a state the other
//! calls unsafe — and a gateway would keep accepting interests that a restart
//! then refuses to open.
//!
//! The controls matter as much as the refusals here, because the collision unit
//! is a pair rather than a principal. One member acting on several listings is
//! several legitimate rows naming one principal; a gate that grouped those
//! would refuse to start every gateway holding an ordinary exchange. Both
//! layers must call that shape clear and call two spellings on ONE listing
//! forbidden.
//!
//! Both halves run against one real sled database at the data-directory level
//! the gate discovers, so the gate reads exactly the bytes the gateway wrote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use icn_gateway::listings_mgr::{
    ListingCategory, ListingId, ListingType, ListingVisibility, ListingsManager,
};
use icn_identity::Did;
use icn_store::n2a_startup_gate::{enforce, GateRefusal, Verdict};

fn principal_bytes(seed: u8) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes()
}

/// base58btc — what `Did::from_public_key` produces.
fn spelling_a(seed: u8) -> Did {
    Did::from_public_key(&ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key())
}

/// The same principal, base16-lower.
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

/// The gateway opens its sled database at `<data_dir>/gateway_store`
/// (`icn_gateway::server`), and the gate walks every sled database beneath the
/// data directory.
fn gateway_store_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("gateway_store")
}

fn open_manager(data_dir: &Path) -> ListingsManager {
    let db = sled::open(gateway_store_path(data_dir)).unwrap();
    ListingsManager::with_sled(Arc::new(db))
}

fn index_row_count(data_dir: &Path) -> usize {
    let db = sled::open(gateway_store_path(data_dir)).unwrap();
    let n = db.scan_prefix(b"v1:interest_idx:").count();
    drop(db);
    n
}

fn new_listing(mgr: &ListingsManager, owner: &Did, title: &str) -> ListingId {
    mgr.create_listing(
        ListingType::Offer,
        title.to_string(),
        "gate agreement fixture".to_string(),
        ListingCategory::Equipment,
        owner.clone(),
        "owner-coop".to_string(),
        "Credits".to_string(),
        vec![],
        ListingVisibility::Federation,
        None,
        vec![],
    )
    .unwrap()
    .id
}

#[test]
fn the_gate_and_the_express_interest_writer_forbid_the_same_alias_state() {
    let dir = tempfile::tempdir().unwrap();
    let owner = spelling_a(1);
    let a = spelling_a(50);
    let b = spelling_b(50);
    let c = spelling_c(50);
    assert_eq!(a, b, "the fixture spellings must name one principal");
    assert_ne!(a.as_str(), b.as_str(), "and must be different strings");

    // --- Runtime: the writer accepts one interest and refuses the alias. ----
    let listing = {
        let mgr = open_manager(dir.path());
        let listing = new_listing(&mgr, &owner, "gate agreement");
        mgr.express_interest(
            listing,
            a.clone(),
            "coop".to_string(),
            "first".to_string(),
            None,
        )
        .expect("the first spelling expresses one interest");
        mgr.express_interest(
            listing,
            b.clone(),
            "coop".to_string(),
            "alias".to_string(),
            None,
        )
        .expect_err("the alias must not create a second interest");
        listing
    };
    assert_eq!(
        index_row_count(dir.path()),
        1,
        "one interest leaves one uniqueness row"
    );

    // --- Gate: the state the writer produced is one a restart accepts. ------
    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("a store the express-interest writer wrote is a store the gate opens");
    assert_eq!(receipt.verdict, Verdict::Clear);

    // --- The forbidden state, planted below the writer that refuses it. -----
    // The runtime can no longer create this, so the fixture writes the raw row
    // the way a pre-M4b binary would have.
    {
        let db = sled::open(gateway_store_path(dir.path())).unwrap();
        db.insert(
            format!("v1:interest_idx:{listing}:{b}").as_bytes(),
            b"1".as_slice(),
        )
        .unwrap();
        db.flush().unwrap();
    }

    // --- Gate: that same state refuses the start, fail-closed. --------------
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
        blockers.iter().any(|b| {
            b.contains("icn-gateway/listing_interest_uniqueness") && b.contains("FAIL-CLOSED")
        }),
        "{blockers:?}"
    );

    // --- Runtime: and the writer still refuses to add to it. ----------------
    {
        let mgr = open_manager(dir.path());
        mgr.express_interest(listing, c, "coop".to_string(), "third".to_string(), None)
            .expect_err("a third spelling of an already-doubled principal must refuse");
    }
    assert_eq!(
        index_row_count(dir.path()),
        2,
        "the refusal added nothing and repaired nothing — M4b chooses no survivor"
    );
}

/// A member holding interests in many listings is the shape a mis-registered
/// collision unit would break, and it would only show up once a store held
/// several listings. This is the scale control for the `Verdict::Clear` above.
#[test]
fn one_principal_across_many_listings_still_clears_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    let owner = spelling_a(1);
    let member = spelling_a(51);

    {
        let mgr = open_manager(dir.path());
        for n in 0..10 {
            let listing = new_listing(&mgr, &owner, &format!("listing {n}"));
            mgr.express_interest(
                listing,
                member.clone(),
                "coop".to_string(),
                "interested".to_string(),
                None,
            )
            .expect("one principal may act on each distinct listing");
        }
    }
    assert_eq!(index_row_count(dir.path()), 10);

    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("ten listings for one member are ten facts, not a collision");
    assert_eq!(receipt.verdict, Verdict::Clear);
}

/// And many distinct members on one listing clears too: the guard is one
/// interest per principal per listing, never one interest per listing.
#[test]
fn many_distinct_members_on_one_listing_still_clear_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    let owner = spelling_a(1);

    {
        let mgr = open_manager(dir.path());
        let listing = new_listing(&mgr, &owner, "popular listing");
        for seed in 60u8..70 {
            mgr.express_interest(
                listing,
                spelling_a(seed),
                "coop".to_string(),
                "interested".to_string(),
                None,
            )
            .expect("each distinct principal may express interest");
        }
    }
    assert_eq!(index_row_count(dir.path()), 10);

    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("ten distinct members on one listing are not a collision");
    assert_eq!(receipt.verdict, Verdict::Clear);
}
