//! Ledger service persistence proofs — Layers 3 and 4.
//!
//! ## Layer 3 — same-runtime drop-and-reopen
//!
//! State written through `LedgerServiceImpl::submit_treasury_entry()` — the
//! canonical production service layer that wraps `Arc<RwLock<Ledger>>` — survives
//! a same-runtime drop-and-reopen boundary.
//!
//! No background Tokio task, no JoinHandle needed. Dropping ALL clones of
//! `Arc<RwLock<Ledger>>` releases the sled file lock, and a fresh `Ledger::new`
//! on the same path reads back the written entry from sled.
//!
//! ## Layer 4 — cross-process restart
//!
//! State written in one OS process is readable in a fresh OS process. Uses the
//! `ledger_restart_helper` binary (same package) via `std::process::Command`.
//! The write subprocess writes through `LedgerServiceImpl`, prints the entry hash
//! to stdout, and exits. The read subprocess opens fresh sled, reads back the entry
//! by hash through `Ledger::new`, and asserts exact field values.
//!
//! ## What is NOT proven
//!
//! - Receipt-index idempotency across restart: the receipt index store is separate
//!   from the main ledger store. See `test_submit_treasury_entry_idempotency_survives_restart`
//!   in `ledger_service.rs` for that proof.
//! - Gossip sync or trust-gated entry acceptance.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_core::services::LedgerServiceImpl;
use icn_identity::KeyPair;
use icn_kernel_api::{
    services::{LedgerService, TreasuryEntryRequest, TreasuryOperationType},
    AllowAllOracle,
};
use icn_ledger::{entry::JournalEntryBuilder, types::ProvenanceRef, ContentHash, Ledger};
use icn_store::SledStore;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Layer 3 — LedgerServiceImpl same-runtime persistence proof.
///
/// Proves that a journal entry written through `LedgerServiceImpl` persists to
/// sled. After dropping all runtime state, a fresh `Ledger::new` on the same
/// path reads back the entry by hash with exact field values.
///
/// Requires multi-thread runtime: `submit_treasury_entry` uses
/// `tokio::task::block_in_place` internally.
#[tokio::test(flavor = "multi_thread")]
async fn test_ledger_service_entry_survives_drop_and_reopen() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_path_buf();

    let treasury_kp = KeyPair::generate().expect("treasury KeyPair");
    let treasury_did = treasury_kp.did().clone();

    let recipient_kp = KeyPair::generate().expect("recipient KeyPair");
    let recipient_did = recipient_kp.did().clone();

    let entry_hash_hex: String;

    // ── Phase 1: write through LedgerServiceImpl ─────────────────────────────
    {
        let store = Arc::new(SledStore::open(&db_path).expect("Phase1: SledStore::open"));
        let ledger = Arc::new(RwLock::new(
            Ledger::new(store).expect("Phase1: Ledger::new"),
        ));

        // LedgerServiceImpl::new (no receipt index) — exercises the fallback
        // compatibility path via find_existing_entry_for_receipt (linear scan).
        let service = LedgerServiceImpl::new(
            ledger,
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
        );

        let request = TreasuryEntryRequest {
            treasury_id: treasury_did.to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 200,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "layer-3-persistence-proof".to_string(),
            expected_nonce: Some(0),
            decision_receipt_id: "proof:receipt:layer3:001".to_string(),
            decision_hash: "proof-decision-hash-layer3".to_string(),
        };

        let result = service
            .submit_treasury_entry(request)
            .expect("Phase1: submit_treasury_entry");

        entry_hash_hex = result.entry_hash;

        // `service`, `ledger` Arc, and `store` Arc all drop here.
        // Sled file lock released when store Arc ref count reaches zero.
    }

    // ── Phase 2: reopen and read via fresh Ledger ────────────────────────────
    //
    // SledStore::open succeeds iff the file lock was released by the drop above.
    let store2 = Arc::new(
        SledStore::open(&db_path).expect("Phase2: SledStore::open — sled lock must be released"),
    );
    let mut ledger2 = Ledger::new(store2).expect("Phase2: Ledger::new");

    // ── Exact assertions ──────────────────────────────────────────────────────

    // 1. Entry count: exactly 1 entry survived to sled.
    assert_eq!(
        ledger2.count_entries().expect("Phase2: count_entries"),
        1,
        "exactly one entry must survive sled round-trip through LedgerServiceImpl"
    );

    // 2. Entry content: parse hash from service result, read back exact fields.
    let hash_bytes: [u8; 32] = hex::decode(&entry_hash_hex)
        .expect("entry_hash_hex must be valid hex")
        .try_into()
        .expect("entry_hash_hex must be 32 bytes");
    let content_hash = ContentHash::from_bytes(hash_bytes);

    let entry = ledger2
        .get_entry(&content_hash)
        .expect("Phase2: get_entry")
        .expect("entry must be present after reopen");

    assert_eq!(
        entry.author, treasury_did,
        "author (treasury) DID must survive sled round-trip"
    );
    assert_eq!(
        entry.accounts.len(),
        2,
        "both account deltas (debit treasury + credit recipient) must survive"
    );

    // 3. Provenance round-trip: the service writes GovernanceProvenance.
    assert!(
        matches!(
            &entry.provenance,
            ProvenanceRef::Governance { receipt_id, .. } if receipt_id == "proof:receipt:layer3:001"
        ),
        "provenance receipt_id must survive sled round-trip, got: {:?}",
        entry.provenance
    );

    // ── Phase 3: reopened ledger accepts a second independent write ───────────
    //
    // Proves sled is writable, not opened in read-only mode after reopen.
    let bundle3 = KeyPair::generate().expect("Phase3 KeyPair");
    let did3 = bundle3.did().clone();

    let entry3 = JournalEntryBuilder::new(treasury_did.clone())
        .debit(treasury_did.clone(), "HOURS".to_string(), 50)
        .credit(did3, "HOURS".to_string(), 50)
        .with_system_provenance("post-reopen-write-proof")
        .build()
        .expect("Phase3: build");

    let _hash3 = ledger2
        .append_entry(entry3)
        .await
        .expect("Phase3: append_entry — reopened ledger must be writable");

    assert_eq!(
        ledger2.count_entries().expect("Phase3: count_entries"),
        2,
        "entry count must be 2 after second write"
    );
}

/// Layer 4 — cross-process restart proof.
///
/// Spawns two OS processes via `ledger_restart_helper`:
/// - Write process: opens sled, writes one `JournalEntry` through
///   `LedgerServiceImpl`, prints the entry hash (hex) to stdout, exits.
/// - Read process: opens the same sled path fresh, reads back the entry by
///   hash through `Ledger::new`, asserts count == 1, account deltas == 2,
///   and exact provenance receipt_id. Exits 0 on success.
///
/// This is a synchronous test — no Tokio runtime in the test thread. The
/// subprocesses each manage their own runtimes independently.
///
/// `CARGO_BIN_EXE_ledger_restart_helper` is set by Cargo when building test
/// binaries in the same package, giving the absolute path to the compiled
/// helper binary.
#[test]
fn test_ledger_service_entry_survives_cross_process_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().to_str().expect("db_path to str");

    let helper = env!("CARGO_BIN_EXE_ledger_restart_helper");

    // ── Phase 1: write subprocess ─────────────────────────────────────────────
    let write_out = std::process::Command::new(helper)
        .args(["write", db_path])
        .output()
        .expect("failed to spawn write subprocess");

    assert!(
        write_out.status.success(),
        "write subprocess failed (exit {:?}):\nstdout: {}\nstderr: {}",
        write_out.status.code(),
        String::from_utf8_lossy(&write_out.stdout),
        String::from_utf8_lossy(&write_out.stderr),
    );

    let entry_hash_hex = String::from_utf8(write_out.stdout)
        .expect("write stdout must be valid UTF-8")
        .trim()
        .to_string();

    // Validate the hash looks like a 32-byte hex string before handing it to
    // the read subprocess. Catches accidental log output in stdout early.
    assert_eq!(
        entry_hash_hex.len(),
        64,
        "entry_hash must be 64 hex chars (32 bytes), got: {entry_hash_hex:?}"
    );
    assert!(
        entry_hash_hex.chars().all(|c| c.is_ascii_hexdigit()),
        "entry_hash must be lowercase hex, got: {entry_hash_hex:?}"
    );

    // ── Phase 2: read subprocess (fresh OS process, no shared memory) ─────────
    let read_out = std::process::Command::new(helper)
        .args(["read", db_path, &entry_hash_hex])
        .output()
        .expect("failed to spawn read subprocess");

    assert!(
        read_out.status.success(),
        "read subprocess failed (exit {:?}):\nstdout: {}\nstderr: {}",
        read_out.status.code(),
        String::from_utf8_lossy(&read_out.stdout),
        String::from_utf8_lossy(&read_out.stderr),
    );
}
