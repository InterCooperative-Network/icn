//! One principal, one personhood anchor (#2627 M4a).
//!
//! `create_anchor_from_enrollment` is the single production constructor of a
//! `PersonhoodAnchor`, reached from two production routes — SDIS enrollment
//! (`api/sdis/simple_enrollment.rs::complete_enrollment`) and the dev standing
//! bootstrap (`api/commons/mod.rs`) — plus the `commons_restart_helper`
//! test-only binary, which ships but is driven only by
//! `commons_persistence.rs`. Before M4a
//! it decided existence with `if self.store.get_anchor(&anchor_id)?.is_some()`
//! — where `anchor_id` had just been derived from a *fresh random* `genesis`,
//! so the check could never fire — and its callers decided existence with
//! `get_anchor_by_did`, a single exact-key `get` that proves only that one
//! spelling is unused.
//!
//! The consequence is inventory row #66. Two enrollments of one principal
//! produce two independent anchors, and because `CommonsHolderRecord`'s id is
//! the anchor id verbatim (`icn-identity::commons`, "Holder ID is derived from
//! anchor_id"), two anchors are also two holders — the seam M3 (§11.7) left
//! open at `commons/holders/by_did/`.
//!
//! The rule enforced is the repository's own, not one invented for N2-A:
//! `api/sdis/recovery.rs` states recovery "allows rotating to a new KeyBundle
//! while keeping the same Anchor", and `complete_enrollment` already refuses a
//! repeat with "This identity has already been enrolled (VUI collision)" — on
//! a VUI computed as `SHA-256(did.to_string())`, which is spelling-derived and
//! so cannot see an alias at all.

// Test-only: assertions and fixture setup panic on failure by design.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use icn_commons::store::{
    AnchorEnrollmentClassification, AnchorIndexDefect, CommonsStore, CommonsStoreBackend,
    InMemoryCommonsStore, ANCHOR_BY_DID_PREFIX, ANCHOR_PREFIX, HOLDER_BY_DID_PREFIX, HOLDER_PREFIX,
};
use icn_commons::CommonsInner;
use icn_identity::{identifier_bytes_of_spelling, Did};

/// A principal, spelled the way `Did::from_public_key` spells it (base58btc).
fn principal(seed: u8) -> Did {
    Did::from_public_key(&SigningKey::from_bytes(&[seed; 32]).verifying_key())
}

/// A second, equally valid textual encoding of the principal `did` names.
///
/// `did:icn:` identifiers are multibase, so the same 32 bytes have a base58btc
/// spelling and a base16 spelling. Both parse; both decode to one identifier.
/// This is the construction every other N2-A proof uses.
fn alternate_spelling(did: &Did) -> Did {
    let bytes = did.identifier_bytes().unwrap();
    let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes))).unwrap();
    assert_ne!(
        did.as_str(),
        alias.as_str(),
        "the two spellings must differ, or the test proves nothing"
    );
    assert_eq!(did, &alias, "the two spellings must name one principal");
    alias
}

fn backend() -> Arc<dyn CommonsStoreBackend> {
    Arc::new(InMemoryCommonsStore::new())
}

/// Rows directly under a prefix, excluding the `by_*` children nested beneath
/// it. `commons/anchors/` is a lexical *prefix of* `commons/anchors/by_did/`,
/// so a naive scan of the primary prefix would count index rows as primaries
/// and report a store as healthier than it is.
fn primary_rows(store: &Arc<dyn CommonsStoreBackend>, prefix: &[u8]) -> Vec<String> {
    store
        .scan(prefix)
        .expect("the namespace is readable")
        .into_iter()
        .map(|(key, _)| String::from_utf8_lossy(&key).into_owned())
        .filter(|k| !k.contains("/by_did/") && !k.contains("/by_anchor/"))
        .collect()
}

/// Every physical row under a prefix, as (textual suffix, value).
fn rows(store: &Arc<dyn CommonsStoreBackend>, prefix: &[u8]) -> Vec<(String, String)> {
    store
        .scan(prefix)
        .expect("the namespace is readable")
        .into_iter()
        .map(|(key, value)| {
            (
                String::from_utf8_lossy(&key[prefix.len()..]).into_owned(),
                String::from_utf8_lossy(&value).into_owned(),
            )
        })
        .collect()
}

// ============================================================================
// The defect, and its closure
// ============================================================================

/// A single healthy enrollment writes TWO anchor-by-DID rows, and that is not
/// a collision.
///
/// This is the control the refusal depends on: if the two rows one enrollment
/// writes named one principal, the descriptor registered for this namespace
/// would refuse every healthy store at startup. They do not — `put_anchor`
/// files the anchor under `anchor.to_did()`, a function of the random anchor
/// id, while `put_anchor_did_index` files it under the enrollment spelling.
#[tokio::test]
async fn one_enrollment_writes_two_index_rows_naming_two_different_principals() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let did = principal(11);

    let anchor = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect("the first enrollment of a principal is permitted");
    let anchor_id = hex::encode(anchor.id());

    let index = rows(&store, ANCHOR_BY_DID_PREFIX);
    assert_eq!(
        index.len(),
        2,
        "one enrollment writes two index rows: {index:?}"
    );
    assert!(
        index.iter().all(|(_, id)| id == &anchor_id),
        "both rows point at the one anchor: {index:?}"
    );

    // Compared by decoded identifier bytes, not by `Did::from_str`: the
    // anchor-derived row is `Did::from_anchor_id` over a SHA-256 anchor id,
    // which is not a valid Ed25519 point, so `from_str` rejects it. This is
    // the same decode the runtime guard and the collision scanner use.
    let bytes: Vec<[u8; 32]> = index
        .iter()
        .map(|(s, _)| identifier_bytes_of_spelling(s).expect("each row's suffix names a principal"))
        .collect();
    assert_ne!(
        bytes[0], bytes[1],
        "the two rows must name two different principals, or a healthy store \
         would read as a collision"
    );
    assert!(
        bytes.contains(&did.identifier_bytes().unwrap()),
        "one row is the enrollment spelling: {index:?}"
    );
    assert!(
        bytes.contains(&anchor.to_did().identifier_bytes().unwrap()),
        "the other is the anchor's own derived DID: {index:?}"
    );
    assert_eq!(primary_rows(&store, ANCHOR_PREFIX).len(), 1);
}

/// An anchor-derived row that is *not* a valid Ed25519 point must not be read
/// as unreadable evidence.
///
/// `Did::from_anchor_id` builds its DID with `new_unchecked` over a SHA-256
/// anchor id, so whether the result happens to decompress to a curve point is
/// an accident of hashing — roughly a coin flip per anchor, which is why this
/// is pinned with a searched id rather than an enrolled one. A classifier
/// built on `Did::from_str` would report this healthy row as `MalformedRow`
/// and refuse every subsequent enrollment on the node.
#[tokio::test]
async fn an_anchor_derived_row_that_is_not_a_curve_point_is_still_readable() {
    let store = backend();
    // A fixed, exhaustive search over a fixed space: deterministic, and it
    // documents that such ids exist rather than asserting a guessed constant.
    let unparseable = (0u8..=255)
        .map(|n| Did::from_anchor_id(&[n; 32]))
        .find(|d| Did::from_str(d.as_str()).is_err())
        .expect("some 32-byte anchor id is not a valid Ed25519 point");
    assert!(
        identifier_bytes_of_spelling(unparseable.as_str()).is_ok(),
        "but the decode the guard and the scanner share still resolves it"
    );

    let mut key = ANCHOR_BY_DID_PREFIX.to_vec();
    key.extend_from_slice(unparseable.as_str().as_bytes());
    store
        .put(
            &key,
            hex::encode(unparseable.identifier_bytes().unwrap()).as_bytes(),
        )
        .expect("write the anchor-derived row");

    let commons = CommonsStore::new(store.clone());
    assert!(
        matches!(
            commons.classify_anchor_enrollment(&principal(51)).unwrap(),
            AnchorEnrollmentClassification::ProvenAbsent
        ),
        "an unrelated principal is proven absent, not blocked by a healthy row"
    );
}

/// The M4a defect: an alias spelling enrols a second personhood anchor, and
/// with it a second holder.
///
/// Observed on unchanged `main` before the guard: two anchor primaries, four
/// anchor-by-DID rows, two holder primaries and two holder-by-DID rows for one
/// principal.
#[tokio::test]
async fn an_alias_spelling_cannot_enrol_a_second_anchor() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(7);
    let b = alternate_spelling(&a);

    let first = inner
        .create_anchor_from_enrollment(&a, None)
        .await
        .expect("the first enrollment is permitted");
    let first_id = hex::encode(first.id());

    let before_anchors = primary_rows(&store, ANCHOR_PREFIX);
    let before_index = rows(&store, ANCHOR_BY_DID_PREFIX);

    let refused = inner.create_anchor_from_enrollment(&b, None).await;
    let err = refused.expect_err("an alias spelling must not enrol a second anchor");
    assert_eq!(
        err.to_string(),
        "anchor_principal_already_enrolled",
        "the refusal is a payload-free reason class, not a spelling"
    );

    // Nothing was created, and nothing was rewritten.
    assert_eq!(
        primary_rows(&store, ANCHOR_PREFIX),
        before_anchors,
        "no second anchor primary, and no orphan from a partial write"
    );
    assert_eq!(
        rows(&store, ANCHOR_BY_DID_PREFIX),
        before_index,
        "no index row added, removed, or re-pointed"
    );
    assert!(
        inner
            .get_anchor(&first_id)
            .await
            .expect("the first anchor is readable")
            .is_some(),
        "the first anchor survives untouched"
    );
}

/// The refusal is not spelling-shaped: a repeat of the *same* spelling is
/// refused on the same evidence.
///
/// This case is not an I7 defect — it is reachable on any tree — but a guard
/// that refused only the alias would be defeated by resubmitting the original
/// spelling, so the Principal-level question subsumes it. Before M4a the
/// repeat also silently re-pointed `commons/anchors/by_did/<spelling>` and
/// `commons/holders/by_did/<spelling>` at the new records, orphaning the
/// first anchor and holder.
#[tokio::test]
async fn a_repeat_of_the_same_spelling_is_refused_on_the_same_evidence() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let did = principal(9);

    inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect("the first enrollment is permitted");

    let before_anchors = primary_rows(&store, ANCHOR_PREFIX);
    let before_index = rows(&store, ANCHOR_BY_DID_PREFIX);

    let err = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect_err("a second enrollment of one principal must be refused");
    assert_eq!(err.to_string(), "anchor_principal_already_enrolled");

    assert_eq!(primary_rows(&store, ANCHOR_PREFIX), before_anchors);
    assert_eq!(
        rows(&store, ANCHOR_BY_DID_PREFIX),
        before_index,
        "the index row still points at the first anchor, not a replacement"
    );
}

/// The holder half follows for free, because the holder id *is* the anchor id.
///
/// M3 (§11.7) closed the profile-update mint seam and left this one open. The
/// guard is at the anchor, so no second `CommonsHolderRecord` can be derived
/// either — the enrollment stops before an anchor exists to derive one from.
#[tokio::test]
async fn refusing_the_anchor_leaves_no_second_holder_to_derive() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(13);
    let b = alternate_spelling(&a);

    let anchor = inner.create_anchor_from_enrollment(&a, None).await.unwrap();
    let anchor_id = hex::encode(anchor.id());
    let holder = inner
        .get_or_create_holder(&anchor_id, &a, Some("first".into()))
        .await
        .expect("the first holder is minted from the first anchor");
    assert_eq!(
        hex::encode(holder.id()),
        anchor_id,
        "the holder id is the anchor id verbatim"
    );

    assert!(inner.create_anchor_from_enrollment(&b, None).await.is_err());

    assert_eq!(
        primary_rows(&store, HOLDER_PREFIX).len(),
        1,
        "one principal, one holder"
    );
    assert_eq!(rows(&store, HOLDER_BY_DID_PREFIX).len(), 1);
}

// ============================================================================
// Controls
// ============================================================================

/// A distinct principal enrols normally. The refusal must be about identity,
/// not about the namespace being non-empty.
#[tokio::test]
async fn a_distinct_principal_still_enrols() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());

    inner
        .create_anchor_from_enrollment(&principal(1), None)
        .await
        .expect("first principal enrols");
    inner
        .create_anchor_from_enrollment(&principal(2), None)
        .await
        .expect("a different principal is not blocked by the first");

    assert_eq!(primary_rows(&store, ANCHOR_PREFIX).len(), 2);
    assert_eq!(rows(&store, ANCHOR_BY_DID_PREFIX).len(), 4);
}

/// Malformed physical evidence refuses; it never reads as absence.
///
/// A row whose key suffix is not a DID names *some* principal this scan cannot
/// identify, and therefore cannot rule out as the one enrolling.
#[tokio::test]
async fn a_malformed_index_row_refuses_rather_than_reading_as_absence() {
    let store = backend();
    let did = principal(21);

    let mut key = ANCHOR_BY_DID_PREFIX.to_vec();
    key.extend_from_slice(b"not-a-did");
    store.put(&key, b"00").expect("write the malformed row");

    let commons = CommonsStore::new(store.clone());
    assert!(matches!(
        commons.classify_anchor_enrollment(&did).unwrap(),
        AnchorEnrollmentClassification::Unreadable(AnchorIndexDefect::MalformedRow)
    ));

    let inner = CommonsInner::new(store.clone());
    let err = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect_err("unreadable evidence must refuse");
    assert_eq!(err.to_string(), "anchor_index_malformed");
    assert!(primary_rows(&store, ANCHOR_PREFIX).is_empty());
}

/// A value that is UTF-8 but not an anchor id is a malformed row, not a
/// dangling reference. The two are different defects and an operator acts on
/// them differently.
#[tokio::test]
async fn an_index_value_that_is_not_an_anchor_id_is_malformed_not_dangling() {
    let store = backend();
    let did = principal(23);

    let mut key = ANCHOR_BY_DID_PREFIX.to_vec();
    key.extend_from_slice(did.as_str().as_bytes());
    store.put(&key, b"not-hex").expect("write the row");

    let commons = CommonsStore::new(store.clone());
    assert!(matches!(
        commons.classify_anchor_enrollment(&did).unwrap(),
        AnchorEnrollmentClassification::Unreadable(AnchorIndexDefect::MalformedRow)
    ));
}

/// A well-formed index row whose primary is absent is a dangling reference.
///
/// `delete_anchor` is the shape that produces it — it removes only the anchor's
/// own derived-DID row, so any row written by `put_anchor_did_index` survives
/// the deletion of the anchor it names — but `delete_anchor` has no caller in
/// the workspace, so this is a guard against a wiring that does not exist yet,
/// not a defect observed in a shipped path. It is pinned anyway: enrolling over
/// unprovable evidence would mint a replacement for a durable record this layer
/// has no authority to discard, whichever path later produces it.
#[tokio::test]
async fn a_dangling_index_row_refuses_and_is_distinguished_from_malformed() {
    let store = backend();
    let did = principal(25);

    let mut key = ANCHOR_BY_DID_PREFIX.to_vec();
    key.extend_from_slice(did.as_str().as_bytes());
    store
        .put(&key, hex::encode([0u8; 32]).as_bytes())
        .expect("write a well-formed row pointing at no anchor");

    let commons = CommonsStore::new(store.clone());
    assert!(matches!(
        commons.classify_anchor_enrollment(&did).unwrap(),
        AnchorEnrollmentClassification::Unreadable(AnchorIndexDefect::PrimaryMissing)
    ));

    let inner = CommonsInner::new(store.clone());
    let err = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect_err("a dangling row must refuse");
    assert_eq!(err.to_string(), "anchor_index_primary_missing");
}

/// The classification does not mutate the evidence it reads.
#[tokio::test]
async fn classification_does_not_mutate_the_namespace() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(31);

    inner.create_anchor_from_enrollment(&a, None).await.unwrap();
    let before = rows(&store, ANCHOR_BY_DID_PREFIX);

    let commons = CommonsStore::new(store.clone());
    for did in [&a, &alternate_spelling(&a), &principal(32)] {
        let _ = commons.classify_anchor_enrollment(did).unwrap();
    }

    assert_eq!(
        rows(&store, ANCHOR_BY_DID_PREFIX),
        before,
        "classifying is a read"
    );
}

/// The exact-spelling arm reports the anchor it found, so a caller that wants
/// to *reuse* an existing anchor has a proven id to reuse rather than a miss
/// to mint over.
#[tokio::test]
async fn the_held_arm_names_the_anchor_it_proved() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let did = principal(41);

    let anchor = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();

    let commons = CommonsStore::new(store.clone());
    match commons.classify_anchor_enrollment(&did).unwrap() {
        AnchorEnrollmentClassification::Held { anchor_id } => {
            assert_eq!(anchor_id, hex::encode(anchor.id()));
        }
        other => panic!("expected Held, got {other:?}"),
    }
}

// ============================================================================
// The two lockouts the guard creates, pinned deliberately rather than by
// accident. Both were raised independently by review; neither is repaired here.
// ============================================================================

/// The guard is **status-blind**, and that is the correct outcome for a
/// revoked anchor.
///
/// `classify_anchor_enrollment` asks only whether a primary resolves; it never
/// reads `AnchorStatus`. So a principal whose anchor was revoked cannot enrol
/// again under that key — and `PersonhoodAnchor::reinstate` refuses a revoked
/// anchor outright ("Cannot reinstate a revoked anchor"), so there is no path
/// back. That is a real, permanent lockout.
///
/// It is pinned rather than carved out because the alternative is worse: if
/// revocation released the principal to mint a fresh, clean anchor, revocation
/// would mean nothing. `revoke` is documented "(permanent)" and this keeps it
/// so. What M4a does *not* claim is that this was designed here — the guard
/// acquired the property by asking a status-free question, and this fixture
/// exists so a later change cannot silently drop it.
#[tokio::test]
async fn a_revoked_anchor_still_blocks_re_enrollment_and_that_is_deliberate() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let did = principal(71);

    let anchor = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    let anchor_id = hex::encode(anchor.id());
    inner
        .update_anchor_status(
            &anchor_id,
            icn_identity::AnchorStatus::Revoked {
                reason: "test".into(),
                revoked_at: 0,
                evidence: Vec::new(),
                authority: principal(72),
            },
        )
        .await
        .expect("an anchor can be revoked");

    let err = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect_err("a revoked principal must not mint a fresh clean anchor");
    assert_eq!(err.to_string(), "anchor_principal_already_enrolled");
    assert_eq!(primary_rows(&store, ANCHOR_PREFIX).len(), 1);
}

/// A retry after a *partial* enrollment is refused, and the anchor is not
/// rolled back.
///
/// `complete_enrollment` is deliberately fail-closed after the anchor write, so
/// a later failure — the holder, the jurisdiction join, the membership approval
/// — aborts with the anchor already durable. The session pins the ephemeral DID
/// from level 1, so the retry presents the same principal, reaches `Held`, and
/// is refused: that ceremony cannot complete.
///
/// This is not repaired here. On the steward-manager configuration the retry
/// was already refused one step earlier by the VUI reservation, so the lockout
/// is pre-existing there; M4a extends it to the steward-less configuration,
/// which is exactly the configuration that previously had no duplicate check at
/// all and "recovered" by minting a second anchor. Rolling the anchor back, or
/// letting a retry adopt it, are both writes M4a does not authorize.
#[tokio::test]
async fn a_retry_after_a_partial_enrollment_is_refused_and_nothing_is_rolled_back() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let did = principal(73);

    // The partial state: anchor written, no holder derived from it.
    let anchor = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .unwrap();
    assert!(primary_rows(&store, HOLDER_PREFIX).is_empty());

    let err = inner
        .create_anchor_from_enrollment(&did, None)
        .await
        .expect_err("the retry presents the same principal and is refused");
    assert_eq!(err.to_string(), "anchor_principal_already_enrolled");

    // The anchor survives, and the classification names it — so a caller that
    // is later authorized to adopt a partial enrollment has a proven id to
    // adopt rather than a miss to mint over.
    let commons = CommonsStore::new(store.clone());
    match commons.classify_anchor_enrollment(&did).unwrap() {
        AnchorEnrollmentClassification::Held { anchor_id } => {
            assert_eq!(anchor_id, hex::encode(anchor.id()));
        }
        other => panic!("expected Held, got {other:?}"),
    }
    assert_eq!(primary_rows(&store, ANCHOR_PREFIX).len(), 1);
}
