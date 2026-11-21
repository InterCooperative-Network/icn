#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Journal entry identity (icn#2871): what `Ledger::append_entry` does when two entries share an id.
//!
//! `compute_entry_hash` covers `timestamp` (milliseconds), `author`, `contract_ref`, `accounts`,
//! `parents` and `nonce`. It does NOT cover `provenance`, `signature` or memo-like fields, and
//! `JournalEntryBuilder` takes the wall clock for `timestamp` and sets no `nonce`. Two entries with
//! the same shape built within one millisecond therefore have the SAME id even when they carry
//! different governance receipts.
//!
//! `append_entry_internal` writes the journal key with `put` (overwrite), then applies the balance
//! deltas. Before icn#2871 it had no check for an id that already exists, so a colliding append
//! replaced the journal record and applied its deltas a second time. It now refuses any id that is
//! already journaled, before any side effect, and `append_entries` refuses such a batch before it
//! commits anything.
//!
//! Test classes: `control_*` pins a premise. `invariant_*` states the safe behaviour; one invariant
//! still fails and is `#[ignore]`d with the reason (run with `--ignored`): distinct entries built in
//! one millisecond still share an id, and are now refused instead of corrupting state. Making them
//! all succeed changes the canonical entry hash and is an owner decision (icn#2871 part 2). The
//! oracle is the ledger's own `verify_integrity()` (cached balances == balances recomputed from the
//! journal).
//!
//! Atomicity of the journal write versus the balance update across a crash is NOT tested here: it
//! needs a fault-injection point that does not exist.

use icn_identity::{Did, IdentityBundle};
use icn_ledger::{
    entry::JournalEntryBuilder,
    types::{JournalEntry, ProvenanceRef},
    Ledger, LedgerSyncMessage,
};
use icn_store::SledStore;
use std::sync::Arc;

const AMOUNT: i64 = 5;
const CURRENCY: &str = "HOURS";

fn did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

/// A balanced governance-provenance entry. Built through the real builder, so its timestamp is the
/// real wall clock in milliseconds and it has no nonce.
fn gov_entry(author: &Did, to: &Did, receipt: &str) -> JournalEntry {
    JournalEntryBuilder::new(author.clone())
        .debit(author.clone(), CURRENCY.to_string(), AMOUNT)
        .credit(to.clone(), CURRENCY.to_string(), AMOUNT)
        .with_governance_provenance(receipt, format!("sha256:{receipt}"))
        .build()
        .unwrap()
}

fn open(path: &std::path::Path) -> Ledger {
    Ledger::new(Arc::new(SledStore::open(path).unwrap())).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Control

// Premise of every collision test: the id really ignores provenance, so a second entry that differs
// only in its governance receipt has the identical id. Fails if the hash starts covering provenance
// (in which case the collision tests below no longer describe the code).
#[tokio::test]
async fn control_entry_id_ignores_provenance() {
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    let mut second = first.clone();
    second.provenance = ProvenanceRef::Governance {
        receipt_id: "gov:coop:2:receipt".to_string(),
        decision_hash: "sha256:other".to_string(),
    };
    assert_eq!(first.id, second.id, "same hashed fields => same id");
    assert_ne!(first.provenance, second.provenance);
}

// Distinct ids behave: three spaced, otherwise identical entries are three journal entries and the
// ledger verifies. Fails if the harness itself, not the id scheme, were the cause of the failures
// below.
#[tokio::test]
async fn control_spaced_identical_shapes_get_distinct_ids_and_verify() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b) = (did(), did());
    for i in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        ledger
            .append_entry(gov_entry(&a, &b, &format!("gov:coop:{i}:receipt")))
            .await
            .unwrap();
    }
    assert_eq!(ledger.get_all_entries().unwrap().len(), 3);
    assert_eq!(ledger.get_balance(&b, CURRENCY), -3 * AMOUNT); // debit-positive convention
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Invariants (fail today)

// Appending the exact same entry twice must not apply it twice.
#[tokio::test]
async fn invariant_the_same_entry_appended_twice_is_applied_once() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");

    ledger.append_entry(entry.clone()).await.unwrap();
    let _ = ledger.append_entry(entry).await; // refusal or idempotent success are both acceptable

    assert_eq!(ledger.get_all_entries().unwrap().len(), 1);
    assert_eq!(
        ledger.get_balance(&b, CURRENCY),
        -AMOUNT,
        "applied exactly once"
    );
    ledger
        .verify_integrity()
        .expect("cached balances must match the journal");
}

// Two entries with the same id but DIFFERENT governance receipts must not silently replace one
// another: the first receipt's journal entry must survive, and balances must match the journal.
#[tokio::test]
async fn invariant_same_id_different_receipt_does_not_overwrite_the_journal() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    let mut second = first.clone();
    second.provenance = ProvenanceRef::Governance {
        receipt_id: "gov:coop:2:receipt".to_string(),
        decision_hash: "sha256:other".to_string(),
    };

    ledger.append_entry(first.clone()).await.unwrap();
    let _ = ledger.append_entry(second).await;

    let stored = ledger
        .get_entry(first.id.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.provenance, first.provenance,
        "first receipt's entry must survive"
    );
    ledger
        .verify_integrity()
        .expect("cached balances must match the journal");
}

// The real builder, in a tight loop, with distinct receipts: every authorised entry must be
// journaled and the ledger must verify. Same-millisecond builds collide on id today.
#[tokio::test]
#[ignore = "fails today: same-millisecond identical-shape entries share an id; the second is refused (icn#2871 part 2, owner decision on entry identity)"]
async fn invariant_rapid_distinct_receipts_are_all_journaled() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b) = (did(), did());
    const N: i64 = 200;
    for i in 0..N {
        ledger
            .append_entry(gov_entry(&a, &b, &format!("gov:coop:{i}:receipt")))
            .await
            .unwrap();
    }
    assert_eq!(
        ledger.get_all_entries().unwrap().len() as i64,
        N,
        "one journal entry per receipt"
    );
    assert_eq!(ledger.get_balance(&b, CURRENCY), -N * AMOUNT);
    ledger
        .verify_integrity()
        .expect("cached balances must match the journal");
}

// Control: a restart WITHOUT a collision keeps the balance and still verifies, so the
// characterization below fails to verify because of the collision and not because of the restart.
#[tokio::test]
async fn control_restart_without_a_collision_verifies() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (did(), did());
    {
        let mut ledger = open(tmp.path());
        ledger
            .append_entry(gov_entry(&a, &b, "gov:coop:1:receipt"))
            .await
            .unwrap();
        ledger.verify_integrity().unwrap();
    }
    let reopened = open(tmp.path());
    assert_eq!(reopened.get_balance(&b, CURRENCY), -AMOUNT);
    reopened.verify_integrity().unwrap();
}

// A refused collision leaves nothing behind: the first entry is the only journal record, its
// deltas are applied once, and that agreement is what a restart reloads. (Before icn#2871 the
// second append succeeded, the cache held both deltas against one journal record, and the
// persisted disagreement survived a restart.) The collision is built deterministically (same id,
// different provenance), not by racing the clock.
#[tokio::test]
async fn invariant_a_refused_collision_leaves_a_ledger_that_verifies_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    let second = with_other_receipt(&first);
    {
        let mut ledger = open(tmp.path());
        ledger.append_entry(first).await.unwrap();
        let err = ledger.append_entry(second).await.unwrap_err();
        assert!(
            err.to_string().contains("already journaled"),
            "refused as a collision, not for another reason: {err}"
        );
        assert_eq!(ledger.get_all_entries().unwrap().len(), 1);
        assert_eq!(ledger.get_balance(&b, CURRENCY), -AMOUNT, "applied once");
        ledger.verify_integrity().unwrap();
    } // dropped: in-memory cache gone
    let reopened = open(tmp.path());
    assert_eq!(reopened.get_balance(&b, CURRENCY), -AMOUNT);
    reopened.verify_integrity().unwrap();
}

/// The same entry under a different governance receipt: identical id, different provenance.
fn with_other_receipt(entry: &JournalEntry) -> JournalEntry {
    let mut other = entry.clone();
    other.provenance = ProvenanceRef::Governance {
        receipt_id: "gov:coop:2:receipt".to_string(),
        decision_hash: "sha256:other".to_string(),
    };
    other
}

// ---------------------------------------------------------------------------------------------
// Batch append: a collision is refused in pre-validation, so no leg of the batch is committed.

#[tokio::test]
async fn invariant_a_batch_containing_an_already_journaled_id_commits_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b, c) = (did(), did(), did());
    let journaled = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(journaled.clone()).await.unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let fresh = gov_entry(&a, &c, "gov:coop:3:receipt");
    assert_ne!(fresh.id, journaled.id, "premise: the first leg is new");

    let err = ledger
        .append_entries(vec![fresh, with_other_receipt(&journaled)])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already journaled"), "{err}");
    assert_eq!(
        ledger.get_all_entries().unwrap().len(),
        1,
        "no leg committed"
    );
    assert_eq!(
        ledger.get_balance(&c, CURRENCY),
        0,
        "the new leg was not applied"
    );
    ledger.verify_integrity().unwrap();
}

#[tokio::test]
async fn invariant_a_batch_repeating_an_id_commits_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    let second = with_other_receipt(&first);

    let err = ledger
        .append_entries(vec![first, second])
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("more than once in this batch"),
        "{err}"
    );
    assert!(
        ledger.get_all_entries().unwrap().is_empty(),
        "no leg committed"
    );
    assert_eq!(ledger.get_balance(&b, CURRENCY), 0);
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Gossip: an `EntryResponse` is accepted without a known-id pre-check (unlike `NewEntry`). A peer
// replaying the response for an entry this node already holds must not re-apply its deltas.

#[tokio::test]
async fn invariant_a_replayed_entry_response_does_not_reapply_balances() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ledger = open(tmp.path());
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");
    let hash = ledger.append_entry(entry.clone()).await.unwrap();

    for replayed in [entry.clone(), with_other_receipt(&entry)] {
        ledger
            .handle_sync_message(LedgerSyncMessage::EntryResponse {
                hash: hash.clone(),
                entry: Some(replayed),
            })
            .await
            .unwrap(); // the handler logs a refused store and returns Ok
    }

    let stored = ledger.get_entry(&hash).unwrap().unwrap();
    assert_eq!(
        stored.provenance, entry.provenance,
        "journal record unchanged"
    );
    assert_eq!(ledger.get_balance(&b, CURRENCY), -AMOUNT, "applied once");
    ledger.verify_integrity().unwrap();
}
