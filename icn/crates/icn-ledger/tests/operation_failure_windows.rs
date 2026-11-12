//! ADR-0091 S0b: characterization of the failure windows of `Ledger::append_operation`.
//!
//! This file needs `append_operation` (#2879) and the create-only journal write (#2873). It
//! changes no production code and does not choose between the designs ADR-0091 proposes.
//!
//! Naming: `characterization_*` asserts what the code does TODAY, including known gaps, and is
//! expected to flip when a gap is closed. `*_invariant` tests state the guarantee a gap-closing
//! change must provide; they are `#[ignore]`d with the reason and are RED if run today. Every
//! "must not" assertion has a positive control.
//!
//! What this establishes, and what it does not:
//! - D1: two `Ledger` values over one store do not share their in-memory operation index.
//! - D5: after a PROCESS abort with the journal row flushed, the derived rows the append had not
//!   yet written are not repaired by opening the ledger. No power-loss claim is made. The
//!   abort follows an explicit `flush()`, because an unflushed tail is lost on SIGKILL
//!   (S0a, #2882) and that loss is not an index-repair defect.
//! - D6: when the journal write is applied but reported as failed, the in-memory operation index
//!   never learns the entry, so a retry under a new content hash is applied again.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::Did;
use icn_ledger::types::ProvenanceRef;
use icn_ledger::{
    AccountDelta, ContentHash, JournalEntry, Ledger, LedgerError, OperationAppend, OperationId,
};
use icn_store::{CompareAndPut, ReplicaMetadata, SledStore, Store};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PAYER: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
const PAYEE: &str = "did:icn:zH3C2AVvLMv6gmMNam3uVAjZpfkcJCwDwnZn6z3wXmqPV";
const UNIT: &str = "hours";
const T0: u64 = 1_762_819_200_000;

const OP_A: OperationId = OperationId::from_bytes([0xA1; 32]);
const OP_B: OperationId = OperationId::from_bytes([0xB2; 32]);

const JOURNAL: &[u8] = b"ledger:journal:";
const JOURNAL_TS: &[u8] = b"ledger:journal_ts:";

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

fn payer() -> Did {
    Did::from_str(PAYER).unwrap()
}

fn payee() -> Did {
    Did::from_str(PAYEE).unwrap()
}

fn transfer(timestamp: u64, operation: Option<OperationId>, amount: i64) -> JournalEntry {
    let (from, to) = (payer(), payee());
    let mut entry = JournalEntry {
        id: None,
        timestamp,
        author: from.clone(),
        contract_ref: None,
        accounts: vec![
            AccountDelta::debit(from.clone(), UNIT.to_string(), amount),
            AccountDelta::credit(to, UNIT.to_string(), amount),
        ],
        parents: vec![],
        signature: None,
        nonce: operation.map(OperationId::to_bytes),
        provenance: ProvenanceRef::DirectMember {
            did: from,
            auth_proof: "system-executed".to_string(),
        },
    };
    entry.compute_hash().unwrap();
    entry
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
// Fault-injection store
// ---------------------------------------------------------------------------------------------

/// What the next journal `compare_and_put` does. Only keys under `ledger:journal:` are affected
/// (not `ledger:journal_ts:`). Every other `Store` method forwards unchanged, `flush` included.
enum Behavior {
    Forward,
    /// Wait until two journal writes have arrived, then forward both: the two writers have
    /// then both passed their operation-index check before either has written.
    Rendezvous,
    /// Fail once BEFORE the inner write: nothing is persisted. Control for `AppliedThenErr`.
    ErrBeforeApply,
    /// Apply the inner write, then report an error once: an indeterminate commit.
    ErrAfterApply,
    /// On the n-th journal write: apply it, `flush()` the inner store, then abort the process.
    AbortAfterNth(usize),
}

struct ProbeStore {
    inner: SledStore,
    behavior: Mutex<Behavior>,
    journal_cas_calls: AtomicUsize,
    arrivals: AtomicUsize,
}

impl ProbeStore {
    fn open(path: &Path, behavior: Behavior) -> Arc<Self> {
        Arc::new(Self {
            inner: SledStore::open(path).unwrap(),
            behavior: Mutex::new(behavior),
            journal_cas_calls: AtomicUsize::new(0),
            arrivals: AtomicUsize::new(0),
        })
    }

    fn arm(&self, behavior: Behavior) {
        *self.behavior.lock().unwrap() = behavior;
    }

    fn as_store(self: &Arc<Self>) -> Arc<dyn Store> {
        self.clone()
    }
}

impl Store for ProbeStore {
    fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
        self.inner.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
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
        if !key.starts_with(JOURNAL) {
            return self.inner.compare_and_put(key, expected, new);
        }
        let call = self.journal_cas_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let mut guard = self.behavior.lock().unwrap();
        match &*guard {
            Behavior::Forward => {
                drop(guard);
                self.inner.compare_and_put(key, expected, new)
            }
            Behavior::Rendezvous => {
                drop(guard);
                self.arrivals.fetch_add(1, Ordering::SeqCst);
                let deadline = Instant::now() + Duration::from_secs(20);
                while self.arrivals.load(Ordering::SeqCst) < 2 {
                    assert!(
                        Instant::now() < deadline,
                        "rendezvous: the second writer never arrived"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                self.inner.compare_and_put(key, expected, new)
            }
            Behavior::ErrBeforeApply => {
                *guard = Behavior::Forward;
                anyhow::bail!("injected: journal write failed before it was applied")
            }
            Behavior::ErrAfterApply => {
                *guard = Behavior::Forward;
                drop(guard);
                let applied = self.inner.compare_and_put(key, expected, new)?;
                assert!(
                    matches!(applied, CompareAndPut::Applied),
                    "premise: the injected write must have been applied"
                );
                anyhow::bail!("injected: journal write applied, then reported as failed")
            }
            Behavior::AbortAfterNth(n) => {
                let n = *n;
                drop(guard);
                let result = self.inner.compare_and_put(key, expected, new);
                if call == n {
                    // The journal row must be durable, or the premise is "unflushed tail lost".
                    Store::flush(&self.inner).expect("flush before abort");
                    std::process::abort();
                }
                result
            }
        }
    }
    fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.scan(prefix)
    }
    fn scan_count(&self, prefix: &[u8]) -> anyhow::Result<usize> {
        self.inner.scan_count(prefix)
    }
    fn flush(&self) -> anyhow::Result<()> {
        Store::flush(&self.inner)
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

/// Self-check of the harness: the injection point fires, and `ErrAfterApply` really applies.
/// Without this, a wrapper that forgot to intercept `compare_and_put` would make D6 vacuous.
#[test]
fn harness_self_check_injection_points_fire() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProbeStore::open(dir.path(), Behavior::ErrAfterApply);
    let key = b"ledger:journal:probe";

    let err = store.compare_and_put(key, None, b"v").unwrap_err();
    assert!(err.to_string().contains("applied, then reported"), "{err}");
    assert_eq!(
        store.get(key).unwrap().as_deref(),
        Some(&b"v"[..]),
        "applied"
    );

    let key2 = b"ledger:journal:probe2";
    store.arm(Behavior::ErrBeforeApply);
    assert!(store.compare_and_put(key2, None, b"v").is_err());
    assert_eq!(store.get(key2).unwrap(), None, "control: not applied");

    // A key outside the journal prefix is not touched by the injection.
    store.arm(Behavior::ErrAfterApply);
    store
        .compare_and_put(b"ledger:journal_ts:x", None, b"v")
        .unwrap();
    assert_eq!(store.journal_cas_calls.load(Ordering::SeqCst), 2);
}

// ---------------------------------------------------------------------------------------------
// D1: two live ledgers over one store
// ---------------------------------------------------------------------------------------------

/// Control: one ledger answers the second attempt of an operation with the first entry.
#[tokio::test]
async fn control_one_ledger_applies_an_operation_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = open(dir.path());
    let first = transfer(T0, Some(OP_A), 5);
    let second = transfer(T0 + 1, Some(OP_A), 5);
    assert_ne!(id(&first), id(&second), "premise: distinct content hashes");

    assert!(matches!(
        ledger.append_operation(first.clone()).await.unwrap(),
        OperationAppend::Applied(_)
    ));
    assert_eq!(
        ledger.append_operation(second).await.unwrap(),
        OperationAppend::Replayed(id(&first))
    );
    assert_eq!(journal_len(&ledger), 1);
    assert_eq!(received(&ledger), 5);
}

/// D1 (sequential form). Both ledgers are opened before either appends, so neither index knows
/// the operation. The second is not told by the first.
#[tokio::test]
async fn characterization_two_live_ledgers_same_operation_both_apply() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SledStore::open(dir.path()).unwrap());
    let mut a = Ledger::new(store.clone()).unwrap();
    let mut b = Ledger::new(store.clone()).unwrap();
    let (ea, eb) = (transfer(T0, Some(OP_A), 5), transfer(T0 + 1, Some(OP_A), 5));

    let ra = a.append_operation(ea.clone()).await.unwrap();
    let rb = b.append_operation(eb.clone()).await.unwrap();

    assert_eq!(ra, OperationAppend::Applied(id(&ea)));
    assert_eq!(rb, OperationAppend::Applied(id(&eb)), "b was not told");
    assert_eq!(journal_len(&a), 2, "two journal rows for one operation");
    let nonces: Vec<_> = a
        .get_all_entries()
        .unwrap()
        .iter()
        .map(|e| e.nonce)
        .collect();
    assert!(nonces.iter().all(|n| *n == Some(OP_A.to_bytes())));

    drop((a, b, store));
    assert_reopened_state_is_a_double_application(dir.path()).await;
}

/// D1 (racing form). The store holds both writers at the journal write until both have passed
/// their operation-index check, so both are in the window at once; this does not depend on
/// scheduling. Each ledger runs on its own thread.
#[test]
fn characterization_two_live_ledgers_racing_on_one_operation_both_apply() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProbeStore::open(dir.path(), Behavior::Rendezvous);
    let (ea, eb) = (transfer(T0, Some(OP_A), 5), transfer(T0 + 1, Some(OP_A), 5));
    let a = Ledger::new(store.as_store()).unwrap();
    let b = Ledger::new(store.as_store()).unwrap();

    let run = |mut ledger: Ledger, entry: JournalEntry| {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let outcome = rt.block_on(ledger.append_operation(entry));
            (ledger, outcome)
        })
    };
    let (ta, tb) = (run(a, ea.clone()), run(b, eb.clone()));
    let (a, ra) = ta.join().unwrap();
    let (b, rb) = tb.join().unwrap();

    assert_eq!(
        store.arrivals.load(Ordering::SeqCst),
        2,
        "both reached the write"
    );
    assert_eq!(ra.unwrap(), OperationAppend::Applied(id(&ea)));
    assert_eq!(rb.unwrap(), OperationAppend::Applied(id(&eb)));
    // What each live instance believes, versus the journal:
    assert_eq!(received(&a), 5, "a's cache only holds its own entry");
    assert_eq!(received(&b), 5, "b's cache only holds its own entry");
    assert_eq!(journal_len(&a), 2);

    drop((a, b, store));
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(assert_reopened_state_is_a_double_application(dir.path()));
}

/// Shared by both D1 forms: what a restart sees.
async fn assert_reopened_state_is_a_double_application(path: &Path) {
    let mut ledger = open(path);
    assert_eq!(journal_len(&ledger), 2);
    let retry = ledger
        .append_operation(transfer(T0 + 60_000, Some(OP_A), 5))
        .await
        .unwrap_err();
    assert!(
        matches!(
            typed(&retry),
            LedgerError::OperationConflict { entries: 2, .. }
        ),
        "{retry}"
    );
    ledger.recompute_balances().unwrap();
    assert_eq!(
        received(&ledger),
        10,
        "the journal folds to a double payment"
    );
}

/// The guarantee a fix must provide: of two live ledgers, one applies and one is told.
#[tokio::test]
#[ignore = "FUT: needs a store-level operation record (ADR-0091 D3); RED today"]
async fn two_live_ledgers_same_operation_apply_once_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn Store> = Arc::new(SledStore::open(dir.path()).unwrap());
    let mut a = Ledger::new(store.clone()).unwrap();
    let mut b = Ledger::new(store.clone()).unwrap();
    a.append_operation(transfer(T0, Some(OP_A), 5))
        .await
        .unwrap();
    let second = b.append_operation(transfer(T0 + 1, Some(OP_A), 5)).await;
    assert!(
        matches!(second, Ok(OperationAppend::Replayed(_))),
        "second writer must be told, got {second:?}"
    );
    assert_eq!(journal_len(&a), 1);
}

// ---------------------------------------------------------------------------------------------
// D5: abort between the journal write and the derived rows
// ---------------------------------------------------------------------------------------------

const CHILD_ENV: &str = "ICN_S0B_D5_CHILD";
const CHILD_PATH_ENV: &str = "ICN_S0B_D5_PATH";

/// Child half of D5. A no-op unless re-executed by the parent with the env switch.
/// `abort`: entry 2's journal write is applied and flushed, then the process aborts.
/// `control`: the same two appends run to completion.
#[test]
fn s0b_d5_child_entry() {
    let Ok(mode) = std::env::var(CHILD_ENV) else {
        return;
    };
    let path = std::env::var(CHILD_PATH_ENV).expect("child path");
    // Watchdog: a hung child must not outlive the parent's patience.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(120));
        std::process::abort();
    });
    let behavior = if mode == "abort" {
        Behavior::AbortAfterNth(2)
    } else {
        Behavior::Forward
    };
    let store = ProbeStore::open(Path::new(&path), behavior);
    let mut ledger = Ledger::new(store.as_store()).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        ledger
            .append_operation(transfer(T0, Some(OP_A), 5))
            .await
            .unwrap();
        // Entry 2: in `abort` mode the process dies inside this call.
        ledger
            .append_operation(transfer(T0 + 10, Some(OP_B), 7))
            .await
            .unwrap();
    });
    // The control child flushes so the parent reads a complete store; the abort child never
    // reaches here.
    ledger_flush(&store);
}

fn ledger_flush(store: &Arc<ProbeStore>) {
    store.flush().unwrap();
}

struct ChildRun {
    signal: Option<i32>,
    success: bool,
}

fn run_child(mode: &str, path: &Path) -> ChildRun {
    use std::os::unix::process::ExitStatusExt;
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "s0b_d5_child_entry",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, mode)
        .env(CHILD_PATH_ENV, path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(150);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    ChildRun {
        signal: status.signal(),
        success: status.success(),
    }
}

/// Counts of the three kinds of row, read through a raw store (before any ledger opens it, since
/// opening a ledger may itself write).
fn raw_counts(path: &Path) -> (usize, usize) {
    let store = SledStore::open(path).unwrap();
    (
        store.scan_count(JOURNAL).unwrap(),
        store.scan_count(JOURNAL_TS).unwrap(),
    )
}

/// Control: with no abort, both entries have a journal row and a timestamp row, and the ledger
/// is consistent. Without this, the abort case could not be told from a broken harness.
#[test]
fn control_d5_without_an_abort_the_derived_rows_exist() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_child("control", dir.path());
    assert!(run.success, "control child must finish normally");
    assert_eq!(raw_counts(dir.path()), (2, 2));

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        let ledger = open(dir.path());
        assert_eq!(ledger.get_entries_paginated_asc(0, 10).unwrap().1, 2);
        assert_eq!(received(&ledger), 12);
        ledger.verify_integrity().unwrap();
    });
}

#[test]
fn characterization_abort_between_journal_and_derived_rows() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_child("abort", dir.path());
    assert_eq!(run.signal, Some(6), "premise: the child died of SIGABRT");

    // Before any ledger opens the store: entry 2's journal row is durable, its timestamp row
    // and derived state were never written.
    let (journal, ts) = raw_counts(dir.path());
    assert_eq!(journal, 2, "premise: both journal rows are durable");
    assert_eq!(ts, 1, "entry 2 has no timestamp row");

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        let mut ledger = open(dir.path());

        // Opening the ledger did not repair the index: it only rebuilds when it is empty.
        let (journal, ts) = {
            // Count through the ledger's own view to avoid a second handle on the sled lock.
            let page = ledger.get_entries_paginated_asc(0, 10).unwrap();
            (journal_len(&ledger), page.1)
        };
        assert_eq!(journal, 2, "the journal holds both entries");
        assert_eq!(
            ts, 1,
            "the timestamp index still counts one: not repaired at open"
        );

        // The cached balances never saw entry 2.
        assert_eq!(received(&ledger), 5);
        assert!(
            ledger.verify_integrity().is_err(),
            "the stale balance cache is detectable by verify_integrity"
        );

        // The retry of the interrupted operation is answered from the journal: at-most-once
        // holds. `Replayed` does not mean the derived state is complete.
        let retry = ledger
            .append_operation(transfer(T0 + 60_000, Some(OP_B), 7))
            .await
            .unwrap();
        assert!(matches!(retry, OperationAppend::Replayed(_)), "{retry:?}");
        assert_eq!(journal_len(&ledger), 2);

        // Explicit recompute is what repairs the balances; the index is still not rebuilt.
        ledger.recompute_balances().unwrap();
        assert_eq!(received(&ledger), 12);
    });
}

/// The guarantee a fix must provide: opening the ledger after the interruption leaves the
/// timestamp index and balances matching the journal.
#[test]
#[ignore = "FUT: needs open-time derived-state parity repair (plan S5); RED today"]
fn open_after_an_interrupted_append_repairs_derived_state_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let run = run_child("abort", dir.path());
    assert_eq!(run.signal, Some(6));
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        let ledger = open(dir.path());
        assert_eq!(ledger.get_entries_paginated_asc(0, 10).unwrap().1, 2);
        ledger.verify_integrity().unwrap();
    });
}

// ---------------------------------------------------------------------------------------------
// D6: the journal write is applied but reported as failed
// ---------------------------------------------------------------------------------------------

/// Control: the same failure injected BEFORE the write persists nothing, and the retry applies
/// once. This is the case the "failed append is retryable" assumption is correct for.
#[tokio::test]
async fn control_d6_a_failure_before_the_write_is_safely_retried() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProbeStore::open(dir.path(), Behavior::ErrBeforeApply);
    let mut ledger = Ledger::new(store.as_store()).unwrap();

    assert!(ledger
        .append_operation(transfer(T0, Some(OP_A), 5))
        .await
        .is_err());
    assert_eq!(journal_len(&ledger), 0, "nothing persisted");

    let retry = ledger
        .append_operation(transfer(T0 + 5, Some(OP_A), 5))
        .await
        .unwrap();
    assert!(matches!(retry, OperationAppend::Applied(_)));
    assert_eq!(journal_len(&ledger), 1);
    assert_eq!(received(&ledger), 5);
}

/// D6. The write is applied and the caller is told it failed. The in-memory index is recorded
/// only after the write returns `Ok`, so it does not know the entry; the retry (a new content
/// hash, so a new journal key) is applied again.
#[tokio::test]
async fn characterization_retry_after_indeterminate_commit() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProbeStore::open(dir.path(), Behavior::ErrAfterApply);
    let mut ledger = Ledger::new(store.as_store()).unwrap();
    let first = transfer(T0, Some(OP_A), 5);
    let retry = transfer(T0 + 5, Some(OP_A), 5);
    assert_ne!(id(&first), id(&retry), "premise: distinct content hashes");

    let failed = ledger.append_operation(first.clone()).await;
    assert!(failed.is_err(), "the caller was told the append failed");
    assert_eq!(journal_len(&ledger), 1, "premise: the entry is journaled");

    let outcome = ledger.append_operation(retry.clone()).await.unwrap();

    assert_eq!(outcome, OperationAppend::Applied(id(&retry)));
    assert_eq!(
        journal_len(&ledger),
        2,
        "two journal rows for one operation"
    );
    drop((ledger, store));
    assert_reopened_state_is_a_double_application_of(dir.path(), 10).await;
}

async fn assert_reopened_state_is_a_double_application_of(path: &Path, folded: i64) {
    let mut ledger = open(path);
    let again = ledger
        .append_operation(transfer(T0 + 60_000, Some(OP_A), 5))
        .await
        .unwrap_err();
    assert!(
        matches!(
            typed(&again),
            LedgerError::OperationConflict { entries: 2, .. }
        ),
        "{again}"
    );
    ledger.recompute_balances().unwrap();
    assert_eq!(received(&ledger), folded);
}

/// A retry that rebuilds the identical entry (same millisecond, so same content hash) is
/// refused by the create-only journal write instead: #2873 protects that narrower case.
#[tokio::test]
async fn characterization_indeterminate_commit_same_hash_retry_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProbeStore::open(dir.path(), Behavior::ErrAfterApply);
    let mut ledger = Ledger::new(store.as_store()).unwrap();
    let first = transfer(T0, Some(OP_A), 5);

    assert!(ledger.append_operation(first.clone()).await.is_err());
    let retry = ledger.append_operation(first.clone()).await.unwrap_err();

    assert!(
        matches!(typed(&retry), LedgerError::EntryAlreadyJournaled(_)),
        "{retry}"
    );
    assert_eq!(journal_len(&ledger), 1);
}

/// The guarantee a fix must provide: a retry after an indeterminate commit is answered from
/// the store, whatever the retry's content hash.
#[tokio::test]
#[ignore = "FUT: needs a store-reading operation lookup (ADR-0091 D3); RED today"]
async fn indeterminate_commit_retry_is_replayed_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProbeStore::open(dir.path(), Behavior::ErrAfterApply);
    let mut ledger = Ledger::new(store.as_store()).unwrap();
    let first = transfer(T0, Some(OP_A), 5);
    assert!(ledger.append_operation(first.clone()).await.is_err());

    let outcome = ledger
        .append_operation(transfer(T0 + 5, Some(OP_A), 5))
        .await;
    assert!(
        matches!(outcome, Ok(OperationAppend::Replayed(_))),
        "got {outcome:?}"
    );
    assert_eq!(journal_len(&ledger), 1);
}
