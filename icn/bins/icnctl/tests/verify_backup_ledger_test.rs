//! `icnctl verify-backup --verify-ledger` must prove it opened the ledger it
//! claims to have verified (#2717).
//!
//! The backup writes the data directory at archive root:
//!
//! ```text
//! tar_builder.append_dir_all(".", data_dir)
//! ```
//!
//! so the canonical `{data_dir}/store/ledger` restores to
//! `{restore_dir}/store/ledger`. The verifier resolved `{restore_dir}/ledger`
//! — one level too high — and its miss branch was *reassuring* rather than
//! fail-closed: it printed `⚠ No ledger database found (may be new node)` and
//! returned `Ok(())`, which still counted toward `✓ BACKUP VERIFICATION PASSED`.
//!
//! The consequence is the sharp one: an archive whose ledger **violates the
//! double-entry invariant** was reported as verified, because the invariant
//! check never opened a database.
//!
//! Every test here drives the real `icnctl` binary through the real operator
//! path — `backup` → tar → `verify-backup --verify-ledger` → restore → ledger
//! — and never a substitute command. That distinction is the point: the
//! previous N2-A coverage for this handler invoked `coop entity-report` against
//! a hand-built directory, so the handler under test was never executed. A
//! control has not been tested merely because a test carrying its name passed.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use icn_store::{SledStore, Store};
use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The canonical ledger location under a data directory.
///
/// Spelled out here on purpose: this test must fail if the *product* stops
/// agreeing with the layout, so it does not import the same helper the fix
/// uses. Agreeing with the code under test by construction would make the
/// assertion vacuous.
fn ledger_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("ledger")
}

/// Provision a real identity. Fail closed: a half-built fixture must never
/// become evidence (the lesson from icn#2730).
fn init_identity(data_dir: &Path) {
    let out = Command::new(icnctl_bin())
        .env("ICN_KEYSTORE_PASSPHRASE", "fixture_passphrase_2717")
        .arg("--data-dir")
        .arg(data_dir)
        .arg("id")
        .arg("init")
        .output()
        .expect("fixture: could not run `icnctl id init`");
    assert!(
        out.status.success(),
        "fixture: `id init` failed:\n{}",
        combined(&out)
    );
    assert!(
        data_dir.join("identity.age").exists(),
        "fixture: id init did not produce identity.age"
    );
}

/// Write real journal rows into the real ledger database at the canonical path.
///
/// Key and value match what `icn-ledger` actually writes: the key prefix is
/// `ledger:journal:` (`icn/crates/icn-ledger/src/ledger.rs`) and the value is
/// `serde_json::to_vec(&entry)` over an entry whose `accounts` array carries
/// `currency` / `debit` / `credit` — the exact shape the verifier parses.
///
/// `deltas` is `(currency, debit, credit)`.
fn seed_ledger(data_dir: &Path, deltas: &[(&str, i64, i64)]) {
    let path = ledger_dir(data_dir);
    std::fs::create_dir_all(&path).expect("fixture: could not create ledger dir");
    let store = SledStore::open(&path).expect("fixture: could not open ledger store");

    let accounts: Vec<serde_json::Value> = deltas
        .iter()
        .map(|(currency, debit, credit)| {
            serde_json::json!({
                "account_id": "did:icn:z6MkfixtureAccount000000000000000000000000",
                "currency": currency,
                "debit": if *debit == 0 { serde_json::Value::Null } else { (*debit).into() },
                "credit": if *credit == 0 { serde_json::Value::Null } else { (*credit).into() },
            })
        })
        .collect();
    let entry = serde_json::json!({ "accounts": accounts });

    store
        .put(
            b"ledger:journal:0000000000000000000000000000000000000000000000000000000000000001",
            &serde_json::to_vec(&entry).unwrap(),
        )
        .expect("fixture: could not write journal row");
    store.db().flush().expect("fixture: could not flush ledger");
    drop(store); // release the sled lock before the binary runs

    assert!(
        ledger_dir(data_dir).exists(),
        "fixture: ledger database was not created at the canonical path"
    );
}

fn make_backup(data_dir: &Path, out_file: &Path) {
    let out = Command::new(icnctl_bin())
        .arg("--data-dir")
        .arg(data_dir)
        .arg("backup")
        .arg(out_file)
        .output()
        .expect("fixture: could not run `icnctl backup`");
    assert!(
        out.status.success(),
        "fixture: `backup` failed:\n{}",
        combined(&out)
    );
    assert!(out_file.exists(), "fixture: backup archive was not written");
}

/// Prove the archive really carries the ledger, so a later "no ledger found"
/// can only mean the verifier looked in the wrong place.
fn archive_contains_ledger(archive: &Path) -> bool {
    let file = std::fs::File::open(archive).expect("fixture: could not open archive");
    let mut tar = tar::Archive::new(file);
    tar.entries()
        .expect("fixture: could not read archive entries")
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .map(|p| p.to_string_lossy().contains("store/ledger"))
                .unwrap_or(false)
        })
}

fn verify(archive: &Path, with_ledger: bool) -> Output {
    let mut cmd = Command::new(icnctl_bin());
    cmd.arg("verify-backup").arg(archive);
    if with_ledger {
        cmd.arg("--verify-ledger");
    }
    cmd.output().expect("could not run `icnctl verify-backup`")
}

// ── the reproduction: a broken ledger reported as verified ──────────────────

/// THE discriminating case. An archive whose ledger violates the double-entry
/// invariant must not pass `--verify-ledger`.
///
/// Pre-fix this fails: the verifier resolves `{restore}/ledger`, misses, prints
/// "may be new node" and returns Ok, and the command reports PASSED.
#[test]
fn an_imbalanced_ledger_in_a_real_archive_is_not_reported_as_verified() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // 100 debited, 40 credited: net +60 "hours". A real check must reject this.
    seed_ledger(&data_dir, &[("hours", 100, 0), ("hours", 0, 40)]);
    make_backup(&data_dir, &archive);

    assert!(
        archive_contains_ledger(&archive),
        "fixture: the archive must actually contain store/ledger, or this test \
         proves nothing about where the verifier looked"
    );

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "an archive whose ledger is imbalanced must FAIL --verify-ledger, but the \
         command succeeded:\n{text}"
    );
    assert!(
        text.contains("double-entry invariant violated") || text.contains("imbalance"),
        "the failure must name the invariant that was violated:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "it must not also print the success banner:\n{text}"
    );
    assert!(
        !text.contains("No ledger database found"),
        "the ledger IS in this archive; reporting it absent means the verifier \
         looked in the wrong place:\n{text}"
    );
}

/// The positive half: a balanced ledger verifies, and says so specifically.
#[test]
fn a_balanced_ledger_is_verified_and_reported_as_actually_inspected() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    seed_ledger(&data_dir, &[("hours", 100, 0), ("hours", 0, 100)]);
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        out.status.success(),
        "a balanced ledger must verify:\n{text}"
    );
    assert!(
        text.contains("Double-entry invariant verified"),
        "success must state that the invariant was actually checked, not merely \
         that the command finished:\n{text}"
    );
    assert!(
        text.contains("Found 1 ledger entries") || text.contains("ledger entries"),
        "it must report the rows it inspected, so success is evidence about what \
         was read:\n{text}"
    );
    assert!(
        !text.contains("No ledger database found"),
        "it must not claim the ledger is absent:\n{text}"
    );
}

/// Requesting ledger verification on an archive with no ledger must not be
/// silently counted as verified.
#[test]
fn verify_ledger_on_an_archive_without_a_ledger_does_not_silently_pass() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir); // identity only: no store/ledger at all
    make_backup(&data_dir, &archive);

    assert!(
        !archive_contains_ledger(&archive),
        "fixture: this archive must NOT contain a ledger"
    );

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "--verify-ledger was explicitly requested and could not be performed, so \
         the command must not report success:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "an unperformed verification must not print the success banner:\n{text}"
    );
}

/// A tampered ledger — rows present, contents unreadable — must fail.
///
/// `icn-ledger` writes every journal row with `serde_json::to_vec`, so a row
/// that will not parse is corrupt or tampered. The verifier used to count these
/// into `parse_errors`, print a warning, and then still report
/// `✓ Double-entry invariant verified` — an invariant asserted over rows it had
/// skipped. That is the same overclaim as the missing ledger, one level in.
#[test]
fn a_tampered_ledger_whose_rows_cannot_be_read_is_not_reported_as_verified() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);

    // A real sled database at the canonical path, holding a real journal key
    // whose value is not JSON at all.
    let path = ledger_dir(&data_dir);
    std::fs::create_dir_all(&path).unwrap();
    let store = SledStore::open(&path).expect("fixture: could not open ledger store");
    store
        .put(
            b"ledger:journal:0000000000000000000000000000000000000000000000000000000000000001",
            b"\x00\x01\x02 not json at all",
        )
        .expect("fixture: could not write tampered row");
    store.db().flush().unwrap();
    drop(store);

    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "a ledger with unreadable rows must fail --verify-ledger:\n{text}"
    );
    assert!(
        text.contains("could not be parsed"),
        "the failure must say the rows could not be read:\n{text}"
    );
    assert!(
        !text.contains("Double-entry invariant verified"),
        "it must not claim the invariant was verified over rows it could not \
         read:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "a tampered ledger must not print the success banner:\n{text}"
    );
}

// ── the M4d gap: the gate, reached through the real handler ─────────────────

/// The N2-A refusal proven through `verify-backup --verify-ledger` itself.
///
/// The previous coverage for this path invoked `coop entity-report` against a
/// directory shaped like a restore, with a comment saying a real archive would
/// be needed. This drives the archive.
#[test]
fn verify_backup_verify_ledger_refuses_a_restored_tree_the_n2a_gate_refuses() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);

    // Two accepted spellings of ONE principal in a registered N2-A keyspace.
    let a = icn_identity::KeyPair::generate().unwrap().did().clone();
    let bytes = a.identifier_bytes().expect("a minted spelling decodes");
    let b: icn_identity::Did = format!("did:icn:f{}", hex::encode(bytes))
        .parse()
        .expect("base16 re-encoding is an accepted spelling");
    // Fail closed on the fixture itself: if these two were not one principal
    // under two spellings, the gate would have nothing to refuse and this test
    // would pass for the wrong reason.
    assert_eq!(a, b, "fixture must be ONE principal");
    assert_ne!(a.as_str(), b.as_str(), "under TWO spellings");
    let peer = icn_identity::KeyPair::generate().unwrap().did().clone();

    let store = SledStore::open(data_dir.join("store")).unwrap();
    for src in [&a, &b] {
        let key = format!("trust/edges/{}:{}", src.as_str(), peer.as_str());
        store.put(key.as_bytes(), b"{}").unwrap();
    }
    store.db().flush().unwrap();
    drop(store);

    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "a backup whose restored tree the gate refuses must not verify:\n{text}"
    );
    assert!(
        text.contains("N2-A startup gate refused"),
        "the refusal must say the gate refused it:\n{text}"
    );
    assert!(
        !text.contains("This backup can be safely restored"),
        "it must not claim the backup can be safely restored:\n{text}"
    );
}

// ── messaging accuracy on the bare command ─────────────────────────────────

/// Bare `verify-backup` must not imply it checked the ledger.
///
/// This asserts only what #2717 owns: the message must not claim a ledger
/// verification that did not happen. It deliberately does not require the bare
/// command to start failing or to run new checks.
#[test]
fn bare_verify_backup_does_not_claim_a_ledger_verification_it_did_not_do() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    seed_ledger(&data_dir, &[("hours", 100, 0), ("hours", 0, 40)]); // imbalanced
    make_backup(&data_dir, &archive);

    let out = verify(&archive, false);
    let text = combined(&out);

    // The bare command does not inspect the ledger, so it must not speak about it.
    assert!(
        !text.contains("Double-entry invariant verified"),
        "bare verify-backup did not check the ledger and must not say it did:\n{text}"
    );
    assert!(
        !text.contains("ledger entries"),
        "bare verify-backup must not report ledger contents it never read:\n{text}"
    );
    // And it must be explicit that ledger verification was not performed, so an
    // operator cannot read the success banner as covering the ledger.
    assert!(
        text.contains("--verify-ledger"),
        "the report must name what was NOT verified and how to ask for it:\n{text}"
    );
}
