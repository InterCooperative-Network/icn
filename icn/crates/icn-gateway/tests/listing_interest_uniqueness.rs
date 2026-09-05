//! Listing-interest uniqueness names the **principal**, not the spelling
//! (#2627 M4b).
//!
//! `v1:interest_idx:<listing>:<spelling>` is claimed with a sled
//! compare-and-swap, and that CAS *is* the one-interest-per-member rule. A
//! `did:icn:` identifier has many accepted spellings of the same 32 bytes, so
//! before this unit one principal arriving under two spellings hit two
//! different keys, won two CASes and got two canonical interests — through the
//! ordinary authenticated route, with the listing owner seeing both.
//!
//! These fixtures drive the production `POST /listings/{id}/interest` handler
//! wherever the property is about the route, and `ListingsManager` directly
//! where it is about storage classification or needs real threads. Nothing here
//! manufactures a sled key except where the point is malformed evidence the
//! writer cannot produce.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Barrier};

use actix_web::test as http;
use actix_web::{web, App, HttpMessage};
use icn_gateway::api::listings;
use icn_gateway::auth::TokenClaims;
use icn_gateway::listings_mgr::{
    Listing, ListingCategory, ListingId, ListingInterest, ListingType, ListingVisibility,
    ListingsManager,
};
use icn_gateway::rate_limit::IpRateLimiter;
use icn_identity::Did;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Spellings
// ---------------------------------------------------------------------------

fn principal_bytes(seed: u8) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes()
}

/// base58btc — what `Did::from_public_key` produces, and what every existing
/// row on disk is spelled as.
fn spelling_a(seed: u8) -> Did {
    Did::from_public_key(&ed25519_dalek::SigningKey::from_bytes(&[seed; 32]).verifying_key())
}

/// The same principal, base16-lower. A different string, one identity.
fn spelling_b(seed: u8) -> Did {
    format!("did:icn:f{}", hex::encode(principal_bytes(seed)))
        .parse()
        .unwrap()
}

/// A third spelling of the same principal, base16-upper.
fn spelling_c(seed: u8) -> Did {
    format!("did:icn:F{}", hex::encode_upper(principal_bytes(seed)))
        .parse()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn sled_manager() -> ListingsManager {
    let db = sled::Config::new().temporary(true).open().unwrap();
    ListingsManager::with_sled(Arc::new(db))
}

fn sled_manager_with_db() -> (ListingsManager, Arc<sled::Db>) {
    let db = Arc::new(sled::Config::new().temporary(true).open().unwrap());
    (ListingsManager::with_sled(db.clone()), db)
}

fn active_listing(mgr: &ListingsManager, owner: Did, title: &str) -> Listing {
    mgr.create_listing(
        ListingType::Offer,
        title.to_string(),
        "fixture listing".to_string(),
        ListingCategory::Equipment,
        owner,
        "owner-coop".to_string(),
        "Credits".to_string(),
        vec![],
        ListingVisibility::Federation,
        None,
        vec![],
    )
    .unwrap()
}

fn claims(did: &str) -> TokenClaims {
    TokenClaims {
        entity_id: None,
        entity_type: None,
        sub: did.to_string(),
        iat: 1_000_000_000,
        coop_id: "test-coop".to_string(),
        scopes: vec!["coop:write".to_string(), "coop:read".to_string()],
        exp: 9_999_999_999,
        jti: None,
    }
}

/// The route as the gateway mounts it: the manager behind the same
/// `Arc<RwLock<_>>` `server.rs` registers, and the IP limiter the handler
/// requires.
async fn route(
    mgr: Arc<RwLock<ListingsManager>>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    http::init_service(
        App::new()
            .app_data(web::Data::new(mgr))
            .app_data(web::Data::new(Arc::new(IpRateLimiter::new_for_auth())))
            .service(web::scope("/listings").configure(listings::configure_routes)),
    )
    .await
}

/// One authenticated `POST /listings/{id}/interest`, returning status and body.
async fn post_interest<S>(
    app: &S,
    listing: &ListingId,
    as_did: &Did,
    message: &str,
) -> (u16, String)
where
    S: actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
{
    let req = http::TestRequest::post()
        .uri(&format!("/listings/{listing}/interest"))
        .set_json(serde_json::json!({ "message": message }))
        .to_request();
    req.extensions_mut().insert(claims(as_did.as_str()));
    let resp = http::call_service(app, req).await;
    let status = resp.status().as_u16();
    let body = String::from_utf8_lossy(&http::read_body(resp).await).into_owned();
    (status, body)
}

fn index_rows(db: &sled::Db, listing: &ListingId) -> Vec<(String, Vec<u8>)> {
    db.scan_prefix(format!("v1:interest_idx:{listing}:").as_bytes())
        .map(|i| {
            let (k, v) = i.unwrap();
            (String::from_utf8_lossy(&k).into_owned(), v.to_vec())
        })
        .collect()
}

fn primary_rows(db: &sled::Db, listing: &ListingId) -> Vec<String> {
    db.scan_prefix(format!("v1:interest:{listing}:").as_bytes())
        .map(|i| String::from_utf8_lossy(&i.unwrap().0).into_owned())
        .collect()
}

// ===========================================================================
// The defect, through the production route
// ===========================================================================

#[actix_web::test]
async fn the_route_refuses_a_second_interest_from_one_principal_under_another_spelling() {
    let (mgr, db) = sled_manager_with_db();
    let a = spelling_a(2);
    let b = spelling_b(2);
    assert_eq!(a, b, "the fixture spellings must name one principal");
    assert_ne!(a.as_str(), b.as_str(), "and must be different strings");

    let listing = active_listing(&mgr, spelling_a(1), "alias defect").id;
    let mgr = Arc::new(RwLock::new(mgr));
    let app = route(mgr.clone()).await;

    let (first, _) = post_interest(&app, &listing, &a, "from spelling A").await;
    assert_eq!(
        first, 201,
        "the principal's first interest must be accepted"
    );

    let (second, _) = post_interest(&app, &listing, &b, "from spelling B").await;
    assert_ne!(
        second, 201,
        "one principal must not get a second interest by re-spelling its DID"
    );

    assert_eq!(
        primary_rows(&db, &listing).len(),
        1,
        "exactly one canonical ListingInterest may exist for one principal"
    );
    assert_eq!(
        index_rows(&db, &listing).len(),
        1,
        "and exactly one uniqueness row"
    );
    assert_eq!(
        mgr.read().await.get_interests(&listing).unwrap().len(),
        1,
        "the read path must report one logical interest"
    );

    // The owner-facing list is the surface the defect was visible on.
    let req = http::TestRequest::get()
        .uri(&format!("/listings/{listing}/interests"))
        .to_request();
    req.extensions_mut().insert(claims(spelling_a(1).as_str()));
    let resp = http::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 200);
    let seen: Vec<serde_json::Value> =
        serde_json::from_slice(&http::read_body(resp).await).unwrap();
    assert_eq!(seen.len(), 1, "the owner must see one interest, not two");
}

/// A third spelling is refused too: the guard is not a two-spelling special
/// case.
#[actix_web::test]
async fn a_third_spelling_of_one_principal_is_refused_as_well() {
    let mgr = sled_manager();
    let listing = active_listing(&mgr, spelling_a(1), "three spellings").id;
    let mgr = Arc::new(RwLock::new(mgr));
    let app = route(mgr.clone()).await;

    assert_eq!(
        post_interest(&app, &listing, &spelling_a(5), "a").await.0,
        201
    );
    assert_ne!(
        post_interest(&app, &listing, &spelling_b(5), "b").await.0,
        201
    );
    assert_ne!(
        post_interest(&app, &listing, &spelling_c(5), "c").await.0,
        201
    );
    assert_eq!(mgr.read().await.get_interests(&listing).unwrap().len(), 1);
}

/// The client must not be able to tell an alias duplicate from an ordinary one:
/// "this principal already holds an interest, under a different spelling" is an
/// implementation detail about someone's identity.
#[actix_web::test]
async fn an_alias_duplicate_is_indistinguishable_from_a_same_spelling_duplicate() {
    let mgr = sled_manager();
    let owner = spelling_a(1);
    let same = active_listing(&mgr, owner.clone(), "same-spelling").id;
    let alias = active_listing(&mgr, owner, "alias").id;
    let app = route(Arc::new(RwLock::new(mgr))).await;

    assert_eq!(post_interest(&app, &same, &spelling_a(7), "1").await.0, 201);
    let (same_status, same_body) = post_interest(&app, &same, &spelling_a(7), "2").await;

    assert_eq!(
        post_interest(&app, &alias, &spelling_a(7), "1").await.0,
        201
    );
    let (alias_status, alias_body) = post_interest(&app, &alias, &spelling_b(7), "2").await;

    assert_eq!(
        same_status, alias_status,
        "both duplicates must carry the same status"
    );
    assert_eq!(
        same_body, alias_body,
        "and the same body: nothing may reveal that a spelling differed"
    );
    for leak in ["spelling", "alias", "principal", "did:icn:"] {
        assert!(
            !alias_body.contains(leak),
            "duplicate response leaked {leak:?}: {alias_body}"
        );
    }
}

// ===========================================================================
// Controls that must keep working
// ===========================================================================

#[actix_web::test]
async fn the_same_spelling_duplicate_control_is_unchanged() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "same spelling").id;
    let app = route(Arc::new(RwLock::new(mgr))).await;
    let a = spelling_a(3);

    assert_eq!(post_interest(&app, &listing, &a, "first").await.0, 201);
    assert_ne!(post_interest(&app, &listing, &a, "second").await.0, 201);
    assert_eq!(primary_rows(&db, &listing).len(), 1);
    assert_eq!(index_rows(&db, &listing).len(), 1);
}

#[actix_web::test]
async fn two_distinct_principals_may_both_express_interest_in_one_listing() {
    let (mgr, db) = sled_manager_with_db();
    let a = spelling_a(3);
    let c = spelling_a(4);
    assert_ne!(a, c, "the control needs two genuinely different principals");

    let listing = active_listing(&mgr, spelling_a(1), "two members").id;
    let mgr = Arc::new(RwLock::new(mgr));
    let app = route(mgr.clone()).await;

    assert_eq!(post_interest(&app, &listing, &a, "from A").await.0, 201);
    assert_eq!(post_interest(&app, &listing, &c, "from C").await.0, 201);
    assert_eq!(primary_rows(&db, &listing).len(), 2);
    assert_eq!(index_rows(&db, &listing).len(), 2);
    assert_eq!(mgr.read().await.get_interests(&listing).unwrap().len(), 2);
}

/// M4b is one interest per principal **per listing**, not one interest per
/// principal. A guard that dropped the listing from the collision unit would
/// stop a member acting on the rest of the exchange.
#[actix_web::test]
async fn uniqueness_is_scoped_to_one_listing() {
    let mgr = sled_manager();
    let owner = spelling_a(1);
    let first = active_listing(&mgr, owner.clone(), "listing one").id;
    let second = active_listing(&mgr, owner, "listing two").id;
    let app = route(Arc::new(RwLock::new(mgr))).await;
    let a = spelling_a(6);

    assert_eq!(post_interest(&app, &first, &a, "one").await.0, 201);
    assert_eq!(
        post_interest(&app, &second, &a, "two").await.0,
        201,
        "one principal must be able to act on a different listing"
    );
    // And an alias on the second listing is still refused there.
    assert_ne!(
        post_interest(&app, &second, &spelling_b(6), "two'").await.0,
        201
    );
}

/// The two backends implement one trait and must therefore encode one domain
/// rule. `InMemoryListingsStore` compared `Did`s — principal equality — while
/// sled compared spellings through the key. That divergence is the defect
/// restated, and this pins that it is gone.
#[actix_web::test]
async fn the_sled_and_in_memory_backends_apply_one_duplicate_rule() {
    for (label, mgr) in [
        ("in-memory", ListingsManager::new()),
        ("sled", sled_manager()),
    ] {
        let listing = active_listing(&mgr, spelling_a(1), "backend parity").id;
        let first = mgr.express_interest(
            listing,
            spelling_a(8),
            "coop".to_string(),
            "A".to_string(),
            None,
        );
        let alias = mgr.express_interest(
            listing,
            spelling_b(8),
            "coop".to_string(),
            "B".to_string(),
            None,
        );
        let distinct = mgr.express_interest(
            listing,
            spelling_a(9),
            "coop".to_string(),
            "C".to_string(),
            None,
        );

        assert!(first.is_ok(), "{label}: first interest must be accepted");
        assert!(alias.is_err(), "{label}: alias spelling must be refused");
        assert!(
            distinct.is_ok(),
            "{label}: another principal must be accepted"
        );
        assert_eq!(
            mgr.get_interests(&listing).unwrap().len(),
            2,
            "{label}: two principals, two interests"
        );
    }
}

// ===========================================================================
// Evidence that cannot be read must be refused, never read as absence
// ===========================================================================

/// Put a raw uniqueness row on disk. Only used to build states the writer
/// cannot produce.
fn forge_index_row(db: &sled::Db, listing: &ListingId, suffix: &str, value: &[u8]) {
    db.insert(
        format!("v1:interest_idx:{listing}:{suffix}").as_bytes(),
        value,
    )
    .unwrap();
}

#[test]
fn a_uniqueness_row_naming_no_principal_refuses_rather_than_reading_absence() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "malformed spelling").id;
    forge_index_row(&db, &listing, "did:icn:!!!not-multibase", b"1");

    let err = mgr
        .express_interest(
            listing,
            spelling_a(10),
            "coop".to_string(),
            "m".to_string(),
            None,
        )
        .expect_err("an undecodable spelling cannot be proven not to name this principal");
    assert!(
        err.to_string()
            .contains("uniqueness-key-names-no-principal"),
        "unexpected reason: {err}"
    );
    assert!(
        primary_rows(&db, &listing).is_empty(),
        "no canonical interest may be written over unreadable evidence"
    );
}

#[test]
fn a_uniqueness_row_with_an_unrecognised_value_refuses() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "bad value").id;
    forge_index_row(&db, &listing, spelling_a(11).as_str(), b"not-the-sentinel");

    let err = mgr
        .express_interest(
            listing,
            spelling_a(12),
            "coop".to_string(),
            "m".to_string(),
            None,
        )
        .expect_err("a row the writer never wrote is evidence this code cannot interpret");
    assert!(
        err.to_string()
            .contains("uniqueness-row-value-unrecognised"),
        "unexpected reason: {err}"
    );
}

#[test]
fn a_uniqueness_key_whose_listing_framing_is_unparsable_refuses() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "bad framing").id;
    // Under the listing's own prefix, but with a second framing component that
    // is not the writer's UUID rendering.
    db.insert(
        format!("v1:interest_idx:{listing}:not-a-uuid:{}", spelling_a(13)).as_bytes(),
        b"1".as_slice(),
    )
    .unwrap();

    let err = mgr
        .express_interest(
            listing,
            spelling_a(14),
            "coop".to_string(),
            "m".to_string(),
            None,
        )
        .expect_err("a key that is not the writer's layout must refuse");
    assert!(
        err.to_string().contains("uniqueness-key"),
        "unexpected reason: {err}"
    );
}

#[test]
fn an_unreadable_canonical_interest_refuses() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "corrupt primary").id;
    db.insert(
        format!("v1:interest:{listing}:{}", uuid::Uuid::new_v4()).as_bytes(),
        b"not an encoded ListingInterest".as_slice(),
    )
    .unwrap();

    let err = mgr
        .express_interest(
            listing,
            spelling_a(15),
            "coop".to_string(),
            "m".to_string(),
            None,
        )
        .expect_err("a canonical row that does not decode may name this principal");
    assert!(
        err.to_string().contains("canonical-interest-unreadable"),
        "unexpected reason: {err}"
    );
}

#[test]
fn a_canonical_interest_filed_under_another_listing_refuses() {
    let (mgr, db) = sled_manager_with_db();
    let owner = spelling_a(1);
    let listing = active_listing(&mgr, owner.clone(), "listing mismatch").id;
    let other = active_listing(&mgr, owner, "other listing").id;

    // A canonical interest whose body names `other`, filed under `listing`.
    let interest = ListingInterest::new(
        other,
        spelling_a(16),
        "coop".to_string(),
        "m".to_string(),
        None,
        1,
    );
    db.insert(
        format!("v1:interest:{listing}:{}", interest.id).as_bytes(),
        icn_encoding::encode_versioned(&interest).unwrap(),
    )
    .unwrap();

    let err = mgr
        .express_interest(
            listing,
            spelling_a(17),
            "coop".to_string(),
            "m".to_string(),
            None,
        )
        .expect_err("a row filed under a listing it does not name is physical disagreement");
    assert!(
        err.to_string()
            .contains("canonical-interest-listing-mismatch"),
        "unexpected reason: {err}"
    );
}

/// A uniqueness row whose canonical interest is gone still suppresses that
/// principal. The live path must not read the missing row as absence and write
/// a replacement — repairing evidence is the maintenance pass's job.
#[test]
fn a_uniqueness_row_whose_canonical_interest_is_gone_still_suppresses_that_principal() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "dangling row").id;
    mgr.express_interest(
        listing,
        spelling_a(18),
        "coop".to_string(),
        "real".to_string(),
        None,
    )
    .unwrap();

    for key in primary_rows(&db, &listing) {
        db.remove(key.as_bytes()).unwrap();
    }
    assert_eq!(index_rows(&db, &listing).len(), 1, "the lock row survives");

    // Both the original spelling and an alias must be refused: neither may
    // create a canonical interest while an unbacked lock names the principal.
    assert!(mgr
        .express_interest(
            listing,
            spelling_a(18),
            "coop".to_string(),
            "again".to_string(),
            None
        )
        .is_err());
    assert!(mgr
        .express_interest(
            listing,
            spelling_b(18),
            "coop".to_string(),
            "alias".to_string(),
            None
        )
        .is_err());
    assert!(
        primary_rows(&db, &listing).is_empty(),
        "no replacement interest may be written"
    );
}

/// A dangling row belonging to **another** principal must not block this one.
/// Refusal is for evidence that cannot be proven not to name the caller; a row
/// that decodes cleanly to somebody else is proven not to.
#[test]
fn a_dangling_row_for_another_principal_does_not_block_this_one() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "other's orphan").id;
    mgr.express_interest(
        listing,
        spelling_a(19),
        "coop".to_string(),
        "theirs".to_string(),
        None,
    )
    .unwrap();
    for key in primary_rows(&db, &listing) {
        db.remove(key.as_bytes()).unwrap();
    }

    assert!(
        mgr.express_interest(
            listing,
            spelling_a(20),
            "coop".to_string(),
            "mine".to_string(),
            None
        )
        .is_ok(),
        "a different principal's orphaned lock must not close the listing"
    );
}

// ===========================================================================
// Cleanup — the parser, and what it may delete
// ===========================================================================

#[test]
fn cleanup_recognises_an_orphan_written_by_the_production_writer() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "orphan").id;
    let a = spelling_a(21);
    mgr.express_interest(
        listing,
        a.clone(),
        "coop".to_string(),
        "x".to_string(),
        None,
    )
    .unwrap();

    // Exactly the crash window: the lock row is claimed, the canonical row is
    // not there.
    for key in primary_rows(&db, &listing) {
        db.remove(key.as_bytes()).unwrap();
    }
    let rows = index_rows(&db, &listing);
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].0.contains("did:icn:"),
        "the fixture must use a real DID-bearing key: {}",
        rows[0].0
    );
    assert!(
        rows[0].0.split(':').count() > 4,
        "a real key has more than four colon-separated components — the count \
         the old parser required"
    );

    assert_eq!(
        mgr.cleanup_orphaned_interest_indexes().unwrap(),
        1,
        "the orphan must be recognised"
    );
    assert!(index_rows(&db, &listing).is_empty());

    // And the lockout the orphan caused is released.
    assert!(mgr
        .express_interest(listing, a, "coop".to_string(), "retry".to_string(), None)
        .is_ok());
}

#[test]
fn cleanup_retains_a_row_whose_canonical_interest_survives() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "healthy").id;
    mgr.express_interest(
        listing,
        spelling_a(22),
        "coop".to_string(),
        "x".to_string(),
        None,
    )
    .unwrap();

    assert_eq!(
        mgr.cleanup_orphaned_interest_indexes().unwrap(),
        0,
        "a backed row is not an orphan"
    );
    assert_eq!(index_rows(&db, &listing).len(), 1);
    assert_eq!(primary_rows(&db, &listing).len(), 1);
}

#[test]
fn cleanup_leaves_another_listings_rows_untouched() {
    let (mgr, db) = sled_manager_with_db();
    let owner = spelling_a(1);
    let orphaned = active_listing(&mgr, owner.clone(), "orphaned").id;
    let healthy = active_listing(&mgr, owner, "healthy").id;

    mgr.express_interest(
        orphaned,
        spelling_a(23),
        "coop".to_string(),
        "x".to_string(),
        None,
    )
    .unwrap();
    mgr.express_interest(
        healthy,
        spelling_a(24),
        "coop".to_string(),
        "y".to_string(),
        None,
    )
    .unwrap();
    for key in primary_rows(&db, &orphaned) {
        db.remove(key.as_bytes()).unwrap();
    }

    assert_eq!(mgr.cleanup_orphaned_interest_indexes().unwrap(), 1);
    assert!(index_rows(&db, &orphaned).is_empty());
    assert_eq!(
        index_rows(&db, &healthy).len(),
        1,
        "the other listing's rows are not this pass's business"
    );
}

#[test]
fn cleanup_skips_a_malformed_row_rather_than_deleting_it() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "malformed").id;

    // No framing separator at all, an unparsable listing component, and a
    // recognisable key carrying a value the writer never writes.
    db.insert(b"v1:interest_idx:no-framing".as_slice(), b"1".as_slice())
        .unwrap();
    db.insert(
        b"v1:interest_idx:not-a-uuid:did:icn:whatever".as_slice(),
        b"1".as_slice(),
    )
    .unwrap();
    forge_index_row(&db, &listing, spelling_a(25).as_str(), b"0");

    assert_eq!(
        mgr.cleanup_orphaned_interest_indexes().unwrap(),
        0,
        "deleting evidence this pass cannot read is not repair"
    );
    assert_eq!(
        db.scan_prefix(b"v1:interest_idx:".as_slice()).count(),
        3,
        "every malformed row must survive"
    );
}

/// The DID suffix carries its own `:` framing, and the parser must hand all of
/// it to the spelling. Pinned across every multibase spelling the fixtures use,
/// because their bodies differ in length and alphabet.
#[test]
fn cleanup_parses_a_did_bearing_key_whole_for_every_spelling() {
    for did in [spelling_a(26), spelling_b(26), spelling_c(26)] {
        let (mgr, db) = sled_manager_with_db();
        let listing = active_listing(&mgr, spelling_a(1), "spellings").id;

        // A canonical interest spelled exactly as the row will be.
        let interest = ListingInterest::new(
            listing,
            did.clone(),
            "coop".to_string(),
            "x".to_string(),
            None,
            1,
        );
        db.insert(
            format!("v1:interest:{listing}:{}", interest.id).as_bytes(),
            icn_encoding::encode_versioned(&interest).unwrap(),
        )
        .unwrap();
        forge_index_row(&db, &listing, did.as_str(), b"1");

        assert_eq!(
            mgr.cleanup_orphaned_interest_indexes().unwrap(),
            0,
            "spelling {did} was mis-parsed: its backed row was called an orphan"
        );
        assert_eq!(index_rows(&db, &listing).len(), 1);
    }
}

/// Cleanup matches the writer's contract — index `A` implies a primary spelled
/// `A` — so a row whose own primary is gone is an orphan even when a
/// differently spelled interest for the same principal survives. Matching by
/// principal instead would retain a row the writer never backed.
#[test]
fn cleanup_matches_the_exact_spelling_the_writer_filed() {
    let (mgr, db) = sled_manager_with_db();
    let listing = active_listing(&mgr, spelling_a(1), "spelling contract").id;
    let a = spelling_a(27);
    let b = spelling_b(27);

    // A canonical interest spelled A, plus a lock row spelled B. Two spellings
    // of one principal: a state M4b refuses to create, and does not reconcile
    // where it already exists.
    let interest = ListingInterest::new(listing, a, "coop".to_string(), "x".to_string(), None, 1);
    db.insert(
        format!("v1:interest:{listing}:{}", interest.id).as_bytes(),
        icn_encoding::encode_versioned(&interest).unwrap(),
    )
    .unwrap();
    forge_index_row(&db, &listing, b.as_str(), b"1");

    assert_eq!(
        mgr.cleanup_orphaned_interest_indexes().unwrap(),
        1,
        "the B row has no primary spelled B, so it is orphaned"
    );
    assert_eq!(
        primary_rows(&db, &listing).len(),
        1,
        "and the canonical interest is untouched — no historical dedupe"
    );
}

// ===========================================================================
// Concurrency
// ===========================================================================

/// Race two requests into one listing from real OS threads released together,
/// and report how many were accepted.
///
/// The listing is pre-loaded with `filler` interests from unrelated principals
/// so the uniqueness classification each thread performs is a scan of real
/// length: the window a naive check leaves open is then wide enough that both
/// threads reliably enter it, rather than depending on scheduler luck at an
/// empty prefix.
fn race_two(first: Did, second: Did, filler: usize) -> (usize, usize) {
    let mgr = Arc::new(sled_manager());
    let listing = active_listing(&mgr, spelling_a(1), "race").id;
    for i in 0..filler {
        // Any 32 bytes is a valid signing-key seed and the verifying key it
        // derives is always a valid point, so every filler principal is
        // distinct and none is silently skipped.
        let mut seed = [7u8; 32];
        seed[0] = (i % 251) as u8;
        seed[1] = (i / 251) as u8;
        let did =
            Did::from_public_key(&ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key());
        mgr.express_interest(listing, did, "coop".to_string(), "filler".to_string(), None)
            .unwrap();
    }
    assert_eq!(
        mgr.get_interests(&listing).unwrap().len(),
        filler,
        "the filler must actually be on disk: it is what gives the uniqueness \
         classification enough length for both threads to enter the window"
    );

    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [first, second]
        .into_iter()
        .map(|did| {
            let mgr = Arc::clone(&mgr);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                mgr.express_interest(listing, did, "coop".to_string(), "racing".to_string(), None)
                    .is_ok()
            })
        })
        .collect();

    let accepted = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    let stored = mgr.get_interests(&listing).unwrap();
    let from_racers = stored.iter().filter(|i| i.message == "racing").count();
    (accepted, from_racers)
}

/// The pre-existing property: the same spelling twice, concurrently, is decided
/// by the sled compare-and-swap. M4b must not weaken it.
#[test]
fn concurrent_same_spelling_requests_yield_one_interest() {
    for _ in 0..8 {
        let (accepted, stored) = race_two(spelling_a(30), spelling_a(30), 60);
        assert_eq!(accepted, 1, "exactly one request may be accepted");
        assert_eq!(stored, 1, "and exactly one canonical interest may exist");
    }
}

/// The M4b race. Two spellings of one principal are two different physical CAS
/// keys, so the storage layer cannot decide this one on its own: both CASes
/// succeed. Exclusion comes from serialising the classification with the write.
#[test]
fn concurrent_alias_spellings_of_one_principal_yield_one_interest() {
    for attempt in 0..8 {
        let (accepted, stored) = race_two(spelling_a(31), spelling_b(31), 60);
        assert_eq!(
            accepted, 1,
            "attempt {attempt}: exactly one of two alias-spelled requests may win"
        );
        assert_eq!(
            stored, 1,
            "attempt {attempt}: one principal, one canonical interest"
        );
    }
}

/// And two genuinely different principals racing must both be served: the
/// serialisation may order them, never exclude one.
#[test]
fn concurrent_distinct_principals_both_succeed() {
    for _ in 0..8 {
        let (accepted, stored) = race_two(spelling_a(32), spelling_a(33), 60);
        assert_eq!(accepted, 2, "both principals must be accepted");
        assert_eq!(stored, 2);
    }
}

// ===========================================================================
// The bounds of the guarantee, stated as tests
// ===========================================================================

/// The alias guard is serialised per store instance, and that is a whole
/// database because sled holds an exclusive lock on the file it opens. Pinned
/// here because the guard's scope argument rests on it.
#[test]
fn sled_refuses_a_second_open_of_one_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gateway_store");
    let held = sled::open(&path).unwrap();
    assert!(
        sled::open(&path).is_err(),
        "a second open of a held sled database must fail; the single-instance \
         argument for the uniqueness lock depends on it"
    );
    drop(held);
}
