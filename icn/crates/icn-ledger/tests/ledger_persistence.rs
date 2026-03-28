//! Ledger sled persistence proof — Layer 1.
//!
//! ## What is proven
//!
//! State written through `Ledger::new(Arc<SledStore>)` → `append_entry()` survives
//! an in-process sled drop-and-reopen boundary.
//!
//! Concretely:
//! 1. Open a temp sled database.
//! 2. Create `Ledger::new(Arc::new(store))`.
//! 3. Build a minimal valid `JournalEntry` (double-entry balanced, system provenance).
//! 4. Call `ledger.append_entry(entry)`, capture the returned `ContentHash`.
//! 5. Drop the `Ledger` — in-memory `cached_balances` is gone.
//! 6. Create a fresh `Ledger::new(same_store_arc)` — zero in-memory state.
//! 7. Call `ledger2.get_entry(&hash)` — success proves the write landed in sled.
//! 8. Assert entry fields round-trip with exact values (DID, currency, amounts, provenance).
//! 9. Append a second entry through the reopened ledger — proves sled is writable,
//!    not just replayed read-only.
//!
//! ## What is NOT proven
//!
//! - Cross-process restart: requires subprocess execution (see governance Layer 4).
//! - Balance query correctness: `cached_balances` is rebuilt from entries on `Ledger::new`;
//!   that is correctness, not persistence. Separate concern.
//! - Gossip sync, fork detection, trust-gated entry acceptance.
//! - Actor-backed path: `Ledger` is a direct struct here; the `apps/ledger` actor
//!   layer is the next proof target (see ledger-proof-layers.md).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::IdentityBundle;
use icn_ledger::{
    entry::JournalEntryBuilder,
    types::{AccountDelta, ProvenanceRef},
    Ledger,
};
use icn_store::SledStore;
use std::sync::Arc;

/// Layer 1 — Ledger sled write proof.
///
/// Proves that a `JournalEntry` written through `Ledger::new` + `append_entry`
/// is readable from a fresh `Ledger::new` on the same store, with all fields
/// intact. Also proves the reopened store accepts a second write (writable,
/// not just WAL-replayed read-only).
#[tokio::test]
async fn test_ledger_entry_survives_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let bundle = IdentityBundle::generate().expect("IdentityBundle::generate");
    let author_did = bundle.did().clone();

    // Second DID for the credit side of the double-entry transfer.
    let bundle2 = IdentityBundle::generate().expect("IdentityBundle::generate (peer)");
    let peer_did = bundle2.did().clone();

    let content_hash;

    // ── Phase 1: write through Ledger ────────────────────────────────────────
    {
        let store = Arc::new(SledStore::open(&db_path).expect("Phase1: SledStore::open"));
        let mut ledger = Ledger::new(store).expect("Phase1: Ledger::new");

        let entry = JournalEntryBuilder::new(author_did.clone())
            // Debit author 100 ICN-credits, credit peer 100 ICN-credits (balanced).
            .debit(author_did.clone(), "ICN".to_string(), 100)
            .credit(peer_did.clone(), "ICN".to_string(), 100)
            .with_system_provenance("persistence-proof")
            .build()
            .expect("Phase1: JournalEntryBuilder::build");

        content_hash = ledger
            .append_entry(entry)
            .await
            .expect("Phase1: append_entry");

        // `ledger` and `store` Arc drop here — in-memory cache released.
    }

    // ── Phase 2: reopen and read via fresh Ledger ────────────────────────────
    //
    // SledStore::open succeeds iff the file lock was released by the drop above.
    let store2 = Arc::new(SledStore::open(&db_path).expect("Phase2: SledStore::open"));
    let mut ledger2 = Ledger::new(store2.clone()).expect("Phase2: Ledger::new");

    let entry2 = ledger2
        .get_entry(&content_hash)
        .expect("Phase2: get_entry")
        .expect("entry must be present after reopen");

    // ── Exact field assertions ────────────────────────────────────────────────

    assert_eq!(
        entry2.author, author_did,
        "author DID must survive sled round-trip"
    );

    // Verify both sides of the double-entry transfer survived exactly.
    assert_eq!(
        entry2.accounts.len(),
        2,
        "both account deltas must be present"
    );

    let debit_delta = entry2
        .accounts
        .iter()
        .find(|d| d.debit.is_some())
        .expect("debit delta must be present");
    assert_eq!(
        debit_delta,
        &AccountDelta::debit(author_did.clone(), "ICN".to_string(), 100)
    );

    let credit_delta = entry2
        .accounts
        .iter()
        .find(|d| d.credit.is_some())
        .expect("credit delta must be present");
    assert_eq!(
        credit_delta,
        &AccountDelta::credit(peer_did.clone(), "ICN".to_string(), 100)
    );

    // Verify provenance round-tripped (proves full body serialization, not just header).
    assert!(
        matches!(
            &entry2.provenance,
            ProvenanceRef::SystemGenerated { reason } if reason == "persistence-proof"
        ),
        "provenance must survive sled round-trip, got: {:?}",
        entry2.provenance
    );

    // Note: entry2.id is None after deserialization because JournalEntry.id has
    // #[serde(skip)] — it is stored only as the sled key, not in the JSON value.
    // A successful get_entry(hash) is the proof: it keyed by content_hash, so
    // returning Some(_) proves the write landed at exactly that key.

    // ── Phase 3: reopened store accepts a second write ────────────────────────
    //
    // Proves the store is writable (not opened in read-only/WAL-replay mode).
    let bundle3 = IdentityBundle::generate().expect("IdentityBundle::generate (phase3)");
    let did3 = bundle3.did().clone();

    let entry3 = JournalEntryBuilder::new(author_did.clone())
        .debit(author_did.clone(), "ICN".to_string(), 50)
        .credit(did3, "ICN".to_string(), 50)
        .with_system_provenance("post-reopen-write-proof")
        .build()
        .expect("Phase3: JournalEntryBuilder::build");

    let hash3 = ledger2
        .append_entry(entry3)
        .await
        .expect("Phase3: append_entry — reopened store must be writable");

    // Confirm the second entry is also readable.
    let retrieved3 = ledger2
        .get_entry(&hash3)
        .expect("Phase3: get_entry")
        .expect("Phase3 entry must be present");
    assert_eq!(
        retrieved3.author, author_did,
        "Phase3 entry author must match"
    );

    drop(ledger2);
    drop(store2);
}
