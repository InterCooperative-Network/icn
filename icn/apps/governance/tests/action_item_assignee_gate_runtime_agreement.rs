//! The N2-A startup gate and the live by-assignee reader agree about which
//! `action_item_by_assignee:` states are safe (#2627 M4c).
//!
//! The two layers describe one boundary from opposite ends. The gate decides
//! whether a store holding a given set of projection rows may be *opened*; the
//! store's own reader decides what those rows *mean* at runtime. If they
//! disagreed, a governance daemon would either refuse to start over rows its
//! reader handles correctly, or start over rows its reader would misread.
//!
//! The disposition under test is `Equivalent`, and the controls carry as much
//! weight as the refusals. Two spellings of one principal on ONE action item
//! are two derivations of one canonical fact, so the gate must call them clear
//! and the reader must return the item once. One person holding MANY action
//! items is the ordinary shape of a cooperative, so it must never group — a
//! gate whose collision unit had collapsed to the principal alone would refuse
//! to start every deployment with a busy member.
//!
//! Two things are deliberately kept apart. Alias *equivalence* is a statement
//! about two rows that derive one canonical fact. Forged-projection
//! *correctness* is a statement about a row whose canonical fact says something
//! else. The gate reads keys and cannot see the second; the reader proves the
//! canonical row and can. Neither substitutes for the other.
//!
//! Both halves run against one real sled database, at the path and the
//! data-directory level the daemon uses, so the gate reads exactly the bytes
//! the store wrote.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use icn_governance::{ActionItem, ActionItemStoreBackend, GovernanceDomainId};
use icn_governance_actor::manager::SledActionItemStore;
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

/// The governance actor opens its action-item database at
/// `<store_path>/governance_action_items` (`icn_governance_actor::init`), and
/// the gate walks every sled database beneath the data directory.
fn action_item_store_path(data_dir: &Path) -> PathBuf {
    data_dir.join("governance_action_items")
}

/// Run one phase against the action-item database, then close it.
///
/// A sled database admits one handle at a time, and the gate opens every store
/// it discovers, so each phase takes the database, does its whole job and gives
/// it back. That is also what the daemon does: the gate runs before the actor
/// opens anything.
fn phase<T>(data_dir: &Path, f: impl FnOnce(&Arc<sled::Db>, &SledActionItemStore) -> T) -> T {
    let db = Arc::new(sled::open(action_item_store_path(data_dir)).unwrap());
    let store = SledActionItemStore::new(Arc::clone(&db));
    let out = f(&db, &store);
    db.flush().unwrap();
    out
}

fn projection_row_count(db: &sled::Db) -> usize {
    db.scan_prefix(b"action_item_by_assignee:").count()
}

/// Write a raw projection row the way a pre-M4c binary would have.
fn plant_row(db: &sled::Db, spelling: &str, domain: &str, item_id: &str) {
    db.insert(
        format!("action_item_by_assignee:{spelling}:{domain}:{item_id}").as_bytes(),
        b"1".as_slice(),
    )
    .unwrap();
}

fn assigned(domain: &GovernanceDomainId, title: &str, assignee: &Did) -> ActionItem {
    let mut item = ActionItem::new(domain.clone(), title.to_string(), spelling_a(1), 1_000);
    item.assignee = Some(assignee.clone());
    item
}

fn blockers(receipt: &icn_store::n2a_startup_gate::GateReceipt) -> Vec<String> {
    receipt
        .stores
        .iter()
        .flat_map(|s| s.blocking.iter().map(|b| b.describe()))
        .collect()
}

#[test]
fn a_healthy_projection_clears_the_gate_and_reads_under_every_spelling() {
    let dir = tempfile::tempdir().unwrap();
    let domain = GovernanceDomainId::new("coop-a");
    let a = spelling_a(81);
    let b = spelling_b(81);
    assert_eq!(a, b, "the fixture spellings must name one principal");
    assert_ne!(a.as_str(), b.as_str(), "and must be different strings");

    let item = phase(dir.path(), |db, store| {
        let item = assigned(&domain, "Write minutes", &a);
        store.save(&item).unwrap();
        assert_eq!(projection_row_count(db), 1);
        item
    });

    // Gate: the state the writer produced is one a restart accepts.
    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("a store the action-item writer wrote is a store the gate opens");
    assert_eq!(receipt.verdict, Verdict::Clear);

    // Runtime: and the item is found under either spelling of its assignee.
    phase(dir.path(), |_db, store| {
        for query in [&a, &b] {
            let found = store.list_by_assignee(query).unwrap();
            assert_eq!(found.len(), 1, "one principal, one item, either spelling");
            assert_eq!(found[0].id, item.id);
        }
    });
}

#[test]
fn historical_alias_rows_on_one_item_clear_the_gate_and_read_as_one_item() {
    // The `Equivalent` disposition, at both layers at once. A store carried
    // over from a pre-M4c binary can hold the superseded spelling beside the
    // current one; the gate must open it, and the reader must return the one
    // canonical obligation once rather than twice.
    let dir = tempfile::tempdir().unwrap();
    let domain = GovernanceDomainId::new("coop-a");
    let a = spelling_a(82);
    let b = spelling_b(82);

    let item = phase(dir.path(), |db, store| {
        let item = assigned(&domain, "Carried over", &b);
        store.save(&item).unwrap();
        plant_row(db, a.as_str(), &domain.0, &item.id.0.to_string());
        assert_eq!(projection_row_count(db), 2, "two rows, one item");
        item
    });

    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("two derivations of one canonical fact must not block a start");
    assert_eq!(receipt.verdict, Verdict::Clear);

    phase(dir.path(), |db, store| {
        for query in [&a, &b] {
            let found = store.list_by_assignee(query).unwrap();
            assert_eq!(found.len(), 1, "one canonical item, once: {found:?}");
            assert_eq!(found[0].id, item.id);
        }
        assert_eq!(
            projection_row_count(db),
            2,
            "and reading repaired nothing — M4c chooses no survivor and re-keys no row"
        );
    });
}

#[test]
fn one_person_with_many_action_items_still_clears_the_gate() {
    // The scale control for the `Verdict::Clear` above. A member holding work
    // in several domains is the ordinary shape of a cooperative, and it is the
    // shape a collision unit reduced to the principal alone would break — a
    // failure that only shows up once a store holds more than one item.
    let dir = tempfile::tempdir().unwrap();
    let a = spelling_a(83);
    let b = spelling_b(83);

    phase(dir.path(), |db, store| {
        for (n, spelling) in [(1u8, &a), (2, &b), (3, &a), (4, &b)] {
            let domain = GovernanceDomainId::new(format!("coop-{n}"));
            store.save(&assigned(&domain, "Ongoing", spelling)).unwrap();
        }
        assert_eq!(projection_row_count(db), 4);
    });

    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("many items for one person is not a collision");
    assert_eq!(receipt.verdict, Verdict::Clear);

    phase(dir.path(), |_db, store| {
        assert_eq!(
            store.list_by_assignee(&b).unwrap().len(),
            4,
            "and every one of them is that person's work, under either spelling"
        );
    });
}

#[test]
fn a_projection_row_naming_no_principal_refuses_at_both_layers() {
    // The one shape both layers must refuse, and they refuse it for the same
    // reason: a row whose anchor holds no readable spelling names no principal,
    // so no migration can classify it and no query can rule it out.
    let dir = tempfile::tempdir().unwrap();
    let domain = GovernanceDomainId::new("coop-a");
    let a = spelling_a(84);

    phase(dir.path(), |db, store| {
        let item = assigned(&domain, "Real work", &a);
        store.save(&item).unwrap();
        plant_row(
            db,
            "did:icn:znotaspelling",
            &domain.0,
            &item.id.0.to_string(),
        );
    });

    let blocked = match enforce(dir.path(), SystemTime::now()) {
        Err(GateRefusal::Blocked { receipt, .. }) => *receipt,
        other => panic!("expected an unreadable anchor to block the start, got {other:?}"),
    };
    let described = blockers(&blocked);
    assert!(
        described
            .iter()
            .any(|b| b.contains("icn-governance-actor/action_item_by_assignee")),
        "{described:?}"
    );

    phase(dir.path(), |_db, store| {
        assert!(
            store.list_by_assignee(&a).is_err(),
            "and the reader refuses rather than answering short"
        );
    });
}

#[test]
fn a_forged_row_is_a_runtime_question_the_gate_does_not_answer() {
    // Alias equivalence and forged-projection correctness are different
    // properties and are deliberately not conflated.
    //
    // A row claiming another person's item names a DIFFERENT principal than the
    // real one, so at the key level it is a second shape and no group forms:
    // the gate is clear, and correctly so — nothing about these keys is
    // ambiguous. What makes the state safe is the reader proving the canonical
    // row's own assignee before returning it. The gate never reads a value and
    // could not have decided this.
    let dir = tempfile::tempdir().unwrap();
    let domain = GovernanceDomainId::new("coop-a");
    let a = spelling_a(85);
    let c = spelling_a(86);
    assert_ne!(a, c);

    phase(dir.path(), |db, store| {
        let item = assigned(&domain, "C's obligation", &c);
        store.save(&item).unwrap();
        plant_row(db, a.as_str(), &domain.0, &item.id.0.to_string());
    });

    let receipt = enforce(dir.path(), SystemTime::now())
        .expect("two principals on one item are two shapes; the gate has nothing to decide");
    assert_eq!(receipt.verdict, Verdict::Clear);

    phase(dir.path(), |_db, store| {
        assert!(
            store.list_by_assignee(&a).unwrap().is_empty(),
            "a projection row is evidence, never authority"
        );
        assert_eq!(
            store.list_by_assignee(&c).unwrap().len(),
            1,
            "and the person the canonical row names still sees it"
        );
    });
}
