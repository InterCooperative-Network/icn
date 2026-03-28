//! Cross-process ledger persistence proof helper binary.
//!
//! Used exclusively by the Layer 4 integration test in
//! `crates/icn-core/tests/ledger_service_persistence.rs` to prove that ledger
//! state written through `LedgerServiceImpl` survives a true OS process
//! boundary — not just a same-runtime drop-and-reopen.
//!
//! ## Usage
//!
//! ```text
//! ledger_restart_helper write <sled_path>
//!     Opens sled at <sled_path>, constructs LedgerServiceImpl, writes one
//!     JournalEntry, prints the entry hash (64 hex chars) to stdout, exits 0.
//!     Exits 1 on any error, with a diagnostic message on stderr.
//!
//! ledger_restart_helper read <sled_path> <entry_hash_hex>
//!     Opens a fresh Ledger::new on the same sled path. Reads back the entry
//!     by hash, asserts count == 1, account deltas == 2, and exact provenance
//!     receipt_id. Exits 0 on success, 1 on failure.
//! ```
//!
//! ## Runtime note
//!
//! `LedgerServiceImpl::submit_treasury_entry` uses `tokio::task::block_in_place`
//! internally and therefore requires a **multi-thread** Tokio runtime. This
//! binary uses `new_multi_thread()`, unlike the governance restart helper which
//! can use `new_current_thread()`.
//!
//! ## Key difference from governance restart helper
//!
//! There is no background scheduler task in the ledger path. Dropping the
//! `Arc<RwLock<Ledger>>` and `Arc<SledStore>` is sufficient to release the sled
//! file lock. No `GovernanceHandle::shutdown()` equivalent is needed.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use icn_core::services::LedgerServiceImpl;
use icn_identity::KeyPair;
use icn_kernel_api::{
    services::{LedgerService, TreasuryEntryRequest, TreasuryOperationType},
    AllowAllOracle,
};
use icn_ledger::{types::ProvenanceRef, ContentHash, Ledger};
use icn_store::SledStore;
use std::{path::PathBuf, process, sync::Arc};
use tokio::sync::RwLock;

/// Provenance receipt ID written by the write phase and verified by the read phase.
/// Hardcoded in both modes so the read side can assert an exact value without
/// receiving it over the wire.
const RECEIPT_ID: &str = "proof:receipt:layer4:001";
const CURRENCY: &str = "HOURS";
const AMOUNT: i64 = 200;

fn main() {
    // Multi-thread runtime required: submit_treasury_entry uses block_in_place.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let args: Vec<String> = std::env::args().collect();
    let exit_code = match args.get(1).map(String::as_str) {
        Some("write") => {
            let sled_path = PathBuf::from(args.get(2).expect("write: missing sled_path"));
            rt.block_on(async { run_write(sled_path) })
        }
        Some("read") => {
            let sled_path = PathBuf::from(args.get(2).expect("read: missing sled_path"));
            let hash_hex = args.get(3).expect("read: missing entry_hash_hex").clone();
            rt.block_on(async { run_read(sled_path, hash_hex) })
        }
        _ => {
            eprintln!("usage: ledger_restart_helper <write|read> <sled_path> [entry_hash_hex]");
            1
        }
    };

    // Drop runtime before process::exit. This ensures all async cleanup runs
    // (including sled flush when the store Arc ref-count reaches zero).
    drop(rt);
    process::exit(exit_code);
}

/// Write phase: open sled, write one JournalEntry through LedgerServiceImpl,
/// print the entry hash to stdout.
fn run_write(sled_path: PathBuf) -> i32 {
    let store = Arc::new(match SledStore::open(&sled_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("write: SledStore::open failed: {e}");
            return 1;
        }
    });

    let ledger = Arc::new(RwLock::new(match Ledger::new(store) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("write: Ledger::new failed: {e}");
            return 1;
        }
    }));

    let treasury_kp = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: treasury KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let treasury_did = treasury_kp.did().clone();

    let recipient_kp = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: recipient KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let recipient_did = recipient_kp.did().clone();

    let service = LedgerServiceImpl::new(
        ledger,
        Arc::new(AllowAllOracle::wildcard()),
        treasury_did.clone(),
    );

    let request = TreasuryEntryRequest {
        treasury_id: treasury_did.to_string(),
        operation_type: TreasuryOperationType::Spend,
        amount: AMOUNT,
        currency: CURRENCY.to_string(),
        recipient: Some(recipient_did.to_string()),
        memo: "layer-4-cross-process-proof".to_string(),
        expected_nonce: Some(0),
        decision_receipt_id: RECEIPT_ID.to_string(),
        decision_hash: "proof-decision-hash-layer4".to_string(),
    };

    let result = match service.submit_treasury_entry(request) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("write: submit_treasury_entry failed: {e}");
            return 1;
        }
    };

    // service and ledger Arc drop at end of this scope; sled flushes on drop(rt).
    println!("{}", result.entry_hash);
    0
}

/// Read phase: open fresh sled, read back entry by hash, assert exact invariants.
fn run_read(sled_path: PathBuf, hash_hex: String) -> i32 {
    // Validate hash format before opening sled — catches stdout parse bugs early.
    let hash_bytes: [u8; 32] = match hex::decode(&hash_hex).ok().and_then(|b| b.try_into().ok()) {
        Some(b) => b,
        None => {
            eprintln!("read: entry_hash_hex must be 64 hex chars (32 bytes), got: {hash_hex:?}");
            return 1;
        }
    };
    let content_hash = ContentHash::from_bytes(hash_bytes);

    let store = Arc::new(match SledStore::open(&sled_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read: SledStore::open failed: {e}");
            return 1;
        }
    });

    let ledger = match Ledger::new(store) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("read: Ledger::new failed: {e}");
            return 1;
        }
    };

    // Assert 1: exactly one entry survived the process boundary.
    let count = match ledger.count_entries() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("read: count_entries failed: {e}");
            return 1;
        }
    };
    if count != 1 {
        eprintln!("read: expected 1 entry after cross-process restart, got {count}");
        return 1;
    }

    // Assert 2: entry is readable by the exact hash written in process A.
    let entry = match ledger.get_entry(&content_hash) {
        Ok(Some(e)) => e,
        Ok(None) => {
            eprintln!("read: entry {hash_hex:?} not found after cross-process restart");
            return 1;
        }
        Err(e) => {
            eprintln!("read: get_entry failed: {e}");
            return 1;
        }
    };

    // Assert 3: both account deltas (debit treasury + credit recipient) survived.
    if entry.accounts.len() != 2 {
        eprintln!(
            "read: expected 2 account deltas, got {}",
            entry.accounts.len()
        );
        return 1;
    }

    // Assert 4: provenance receipt_id survived exact round-trip.
    // The receipt_id is a hardcoded constant shared between write and read modes
    // so it can be verified without passing it between processes.
    match &entry.provenance {
        ProvenanceRef::Governance { receipt_id, .. } if receipt_id == RECEIPT_ID => {}
        other => {
            eprintln!(
                "read: expected Governance provenance with receipt_id={RECEIPT_ID:?}, got {other:?}"
            );
            return 1;
        }
    }

    0
}
