//! Fork resolution must not erase journal history (icn#2880 / #2881).
//!
//! Detecting a fork is observation. `Ledger::detect_and_resolve_forks` used to
//! also quarantine the losing entries by deleting their journal and
//! timestamp-index rows — dormant only because loaded entries had no id.
//! These tests pin that a fork that would require deleting an entry is
//! refused, leaving every row, balance and piece of fork evidence intact,
//! while detection stays fully available.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::{Did, IdentityBundle};
use icn_ledger::{entry::JournalEntryBuilder, types::ContentHash, Ledger};
use icn_store::{SledStore, Store};
use std::collections::BTreeSet;
use std::sync::Arc;

const JOURNAL_TS_PREFIX: &[u8] = b"ledger:journal_ts:";

fn build(
    author: &Did,
    other: &Did,
    amount: i64,
    parent: Option<&ContentHash>,
) -> (icn_ledger::types::JournalEntry, ContentHash) {
    let mut b = JournalEntryBuilder::new(author.clone())
        .debit(author.clone(), "ICN".to_string(), amount)
        .credit(other.clone(), "ICN".to_string(), amount)
        .with_system_provenance("fork-quarantine-fail-closed");
    if let Some(p) = parent {
        b = b.add_parent(p.clone());
    }
    let e = b.build().unwrap();
    let id = e.id.clone().unwrap();
    (e, id)
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    ids: BTreeSet<String>,
    journal_count: usize,
    ts_rows: usize,
    balances: (i64, i64),
    forks: Vec<(String, BTreeSet<String>)>,
    quarantined: usize,
}

fn snap(ledger: &Ledger, store: &SledStore, a: &Did, b: &Did) -> Snapshot {
    let mut forks: Vec<_> = ledger
        .detect_forks()
        .into_iter()
        .map(|(p, kids)| (p.to_hex(), kids.iter().map(|k| k.to_hex()).collect()))
        .collect();
    forks.sort();
    Snapshot {
        ids: ledger
            .get_all_entries()
            .unwrap()
            .iter()
            .map(|e| e.id.as_ref().unwrap().to_hex())
            .collect(),
        journal_count: ledger.count_entries().unwrap(),
        ts_rows: store.scan_count(JOURNAL_TS_PREFIX).unwrap(),
        balances: (ledger.get_balance(a, "ICN"), ledger.get_balance(b, "ICN")),
        forks,
        quarantined: ledger.quarantine().count().unwrap(),
    }
}

async fn forked() -> (Arc<SledStore>, tempfile::TempDir, Ledger, Did, Did) {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(tmp.path()).unwrap());
    let mut ledger = Ledger::new(store.clone()).unwrap();
    let a = IdentityBundle::generate().unwrap().did().clone();
    let b = IdentityBundle::generate().unwrap().did().clone();
    let (root, root_id) = build(&a, &b, 1, None);
    let (c1, _) = build(&a, &b, 2, Some(&root_id));
    let (c2, _) = build(&b, &a, 3, Some(&root_id));
    for e in [root, c1, c2] {
        ledger.append_entry(e).await.unwrap();
    }
    assert!(ledger.has_fork(&root_id), "fixture must contain a fork");
    (store, tmp, ledger, a, b)
}

/// Resolution that would delete a losing entry is refused and changes nothing.
#[tokio::test]
async fn destructive_fork_resolution_is_refused_and_preserves_everything() {
    let (store, _t, mut ledger, a, b) = forked().await;
    let before = snap(&ledger, &store, &a, &b);
    assert_eq!(before.journal_count, 3);
    assert_eq!(before.forks.len(), 1);

    let res = ledger.detect_and_resolve_forks();
    assert!(
        res.is_err(),
        "a resolution requiring deletion must not report success: {res:?}"
    );

    assert_eq!(snap(&ledger, &store, &a, &b), before, "state changed");
    ledger.verify_integrity().unwrap();
}

/// The refusal survives a reopen: rows, balances and the fork are still there.
#[tokio::test]
async fn refused_resolution_leaves_fork_evidence_across_restart() {
    let (store, _t, mut ledger, a, b) = forked().await;
    let before = snap(&ledger, &store, &a, &b);
    let _ = ledger.detect_and_resolve_forks();
    drop(ledger);

    let reopened = Ledger::new(store.clone()).unwrap();
    assert_eq!(snap(&reopened, &store, &a, &b), before);
}

/// Detection stays available while mutation is refused, and repeated
/// attempts keep refusing.
#[tokio::test]
async fn detection_remains_available_and_refusal_is_stable() {
    let (store, _t, mut ledger, a, b) = forked().await;
    let before = snap(&ledger, &store, &a, &b);
    for _ in 0..2 {
        assert!(ledger.detect_and_resolve_forks().is_err());
        assert_eq!(ledger.detect_forks().len(), 1);
    }
    assert_eq!(snap(&ledger, &store, &a, &b), before);
}

/// Harmless behaviour is preserved: with no fork there is nothing to refuse.
#[tokio::test]
async fn no_fork_resolves_to_empty_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(tmp.path()).unwrap());
    let mut ledger = Ledger::new(store).unwrap();
    let a = IdentityBundle::generate().unwrap().did().clone();
    let b = IdentityBundle::generate().unwrap().did().clone();
    let (root, root_id) = build(&a, &b, 1, None);
    let (child, _) = build(&a, &b, 2, Some(&root_id));
    ledger.append_entry(root).await.unwrap();
    ledger.append_entry(child).await.unwrap();
    assert!(ledger.detect_and_resolve_forks().unwrap().is_empty());
    assert_eq!(ledger.count_entries().unwrap(), 2);
}
