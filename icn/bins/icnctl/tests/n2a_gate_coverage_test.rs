//! `icnctl` refuses a data directory the N2-A startup gate refuses (#2627 M4d).
//!
//! `icnd` runs the gate in `main` before it opens a single store, so the daemon
//! can never fold alias-spelled rows of one principal. `icnctl` opens the same
//! sled databases beneath the same data directory, and its maintenance
//! commands are documented to run *with the daemon stopped* — precisely the
//! window in which nothing else has checked them. Before M4d the guarantee
//! therefore held only for whichever binary an operator happened to reach for.
//!
//! These tests drive the real binary against real sled fixtures and assert
//! **refusal semantics**, not that a function was called: the command exits
//! non-zero, says why, and leaves the store byte-for-byte unchanged.
//!
//! The companion half — that `did-collision-scan` can still read the very state
//! these commands now refuse — lives in `icn-store`, where
//! `CARGO_BIN_EXE_did-collision-scan` resolves.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use icn_identity::Did;
use icn_store::{SledStore, Store};
use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// A freshly minted principal, in the base58btc spelling the mint produces.
fn fresh_did() -> Did {
    icn_identity::KeyPair::generate().unwrap().did().clone()
}

/// Two accepted spellings of ONE principal: the minted base58btc form, and the
/// same 32 identifier bytes re-encoded as base16.
fn alias_pair() -> (Did, Did) {
    let a = fresh_did();
    let bytes = a.identifier_bytes().expect("a minted spelling decodes");
    let b: Did = format!("did:icn:f{}", hex::encode(bytes))
        .parse()
        .expect("a re-encoding of a valid key is a valid DID");
    assert_eq!(a, b, "fixture must be one principal");
    assert_ne!(a.as_str(), b.as_str(), "under two spellings");
    (a, b)
}

/// `icnctl init-coop` opens `<data_dir>/store` itself as a sled database — a
/// sibling of the daemon's `<data_dir>/store/{trust,ledger,...}` stores, and a
/// sled root the gate discovers like any other.
fn store_root(data_dir: &Path) -> PathBuf {
    data_dir.join("store")
}

/// Plant an alias pair in `trust/edges/` — a *registered* N2-A keyspace whose
/// basis is `AwaitingDomainSignOff`, so a collision there fails closed however
/// its disposition reads.
fn seed_alias_collision(data_dir: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let (a, b) = alias_pair();
    let peer = fresh_did();

    let store = SledStore::open(store_root(data_dir)).unwrap();
    for src in [&a, &b] {
        let key = format!("trust/edges/{}:{}", src.as_str(), peer.as_str());
        store.put(key.as_bytes(), b"{}").unwrap();
    }
    store.db().flush().unwrap();
    let snapshot = store.scan(b"trust/edges/").unwrap();
    drop(store); // release the sled lock for the binary
    snapshot
}

/// A store with one spelling per principal: nothing for the gate to refuse.
fn seed_clean(data_dir: &Path) {
    let store = SledStore::open(store_root(data_dir)).unwrap();
    let key = format!(
        "trust/edges/{}:{}",
        fresh_did().as_str(),
        fresh_did().as_str()
    );
    store.put(key.as_bytes(), b"{}").unwrap();
    store.db().flush().unwrap();
    drop(store);
}

fn run_icnctl(data_dir: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(icnctl_bin());
    cmd.arg("--data-dir").arg(data_dir);
    cmd.args(args);
    cmd.output().unwrap()
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn trust_rows(data_dir: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let store = SledStore::open(store_root(data_dir)).unwrap();
    let rows = store.scan(b"trust/edges/").unwrap();
    drop(store);
    rows
}

#[test]
fn coop_maintenance_refuses_a_data_directory_the_gate_refuses() {
    let dir = TempDir::new().unwrap();
    let before = seed_alias_collision(dir.path());

    let out = run_icnctl(dir.path(), &["coop", "entity-report"]);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "a gate-refused data directory must fail the command, got success:\n{text}"
    );
    assert!(
        text.contains("N2-A startup gate refused"),
        "the refusal must say what happened:\n{text}"
    );
    assert_eq!(
        trust_rows(dir.path()),
        before,
        "a refused command must leave the store byte-for-byte unchanged"
    );
}

#[test]
fn treasury_maintenance_refuses_a_data_directory_the_gate_refuses() {
    let dir = TempDir::new().unwrap();
    let before = seed_alias_collision(dir.path());

    // The mutating form, with both confirmations supplied: refusal must happen
    // before any write is even considered.
    let out = run_icnctl(
        dir.path(),
        &[
            "treasury",
            "entity-backfill-apply",
            "--apply",
            "--confirm-apply",
        ],
    );
    let text = combined(&out);

    assert!(!out.status.success(), "must refuse:\n{text}");
    assert!(text.contains("N2-A startup gate refused"), "{text}");
    assert_eq!(
        trust_rows(dir.path()),
        before,
        "mutation must not occur before a successful gate"
    );
}

#[test]
fn init_coop_refuses_a_data_directory_the_gate_refuses() {
    // The path M4d proved was a real bypass: `init-coop` opened
    // `<data_dir>/store` and wrote `trust/edges/` rows with no gate and — unlike
    // the treasury path — no loader guard behind it either.
    let dir = TempDir::new().unwrap();
    let before = seed_alias_collision(dir.path());

    let out = run_icnctl(
        dir.path(),
        &["init-coop", "--name", "Probe Coop", "--yes", "--no-start"],
    );
    let text = combined(&out);

    assert!(!out.status.success(), "must refuse:\n{text}");
    assert!(text.contains("N2-A startup gate refused"), "{text}");
    assert_eq!(
        trust_rows(dir.path()),
        before,
        "no trust edge may be written into a store the gate refuses"
    );
}

#[test]
fn a_clean_data_directory_is_not_refused() {
    // The control that stops the fix from being "refuse everything". A store
    // with one spelling per principal must get past the gate — whatever the
    // command then does about its own missing inputs.
    let dir = TempDir::new().unwrap();
    seed_clean(dir.path());

    let out = run_icnctl(dir.path(), &["coop", "entity-report"]);
    let text = combined(&out);

    assert!(
        !text.contains("N2-A startup gate refused"),
        "a clean store must not be refused by the gate:\n{text}"
    );
}

#[test]
fn an_absent_data_directory_is_not_refused() {
    // A directory that does not exist holds no rows, so there is nothing to
    // fold and nothing to audit. `init-coop` on a fresh machine takes this arm;
    // treating absence as a refusal would make first-run setup impossible.
    let dir = TempDir::new().unwrap();
    let fresh = dir.path().join("not-created-yet");

    let out = run_icnctl(&fresh, &["coop", "entity-report"]);
    let text = combined(&out);

    assert!(
        !text.contains("N2-A startup gate refused"),
        "an absent data directory is not a verdict:\n{text}"
    );
}
