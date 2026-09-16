//! One principal, one weak Commons holder (#2627 M3).
//!
//! `update_display_name` mints a weak `CommonsHolderRecord` when no holder is
//! found for the DID it is handed. Before M3 that "not found" was decided by a
//! single exact-key `get` on `commons/holders/by_did/<spelling>`, while every
//! gate upstream of it — the self-service check in
//! `api/members.rs::update_member_profile` and the cooperative-membership check
//! beside it — compares `Did`s, which name principals since I7. A second
//! textual spelling of one principal therefore passed authorization and missed
//! the existence test, and the miss minted a second durable holder whose id is
//! `SHA-256(spelling)` and so differs from the first (inventory rows #65, #67).
//!
//! These fixtures pin the seam every production caller of
//! `update_display_name` reaches: both gateway callers delegate through
//! `CommonsManager` and `CommonsHandle` to `CommonsInner::update_display_name`.

// Test-only: assertions and fixture setup panic on failure by design.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use icn_commons::store::{CommonsStoreBackend, InMemoryCommonsStore, HOLDER_BY_DID_PREFIX};
use icn_commons::{CommonsHandle, CommonsInner};
use icn_identity::Did;

/// A principal, spelled the way `Did::from_public_key` spells it (base58btc).
fn principal(seed: u8) -> Did {
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    Did::from_public_key(&signing_key.verifying_key())
}

/// A second, equally valid textual encoding of the principal `did` names.
///
/// `did:icn:` identifiers are multibase, so the same 32 bytes have a base58btc
/// spelling and a base16 spelling. Both parse; both decode to one identifier.
/// This is the construction the ledger, federation and CCL I7 proofs use.
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

/// Every physical `commons/holders/by_did/` row, as (textual suffix, holder id).
fn by_did_rows(store: &Arc<dyn CommonsStoreBackend>) -> Vec<(String, String)> {
    store
        .scan(HOLDER_BY_DID_PREFIX)
        .expect("the holder-by-DID namespace is readable")
        .into_iter()
        .map(|(key, value)| {
            (
                String::from_utf8(key[HOLDER_BY_DID_PREFIX.len()..].to_vec()).unwrap(),
                String::from_utf8(value).unwrap(),
            )
        })
        .collect()
}

/// The defect: an alternate spelling of an already-held principal must not mint
/// a second weak holder.
#[tokio::test]
async fn an_alternate_spelling_does_not_mint_a_second_weak_holder() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());

    let a = principal(1);
    let b = alternate_spelling(&a);

    inner
        .update_display_name(&a, "First".to_string())
        .await
        .expect("the first spelling mints a weak holder");

    let after_first = by_did_rows(&store);
    assert_eq!(after_first.len(), 1, "one spelling, one index row");

    // Before M3 this call found no row at `by_did/<b>`, inferred the principal
    // absent, and minted a holder whose id is SHA-256(b) — a second durable
    // holder for one principal.
    let err = inner
        .update_display_name(&b, "Second".to_string())
        .await
        .expect_err("an alternate spelling of a held principal must refuse");

    assert_eq!(
        by_did_rows(&store),
        after_first,
        "a refusal must leave the namespace byte-for-byte as it found it"
    );

    let reason = err.to_string();
    assert!(
        reason.contains("holder_principal_already_indexed"),
        "the refusal names its reason class, got: {reason}"
    );
    assert!(
        !reason.contains(a.as_str()) && !reason.contains(b.as_str()),
        "no textual DID may reach a diagnostic"
    );
    assert!(
        !reason.contains("First") && !reason.contains("Second"),
        "no display name may reach a diagnostic"
    );
}

/// Control: the ordinary same-spelling update stays idempotent.
#[tokio::test]
async fn the_same_spelling_still_updates_one_holder_in_place() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(2);

    inner
        .update_display_name(&a, "First".to_string())
        .await
        .expect("first update mints");
    let first = inner.get_holder_by_did(&a).await.unwrap().unwrap();

    inner
        .update_display_name(&a, "Renamed".to_string())
        .await
        .expect("the same spelling updates in place");
    let second = inner.get_holder_by_did(&a).await.unwrap().unwrap();

    assert_eq!(first.id(), second.id(), "holder id unchanged");
    assert_eq!(by_did_rows(&store).len(), 1, "holder count unchanged");
    assert_eq!(second.display_name.as_deref(), Some("Renamed"));
}

/// Control: two distinct principals remain independently mintable. M3 is not
/// "one weak holder globally".
#[tokio::test]
async fn distinct_principals_remain_independently_mintable() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());

    let a = principal(3);
    let c = principal(4);
    assert_ne!(a, c, "the two fixtures must name different principals");

    inner
        .update_display_name(&a, "A".to_string())
        .await
        .unwrap();
    inner
        .update_display_name(&c, "C".to_string())
        .await
        .unwrap();

    let rows = by_did_rows(&store);
    assert_eq!(rows.len(), 2, "two principals, two holders");
    let holder_a = inner.get_holder_by_did(&a).await.unwrap().unwrap();
    let holder_c = inner.get_holder_by_did(&c).await.unwrap().unwrap();
    assert_ne!(holder_a.id(), holder_c.id());
}

/// The writer contract is unchanged: a proven-absent spelling still derives the
/// same holder id it derived before M3. Changing that derivation would be a
/// persisted-identity migration, which M3 is not.
#[tokio::test]
async fn a_proven_absent_spelling_derives_the_same_holder_id_as_before() {
    use sha2::{Digest, Sha256};

    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(5);

    inner
        .update_display_name(&a, "A".to_string())
        .await
        .unwrap();

    let holder = inner.get_holder_by_did(&a).await.unwrap().unwrap();
    let expected: [u8; 32] = Sha256::digest(a.to_string().as_bytes()).into();

    assert_eq!(
        holder.id(),
        &expected,
        "holder id is SHA-256 of the textual spelling, unchanged by M3"
    );
    assert_eq!(holder.anchor_id, expected, "anchor id unchanged by M3");
    assert_eq!(holder.holder_did, a);
    assert_eq!(
        holder.personhood_level,
        icn_identity::POPLevel::Weak,
        "weak level unchanged by M3"
    );
    assert_eq!(
        by_did_rows(&store)[0].1,
        hex::encode(expected),
        "the index still values the row with the hex holder id"
    );
}

/// Two profile updates naming one principal under two spellings, issued from
/// two independently scheduled tasks against the production ownership path.
///
/// What this does and does not establish. `CommonsHandle` serializes every
/// mutation through one write lock, and `CommonsInner::update_display_name`
/// contains no `.await` between classifying and writing — so once a task holds
/// the guard it runs the check and the write to completion, and there is no
/// interleaving window to close. The guard is concurrency-correct *because of
/// that*, not because it defends a race.
///
/// So this fixture is an outcome check, not a race detector: under real
/// multi-threaded scheduling and genuine lock contention (both tasks are
/// spawned and released together by a barrier), exactly one of two
/// same-Principal mints succeeds. It failed before the guard existed, but it
/// failed for the plain alias reason the fixture above states — not because a
/// race was observed. No new synchronization primitive was added for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_alias_updates_mint_at_most_one_holder() {
    let handle = CommonsHandle::new_in_memory();
    let a = principal(6);
    let b = alternate_spelling(&a);

    // Both tasks are parked here until the other has also reached it, so the
    // two `update_display_name` calls contend for the write lock rather than
    // being serialized by the scheduler happening to start one much earlier.
    let gate = Arc::new(tokio::sync::Barrier::new(2));

    let first = tokio::spawn({
        let (handle, a, gate) = (handle.clone(), a.clone(), gate.clone());
        async move {
            gate.wait().await;
            handle.update_display_name(&a, "A".to_string()).await
        }
    });
    let second = tokio::spawn({
        let (handle, b, gate) = (handle.clone(), b.clone(), gate.clone());
        async move {
            gate.wait().await;
            handle.update_display_name(&b, "B".to_string()).await
        }
    });

    let first = first.await.expect("the first task ran to completion");
    let second = second.await.expect("the second task ran to completion");

    assert_eq!(
        [first.is_ok(), second.is_ok()]
            .iter()
            .filter(|ok| **ok)
            .count(),
        1,
        "exactly one of two same-principal mints may succeed"
    );

    let held_a = handle.get_holder_by_did(&a).await.unwrap();
    let held_b = handle.get_holder_by_did(&b).await.unwrap();
    assert_eq!(
        [held_a.is_some(), held_b.is_some()]
            .iter()
            .filter(|held| **held)
            .count(),
        1,
        "exactly one spelling names a holder afterwards; a holder is reachable \
         only through its own by_did row, so one row is one holder"
    );
}

// ============================================================================
// Evidence the guard must refuse to read as absence
// ============================================================================

/// Plant a raw `commons/holders/by_did/` row, bypassing `put_holder`, so the
/// fixture can build namespace states the writer never produces.
fn plant_index_row(store: &Arc<dyn CommonsStoreBackend>, suffix: &[u8], value: &[u8]) {
    let mut key = HOLDER_BY_DID_PREFIX.to_vec();
    key.extend_from_slice(suffix);
    store.put(&key, value).expect("fixture row is storable");
}

/// Every physical `commons/holders/` row, index rows included — the count a
/// refusal must leave untouched.
fn all_holder_rows(store: &Arc<dyn CommonsStoreBackend>) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut rows = store
        .scan(icn_commons::store::HOLDER_PREFIX)
        .expect("the holder namespace is readable");
    rows.sort();
    rows
}

fn refusal_reason(err: &anyhow::Error) -> String {
    err.to_string()
}

/// §11: an index row for this principal whose primary cannot be proven is not
/// absence. Minting would write a replacement over durable evidence.
#[tokio::test]
async fn an_index_row_whose_primary_is_missing_refuses_instead_of_minting_over_it() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(7);

    // `by_did/A -> G`, with no `commons/holders/G`. `get_holder_by_did` reads
    // this as a plain miss.
    plant_index_row(
        &store,
        a.to_string().as_bytes(),
        hex::encode([0u8; 32]).as_bytes(),
    );
    assert!(
        inner.get_holder_by_did(&a).await.unwrap().is_none(),
        "the orphaned index row looks like absence to the ordinary lookup"
    );
    let before = all_holder_rows(&store);

    let err = inner
        .update_display_name(&a, "First".to_string())
        .await
        .expect_err("an unprovable primary is not proof of absence");

    assert_eq!(
        refusal_reason(&err),
        "holder_index_primary_missing",
        "the refusal names its reason class"
    );
    assert_eq!(
        all_holder_rows(&store),
        before,
        "the orphan is left exactly as found — M3 repairs nothing"
    );
}

/// §12: an index row that resolves to a primary carrying a *different*
/// principal is not valid mint-absence evidence, and the projection must not
/// manufacture the identity its primary denies.
#[tokio::test]
async fn an_index_row_crossed_to_another_principals_primary_refuses() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(8);
    let c = principal(9);
    assert_ne!(a, c);

    inner
        .update_display_name(&c, "C".to_string())
        .await
        .unwrap();
    let held_c = inner.get_holder_by_did(&c).await.unwrap().unwrap();

    // `by_did/A` now points at C's primary record.
    plant_index_row(
        &store,
        a.to_string().as_bytes(),
        hex::encode(held_c.id()).as_bytes(),
    );
    let before = all_holder_rows(&store);

    let err = inner
        .update_display_name(&a, "Mallory".to_string())
        .await
        .expect_err("a crossed index row must not be adopted");

    assert_eq!(refusal_reason(&err), "holder_index_primary_mismatch");
    assert_eq!(
        all_holder_rows(&store),
        before,
        "C's holder must not be renamed by a request naming A"
    );
    assert_eq!(
        inner
            .get_holder_by_did(&c)
            .await
            .unwrap()
            .unwrap()
            .display_name
            .as_deref(),
        Some("C"),
        "the other principal's display name is untouched"
    );
}

/// §12, second half: same principal, but the primary is filed under the other
/// spelling. Adoption would silently normalize a row M3 may not rewrite.
#[tokio::test]
async fn an_index_row_whose_primary_carries_another_spelling_refuses() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(10);
    let b = alternate_spelling(&a);

    inner
        .update_display_name(&b, "B".to_string())
        .await
        .unwrap();
    let held_b = inner.get_holder_by_did(&b).await.unwrap().unwrap();

    plant_index_row(
        &store,
        a.to_string().as_bytes(),
        hex::encode(held_b.id()).as_bytes(),
    );
    let before = all_holder_rows(&store);

    let err = inner
        .update_display_name(&a, "A".to_string())
        .await
        .expect_err("one principal spelled two ways is still no adoption rule");

    assert_eq!(refusal_reason(&err), "holder_index_primary_mismatch");
    assert_eq!(
        all_holder_rows(&store),
        before,
        "no row is re-filed under the body's spelling"
    );
}

/// §13: a row the namespace cannot classify blocks the *existential* proof.
/// Absence of one principal is a claim about every row, so an unreadable row is
/// unreadable evidence, never absent evidence.
#[tokio::test]
async fn malformed_namespace_evidence_cannot_be_read_as_absence() {
    for (label, suffix) in [
        ("invalid utf-8", vec![0xff, 0xfe, 0xfd]),
        ("not a DID", b"not-a-did".to_vec()),
        ("empty suffix", Vec::new()),
        ("valid scheme, undecodable body", b"did:icn:zzz!!!".to_vec()),
    ] {
        let store = backend();
        let inner = CommonsInner::new(store.clone());
        let a = principal(11);

        plant_index_row(&store, &suffix, b"00");
        let before = all_holder_rows(&store);

        let err = match inner.update_display_name(&a, "First".to_string()).await {
            Err(err) => err,
            Ok(()) => panic!("{label}: unreadable evidence must refuse, not mint"),
        };

        assert_eq!(
            refusal_reason(&err),
            "holder_index_malformed",
            "{label}: the refusal names its reason class"
        );
        assert_eq!(
            all_holder_rows(&store),
            before,
            "{label}: no row is added, repaired or removed"
        );
    }
}

/// §13: a row under this principal's own spelling whose *value* is not a
/// readable holder id is equally unreadable.
///
/// The value must be the shape `put_holder` writes — 64 lowercase hex digits.
/// A value that is merely UTF-8 would otherwise fail to resolve and be reported
/// as a *stale* index whose primary is missing, which is a different defect
/// with a different remedy: a dangling reference is not an unreadable one.
#[tokio::test]
async fn a_malformed_holder_id_value_cannot_be_read_as_absence() {
    for (label, value) in [
        ("invalid utf-8", vec![0xff, 0xfe]),
        ("not hex at all", b"not-a-holder-id".to_vec()),
        ("uppercase hex", "AB".repeat(32).into_bytes()),
        ("hex, wrong length", "ab".repeat(16).into_bytes()),
        ("empty", Vec::new()),
    ] {
        let store = backend();
        let inner = CommonsInner::new(store.clone());
        let a = principal(12);

        plant_index_row(&store, a.to_string().as_bytes(), &value);
        let before = all_holder_rows(&store);

        let err = match inner.update_display_name(&a, "First".to_string()).await {
            Err(err) => err,
            Ok(()) => panic!("{label}: an unreadable holder id must refuse, not mint"),
        };

        assert_eq!(
            refusal_reason(&err),
            "holder_index_malformed",
            "{label}: an unreadable value is not a dangling reference"
        );
        assert_eq!(all_holder_rows(&store), before, "{label}: nothing written");
    }
}

/// §27: every refusal is bounded and payload-free. A diagnostic that reproduced
/// the spelling, the display name or the row bytes would leak the member
/// identity the refusal exists to protect.
#[tokio::test]
async fn refusal_diagnostics_are_bounded_and_payload_free() {
    let store = backend();
    let inner = CommonsInner::new(store.clone());
    let a = principal(13);
    let b = alternate_spelling(&a);
    let secret_name = "Ada Lovelace";

    inner
        .update_display_name(&a, secret_name.to_string())
        .await
        .unwrap();
    let err = inner
        .update_display_name(&b, "Second".to_string())
        .await
        .unwrap_err();
    let reason = refusal_reason(&err);

    assert_eq!(reason, "holder_principal_already_indexed");
    assert!(
        reason.len() <= 64,
        "reason class stays short: {}",
        reason.len()
    );
    // The values are named rather than interpolated: an assertion message that
    // printed the very thing it checks for absence would put that value into
    // test output, which is the leak this test exists to rule out.
    for (what, leak) in [
        ("the requested spelling", a.as_str()),
        ("the alias spelling", b.as_str()),
        ("the display name", secret_name),
        ("the DID scheme", "did:icn:"),
        ("the physical key prefix", "commons/holders"),
    ] {
        assert!(
            !reason.contains(leak),
            "the diagnostic must not carry {what}"
        );
    }
}
