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
    assert!(text.contains("not the same objects"), "{text}");
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
        "icn-restore-incomplete v1\nphase moving\narchive {}\nmove-aside {}\nentry {} {} {birth} {}\n",
        hex(b"/srv/backup.tar"),
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
