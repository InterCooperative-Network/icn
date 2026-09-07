//! `init-coop`'s first run must be automatable through the environment (#2727).
//!
//! `icnctl init-coop` sourced its passphrase two different ways depending on a
//! condition the operator does not control. The **existing-identity** branch
//! called `read_passphrase`, which honours `ICN_KEYSTORE_PASSPHRASE` (then the
//! legacy `ICN_PASSPHRASE`) before prompting. The **new-identity** branch —
//! the one that runs on the machine `init-coop` exists to set up — called
//! `rpassword::read_password()` directly, twice, so it ignored the environment
//! entirely and demanded a TTY:
//!
//! ```text
//! Step 1: Creating new identity
//! Choose a passphrase: Error: No such device or address (os error 6)
//! ```
//!
//! That made the documented first-run wizard the one `icnctl` path that could
//! not be scripted, blocking unattended provisioning and any end-to-end test of
//! the wizard's later steps.
//!
//! **Why these tests drive the real binary.** The defect is that a *process*
//! with no controlling terminal cannot get past step 1. An in-process unit test
//! calling a helper cannot observe that: it would have to fake the very thing
//! under test. So each test spawns the real `icnctl`, gives it a genuinely
//! fresh data directory with no keystore, closes stdin, and asserts on what the
//! process did.
//!
//! **Why the obvious fix is dangerous, and what pins it here.** Replacing the
//! prompt block with `confirm_passphrase()` fixes automation but silently
//! deletes the branch's inline `passphrase.len() < 8` rejection, because
//! `confirm_passphrase()` carries no minimum-length rule of its own (`id init`
//! deliberately has none). `a_short_environment_passphrase_is_rejected_on_first_run`
//! is the test that fails if that check is dropped, and it drives the
//! environment arm specifically — the arm a length rule enforced only around
//! the interactive prompt would miss.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// Where `init-coop` writes the keystore it creates.
///
/// Spelled out rather than resolved through the binary's own helper on purpose:
/// a test that asked the code under test where it put the file would agree with
/// it by construction, including when both are wrong.
fn keystore_path(data_dir: &Path) -> PathBuf {
    data_dir.join("identity.age")
}

/// At least 8 bytes, so it clears the minimum-length rule under test.
const PASSPHRASE: &str = "icn-2727-first-run-passphrase";

/// The legacy variable's value, deliberately different from `PASSPHRASE` so a
/// precedence claim can be checked by which one actually unlocks the keystore.
const LEGACY_PASSPHRASE: &str = "icn-2727-legacy-passphrase";

/// Seven bytes: one short of the branch's `len() < 8` rejection.
const SHORT_PASSPHRASE: &str = "sevenby";

/// An `init-coop` invocation with every ambient influence neutralised.
///
/// Three separate hazards are closed here, and none of them is tidiness:
///
/// * **stdin is closed** (`Stdio::null`). The claim under test is "this works
///   with no TTY". A test that inherited the developer's terminal would pass on
///   the defect whenever it was run by hand.
/// * **The gateway is pointed at a port nothing can serve, and `ICN_TOKEN` is
///   removed.** Step 4 probes `$ICN_GATEWAY/v1/health` (default
///   `localhost:8080`) and, if a token is present, POSTs a governance domain to
///   it. Without this a developer or runner with a live gateway would have this
///   test create real governance state on it. This is the pattern established
///   for this path in #2726, and it is a safety boundary, not cleanup.
/// * **Both passphrase variables are cleared before the caller sets any.** The
///   tests that assert on the *absence* of a passphrase are only meaningful if
///   the ambient environment cannot supply one.
///
/// `ICN_LOCALE` is pinned so a developer's locale cannot change the strings the
/// assertions read.
fn init_coop_command(data_dir: &Path) -> Command {
    let mut cmd = Command::new(icnctl_bin());
    cmd.stdin(Stdio::null())
        .env_remove("ICN_KEYSTORE_PASSPHRASE")
        .env_remove("ICN_PASSPHRASE")
        .env_remove("ICN_TOKEN")
        .env("ICN_GATEWAY", "http://127.0.0.1:1")
        .env("ICN_LOCALE", "en")
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "init-coop",
            "--name",
            "First Run Coop",
            "--yes",
            "--no-start",
        ]);
    cmd
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Guard shared by every test here: prove the run really took the *new
/// identity* branch.
///
/// Without this, a run that silently found an existing keystore would exercise
/// the already-working `read_passphrase` branch and pass while proving nothing
/// about the defect.
fn assert_took_new_identity_branch(text: &str) {
    assert!(
        text.contains("Step 1: Creating new identity"),
        "these tests are only about the new-identity branch; this run did not \
         take it:\n{text}"
    );
}

/// C6 + C1: the whole defect, end to end.
///
/// Fails on the pre-fix baseline at `Choose a passphrase:` with
/// `No such device or address (os error 6)`, because the branch never consults
/// the environment.
#[test]
fn init_coop_completes_first_run_with_only_the_keystore_passphrase_in_the_environment() {
    let dir = TempDir::new().unwrap();
    assert!(
        !keystore_path(dir.path()).exists(),
        "fixture must start with no identity, or it tests the wrong branch"
    );

    let out = init_coop_command(dir.path())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .output()
        .unwrap();
    let text = combined(&out);

    assert_took_new_identity_branch(&text);
    assert!(
        out.status.success(),
        "first-run `init-coop` must complete with the passphrase supplied \
         through ICN_KEYSTORE_PASSPHRASE and no TTY:\n{text}"
    );
    assert!(
        keystore_path(dir.path()).exists(),
        "the wizard reported success but wrote no keystore at {}:\n{text}",
        keystore_path(dir.path()).display()
    );
}

/// C2, on the arm that actually loses the rule.
///
/// The minimum-length rejection lives inline in the branch being replaced, not
/// in `confirm_passphrase()`. A fix that swaps the block for the helper and
/// stops there deletes it, and a short *environment* value would then create a
/// keystore. This test is what makes that regression visible.
#[test]
fn a_short_environment_passphrase_is_rejected_on_first_run() {
    let dir = TempDir::new().unwrap();

    let out = init_coop_command(dir.path())
        .env("ICN_KEYSTORE_PASSPHRASE", SHORT_PASSPHRASE)
        .output()
        .unwrap();
    let text = combined(&out);

    assert_took_new_identity_branch(&text);
    assert!(
        !out.status.success(),
        "a {}-byte passphrase must be rejected, not accepted because the \
         confirmation step was non-interactive:\n{text}",
        SHORT_PASSPHRASE.len()
    );
    assert!(
        text.contains("at least 8 characters"),
        "the failure must be the minimum-length rule, not an incidental error \
         such as a missing TTY — otherwise this test would pass on a build \
         that had dropped the rule entirely:\n{text}"
    );
    assert!(
        !keystore_path(dir.path()).exists(),
        "a rejected passphrase must not leave a keystore behind at {}:\n{text}",
        keystore_path(dir.path()).display()
    );
}

/// C1, legacy arm: `ICN_PASSPHRASE` keeps working on its own.
#[test]
fn the_legacy_passphrase_variable_still_drives_first_run() {
    let dir = TempDir::new().unwrap();

    let out = init_coop_command(dir.path())
        .env("ICN_PASSPHRASE", LEGACY_PASSPHRASE)
        .output()
        .unwrap();
    let text = combined(&out);

    assert_took_new_identity_branch(&text);
    assert!(
        out.status.success(),
        "the legacy ICN_PASSPHRASE must still drive an unattended first run:\n{text}"
    );
    assert!(keystore_path(dir.path()).exists(), "no keystore:\n{text}");
}

/// C1, precedence: `ICN_KEYSTORE_PASSPHRASE` wins over the legacy variable.
///
/// Checked by *which value opens the keystore afterwards*, not by reading the
/// helper's source. The second run takes the existing-identity branch, whose
/// sourcing was never in doubt, so it is a fair oracle for what the first run
/// actually used.
#[test]
fn the_keystore_passphrase_variable_takes_precedence_over_the_legacy_one() {
    let dir = TempDir::new().unwrap();

    let first = init_coop_command(dir.path())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .env("ICN_PASSPHRASE", LEGACY_PASSPHRASE)
        .output()
        .unwrap();
    let text = combined(&first);
    assert_took_new_identity_branch(&text);
    assert!(first.status.success(), "first run failed:\n{text}");

    let with_preferred = init_coop_command(dir.path())
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .output()
        .unwrap();
    assert!(
        combined(&with_preferred).contains("Step 1: Using existing identity"),
        "the second run should have found the keystore the first one wrote"
    );
    assert!(
        with_preferred.status.success(),
        "the keystore must open with the ICN_KEYSTORE_PASSPHRASE value, which \
         is the one that should have created it:\n{}",
        combined(&with_preferred)
    );

    let with_legacy = init_coop_command(dir.path())
        .env("ICN_PASSPHRASE", LEGACY_PASSPHRASE)
        .output()
        .unwrap();
    assert!(
        !with_legacy.status.success(),
        "the legacy value must NOT open a keystore created while \
         ICN_KEYSTORE_PASSPHRASE was set — if it does, precedence inverted:\n{}",
        combined(&with_legacy)
    );
}

/// C5: automation must not relax the human path.
///
/// With no passphrase in the environment the wizard must still try to obtain
/// one interactively and fail when it cannot, rather than inventing a default,
/// accepting an empty value, or skipping keystore creation. `rpassword` reads
/// the terminal directly, so a test process without one cannot drive the
/// prompt-and-confirm exchange itself; what this pins is that the interactive
/// path is still *taken*, and that no keystore is ever created without a
/// passphrase.
#[test]
fn without_a_passphrase_in_the_environment_first_run_still_requires_interactive_entry() {
    let dir = TempDir::new().unwrap();

    let out = init_coop_command(dir.path()).output().unwrap();
    let text = combined(&out);

    assert_took_new_identity_branch(&text);
    assert!(
        !out.status.success(),
        "with no passphrase available and no terminal, the wizard must fail \
         rather than proceed with an unspecified passphrase:\n{text}"
    );
    assert!(
        !keystore_path(dir.path()).exists(),
        "no keystore may be created when no passphrase was ever supplied; \
         found one at {}:\n{text}",
        keystore_path(dir.path()).display()
    );
}
