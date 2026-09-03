//! N2-A: the ledger's principal-keyed rebuilds defend themselves (#2627).
//!
//! ## What is proven
//!
//! `Ledger::new` rebuilds three principal-keyed indexes from sled — cached
//! balances, the cleared-volume index and the freeze table. I7 (#2686) made
//! `Did` equality and hashing name the *principal*; the persisted keys still
//! name a *spelling*. These tests write real sled rows and prove the rebuild
//! refuses, rather than collapsing, when those two regimes disagree.
//!
//! Each keyspace gets the same four fixtures:
//!
//! 1. one valid spelling — loads;
//! 2. two spellings of one principal — refuses;
//! 3. two genuinely different principals — loads;
//! 4. a key that names no principal — refuses.
//!
//! ## What is NOT proven
//!
//! - That any deployed store contains such rows. Three scanned deployments
//!   found zero collisions (`docs/architecture/n2-a-migration-gate.md` §3.2);
//!   these are constructed fixtures, not deployment evidence.
//! - Any merge rule. `icn-ledger/{balance,cleared_volume,frozen}` carry
//!   `RuleBasis::AwaitingDomainSignOff`; refusing is the whole behaviour under
//!   test, and a test that asserted a sum or a union would be asserting an
//!   economic decision no domain owner has made.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::{Did, KeyPair};
use icn_ledger::freeze::FrozenMember;
use icn_ledger::principal_rows::PrincipalRowsRefusal;
use icn_ledger::types::AccountBalances;
use icn_ledger::Ledger;
use icn_store::{SledStore, Store};
use std::sync::Arc;

const BALANCE_PREFIX: &str = "ledger:balance:";
const CLEARED_VOLUME_PREFIX: &str = "ledger:cleared_volume:";
const FREEZE_PREFIX: &str = "ledger:frozen:";

fn a_principal() -> Did {
    KeyPair::generate().unwrap().did().clone()
}

/// A second, equally valid textual encoding of the principal `did` names.
///
/// `did:icn:` identifiers are multibase, so the same 32 bytes have a base58btc
/// spelling and a base16 spelling. Both parse; both decode to one identifier.
/// This is the same construction the CCL and networking I7 proofs use.
fn alternate_spelling(did: &Did) -> Did {
    let bytes = did.identifier_bytes().unwrap();
    let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes))).unwrap();
    assert_ne!(
        did.as_str(),
        alias.as_str(),
        "the two spellings must differ, or the test proves nothing"
    );
    alias
}

fn open_store(dir: &std::path::Path) -> Arc<SledStore> {
    Arc::new(SledStore::open(dir).unwrap())
}

// ── row writers, byte-for-byte as the ledger's own save paths build them ────

fn put_balance_row(store: &Arc<SledStore>, spelling: &Did, currency: &str, amount: i64) {
    let mut balances = AccountBalances::new(spelling.clone());
    balances.balances.insert(currency.to_string(), amount);
    let key = format!(
        "{BALANCE_PREFIX}{}",
        serde_json::to_string(spelling).unwrap()
    );
    store
        .put(key.as_bytes(), &serde_json::to_vec(&balances).unwrap())
        .unwrap();
}

fn put_cleared_volume_row(store: &Arc<SledStore>, spelling: &Did, currency: &str, volume: i64) {
    let key = format!("{CLEARED_VOLUME_PREFIX}{spelling}:{currency}");
    store
        .put(key.as_bytes(), &serde_json::to_vec(&volume).unwrap())
        .unwrap();
}

fn put_frozen_row(store: &Arc<SledStore>, spelling: &Did, reason: &str) {
    let record = FrozenMember::new(spelling.clone(), reason.to_string(), None);
    let key = format!("{FREEZE_PREFIX}{spelling}");
    store
        .put(key.as_bytes(), &serde_json::to_vec(&record).unwrap())
        .unwrap();
}

/// The typed refusal behind an opaque `anyhow::Error`, or a panic naming what
/// actually came back — a rebuild that succeeded is the regression these tests
/// exist to catch, so it must not read as a pass.
fn refusal_of(result: anyhow::Result<Ledger>) -> PrincipalRowsRefusal {
    match result {
        Ok(_) => panic!("the rebuild accepted state it cannot interpret"),
        Err(e) => e
            .downcast_ref::<PrincipalRowsRefusal>()
            .unwrap_or_else(|| panic!("refused, but not with a typed refusal: {e}"))
            .clone(),
    }
}

// ── ledger:balance: ─────────────────────────────────────────────────────────

#[test]
fn one_balance_spelling_rebuilds() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    put_balance_row(&store, &account, "hours", 10);

    let ledger = Ledger::new(store).expect("a single spelling is unambiguous");
    assert_eq!(ledger.get_balance(&account, "hours"), 10);
}

#[test]
fn two_spellings_of_one_account_refuse_the_balance_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    let alias = alternate_spelling(&account);

    // Two rows, two spellings, one principal, and two different balances. A
    // last-writer-wins rebuild would pick one by key byte order and write it
    // back under the other's key.
    put_balance_row(&store, &account, "hours", 10);
    put_balance_row(&store, &alias, "hours", 999);

    match refusal_of(Ledger::new(store)) {
        PrincipalRowsRefusal::AliasCollision { keyspace, groups } => {
            assert_eq!(keyspace, "icn-ledger/balance");
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].spellings, 2);
        }
        other => panic!("expected AliasCollision, got {other:?}"),
    }
}

#[test]
fn two_different_accounts_rebuild_normally() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let alice = a_principal();
    let bob = a_principal();
    put_balance_row(&store, &alice, "hours", 10);
    put_balance_row(&store, &bob, "hours", -10);

    let ledger = Ledger::new(store).expect("two principals are two accounts");
    assert_eq!(ledger.get_balance(&alice, "hours"), 10);
    assert_eq!(ledger.get_balance(&bob, "hours"), -10);
}

#[test]
fn a_balance_key_naming_no_principal_refuses_rather_than_vanishing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    put_balance_row(&store, &account, "hours", 10);

    // A truncated spelling: the key still looks like a balance row, but names
    // no principal. Skipping it would rebuild an account list that silently
    // omits whatever it held.
    let broken = AccountBalances::new(account.clone());
    store
        .put(
            format!("{BALANCE_PREFIX}\"did:icn:ztruncated\"").as_bytes(),
            &serde_json::to_vec(&broken).unwrap(),
        )
        .unwrap();

    assert!(matches!(
        refusal_of(Ledger::new(store)),
        PrincipalRowsRefusal::UnreadableKey { rows: 1, .. }
    ));
}

#[test]
fn a_row_whose_key_and_contents_disagree_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    let alias = alternate_spelling(&account);

    // The residue of a rebuild that already collapsed two spellings: the key
    // names one spelling, the stored balances name the other.
    let mut balances = AccountBalances::new(alias);
    balances.balances.insert("hours".to_string(), 10);
    let key = format!(
        "{BALANCE_PREFIX}{}",
        serde_json::to_string(&account).unwrap()
    );
    store
        .put(key.as_bytes(), &serde_json::to_vec(&balances).unwrap())
        .unwrap();

    assert!(matches!(
        refusal_of(Ledger::new(store)),
        PrincipalRowsRefusal::KeyValueSpellingMismatch { rows: 1, .. }
    ));
}

#[test]
fn an_unquoted_balance_key_is_not_the_writers_shape_and_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    put_balance_row(&store, &account, "USD", 100);

    // `save_cached_balances` keys a row by the JSON-quoted spelling. A key that
    // carries the bare spelling decodes to a principal, so a lenient parser
    // adopts it; but it is a shape the writer never produced, and adopting it
    // would rebuild an account from a row nothing in this crate wrote.
    let other = a_principal();
    let balances = AccountBalances::new(other.clone());
    store
        .put(
            format!("{BALANCE_PREFIX}{other}").as_bytes(),
            &serde_json::to_vec(&balances).unwrap(),
        )
        .unwrap();

    assert!(matches!(
        refusal_of(Ledger::new(store)),
        PrincipalRowsRefusal::UnreadableKey { rows: 1, .. }
    ));
}

// ── ledger:cleared_volume: ──────────────────────────────────────────────────

#[test]
fn one_cleared_volume_spelling_rebuilds() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    put_cleared_volume_row(&store, &a_principal(), "USD", 500);

    Ledger::new(store).expect("a single spelling is unambiguous");
}

#[test]
fn two_spellings_of_one_account_refuse_the_cleared_volume_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    put_cleared_volume_row(&store, &account, "USD", 500);
    put_cleared_volume_row(&store, &alternate_spelling(&account), "USD", 700);

    match refusal_of(Ledger::new(store)) {
        PrincipalRowsRefusal::AliasCollision { keyspace, groups } => {
            assert_eq!(keyspace, "icn-ledger/cleared_volume");
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].discriminator, "USD");
        }
        other => panic!("expected AliasCollision, got {other:?}"),
    }
}

#[test]
fn one_account_in_two_currencies_is_not_a_collision() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    put_cleared_volume_row(&store, &account, "USD", 500);
    put_cleared_volume_row(&store, &account, "EUR", 700);

    Ledger::new(store).expect("two currencies of one account are two rows of state");
}

#[test]
fn an_unparseable_cleared_volume_key_refuses_rather_than_being_dropped() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    put_cleared_volume_row(&store, &a_principal(), "USD", 500);
    store
        .put(
            format!("{CLEARED_VOLUME_PREFIX}did:icn:ztruncated:USD").as_bytes(),
            &serde_json::to_vec(&42i64).unwrap(),
        )
        .unwrap();

    assert!(matches!(
        refusal_of(Ledger::new(store)),
        PrincipalRowsRefusal::UnreadableKey { rows: 1, .. }
    ));
}

#[test]
fn a_currency_containing_a_colon_still_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();

    // No charset validation stops a colon reaching a currency, so the key
    // parser must not assume the last colon separates it from the DID. Under a
    // `rfind` split this row names no principal, and the node refuses to start
    // for good after having accepted the entry that wrote it.
    put_cleared_volume_row(&store, &account, "USD:SPOT", 500);
    put_cleared_volume_row(&store, &account, "EUR", 700);

    Ledger::new(store).expect("a colon in a currency is not an unreadable principal");
}

#[test]
fn a_cleared_volume_key_without_a_currency_delimiter_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = a_principal();
    put_cleared_volume_row(&store, &account, "USD", 500);

    // `save_cleared_volume_index` always writes `<did>:<currency>`, so a key
    // that ends at the spelling is one the writer never produced. Adopting it
    // as an empty-currency row would turn corrupted state into an account
    // with a phantom currency.
    store
        .put(
            format!("{CLEARED_VOLUME_PREFIX}{account}").as_bytes(),
            &serde_json::to_vec(&42i64).unwrap(),
        )
        .unwrap();

    assert!(matches!(
        refusal_of(Ledger::new(store)),
        PrincipalRowsRefusal::UnreadableKey { rows: 1, .. }
    ));
}

/// A `Did` spelled in the multibase Identity base, whose body is the raw
/// identifier bytes and here contains `:`.
///
/// The body must be a valid Ed25519 point for `Did::from_str` to accept it;
/// about half of all 32-byte strings are, so a few tweaks of the last byte
/// find one. No private key is needed: a persisted account id is only ever
/// validated as a public point.
fn identity_spelled_did_containing_a_colon() -> Did {
    let mut body = *b"ab:cd:ef:gh:ij:kl:mn:op:qr:st:u0";
    for last in b'0'..=b'z' {
        body[31] = last;
        let spelling = format!("did:icn:\u{0}{}", std::str::from_utf8(&body).unwrap());
        if let Ok(did) = Did::from_str(&spelling) {
            assert_eq!(did.identifier_bytes().unwrap(), body);
            return did;
        }
    }
    panic!("no ASCII body in the search range decompressed to a point");
}

#[test]
fn an_identity_spelled_account_whose_bytes_contain_a_colon_still_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let account = identity_spelled_did_containing_a_colon();

    // Every multibase alphabet the parser assumed colon-free is, except the
    // Identity base, whose payload is unencoded. A first-colon split cuts
    // inside this spelling, the truncated remainder names no principal, and
    // the node refuses to start for good after having accepted the entry.
    put_cleared_volume_row(&store, &account, "USD", 500);
    put_cleared_volume_row(&store, &account, "EUR:SPOT", 700);

    let ledger = Ledger::new(store).expect("an Identity-base spelling is a readable principal");
    assert_eq!(ledger.total_cleared_by(&account, "USD").unwrap(), 500);
    assert_eq!(ledger.total_cleared_by(&account, "EUR:SPOT").unwrap(), 700);
}

// ── ledger:frozen: ──────────────────────────────────────────────────────────

#[test]
fn one_freeze_spelling_rebuilds() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();
    put_frozen_row(&store, &member, "suspected fraud");

    let mut ledger = Ledger::new(store).expect("a single spelling is unambiguous");
    assert!(ledger.is_member_frozen(&member));
}

#[test]
fn two_spellings_of_one_member_refuse_the_freeze_rebuild() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();

    // Two live freezes disagreeing about why. Choosing between them is a
    // governance decision, and the union rule in the migration gate is
    // asserted, not authorized.
    put_frozen_row(&store, &member, "suspected fraud");
    put_frozen_row(&store, &alternate_spelling(&member), "legal hold");

    match refusal_of(Ledger::new(store)) {
        PrincipalRowsRefusal::AliasCollision { keyspace, groups } => {
            assert_eq!(keyspace, "icn-ledger/frozen");
            assert_eq!(groups.len(), 1);
        }
        other => panic!("expected AliasCollision, got {other:?}"),
    }
}

#[test]
fn two_different_frozen_members_rebuild_normally() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let one = a_principal();
    let two = a_principal();
    put_frozen_row(&store, &one, "suspected fraud");
    put_frozen_row(&store, &two, "legal hold");

    let mut ledger = Ledger::new(store).expect("two principals are two members");
    assert!(ledger.is_member_frozen(&one));
    assert!(ledger.is_member_frozen(&two));
}

#[test]
fn a_lapsed_alias_freeze_does_not_block_a_start() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();

    put_frozen_row(&store, &member, "legal hold");

    // The alias row expired an hour ago, so it binds nothing. Refusing on it
    // would block a start over state that has no effect.
    let mut lapsed = FrozenMember::new(
        alternate_spelling(&member),
        "old freeze".to_string(),
        Some(1),
    );
    lapsed.frozen_at = icn_time::current_timestamp_secs() - 3600;
    lapsed.expires_at = Some(icn_time::current_timestamp_secs() - 3540);
    let key = format!("{FREEZE_PREFIX}{}", lapsed.did);
    store
        .put(key.as_bytes(), &serde_json::to_vec(&lapsed).unwrap())
        .unwrap();

    let mut ledger = Ledger::new(store).expect("an expired freeze is not live state");
    assert!(ledger.is_member_frozen(&member));
}

#[test]
fn a_freeze_row_whose_key_and_body_disagree_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();
    let alias = alternate_spelling(&member);

    // ONE row, stored under the alias spelling, whose body names the other
    // spelling. Isolated deliberately: two keys would be an alias collision and
    // would refuse for that reason instead, hiding whether the key/body check
    // works at all. A guard reading only the body sees a single well-formed
    // record here and adopts it — after which every write and delete addresses
    // `ledger:frozen:<member>`, a key that does not exist, and the real row at
    // `<alias>` survives every unfreeze.
    let masquerading = FrozenMember::new(member.clone(), "legal hold".to_string(), None);
    let key = format!("{FREEZE_PREFIX}{alias}");
    store
        .put(key.as_bytes(), &serde_json::to_vec(&masquerading).unwrap())
        .unwrap();

    assert!(
        matches!(
            refusal_of(Ledger::new(store)),
            PrincipalRowsRefusal::KeyValueSpellingMismatch { .. }
        ),
        "a freeze row whose key names a different spelling than its body must refuse"
    );
}

#[test]
fn two_freeze_keys_are_refused_even_when_both_bodies_agree() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();
    let alias = alternate_spelling(&member);

    // The same masquerade, reached from the other direction: the keyspace holds
    // two spellings of one principal, which is a collision whatever the bodies
    // say. Grouping by the stored key is what makes this visible.
    for spelling in [&member, &alias] {
        let record = FrozenMember::new(member.clone(), "legal hold".to_string(), None);
        let key = format!("{FREEZE_PREFIX}{spelling}");
        store
            .put(key.as_bytes(), &serde_json::to_vec(&record).unwrap())
            .unwrap();
    }

    assert!(
        Ledger::new(store).is_err(),
        "two spelling-keyed freeze rows for one principal must not load"
    );
}

// ── write-back keeps the row identity it loaded ─────────────────────────────

#[test]
fn unfreezing_removes_the_row_the_store_actually_holds() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();
    put_frozen_row(&store, &member, "suspected fraud");

    {
        let mut ledger = Ledger::new(store.clone()).unwrap();
        assert!(ledger.is_member_frozen(&member));
        // Unfreeze naming the *other* spelling of the same principal. Deleting
        // by the caller's spelling would leave the stored row behind and the
        // member would come back frozen on the next start.
        ledger.unfreeze_member(&alternate_spelling(&member), "cleared".to_string());
    }

    let rows = store.scan(FREEZE_PREFIX.as_bytes()).unwrap();
    assert!(
        rows.is_empty(),
        "unfreeze left {} freeze row(s) on disk",
        rows.len()
    );

    let mut ledger = Ledger::new(store).unwrap();
    assert!(
        !ledger.is_member_frozen(&member),
        "the freeze came back from a row the unfreeze did not reach"
    );
}

#[test]
fn refreezing_under_a_second_spelling_does_not_open_a_second_row() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let member = a_principal();
    put_frozen_row(&store, &member, "suspected fraud");

    {
        let mut ledger = Ledger::new(store.clone()).unwrap();
        ledger.freeze_member(alternate_spelling(&member), "escalated".to_string(), None);
    }

    let rows = store.scan(FREEZE_PREFIX.as_bytes()).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "re-freezing under a second spelling opened a second row, which the \
         next rebuild would refuse"
    );

    // And the store is still loadable, which is the property that second row
    // would have destroyed.
    let mut ledger = Ledger::new(store).expect("one principal still has one row");
    assert!(ledger.is_member_frozen(&member));
}

#[test]
fn reopening_a_clean_store_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let store = open_store(tmp.path());
    let alice = a_principal();
    put_balance_row(&store, &alice, "hours", 10);
    put_cleared_volume_row(&store, &alice, "USD", 500);
    put_frozen_row(&store, &alice, "legal hold");

    let keys = |store: &Arc<SledStore>| -> Vec<String> {
        store
            .scan(b"ledger:")
            .unwrap()
            .into_iter()
            .map(|(k, _)| String::from_utf8_lossy(&k).into_owned())
            .collect()
    };

    // The first open is allowed to write its one-time index markers; that is
    // what makes every later open a no-op. Idempotence is the property of the
    // steady state, so the baseline is taken after it.
    drop(Ledger::new(store.clone()).expect("a clean store loads"));
    let after_first = keys(&store);

    for _ in 0..3 {
        drop(Ledger::new(store.clone()).expect("a clean store stays loadable"));
    }

    assert_eq!(
        after_first,
        keys(&store),
        "repeated safe loads changed the persisted key set"
    );

    // And the principal-keyed rows specifically are still exactly the three
    // written above — no rebuild opened a second row for the same account.
    for prefix in [BALANCE_PREFIX, CLEARED_VOLUME_PREFIX, FREEZE_PREFIX] {
        assert_eq!(
            store.scan(prefix.as_bytes()).unwrap().len(),
            1,
            "{prefix} gained or lost a row across repeated loads"
        );
    }
}
