//! A keystore the CLI creates must be owner-only, whatever the operator's
//! umask happens to be (#2748).
//!
//! `AgeKeyStore` wrote keystores with a plain `std::fs::write`, which creates at
//! `0o666 & ~umask`. Under the common `umask 0002` that is `0664`: group- and
//! world-readable. The file is age-encrypted, so this is not plaintext key
//! exposure — it hands any local user the ciphertext and the scrypt salt, which
//! turns a weak or reused passphrase into an offline attack instead of an
//! online one.
//!
//! **The umask has to be real, and it has to belong to the child.** `umask(2)`
//! is process-global, so setting it inside the test binary would race every
//! other test in the same process. Instead the CLI is launched through
//! `sh -c 'umask 0002; exec ...'`, which puts the permissive umask in a
//! dedicated child and leaves the harness alone. That also makes this a test of
//! the shipped binary rather than of a library helper.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

const PASSPHRASE: &str = "icn-2748-keystore-mode-fixture-passphrase";

/// Run `icnctl` under a deliberately permissive umask.
///
/// `0002` is the umask that produced the `0664` reported in #2748, so a run of
/// the unfixed binary here reproduces the defect exactly.
fn icnctl_under_permissive_umask(data_dir: &Path, args: &[&str]) -> Output {
    let quoted: Vec<String> = args.iter().map(|a| format!("'{a}'")).collect();
    let script = format!(
        "umask 0002; exec '{}' -d '{}' {}",
        icnctl_bin().display(),
        data_dir.display(),
        quoted.join(" ")
    );

    Command::new("sh")
        .arg("-c")
        .arg(script)
        .env("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE)
        .output()
        .expect("failed to run icnctl")
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .unwrap_or_else(|e| panic!("{} should exist: {e}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn id_init_creates_identity_age_owner_only_under_a_permissive_umask() {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    let output = icnctl_under_permissive_umask(&data_dir, &["id", "init"]);
    assert!(
        output.status.success(),
        "`icnctl id init` failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let keystore = data_dir.join("identity.age");
    let mode = mode_of(&keystore);
    assert_eq!(
        mode, 0o600,
        "identity.age was created {mode:o} under umask 0002; keystore files must \
         be owner-only regardless of the operator's umask"
    );
}

/// The umask really is permissive in that child — otherwise the test above
/// could pass on unfixed code for the wrong reason.
///
/// This writes an ordinary file from the same `sh` invocation shape and asserts
/// it comes out `0664`, which is what `umask 0002` yields for a plain create.
/// If this ever stops being `0664`, the sibling test has lost its teeth.
#[test]
fn the_fixture_umask_is_actually_permissive() {
    let root = TempDir::new().unwrap();
    let probe = root.path().join("probe");

    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("umask 0002; : > '{}'", probe.display()))
        .status()
        .expect("failed to run sh");
    assert!(status.success());

    assert_eq!(
        mode_of(&probe),
        0o664,
        "the fixture's umask is not permissive, so the keystore assertion would \
         not discriminate"
    );
}
