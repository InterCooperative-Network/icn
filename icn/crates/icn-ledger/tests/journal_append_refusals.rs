#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Journal id refusals (icn#2871): typed, side-effect free, and enforced at the storage boundary.
//!
//! Companion to `journal_entry_identity.rs`, which proves that a journaled id is refused. This file
//! proves how:
//!
//! 1. **Typed.** An identical re-append is `EntryAlreadyJournaled`, different content under the
//!    same id is `EntryIdCollision`, an id repeated inside a batch is `EntryIdRepeatedInBatch`.
//! 2. **Side-effect free.** A refusal leaves the journal record byte-for-byte as it was, leaves
//!    cached balances alone and leaves no decision-index row of its own.
//! 3. **Create-only write.** A writer that passed the read-side check just before another writer
//!    journaled the id (modelled by hiding one journal read) is refused by the journal write
//!    itself, not by the check. A store that cannot compare-and-put atomically refuses to journal
//!    anything rather than overwrite.
//! 4. **Errors are not absences.** A failed journal read is an error, never "not journaled".
//!
//! `FaultStore` delegates to a real `SledStore`; its switches only hide or fail operations.
//!
//! Not covered: crash atomicity of the journal write versus the balance cache (one
//! characterization below pins the observable state after a failure between them).

use icn_identity::{Did, IdentityBundle};
use icn_ledger::{
    entry::JournalEntryBuilder,
    types::{JournalEntry, ProvenanceRef},
    Ledger, LedgerError, LedgerSyncMessage,
};
use icn_store::{CompareAndPut, CompareAndPutUnsupported, ReplicaMetadata, SledStore, Store};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const AMOUNT: i64 = 5;
const CURRENCY: &str = "HOURS";

/// Persisted key layouts the assertions read directly. These are compatibility surfaces; a change
/// to either must fail here.
const JOURNAL_PREFIX: &str = "ledger:journal:";
const DECISION_INDEX_PREFIX: &str = "ledger:decision_idx:";

// ---------------------------------------------------------------------------------------------
// Harness

struct FaultStore {
    inner: SledStore,
    /// Hide the next read of a journal record (it reads as absent once).
    hide_next_journal_get: AtomicBool,
    /// Fail every read of a journal record.
    fail_journal_get: AtomicBool,
    /// Fail every `put` whose key starts with this prefix.
    fail_put_prefix: Mutex<Option<Vec<u8>>>,
    /// Behave like a backend with no atomic compare-and-put (the trait default).
    cas_unsupported: AtomicBool,
}

impl FaultStore {
    fn open(path: &std::path::Path) -> Arc<Self> {
        Arc::new(Self {
            inner: SledStore::open(path).unwrap(),
            hide_next_journal_get: AtomicBool::new(false),
            fail_journal_get: AtomicBool::new(false),
            fail_put_prefix: Mutex::new(None),
            cas_unsupported: AtomicBool::new(false),
        })
    }

    fn journal_record(&self, entry: &JournalEntry) -> Option<Vec<u8>> {
        self.inner.get(journal_key(entry).as_bytes()).unwrap()
    }

    fn decision_index_rows(&self, decision_hash: &str) -> Vec<String> {
        let prefix = format!("{DECISION_INDEX_PREFIX}{decision_hash}:");
        self.inner
            .scan(prefix.as_bytes())
            .unwrap()
            .into_iter()
            .map(|(k, _)| String::from_utf8(k).unwrap())
            .collect()
    }
}

/// A journal record key, and not the `ledger:journal_ts:` timestamp index.
fn is_journal_record(key: &[u8]) -> bool {
    key.starts_with(JOURNAL_PREFIX.as_bytes())
}

impl Store for FaultStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        if is_journal_record(key) {
            if self.fail_journal_get.load(Ordering::SeqCst) {
                anyhow::bail!("injected journal read failure");
            }
            if self.hide_next_journal_get.swap(false, Ordering::SeqCst) {
                return Ok(None);
            }
        }
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        if let Some(prefix) = self.fail_put_prefix.lock().unwrap().as_ref() {
            if key.starts_with(prefix) {
                anyhow::bail!("injected put failure");
            }
        }
        self.inner.put(key, value)
    }
    fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
        self.inner.delete(key)
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> anyhow::Result<()> {
        self.inner.delete_atomic(keys)
    }
    fn compare_and_put(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new: &[u8],
    ) -> anyhow::Result<CompareAndPut> {
        if self.cas_unsupported.load(Ordering::SeqCst) {
            return Err(CompareAndPutUnsupported.into());
        }
        self.inner.compare_and_put(key, expected, new)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn get_replica_metadata(&self, h: &[u8; 32]) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<[u8; 32]>> {
        self.inner.list_replica_hashes()
    }
    fn flush(&self) -> anyhow::Result<()> {
        Store::flush(&self.inner)
    }
}

fn did() -> Did {
    IdentityBundle::generate().unwrap().did().clone()
}

/// A balanced governance-provenance entry built by the real builder (wall-clock ms, no nonce).
fn gov_entry(author: &Did, to: &Did, receipt: &str) -> JournalEntry {
    JournalEntryBuilder::new(author.clone())
        .debit(author.clone(), CURRENCY.to_string(), AMOUNT)
        .credit(to.clone(), CURRENCY.to_string(), AMOUNT)
        .with_governance_provenance(receipt, format!("sha256:{receipt}"))
        .build()
        .unwrap()
}

/// Same id as `entry` (the id does not cover provenance), different receipt and decision.
fn with_other_decision(entry: &JournalEntry) -> JournalEntry {
    let mut other = entry.clone();
    other.provenance = ProvenanceRef::Governance {
        receipt_id: "gov:coop:2:receipt".to_string(),
        decision_hash: OTHER_DECISION.to_string(),
    };
    other
}
const OTHER_DECISION: &str = "sha256:other";

fn decision_of(entry: &JournalEntry) -> String {
    match &entry.provenance {
        ProvenanceRef::Governance { decision_hash, .. } => decision_hash.clone(),
        other => panic!("test entry must carry governance provenance, has {other:?}"),
    }
}

fn journal_key(entry: &JournalEntry) -> String {
    format!("{JOURNAL_PREFIX}{}", entry.id.as_ref().unwrap().to_hex())
}

fn ledger_over(store: &Arc<FaultStore>) -> Ledger {
    Ledger::new(store.clone()).unwrap()
}

fn ledger_error(err: &anyhow::Error) -> &LedgerError {
    err.downcast_ref::<LedgerError>()
        .unwrap_or_else(|| panic!("expected a typed LedgerError, got: {err:#}"))
}

/// What a refusal must leave untouched: the journal record bytes and both parties' balances.
struct Snapshot {
    record: Option<Vec<u8>>,
    entries: usize,
    from: i64,
    to: i64,
}

fn snapshot(
    store: &FaultStore,
    ledger: &Ledger,
    e: &JournalEntry,
    from: &Did,
    to: &Did,
) -> Snapshot {
    Snapshot {
        record: store.journal_record(e),
        entries: ledger.get_all_entries().unwrap().len(),
        from: ledger.get_balance(from, CURRENCY),
        to: ledger.get_balance(to, CURRENCY),
    }
}

fn assert_unchanged(before: &Snapshot, after: &Snapshot) {
    assert_eq!(
        before.record, after.record,
        "journal record bytes unchanged"
    );
    assert_eq!(
        before.entries, after.entries,
        "journal entry count unchanged"
    );
    assert_eq!(before.from, after.from, "debited balance unchanged");
    assert_eq!(before.to, after.to, "credited balance unchanged");
}

// ---------------------------------------------------------------------------------------------
// 1 + 2. Typed refusals with no side effect

#[tokio::test]
async fn an_identical_reappend_is_refused_as_already_journaled_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(entry.clone()).await.unwrap();
    let before = snapshot(&store, &ledger, &entry, &a, &b);

    let err = ledger.append_entry(entry.clone()).await.unwrap_err();

    assert!(
        matches!(ledger_error(&err), LedgerError::EntryAlreadyJournaled(id) if *id == entry.id.as_ref().unwrap().to_hex()),
        "{err}"
    );
    assert_unchanged(&before, &snapshot(&store, &ledger, &entry, &a, &b));
    assert_eq!(ledger.get_balance(&b, CURRENCY), -AMOUNT, "applied once");
    ledger.verify_integrity().unwrap();
}

#[tokio::test]
async fn different_content_under_a_journaled_id_is_refused_as_a_collision_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(first.clone()).await.unwrap();
    let before = snapshot(&store, &ledger, &first, &a, &b);

    let err = ledger
        .append_entry(with_other_decision(&first))
        .await
        .unwrap_err();

    assert!(
        matches!(ledger_error(&err), LedgerError::EntryIdCollision(_)),
        "{err}"
    );
    assert_unchanged(&before, &snapshot(&store, &ledger, &first, &a, &b));
    assert!(
        store.decision_index_rows(OTHER_DECISION).is_empty(),
        "refused before the decision index write"
    );
    ledger.verify_integrity().unwrap();
}

// The refusal comes before every side effect, including the validation hook's quarantine: a
// colliding entry that the hook would reject is refused as a collision and quarantined nowhere.
// (The create-only write alone would also refuse it, but only after the hook had run.)
#[tokio::test]
async fn a_journaled_id_is_refused_before_the_validation_hook_can_quarantine_it() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    ledger.set_validation_hook(|entry| match &entry.provenance {
        ProvenanceRef::Governance { decision_hash, .. } if decision_hash == OTHER_DECISION => {
            anyhow::bail!("rejected by the test hook")
        }
        _ => Ok(()),
    });
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(first.clone()).await.unwrap();
    let before = snapshot(&store, &ledger, &first, &a, &b);

    let err = ledger
        .append_entry(with_other_decision(&first))
        .await
        .unwrap_err();

    assert!(
        matches!(ledger_error(&err), LedgerError::EntryIdCollision(_)),
        "refused as a collision, before the hook ran: {err:#}"
    );
    assert_eq!(
        ledger.quarantine().count().unwrap(),
        0,
        "nothing quarantined"
    );
    assert_unchanged(&before, &snapshot(&store, &ledger, &first, &a, &b));
    ledger.verify_integrity().unwrap();
}

// Same refusals through the sync route, which appends without broadcasting.
#[tokio::test]
async fn append_entry_from_sync_refuses_a_journaled_id_by_type() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(entry.clone()).await.unwrap();
    let before = snapshot(&store, &ledger, &entry, &a, &b);

    let identical = ledger
        .append_entry_from_sync(entry.clone())
        .await
        .unwrap_err();
    let different = ledger
        .append_entry_from_sync(with_other_decision(&entry))
        .await
        .unwrap_err();

    assert!(matches!(
        ledger_error(&identical),
        LedgerError::EntryAlreadyJournaled(_)
    ));
    assert!(matches!(
        ledger_error(&different),
        LedgerError::EntryIdCollision(_)
    ));
    assert_unchanged(&before, &snapshot(&store, &ledger, &entry, &a, &b));
    ledger.verify_integrity().unwrap();
}

#[tokio::test]
async fn batch_refusals_are_typed_and_commit_no_leg() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b, c) = (did(), did(), did());
    let journaled = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(journaled.clone()).await.unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let fresh = gov_entry(&a, &c, "gov:coop:3:receipt");
    assert_ne!(fresh.id, journaled.id, "premise: the fresh leg is new");

    // An id repeated inside the batch, neither copy journaled.
    let repeated = ledger
        .append_entries(vec![fresh.clone(), fresh.clone()])
        .await
        .unwrap_err();
    assert!(
        matches!(ledger_error(&repeated), LedgerError::EntryIdRepeatedInBatch(id) if *id == fresh.id.as_ref().unwrap().to_hex()),
        "{repeated}"
    );

    // A fresh leg first, then an identical copy of a journaled entry.
    let identical = ledger
        .append_entries(vec![fresh.clone(), journaled.clone()])
        .await
        .unwrap_err();
    assert!(
        matches!(
            ledger_error(&identical),
            LedgerError::EntryAlreadyJournaled(_)
        ),
        "{identical}"
    );

    assert_eq!(
        ledger.get_all_entries().unwrap().len(),
        1,
        "no leg committed"
    );
    assert!(store.journal_record(&fresh).is_none());
    assert_eq!(ledger.get_balance(&c, CURRENCY), 0, "fresh leg not applied");
    assert_eq!(ledger.get_balance(&b, CURRENCY), -AMOUNT);
    ledger.verify_integrity().unwrap();

    // Control: the same fresh leg on its own is accepted, so the refusals above were about ids.
    ledger.append_entries(vec![fresh]).await.unwrap();
    assert_eq!(ledger.get_balance(&c, CURRENCY), -AMOUNT);
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// 3. The create-only journal write is the guarantee, not the read-side check

// A writer that passed the read-side check just before another writer journaled the id: the
// journal write itself refuses it. The first record survives byte-for-byte, balances move once,
// the refused writer's decision-index row is removed, and all of it holds across a restart.
#[tokio::test]
async fn a_writer_that_passed_the_check_is_refused_by_the_create_only_write() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (did(), did());
    let first = gov_entry(&a, &b, "gov:coop:1:receipt");
    {
        let store = FaultStore::open(tmp.path());
        let mut ledger = ledger_over(&store);
        ledger.append_entry(first.clone()).await.unwrap();
        let before = snapshot(&store, &ledger, &first, &a, &b);

        store.hide_next_journal_get.store(true, Ordering::SeqCst);
        let err = ledger
            .append_entry(with_other_decision(&first))
            .await
            .unwrap_err();
        assert!(
            !store.hide_next_journal_get.load(Ordering::SeqCst),
            "premise: the read-side check consumed the hidden read, so the write refused"
        );

        assert!(
            matches!(ledger_error(&err), LedgerError::EntryIdCollision(_)),
            "{err}"
        );
        assert_unchanged(&before, &snapshot(&store, &ledger, &first, &a, &b));
        assert!(
            store.decision_index_rows(OTHER_DECISION).is_empty(),
            "the refused writer's decision-index row is removed"
        );
        assert_eq!(
            store.decision_index_rows(&decision_of(&first)).len(),
            1,
            "the journaled entry keeps its row"
        );
        ledger.verify_integrity().unwrap();
    } // ledger and store dropped: sled lock released, in-memory cache gone

    let store = FaultStore::open(tmp.path());
    let reopened = ledger_over(&store);
    assert_eq!(reopened.get_all_entries().unwrap().len(), 1);
    assert_eq!(reopened.get_balance(&b, CURRENCY), -AMOUNT);
    let stored = reopened
        .get_entry(first.id.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(stored.provenance, first.provenance, "first record survives");
    reopened.verify_integrity().unwrap();
}

// The identical case through the same window: refused as already journaled, and the decision-index
// row it shares with the journaled entry (same key) is NOT removed.
#[tokio::test]
async fn an_identical_writer_refused_by_the_write_keeps_the_shared_decision_index_row() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");
    ledger.append_entry(entry.clone()).await.unwrap();
    let before = snapshot(&store, &ledger, &entry, &a, &b);

    store.hide_next_journal_get.store(true, Ordering::SeqCst);
    let err = ledger.append_entry(entry.clone()).await.unwrap_err();

    assert!(
        matches!(ledger_error(&err), LedgerError::EntryAlreadyJournaled(_)),
        "{err}"
    );
    assert_unchanged(&before, &snapshot(&store, &ledger, &entry, &a, &b));
    assert_eq!(
        store.decision_index_rows(&decision_of(&entry)).len(),
        1,
        "the shared row stays"
    );
    let (page, total) = ledger
        .get_entries_by_decision_hash(&decision_of(&entry), 0, 10)
        .unwrap();
    assert_eq!((page.len(), total), (1, 1), "still found by its decision");
    ledger.verify_integrity().unwrap();
}

// When the record that won the id cannot be parsed, the refused writer cannot prove the index row
// is not that record's, so it leaves the row. (A stray hint is harmless; lookups re-read the
// journal.)
#[tokio::test]
async fn a_writer_refused_by_an_unparseable_record_leaves_the_index_row() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");
    store
        .inner
        .put(journal_key(&entry).as_bytes(), b"not a journal entry")
        .unwrap();

    store.hide_next_journal_get.store(true, Ordering::SeqCst);
    let err = ledger.append_entry(entry.clone()).await.unwrap_err();

    assert!(
        matches!(ledger_error(&err), LedgerError::EntryIdCollision(_)),
        "{err}"
    );
    assert_eq!(
        store.journal_record(&entry).as_deref(),
        Some(b"not a journal entry".as_slice()),
        "the existing record is not replaced"
    );
    assert_eq!(store.decision_index_rows(&decision_of(&entry)).len(), 1);
    assert_eq!(ledger.get_balance(&b, CURRENCY), 0, "nothing applied");
}

// A backend that cannot compare-and-put atomically journals nothing: the append fails closed
// instead of falling back to an overwriting put.
#[tokio::test]
async fn a_store_without_atomic_create_journals_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");

    store.cas_unsupported.store(true, Ordering::SeqCst);
    let err = ledger.append_entry(entry.clone()).await.unwrap_err();

    assert!(
        err.downcast_ref::<CompareAndPutUnsupported>().is_some(),
        "{err:#}"
    );
    assert!(store.journal_record(&entry).is_none());
    assert!(ledger.get_all_entries().unwrap().is_empty());
    assert_eq!(ledger.get_balance(&b, CURRENCY), 0);
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// 4. A failed read is an error, not an absence

#[tokio::test]
async fn a_failed_journal_read_refuses_the_append_untyped() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");

    store.fail_journal_get.store(true, Ordering::SeqCst);
    let err = ledger.append_entry(entry.clone()).await.unwrap_err();
    store.fail_journal_get.store(false, Ordering::SeqCst);

    assert!(
        err.downcast_ref::<LedgerError>().is_none(),
        "a storage failure is not reported as a journal-id refusal: {err:#}"
    );
    assert!(store.journal_record(&entry).is_none());
    assert_eq!(ledger.get_balance(&b, CURRENCY), 0);
}

// EntryResponse skips the two known refusals quietly, and propagates a store failure from its
// known-id check instead of swallowing it.
#[tokio::test]
async fn entry_response_propagates_a_store_failure_and_skips_known_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");
    let hash = ledger.append_entry(entry.clone()).await.unwrap();
    let before = snapshot(&store, &ledger, &entry, &a, &b);
    let response = |e: &JournalEntry| LedgerSyncMessage::EntryResponse {
        hash: hash.clone(),
        entry: Some(e.clone()),
    };

    store.fail_journal_get.store(true, Ordering::SeqCst);
    let err = ledger
        .handle_sync_message(response(&entry))
        .await
        .unwrap_err();
    store.fail_journal_get.store(false, Ordering::SeqCst);
    assert!(
        err.to_string().contains("injected journal read failure"),
        "{err:#}"
    );

    ledger.handle_sync_message(response(&entry)).await.unwrap();
    ledger
        .handle_sync_message(response(&with_other_decision(&entry)))
        .await
        .unwrap();

    assert_unchanged(&before, &snapshot(&store, &ledger, &entry, &a, &b));
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Retry after a failure past the journal write

// CHARACTERIZATION, not an endorsement. `append_entry` can return Err AFTER its journal write
// (here: the balance-cache save fails). A caller that retries the identical entry is refused as
// already journaled, so the deltas are never applied twice. Nothing completes the append either:
// that is the journaled-but-unfinished state, whose completion is an open owner decision.
#[tokio::test]
async fn characterization_a_retry_after_a_post_journal_failure_is_refused_not_reapplied() {
    let tmp = tempfile::tempdir().unwrap();
    let store = FaultStore::open(tmp.path());
    let mut ledger = ledger_over(&store);
    let (a, b) = (did(), did());
    let entry = gov_entry(&a, &b, "gov:coop:1:receipt");

    *store.fail_put_prefix.lock().unwrap() = Some(b"ledger:balance:".to_vec());
    let first = ledger.append_entry(entry.clone()).await;
    *store.fail_put_prefix.lock().unwrap() = None;
    assert!(first.is_err(), "premise: the append reported a failure");
    assert!(
        store.journal_record(&entry).is_some(),
        "premise: the failure came after the journal write"
    );

    let retry = ledger.append_entry(entry.clone()).await.unwrap_err();

    assert!(
        matches!(ledger_error(&retry), LedgerError::EntryAlreadyJournaled(_)),
        "{retry}"
    );
    assert_eq!(ledger.get_all_entries().unwrap().len(), 1);
    assert_eq!(
        ledger.get_balance(&b, CURRENCY),
        -AMOUNT,
        "applied once in memory, not twice"
    );
}
