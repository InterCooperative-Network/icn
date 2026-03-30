//! Cross-process commons persistence proof helper binary.
//!
//! Used exclusively by the Layer 3 integration test in
//! `crates/icn-commons/tests/commons_persistence.rs` to prove that commons
//! state written through `CommonsHandle` survives a true OS process boundary.
//!
//! ## Usage
//!
//! ```text
//! commons_restart_helper write <sled_path>
//!     Opens a CommonsHandle at <sled_path>, creates a PersonhoodAnchor,
//!     flushes to sled, prints "anchor_id_hex,did_str" to stdout, exits 0.
//!
//! commons_restart_helper read <sled_path> <anchor_id_hex> <did_str>
//!     Opens fresh CommonsHandle, verifies the anchor exists by ID and by
//!     DID index lookup. Exits 0 on success, 1 on failure.
//! ```
//!
//! ## Architecture note
//!
//! CommonsHandle uses `tokio::sync::RwLock`. All operations are async.
//! A current-thread runtime is sufficient — no blocking I/O bypass needed.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use icn_commons::CommonsHandle;
use icn_identity::{Did, KeyPair};
use std::{path::PathBuf, process};

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let args: Vec<String> = std::env::args().collect();
    let exit_code = match args.get(1).map(String::as_str) {
        Some("write") => {
            let sled_path = PathBuf::from(args.get(2).expect("write: missing sled_path"));
            rt.block_on(run_write(sled_path))
        }
        Some("read") => {
            let sled_path = PathBuf::from(args.get(2).expect("read: missing sled_path"));
            let anchor_id = args.get(3).expect("read: missing anchor_id_hex").clone();
            let did_str = args.get(4).expect("read: missing did_str").clone();
            rt.block_on(run_read(sled_path, anchor_id, did_str))
        }
        _ => {
            eprintln!(
                "usage: commons_restart_helper <write|read> <sled_path> [anchor_id_hex did_str]"
            );
            1
        }
    };

    process::exit(exit_code);
}

/// Write phase: create a PersonhoodAnchor through CommonsHandle, flush to sled,
/// print "anchor_id_hex,did_str" to stdout.
async fn run_write(sled_path: PathBuf) -> i32 {
    let keypair = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let did = keypair.did().clone();

    let handle = match CommonsHandle::with_sled_path(&sled_path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("write: CommonsHandle::with_sled_path failed: {e}");
            return 1;
        }
    };

    let anchor = match handle.create_anchor_from_enrollment(&did, None).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("write: create_anchor_from_enrollment failed: {e}");
            return 1;
        }
    };

    if let Err(e) = handle.flush().await {
        eprintln!("write: flush failed: {e}");
        return 1;
    }

    let anchor_id_hex = hex::encode(anchor.id());
    println!("{},{}", anchor_id_hex, did);
    0
}

/// Read phase: open fresh CommonsHandle, verify anchor exists by ID and by
/// DID index.
async fn run_read(sled_path: PathBuf, anchor_id_hex: String, did_str: String) -> i32 {
    let did: Did = match did_str.parse() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read: invalid did_str {did_str:?}: {e}");
            return 1;
        }
    };

    let handle = match CommonsHandle::with_sled_path(&sled_path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("read: CommonsHandle::with_sled_path failed: {e}");
            return 1;
        }
    };

    // Assert 1: anchor present by ID.
    let anchor = match handle.get_anchor(&anchor_id_hex).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            eprintln!("read: anchor {anchor_id_hex} not found after cross-process restart");
            return 1;
        }
        Err(e) => {
            eprintln!("read: get_anchor failed: {e}");
            return 1;
        }
    };

    // Assert 2: anchor ID survives exact round-trip.
    let stored_id = hex::encode(anchor.id());
    if stored_id != anchor_id_hex {
        eprintln!("read: anchor ID mismatch: expected {anchor_id_hex}, got {stored_id}");
        return 1;
    }

    // Assert 3: DID index survives cross-process boundary.
    match handle.get_anchor_by_did(&did).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            eprintln!("read: DID index not found for {did_str} after cross-process restart");
            return 1;
        }
        Err(e) => {
            eprintln!("read: get_anchor_by_did failed: {e}");
            return 1;
        }
    }

    0
}
