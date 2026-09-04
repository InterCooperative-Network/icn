//! N2-A M1: the treasury loader classifies before it adopts (#2627).
//!
//! ## What is proven
//!
//! `TreasuryManager::with_store` rebuilds `HashMap<Did, Treasury>` and the
//! coop/entity/budget/rule indexes from `ledger:treasury:<did>` rows. I7
//! (#2686) made `Did` equality and hashing name the *principal*; the persisted
//! keys still name a *spelling*. Before this change the two pre-existing
//! hydration guards compared `Did`s — principal equality — so two spellings of
//! one treasury were not an inconsistency to them: the fold collapsed them at
//! insert, the scan-last row's value survived under the scan-first row's key,
//! and a later write-back rewrote only the survivor (observed live on
//! unchanged `main`, recorded in the PR). These tests write real sled rows and
//! prove the loader now refuses, rather than collapses, whenever the two
//! regimes disagree — and prove each refusal is narrow with a control that
//! differs in exactly one fact.
//!
//! ## What is NOT proven
//!
//! - That any deployed store holds such rows: constructed fixtures only.
//! - Any merge rule. Two treasury records for one principal can disagree
//!   about every field; the registry disposition is `FailClosed` and no
//!   economics owner has authorized a survivor. Refusing is the behaviour.
//! - Anything about the budget, rule, audit, labor-share, bond or allocation
//!   subspaces beyond "they are not primary rows".

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_entity::EntityId;
use icn_identity::{identifier_bytes_of_spelling, Did, KeyPair};
use icn_ledger::principal_rows::{PrincipalRowsRefusal, TREASURY_KEYSPACE};
use icn_ledger::treasury::{Treasury, TreasuryHydrationRefusal, TreasuryManager};
use icn_store::did_collision_scan::{
    audit_store, n2a_deferred_namespaces, n2a_keyspaces, scan_keyspace, KeyspaceDescriptor,
};
use icn_store::{SledStore, Store};
use std::sync::Arc;

// Byte-for-byte as `TreasuryManager`'s own persistence paths build them.
const TREASURY_PREFIX: &str = "ledger:treasury:";
const BUDGET_PREFIX: &str = "ledger:treasury:budget:";
const SPENDING_RULE_PREFIX: &str = "ledger:treasury:rule:";
const TREASURY_AUDIT_PREFIX: &str = "ledger:treasury:audit:";
const TREASURY_IDX_COOP_PREFIX: &str = "ledger:treasury:idx:coop:";
const TREASURY_IDX_BUDGETS_PREFIX: &str = "ledger:treasury:idx:budgets:";
const VELOCITY_LIMIT_PREFIX: &str = "ledger:treasury:vlimit:";

fn a_principal() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

/// The base16-lower spelling (`did:icn:f…`) of the principal `did` names.
///
/// `did:icn:` identifiers are multibase, so one principal has a base58btc
/// spelling (`z…`), a base16-lower spelling (`f…`) and a base16-upper
/// spelling (`F…`) among others. All parse; all decode to one identifier;
/// all are distinct strings, which is the whole hazard.
fn base16_lower(did: &Did) -> Did {
    let alias = Did::from_str(&format!(
        "did:icn:f{}",
        hex::encode(did.identifier_bytes().unwrap())
    ))
    .unwrap();
    assert_ne!(did.as_str(), alias.as_str(), "the spellings must differ");
    alias
}

/// The base16-upper spelling (`did:icn:F…`), which sorts *before* `f…` and
/// `z…` in `Store::scan` order — so it lets the fixtures vary which row of a
/// pair scans last without a third encoding dependency.
fn base16_upper(did: &Did) -> Did {
    let alias = Did::from_str(&format!(
        "did:icn:F{}",
        hex::encode_upper(did.identifier_bytes().unwrap())
    ))
    .unwrap();
    assert_ne!(did.as_str(), alias.as_str(), "the spellings must differ");
    alias
}

fn open_store(dir: &std::path::Path) -> Arc<SledStore> {
    Arc::new(SledStore::open(dir).unwrap())
}

fn record(spelling: &Did, coop_id: &str, description: &str) -> Treasury {
    Treasury::new(
        spelling.clone(),
        coop_id.to_string(),
        "HOURS".to_string(),
        spelling.clone(),
        Some(description.to_string()),
    )
}

/// A primary row exactly as `persist_treasury` writes it: key from the
/// record's own `treasury_did`, value the JSON record.
fn put_primary(store: &Arc<SledStore>, treasury: &Treasury) {
    put_primary_under(store, treasury.treasury_did.as_str(), treasury);
}

/// A primary row under an explicit key spelling, for the key/body fixtures.
fn put_primary_under(store: &Arc<SledStore>, key_spelling: &str, treasury: &Treasury) {
    let key = format!("{TREASURY_PREFIX}{key_spelling}");
    store
        .put(key.as_bytes(), &serde_json::to_vec(treasury).unwrap())
        .unwrap();
}

/// A cooperative index row exactly as `persist_coop_index` writes it.
fn put_coop_index(store: &Arc<SledStore>, coop_id: &str, value: &str) {
    let key = format!("{TREASURY_IDX_COOP_PREFIX}{coop_id}");
    store.put(key.as_bytes(), value.as_bytes()).unwrap();
}

fn rows_under(store: &Arc<SledStore>, prefix: &str) -> Vec<(Vec<u8>, Vec<u8>)> {
    store.scan(prefix.as_bytes()).unwrap()
}

/// The typed refusal behind an opaque `anyhow::Error`, or a panic naming what
/// actually came back — a hydration that succeeded is the regression these
/// tests exist to catch, so it must never read as a pass.
#[derive(Debug)]
enum Refusal {
    Principal(PrincipalRowsRefusal),
    Treasury(TreasuryHydrationRefusal),
}

fn refusal_of(result: anyhow::Result<TreasuryManager>) -> Refusal {
    match result {
        Ok(mgr) => panic!(
            "the loader adopted state it cannot interpret: {} treasur{} survived",
            mgr.list_treasuries().len(),
            if mgr.list_treasuries().len() == 1 {
                "y"
            } else {
                "ies"
            }
        ),
        Err(e) => {
            if let Some(r) = e.downcast_ref::<PrincipalRowsRefusal>() {
                Refusal::Principal(r.clone())
            } else if let Some(r) = e.downcast_ref::<TreasuryHydrationRefusal>() {
                Refusal::Treasury(r.clone())
            } else {
                panic!("refused, but not with a typed refusal: {e}")
            }
        }
    }
}

fn treasury_descriptor() -> KeyspaceDescriptor {
    n2a_keyspaces()
        .into_iter()
        .find(|d| d.name == TREASURY_KEYSPACE)
        .expect("the treasury keyspace is registered in the N2-A scanner")
}

// ── fixture guard ───────────────────────────────────────────────────────────

#[test]
fn fixture_spellings_are_three_distinct_strings_naming_one_principal() {
    let z = a_principal();
    let f = base16_lower(&z);
    let upper = base16_upper(&z);
    assert!(
        z.as_str() > f.as_str() && f.as_str() > upper.as_str(),
        "scan order must be F < f < z for the ordering fixtures to mean anything"
    );
    for spelling in [&f, &upper] {
        assert_eq!(
            identifier_bytes_of_spelling(spelling.as_str()).unwrap(),
            z.identifier_bytes().unwrap()
        );
        assert_eq!(spelling, &z, "one principal under I7");
    }
}

// ── alias pair ──────────────────────────────────────────────────────────────

#[test]
fn two_spellings_of_one_treasury_refuse_hydration_and_touch_no_row() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    // Each row is individually valid: its key is its own spelling, its body
    // agrees with its key, and it names the same cooperative — exactly the
    // shape a re-spelled registration would leave behind.
    put_primary(&store, &record(&z, "food-coop", "row under z"));
    put_primary(&store, &record(&f, "food-coop", "row under f"));
    let before = rows_under(&store, TREASURY_PREFIX);

    let refusal = refusal_of(TreasuryManager::with_store(store.clone()));
    match refusal {
        Refusal::Principal(PrincipalRowsRefusal::AliasCollision { keyspace, groups }) => {
            assert_eq!(keyspace, TREASURY_KEYSPACE);
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].spellings, 2);
            assert_eq!(groups[0].discriminator, "");
        }
        other => panic!("expected AliasCollision, got {other:?}"),
    }

    // Refusal is read-only: both physical spellings survive byte-for-byte.
    assert_eq!(rows_under(&store, TREASURY_PREFIX), before);
    assert_eq!(before.len(), 2);
}

#[test]
fn two_spellings_of_one_treasury_under_two_coop_ids_still_refuse_as_an_alias() {
    // One fact different from the fixture above: the coop ids differ. That
    // is still two treasury records for one principal, and the alias guard
    // must say so before the coop guard gets a chance to say anything else.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    put_primary(&store, &record(&z, "food-coop", "z"));
    put_primary(&store, &record(&f, "housing-coop", "f"));

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::AliasCollision { .. })
    ));
}

#[test]
fn three_spellings_of_one_treasury_are_one_group_of_three() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "z"));
    put_primary(&store, &record(&base16_lower(&z), "food-coop", "f"));
    put_primary(&store, &record(&base16_upper(&z), "food-coop", "F"));

    match refusal_of(TreasuryManager::with_store(store)) {
        Refusal::Principal(PrincipalRowsRefusal::AliasCollision { groups, .. }) => {
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].spellings, 3);
        }
        other => panic!("expected AliasCollision, got {other:?}"),
    }
}

#[test]
fn the_alias_refusal_does_not_depend_on_insertion_or_scan_order() {
    // Before the fix the survivor was the scan-last row (byte-greatest key),
    // so the outcome varied with the pair's encodings. The refusal must not:
    // every ordering of every pair refuses identically.
    let z = a_principal();
    let f = base16_lower(&z);
    let upper = base16_upper(&z);
    for (first, second) in [(&f, &z), (&z, &f), (&upper, &f), (&f, &upper), (&upper, &z)] {
        let tmp = tempfile::tempdir().unwrap();
        let store = open_store(tmp.path());
        put_primary(&store, &record(first, "food-coop", "first written"));
        put_primary(&store, &record(second, "food-coop", "second written"));
        match refusal_of(TreasuryManager::with_store(store)) {
            Refusal::Principal(PrincipalRowsRefusal::AliasCollision { groups, .. }) => {
                assert_eq!(groups.len(), 1);
                assert_eq!(groups[0].spellings, 2);
            }
            other => panic!("expected AliasCollision, got {other:?}"),
        }
    }
}

#[test]
fn control_one_treasury_under_one_spelling_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", z.as_str());

    let mgr = TreasuryManager::with_store(store).expect("one spelling is unambiguous");
    assert_eq!(mgr.list_treasuries().len(), 1);
    assert_eq!(mgr.get_treasury(&z).unwrap().coop_id(), "food-coop");
    assert_eq!(
        mgr.get_treasury_by_coop("food-coop")
            .unwrap()
            .treasury_did
            .as_str(),
        z.as_str(),
        "the coop index resolves to the stored spelling"
    );
    // Under I7 a lookup by any spelling of the principal finds the one row.
    assert!(mgr.is_treasury_account(&base16_lower(&z)));
}

#[test]
fn control_two_different_treasuries_load() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let a = a_principal();
    let b = a_principal();
    put_primary(&store, &record(&a, "food-coop", "a"));
    put_primary(&store, &record(&b, "housing-coop", "b"));

    let mgr = TreasuryManager::with_store(store).expect("two principals are two treasuries");
    assert_eq!(mgr.list_treasuries().len(), 2);
    assert!(mgr.get_treasury_by_coop("food-coop").is_some());
    assert!(mgr.get_treasury_by_coop("housing-coop").is_some());
}

// ── key/body identity ───────────────────────────────────────────────────────

#[test]
fn a_primary_row_whose_key_and_body_spell_the_principal_differently_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    // ONE row, stored under the `f` spelling, whose body names the `z`
    // spelling. Isolated deliberately: two keys would be an alias collision
    // and would refuse for that reason instead, hiding whether the key/body
    // check works at all. Under I7 `key_did == body.treasury_did` is true
    // here, so a guard using `Did` equality adopts it — and every later
    // `persist_treasury` then writes under `z`, opening a second row while the
    // `f` row stays on disk holding the old record.
    put_primary_under(&store, f.as_str(), &record(&z, "food-coop", "masquerade"));

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::KeyValueSpellingMismatch { rows: 1, .. })
    ));
}

#[test]
fn control_a_primary_row_whose_key_and_body_agree_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let f = base16_lower(&a_principal());
    // The same construction with the one fact restored: key `f`, body `f`.
    // A non-base58 spelling is a legal stored spelling; nothing canonicalizes.
    put_primary_under(&store, f.as_str(), &record(&f, "food-coop", "consistent"));

    let mgr = TreasuryManager::with_store(store).expect("key and body agree");
    assert_eq!(
        mgr.get_treasury(&f).unwrap().treasury_did.as_str(),
        f.as_str()
    );
}

// ── unreadable primary rows ─────────────────────────────────────────────────

#[test]
fn a_primary_key_naming_no_principal_refuses_rather_than_vanishing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "valid"));
    // Each carries a perfectly readable value, so a loader that skipped by
    // "does the value parse" would adopt them under invented identities.
    let valid_body = serde_json::to_vec(&record(&z, "other", "x")).unwrap();
    for bad_key in [
        format!("{TREASURY_PREFIX}not-a-did"),
        format!("{TREASURY_PREFIX}did:icn:zNOTAKEY"),
        format!("{TREASURY_PREFIX}did:icn:"),
    ] {
        store.put(bad_key.as_bytes(), &valid_body).unwrap();
    }

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::UnreadableKey { rows: 3, keyspace })
            if keyspace == TREASURY_KEYSPACE
    ));
}

/// Thirty-two bytes that decode as an identifier but are no Ed25519 point:
/// the shape `Did::from_anchor_id` produces about half the time (inventory
/// §10.1), which `Did::from_str` and `Deserialize` both reject.
fn non_point_identifier() -> [u8; 32] {
    (0u8..=255)
        .map(|b| [b; 32])
        .find(|bytes| ed25519_dalek::VerifyingKey::from_bytes(bytes).is_err())
        .expect("some repeated byte is no compressed Edwards point")
}

#[test]
fn an_anchor_style_key_that_decodes_but_is_no_point_refuses_rather_than_vanishing() {
    // Before M1 this row was skipped silently — the "half of cooperative
    // treasuries drop out on reload" defect the inventory records against
    // #2628. M1 does not repair it and does not hide it: the row is a primary
    // row the loader cannot read, so hydration refuses. The startup gate
    // reads the same row as a decodable spelling, so this is the loader being
    // the stricter layer (migration gate §10.6), not a disagreement.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let spelling = format!("did:icn:f{}", hex::encode(non_point_identifier()));
    assert!(identifier_bytes_of_spelling(&spelling).is_ok());
    assert!(Did::from_str(&spelling).is_err());
    store
        .put(
            format!("{TREASURY_PREFIX}{spelling}").as_bytes(),
            br#"{"treasury_did":"not even needed","coop_id":"c"}"#,
        )
        .unwrap();

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::UnreadableKey { rows: 1, .. })
    ));
}

#[test]
fn a_primary_key_that_is_not_utf8_refuses_not_normalizes() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let mut key = format!("{TREASURY_PREFIX}{z}").into_bytes();
    key.push(0xFF);
    store
        .put(&key, &serde_json::to_vec(&record(&z, "c", "x")).unwrap())
        .unwrap();

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::UnreadableKey { rows: 1, .. })
    ));
}

#[test]
fn a_primary_key_with_material_after_the_spelling_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let body = serde_json::to_vec(&record(&z, "c", "x")).unwrap();
    store
        .put(format!("{TREASURY_PREFIX}{z}junk").as_bytes(), &body)
        .unwrap();
    store
        .put(format!("{TREASURY_PREFIX}{z}:x").as_bytes(), &body)
        .unwrap();

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::UnreadableKey { rows: 2, .. })
    ));
}

#[test]
fn a_primary_row_with_an_unreadable_value_refuses_rather_than_vanishing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    // The key is a perfectly good treasury principal; the value is not a
    // treasury record. Skipping it would leave the store looking as if it
    // held only the `f` row — unambiguous, and wrong.
    store
        .put(format!("{TREASURY_PREFIX}{z}").as_bytes(), b"{not a record")
        .unwrap();
    put_primary(&store, &record(&f, "food-coop", "the readable one"));

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Treasury(TreasuryHydrationRefusal::UnreadablePrimaryValue { rows: 1 })
    ));
}

#[test]
fn unreadability_is_reported_before_a_collision() {
    // An incomplete view is reported as incomplete, never as a collision
    // count computed over the rows that happened to be readable.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "z"));
    put_primary(&store, &record(&base16_lower(&z), "food-coop", "f"));
    store
        .put(format!("{TREASURY_PREFIX}garbage").as_bytes(), b"v")
        .unwrap();

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::UnreadableKey { .. })
    ));
}

// ── sibling subspaces beneath the lexical parent ────────────────────────────

#[test]
fn sibling_subspace_rows_are_never_classified_as_primary_rows() {
    // Every subspace that shares `ledger:treasury:` with the primary rows,
    // each holding a value that is not a treasury record — including a DID
    // spelling inside the audit and budget-index keys, which are key
    // structure there and never a treasury principal to this loader.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", z.as_str());
    for key in [
        format!("{BUDGET_PREFIX}budget-1"),
        format!("{SPENDING_RULE_PREFIX}rule-1"),
        format!("{TREASURY_AUDIT_PREFIX}{z}:1700000000:audit-1"),
        format!("{TREASURY_IDX_BUDGETS_PREFIX}{z}:budget-1"),
        format!("{VELOCITY_LIMIT_PREFIX}vlimit-1"),
    ] {
        store
            .put(key.as_bytes(), b"{not a treasury record")
            .unwrap();
    }

    let mgr = TreasuryManager::with_store(store).expect("siblings are not primary rows");
    assert_eq!(mgr.list_treasuries().len(), 1);
}

#[test]
fn a_velocity_limit_row_no_longer_depends_on_failing_to_parse() {
    // `ledger:treasury:vlimit:` was missing from the old skip list and was
    // tolerated only because its value never parsed as a `Treasury`. A row
    // there whose value *does* parse as a treasury record must still be a
    // sibling, never a treasury — classification is by key shape.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let stray = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    store
        .put(
            format!("{VELOCITY_LIMIT_PREFIX}vlimit-1").as_bytes(),
            &serde_json::to_vec(&record(&stray, "housing-coop", "not a treasury")).unwrap(),
        )
        .unwrap();

    let mgr = TreasuryManager::with_store(store).unwrap();
    assert_eq!(mgr.list_treasuries().len(), 1);
    assert!(!mgr.is_treasury_account(&stray));
    assert!(mgr.get_treasury_by_coop("housing-coop").is_none());
}

#[test]
fn an_unknown_shape_beneath_the_parent_is_refused_not_adopted_and_not_skipped() {
    // A key beneath `ledger:treasury:` that is neither a registered sibling
    // nor a principal is a shape no writer produces. Fail closed: a new
    // subspace must be named in the sibling list, not tolerated by accident.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    store
        .put(
            format!("{TREASURY_PREFIX}idx:other:x").as_bytes(),
            z.as_str().as_bytes(),
        )
        .unwrap();

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::UnreadableKey { rows: 1, .. })
    ));
}

// ── ledger:treasury:idx:coop: integrity ─────────────────────────────────────

#[test]
fn a_coop_index_naming_an_alternate_spelling_of_its_primary_row_refuses() {
    // primary spelling = z, idx:coop value = f, one principal. Under I7 the
    // two are equal as `Did`; the index must not be allowed to retarget the
    // spelling on that basis.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", f.as_str());
    let before = rows_under(&store, TREASURY_PREFIX);

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store.clone())),
        Refusal::Treasury(TreasuryHydrationRefusal::CoopIndexSpellingMismatch { rows: 1 })
    ));
    assert_eq!(
        rows_under(&store, TREASURY_PREFIX),
        before,
        "read-only refusal"
    );
}

#[test]
fn control_a_coop_index_naming_the_exact_primary_spelling_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", z.as_str());

    let mgr = TreasuryManager::with_store(store).expect("index agrees with the row");
    assert_eq!(
        mgr.get_treasury_by_coop("food-coop")
            .unwrap()
            .treasury_did
            .as_str(),
        z.as_str()
    );
}

#[test]
fn a_coop_index_under_another_coop_id_naming_an_alternate_spelling_refuses() {
    // The index row is filed under a coop id that has no primary row, but
    // its value is an alias spelling of a treasury that does exist. It would
    // let a consumer reach that treasury under the other spelling.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "housing-coop", base16_lower(&z).as_str());

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Treasury(TreasuryHydrationRefusal::CoopIndexSpellingMismatch { rows: 1 })
    ));
}

#[test]
fn a_coop_index_pointing_at_a_different_registered_treasury_refuses() {
    // Not a spelling problem: the index for `food-coop` names `housing-coop`'s
    // treasury. It is refused by the same byte comparison, because the row
    // registered for `food-coop` is not the row the value spells.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let a = a_principal();
    let b = a_principal();
    put_primary(&store, &record(&a, "food-coop", "a"));
    put_primary(&store, &record(&b, "housing-coop", "b"));
    put_coop_index(&store, "food-coop", b.as_str());
    put_coop_index(&store, "housing-coop", b.as_str());

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Treasury(TreasuryHydrationRefusal::CoopIndexSpellingMismatch { rows: 1 })
    ));
}

#[test]
fn a_coop_index_whose_value_names_no_principal_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", "did:icn:zNOTAKEY");
    store
        .put(
            format!("{TREASURY_IDX_COOP_PREFIX}other").as_bytes(),
            &[0xFF, 0xFE],
        )
        .unwrap();

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Treasury(TreasuryHydrationRefusal::CoopIndexUnreadable { rows: 2 })
    ));
}

#[test]
fn an_orphan_coop_index_is_tolerated_and_grants_nothing() {
    // Filed under a coop id with no primary row, naming a principal with no
    // primary row: nothing consumes it and nothing can be adopted from it.
    // The coop map is rebuilt from the primary rows, so the orphan does not
    // conjure a treasury.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let ghost = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "housing-coop", ghost.as_str());

    let mgr = TreasuryManager::with_store(store).expect("an orphan index blocks nothing");
    assert!(mgr.get_treasury_by_coop("housing-coop").is_none());
    assert!(!mgr.is_treasury_account(&ghost));
}

#[test]
fn the_coop_index_refusal_does_not_depend_on_write_order() {
    let z = a_principal();
    let f = base16_lower(&z);
    for index_first in [true, false] {
        let tmp = tempfile::tempdir().unwrap();
        let store = open_store(tmp.path());
        if index_first {
            put_coop_index(&store, "food-coop", f.as_str());
            put_primary(&store, &record(&z, "food-coop", "t"));
        } else {
            put_primary(&store, &record(&z, "food-coop", "t"));
            put_coop_index(&store, "food-coop", f.as_str());
        }
        assert!(matches!(
            refusal_of(TreasuryManager::with_store(store)),
            Refusal::Treasury(TreasuryHydrationRefusal::CoopIndexSpellingMismatch { rows: 1 })
        ));
    }
}

// ── institutional duplicates, moved before adoption ─────────────────────────

#[test]
fn two_treasuries_for_one_coop_id_still_refuse_with_a_typed_payload_free_error() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let a = a_principal();
    let b = a_principal();
    put_primary(&store, &record(&a, "food-coop", "a"));
    put_primary(&store, &record(&b, "food-coop", "b"));

    let result = TreasuryManager::with_store(store);
    let text = result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(matches!(
        refusal_of(result),
        Refusal::Treasury(TreasuryHydrationRefusal::DuplicateCoopId { rows: 1 })
    ));
    assert!(!text.contains(a.as_str()) && !text.contains(b.as_str()));
    assert!(!text.contains("food-coop"), "no coop id in the diagnostic");
}

// ── write path: no alias pair can be opened after hydration ─────────────────

#[test]
fn registering_a_second_spelling_of_a_hydrated_treasury_is_refused_and_writes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", z.as_str());
    let before = rows_under(&store, TREASURY_PREFIX);

    let mut mgr = TreasuryManager::with_store(store.clone()).unwrap();
    // Same principal under the other spelling, whether or not the coop id
    // is new: the manager keys by principal, so both are "already exists".
    for coop in ["food-coop", "housing-coop"] {
        assert!(mgr
            .register_treasury(f.clone(), coop.to_string(), "HOURS".into(), z.clone(), None)
            .is_err());
        assert!(mgr
            .register_treasury_with_entity(
                f.clone(),
                EntityId::cooperative(coop).unwrap(),
                "HOURS".into(),
                z.clone(),
                None
            )
            .is_err());
    }
    assert_eq!(rows_under(&store, TREASURY_PREFIX), before);
    assert_eq!(mgr.list_treasuries().len(), 1);
}

#[test]
fn populating_entity_id_under_an_alias_spelling_rewrites_only_the_stored_row() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    put_primary(&store, &record(&z, "food-coop", "the treasury"));

    let mut mgr = TreasuryManager::with_store(store.clone()).unwrap();
    let entity = EntityId::cooperative("food-coop").unwrap();
    // Addressed by the alias: the seam locates the row by principal and
    // persists under the record's own spelling, so no second row opens.
    mgr.populate_entity_id_at_creation(&f, "food-coop", entity.clone())
        .unwrap();

    let rows = rows_under(&store, TREASURY_PREFIX);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, format!("{TREASURY_PREFIX}{z}").into_bytes());
    let stored: Treasury = serde_json::from_slice(&rows[0].1).unwrap();
    assert_eq!(stored.treasury_did.as_str(), z.as_str());
    assert_eq!(stored.entity_id(), Some(&entity));

    // And the store it left behind hydrates again.
    drop(mgr);
    let again = TreasuryManager::with_store(store).unwrap();
    assert_eq!(
        again.get_treasury_by_entity(&entity).unwrap().coop_id(),
        "food-coop"
    );
}

#[test]
fn reopening_a_clean_store_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let a = a_principal();
    let b = a_principal();
    {
        let mut mgr = TreasuryManager::with_store(store.clone()).unwrap();
        mgr.register_treasury(
            a.clone(),
            "food-coop".into(),
            "HOURS".into(),
            a.clone(),
            None,
        )
        .unwrap();
        mgr.register_treasury(
            b.clone(),
            "housing-coop".into(),
            "HOURS".into(),
            b.clone(),
            None,
        )
        .unwrap();
    }
    for _ in 0..3 {
        let mgr = TreasuryManager::with_store(store.clone()).unwrap();
        assert_eq!(mgr.list_treasuries().len(), 2);
    }
}

// ── the startup gate and the loader read the same rows ──────────────────────

#[test]
fn the_scanner_and_the_loader_refuse_the_same_alias_pair() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "z"));
    put_primary(&store, &record(&base16_lower(&z), "food-coop", "f"));

    let report = scan_keyspace(store.as_ref(), &treasury_descriptor()).unwrap();
    assert_eq!(report.rows_scanned, 2);
    assert_eq!(report.collision_groups.len(), 1);
    assert!(report.must_fail_closed(), "the gate refuses this pair");

    assert!(matches!(
        refusal_of(TreasuryManager::with_store(store)),
        Refusal::Principal(PrincipalRowsRefusal::AliasCollision { .. })
    ));
}

#[test]
fn a_store_the_loader_accepts_is_clear_and_covered_at_the_gate() {
    // One registered treasury, its index, and one row of every sibling
    // subspace that carries no principal in its key: the loader adopts one
    // treasury, the audit is clear, and the primary row is *covered* — never
    // reported as an uncovered shape, which is what made an ordinary treasury
    // store refuse `icnd` before this registration.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    put_coop_index(&store, "food-coop", z.as_str());
    for key in [
        format!("{BUDGET_PREFIX}budget-1"),
        format!("{SPENDING_RULE_PREFIX}rule-1"),
        format!("{VELOCITY_LIMIT_PREFIX}vlimit-1"),
    ] {
        store.put(key.as_bytes(), b"{}").unwrap();
    }

    let audit = audit_store(
        store.as_ref(),
        &n2a_keyspaces(),
        &n2a_deferred_namespaces(),
        0,
    )
    .unwrap();
    assert!(audit.uncovered.is_empty(), "{:?}", audit.uncovered);
    assert!(audit.is_clear());
    let report = scan_keyspace(store.as_ref(), &treasury_descriptor()).unwrap();
    assert_eq!(
        report.rows_scanned, 1,
        "siblings are outside the descriptor"
    );
    assert_eq!(report.distinct_principals, 1);

    assert_eq!(
        TreasuryManager::with_store(store)
            .unwrap()
            .list_treasuries()
            .len(),
        1
    );
}

#[test]
fn a_sibling_row_carrying_a_spelling_is_outside_the_descriptor_not_a_treasury_principal() {
    // The audit and budget-index subspaces embed the treasury DID as key
    // structure. The treasury descriptor must not claim them — their
    // disposition is not M1's — and must never read the spelling inside them
    // as a second spelling of the primary row. To the gate they remain what
    // they were before M1: principal-bearing rows under no registered
    // keyspace, i.e. uncovered, exactly as documented for follow-up.
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let z = a_principal();
    let f = base16_lower(&z);
    put_primary(&store, &record(&z, "food-coop", "the treasury"));
    store
        .put(
            format!("{TREASURY_AUDIT_PREFIX}{f}:1700000000:audit-1").as_bytes(),
            b"{}",
        )
        .unwrap();
    store
        .put(
            format!("{TREASURY_IDX_BUDGETS_PREFIX}{f}:budget-1").as_bytes(),
            b"budget-1",
        )
        .unwrap();

    let report = scan_keyspace(store.as_ref(), &treasury_descriptor()).unwrap();
    assert_eq!(report.rows_scanned, 1);
    assert_eq!(
        report.collision_groups.len(),
        0,
        "no alias group from siblings"
    );
    let audit = audit_store(
        store.as_ref(),
        &n2a_keyspaces(),
        &n2a_deferred_namespaces(),
        0,
    )
    .unwrap();
    assert_eq!(audit.uncovered.len(), 2, "{:?}", audit.uncovered);
    assert!(audit
        .uncovered
        .keys()
        .all(|shape| shape.starts_with("ledger:treasury:audit:<did>")
            || shape.starts_with("ledger:treasury:idx:budgets:<did>")));

    // The loader, for its part, sees one treasury and no alias.
    assert_eq!(
        TreasuryManager::with_store(store)
            .unwrap()
            .list_treasuries()
            .len(),
        1
    );
}
