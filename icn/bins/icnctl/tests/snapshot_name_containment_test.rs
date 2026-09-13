//! An operator-supplied snapshot identifier must never name a path outside the
//! snapshot store (#2779).
//!
//! `icnctl snapshot delete` joined its `String` argument onto `<data_dir>/store`
//! and called `remove_file` on the result. `Path::join` consumes `..` components
//! and an absolute argument replaces the base outright, so the argument selected
//! *any* file the operator could unlink: the node keystore `identity.age`, the
//! `.icn-data-dir.lock` exclusion anchor, or a file outside the data root
//! entirely.
//!
//! **These tests drive the real `icnctl` binary.** A test that called
//! `SnapshotName::parse` directly would prove the parser rejects a string, not
//! that the CLI refuses the deletion — and the defect was never in a parser, it
//! was in the absence of one at the boundary. So each case runs the shipped
//! subcommand and then asserts on the *filesystem*: the target survives, byte
//! for byte, and the process exits non-zero.
//!
//! To confirm these discriminate, revert `SnapshotName` out of
//! `handle_snapshot_command` and re-run: every containment case fails, because
//! on the unfixed code the deletion succeeds and the CLI reports success.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// Run `icnctl -d <data_dir> snapshot delete <argument>`.
///
/// `argument` is passed with `--` ahead of it so that a leading-dash spelling
/// reaches the handler as a value rather than being parsed as a flag; the point
/// of these tests is what the *handler* does with a hostile string.
fn snapshot_delete(data_dir: &Path, argument: &str) -> Output {
    Command::new(icnctl_bin())
        .arg("-d")
        .arg(data_dir)
        .args(["snapshot", "delete", "--"])
        .arg(argument)
        .output()
        .expect("failed to run icnctl")
}

/// A data directory laid out the way a provisioned node's is: a `store/`
/// subdirectory holding snapshots, the keystore and the exclusion anchor beside
/// it at the data-dir root.
struct Fixture {
    _root: TempDir,
    data_dir: PathBuf,
    store_dir: PathBuf,
}

const KEYSTORE_BYTES: &[u8] = b"age-encrypted-keystore-fixture-not-a-real-key";
const LOCK_BYTES: &[u8] = b"";
const VALID_SNAPSHOT: &str = "state.snapshot.1700000000";

impl Fixture {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let data_dir = root.path().join("data");
        let store_dir = data_dir.join("store");
        std::fs::create_dir_all(&store_dir).unwrap();

        std::fs::write(data_dir.join("identity.age"), KEYSTORE_BYTES).unwrap();
        std::fs::write(data_dir.join(".icn-data-dir.lock"), LOCK_BYTES).unwrap();

        Self {
            _root: root,
            data_dir,
            store_dir,
        }
    }

    /// The directory *containing* the data dir, so a test can place a file that
    /// is outside the ICN data root altogether.
    fn outside_dir(&self) -> PathBuf {
        self.data_dir.parent().unwrap().join("outside")
    }

    fn write_snapshot(&self, name: &str) {
        std::fs::write(self.store_dir.join(name), b"{}").unwrap();
        std::fs::write(self.store_dir.join(format!("{name}.sha256")), b"deadbeef").unwrap();
    }
}

fn assert_refused(output: &Output, argument: &str) {
    assert!(
        !output.status.success(),
        "`snapshot delete {argument}` exited successfully; it must be refused.\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn traversal_to_the_keystore_is_refused_and_identity_age_survives() {
    let fixture = Fixture::new();
    let keystore = fixture.data_dir.join("identity.age");

    let output = snapshot_delete(&fixture.data_dir, "../identity.age");

    assert_refused(&output, "../identity.age");
    assert!(keystore.exists(), "identity.age was deleted");
    assert_eq!(
        std::fs::read(&keystore).unwrap(),
        KEYSTORE_BYTES,
        "identity.age was modified"
    );
}

#[test]
fn traversal_to_the_exclusion_anchor_is_refused_and_the_lock_survives() {
    let fixture = Fixture::new();
    let anchor = fixture.data_dir.join(".icn-data-dir.lock");

    let output = snapshot_delete(&fixture.data_dir, "../.icn-data-dir.lock");

    assert_refused(&output, "../.icn-data-dir.lock");
    assert!(
        anchor.exists(),
        ".icn-data-dir.lock was unlinked; the exclusion anchor's pathname is now \
         free for a fresh uncontended lock (#2758's mechanism, reached by unlink)"
    );
}

#[test]
fn escape_outside_the_data_root_is_refused_and_the_victim_survives() {
    let fixture = Fixture::new();
    let outside = fixture.outside_dir();
    std::fs::create_dir_all(&outside).unwrap();
    let victim = outside.join("victim");
    std::fs::write(&victim, b"unrelated file contents").unwrap();

    let output = snapshot_delete(&fixture.data_dir, "../../outside/victim");

    assert_refused(&output, "../../outside/victim");
    assert_eq!(
        std::fs::read(&victim).unwrap(),
        b"unrelated file contents",
        "a file outside the ICN data root was deleted or modified"
    );
}

#[test]
fn an_absolute_path_argument_is_refused() {
    let fixture = Fixture::new();
    let outside = fixture.outside_dir();
    std::fs::create_dir_all(&outside).unwrap();
    let victim = outside.join("absolute-victim");
    std::fs::write(&victim, b"absolute target").unwrap();

    // `Path::join` with an absolute argument discards the base entirely, so this
    // resolves to `victim` itself rather than to anything under `store/`.
    let output = snapshot_delete(&fixture.data_dir, victim.to_str().unwrap());

    assert_refused(&output, "<absolute path>");
    assert_eq!(
        std::fs::read(&victim).unwrap(),
        b"absolute target",
        "an absolute-path argument deleted its target"
    );
}

#[test]
fn dot_and_dotdot_components_are_refused() {
    let fixture = Fixture::new();
    fixture.write_snapshot(VALID_SNAPSHOT);

    // `./<name>` and `sub/../<name>` both resolve *inside* the store, so they are
    // not escapes — but they are not snapshot names either, and accepting them
    // would mean the handler is normalising paths rather than naming snapshots.
    for argument in [".", "..", "./state.snapshot.1700000000", "a/../.."] {
        let output = snapshot_delete(&fixture.data_dir, argument);
        assert_refused(&output, argument);
    }

    assert!(
        fixture.store_dir.join(VALID_SNAPSHOT).exists(),
        "a component-trick argument reached the real snapshot"
    );
}

/// Separator-bearing arguments are refused even when they name a file that
/// really is reachable under the store.
///
/// **Unix-only, and the gate is the fixture, not the rule.** The setup relies on
/// `\` being an ordinary filename byte so the targets can be created *inside*
/// `store/`; on Windows `Path::join` would treat it as a separator, resolve
/// outside the store, and the fixture would clobber the keystore or panic on a
/// directory it never created. The cross-platform guarantee — that both `/` and
/// `\` are refused on every target — is carried by
/// `icn-snapshot`'s `hostile_snapshot_names_are_refused`, which is pure string
/// validation and runs everywhere.
#[cfg(unix)]
#[test]
fn separator_bearing_names_are_refused_even_when_reachable() {
    let fixture = Fixture::new();

    // Created as real files inside the store — otherwise the unfixed handler
    // would refuse them merely because they do not exist, and the test would
    // pass without discriminating. Made reachable, they show the actual rule:
    // `snapshot delete` removes snapshots, not whatever file the argument
    // happens to reach.
    std::fs::create_dir_all(fixture.store_dir.join("sub")).unwrap();
    let reachable = [
        "..\\identity.age",
        "..\\..\\outside\\victim",
        "sub/state.snapshot.1700000000",
    ];
    for name in reachable {
        std::fs::write(fixture.store_dir.join(name), b"reachable").unwrap();
    }

    for argument in reachable {
        let output = snapshot_delete(&fixture.data_dir, argument);
        assert_refused(&output, argument);
        assert!(
            fixture.store_dir.join(argument).exists(),
            "`{argument}` was deleted; it is reachable under the store but is not \
             a snapshot name"
        );
    }

    assert!(fixture.data_dir.join("identity.age").exists());
}

#[test]
fn the_checksum_sidecar_cannot_escape_alongside_a_refused_snapshot() {
    let fixture = Fixture::new();
    let outside = fixture.outside_dir();
    std::fs::create_dir_all(&outside).unwrap();
    let victim = outside.join("victim");
    let victim_sidecar = outside.join("victim.sha256");
    std::fs::write(&victim, b"primary").unwrap();
    std::fs::write(&victim_sidecar, b"sidecar").unwrap();

    // On the unfixed handler the sidecar path was re-derived from the *raw*
    // argument (`format!("{snapshot}.sha256")`) independently of the primary
    // path, so it escaped by the same mechanism and was unlinked too.
    let output = snapshot_delete(&fixture.data_dir, "../../outside/victim");

    assert_refused(&output, "../../outside/victim");
    assert_eq!(std::fs::read(&victim).unwrap(), b"primary");
    assert_eq!(
        std::fs::read(&victim_sidecar).unwrap(),
        b"sidecar",
        "the checksum sidecar escaped the snapshot store"
    );
}

#[test]
fn a_valid_snapshot_is_still_deleted_with_its_checksum() {
    let fixture = Fixture::new();
    fixture.write_snapshot(VALID_SNAPSHOT);
    let snapshot = fixture.store_dir.join(VALID_SNAPSHOT);
    let sidecar = fixture.store_dir.join(format!("{VALID_SNAPSHOT}.sha256"));
    assert!(snapshot.exists() && sidecar.exists());

    let output = snapshot_delete(&fixture.data_dir, VALID_SNAPSHOT);

    assert!(
        output.status.success(),
        "deleting an ordinary snapshot failed.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!snapshot.exists(), "the snapshot was not deleted");
    assert!(!sidecar.exists(), "the checksum sidecar was not deleted");

    // Containment must not have cost the surrounding files.
    assert!(fixture.data_dir.join("identity.age").exists());
    assert!(fixture.data_dir.join(".icn-data-dir.lock").exists());
}

#[test]
fn the_primary_snapshot_is_still_a_valid_name() {
    let fixture = Fixture::new();
    fixture.write_snapshot("state.snapshot");

    let output = snapshot_delete(&fixture.data_dir, "state.snapshot");

    assert!(
        output.status.success(),
        "`state.snapshot` is the primary snapshot name and must stay deletable.\n\
         stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!fixture.store_dir.join("state.snapshot").exists());
}

/// `snapshot verify` reaches the filesystem through the same join, and although
/// it only reads, its error messages distinguished "no such file" from "file
/// present but unchecksummed". That is an existence oracle for paths outside the
/// store, so the sibling takes the same boundary.
#[test]
fn verify_does_not_stat_paths_outside_the_snapshot_store() {
    let fixture = Fixture::new();

    let output = Command::new(icnctl_bin())
        .arg("-d")
        .arg(&fixture.data_dir)
        .args(["snapshot", "verify", "--"])
        .arg("../identity.age")
        .output()
        .expect("failed to run icnctl");

    assert_refused(&output, "verify ../identity.age");

    // On the unfixed handler `identity.age` exists, so the reported failure was
    // "Checksum file for '../identity.age' does not exist (legacy snapshot?)" —
    // which confirms the file is there. The refusal must not depend on, or
    // reveal, whether the target exists.
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !combined.contains("legacy snapshot"),
        "verify probed a path outside the store and reported what it found there: {combined}"
    );
    assert!(fixture.data_dir.join("identity.age").exists());
}
