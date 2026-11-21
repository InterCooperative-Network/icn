//! Journal identity restoration (icn#2880).
//!
//! `JournalEntry.id` is `#[serde(skip)]`; the identity of a persisted record
//! lives only in its journal storage key (`ledger:journal:<64 hex>`). These
//! tests pin that every journal reader restores the id from that key, that the
//! fork index survives a reopen, and that rollback never reports a success it
//! did not perform.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::IdentityBundle;
use icn_ledger::{
    entry::JournalEntryBuilder, hash::compute_entry_hash, types::ContentHash, Ledger,
};
use icn_store::{SledStore, Store};
use std::collections::BTreeSet;
use std::sync::Arc;

const JOURNAL_PREFIX: &str = "ledger:journal:";

fn open() -> (Arc<SledStore>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(SledStore::open(tmp.path()).unwrap());
    (store, tmp)
}

fn entry(
    amount: i64,
    parent: Option<ContentHash>,
) -> (icn_ledger::types::JournalEntry, ContentHash) {
    let a = IdentityBundle::generate().unwrap().did().clone();
    let b = IdentityBundle::generate().unwrap().did().clone();
    let mut builder = JournalEntryBuilder::new(a.clone())
        .debit(a, "ICN".to_string(), amount)
        .credit(b, "ICN".to_string(), amount)
        .with_system_provenance("journal-id-restore");
    if let Some(p) = parent {
        builder = builder.add_parent(p);
    }
    let e = builder.build().unwrap();
    let id = e.id.clone().unwrap();
    (e, id)
}

/// A — single-entry readback carries the key's identity.
#[tokio::test]
async fn a_get_entry_restores_id_from_key() {
    let (store, _t) = open();
    let mut ledger = Ledger::new(store).unwrap();
    let (e, id) = entry(10, None);
    let appended = ledger.append_entry(e).await.unwrap();
    assert_eq!(appended, id);
    let loaded = ledger.get_entry(&id).unwrap().unwrap();
    assert_eq!(loaded.id.as_ref(), Some(&id));
}

/// B — collection readback: every id equals its own storage key.
#[tokio::test]
async fn b_get_all_entries_restores_ids() {
    let (store, _t) = open();
    let mut ledger = Ledger::new(store).unwrap();
    let mut expected = BTreeSet::new();
    for n in 1..=4 {
        let (e, id) = entry(n, None);
        ledger.append_entry(e).await.unwrap();
        expected.insert(id.to_hex());
    }
    let got: Vec<_> = ledger.get_all_entries().unwrap();
    assert_eq!(got.len(), 4);
    let got_ids: BTreeSet<String> = got
        .iter()
        .map(|e| {
            e.id.as_ref()
                .expect("loaded entry must carry its id")
                .to_hex()
        })
        .collect();
    assert_eq!(got_ids, expected);
}

/// C + G — reopen preserves ids, and the restored id is the canonical hash of
/// the loaded content (key and content agree; hashing is unchanged).
#[tokio::test]
async fn c_reopen_preserves_ids_and_canonical_hash() {
    let (store, _t) = open();
    let mut ids = Vec::new();
    {
        let mut ledger = Ledger::new(store.clone()).unwrap();
        for n in 1..=3 {
            let (e, id) = entry(n, None);
            ledger.append_entry(e).await.unwrap();
            ids.push(id);
        }
    }
    let ledger = Ledger::new(store).unwrap();
    for id in &ids {
        let loaded = ledger.get_entry(id).unwrap().unwrap();
        assert_eq!(loaded.id.as_ref(), Some(id));
        assert_eq!(&compute_entry_hash(&loaded).unwrap(), id);
    }
}

/// D — a fork is reported identically before and after reopen; no fork appears
/// where none existed.
#[tokio::test]
async fn d_fork_survives_reopen_and_no_artificial_fork() {
    let (store, _t) = open();
    let (root, root_id) = entry(1, None);
    let (c1, c1_id) = entry(2, Some(root_id.clone()));
    let (c2, c2_id) = entry(3, Some(root_id.clone()));
    let (solo, solo_id) = entry(4, None);
    let (solo_child, _) = entry(5, Some(solo_id.clone()));

    let before;
    {
        let mut ledger = Ledger::new(store.clone()).unwrap();
        for e in [root, c1, c2, solo, solo_child] {
            ledger.append_entry(e).await.unwrap();
        }
        assert!(ledger.has_fork(&root_id));
        assert!(!ledger.has_fork(&solo_id));
        before = ledger.detect_forks();
    }
    assert_eq!(before.len(), 1);

    let ledger = Ledger::new(store).unwrap();
    assert!(ledger.has_fork(&root_id), "fork lost after reopen");
    assert!(!ledger.has_fork(&solo_id), "artificial fork after reopen");
    let after = ledger.detect_forks();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].0, root_id);
    let kids: BTreeSet<_> = after[0].1.iter().map(|h| h.to_hex()).collect();
    let want: BTreeSet<_> = [c1_id.to_hex(), c2_id.to_hex()].into();
    assert_eq!(kids, want);
}

/// E — a journal key that is not `<prefix><64 hex>` is an explicit error, never
/// an entry with `id == None`.
#[tokio::test]
async fn e_malformed_journal_key_is_explicit_error() {
    for bad in ["zz", "abcd", &"a".repeat(63), &"g".repeat(64)] {
        let (store, _t) = open();
        let mut ledger = Ledger::new(store.clone()).unwrap();
        let (e, id) = entry(7, None);
        ledger.append_entry(e).await.unwrap();
        let bytes = store
            .get(format!("{JOURNAL_PREFIX}{}", id.to_hex()).as_bytes())
            .unwrap()
            .unwrap();
        store
            .put(format!("{JOURNAL_PREFIX}{bad}").as_bytes(), &bytes)
            .unwrap();
        assert!(
            ledger.get_all_entries().is_err(),
            "malformed key {bad:?} must fail get_all_entries"
        );
        assert!(
            Ledger::new(store).is_err(),
            "malformed key {bad:?} must fail fork-index rebuild on open"
        );
    }
}

/// F — rollback must never report success it did not perform. With the id
/// restored, a successful rollback would delete journal rows; that is gated
/// (see ledger.rs), so the call must be an explicit refusal and must leave
/// the journal untouched.
#[tokio::test]
async fn f_rollback_never_false_succeeds() {
    let (store, _t) = open();
    let mut ledger = Ledger::new(store).unwrap();
    let mut ids = Vec::new();
    for n in 1..=3 {
        let parent = ids.last().cloned();
        let (e, id) = entry(n, parent);
        ledger.append_entry(e).await.unwrap();
        ids.push(id);
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }
    let before = ledger.count_entries().unwrap();
    let res = ledger.rollback_to_entry(&ids[0], "test", false).await;
    let after = ledger.count_entries().unwrap();
    match res {
        Ok(archived) => assert_eq!(
            before - after,
            archived.len(),
            "reported {} archived but journal shrank by {}",
            archived.len(),
            before - after
        ),
        Err(_) => assert_eq!(before, after, "refused rollback must not mutate"),
    }
    // Refusal contract chosen for this change: rollback is not enabled.
    assert!(
        ledger
            .rollback_to_entry(&ids[0], "test", false)
            .await
            .is_err(),
        "rollback_to_entry must fail closed until its contract is owned"
    );
    assert_eq!(ledger.count_entries().unwrap(), before);
}
