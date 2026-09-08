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

/// Collapse every run of whitespace to one space.
///
/// The operator summary is a wrapped paragraph printed as several `println!`
/// lines, so a substring that reads as one sentence may straddle a line break.
/// Asserting against the raw text made these tests depend on *where* the wrap
/// falls, which is not a property any of them means to pin — and which broke
/// three separate assertions when the sentences were rewritten. Flattening
/// removes the dependence without weakening anything: the words, their order and
/// their adjacency are all still asserted.
fn flattened(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
        text.contains("are not valid journal entries"),
        "the failure must say the rows were rejected as journal entries:\n{text}"
    );
    assert!(
        text.contains("does not balance"),
        "and the per-row detail must say why — that this row does not balance, \
         which is a different statement from the row being unreadable:\n{text}"
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
        text.contains("double-entry invariant holds per entry"),
        "success must state that the invariant was actually checked, not merely \
         that the command finished:\n{text}"
    );
    assert!(
        text.contains("valid under icn-ledger's entry validation"),
        "and it must name the owner it asked, because that — not a local copy of \
         the rule — is what the result is evidence of (icn#2736):\n{text}"
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
        text.contains("Verified: archive integrity; that all 1 ledger entries are valid"),
        "a balanced ledger must state that every entry was validated, and over how \
         many rows:\n{text}"
    );
    // The WIDENED claim. Delegation made "every entry carries at least one
    // account delta" a checked property, so icn#2736 required the summary to say
    // so in the same change. Without this the widening is unfalsifiable: dropping
    // the sentence would leave every other assertion here green.
    assert!(
        text.contains("at least one account delta"),
        "the summary must name the property delegation actually added:\n{text}"
    );
    assert!(
        text.contains("checked i64"),
        "and the arithmetic the balance was computed in, since a widened \
         accumulator is the divergence that started this:\n{text}"
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
    // Pin the ENUMERATION, not just its presence. This sentence is the one an
    // operator reads as the not-verified set, so a token silently dropping out of
    // it is a widening of the claim without a widening of the checks — which is
    // exactly what happened to "amount signs" in review. `icn-ledger` does own a
    // sign check (`entry::validate_positive_amounts`, via `JournalEntryBuilder`)
    // that this command does not run, so the omission would have been actively
    // misleading rather than merely incomplete.
    assert!(
        flattened(&text).contains(
            "NOT verified: amount signs, content hashes, signatures, provenance, \
             parent existence."
        ),
        "success must state which ledger validations it did not perform, as a \
         complete enumeration — amount signs included, because icn-ledger owns a \
         sign check (entry::validate_positive_amounts, via JournalEntryBuilder) \
         that this command does not run:\n{text}"
    );
    // Freeze state and credit limits are append-time policy, not properties of a
    // backup. Delegation deliberately does NOT reach them, so the output must not
    // let an operator read "validated by icn-ledger" as covering them.
    let flat = flattened(&text);
    assert!(
        flat.contains("Freeze state, credit limits and progressive limits are append-time policy")
            && flat.contains("deliberately not checked here"),
        "the summary must name what delegation deliberately did not bring with \
         it, or 'valid under icn-ledger's entry validation' overclaims:\n{text}"
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
        !text.contains("double-entry invariant holds per entry"),
        "it must not claim the invariant was verified over rows it could not \
         read:\n{text}"
    );
    assert!(
        !text.contains("valid under icn-ledger's entry validation"),
        "nor that the ledger's own validator accepted rows that never reached \
         it:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "a tampered ledger must not print the success banner:\n{text}"
    );
}

/// The one success terminal that reaches the banner WITHOUT computing a balance
/// must not claim the double-entry invariant was verified.
///
/// This was a success path in the command with no test. It exits 0 and prints the
/// full `--verify-ledger` summary, so under the acceptance ceiling it is a claim
/// the command makes and needs discriminating evidence like any other.
///
/// It used to have TWO arms. The second was a journal whose entries carried no
/// currency deltas — an entry with an empty `accounts` array, which satisfies the
/// per-currency invariant *vacuously*. icn#2717 responded by narrowing the claim
/// there; icn#2736 moved the decision to `icn_ledger::entry_validation`, which
/// REFUSES such an entry, so that arm is now a failure and lives in
/// `an_entry_with_no_account_deltas_is_refused_rather_than_certified`.
///
/// What remains is genuinely unreachable by any other route: every `AccountDelta`
/// carries a currency and an entry with no deltas is refused, so a journal that
/// reaches the banner with nothing summed is a journal with no entries at all.
#[test]
fn an_empty_ledger_does_not_claim_the_invariant_was_verified() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    // A real database with no journal rows at all.
    let path = ledger_dir(&data_dir);
    std::fs::create_dir_all(&path).unwrap();
    let store = SledStore::open(&path).unwrap();
    store.db().flush().unwrap();
    drop(store);
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        out.status.success(),
        "a ledger with nothing to balance is not itself a failure:\n{text}"
    );
    assert!(
        text.contains("Ledger empty (no entries)"),
        "the detail line must say what was actually read:\n{text}"
    );
    // The claim, which is the point of this case.
    assert!(
        !text.contains("double-entry invariant per entry"),
        "no balance was computed, so the summary must not claim the double-entry \
         invariant was verified:\n{text}"
    );
    assert!(
        text.contains("no double-entry balance was computed"),
        "the summary must say plainly that no balance was computed:\n{text}"
    );
    assert!(
        !text.contains("This backup can be safely restored"),
        "it must not certify safe restoration:\n{text}"
    );
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
        text.contains("1 of 1 ledger row(s) are not valid journal entries"),
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
        text.contains("are not valid journal entries") && text.contains("does not balance"),
        "the failure must say which rows were rejected, and that the reason was \
         balance rather than some other defect:\n{text}"
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
        !text.contains("double-entry invariant holds per entry")
            && !text.contains("valid under icn-ledger's entry validation"),
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

// ── icn#2736: the verifier consults icn-ledger's validator, not a copy ──────

/// Recursive content identity of a directory tree.
///
/// Records every path *and* what is at it: directory, symlink target, or file
/// length plus the SHA-256 of its bytes. Two properties matter and neither is
/// decoration. Comparing the whole map catches a **new** file — the sled
/// artefacts (`snap.*`, a grown `db`, a rewritten `conf`) an opening verifier
/// would leave behind — because an added key makes the maps unequal. And the
/// identity is content, never mtime, so the assertion cannot pass merely
/// because a clock did not tick, nor fail merely because it did.
fn tree_identity(root: &Path) -> std::collections::BTreeMap<String, String> {
    use sha2::{Digest, Sha256};

    let mut out = std::collections::BTreeMap::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.expect("could not walk tree");
        let relative = entry
            .path()
            .strip_prefix(root)
            .expect("walked path must be under root")
            .to_string_lossy()
            .to_string();
        if relative.is_empty() {
            continue; // the root itself
        }
        let identity = if entry.file_type().is_symlink() {
            format!(
                "symlink:{}",
                std::fs::read_link(entry.path())
                    .expect("could not read symlink")
                    .display()
            )
        } else if entry.file_type().is_dir() {
            "dir".to_string()
        } else {
            let bytes = std::fs::read(entry.path()).expect("could not read file");
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            format!("file:{}:{:x}", bytes.len(), hasher.finalize())
        };
        out.insert(relative, identity);
    }
    assert!(
        !out.is_empty(),
        "fixture: a tree identity over an empty walk would make every comparison vacuous"
    );
    out
}

fn file_sha(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(path).expect("could not read file"));
    format!("{:x}", hasher.finalize())
}

/// THE discriminating case for icn#2736.
///
/// A `JournalEntry` with an empty `accounts` array decodes cleanly and satisfies
/// a per-currency balance check **vacuously** — there is nothing to sum — so a
/// verifier that mirrors only the balance rule accepts it. `Ledger::validate_entry`
/// rejects it on its first line: *"Entry has no account deltas"*. The ledger
/// would never have accepted this row on append.
///
/// Against `main` this test FAILS: the command exits 0 and prints
/// `BACKUP VERIFICATION PASSED` with a narrowed claim. It can only pass once the
/// verifier asks `icn-ledger`'s own validator instead of re-deriving one rule of
/// it. That is the whole observable purchase of the delegation.
#[test]
fn an_entry_with_no_account_deltas_is_refused_rather_than_certified() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    write_journal_row(&data_dir, &serde_json::to_vec(&journal_entry(&[])).unwrap());
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);

    assert!(
        !out.status.success(),
        "the ledger rejects an entry with no account deltas, so a backup \
         containing one must not verify:\n{text}"
    );
    assert!(
        !text.contains("BACKUP VERIFICATION PASSED"),
        "it must not print the success banner:\n{text}"
    );
    assert!(
        text.contains("no account deltas"),
        "the failure must name the actual defect, not a generic imbalance — an \
         entry with nothing in it does not 'fail to balance':\n{text}"
    );
    assert!(
        !text.contains("This backup can be safely restored"),
        "it must not certify safe restoration:\n{text}"
    );
}

/// Every path a verification run is allowed to touch inside the restored tree.
///
/// A PERMITLIST, not a denylist: anything not named here failing the comparison
/// is the point. Each entry is a side effect that exists today and is understood:
///
/// - `n2a-startup-gate.json` — the N2-A gate writes its receipt into the very
///   directory it audits.
/// - `store/ledger/db`, `snap.*`, `blobs/*` — `SledStore::open` is `sled::open`,
///   which has no read-only mode: it preallocates the backing file and may write
///   a snapshot on open or flush on drop.
///
/// None of these is reached by icn#2736's delegation, which adds no store access
/// at all. They are pre-existing and are tracked as their own debt; this list
/// exists so that the moment anything *else* is written the test fails.
fn is_known_verification_side_effect(path: &str) -> bool {
    path == "n2a-startup-gate.json"
        || path == "store/ledger/db"
        || path.starts_with("store/ledger/snap.")
        || path.starts_with("store/ledger/blobs/")
}

/// Verification must not disturb what it inspects (icn#2717's whole point).
///
/// Three things are proved, because the command destroys the tree it extracts and
/// so the interesting object cannot be observed from outside the process:
///
/// 1. **Operator-visible bytes.** The data directory and the archive file are
///    byte-for-byte identical after a real `--verify-ledger` run. This is the
///    property an operator actually relies on and it holds exactly.
/// 2. **The restored tree's mutation surface is bounded and known.** The archive
///    is extracted here and driven through the *same* sequence the handler drives
///    — the pre-gate recovery open (#2732), then `n2a_startup_gate::enforce`,
///    then `SledStore::open` and a journal scan — with a full content identity
///    taken either side. Every difference must be on the permitlist above, and
///    nothing may disappear. The recovery open is part of the sequence precisely
///    because it must come first; replicating the handler in the wrong order
///    would make this test's claim about a run the handler never performs.
/// 3. **The journal's contents survive.** The rows read back are exactly the row
///    the fixture wrote, and reading them a second time returns the same bytes.
///    So the side effects above are confined to sled's container: the evidence
///    the verdict is about is not altered by the act of verifying it.
///
/// Claim 2 is deliberately NOT "the restored tree is byte-for-byte unchanged".
/// That is false on `main` today and is not achievable through the current store
/// API — see the permitlist. Asserting it would have meant either a red test or a
/// quietly weakened one; asserting the exact surface instead keeps the statement
/// true and still fails on any new write.
#[test]
fn verification_touches_nothing_outside_its_known_side_effect_surface() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    seed_ledger(&data_dir, &[("hours", 60, 0), ("hours", 0, 60)]);
    make_backup(&data_dir, &archive);

    // ── 1. operator-visible bytes ──────────────────────────────────────────
    let data_before = tree_identity(&data_dir);
    let archive_before = file_sha(&archive);

    let out = verify(&archive, true);
    let text = combined(&out);
    assert!(
        out.status.success(),
        "fixture: a balanced ledger must verify, or the claims below are made \
         about a run that failed early:\n{text}"
    );

    assert_eq!(
        tree_identity(&data_dir),
        data_before,
        "verification must not touch the operator's data directory"
    );
    assert_eq!(
        file_sha(&archive),
        archive_before,
        "verification must not touch the archive it read"
    );

    // ── 2. the restored tree, driven through the handler's own sequence ────
    let restore = dir.path().join("restored");
    std::fs::create_dir_all(&restore).unwrap();
    tar::Archive::new(std::fs::File::open(&archive).unwrap())
        .unpack(&restore)
        .expect("could not extract archive");

    let restored_ledger = ledger_dir(&restore);
    assert!(
        restored_ledger.join("conf").is_file() && restored_ledger.join("db").is_file(),
        "fixture: the extracted tree must carry a complete sled database, or the \
         comparison below is about a directory nothing opened"
    );

    let before = tree_identity(&restore);

    // The handler's FIRST open: the #2732 recovery check. It adds no new path to
    // the permitlist because it opens the same sled root the scan below opens —
    // but it does open it, and this test exists to notice if that ever stops
    // being true.
    {
        let store = SledStore::open(&restored_ledger).expect("could not open restored ledger");
        assert!(
            store.db().was_recovered(),
            "a healthy restored ledger must report as recovered, or the handler would refuse this archive"
        );
    }

    icn_store::n2a_startup_gate::enforce(&restore, std::time::SystemTime::now())
        .expect("the N2-A gate must accept this tree");

    let rows_first = {
        let store = SledStore::open(&restored_ledger).expect("could not open restored ledger");
        store
            .scan(b"ledger:journal:")
            .expect("could not scan journal")
    };
    assert!(
        !rows_first.is_empty(),
        "fixture: the scan must have read the journal, or every claim here would \
         be about a database that was never opened"
    );

    let after = tree_identity(&restore);

    let mut unexpected: Vec<String> = Vec::new();
    for (path, identity) in &after {
        if before.get(path) != Some(identity) && !is_known_verification_side_effect(path) {
            unexpected.push(format!("written: {path}"));
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            unexpected.push(format!("removed: {path}"));
        }
    }
    assert!(
        unexpected.is_empty(),
        "verification wrote outside its known side-effect surface: {unexpected:?}\n\
         A verifier is evidence about the artefact it inspected; a write nobody \
         accounted for changes the very thing the verdict is about."
    );

    // `conf` carries sled's on-disk format parameters. Pinned by name because a
    // rewritten `conf` would mean the open had renegotiated the database rather
    // than read it — the difference between inspecting a backup and migrating it.
    assert_eq!(
        after.get("store/ledger/conf"),
        before.get("store/ledger/conf"),
        "opening the restored ledger must not rewrite its sled configuration"
    );

    // ── 3. the journal's contents survive being read ───────────────────────
    let rows_second = {
        let store = SledStore::open(&restored_ledger).expect("could not reopen restored ledger");
        store
            .scan(b"ledger:journal:")
            .expect("could not rescan journal")
    };
    assert_eq!(
        rows_first, rows_second,
        "a second open must return the same journal bytes: the side effects above \
         are sled's container, and must never reach the rows themselves"
    );

    let entry: icn_ledger::JournalEntry = serde_json::from_slice(&rows_first[0].1)
        .expect("the restored row must still decode as the entry the fixture wrote");
    assert_eq!(
        entry.accounts.len(),
        2,
        "the restored row must still be the fixture's two-delta entry, not a \
         re-encoded or repaired one"
    );
    assert!(
        icn_ledger::entry_validation::inspect_entry(&entry).is_valid(),
        "and it must still be valid under the ledger's own entry validation"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// icn#2732 — a ledger that could not be read as written must not be reported as
// an empty one.
// ─────────────────────────────────────────────────────────────────────────────

/// Copy a tree so the fixture's contents can be PROVED without perturbing the
/// artifact under test.
///
/// This helper exists because of a trap that produced a false CLEAR while these
/// tests were being written. `SledStore::open` writes a snapshot, and with a
/// snapshot present sled *errors* on a damaged `db` instead of silently
/// recovering from it. So a fixture that proved its own row count by opening the
/// database it was about to damage changed which sled code path the damage took,
/// and the run came back fail-closed — the defect masked by the act of
/// establishing the precondition. Count on a copy; damage the original.
fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("fixture: could not create copy target");
    for entry in std::fs::read_dir(src).expect("fixture: could not read source tree") {
        let entry = entry.expect("fixture: could not read directory entry");
        let to = dst.join(entry.file_name());
        if entry
            .file_type()
            .expect("fixture: could not stat entry")
            .is_dir()
        {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("fixture: could not copy file");
        }
    }
}

/// Prove, without touching `data_dir`, that its ledger really holds `expected`
/// journal rows.
fn assert_ledger_holds_rows_without_touching_it(dir: &TempDir, data_dir: &Path, expected: usize) {
    let witness = dir.path().join(format!(
        "witness-{}",
        data_dir.file_name().unwrap().to_string_lossy()
    ));
    copy_tree(data_dir, &witness);
    let store = SledStore::open(ledger_dir(&witness)).expect("fixture: could not open the witness");
    let rows = store
        .scan(b"ledger:journal:")
        .expect("fixture: could not scan the witness");
    assert_eq!(
        rows.len(),
        expected,
        "fixture: the ledger must really hold {expected} row(s) before it is \
         damaged, or the assertions below are about an already-empty ledger and \
         discriminate nothing"
    );
}

/// The shared shape of both damage cases.
///
/// A backup taken from a data directory whose ledger database was ALREADY
/// damaged. The checksum in `backup_metadata.json` is computed over the data
/// directory at backup time, so the damage is inside the checksummed bytes and
/// the archive verifies as intact — which is exactly why the checksum cannot
/// stand in for reading the ledger.
fn a_backup_whose_ledger_was_damaged_before_it_was_taken(
    dir: &TempDir,
    name: &str,
    damage: impl Fn(&Path),
) -> PathBuf {
    let data_dir = dir.path().join(name);
    let archive = dir.path().join(format!("{name}.tar"));

    init_identity(&data_dir);
    seed_ledger(&data_dir, &[("hours", 60, 0), ("hours", 0, 60)]);
    assert_ledger_holds_rows_without_touching_it(dir, &data_dir, 1);

    damage(&ledger_dir(&data_dir));

    // The presence proof #2717 established still passes: this is not a missing
    // ledger, it is an unreadable one. Both markers are still files.
    assert!(
        ledger_dir(&data_dir).join("conf").is_file() && ledger_dir(&data_dir).join("db").is_file(),
        "fixture: the damage must leave a directory that still LOOKS like a sled \
         database, or this exercises #2717's presence check instead of #2732"
    );

    make_backup(&data_dir, &archive);
    assert!(
        archive_contains_ledger(&archive),
        "fixture: the archive must carry the ledger directory"
    );
    archive
}

/// A refusal may not tell the operator WHY, because nothing here establishes why.
///
/// A presence check on the hedge is not enough on its own: a future edit that
/// re-added `— a truncated or partially written copy, a failed transfer, or
/// tampering —` *alongside* "cannot be narrowed further" would satisfy the hedge
/// assertion and still deliver the fabricated conclusion. The regression to guard
/// is the reappearance of a cause, so this asserts its ABSENCE.
///
/// `was_recovered() == false` is equally consistent with a truncated copy, a
/// failed transfer, tampering, and a ledger whose first writes were never durably
/// flushed; sled cannot tell those apart, so neither may the message. The
/// open-failed branch is looser still — it attaches to every `sled::Error`,
/// including a permission problem or flock contention that says nothing about the
/// archive.
///
/// NAMING the possibilities is allowed; ASSERTING one is not. A message that
/// lists what it cannot distinguish is telling the operator something true and
/// useful, and the current wording does exactly that. So the forbidden strings
/// are the assertive constructions only — the flat claim, and the disjunction
/// that closed the original overclaim — rather than the individual causes, which
/// legitimately appear in the "cannot be narrowed further" enumeration. A
/// forbidden-word list over the causes themselves was tried first and failed the
/// honest wording, which is the distinction this comment exists to keep.
fn assert_names_no_cause(flat: &str, text: &str) {
    for forbidden in ["or tampering", "damage to the database itself"] {
        assert!(
            !flat.contains(forbidden),
            "the refusal must not assert a cause it cannot establish, but it \
             contains {forbidden:?}:\n{text}"
        );
    }
}

/// The assertions both damage cases share.
fn assert_not_certified_as_empty(text: &str, status_ok: bool) {
    let flat = flattened(text);
    assert!(
        !status_ok,
        "a ledger that could not be read as written must not verify:\n{text}"
    );
    // THE defect, stated exactly: this pairing is the operator-visible false
    // conclusion #2732 exists to stop.
    assert!(
        !flat.contains("Ledger empty"),
        "an unreadable ledger must never be reported as an empty one:\n{text}"
    );
    assert!(
        !flat.contains("BACKUP VERIFICATION PASSED"),
        "and the run must not end in a success banner:\n{text}"
    );
    assert!(
        flat.contains("could not be read as written"),
        "the failure must name what actually happened — the database was replaced, \
         not read:\n{text}"
    );
    assert!(
        flat.contains("Ledger verification was NOT performed"),
        "and must say the requested verification did not happen, rather than \
         reporting a result it never obtained:\n{text}"
    );
    // Say only what the evidence supports, in two directions.
    //
    // Nothing in the archive records how many rows the ledger held, so the
    // message must not imply a count.
    assert!(
        flat.contains("not established by this archive"),
        "the message must not imply the original contents are known:\n{text}"
    );
    assert!(
        flat.contains("Do not rely on it as an empty ledger"),
        "and it must give the operator the one instruction the evidence does \
         support:\n{text}"
    );
    // Nor may it assert a CAUSE.
    assert!(
        flat.contains("cannot be narrowed further"),
        "the message must say plainly that the cause is not established:\n{text}"
    );
    assert_names_no_cause(&flat, text);
}

/// A ledger `db` truncated in transfer must not be certified as an empty ledger.
///
/// This is the mechanism icn#2732 names. `sled::open` has no read-only mode and
/// no fail-closed mode: given a `db` it cannot parse it allocates a fresh meta
/// page and hands back an EMPTY database. The journal scan then honestly finds
/// zero rows, and the verifier — faithfully reporting what the layer below gave
/// it — printed `✓ Ledger empty (no entries)` and `✓ BACKUP VERIFICATION PASSED`
/// for a backup that in fact held journal rows nobody could read.
///
/// Against `main` before the fix this test fails on the `Ledger empty` assertion
/// with exit status 0, which is the discrimination it is here to make.
#[test]
fn a_truncated_ledger_database_is_not_reported_as_an_empty_one() {
    let dir = TempDir::new().unwrap();
    let archive = a_backup_whose_ledger_was_damaged_before_it_was_taken(&dir, "truncated", |lp| {
        // An interrupted copy: the file is still there, and holds nothing.
        std::fs::write(lp.join("db"), b"").expect("fixture: could not truncate db");
    });

    let out = verify(&archive, true);
    assert_not_certified_as_empty(&combined(&out), out.status.success());
}

/// The same conclusion must not survive same-length damage either.
///
/// Truncation is one shape of a partial write; a `db` overwritten in place with
/// zeroes is another, and it defeats any check that reasons about file SIZE
/// rather than about whether the database was recovered. Both must fail, and for
/// the same stated reason.
#[test]
fn a_ledger_database_overwritten_in_place_is_not_reported_as_an_empty_one() {
    let dir = TempDir::new().unwrap();
    let archive = a_backup_whose_ledger_was_damaged_before_it_was_taken(&dir, "zeroed", |lp| {
        let len = std::fs::metadata(lp.join("db"))
            .expect("fixture: could not stat db")
            .len() as usize;
        std::fs::write(lp.join("db"), vec![0u8; len]).expect("fixture: could not zero db");
    });

    let out = verify(&archive, true);
    assert_not_certified_as_empty(&combined(&out), out.status.success());
}

/// The signal exists only at the FIRST open, and that is why the check is placed
/// where it is.
///
/// `Db::was_recovered()` is false when sled had to allocate the meta page rather
/// than recover it — but the allocation is persisted, so the next open of the
/// same damaged directory legitimately recovers what the previous one created.
/// `enforce_n2a_gate` opens every sled root it finds. A recovery check placed
/// after the gate would therefore read `true` on a corrupt ledger and certify it,
/// which is the same class of ordering bug #2717 fixed for the presence check.
///
/// This test pins the property the ordering depends on, so that if a future sled
/// upgrade makes the signal survive (or stop being produced at all) this fails
/// rather than the ordering silently becoming decorative.
#[test]
fn sleds_recovery_signal_is_destroyed_by_the_first_open() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    init_identity(&data_dir);
    seed_ledger(&data_dir, &[("hours", 60, 0), ("hours", 0, 60)]);
    let lp = ledger_dir(&data_dir);
    std::fs::write(lp.join("db"), b"").expect("fixture: could not truncate db");

    let signal = |p: &Path| {
        let store = SledStore::open(p).expect("could not open ledger");
        let recovered = store.db().was_recovered();
        let rows = store
            .scan(b"ledger:journal:")
            .expect("could not scan")
            .len();
        (recovered, rows)
    };

    let (first, rows_first) = signal(&lp);
    let (second, _) = signal(&lp);

    assert!(
        !first,
        "sled must report a damaged database as NOT recovered on the first open, \
         or there is no mechanism to place before the gate"
    );
    assert_eq!(
        rows_first, 0,
        "and the fresh database it substituted must scan as empty — that is the \
         false conclusion being prevented"
    );
    assert!(
        second,
        "the second open must report `recovered`, because the first open \
         PERSISTED the meta page it allocated. This is why the check cannot run \
         after `enforce_n2a_gate`, which opens every sled root it discovers"
    );
}

/// A readable ledger that is genuinely empty must keep its meaning.
///
/// The whole point of icn#2732 is to separate two facts, not to collapse them in
/// the other direction. A database sled really did recover, which really has no
/// journal rows, must still report `✓ Ledger empty` and still pass — otherwise
/// the fix has simply moved the false conclusion.
///
/// Complements `an_empty_ledger_does_not_claim_the_invariant_was_verified`, which
/// pins the same case's summary wording; this one pins that the new recovery
/// check does not reject it.
#[test]
fn a_genuinely_empty_readable_ledger_still_verifies() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("backup.tar");

    init_identity(&data_dir);
    let path = ledger_dir(&data_dir);
    std::fs::create_dir_all(&path).unwrap();
    let store = SledStore::open(&path).unwrap();
    store.db().flush().unwrap();
    drop(store);
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);
    assert!(
        out.status.success(),
        "an honestly empty ledger must still verify — the recovery check must not \
         reject a database sled actually recovered:\n{text}"
    );
    assert!(
        flattened(&text).contains("Ledger empty (no entries)"),
        "and it must still be reported as empty:\n{text}"
    );
}

/// A restarted node's ledger takes the OTHER refusal branch, and must also refuse.
///
/// The two damage tests above build a ledger that has been opened exactly once —
/// `seed_ledger` opens a fresh directory, writes, flushes and drops. sled writes
/// a `snap.*` only when a later open advances the stable LSN, so a first-ever
/// open leaves `conf`, `db` and `blobs` and no snapshot. That is the shape of a
/// node that has never restarted.
///
/// A real node's ledger carries a `snap.*` after its second start, and with a
/// snapshot present the same truncation makes `sled::open` return `Corruption`
/// instead of silently recovering — so production damage reaches
/// `assert_ledger_recovered_as_written`'s OPEN-FAILED branch, not its
/// `was_recovered()` branch. Both are refusals, but only one of them was covered,
/// and "a corrupt ledger is never reported as empty" is a claim about both.
///
/// This test reopens the seeded ledger once before damaging it, which is the
/// cheapest faithful stand-in for a restart.
#[test]
fn a_restarted_nodes_ledger_damaged_the_same_way_is_also_refused() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path().join("data");
    let archive = dir.path().join("restarted.tar");

    init_identity(&data_dir);
    seed_ledger(&data_dir, &[("hours", 60, 0), ("hours", 0, 60)]);

    // The "restart": a second open, which is what writes the snapshot.
    {
        let store = SledStore::open(ledger_dir(&data_dir)).expect("fixture: could not reopen");
        store.db().flush().expect("fixture: could not flush");
    }
    let snapshots: Vec<_> = std::fs::read_dir(ledger_dir(&data_dir))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("snap."))
        .collect();
    assert!(
        !snapshots.is_empty(),
        "fixture: a restarted ledger must carry a snapshot, or this test builds \
         the same shape as the two above and covers nothing new"
    );

    std::fs::write(ledger_dir(&data_dir).join("db"), b"").expect("fixture: could not truncate db");
    make_backup(&data_dir, &archive);

    let out = verify(&archive, true);
    let text = combined(&out);
    let flat = flattened(&text);

    assert!(
        !out.status.success(),
        "a restarted node's damaged ledger must not verify:\n{text}"
    );
    // The claim under test is the same one, on the branch the other tests miss.
    assert!(
        !flat.contains("Ledger empty"),
        "an unreadable ledger must never be reported as an empty one, on either \
         refusal branch:\n{text}"
    );
    assert!(
        !flat.contains("BACKUP VERIFICATION PASSED"),
        "and the run must not end in a success banner:\n{text}"
    );
    assert!(
        flat.contains("Ledger verification was NOT performed"),
        "and it must say the requested verification did not happen:\n{text}"
    );
    // This shape reaches the open-failed branch, whose wording differs from the
    // recovery branch. Asserting the distinct wording is what proves the two are
    // actually different code paths rather than one path reached twice.
    assert!(
        flat.contains("could not be opened"),
        "a snapshot-bearing ledger fails at OPEN, so the message must be the \
         open-failed one rather than the recovery one:\n{text}"
    );
    // This branch attaches to every `sled::Error`, not only corruption, so it may
    // name a cause even less than the recovery branch may.
    assert_names_no_cause(&flat, &text);
}

/// A crafted archive must not make verification write outside what it extracted.
///
/// The archive is untrusted input, so a `store/ledger` that is a *symlink* to a
/// sled database elsewhere on the machine has to be refused before anything
/// follows it. Every gate in front of the ledger open follows symlinks or ignores
/// them:
///
/// - `calculate_dir_checksum` walks with `follow_links(false)` and hashes only
///   `is_file()` entries, so a symlink is skipped and the checksum still matches;
/// - `Path::exists` and `Path::is_file` in the presence check follow it, and find
///   a complete `conf`/`db` pair;
/// - `sled::open` follows it too, and **writes** — it grows the backing file, may
///   write a snapshot, and rewrites the configuration.
///
/// `enforce_n2a_gate` does refuse symlinks (`find_sled_roots` uses `file_type()`,
/// which does not follow, and errors rather than skipping). That was sufficient
/// while the gate held the first open of the extracted tree. icn#2732 moved an
/// open in front of the gate — it has to, or sled's recovery signal is already
/// gone — and that reordering stepped the new open past the gate's guard.
///
/// Measured with the guard removed: the external database's `db` was rewritten
/// and a `snap.*` was created, from nothing but an operator running
/// `verify-backup` on a hostile file. The refusal must therefore come before the
/// open, not from the gate after it.
///
/// The assertion is on the VICTIM, not on the exit status. The command already
/// failed in the end either way — the gate caught it — so a status assertion
/// passes whether or not the escape happened. What distinguishes the two is
/// whether bytes outside the extraction root changed.
#[test]
fn a_symlinked_ledger_cannot_make_verification_write_outside_the_archive() {
    crafted_symlink_archive_must_not_touch_the_victim(SymlinkShape::WholeDirectory);
}

/// The same escape one level down, which the first containment fix did not close.
///
/// An archive can keep `store/ledger` a genuinely local directory and make its
/// CHILDREN links instead. A path-only containment check walks to the directory
/// and stops, so it sees nothing wrong — but the presence check then follows
/// `conf` and `db` with `is_file()`, and `sled::open` opens exactly those paths
/// and writes to them.
///
/// Reproduced against the path-only check: the victim's `db` was rewritten
/// through a symlinked child while the ledger directory itself was local. sled
/// decides which files under its directory to open, so containment walks the
/// whole subtree rather than guessing at a list of names.
#[test]
fn a_symlinked_ledger_child_cannot_make_verification_write_outside_the_archive() {
    crafted_symlink_archive_must_not_touch_the_victim(SymlinkShape::ChildrenOnly);
}

/// Where the crafted archive puts its link.
enum SymlinkShape {
    /// `store/ledger` is itself a link to the victim directory.
    WholeDirectory,
    /// `store/ledger` is a real directory whose `conf`/`db` link to the victim's.
    ChildrenOnly,
}

fn crafted_symlink_archive_must_not_touch_the_victim(shape: SymlinkShape) {
    let dir = TempDir::new().unwrap();

    // A real sled database that has nothing to do with any backup.
    let victim = dir.path().join("victim-ledger");
    std::fs::create_dir_all(&victim).unwrap();
    {
        let store = SledStore::open(&victim).expect("fixture: could not create the victim");
        store
            .put(b"ledger:journal:victim", b"{}")
            .expect("fixture: could not write to the victim");
        store
            .db()
            .flush()
            .expect("fixture: could not flush the victim");
    }
    let victim_before = tree_identity(&victim);
    assert!(
        !victim_before.is_empty(),
        "fixture: the victim must hold files, or 'unchanged' proves nothing"
    );

    // A legitimate ledger-less backup, so `backup_metadata.json` carries a real
    // checksum that the crafted tree below genuinely satisfies.
    let data_dir = dir.path().join("data");
    init_identity(&data_dir);
    let honest = dir.path().join("honest.tar");
    make_backup(&data_dir, &honest);

    let stage = dir.path().join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    tar::Archive::new(std::fs::File::open(&honest).unwrap())
        .unpack(&stage)
        .expect("fixture: could not extract the honest archive");

    // The craft: the same payload, plus a symlink where the ledger belongs.
    let crafted = dir.path().join("crafted.tar");
    {
        let mut builder = tar::Builder::new(std::fs::File::create(&crafted).unwrap());
        builder.follow_symlinks(false);
        for entry in walkdir::WalkDir::new(&stage)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let rel = entry.path().strip_prefix(&stage).unwrap();
                builder
                    .append_path_with_name(entry.path(), rel)
                    .expect("fixture: could not append file");
            }
        }
        match shape {
            SymlinkShape::WholeDirectory => {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_mode(0o777);
                header.set_cksum();
                builder
                    .append_link(&mut header, "store/ledger", &victim)
                    .expect("fixture: could not append the symlink");
            }
            SymlinkShape::ChildrenOnly => {
                let mut dir_header = tar::Header::new_gnu();
                dir_header.set_entry_type(tar::EntryType::Directory);
                dir_header.set_size(0);
                dir_header.set_mode(0o755);
                dir_header.set_cksum();
                builder
                    .append_data(&mut dir_header, "store/ledger/", std::io::empty())
                    .expect("fixture: could not append the ledger directory");
                for marker in ["conf", "db"] {
                    let mut header = tar::Header::new_gnu();
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_size(0);
                    header.set_mode(0o777);
                    header.set_cksum();
                    builder
                        .append_link(
                            &mut header,
                            format!("store/ledger/{marker}"),
                            victim.join(marker),
                        )
                        .expect("fixture: could not append the child symlink");
                }
            }
        }
        builder
            .finish()
            .expect("fixture: could not finish the archive");
    }

    let out = verify(&crafted, true);
    let text = combined(&out);

    // The property. Everything else here is scaffolding for it.
    assert_eq!(
        tree_identity(&victim),
        victim_before,
        "verifying a crafted archive must not touch a database outside it:\n{text}"
    );

    assert!(
        !out.status.success(),
        "and the crafted archive must not verify:\n{text}"
    );
    let flat = flattened(&text);
    assert!(
        flat.contains("symbolic link"),
        "the refusal must name what was actually wrong with the archive, so an \
         operator is not sent looking for a principal collision:\n{text}"
    );
    assert!(
        flat.contains("nothing was opened"),
        "and must say that no database was opened, which is the property that \
         distinguishes this refusal from the gate catching it afterwards:\n{text}"
    );
}
