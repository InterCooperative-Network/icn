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
    let author = icn_identity::KeyPair::generate().unwrap().did().clone();
    let entry = icn_ledger::JournalEntry {
        id: None,
        timestamp: 1_700_000_000,
        author: author.clone(),
        contract_ref: None,
        accounts: deltas
            .iter()
            .map(|(currency, debit, credit)| icn_ledger::AccountDelta {
                account_id: author.clone(),
                currency: (*currency).to_string(),
                debit: if *debit == 0 { None } else { Some(*debit) },
                credit: if *credit == 0 { None } else { Some(*credit) },
            })
            .collect(),
        parents: Vec::new(),
        signature: None,
        nonce: None,
        provenance: icn_ledger::types::ProvenanceRef::SystemGenerated {
            reason: "icn#2717 fixture".to_string(),
        },
    };
    write_journal_row(data_dir, &serde_json::to_vec(&entry).unwrap());
}

/// Build one `JournalEntry` from `(currency, debit, credit)` deltas.
fn journal_entry(deltas: &[(&str, i64, i64)]) -> icn_ledger::JournalEntry {
    let author = icn_identity::KeyPair::generate().unwrap().did().clone();
    icn_ledger::JournalEntry {
        id: None,
        timestamp: 1_700_000_000,
        author: author.clone(),
        contract_ref: None,
        accounts: deltas
            .iter()
            .map(|(currency, debit, credit)| icn_ledger::AccountDelta {
                account_id: author.clone(),
                currency: (*currency).to_string(),
                debit: if *debit == 0 { None } else { Some(*debit) },
                credit: if *credit == 0 { None } else { Some(*credit) },
            })
            .collect(),
        parents: Vec::new(),
        signature: None,
        nonce: None,
        provenance: icn_ledger::types::ProvenanceRef::SystemGenerated {
            reason: "icn#2717 fixture".to_string(),
        },
    }
}

/// Write two distinct journal rows, so per-row and journal-wide balance can differ.
fn write_two_journal_rows(
    data_dir: &Path,
    first: &[(&str, i64, i64)],
    second: &[(&str, i64, i64)],
) {
    let path = ledger_dir(data_dir);
    std::fs::create_dir_all(&path).expect("fixture: could not create ledger dir");
    let store = SledStore::open(&path).expect("fixture: could not open ledger store");
    for (suffix, deltas) in [(b'1', first), (b'2', second)] {
        let mut key =
            b"ledger:journal:000000000000000000000000000000000000000000000000000000000000000"
                .to_vec();
        key.push(suffix);
        store
            .put(&key, &serde_json::to_vec(&journal_entry(deltas)).unwrap())
            .expect("fixture: could not write journal row");
    }
    store.db().flush().expect("fixture: could not flush ledger");
    drop(store);

    assert!(
        path.join("conf").is_file() && path.join("db").is_file(),
        "fixture: a complete sled database must exist at the canonical path"
    );
}

/// Write one raw journal row at the canonical ledger path.
///
/// Kept separate from [`seed_ledger`] so the corrupt/tampered fixtures can write
/// bytes that are deliberately NOT a `JournalEntry` without going through the
/// typed constructor.
fn write_journal_row(data_dir: &Path, body: &[u8]) {
    let path = ledger_dir(data_dir);
    std::fs::create_dir_all(&path).expect("fixture: could not create ledger dir");
    let store = SledStore::open(&path).expect("fixture: could not open ledger store");
    store
        .put(
            b"ledger:journal:0000000000000000000000000000000000000000000000000000000000000001",
            body,
        )
        .expect("fixture: could not write journal row");
    store.db().flush().expect("fixture: could not flush ledger");
    drop(store); // release the sled lock before the binary runs

    assert!(
        ledger_dir(data_dir).join("conf").is_file(),
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
    // Name the row AND the amount, not just "something was wrong". The message
    // changed when balance moved to per-entry scope; asserting the specific
    // wording is what keeps this from passing on a vaguer failure later.
    assert!(
        text.contains("do not balance"),
        "the failure must say the rows do not balance:\n{text}"
    );
    assert!(
        text.contains("sums to 60"),
        "it must name the offending currency and amount, so the operator can find \
         the row rather than being told only that something failed:\n{text}"
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
    // The exact count, not a disjunction that its own second operand subsumes:
    // `contains("Found 1 ...") || contains("ledger entries")` can never fail on
    // the count, so a verifier that read the wrong number of rows would pass it.
    assert!(
        text.contains("Found 1 ledger entries"),
        "it must report the exact number of rows it inspected, so success is \
         evidence about what was read:\n{text}"
    );
    assert!(
        !text.contains("No ledger database found"),
        "it must not claim the ledger is absent:\n{text}"
    );
    // Pin the POSITIVE summary. `success_without_a_computed_balance_…` asserts the
    // absence of this exact sentence; without something asserting its presence
    // here, rewording it would make that negative unfalsifiable and leave the
    // balanced branch's headline claim pinned by nothing.
    assert!(
        text.contains("Verified: archive integrity, the double-entry invariant"),
        "a balanced ledger must state that the double-entry invariant was among \
         the things verified:\n{text}"
    );
    // The success summary must name only what was checked. This command verifies
    // Sigma-debit == Sigma-credit and nothing else about the ledger, so a plural
    // "ledger invariants" claim would cover validations it never runs.
    assert!(
        !text.contains("ledger invariants"),
        "success must not claim ledger invariants in general; only the \
         double-entry invariant is checked:\n{text}"
    );
    assert!(
        !text.contains("This backup can be safely restored"),
        "even under --verify-ledger this command does not check hashes, \
         signatures or provenance, so it must not certify safe restoration:\n{text}"
    );
    assert!(
        text.contains("were NOT performed"),
        "success must state which ledger validations it did not perform:\n{text}"
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
    write_journal_row(&data_dir, b"\x00\x01\x02 not json at all");

    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "a ledger with unreadable rows must fail --verify-ledger:\n{text}"
    );
    assert!(
        text.contains("could not be decoded as journal entries"),
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

/// The two success terminals that reach the banner WITHOUT computing a balance
/// must not claim the double-entry invariant was verified.
///
/// These were the only success paths in the command with no test. Both exit 0
/// and print the full `--verify-ledger` summary, so under the acceptance ceiling
/// they are claims the command makes and need discriminating evidence like any
/// other. A journal whose entries carry no currency deltas satisfies the
/// per-currency invariant *vacuously*; saying it was verified there would
/// contradict the detail line printed three lines above.
#[test]
fn success_without_a_computed_balance_does_not_claim_the_invariant_was_verified() {
    // (label, seed the ledger, expected detail line)
    let cases: [(&str, bool, &str); 2] = [
        ("empty ledger", false, "Ledger empty (no entries)"),
        (
            "entries with no currency delta",
            true,
            // Must be unique to the DETAIL line. "carried no currency delta"
            // alone also appears in the summary that BOTH arms print, so arm 2
            // would pass without its terminal ever executing.
            "1 entries read; none carried a currency delta",
        ),
    ];

    for (label, with_rows, expected_detail) in cases {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let archive = dir.path().join("backup.tar");

        init_identity(&data_dir);
        if with_rows {
            // A decodable entry with an empty `accounts` array: nothing to sum.
            write_journal_row(&data_dir, &serde_json::to_vec(&journal_entry(&[])).unwrap());
        } else {
            // A real database with no journal rows at all.
            let path = ledger_dir(&data_dir);
            std::fs::create_dir_all(&path).unwrap();
            let store = SledStore::open(&path).unwrap();
            store.db().flush().unwrap();
            drop(store);
        }
        make_backup(&data_dir, &archive);

        let out = verify(&archive, true);
        let text = combined(&out);

        assert!(
            out.status.success(),
            "[{label}] a ledger with nothing to balance is not itself a failure:\n{text}"
        );
        assert!(
            text.contains(expected_detail),
            "[{label}] the detail line must say what was actually read:\n{text}"
        );
        // The claim, which is the point of these two cases.
        assert!(
            !text.contains("Verified: archive integrity, the double-entry invariant"),
            "[{label}] no balance was computed, so the summary must not claim the \
             double-entry invariant was verified:\n{text}"
        );
        assert!(
            text.contains("no double-entry balance was computed"),
            "[{label}] the summary must say plainly that no balance was computed:\n{text}"
        );
        assert!(
            !text.contains("This backup can be safely restored"),
            "[{label}] it must not certify safe restoration:\n{text}"
        );
    }
}

/// A hostile journal key must not reach the operator's terminal verbatim.
///
/// The archive is untrusted input — judging it is the whole job. A key carrying
/// newlines and ANSI escapes, printed raw into a failure diagnostic, lets a
/// crafted backup repaint this command's own output, up to forging a success
/// banner over a failing run.
#[test]
fn a_hostile_journal_key_cannot_inject_text_into_the_report() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);

    let path = ledger_dir(&data_dir);
    std::fs::create_dir_all(&path).unwrap();
    let store = SledStore::open(&path).unwrap();
    // A decodable-prefix key whose suffix is a forged banner behind ANSI escapes.
    let mut key = b"ledger:journal:".to_vec();
    key.extend_from_slice(b"\x1b[2J\x1b[H\n\xe2\x9c\x93 BACKUP VERIFICATION PASSED\n");
    store
        .put(&key, b"\x00 not a journal entry")
        .expect("fixture: could not write hostile row");
    store.db().flush().unwrap();
    drop(store);
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "an undecodable row must still fail:\n{text}"
    );
    // The injection, not merely the failure.
    assert!(
        !text.contains("\x1b["),
        "no ANSI escape from the archive may reach the terminal:\n{text:?}"
    );
    assert!(
        !text.contains("✓ BACKUP VERIFICATION PASSED"),
        "a key must not be able to forge the success banner:\n{text}"
    );
    assert!(
        text.contains("\\x1b"),
        "the key should still be reported, escaped rather than executed:\n{text}"
    );
}

/// A hostile *currency* must not inject text either.
///
/// The key is not the only attacker-influenced value in a row: `currency`,
/// the `account_id` inside a `net_change` error, and whatever a serde error
/// quotes back all come from the archive. Escaping the key alone left this one
/// live, which is why sanitizing moved to the output boundary — this test is the
/// evidence that the boundary covers a field nobody enumerated individually.
#[test]
fn a_hostile_currency_cannot_inject_text_into_the_report() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // A structurally valid entry, imbalanced, whose currency carries a forged
    // banner behind ANSI escapes.
    // ANSI escapes AND Unicode format characters. U+202E (right-to-left
    // override) and U+2066 (left-to-right isolate) are NOT `char::is_control()`,
    // so a control-character denylist passes them through and they can visually
    // reorder or conceal the rest of the line. They are in this fixture because
    // the denylist version of the sanitizer missed exactly them.
    let hostile = "hours\x1b[2J\x1b[H\n\u{202e}\u{2066}\u{200b}✓ BACKUP VERIFICATION PASSED";
    write_journal_row(
        &data_dir,
        &serde_json::to_vec(&journal_entry(&[(hostile, 100, 40)])).unwrap(),
    );
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "an imbalanced row must still fail:\n{text}"
    );
    // The mechanism, not the substring. Control bytes are what let injected
    // content reposition the cursor, clear the screen, or start a new line that
    // the operator reads as the command's own output. Once they are escaped, the
    // payload can only appear as inert text inside a clearly-marked failure line.
    assert!(
        !text.contains('\x1b'),
        "no ANSI escape from the archive may reach the terminal:\n{text:?}"
    );
    assert!(
        !text
            .lines()
            .any(|l| l.trim_start().starts_with("✓ BACKUP VERIFICATION PASSED")),
        "the archive must not be able to produce a line the operator reads as this \
         command's own verdict:\n{text}"
    );
    assert!(
        text.contains("\\u{001b}"),
        "the currency should still be reported, escaped rather than executed:\n{text}"
    );
    // The bidi/format characters must be escaped too, not merely the control ones.
    for (label, ch) in [("RTL override", '\u{202e}'), ("LTR isolate", '\u{2066}')] {
        assert!(
            !text.contains(ch),
            "[{label}] a Unicode format character must not reach the terminal \
             — it is not `is_control()`, which is why a denylist missed it:\n{text:?}"
        );
    }
    assert!(
        text.contains("\\u{202e}") && text.contains("\\u{2066}"),
        "they must still be reported, in escaped form:\n{text}"
    );
    // And the payload must not have gained its own line.
    assert!(
        text.lines()
            .filter(|l| l.contains("BACKUP VERIFICATION PASSED"))
            .all(|l| l.contains("currency") || l.contains("ledger:journal:")),
        "injected text may only appear inside the failure diagnostic that reports \
         it, never standing alone:\n{text}"
    );
}

/// The failing-row count must be a count of ROWS.
///
/// Detail lines are per currency, so one row imbalanced in two currencies used to
/// contribute two — letting the report print "2 of 1 ledger row(s) do not
/// balance", an impossible statement about the operator's own data.
#[test]
fn one_row_imbalanced_in_two_currencies_is_counted_as_one_row() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // ONE entry, imbalanced in BOTH currencies.
    write_journal_row(
        &data_dir,
        &serde_json::to_vec(&journal_entry(&[("hours", 100, 40), ("kwh", 5, 1)])).unwrap(),
    );
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "an imbalanced row must fail:\n{text}"
    );
    assert!(
        text.contains("1 of 1 ledger row(s) do not balance"),
        "the count must be of rows, not of per-currency detail lines:\n{text}"
    );
    // Both currencies should still be named in the detail lines.
    assert!(
        text.contains("currency hours") && text.contains("currency kwh"),
        "both offending currencies must still be reported:\n{text}"
    );
}

/// A row whose per-currency total overflows `i64` must fail, even though the
/// deltas cancel.
///
/// `Ledger::validate_entry` accumulates in `HashMap<String, i64>` via
/// `AccountDelta::net_change()` and rejects overflow as `ArithmeticOverflow`, so
/// this row would never have been accepted on append. Computing the same sum in a
/// widened `i128` cannot overflow: it reaches zero and reports the row verified —
/// accepting precisely what the ledger refuses.
#[test]
fn a_row_whose_running_total_overflows_i64_is_not_reported_as_verified() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // debit i64::MAX, debit 1, then matching credits: cancels in i128, overflows
    // the ledger's i64 accumulator.
    write_journal_row(
        &data_dir,
        &serde_json::to_vec(&journal_entry(&[
            ("hours", i64::MAX, 0),
            ("hours", 1, 0),
            ("hours", 0, i64::MAX),
            ("hours", 0, 1),
        ]))
        .unwrap(),
    );
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "a row the ledger would reject as an arithmetic overflow must not verify:\n{text}"
    );
    assert!(
        text.contains("overflow"),
        "the failure must name the overflow rather than a generic imbalance:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "it must not print the success banner:\n{text}"
    );
}

/// Two rows whose imbalances cancel must still fail.
///
/// `Ledger::validate_entry` enforces Σdebit == Σcredit per currency **for each
/// entry**, so a +60 row and a −60 row are both invalid and neither would have
/// been accepted on append. A verifier that accumulates across the whole journal
/// sees zero and reports the invariant verified — a weaker check than the one the
/// ledger actually enforces, wearing the same name.
#[test]
fn two_rows_whose_imbalances_cancel_are_not_reported_as_verified() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // +60 hours in one row, −60 in another: zero ledger-wide, both invalid.
    write_two_journal_rows(
        &data_dir,
        &[("hours", 100, 40)], // debit 100, credit 40  -> +60
        &[("hours", 40, 100)], // debit 40,  credit 100 -> -60
    );
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "rows that individually do not balance must fail even when their totals \
         cancel across the journal:\n{text}"
    );
    assert!(
        text.contains("do not balance"),
        "the failure must say which rows did not balance:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "it must not print the success banner:\n{text}"
    );
}

/// A row that is valid JSON but not a valid journal entry must not be skipped.
///
/// `serde_json::Value` parses `{}` happily, so a structurally invalid row used to
/// contribute nothing to the per-currency sums and leave them empty — a wholly
/// corrupt ledger then reported "no currencies" and passed. Parsing is not
/// interpreting.
#[test]
fn schema_invalid_ledger_rows_are_not_silently_skipped() {
    for (label, body) in [
        ("no accounts array", br#"{}"#.to_vec()),
        (
            "account without a string currency",
            br#"{"accounts":[{"debit":100}]}"#.to_vec(),
        ),
        (
            "non-integer debit",
            br#"{"accounts":[{"currency":"hours","debit":"lots"}]}"#.to_vec(),
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let archive = dir.path().join("backup.tar");

        init_identity(&data_dir);
        write_journal_row(&data_dir, &body);
        make_backup(&data_dir, &archive);

        let out = verify(&archive, true);
        let text = combined(&out);

        assert!(
            !out.status.success(),
            "[{label}] a row that is not a valid journal entry must fail \
             --verify-ledger:\n{text}"
        );
        assert!(
            text.contains("could not be decoded as journal entries"),
            "[{label}] the failure must say the row was not interpretable:\n{text}"
        );
        assert!(
            !text.contains("BACKUP VERIFICATION PASSED"),
            "[{label}] it must not print the success banner:\n{text}"
        );
    }
}

/// A ledger *directory* with no database inside must not be "opened".
///
/// `SledStore::open` calls `sled::open`, which CREATES when nothing is there. An
/// archive carrying an empty `store/ledger` (icnd creates the directory before
/// opening it, so a crash in between leaves exactly that) would otherwise have a
/// fresh empty database created at verify time and reported as verified — this
/// command certifying a database the backup does not contain.
#[test]
fn an_empty_ledger_directory_is_refused_rather_than_created() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // The directory exists and is archived, but holds no sled database.
    let path = ledger_dir(&data_dir);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("placeholder"), b"not a database").unwrap();
    make_backup(&data_dir, &archive);

    assert!(
        archive_contains_ledger(&archive),
        "fixture: the archive must carry the ledger directory itself"
    );

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "an empty ledger directory must be refused, not opened into a new \
         database:\n{text}"
    );
    assert!(
        text.contains("holds no ledger database"),
        "the failure must say the directory held no database:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "it must not print the success banner:\n{text}"
    );
}

/// A `conf`-without-`db` ledger directory must be refused — and the check must
/// run BEFORE the N2-A gate.
///
/// This is the fixture that makes the ordering observable, and mutation testing
/// is what showed it was missing: `an_empty_ledger_directory_is_refused_rather_
/// than_created` uses a directory with no `conf`, which `find_sled_roots` never
/// discovers, so the gate never opens it and the presence check fails from either
/// position. Only a directory sled *recognises* distinguishes them.
///
/// With `conf` present and `db` absent — an interrupted sled init — the gate
/// discovers the root (`did_collision_scan.rs` matches on `conf`), opens it with
/// a creating `sled::open`, and materialises `db`. A presence check running after
/// the gate would then find a complete database and certify a ledger the backup
/// never contained.
#[test]
fn a_conf_without_db_ledger_directory_is_refused_before_the_gate_can_create_one() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);

    // Build a real sled database so `conf` is genuine, then remove `db` to model
    // an initialisation that was interrupted between the two writes.
    seed_ledger(&data_dir, &[("hours", 100, 0), ("hours", 0, 100)]);
    let path = ledger_dir(&data_dir);
    std::fs::remove_file(path.join("db")).expect("fixture: could not remove db");
    assert!(
        path.join("conf").is_file(),
        "fixture: conf must survive, or the gate will not discover this root"
    );
    assert!(
        !path.join("db").exists(),
        "fixture: db must be absent, or there is nothing to distinguish"
    );

    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "a conf-only ledger directory must be refused, not completed by the gate \
         and then certified:\n{text}"
    );
    assert!(
        text.contains("holds no ledger database"),
        "the failure must name the incomplete database:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "it must not print the success banner:\n{text}"
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
    // A real, balanced ledger as well. `assert_backup_carried_a_ledger` runs
    // BEFORE the N2-A gate (it has to — see the handler), so without one this
    // fixture would fail on the missing-ledger bail and never reach the gate it
    // exists to exercise. A backup of a node that has run has both.
    seed_ledger(&data_dir, &[("hours", 100, 0), ("hours", 0, 100)]);

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

    // The canonical trust path (`{data_dir}/store/trust`), not `{data_dir}/store`.
    // The gate would discover either, since `find_sled_roots` recurses — but this
    // PR is about store-layout ownership, and a fixture modelling the exact
    // layout icn#2718 fixed would be a poor thing to leave behind in it.
    let trust_dir = data_dir.join("store").join("trust");
    std::fs::create_dir_all(&trust_dir).unwrap();
    let store = SledStore::open(&trust_dir).unwrap();
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

    // The headline claim itself. Without this, re-adding "This backup can be
    // safely restored" to the bare branch would leave every other assertion here
    // green — the change would be unpinned by the very test named for it.
    assert!(
        !text.contains("This backup can be safely restored"),
        "bare verify-backup checked neither the ledger nor the N2-A audit, so it \
         must not claim the backup can be safely restored:\n{text}"
    );
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
