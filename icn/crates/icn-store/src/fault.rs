//! Storage fault containment: a meaning-neutral latch shared by every handle of one backend.
//!
//! A storage error during a write can leave the write's outcome unknown: on sled 0.34.7 a
//! multi-key commit that fails part-way may leave some of its writes visible in memory, and can
//! even recover a prefix of them after a restart (see [`crate::Store::compare_and_write`]).
//! Reads do not notice: sled keeps serving its in-memory state. A [`FaultDomain`] turns such an
//! error into a **fault**: from the moment the failing call returns, every operation on every
//! handle sharing the domain is refused with [`StoreFaulted`] until the database is reopened by a
//! new owner. The store gives the fault no meaning; interpreting and reconciling the data behind
//! it is the caller's job.
//!
//! # Guarantees
//!
//! 1. **Publication before return.** A failing write publishes the fault before it returns, so
//!    any operation that *begins* after the failing call returned is refused.
//! 2. **No overlap with multi-key mutations.** Transactions (and therefore `compare_and_write`
//!    and `delete_atomic`) hold the domain exclusively, from before they start until any fault
//!    is published. No read or write on a handle of the domain can observe a multi-key mutation
//!    part-way, or between its failure and the fault's publication. Reads and single-key writes
//!    share the domain with each other.
//! 3. **Faults are permanent for the domain.** A fault is recorded once (the first one wins) and
//!    is never cleared. Cloning or sharing a handle shares the domain; there is no method that
//!    clears it. Only a new owner opening the database creates a new domain.
//! 4. **What is not a fault.** A refused request (validation, unsupported backend), a comparison
//!    conflict, a transaction closure's own abort (sled discards a transaction's staged writes
//!    when its closure fails), and a read error do not latch. A read error is returned as is.
//!
//! # Limits
//!
//! - The domain covers the [`crate::Store`] and [`crate::TransactionalStore`] methods and the
//!   inherent flushes of a [`crate::SledStore`]. Raw access through `SledStore::db()` bypasses it.
//! - The domain belongs to the handle that opened the database and to every handle shared from
//!   it. `SledStore::from_db` over an already-open `sled::Db` starts an independent domain unless
//!   it is given the owner's domain (`SledStore::from_db_in`).
//! - A reader that *overlaps* a failing single-key write may observe either value of that key,
//!   as it may for any concurrent write; guarantee 1 covers every later reader.
//! - Local to one process. It is not consensus, and it does not repair anything.

use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// The first storage fault recorded in a [`FaultDomain`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "store is faulted: a {operation} failed with an unknown outcome ({detail}); every read and \
     write is refused until the database is reopened and reconciled"
)]
pub struct StoreFaulted {
    /// The operation whose failure faulted the store.
    pub operation: &'static str,
    /// The backend's error, as text.
    pub detail: String,
}

/// The error returned by the write that faulted the store: it may have applied none, some or
/// all of its writes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{operation} failed and its outcome is unknown (none, some or all of its writes may be \
     applied); the store is now faulted: {detail}"
)]
pub struct StoreOutcomeUnknown {
    /// The operation that failed.
    pub operation: &'static str,
    /// The backend's error, as text.
    pub detail: String,
}

/// A fault latch and access gate shared by every handle of one backend instance.
#[derive(Debug, Default)]
pub struct FaultDomain {
    gate: RwLock<()>,
    fault: OnceLock<StoreFaulted>,
}

impl FaultDomain {
    /// A fresh, unfaulted domain.
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded fault, if any.
    pub fn fault(&self) -> Option<StoreFaulted> {
        self.fault.get().cloned()
    }

    fn latch(&self, operation: &'static str, detail: String) {
        let _ = self.fault.set(StoreFaulted { operation, detail });
    }

    fn refuse_if_faulted(&self) -> anyhow::Result<()> {
        match self.fault.get() {
            Some(f) => Err(f.clone().into()),
            None => Ok(()),
        }
    }

    fn shared(&self, operation: &'static str) -> anyhow::Result<RwLockReadGuard<'_, ()>> {
        // A poisoned gate means a thread panicked inside an operation: fail closed.
        let guard = self.gate.read().map_err(|_| {
            self.latch(operation, "access gate poisoned by a panic".into());
            anyhow::Error::from(self.fault().unwrap_or(StoreFaulted {
                operation,
                detail: "access gate poisoned".into(),
            }))
        })?;
        self.refuse_if_faulted()?;
        Ok(guard)
    }

    fn exclusive(&self, operation: &'static str) -> anyhow::Result<RwLockWriteGuard<'_, ()>> {
        let guard = self.gate.write().map_err(|_| {
            self.latch(operation, "access gate poisoned by a panic".into());
            anyhow::Error::from(self.fault().unwrap_or(StoreFaulted {
                operation,
                detail: "access gate poisoned".into(),
            }))
        })?;
        self.refuse_if_faulted()?;
        Ok(guard)
    }

    /// Run a read. Refused if faulted; a read error is returned as is and does not latch.
    pub(crate) fn read<T>(
        &self,
        operation: &'static str,
        f: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let _guard = self.shared(operation)?;
        f()
    }

    /// Run a single-key write (or a flush). Refused if faulted; any error, or a panic, latches
    /// the fault before the domain is released, and an error is reported as
    /// [`StoreOutcomeUnknown`].
    pub(crate) fn write_single<T>(
        &self,
        operation: &'static str,
        f: impl FnOnce() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let _guard = self.shared(operation)?;
        let _on_panic = LatchOnUnwind {
            domain: self,
            operation,
        };
        f().map_err(|e| self.fail(operation, e))
    }

    /// Run a multi-key mutation exclusively. Refused if faulted; an error for which
    /// `ambiguous` is true latches the fault while the domain is still held exclusively.
    pub(crate) fn write_exclusive<T>(
        &self,
        operation: &'static str,
        f: impl FnOnce() -> anyhow::Result<T>,
        ambiguous: impl FnOnce(&anyhow::Error) -> bool,
    ) -> anyhow::Result<T> {
        let _guard = self.exclusive(operation)?;
        // Declared after the guard, so it is dropped first: a panic latches the fault while the
        // domain is still held exclusively (the gate is then also poisoned).
        let _on_panic = LatchOnUnwind {
            domain: self,
            operation,
        };
        match f() {
            Ok(v) => Ok(v),
            Err(e) if ambiguous(&e) => Err(self.fail(operation, e)),
            Err(e) => Err(e),
        }
    }

    fn fail(&self, operation: &'static str, e: anyhow::Error) -> anyhow::Error {
        let detail = format!("{e:#}");
        self.latch(operation, detail.clone());
        StoreOutcomeUnknown { operation, detail }.into()
    }

    /// Check the latch without holding the gate (for async paths that cannot hold a guard
    /// across an await). Pair with [`FaultDomain::fail_async`].
    pub(crate) fn check(&self) -> anyhow::Result<()> {
        self.refuse_if_faulted()
    }

    /// Latch a failure observed on an async path and return its error.
    pub(crate) fn fail_async(&self, operation: &'static str, e: anyhow::Error) -> anyhow::Error {
        self.fail(operation, e)
    }
}

/// Latches the domain's fault if a write unwinds: a panic part-way through a write leaves its
/// outcome unknown. (std's `RwLock` is poisoned only by a panic under a *write* guard, so the
/// shared-guard paths need this to fail closed.) Reads do not use it: a panicking read changes
/// nothing.
struct LatchOnUnwind<'a> {
    domain: &'a FaultDomain,
    operation: &'static str,
}

impl Drop for LatchOnUnwind<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.domain
                .latch(self.operation, "panicked during the operation".into());
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::{
        CompareAndWrite, CompareAndWriteInvalid, Expectation, Mutation, SledStore, Store,
        StoreTransactionError, TransactionalStore,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn storage_error() -> anyhow::Error {
        StoreTransactionError::StorageError {
            details: "injected".into(),
        }
        .into()
    }

    fn is_storage(e: &anyhow::Error) -> bool {
        matches!(
            e.downcast_ref::<StoreTransactionError>(),
            Some(StoreTransactionError::StorageError { .. })
        )
    }

    /// A multi-key mutation that applies the first `applied` of `keys` to the database and then
    /// fails with a storage error, run through the store's own exclusive gate.
    fn failing_commit(store: &SledStore, keys: &[&[u8]], applied: usize) -> anyhow::Error {
        store
            .fault
            .write_exclusive(
                "transaction",
                || {
                    for k in &keys[..applied] {
                        store.db.insert(*k, b"new".as_slice())?;
                    }
                    Err::<(), _>(storage_error())
                },
                is_storage,
            )
            .unwrap_err()
    }

    fn assert_faulted(r: anyhow::Result<impl std::fmt::Debug>) {
        let e = r.unwrap_err();
        assert!(e.downcast_ref::<StoreFaulted>().is_some(), "{e:#}");
    }

    fn every_operation_is_refused(store: &SledStore) {
        assert_faulted(store.get(b"a"));
        assert_faulted(store.scan(b""));
        assert_faulted(store.scan_keys(b""));
        assert_faulted(store.scan_count(b""));
        assert_faulted(store.put(b"x", b"y"));
        assert_faulted(store.delete(b"x"));
        assert_faulted(store.compare_and_put(b"x", None, b"y"));
        assert_faulted(store.delete_atomic(&[b"x".to_vec()]));
        assert_faulted(store.compare_and_write(
            &[Expectation {
                key: b"x",
                value: None,
            }],
            &[Mutation {
                key: b"x",
                value: Some(b"y"),
            }],
        ));
        assert_faulted(store.transaction(|_| Ok(())));
        assert_faulted(Store::flush(store));
        assert_faulted(store.list_replica_hashes());
    }

    #[test]
    fn an_ambiguous_commit_faults_the_store_whether_none_some_or_all_writes_applied() {
        let keys: [&[u8]; 2] = [b"a", b"b"];
        for applied in 0..=keys.len() {
            let store = SledStore::temporary().unwrap();
            store.put(b"a", b"old").unwrap();
            let err = failing_commit(&store, &keys, applied);
            let unknown = err.downcast_ref::<StoreOutcomeUnknown>().expect("typed");
            assert_eq!(unknown.operation, "transaction");
            assert_eq!(store.fault().unwrap().operation, "transaction");
            every_operation_is_refused(&store);
            // The partial state is really there underneath: containment hides it, it does not
            // repair it.
            let raw_a = store.db.get(b"a").unwrap().unwrap();
            assert_eq!(
                raw_a.as_ref(),
                if applied >= 1 {
                    b"new".as_slice()
                } else {
                    b"old"
                }
            );
        }
    }

    #[test]
    fn every_shared_handle_sees_the_fault_and_no_handle_can_clear_it() {
        let store = SledStore::temporary().unwrap();
        let shared = store.share();
        let in_domain = SledStore::from_db_in(store.db.clone(), store.fault_domain());
        failing_commit(&store, &[b"a"], 1);
        every_operation_is_refused(&shared);
        every_operation_is_refused(&in_domain);
        // Sharing again after the fault does not start a clean domain.
        every_operation_is_refused(&shared.share());
        // `from_db` without the owner's domain is a separate domain: the documented limit.
        let independent = SledStore::from_db(store.db.clone());
        assert!(independent.get(b"a").is_ok());
    }

    #[test]
    fn only_a_commit_time_storage_error_is_an_unknown_transaction_outcome() {
        use crate::transaction_outcome_is_unknown as unknown;
        assert!(unknown(&storage_error()));
        assert!(!unknown(
            &StoreTransactionError::Aborted {
                reason: "closure".into()
            }
            .into()
        ));
        assert!(!unknown(&anyhow::anyhow!("anything else")));
        assert!(!unknown(&CompareAndWriteInvalid::NoWrites.into()));
    }

    #[test]
    fn the_first_fault_is_kept() {
        let store = SledStore::temporary().unwrap();
        failing_commit(&store, &[b"a"], 0);
        let first = store.fault().unwrap();
        // A further failure through the domain cannot replace the record.
        let _ = store.fault.fail_async("flush", anyhow::anyhow!("later"));
        assert_eq!(store.fault().unwrap(), first);
    }

    #[test]
    fn single_key_write_and_flush_failures_fault_the_store() {
        for op in ["put", "flush"] {
            let store = SledStore::temporary().unwrap();
            let e = store
                .fault
                .write_single(op, || Err::<(), _>(anyhow::anyhow!("injected io error")))
                .unwrap_err();
            assert_eq!(
                e.downcast_ref::<StoreOutcomeUnknown>().unwrap().operation,
                op
            );
            every_operation_is_refused(&store);
        }
    }

    #[test]
    fn refusals_conflicts_aborts_and_read_errors_do_not_fault() {
        let store = SledStore::temporary().unwrap();
        // Validation refusal.
        let e = store.compare_and_write(&[], &[]).unwrap_err();
        assert!(e.downcast_ref::<CompareAndWriteInvalid>().is_some());
        // Comparison conflict.
        store.put(b"k", b"v").unwrap();
        let r = store
            .compare_and_write(
                &[Expectation {
                    key: b"k",
                    value: None,
                }],
                &[Mutation {
                    key: b"k",
                    value: Some(b"w"),
                }],
            )
            .unwrap();
        assert!(matches!(r, CompareAndWrite::Conflict { .. }));
        // A closure's own abort: sled discards the staged write.
        let e = store
            .transaction(|tx| -> anyhow::Result<()> {
                tx.set(b"staged", b"x")?;
                anyhow::bail!("caller aborts")
            })
            .unwrap_err();
        assert!(matches!(
            e.downcast_ref::<StoreTransactionError>(),
            Some(StoreTransactionError::Aborted { .. })
        ));
        assert_eq!(store.get(b"staged").unwrap(), None);
        // A read error is returned as is.
        let e = store
            .fault
            .read("get", || Err::<(), _>(anyhow::anyhow!("read failed")))
            .unwrap_err();
        assert!(e.downcast_ref::<StoreFaulted>().is_none());
        assert!(store.fault().is_none());
        assert_eq!(store.get(b"k").unwrap(), Some(b"v".to_vec()));
    }

    #[test]
    fn no_reader_observes_a_failing_multi_key_mutation_part_way() {
        // A writer applies one of two keys, pauses inside the exclusive gate, then fails. Readers
        // running throughout must either see the old value (before the writer entered) or be
        // refused (after it published the fault), never the half-applied new value.
        let store = Arc::new(SledStore::temporary().unwrap());
        store.put(b"a", b"old").unwrap();
        let entered = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (store, done) = (store.clone(), done.clone());
                std::thread::spawn(move || {
                    let mut saw_new = false;
                    while !done.load(Ordering::SeqCst) {
                        if let Ok(Some(v)) = store.get(b"a") {
                            saw_new |= v == b"new";
                        }
                    }
                    // After the writer returned, every read is refused.
                    assert!(store
                        .get(b"a")
                        .unwrap_err()
                        .downcast_ref::<StoreFaulted>()
                        .is_some());
                    saw_new
                })
            })
            .collect();
        let writer = {
            let (store, entered) = (store.clone(), entered.clone());
            std::thread::spawn(move || {
                store.fault.write_exclusive(
                    "transaction",
                    || {
                        entered.store(true, Ordering::SeqCst);
                        store.db.insert(b"a", b"new".as_slice())?;
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        Err::<(), _>(storage_error())
                    },
                    is_storage,
                )
            })
        };
        assert!(writer.join().unwrap().is_err());
        assert!(entered.load(Ordering::SeqCst));
        done.store(true, Ordering::SeqCst);
        for r in readers {
            assert!(!r.join().unwrap(), "a reader saw the half-applied value");
        }
    }

    #[test]
    fn concurrent_writers_after_a_fault_are_all_refused() {
        let store = Arc::new(SledStore::temporary().unwrap());
        failing_commit(&store, &[b"a"], 1);
        let writers: Vec<_> = (0..8)
            .map(|i| {
                let store = store.clone();
                std::thread::spawn(move || store.put(format!("k{i}").as_bytes(), b"v"))
            })
            .collect();
        for w in writers {
            assert!(w
                .join()
                .unwrap()
                .unwrap_err()
                .downcast_ref::<StoreFaulted>()
                .is_some());
        }
        for i in 0..8 {
            assert!(
                store.db.get(format!("k{i}")).unwrap().is_none(),
                "no write landed"
            );
        }
    }

    #[test]
    fn a_panic_inside_a_write_fails_the_domain_closed_a_panicking_read_does_not() {
        // A panicking read changes nothing: no fault.
        let store = Arc::new(SledStore::temporary().unwrap());
        let s2 = store.clone();
        let _ = std::thread::spawn(move || {
            let _ = s2
                .fault
                .read("get", || -> anyhow::Result<()> { panic!("boom") });
        })
        .join();
        assert!(store.fault().is_none());
        assert!(store.get(b"a").is_ok());

        // A panicking write (shared guard: no lock poisoning) must still fault.
        let s3 = store.clone();
        let _ = std::thread::spawn(move || {
            let _ = s3.fault.write_single("put", || -> anyhow::Result<()> {
                s3.db.insert(b"a", b"half".as_slice())?;
                panic!("boom mid-write")
            });
        })
        .join();
        assert_eq!(store.fault().unwrap().operation, "put");
        every_operation_is_refused(&store);

        // A panicking multi-key mutation faults too (and poisons the gate).
        let store = Arc::new(SledStore::temporary().unwrap());
        let s4 = store.clone();
        let _ = std::thread::spawn(move || {
            let _ = s4.fault.write_exclusive(
                "transaction",
                || -> anyhow::Result<()> { panic!("boom") },
                |_| true,
            );
        })
        .join();
        assert_eq!(store.fault().unwrap().operation, "transaction");
        every_operation_is_refused(&store);
    }

    #[test]
    fn reopening_starts_a_new_domain_and_shows_what_was_applied() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = SledStore::open(dir.path()).unwrap();
            store.put(b"a", b"old").unwrap();
            failing_commit(&store, &[b"a", b"b"], 1);
            store.db.flush().unwrap(); // the partial write reaches disk (simulated)
            assert!(store.fault().is_some());
        }
        let store = SledStore::open(dir.path()).unwrap();
        assert!(store.fault().is_none(), "a new owner starts a new domain");
        // Reopening does not repair: the caller reconciles what it finds.
        assert_eq!(store.get(b"a").unwrap(), Some(b"new".to_vec()));
        assert_eq!(store.get(b"b").unwrap(), None);
    }
}
