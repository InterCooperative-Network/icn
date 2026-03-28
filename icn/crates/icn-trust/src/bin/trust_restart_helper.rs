//! Cross-process trust persistence proof helper binary.
//!
//! Used exclusively by the Layer 4 integration test in
//! `crates/icn-trust/tests/trust_persistence.rs` to prove that trust graph
//! state written through `TrustGraphFacade` survives a true OS process
//! boundary — not just a same-runtime drop-and-reopen.
//!
//! ## Usage
//!
//! ```text
//! trust_restart_helper write <sled_path>
//!     Opens a real SledStore at <sled_path>, constructs TrustGraphFacade,
//!     writes one Social TrustEdge, prints "src_did,tgt_did,score" to
//!     stdout, exits 0. Exits 1 on error.
//!
//! trust_restart_helper read <sled_path> <src_did> <tgt_did> <score>
//!     Opens fresh SledStore, constructs fresh TrustGraphFacade, reads
//!     the edge by (src_did, tgt_did), asserts exact DID and score values.
//!     Exits 0 on success, 1 on failure.
//! ```
//!
//! ## Architecture note
//!
//! Trust persistence is sled-backed. Write path:
//!   `TrustGraphFacade::add_edge()` → `store.put("trust/social/edges/{src}:{tgt}", json)`
//!
//! No background scheduler. No `block_in_place`. `new_current_thread()` runtime
//! is sufficient (unlike ledger's `new_multi_thread()`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use icn_identity::KeyPair;
use icn_store::SledStore;
use icn_trust::{TrustEdge, TrustGraphFacade, TrustScore};
use std::{path::PathBuf, process};

const PROOF_SCORE: f64 = 0.72;

fn main() {
    // Current-thread runtime is sufficient: no block_in_place in trust path.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let args: Vec<String> = std::env::args().collect();
    let exit_code = match args.get(1).map(String::as_str) {
        Some("write") => {
            let sled_path = PathBuf::from(args.get(2).expect("write: missing sled_path"));
            run_write(sled_path)
        }
        Some("read") => {
            let sled_path = PathBuf::from(args.get(2).expect("read: missing sled_path"));
            let src_did_str = args.get(3).expect("read: missing src_did").clone();
            let tgt_did_str = args.get(4).expect("read: missing tgt_did").clone();
            let score_str = args.get(5).expect("read: missing score").clone();
            run_read(sled_path, src_did_str, tgt_did_str, score_str)
        }
        _ => {
            eprintln!(
                "usage: trust_restart_helper <write|read> <sled_path> [src_did tgt_did score]"
            );
            1
        }
    };

    drop(rt);
    process::exit(exit_code);
}

/// Write phase: build a Social trust edge through TrustGraphFacade, persist
/// to sled, print "src_did,tgt_did,score" to stdout.
fn run_write(sled_path: PathBuf) -> i32 {
    let src_kp = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: src KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let src_did = src_kp.did().clone();

    let tgt_kp = match KeyPair::generate() {
        Ok(kp) => kp,
        Err(e) => {
            eprintln!("write: tgt KeyPair::generate failed: {e}");
            return 1;
        }
    };
    let tgt_did = tgt_kp.did().clone();

    let store = match SledStore::open(&sled_path) {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => {
            eprintln!("write: SledStore::open failed: {e}");
            return 1;
        }
    };

    let mut facade = TrustGraphFacade::new(store, src_did.clone());
    let edge = TrustEdge::new(
        src_did.clone(),
        tgt_did.clone(),
        TrustScore::unchecked(PROOF_SCORE),
    );

    if let Err(e) = facade.add_edge(edge) {
        eprintln!("write: facade.add_edge failed: {e}");
        return 1;
    }

    // Print "src_did,tgt_did,score" — one line, no trailing whitespace.
    println!("{},{},{}", src_did, tgt_did, PROOF_SCORE);
    0
}

/// Read phase: open fresh SledStore, reconstruct TrustGraphFacade, assert
/// the edge matches the values printed by the write phase.
fn run_read(
    sled_path: PathBuf,
    src_did_str: String,
    tgt_did_str: String,
    score_str: String,
) -> i32 {
    use icn_identity::Did;

    let src_did: Did = match Did::from_str(&src_did_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read: invalid src_did {src_did_str:?}: {e}");
            return 1;
        }
    };
    let tgt_did: Did = match Did::from_str(&tgt_did_str) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("read: invalid tgt_did {tgt_did_str:?}: {e}");
            return 1;
        }
    };
    let expected_score: f64 = match score_str.parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("read: invalid score {score_str:?}: {e}");
            return 1;
        }
    };

    let store = match SledStore::open(&sled_path) {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => {
            eprintln!("read: SledStore::open failed: {e}");
            return 1;
        }
    };

    let facade = TrustGraphFacade::new(store, src_did.clone());

    let edge = match facade.get_edge(&src_did, &tgt_did) {
        Ok(Some(e)) => e,
        Ok(None) => {
            eprintln!("read: edge not found after cross-process restart");
            return 1;
        }
        Err(e) => {
            eprintln!("read: get_edge failed: {e}");
            return 1;
        }
    };

    // Assert 1: source DID survives process boundary.
    if edge.source != src_did {
        eprintln!(
            "read: source DID mismatch: expected {src_did_str:?}, got {:?}",
            edge.source
        );
        return 1;
    }

    // Assert 2: target DID survives process boundary.
    if edge.target != tgt_did {
        eprintln!(
            "read: target DID mismatch: expected {tgt_did_str:?}, got {:?}",
            edge.target
        );
        return 1;
    }

    // Assert 3: score survives process boundary.
    if (edge.score.value() - expected_score).abs() >= 1e-9 {
        eprintln!(
            "read: score mismatch: expected {expected_score}, got {}",
            edge.score.value()
        );
        return 1;
    }

    0
}
