//! `Store::compare_and_write`: the generic, meaning-neutral multi-key conditional write.
//!
//! Exercised through `&dyn Store` (object safety) against the sled backend, and through a store
//! that does not override the method (the refusing default).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Barrier};

use anyhow::Result;
use icn_store::{
    CompareAndPut, CompareAndWrite, CompareAndWriteInvalid, CompareAndWriteUnsupported,
    ContentHash, Expectation, Mutation, ReplicaMetadata, SledStore, Store,
    MAX_COMPARE_AND_WRITE_ENTRIES,
};

fn exp<'a>(key: &'a [u8], value: Option<&'a [u8]>) -> Expectation<'a> {
    Expectation { key, value }
}

fn set<'a>(key: &'a [u8], value: &'a [u8]) -> Mutation<'a> {
    Mutation {
        key,
        value: Some(value),
    }
}

fn del(key: &[u8]) -> Mutation<'_> {
    Mutation { key, value: None }
}

fn sled() -> (tempfile::TempDir, SledStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = SledStore::open(dir.path()).unwrap();
    (dir, store)
}

#[test]
fn applies_every_write_when_every_expectation_holds() -> Result<()> {
    let (_dir, sled) = sled();
    let store: &dyn Store = &sled;
    store.put(b"a", b"1")?;
    store.put(b"gone", b"x")?;
    let outcome = store.compare_and_write(
        &[exp(b"a", Some(b"1")), exp(b"b", None)],
        &[set(b"a", b"2"), set(b"b", b"new"), del(b"gone")],
    )?;
    assert_eq!(outcome, CompareAndWrite::Applied);
    assert_eq!(store.get(b"a")?, Some(b"2".to_vec()));
    assert_eq!(store.get(b"b")?, Some(b"new".to_vec()));
    assert_eq!(store.get(b"gone")?, None);
    Ok(())
}

#[test]
fn any_failing_expectation_writes_nothing_and_reports_the_first_mismatch() -> Result<()> {
    let (_dir, sled) = sled();
    let store: &dyn Store = &sled;
    store.put(b"a", b"1")?;
    store.put(b"b", b"present")?;
    let before = store.scan(b"")?;
    for (expect, key, current) in [
        // Wrong bytes.
        (
            vec![exp(b"a", Some(b"9")), exp(b"b", Some(b"present"))],
            b"a".to_vec(),
            Some(b"1".to_vec()),
        ),
        // Absence expected, but present (a later expectation fails; earlier ones held).
        (
            vec![exp(b"a", Some(b"1")), exp(b"b", None)],
            b"b".to_vec(),
            Some(b"present".to_vec()),
        ),
        // Presence expected, but absent.
        (vec![exp(b"missing", Some(b"v"))], b"missing".to_vec(), None),
    ] {
        let outcome =
            store.compare_and_write(&expect, &[set(b"a", b"X"), set(b"c", b"X"), del(b"b")])?;
        assert_eq!(outcome, CompareAndWrite::Conflict { key, current });
        assert_eq!(
            store.scan(b"")?,
            before,
            "a conflict must leave every key unchanged"
        );
    }
    Ok(())
}

#[test]
fn absence_and_empty_value_are_distinct() -> Result<()> {
    let (_dir, sled) = sled();
    let store: &dyn Store = &sled;
    store.put(b"empty", b"")?;
    assert!(matches!(
        store.compare_and_write(&[exp(b"empty", None)], &[set(b"x", b"1")])?,
        CompareAndWrite::Conflict { .. }
    ));
    assert!(matches!(
        store.compare_and_write(&[exp(b"absent", Some(b""))], &[set(b"x", b"1")])?,
        CompareAndWrite::Conflict { .. }
    ));
    assert_eq!(
        store.compare_and_write(
            &[exp(b"empty", Some(b"")), exp(b"absent", None)],
            &[set(b"x", b"1")]
        )?,
        CompareAndWrite::Applied
    );
    Ok(())
}

#[test]
fn malformed_requests_are_refused_before_any_write() -> Result<()> {
    let (_dir, sled) = sled();
    let store: &dyn Store = &sled;
    let invalid = |e: anyhow::Error| *e.downcast_ref::<CompareAndWriteInvalid>().unwrap();
    assert_eq!(
        invalid(
            store
                .compare_and_write(&[exp(b"a", None)], &[])
                .unwrap_err()
        ),
        CompareAndWriteInvalid::NoWrites
    );
    assert_eq!(
        invalid(
            store
                .compare_and_write(&[exp(b"a", None), exp(b"a", None)], &[set(b"z", b"1")])
                .unwrap_err()
        ),
        CompareAndWriteInvalid::DuplicateExpectation
    );
    assert_eq!(
        invalid(
            store
                .compare_and_write(&[], &[set(b"z", b"1"), del(b"z")])
                .unwrap_err()
        ),
        CompareAndWriteInvalid::DuplicateMutation
    );
    let keys: Vec<Vec<u8>> = (0..=MAX_COMPARE_AND_WRITE_ENTRIES)
        .map(|i| (i as u32).to_be_bytes().to_vec())
        .collect();
    let too_many: Vec<Mutation<'_>> = keys.iter().map(|k| set(k, b"v")).collect();
    assert_eq!(
        invalid(store.compare_and_write(&[], &too_many).unwrap_err()),
        CompareAndWriteInvalid::TooManyEntries
    );
    let too_many_expect: Vec<Expectation<'_>> = keys.iter().map(|k| exp(k, None)).collect();
    assert_eq!(
        invalid(
            store
                .compare_and_write(&too_many_expect, &[set(b"z", b"1")])
                .unwrap_err()
        ),
        CompareAndWriteInvalid::TooManyEntries
    );
    // At the bound is accepted; an unconditional multi-key write (no expectations) is allowed.
    assert_eq!(
        store.compare_and_write(&[], &too_many[..MAX_COMPARE_AND_WRITE_ENTRIES])?,
        CompareAndWrite::Applied
    );
    // None of the refused requests wrote anything.
    assert_eq!(store.get(b"z")?, None);
    Ok(())
}

/// A backend that does not override `compare_and_write` refuses it; it never emulates it with
/// sequential reads and writes.
struct NoMultiKey(SledStore);

impl Store for NoMultiKey {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.0.get(key)
    }
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.0.put(key, value)
    }
    fn delete(&self, key: &[u8]) -> Result<()> {
        self.0.delete(key)
    }
    fn delete_atomic(&self, keys: &[Vec<u8>]) -> Result<()> {
        self.0.delete_atomic(keys)
    }
    fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        self.0.scan(prefix)
    }
    fn get_replica_metadata(&self, h: &ContentHash) -> Result<Option<ReplicaMetadata>> {
        self.0.get_replica_metadata(h)
    }
    fn put_replica_metadata(&self, m: &ReplicaMetadata) -> Result<()> {
        self.0.put_replica_metadata(m)
    }
    fn list_replica_hashes(&self) -> Result<Vec<ContentHash>> {
        self.0.list_replica_hashes()
    }
}

#[test]
fn an_unsupporting_backend_refuses_and_writes_nothing() -> Result<()> {
    let (_dir, sled) = sled();
    let store: Arc<dyn Store> = Arc::new(NoMultiKey(sled));
    let err = store
        .compare_and_write(&[exp(b"a", None)], &[set(b"a", b"1")])
        .unwrap_err();
    assert!(err.downcast_ref::<CompareAndWriteUnsupported>().is_some());
    assert_eq!(store.get(b"a")?, None);
    Ok(())
}

#[test]
fn an_applied_write_survives_flush_and_reopen() -> Result<()> {
    let dir = tempfile::tempdir()?;
    {
        let store = SledStore::open(dir.path())?;
        let dyn_store: &dyn Store = &store;
        dyn_store.put(b"a", b"1")?;
        assert_eq!(
            dyn_store.compare_and_write(
                &[exp(b"a", Some(b"1"))],
                &[set(b"a", b"2"), set(b"b", b"2")]
            )?,
            CompareAndWrite::Applied
        );
        dyn_store.flush()?;
    }
    let store = SledStore::open(dir.path())?;
    assert_eq!(Store::get(&store, b"a")?, Some(b"2".to_vec()));
    assert_eq!(Store::get(&store, b"b")?, Some(b"2".to_vec()));
    Ok(())
}

/// Interoperation with single-key writers: a compare_and_put or put that commits first makes the
/// compare-and-write conflict; one that comes after sees its result.
#[test]
fn single_key_writers_and_compare_and_write_observe_each_other() -> Result<()> {
    let (_dir, sled) = sled();
    let store: &dyn Store = &sled;
    store.put(b"a", b"1")?;
    assert_eq!(
        store.compare_and_put(b"a", Some(b"1"), b"cas")?,
        CompareAndPut::Applied
    );
    assert_eq!(
        store.compare_and_write(&[exp(b"a", Some(b"1"))], &[set(b"a", b"multi")])?,
        CompareAndWrite::Conflict {
            key: b"a".to_vec(),
            current: Some(b"cas".to_vec())
        }
    );
    assert_eq!(
        store.compare_and_write(&[exp(b"a", Some(b"cas"))], &[set(b"a", b"multi")])?,
        CompareAndWrite::Applied
    );
    assert_eq!(
        store.compare_and_put(b"a", Some(b"cas"), b"late")?,
        CompareAndPut::Conflict {
            current: Some(b"multi".to_vec())
        }
    );
    Ok(())
}

/// Concurrent conditional writers on two keys that must move together: every successful
/// compare-and-write advances both counters from the values it read, so the invariant
/// `a == b` holds after every commit and no update is lost.
#[test]
fn concurrent_compare_and_writers_never_lose_or_split_an_update() -> Result<()> {
    let (_dir, sled) = sled();
    let store = Arc::new(sled);
    store.put(b"a", &0u64.to_be_bytes())?;
    store.put(b"b", &0u64.to_be_bytes())?;
    const THREADS: usize = 8;
    const INCREMENTS: u64 = 50;
    let barrier = Arc::new(Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                let mut done = 0;
                let mut conflicts = 0u64;
                while done < INCREMENTS {
                    let a = Store::get(&*store, b"a").unwrap().unwrap();
                    let b = Store::get(&*store, b"b").unwrap().unwrap();
                    let n = u64::from_be_bytes(a.clone().try_into().unwrap());
                    let next = (n + 1).to_be_bytes();
                    match Store::compare_and_write(
                        &*store,
                        &[exp(b"a", Some(&a)), exp(b"b", Some(&b))],
                        &[set(b"a", &next), set(b"b", &next)],
                    )
                    .unwrap()
                    {
                        CompareAndWrite::Applied => done += 1,
                        CompareAndWrite::Conflict { .. } => conflicts += 1,
                    }
                }
                conflicts
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let total = (THREADS as u64) * INCREMENTS;
    assert_eq!(
        Store::get(&*store, b"a")?,
        Some(total.to_be_bytes().to_vec())
    );
    assert_eq!(
        Store::get(&*store, b"b")?,
        Some(total.to_be_bytes().to_vec())
    );
    Ok(())
}

/// Concurrent single-key compare_and_put writers on one of the keys race compare-and-writers that
/// move both keys together. Every compare-and-write sets both keys to the same token; every
/// compare_and_put moves only `a`. Each successful writer is counted; the final state must
/// account for every success exactly (no lost update between a comparison and its write).
#[test]
fn compare_and_write_and_compare_and_put_interleave_without_lost_updates() -> Result<()> {
    let (_dir, sled) = sled();
    let store = Arc::new(sled);
    store.put(b"a", &0u64.to_be_bytes())?;
    store.put(b"b", &0u64.to_be_bytes())?;
    const PER: u64 = 200;
    let barrier = Arc::new(Barrier::new(2));
    let multi = {
        let (store, barrier) = (Arc::clone(&store), Arc::clone(&barrier));
        std::thread::spawn(move || {
            barrier.wait();
            let mut applied = 0u64;
            for _ in 0..PER {
                let a = Store::get(&*store, b"a").unwrap().unwrap();
                let b = Store::get(&*store, b"b").unwrap().unwrap();
                let next = (u64::from_be_bytes(a.clone().try_into().unwrap()) + 1).to_be_bytes();
                if Store::compare_and_write(
                    &*store,
                    &[exp(b"a", Some(&a)), exp(b"b", Some(&b))],
                    &[
                        set(b"a", &next),
                        set(
                            b"b",
                            &(u64::from_be_bytes(b.clone().try_into().unwrap()) + 1).to_be_bytes(),
                        ),
                    ],
                )
                .unwrap()
                    == CompareAndWrite::Applied
                {
                    applied += 1;
                }
            }
            applied
        })
    };
    let single = {
        let (store, barrier) = (Arc::clone(&store), Arc::clone(&barrier));
        std::thread::spawn(move || {
            barrier.wait();
            let mut applied = 0u64;
            for _ in 0..PER {
                let a = Store::get(&*store, b"a").unwrap().unwrap();
                let next = (u64::from_be_bytes(a.clone().try_into().unwrap()) + 1).to_be_bytes();
                if Store::compare_and_put(&*store, b"a", Some(&a), &next).unwrap()
                    == CompareAndPut::Applied
                {
                    applied += 1;
                }
            }
            applied
        })
    };
    let multi_applied = multi.join().unwrap();
    let single_applied = single.join().unwrap();
    let a = u64::from_be_bytes(Store::get(&*store, b"a")?.unwrap().try_into().unwrap());
    let b = u64::from_be_bytes(Store::get(&*store, b"b")?.unwrap().try_into().unwrap());
    // `a` advanced once per success of either writer; `b` once per compare-and-write success.
    assert_eq!(a, multi_applied + single_applied);
    assert_eq!(b, multi_applied);
    Ok(())
}
