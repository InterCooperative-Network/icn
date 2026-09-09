//! The daemon's half of the data-directory exclusion protocol (#2744).
//!
//! This lives in the `icnd` package on purpose. An equivalent test in `icnctl`
//! had to locate `icnd` by guessing a sibling path and skip when it was absent —
//! so `cargo test -p icnctl`, which is not obliged to build another package's
//! binary, could report success precisely because the subject under test did not
//! exist. Here `CARGO_BIN_EXE_icnd` is provided by Cargo for a binary it must
//! build, so there is nothing to skip.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn icnd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnd"))
}

/// A configuration the daemon *cannot* load.
///
/// That is the discriminator, not an accident. Asserting only that a held lock
/// makes `icnd` exit would prove it takes the lock *somewhere*, and a mutation
/// removing the pre-config acquisition survives that — the later acquisition
/// still refuses. With an unloadable configuration the two orderings answer
/// differently:
///
/// * lock first   -> the exclusion refusal
/// * config first -> a config load failure
fn write_unloadable_config(data_dir: &Path) -> PathBuf {
    let path = data_dir.join("icn.toml");
    std::fs::write(
        &path,
        "# deliberately not loadable by Config::from_file\ndata_dir = \"/nonexistent\"\n\
         [network]\nmdns_enabled = true\n",
    )
    .unwrap();
    path
}

fn run_icnd(config: &Path) -> (bool, String) {
    let out = std::process::Command::new(icnd_bin())
        .arg("--config")
        .arg(config)
        .output()
        .expect("the icnd binary must be runnable");
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// The daemon refuses while another ICN process owns the data root, and refuses
/// **before it consumes the configuration**.
#[test]
fn the_daemon_refuses_before_consuming_a_configuration_of_an_owned_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = write_unloadable_config(dir.path());

    let held = icn_core::DataDirLock::acquire(dir.path(), "a maintenance ceremony").unwrap();
    let (ok, text) = run_icnd(&config);
    drop(held);

    assert!(
        !ok,
        "the daemon must not start while the root is owned:\n{text}"
    );
    assert!(
        text.contains("already holds"),
        "it must refuse at the exclusion. A configuration error here would mean \
         the lock is taken after the read, which is too late to prevent a stale \
         consumption:\n{text}"
    );
    assert!(
        !text.contains("Failed to load config"),
        "and it must not have reached the configuration at all:\n{text}"
    );
}

/// Once the root is released, the same daemon invocation gets as far as the
/// configuration — proving the refusal above was the exclusion and not some
/// unrelated permanent failure.
#[test]
fn the_daemon_proceeds_to_the_configuration_once_the_root_is_free() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = write_unloadable_config(dir.path());

    let (ok, text) = run_icnd(&config);

    assert!(!ok, "this fixture's config is deliberately unloadable");
    assert!(
        text.contains("Failed to load config") || text.contains("missing field"),
        "with the root free the daemon must reach the configuration:\n{text}"
    );
    assert!(
        !text.contains("already holds"),
        "and must not report an exclusion that nobody holds:\n{text}"
    );
}

/// A symlinked configuration path must not let the daemon derive a different
/// exclusion identity from the one the ceremony owns.
///
/// Canonicalizing the *directory* does not cover this: the alias is in the file
/// component. `icnd --config /elsewhere/link.toml` pointing at
/// `<root>/icn.toml` would otherwise lock `/elsewhere`, contend with nobody, and
/// then follow the link and consume the very bytes a ceremony owning `<root>` is
/// about to replace.
#[test]
fn a_symlinked_config_path_still_resolves_to_the_managed_root() {
    let root = tempfile::TempDir::new().unwrap();
    let elsewhere = tempfile::TempDir::new().unwrap();
    let real_config = write_unloadable_config(root.path());

    let link = elsewhere.path().join("link.toml");
    std::os::unix::fs::symlink(&real_config, &link).unwrap();

    // A ceremony owns the REAL root.
    let held = icn_core::DataDirLock::acquire(root.path(), "a maintenance ceremony").unwrap();

    let (ok, text) = run_icnd(&link);
    drop(held);

    assert!(!ok, "the daemon must not start:\n{text}");
    assert!(
        text.contains("already holds"),
        "the daemon must contend with the lock on the root the config really \
         lives in, not with one derived from the alias path:\n{text}"
    );
    assert!(
        !text.contains("Failed to load config"),
        "and must refuse before following the link to read it:\n{text}"
    );
}
