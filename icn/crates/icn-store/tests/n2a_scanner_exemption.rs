//! The scanner keeps deliberate pre-gate access to state the gate refuses
//! (#2627 M4d).
//!
//! M4d closed the ungated operator paths in `icnctl`, and the obvious way to
//! over-close it would be to make *everything* that opens a sled database run
//! the gate first. `did-collision-scan` must not: a safety scanner able to read
//! unsafe state is the entire point of having one, and an operator handed a
//! refusal is told to run exactly this tool for the row-level report
//! (`n2a_startup_gate.rs`, `icn_ledger::principal_rows`).
//!
//! The exemption is narrow and stated rather than accidental. The scanner is
//! not skipping the check — it *is* the offline form of the same computation
//! (`audit_sled_store`), so it can never disagree with the gate about what is
//! safe. What M4d forbids is an ordinary operational mutation path acquiring
//! the same privilege by omission.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use icn_identity::Did;
use icn_store::{SledStore, Store};

/// Two accepted spellings of one principal.
fn alias_pair() -> (Did, Did) {
    let a = icn_identity::KeyPair::generate().unwrap().did().clone();
    let bytes = a.identifier_bytes().unwrap();
    let b: Did = format!("did:icn:f{}", hex::encode(bytes)).parse().unwrap();
    assert_eq!(a, b);
    assert_ne!(a.as_str(), b.as_str());
    (a, b)
}

/// An alias collision in the registered `trust/edges/` keyspace.
fn seed_refused_store(dir: &Path) {
    let (a, b) = alias_pair();
    let peer = icn_identity::KeyPair::generate().unwrap().did().clone();
    let store = SledStore::open(dir).unwrap();
    for src in [&a, &b] {
        store
            .put(
                format!("trust/edges/{}:{}", src.as_str(), peer.as_str()).as_bytes(),
                b"{}",
            )
            .unwrap();
    }
    store.db().flush().unwrap();
    drop(store);
}

#[test]
fn the_scanner_still_reports_over_a_store_the_gate_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let store_path = dir.path().join("store");
    seed_refused_store(&store_path);

    // The gate refuses this data directory.
    let gate = icn_store::n2a_startup_gate::enforce(dir.path(), std::time::SystemTime::now());
    assert!(
        gate.is_err(),
        "fixture must be a store the gate refuses, got {gate:?}"
    );

    // The scanner reads it anyway, and produces a report rather than a refusal
    // to look.
    let out = Command::new(env!("CARGO_BIN_EXE_did-collision-scan"))
        .arg(&store_path)
        .arg("--json")
        .output()
        .expect("run did-collision-scan");
    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("scanner must emit a report, not refuse to look: {e}\n{stdout}")
    });

    assert!(
        stdout.contains("trust/edges"),
        "the report must name the keyspace it found the collision in:\n{stdout}"
    );
    assert!(
        report.is_object() || report.is_array(),
        "report shape: {report}"
    );
}
