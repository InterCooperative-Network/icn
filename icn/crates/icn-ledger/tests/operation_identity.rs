//! Operation identity at the ledger boundary (icn#2871 part 2).
//!
//! A journal entry id is a digest of content that includes a millisecond timestamp, so it can
//! tell neither a retry from a new operation nor two same-shape operations apart. These tests
//! pin what `Ledger::append_operation` adds when the author names the operation:
//!
//! 1. Two operations of one shape in one millisecond are two entries.
//! 2. A retry of one operation, in the same or a later millisecond, appends nothing and is
//!    answered with the entry appended the first time.
//! 3. An operation id cannot be reused for different content.
//! 4. All of that holds after the store is reopened and across the two failure windows of an
//!    append (before and after the journal write). An entry that left the journal is not
//!    reported as journaled.
//! 5. The plain append path, and the commons pair that shares one nonce across two entries,
//!    behave exactly as before.
//!
//! The `control_*` tests run the same scenarios through `append_entry` and pin today's
//! behaviour of the plain path. They are what the operation path is measured against, not a
//! statement that the plain path is correct.
//!
//! Every timestamp is fixed and every failure is injected. Nothing depends on wall-clock timing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::{Did, IdentityBundle};
use icn_ledger::entry::JournalEntryBuilder;
use icn_ledger::types::ProvenanceRef;
use icn_ledger::{
    AccountDelta, ContentHash, JournalEntry, Ledger, LedgerError, OperationAppend, OperationId,
};
use icn_store::{CompareAndPut, ReplicaMetadata, SledStore, Store};
use std::path::Path;
use std::sync::{Arc, Mutex};

const PAYER: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const PAYEE: &str = "did:icn:zH3C2AVvLMv6gmMNam3uVAjZpfkcJCwDwnZn6z3wXmqPV";
const UNIT: &str = "hours";
const T0: u64 = 1_762_819_200_000; // fixed millisecond

const OP_A: OperationId = OperationId::from_bytes([0xA1; 32]);
const OP_B: OperationId = OperationId::from_bytes([0xB2; 32]);
const OP_C: OperationId = OperationId::from_bytes([0xC3; 32]);

fn payer() -> Did {
    Did::from_str(PAYER).unwrap()
}

fn payee() -> Did {
    Did::from_str(PAYEE).unwrap()
}

/// A transfer with a fixed timestamp: debit `from`, credit `to`.
fn transfer_between(
    from: &Did,
    to: &Did,
    timestamp: u64,
    operation: Option<OperationId>,
    amount: i64,
) -> JournalEntry {
    let mut entry = JournalEntry {
        id: None,
        timestamp,
        author: from.clone(),
        contract_ref: None,
        accounts: vec![
            AccountDelta::debit(from.clone(), UNIT.to_string(), amount),
            AccountDelta::credit(to.clone(), UNIT.to_string(), amount),
        ],
        parents: vec![],
        signature: None,
        nonce: operation.map(OperationId::to_bytes),
        provenance: ProvenanceRef::DirectMember {
            did: from.clone(),
            auth_proof: "system-executed".to_string(),
        },
    };
    entry.compute_hash().unwrap();
    entry
}

/// The settlement shape: payer to payee.
fn transfer(timestamp: u64, operation: Option<OperationId>, amount: i64) -> JournalEntry {
    transfer_between(&payer(), &payee(), timestamp, operation, amount)
}

fn id(entry: &JournalEntry) -> ContentHash {
    entry.id.clone().expect("compute_hash sets the id")
}

fn open(path: &Path) -> Ledger {
    Ledger::new(Arc::new(SledStore::open(path).unwrap())).unwrap()
}

fn journal_len(ledger: &Ledger) -> usize {
    ledger.get_all_entries().unwrap().len()
}

/// What the payee holds. Balances are signed by convention, so compare magnitudes.
fn received(ledger: &Ledger) -> i64 {
    ledger.get_balance(&payee(), UNIT).abs()
}

fn typed(err: &anyhow::Error) -> &LedgerError {
    err.downcast_ref::<LedgerError>()
        .unwrap_or_else(|| panic!("expected a typed LedgerError, got: {err:#}"))
}

// ---------------------------------------------------------------------------------------------
// Distinct operations
// ---------------------------------------------------------------------------------------------

/// Control: without an operation id, two same-shape transfers in one millisecond are one id.
#[test]
fn control_plain_same_shape_transfers_in_one_millisecond_share_an_id() {
    assert_eq!(id(&transfer(T0, None, 5)), id(&transfer(T0, None, 5)));
}

#[tokio::test]
async fn distinct_operations_of_one_shape_in_one_millisecond_are_two_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let first = ledger
        .append_operation(transfer(T0, Some(OP_A), 5))
        .await
        .unwrap();
    let second = ledger
        .append_operation(transfer(T0, Some(OP_B), 5))
        .await
        .unwrap();

    assert!(matches!(first, OperationAppend::Applied(_)));
    assert!(matches!(second, OperationAppend::Applied(_)));
    assert_ne!(first.entry_id(), second.entry_id());
    assert_eq!(journal_len(&ledger), 2);
    assert_eq!(received(&ledger), 10);
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Retries
// ---------------------------------------------------------------------------------------------

/// Control: the plain path cannot recognise a retry built a millisecond later. Same operation
/// id, different entry id, appended and applied a second time.
#[tokio::test]
async fn control_plain_append_applies_a_later_retry_twice() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    ledger
        .append_entry(transfer(T0, Some(OP_A), 5))
        .await
        .unwrap();
    ledger
        .append_entry(transfer(T0 + 1, Some(OP_A), 5))
        .await
        .unwrap();

    assert_eq!(journal_len(&ledger), 2);
    assert_eq!(received(&ledger), 10);
}

#[tokio::test]
async fn a_retry_in_a_later_millisecond_is_answered_with_the_original_entry() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let original = transfer(T0, Some(OP_A), 5);
    let retry = transfer(T0 + 1, Some(OP_A), 5);
    assert_ne!(id(&original), id(&retry), "the retry is a different record");

    let first = ledger.append_operation(original.clone()).await.unwrap();
    let second = ledger.append_operation(retry).await.unwrap();

    assert_eq!(first, OperationAppend::Applied(id(&original)));
    assert_eq!(second, OperationAppend::Replayed(id(&original)));
    assert!(second.is_replay());
    assert_eq!(journal_len(&ledger), 1);
    assert_eq!(received(&ledger), 5);
    ledger.verify_integrity().unwrap();
}

#[tokio::test]
async fn a_retry_in_the_same_millisecond_is_a_replay() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let original = transfer(T0, Some(OP_A), 5);
    ledger.append_operation(original.clone()).await.unwrap();
    let second = ledger.append_operation(original.clone()).await.unwrap();

    assert_eq!(second, OperationAppend::Replayed(id(&original)));
    assert_eq!(journal_len(&ledger), 1);
    assert_eq!(received(&ledger), 5);
    ledger.verify_integrity().unwrap();
}

/// A retry is answered from the journal before anything is validated, so a rule that would
/// refuse a new entry (here a frozen payer) cannot turn an applied operation into an error.
#[tokio::test]
async fn a_retry_of_an_applied_operation_is_not_revalidated() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let original = transfer(T0, Some(OP_A), 5);
    ledger.append_operation(original.clone()).await.unwrap();
    ledger.freeze_member(payer(), "test freeze".to_string(), None);

    let refused = ledger
        .append_operation(transfer(T0 + 1, Some(OP_B), 5))
        .await;
    assert!(
        refused.is_err(),
        "a new operation by a frozen payer is refused"
    );

    let retry = ledger
        .append_operation(transfer(T0 + 2, Some(OP_A), 5))
        .await
        .unwrap();
    assert_eq!(retry, OperationAppend::Replayed(id(&original)));
    assert_eq!(journal_len(&ledger), 1);
}

/// Many holders of one ledger retry one operation at once. Exactly one append happens and every
/// caller is given the same entry.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_retries_of_one_operation_append_once() {
    const CALLERS: u64 = 16;
    let dir = tempfile::tempdir().unwrap();
    let ledger = Arc::new(tokio::sync::RwLock::new(open(dir.path())));
    let start = Arc::new(tokio::sync::Barrier::new(CALLERS as usize));

    let mut tasks = Vec::new();
    for i in 0..CALLERS {
        let ledger = ledger.clone();
        let start = start.clone();
        tasks.push(tokio::spawn(async move {
            // Each caller builds its own attempt, in its own millisecond.
            let attempt = transfer(T0 + i, Some(OP_A), 5);
            start.wait().await;
            ledger
                .write()
                .await
                .append_operation(attempt)
                .await
                .unwrap()
        }));
    }

    let mut outcomes = Vec::new();
    for task in tasks {
        outcomes.push(task.await.unwrap());
    }

    let applied = outcomes.iter().filter(|o| !o.is_replay()).count();
    assert_eq!(applied, 1, "exactly one caller appends");
    let winner = outcomes[0].entry_id().clone();
    assert!(
        outcomes.iter().all(|o| o.entry_id() == &winner),
        "every caller is answered with the same entry"
    );

    let ledger = ledger.read().await;
    assert_eq!(journal_len(&ledger), 1);
    assert_eq!(received(&ledger), 5);
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_operation_id_cannot_be_reused_for_different_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let original = transfer(T0, Some(OP_A), 5);
    ledger.append_operation(original.clone()).await.unwrap();

    let other = IdentityBundle::generate().unwrap().did().clone();
    let variants = [
        ("another amount", transfer(T0 + 1, Some(OP_A), 6)),
        (
            "another payee",
            transfer_between(&payer(), &other, T0 + 1, Some(OP_A), 5),
        ),
        (
            "another payer",
            transfer_between(&other, &payee(), T0 + 1, Some(OP_A), 5),
        ),
        ("another unit", {
            let mut e = transfer(T0 + 1, Some(OP_A), 5);
            for delta in &mut e.accounts {
                delta.currency = "credits".to_string();
            }
            e.compute_hash().unwrap();
            e
        }),
    ];

    for (what, entry) in variants {
        let err = ledger.append_operation(entry).await.unwrap_err();
        match typed(&err) {
            LedgerError::OperationConflict {
                operation,
                existing,
                entries,
            } => {
                assert_eq!(operation, &OP_A.to_hex(), "{what}");
                assert_eq!(existing, &id(&original).to_hex(), "{what}");
                assert_eq!(*entries, 1, "{what}");
            }
            other => panic!("{what}: expected OperationConflict, got {other:?}"),
        }
    }

    assert_eq!(journal_len(&ledger), 1, "a refusal appends nothing");
    assert_eq!(received(&ledger), 5, "a refusal changes no balance");
    ledger.verify_integrity().unwrap();
}

#[tokio::test]
async fn an_entry_without_an_operation_id_is_refused_as_an_operation() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let err = ledger
        .append_operation(transfer(T0, None, 5))
        .await
        .unwrap_err();
    assert!(matches!(typed(&err), LedgerError::OperationIdMissing));
    assert_eq!(journal_len(&ledger), 0);
    assert_eq!(received(&ledger), 0);
}

// ---------------------------------------------------------------------------------------------
// Durability
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn operation_identity_survives_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let original_a = transfer(T0, Some(OP_A), 5);
    let original_b = transfer(T0, Some(OP_B), 5);

    {
        let mut ledger = open(dir.path());
        ledger.append_operation(original_a.clone()).await.unwrap();
        ledger.append_operation(original_b.clone()).await.unwrap();
    }

    let mut ledger = open(dir.path());

    let retry_a = ledger
        .append_operation(transfer(T0 + 60_000, Some(OP_A), 5))
        .await
        .unwrap();
    assert_eq!(retry_a, OperationAppend::Replayed(id(&original_a)));

    let conflict = ledger
        .append_operation(transfer(T0 + 60_000, Some(OP_B), 6))
        .await
        .unwrap_err();
    assert!(matches!(
        typed(&conflict),
        LedgerError::OperationConflict { .. }
    ));

    let new = ledger
        .append_operation(transfer(T0 + 60_000, Some(OP_C), 5))
        .await
        .unwrap();
    assert!(matches!(new, OperationAppend::Applied(_)));

    assert_eq!(journal_len(&ledger), 3);
    assert_eq!(received(&ledger), 15);
    ledger.verify_integrity().unwrap();
}

/// The journal is the record. The index is only a hint: when an entry is no longer in the
/// journal, its operation is not reported as journaled, and a retry appends it again instead of
/// being answered with the id of an entry that does not exist.
///
/// The entry is removed through the store here because no ledger API that removes journal
/// entries is exercised by this test; balances are therefore not asserted. Whether a removal
/// should leave a permanent refusal behind needs a persisted record and is not decided.
#[tokio::test]
async fn an_operation_whose_entry_left_the_journal_is_not_reported_as_journaled() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(dir.path()).unwrap());
    let mut ledger = Ledger::new(store.clone()).unwrap();

    let kept = transfer(T0, Some(OP_A), 5);
    let removed = transfer(T0, Some(OP_B), 5);
    ledger.append_operation(kept.clone()).await.unwrap();
    ledger.append_operation(removed.clone()).await.unwrap();

    let key = format!("ledger:journal:{}", id(&removed).to_hex());
    store.delete(key.as_bytes()).unwrap();
    assert_eq!(journal_len(&ledger), 1);

    let kept_retry = ledger
        .append_operation(transfer(T0 + 1, Some(OP_A), 5))
        .await
        .unwrap();
    assert_eq!(kept_retry, OperationAppend::Replayed(id(&kept)));

    let again = transfer(T0 + 1, Some(OP_B), 5);
    let reapplied = ledger.append_operation(again.clone()).await.unwrap();
    assert_eq!(reapplied, OperationAppend::Applied(id(&again)));
    assert_eq!(journal_len(&ledger), 2);

    // And the replacement is the one a further retry is answered with.
    let retry = ledger
        .append_operation(transfer(T0 + 2, Some(OP_B), 5))
        .await
        .unwrap();
    assert_eq!(retry, OperationAppend::Replayed(id(&again)));
    assert_eq!(journal_len(&ledger), 2);
}

// ---------------------------------------------------------------------------------------------
// Failure windows
// ---------------------------------------------------------------------------------------------

const JOURNAL: &[u8] = b"ledger:journal:";
const JOURNAL_TS: &[u8] = b"ledger:journal_ts:";

/// A real sled store whose `put` can be made to fail for one key prefix.
struct FaultStore {
    inner: SledStore,
    fail_put_prefix: Mutex<Option<&'static [u8]>>,
}

impl FaultStore {
    fn open(path: &Path) -> Arc<Self> {
        Arc::new(Self {
            inner: SledStore::open(path).unwrap(),
            fail_put_prefix: Mutex::new(None),
        })
    }

    fn fail_puts_to(&self, prefix: Option<&'static [u8]>) {
        *self.fail_put_prefix.lock().unwrap() = prefix;
    }
}

impl Store for FaultStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
        if let Some(prefix) = *self.fail_put_prefix.lock().unwrap() {
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
        self.inner.compare_and_put(key, expected, new)
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn get_replica_metadata(
        &self,
        hash: &icn_store::ContentHash,
    ) -> anyhow::Result<Option<ReplicaMetadata>> {
        self.inner.get_replica_metadata(hash)
    }
    fn put_replica_metadata(&self, metadata: &ReplicaMetadata) -> anyhow::Result<()> {
        self.inner.put_replica_metadata(metadata)
    }
    fn list_replica_hashes(&self) -> anyhow::Result<Vec<icn_store::ContentHash>> {
        self.inner.list_replica_hashes()
    }
}

/// Window 1: the journal write itself fails. Nothing is journaled, so the operation is not
/// applied and a retry applies it exactly once.
#[tokio::test]
async fn a_failed_journal_write_leaves_the_operation_unapplied() {
    let dir = tempfile::tempdir().unwrap();
    let store = FaultStore::open(dir.path());
    let mut ledger = Ledger::new(store.clone()).unwrap();

    store.fail_puts_to(Some(JOURNAL));
    let failed = ledger.append_operation(transfer(T0, Some(OP_A), 5)).await;
    assert!(failed.is_err());
    assert_eq!(journal_len(&ledger), 0);

    store.fail_puts_to(None);
    let retry = transfer(T0 + 1, Some(OP_A), 5);
    let outcome = ledger.append_operation(retry.clone()).await.unwrap();
    assert_eq!(outcome, OperationAppend::Applied(id(&retry)));

    assert_eq!(journal_len(&ledger), 1);
    assert_eq!(received(&ledger), 5);
    ledger.verify_integrity().unwrap();
}

/// Control for window 2: the write right after the journal write fails. The entry is journaled
/// but the append reports an error. On the plain path a retry journals the operation a second
/// time, and the journal then says the payee received twice.
#[tokio::test]
async fn control_plain_retry_after_a_post_journal_failure_journals_twice() {
    let dir = tempfile::tempdir().unwrap();
    let store = FaultStore::open(dir.path());
    let mut ledger = Ledger::new(store.clone()).unwrap();

    store.fail_puts_to(Some(JOURNAL_TS));
    let failed = ledger.append_entry(transfer(T0, Some(OP_A), 5)).await;
    assert!(failed.is_err());
    assert_eq!(journal_len(&ledger), 1, "journaled despite the error");

    store.fail_puts_to(None);
    ledger
        .append_entry(transfer(T0 + 1, Some(OP_A), 5))
        .await
        .unwrap();

    assert_eq!(journal_len(&ledger), 2);
    ledger.recompute_balances().unwrap();
    assert_eq!(
        received(&ledger),
        10,
        "the journal carries the transfer twice"
    );
}

/// Window 2 on the operation path: the retry is recognised, in the same process and after a
/// reopen, and never journals a second entry. The failed attempt left the cached balances
/// behind the journal; that is reported by `verify_integrity`, not hidden, and recomputing from
/// the journal yields the operation exactly once.
#[tokio::test]
async fn a_failure_after_the_journal_write_is_not_applied_twice_by_a_retry() {
    let dir = tempfile::tempdir().unwrap();
    let original = transfer(T0, Some(OP_A), 5);

    {
        let store = FaultStore::open(dir.path());
        let mut ledger = Ledger::new(store.clone()).unwrap();

        store.fail_puts_to(Some(JOURNAL_TS));
        let failed = ledger.append_operation(original.clone()).await;
        assert!(failed.is_err());
        assert_eq!(journal_len(&ledger), 1, "journaled despite the error");

        store.fail_puts_to(None);
        let retry = ledger
            .append_operation(transfer(T0 + 1, Some(OP_A), 5))
            .await
            .unwrap();
        assert_eq!(retry, OperationAppend::Replayed(id(&original)));
        assert_eq!(journal_len(&ledger), 1);

        // Journaled is not complete: the interrupted append never reached the balance cache.
        assert_eq!(received(&ledger), 0);
        assert!(
            ledger.verify_integrity().is_err(),
            "the stale balance cache must be detectable"
        );
    }

    let mut ledger = open(dir.path());
    let retry = ledger
        .append_operation(transfer(T0 + 60_000, Some(OP_A), 5))
        .await
        .unwrap();
    assert_eq!(retry, OperationAppend::Replayed(id(&original)));
    assert_eq!(journal_len(&ledger), 1);

    ledger.recompute_balances().unwrap();
    assert_eq!(received(&ledger), 5, "exactly once, from the journal");
    ledger.verify_integrity().unwrap();
}

// ---------------------------------------------------------------------------------------------
// The plain path and existing nonce users are unchanged
// ---------------------------------------------------------------------------------------------

/// The commons settlement pair gives both of its entries one nonce (the receipt hash). That
/// predates operation identity and must keep working through the plain path. Because two
/// entries carry the nonce, it cannot name one operation: using it as an operation id is
/// refused as ambiguous instead of being matched against either entry.
#[tokio::test]
async fn the_commons_pair_sharing_one_nonce_still_appends() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());
    let receipt = [0x5E; 32];

    let earn =
        icn_ledger::commons_credits::build_earn_entry_with_receipt(&payee(), 5, receipt).unwrap();
    let spend =
        icn_ledger::commons_credits::build_spend_entry_with_receipt(&payer(), 5, receipt).unwrap();
    assert_eq!(earn.nonce, Some(receipt));
    assert_eq!(spend.nonce, Some(receipt));

    let hashes = ledger
        .append_entries(vec![earn.clone(), spend.clone()])
        .await
        .unwrap();
    assert_eq!(hashes.len(), 2);
    assert_eq!(journal_len(&ledger), 2);
    ledger.verify_integrity().unwrap();

    let err = ledger.append_operation(earn).await.unwrap_err();
    match typed(&err) {
        LedgerError::OperationConflict { entries, .. } => assert_eq!(*entries, 2),
        other => panic!("expected OperationConflict, got {other:?}"),
    }
    assert_eq!(journal_len(&ledger), 2);
}

/// A refusal names the lowest of the entry ids that carry the operation id. The index holds ids
/// in append order while the ledger runs and in journal-key order after a reopen, so one journal
/// must produce one refusal either way. Both append orders are run: in one of them the entry
/// appended first is not the lowest.
#[tokio::test]
async fn a_refusal_names_the_same_entry_before_and_after_a_reopen() {
    fn refusal(err: anyhow::Error) -> (String, usize) {
        match typed(&err) {
            LedgerError::OperationConflict {
                existing, entries, ..
            } => (existing.clone(), *entries),
            other => panic!("expected OperationConflict, got {other:?}"),
        }
    }

    // Two entries with one nonce and different content, appended through the plain path.
    let five = transfer(T0, Some(OP_A), 5);
    let six = transfer(T0, Some(OP_A), 6);
    let lowest = [id(&five), id(&six)]
        .iter()
        .map(ContentHash::to_hex)
        .min()
        .unwrap();

    for order in [[&five, &six], [&six, &five]] {
        let dir = tempfile::tempdir().unwrap();
        let mut ledger = open(dir.path());
        for entry in order {
            ledger.append_entry(entry.clone()).await.unwrap();
        }

        let probe = transfer(T0 + 1, Some(OP_A), 5);
        let running = refusal(ledger.append_operation(probe.clone()).await.unwrap_err());
        assert_eq!(running, (lowest.clone(), 2));
        drop(ledger);

        let mut reopened = open(dir.path());
        let after_reopen = refusal(reopened.append_operation(probe).await.unwrap_err());
        assert_eq!(after_reopen, (lowest.clone(), 2));
        assert_eq!(journal_len(&reopened), 2);
    }
}

/// Naming the operation is setting the nonce: no second encoding, no change to any entry id.
#[test]
fn naming_the_operation_is_setting_the_nonce() {
    let named = JournalEntryBuilder::new(payer())
        .debit(payer(), UNIT.to_string(), 5)
        .credit(payee(), UNIT.to_string(), 5)
        .operation(OP_A)
        .with_direct_member_provenance(payer(), "system-executed".to_string())
        .build()
        .unwrap();
    assert_eq!(named.nonce, Some(OP_A.to_bytes()));

    let by_hand = transfer(named.timestamp, Some(OP_A), 5);
    assert_eq!(id(&named), id(&by_hand));
}

/// Entries without a nonce are untouched: the plain path still accepts them and nothing about
/// them is indexed or compared.
#[tokio::test]
async fn plain_appends_without_a_nonce_are_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());

    let first = transfer(T0, None, 5);
    let second = transfer(T0 + 1, None, 5);
    assert_eq!(
        ledger.append_entry(first.clone()).await.unwrap(),
        id(&first)
    );
    assert_eq!(
        ledger.append_entry(second.clone()).await.unwrap(),
        id(&second)
    );

    assert_eq!(journal_len(&ledger), 2);
    assert_eq!(received(&ledger), 10);
    ledger.verify_integrity().unwrap();
}
