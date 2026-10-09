//! A restore that does not finish must leave its data root refused to every
//! participant until it is recovered -- and recovery must put back exactly what
//! was there, verify it, and only then clear the marker. Through the real
//! binary.
//!
//! The failure used here is deterministic: an archive whose recorded checksum
//! is wrong, so the restore moves the old contents aside, extracts, and then
//! fails verification -- the longest window, and the one that predates the
//! move-aside design. A restore killed mid-move or mid-extraction leaves the
//! same marker (that is exercised with injected SIGKILLs outside this suite).
#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg(unix)]

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const MARKER: &str = ".icn-restore-incomplete";

fn icnctl(data_dir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_icnctl"));
    for (key, _) in std::env::vars() {
        if key.starts_with("ICN_DEV_") {
            cmd.env_remove(&key);
        }
    }
    cmd.env("RUST_LOG", "off")
        .env("ICN_GATEWAY", "http://127.0.0.1:1")
        .env_remove("ICN_TOKEN")
        .arg("--data-dir")
        .arg(data_dir);
    cmd
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn seeded_root(root: &Path, marker: &str) {
    std::fs::create_dir_all(root.join("store")).unwrap();
    std::fs::write(root.join("icn.toml"), b"# fixture\n").unwrap();
    std::fs::write(root.join("store").join("marker"), marker).unwrap();
}

fn archive_from(source: &Path, output: &Path) {
    let out = icnctl(source).arg("backup").arg(output).output().unwrap();
    assert!(out.status.success(), "backup failed: {}", combined(&out));
}

/// An archive laid out as `icnctl backup` lays one out, but with a recorded
/// checksum that cannot match: the restore gets all the way through extraction
/// and then fails.
fn archive_with_a_wrong_checksum(source: &Path, output: &Path) {
    let mut builder = tar::Builder::new(std::fs::File::create(output).unwrap());
    builder.append_dir_all(".", source).unwrap();
    let metadata = serde_json::json!({
        "icn_version": "test",
        "created_at": 1_700_000_000u64,
        "checksum": "0".repeat(64),
    })
    .to_string();
    let mut header = tar::Header::new_gnu();
    header.set_size(metadata.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, "backup_metadata.json", metadata.as_bytes())
        .unwrap();
    builder.finish().unwrap();
}

/// Names of a root's entries other than its lock files, sorted.
fn contents(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".icn-data-dir.lock" && n != ".icn-config.lock")
        .collect();
    v.sort();
    v
}

fn identity(path: &Path) -> (u64, u64) {
    let m = std::fs::symlink_metadata(path).unwrap();
    (m.dev(), m.ino())
}

fn move_aside_dir(parent: &Path) -> PathBuf {
    std::fs::read_dir(parent)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("icn.backup-"))
        })
        .expect("the restore moved the old contents aside")
}

/// A live root with an archive whose restore fails after the move: returns the
/// scratch directory, the root, and the identities of what it held before.
/// What a root held before a restore: each entry's name and (dev, ino).
type Held = Vec<(String, (u64, u64))>;

fn a_root_whose_restore_did_not_finish() -> (TempDir, PathBuf, Held) {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    let bad = scratch.path().join("bad.tar");
    archive_with_a_wrong_checksum(&source, &bad);

    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let before: Held = contents(&root)
        .into_iter()
        .map(|n| {
            let id = identity(&root.join(&n));
            (n, id)
        })
        .collect();

    let out = icnctl(&root)
        .arg("restore")
        .arg(&bad)
        .arg("--force")
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "the restore must fail: {text}");
    assert!(text.contains("Checksum mismatch"), "{text}");
    assert!(
        text.contains("--recover-incomplete"),
        "the failure must say how to recover: {text}"
    );
    assert!(
        root.join(MARKER).exists(),
        "a restore that changed the root and did not finish must leave its marker"
    );
    (scratch, root, before)
}

#[test]
fn a_finished_restore_leaves_no_marker() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);
    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");

    let out = icnctl(&root)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(
        !root.join(MARKER).exists(),
        "a finished restore clears its marker"
    );
    assert_eq!(
        std::fs::read(root.join("store").join("marker")).unwrap(),
        b"archived\n"
    );
}

/// While the marker is there, nothing opens the root's stores: not a store
/// opener, not a backup (which would archive an incomplete tree with a valid
/// checksum), not another restore.
#[test]
fn an_unfinished_restore_is_refused_by_every_participant() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    let good = scratch.path().join("good.tar");
    let source = scratch.path().join("source");
    archive_from(&source, &good);

    for (what, args) in [
        ("a store opener", vec!["coop", "entity-report", "--json"]),
        ("a backup", vec!["backup", "/dev/null"]),
    ] {
        let out = icnctl(&root).args(&args).output().unwrap();
        let text = combined(&out);
        assert!(!out.status.success(), "{what} must be refused: {text}");
        assert!(text.contains("did not finish"), "{what}: {text}");
    }
    let out = icnctl(&root)
        .arg("restore")
        .arg(&good)
        .arg("--force")
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(
        !out.status.success(),
        "a second restore must be refused: {text}"
    );
    assert!(text.contains("did not finish"), "{text}");
    assert!(root.join(MARKER).exists());
}

/// Recovery deletes nothing: while the root still holds what the archive put
/// there, it refuses and names those entries. Once the operator removes them it
/// puts the previous contents back -- the same objects, not copies -- verifies
/// the root, and clears the marker.
#[test]
fn recovery_puts_back_the_same_objects_verifies_and_only_then_clears() {
    let (_scratch, root, before) = a_root_whose_restore_did_not_finish();

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("were not there before"), "{text}");
    assert!(text.contains("deletes nothing"), "{text}");
    assert!(
        root.join(MARKER).exists(),
        "a refused recovery keeps the marker"
    );
    assert_eq!(
        std::fs::read(root.join("store").join("marker")).unwrap(),
        b"archived\n",
        "recovery must not have touched what the archive put there"
    );

    // The operator removes what the archive extracted.
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("Recovered"), "{text}");
    assert!(
        !root.join(MARKER).exists(),
        "a verified recovery clears the marker"
    );
    for (name, id) in &before {
        assert_eq!(
            identity(&root.join(name)),
            *id,
            "{name} must be the same object as before the restore, not a copy"
        );
    }
    assert_eq!(
        std::fs::read(root.join("store").join("marker")).unwrap(),
        b"pre-restore\n"
    );
    let out = icnctl(&root)
        .args(["coop", "entity-report", "--json"])
        .output()
        .unwrap();
    assert!(
        !combined(&out).contains("did not finish"),
        "the root is no longer refused for an incomplete restore: {}",
        combined(&out)
    );
}

/// An entry in the move-aside directory that is not the object the restore
/// inventoried is not "put back": recovery refuses and keeps the marker.
#[test]
fn recovery_refuses_an_entry_that_is_not_the_same_object() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    let aside = move_aside_dir(&scratch.path().join("parent"));
    std::fs::remove_file(aside.join("icn.toml")).unwrap();
    std::fs::write(aside.join("icn.toml"), b"# not the original\n").unwrap();

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("is not one of the entries it held"), "{text}");
    assert!(root.join(MARKER).exists(), "the marker stays");
}

/// Recovery reads where things are now, not how far anything got: an operator
/// (or an interrupted recovery) having moved part of it back already is fine.
#[test]
fn an_interrupted_recovery_converges_when_run_again() {
    let (scratch, root, before) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    let aside = move_aside_dir(&scratch.path().join("parent"));
    std::fs::rename(aside.join("store"), root.join("store")).unwrap();

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(!root.join(MARKER).exists());
    for (name, id) in &before {
        assert_eq!(identity(&root.join(name)), *id, "{name}");
    }
}

#[test]
fn recovery_with_nothing_incomplete_changes_nothing() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path().join("icn");
    seeded_root(&root, "live\n");
    let before = contents(&root);
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(combined(&out).contains("nothing to recover"));
    assert_eq!(contents(&root), before);
}

/// An archive that carries a marker was taken of a root whose restore had not
/// finished. It is refused before anything changes -- nothing moved aside, no
/// marker of this restore created -- and `verify-backup` fails it too.
#[test]
fn an_archive_carrying_a_marker_is_refused_before_anything_changes() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    std::fs::write(source.join(MARKER), b"icn-restore-incomplete v1\n").unwrap();
    let archive = scratch.path().join("tainted.tar");
    archive_with_a_wrong_checksum(&source, &archive);

    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let before = contents(&root);
    let out = icnctl(&root)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(
        text.contains("carries a restore-incomplete marker"),
        "{text}"
    );
    assert!(text.contains("Nothing has been changed"), "{text}");
    assert_eq!(contents(&root), before, "nothing may move");
    assert_eq!(
        contents(&scratch.path().join("parent")),
        vec!["icn".to_string()],
        "no move-aside directory"
    );

    let out = icnctl(&scratch.path().join("anything"))
        .arg("verify-backup")
        .arg(&archive)
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "verify-backup must fail it: {text}");
    assert!(
        text.contains("carries a restore-incomplete marker"),
        "{text}"
    );
}

/// The name check sees only an entry's own path. An archive can reach the
/// root's marker through a link it unpacked first (`l -> .`, then
/// `l/.icn-restore-incomplete`), and unpacking replaces what is there -- so a
/// forged record would take the place of this restore's own. Where the entry
/// would land is what is checked.
#[test]
fn an_archive_reaching_the_marker_through_a_link_is_refused() {
    let scratch = TempDir::new().unwrap();
    let archive = scratch.path().join("crafted.tar");
    {
        let mut builder = tar::Builder::new(std::fs::File::create(&archive).unwrap());
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        link.set_mode(0o777);
        link.set_link_name(".").unwrap();
        link.set_cksum();
        builder
            .append_data(&mut link, "l", std::io::empty())
            .unwrap();
        let forged = b"icn-restore-incomplete v1\nphase extracting\narchive 2f\nmove-aside -\n";
        let mut file = tar::Header::new_gnu();
        file.set_size(forged.len() as u64);
        file.set_mode(0o600);
        file.set_cksum();
        builder
            .append_data(&mut file, "l/.icn-restore-incomplete", &forged[..])
            .unwrap();
        let metadata = serde_json::json!({
            "icn_version": "test", "created_at": 1_700_000_000u64, "checksum": "0".repeat(64),
        })
        .to_string();
        let mut header = tar::Header::new_gnu();
        header.set_size(metadata.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "backup_metadata.json", metadata.as_bytes())
            .unwrap();
        builder.finish().unwrap();
    }

    let root = scratch.path().join("fresh");
    let out = icnctl(&root).arg("restore").arg(&archive).output().unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(
        text.contains("would land on this data directory's own"),
        "{text}"
    );
    let record = std::fs::read_to_string(root.join(MARKER)).unwrap();
    assert!(
        record.contains(&hex(archive.as_os_str().as_encoded_bytes())),
        "the marker must still be this restore's own record: {record}"
    );
}

/// A name the root held before, now a different object, is not "from the
/// archive": it may be the only copy of that entry, changed in place. Recovery
/// says to compare it, never to delete it, and keeps the marker.
#[test]
fn recovery_does_not_call_a_changed_original_foreign() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    let aside = move_aside_dir(&scratch.path().join("parent"));
    // Put icn.toml back, then rewrite it the way an editor does: a new file
    // renamed over the old one.
    std::fs::rename(aside.join("icn.toml"), root.join("icn.toml")).unwrap();
    std::fs::write(root.join("icn.toml.new"), b"# edited after the crash\n").unwrap();
    std::fs::rename(root.join("icn.toml.new"), root.join("icn.toml")).unwrap();

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("is not the same object"), "{text}");
    assert!(text.contains("do not delete them"), "{text}");
    assert!(!text.contains("came from the archive"), "{text}");
    assert!(root.join(MARKER).exists());
    assert_eq!(
        std::fs::read(root.join("icn.toml")).unwrap(),
        b"# edited after the crash\n"
    );
}

/// The owning-account rule holds only if the owner is the one who could have
/// written the record. A data directory every account can write breaks that,
/// and recovery refuses to follow its record.
#[test]
fn recovery_refuses_a_data_directory_every_account_can_write() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_scratch, root, _) = a_root_whose_restore_did_not_finish();
    let mode = std::fs::metadata(&root).unwrap().permissions().mode();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode | 0o002)).unwrap();
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("writable by every account"), "{text}");
    assert!(root.join(MARKER).exists());
}

/// Nothing writes a fresh identity into a half-restored root.
#[test]
fn identity_initialization_refuses_a_half_restored_root() {
    let (_scratch, root, _) = a_root_whose_restore_did_not_finish();
    let out = icnctl(&root)
        .args(["id", "init"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("did not finish"), "{text}");
    assert!(!root.join("identity.age").exists());
}

/// Recovery clears the marker only when the root holds *everything* it held
/// before. An entry that is gone -- deleted from the move-aside directory, say
/// -- is not "nothing left to put back": the root is incomplete, and the marker
/// must stay.
#[test]
fn recovery_refuses_to_clear_when_an_entry_is_missing() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    let aside = move_aside_dir(&scratch.path().join("parent"));
    std::fs::remove_dir_all(aside.join("store")).unwrap();

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("\"store\" is missing"), "{text}");
    assert!(root.join(MARKER).exists(), "the marker stays");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The checksum an archive's metadata declares, which a restore records.
fn declared_checksum(archive: &Path) -> String {
    use std::io::Read as _;
    let mut reader = tar::Archive::new(std::fs::File::open(archive).unwrap());
    for entry in reader.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry.path().unwrap() == Path::new("backup_metadata.json") {
            let mut json = String::new();
            entry.read_to_string(&mut json).unwrap();
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            return value["checksum"].as_str().unwrap().to_string();
        }
    }
    panic!("{} has no metadata", archive.display());
}

/// A record as an account that can write the data directory could forge one:
/// the real identities of files *elsewhere*, and a move-aside path pointing at
/// them -- so that following the record would move those files into the root.
fn forge_record(root: &Path, move_aside: &Path, victim: &Path) {
    use std::os::unix::ffi::OsStrExt as _;
    let meta = std::fs::symlink_metadata(victim).unwrap();
    let birth = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_else(|| "-".to_string());
    let record = format!(
        "icn-restore-incomplete v1\nphase moving\narchive {}\narchive-checksum {}\nmove-aside {}\n\
         entry {} {} {birth} {}\n",
        hex(b"/srv/backup.tar"),
        hex("0".repeat(64).as_bytes()),
        hex(move_aside.as_os_str().as_bytes()),
        meta.dev(),
        meta.ino(),
        hex(victim.file_name().unwrap().as_bytes()),
    );
    std::fs::write(root.join(MARKER), record).unwrap();
}

/// The marker is only as trustworthy as the directory it sits in. Recovery
/// follows it only into a move-aside directory restore itself would have made
/// -- `<root>.backup-<digits>` beside the resolved root, a real directory of
/// the root's account -- so a forged record cannot have it move files from
/// anywhere else into the data directory.
#[test]
fn recovery_refuses_a_move_aside_path_restore_would_not_have_made() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    let parent = scratch.path().join("parent").canonicalize().unwrap();

    let elsewhere = scratch
        .path()
        .join("elsewhere")
        .canonicalize()
        .ok()
        .unwrap_or_else(|| {
            std::fs::create_dir(scratch.path().join("elsewhere")).unwrap();
            scratch.path().join("elsewhere").canonicalize().unwrap()
        });
    std::fs::write(elsewhere.join("secret"), b"not the data directory's\n").unwrap();
    let beside_but_misnamed = parent.join("not-a-backup");
    std::fs::create_dir(&beside_but_misnamed).unwrap();
    std::fs::write(beside_but_misnamed.join("secret"), b"also not\n").unwrap();
    let linked = parent.join("icn.backup-424242");
    std::os::unix::fs::symlink(&elsewhere, &linked).unwrap();

    for (aside, victim, why) in [
        (
            elsewhere.clone(),
            elsewhere.join("secret"),
            "not beside the data directory",
        ),
        (
            beside_but_misnamed.clone(),
            beside_but_misnamed.join("secret"),
            "not the name restore gives it",
        ),
        (linked.clone(), elsewhere.join("secret"), "not a directory"),
    ] {
        forge_record(&root, &aside, &victim);
        let out = icnctl(&root)
            .args(["restore", "--recover-incomplete"])
            .output()
            .unwrap();
        let text = combined(&out);
        assert!(!out.status.success(), "{aside:?}: {text}");
        assert!(
            text.contains(why),
            "{aside:?} refused for this reason: {text}"
        );
        assert!(victim.exists(), "{victim:?} must not have been moved");
        assert!(
            !root.join("secret").exists(),
            "nothing may be moved into the root"
        );
        assert!(root.join(MARKER).exists(), "the marker stays");
    }
}

/// D2: a backup refused for an incomplete restore touches no destination --
/// it neither creates an absent one nor truncates one that is already there.
#[test]
fn a_refused_backup_leaves_every_destination_untouched() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    let absent = scratch.path().join("new-backup.tar");
    let existing = scratch.path().join("old-backup.tar");
    std::fs::write(&existing, b"an earlier backup\n").unwrap();
    for dest in [&absent, &existing] {
        let out = icnctl(&root).arg("backup").arg(dest).output().unwrap();
        let text = combined(&out);
        assert!(!out.status.success(), "{text}");
        assert!(text.contains("did not finish"), "{text}");
    }
    assert!(
        !absent.exists(),
        "an absent destination must not be created"
    );
    assert_eq!(std::fs::read(&existing).unwrap(), b"an earlier backup\n");
}

/// D4: recovery takes no archive and can never start a restore. It cannot be
/// combined with an archive or `--force`, and the refusal changes nothing.
#[test]
fn recovery_cannot_be_combined_with_an_archive_or_force() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);
    let root = scratch.path().join("fresh");

    for args in [
        vec!["restore", "--recover-incomplete", archive.to_str().unwrap()],
        vec!["restore", "--recover-incomplete", "--force"],
        vec!["restore"],
    ] {
        let out = icnctl(&root).args(&args).output().unwrap();
        assert!(
            !out.status.success(),
            "{args:?} must be rejected: {}",
            combined(&out)
        );
        assert!(
            !root.exists(),
            "{args:?} must not create or touch the data directory"
        );
    }
}

/// D1: the marker is found however the data directory is spelled.
#[test]
fn an_alias_of_a_root_whose_restore_did_not_finish_is_refused_too() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    let alias = scratch.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let out = icnctl(&alias)
        .args(["coop", "entity-report", "--json"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("did not finish"), "{text}");
}

/// D5 and interruption: putting an entry back links it at its old name before
/// unlinking it from the move-aside directory, so a recovery stopped between
/// the two leaves one object under two names. Run again, recovery sees the same
/// object, drops the redundant name, and finishes.
#[test]
fn a_put_back_interrupted_between_link_and_unlink_converges() {
    let (scratch, root, before) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    let aside = move_aside_dir(&scratch.path().join("parent"));
    std::fs::hard_link(aside.join("icn.toml"), root.join("icn.toml")).unwrap();

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(!root.join(MARKER).exists());
    assert!(
        !aside.join("icn.toml").exists(),
        "the redundant name is gone"
    );
    for (name, id) in &before {
        assert_eq!(identity(&root.join(name)), *id, "{name}");
    }
}

/// D3: a restore that finished can still leave its marker, if a crash comes
/// between the marker's removal and the flush that makes it stick. Recovery
/// must recognize the finished restore -- the root matches the archive exactly
/// -- and clear the marker, not walk the operator into undoing it.
#[test]
fn recovery_recognizes_a_restore_that_had_finished() {
    use std::os::unix::ffi::OsStrExt as _;
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);
    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let out = icnctl(&root)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    let aside = move_aside_dir(&scratch.path().join("parent"));

    // The marker as it stood before its removal was lost.
    let mut record = format!(
        "icn-restore-incomplete v1\nphase moving\narchive {}\narchive-checksum {}\n\
         move-aside {}\n",
        hex(archive.as_os_str().as_bytes()),
        hex(declared_checksum(&archive).as_bytes()),
        hex(aside.as_os_str().as_bytes())
    );
    for name in contents(&aside) {
        let meta = std::fs::symlink_metadata(aside.join(&name)).unwrap();
        let birth = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_else(|| "-".to_string());
        record.push_str(&format!(
            "entry {} {} {birth} {}\n",
            meta.dev(),
            meta.ino(),
            hex(name.as_bytes())
        ));
    }
    std::fs::write(root.join(MARKER), record).unwrap();
    let restored = contents(&root);

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("had finished"), "{text}");
    assert!(!root.join(MARKER).exists());
    let mut after = contents(&root);
    after.retain(|n| n != MARKER);
    let mut expected = restored.clone();
    expected.retain(|n| n != MARKER);
    assert_eq!(after, expected, "nothing in the restored root may move");
    assert_eq!(
        std::fs::read(root.join("store").join("marker")).unwrap(),
        b"archived\n"
    );
    assert!(
        aside.join("store").exists(),
        "the previous contents stay aside"
    );
}

/// An entry spelled with a leading `/` unpacks into the data directory just
/// as `./` and bare names do, so it must meet the same checks: an archive
/// carrying `/.icn-restore-incomplete` is refused before anything changes, and
/// `verify-backup` fails it.
#[test]
fn an_archive_spelling_the_marker_absolute_is_refused_before_anything_changes() {
    let scratch = TempDir::new().unwrap();
    let archive = scratch.path().join("absolute.tar");
    {
        let mut builder = tar::Builder::new(std::fs::File::create(&archive).unwrap());
        // Written raw: the builder's own path setter would refuse an absolute
        // path, and a crafted archive does not use it.
        let forged = b"icn-restore-incomplete v1\nphase extracting\narchive 2f\nmove-aside -\n";
        let mut header = tar::Header::new_old();
        let name = b"/.icn-restore-incomplete";
        header.as_old_mut().name[..name.len()].copy_from_slice(name);
        header.set_size(forged.len() as u64);
        header.set_mode(0o600);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append(&header, &forged[..]).unwrap();
        let metadata = serde_json::json!({
            "icn_version": "test", "created_at": 1_700_000_000u64, "checksum": "0".repeat(64),
        })
        .to_string();
        let mut header = tar::Header::new_gnu();
        header.set_size(metadata.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "backup_metadata.json", metadata.as_bytes())
            .unwrap();
        builder.finish().unwrap();
    }
    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let before = contents(&root);
    let out = icnctl(&root)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(
        text.contains("carries a restore-incomplete marker"),
        "{text}"
    );
    assert_eq!(contents(&root), before, "nothing may move");
    assert!(!root.join(MARKER).exists());
    let out = icnctl(&scratch.path().join("x"))
        .arg("verify-backup")
        .arg(&archive)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "verify-backup must fail it: {}",
        combined(&out)
    );
}

/// The record names the archive by its resolved path, so a recovery run from
/// another working directory finds the same file.
#[test]
fn the_record_names_the_archive_by_its_resolved_path() {
    use std::os::unix::ffi::OsStrExt as _;
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    archive_with_a_wrong_checksum(&source, &scratch.path().join("bad.tar"));
    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let out = icnctl(&root)
        .current_dir(scratch.path())
        .args(["restore", "bad.tar", "--force"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{}", combined(&out));
    let record = std::fs::read_to_string(root.join(MARKER)).unwrap();
    let resolved = scratch.path().join("bad.tar").canonicalize().unwrap();
    assert!(
        record.contains(&format!(
            "archive {}\n",
            hex(resolved.as_os_str().as_bytes())
        )),
        "{record}"
    );
    assert!(
        record.contains(&format!(
            "archive-checksum {}\n",
            hex(declared_checksum(&resolved).as_bytes())
        )),
        "the record names the backup, not just its path: {record}"
    );
}

/// A restore that finished, with its marker brought back as a crash between
/// the marker's unlink and its directory's flush would leave it. `ghost` adds
/// an inventory entry whose original is not in the move-aside directory, as if
/// the move had not completed.
fn a_finished_restore_with_its_marker_back(ghost: bool) -> (TempDir, PathBuf) {
    use std::os::unix::ffi::OsStrExt as _;
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);
    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let out = icnctl(&root)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    let aside = move_aside_dir(&scratch.path().join("parent"));
    let mut record = format!(
        "icn-restore-incomplete v1\nphase moving\narchive {}\narchive-checksum {}\n\
         move-aside {}\n",
        hex(archive.canonicalize().unwrap().as_os_str().as_bytes()),
        hex(declared_checksum(&archive).as_bytes()),
        hex(aside.as_os_str().as_bytes())
    );
    for name in contents(&aside) {
        let meta = std::fs::symlink_metadata(aside.join(&name)).unwrap();
        let birth = meta
            .created()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_else(|| "-".to_string());
        record.push_str(&format!(
            "entry {} {} {birth} {}\n",
            meta.dev(),
            meta.ino(),
            hex(name.as_bytes())
        ));
    }
    if ghost {
        record.push_str(&format!("entry 1 1 - {}\n", hex(b"ghost")));
    }
    std::fs::write(root.join(MARKER), record).unwrap();
    (scratch, root)
}

fn recovery_says_finished(root: &Path) -> bool {
    let out = icnctl(root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let finished = combined(&out).contains("had finished");
    assert_eq!(out.status.success(), finished, "{}", combined(&out));
    finished
}

/// "The restore had finished" clears the marker, so it is strict. A restore
/// that really finished is recognised; any one of these says no: a restored
/// file whose permissions are not the archive's, an entry the archive does not
/// have, or an inventoried original that is not in the move-aside directory.
#[test]
fn the_finished_restore_shortcut_is_strict() {
    use std::os::unix::fs::PermissionsExt as _;
    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    assert!(
        recovery_says_finished(&root),
        "a restore that finished is recognised"
    );
    assert!(!root.join(MARKER).exists());

    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    let file = root.join("icn.toml");
    let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode ^ 0o004)).unwrap();
    assert!(!recovery_says_finished(&root), "a permission that differs");
    assert!(root.join(MARKER).exists());

    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    std::fs::create_dir(root.join("store").join("extra")).unwrap();
    assert!(
        !recovery_says_finished(&root),
        "an entry the archive does not have"
    );
    assert!(root.join(MARKER).exists());

    let (_s, root) = a_finished_restore_with_its_marker_back(true);
    assert!(
        !recovery_says_finished(&root),
        "a move that had not completed"
    );
    assert!(root.join(MARKER).exists());

    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    let dir = root.join("store");
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode ^ 0o004)).unwrap();
    assert!(
        !recovery_says_finished(&root),
        "a directory permission that differs"
    );
    assert!(root.join(MARKER).exists());

    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    std::fs::write(root.join("store").join("marker"), b"archivex\n").unwrap();
    assert!(!recovery_says_finished(&root), "a file's contents differ");
    assert!(root.join(MARKER).exists());

    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    std::fs::remove_file(root.join("store").join("marker")).unwrap();
    assert!(
        !recovery_says_finished(&root),
        "an entry the archive has is missing"
    );
    assert!(root.join(MARKER).exists());

    // The root is exactly what the archive holds, but the record says the
    // restore was of a different backup: no evidence that it finished.
    let (_s, root) = a_finished_restore_with_its_marker_back(false);
    let record = std::fs::read_to_string(root.join(MARKER)).unwrap();
    let line = record
        .lines()
        .find(|l| l.starts_with("archive-checksum "))
        .unwrap()
        .to_string();
    let other = format!("archive-checksum {}", hex("1".repeat(64).as_bytes()));
    std::fs::write(root.join(MARKER), record.replace(&line, &other)).unwrap();
    assert!(
        !recovery_says_finished(&root),
        "a record naming a different backup"
    );
    assert!(root.join(MARKER).exists());
}

/// When the archive cannot be read, what it extracted may be the only copy
/// left: recovery says to move those entries somewhere safe, not to delete them.
#[test]
fn recovery_does_not_advise_deleting_what_only_a_missing_archive_held() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    std::fs::rename(
        scratch.path().join("bad.tar"),
        scratch.path().join("moved.tar"),
    )
    .unwrap();
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("cannot vouch for them"), "{text}");
    assert!(text.contains("do not delete them"), "{text}");
    assert!(!text.contains("can extract them again"), "{text}");
}

/// What the archive holds exactly, it can extract again; anything else may be
/// the only copy of what it holds. Recovery's advice is per entry, and by
/// contents: a directory the archive extracted, with a file written into it
/// since, is not "held by the archive".
#[test]
fn recovery_advises_removal_only_for_what_the_archive_holds() {
    let (_scratch, root, _) = a_root_whose_restore_did_not_finish();
    std::fs::write(root.join("stray"), b"written by something else\n").unwrap();
    std::fs::write(root.join("store").join("precious"), b"written since\n").unwrap();
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    let removable = text
        .lines()
        .find(|l| l.contains("can extract them again"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(removable.contains("\"icn.toml\""), "{text}");
    assert!(!removable.contains("store"), "{text}");
    assert!(!removable.contains("stray"), "{text}");
    let keep = text
        .lines()
        .find(|l| l.contains("do not delete them"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(keep.contains("\"stray\""), "{text}");
    assert!(keep.contains("\"store\""), "{text}");
    assert!(keep.contains("does not hold these exactly"), "{text}");
    assert!(root.join("stray").exists());
    assert!(root.join("store").join("precious").exists());
    assert!(root.join(MARKER).exists());
}

/// A declared checksum is not proof of contents: a different archive at the
/// path declaring the same checksum vouches only for what it actually holds,
/// byte for byte.
#[test]
fn recovery_compares_contents_not_the_declared_checksum() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    // The same declared checksum (this fixture's), a different store.
    let other = scratch.path().join("other");
    seeded_root(&other, "a later backup\n");
    archive_with_a_wrong_checksum(&other, &scratch.path().join("later.tar"));
    std::fs::rename(
        scratch.path().join("later.tar"),
        scratch.path().join("bad.tar"),
    )
    .unwrap();
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    let removable = text
        .lines()
        .find(|l| l.contains("can extract them again"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(
        removable.contains("\"icn.toml\""),
        "identical bytes: {text}"
    );
    assert!(!removable.contains("store"), "{text}");
    let keep = text
        .lines()
        .find(|l| l.contains("do not delete them"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(keep.contains("\"store\""), "{text}");
    assert_eq!(
        std::fs::read(root.join("store").join("marker")).unwrap(),
        b"archived\n"
    );
}

/// A file the finished-restore check cannot read cannot be shown to match: the
/// restore is not "finished", and recovery goes on to say what is there. (An
/// account that can read a mode-000 file -- root -- cannot set this up, so the
/// test says so and stops; CI's hosted runners are not root.)
#[test]
fn an_unreadable_file_is_not_a_finished_restore() {
    use std::io::Read as _;
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source, "archived\n");
    let good = scratch.path().join("good.tar");
    archive_from(&source, &good);
    // The same archive, with one file recorded as unreadable (mode 000). The
    // declared checksum covers contents, not permissions, so it still holds.
    let unreadable = scratch.path().join("unreadable.tar");
    {
        let mut reader = tar::Archive::new(std::fs::File::open(&good).unwrap());
        let mut builder = tar::Builder::new(std::fs::File::create(&unreadable).unwrap());
        for entry in reader.entries().unwrap() {
            let mut entry = entry.unwrap();
            let mut header = entry.header().clone();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            if entry.path().unwrap().ends_with("store/marker") {
                header.set_mode(0o000);
                header.set_cksum();
            }
            builder.append(&header, &data[..]).unwrap();
        }
        builder.finish().unwrap();
    }
    let root = scratch.path().join("parent").join("icn");
    seeded_root(&root, "pre-restore\n");
    let out = icnctl(&root)
        .arg("restore")
        .arg(&unreadable)
        .arg("--force")
        .output()
        .unwrap();
    if std::fs::File::open(root.join("store").join("marker")).is_ok() {
        eprintln!("skipped: this account can read a mode-000 file (root?)");
        return;
    }
    assert!(!out.status.success(), "{}", combined(&out));
    assert!(root.join(MARKER).exists());

    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(!text.contains("had finished"), "{text}");
    assert!(text.contains("were not there before"), "{text}");
    assert!(root.join(MARKER).exists());
}

/// A path names whatever is there now. A newer backup rotated into the
/// archive's name does not hold what this restore extracted, so it is no
/// reason to delete anything.
#[test]
fn recovery_does_not_advise_deleting_what_a_replaced_archive_held() {
    let (scratch, root, _) = a_root_whose_restore_did_not_finish();
    let other = scratch.path().join("other");
    seeded_root(&other, "a later backup\n");
    archive_from(&other, &scratch.path().join("later.tar"));
    std::fs::rename(
        scratch.path().join("later.tar"),
        scratch.path().join("bad.tar"),
    )
    .unwrap();
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    let text = combined(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("declares a different backup"), "{text}");
    assert!(text.contains("do not delete them"), "{text}");
    assert!(!text.contains("can extract them again"), "{text}");
    assert!(root.join(MARKER).exists());
}

/// The remaining writers into a data root refuse a half-restored one too.
#[test]
fn snapshots_and_device_enrollment_refuse_a_half_restored_root() {
    let (_scratch, root, _) = a_root_whose_restore_did_not_finish();
    for args in [
        vec!["snapshot", "create"],
        vec!["snapshot", "delete", "any"],
        vec!["snapshot", "cleanup"],
        vec!["device", "add", "laptop"],
    ] {
        let out = icnctl(&root)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let text = combined(&out);
        assert!(!out.status.success(), "{args:?}: {text}");
        assert!(text.contains("did not finish"), "{args:?}: {text}");
    }
}

/// Recovery does not depend on the lock file a restore left: removed by hand,
/// it is made again (by the directory's own account) and recovery proceeds.
#[test]
fn recovery_works_after_the_lock_file_was_removed_by_hand() {
    let (_scratch, root, before) = a_root_whose_restore_did_not_finish();
    for name in contents(&root) {
        if name != MARKER {
            let path = root.join(&name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path).unwrap();
            } else {
                std::fs::remove_file(&path).unwrap();
            }
        }
    }
    std::fs::remove_file(root.join(".icn-data-dir.lock")).unwrap();
    let out = icnctl(&root)
        .args(["restore", "--recover-incomplete"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(!root.join(MARKER).exists());
    for (name, id) in &before {
        assert_eq!(identity(&root.join(name)), *id, "{name}");
    }
}
