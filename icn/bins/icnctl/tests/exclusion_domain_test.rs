//! Local commands that touch a data root must be inside its exclusion domain
//! (icn#2758, icn#2759).
//!
//! #2749 introduced `DataDirLock` and the ceremony that depends on it. These
//! tests cover the two holes that were left outside it, through the **real
//! binary** and the real dispatch rather than through the handlers:
//!
//! * `icnctl restore --force` replaced a data root while another process held
//!   it, and renamed that root out from under the lock file — so the holder's
//!   `flock(2)` travelled into the backup directory while a fresh, unlocked
//!   file appeared at the stable pathname. Two valid exclusive locks, no
//!   contention (icn#2758).
//! * `coop`, `treasury` and `init-coop` crossed the N2-A gate and then opened
//!   the ceremony's own sled databases holding nothing. Sled's per-database
//!   lock was the only thing between them and a ceremony — and a ceremony
//!   deliberately drops its handles to re-read state, so there is an interval
//!   in which sled locks nothing (icn#2759).
//!
//! **What makes these evidence.** The holder is taken with the same
//! `DataDirLock` the daemon and the ceremony take, in this process, and the
//! command under test is a separate real `icnctl` invocation. A refusal
//! therefore comes from ICN's own exclusion protocol, not from `flock`
//! arranged to look like one.
//!
//! The negative control matters as much as the positives: `backup` reads the
//! same tree and is deliberately **not** brought into the domain, because
//! serializing every observer against every holder is the over-correction
//! #2749 had to undo twice.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn icnctl_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_icnctl"))
}

/// An `icnctl` invocation with every `ICN_DEV_*` authority shortcut removed, so
/// no dev bypass can be what makes a case pass.
fn icnctl(data_dir: &Path) -> Command {
    let mut cmd = Command::new(icnctl_bin());
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

/// A data root with enough in it to be worth protecting.
fn seeded_root(root: &Path) {
    std::fs::create_dir_all(root.join("store")).unwrap();
    std::fs::write(root.join("icn.toml"), b"# fixture\n").unwrap();
    std::fs::write(root.join("store").join("marker"), b"original\n").unwrap();
}

/// Produce a backup archive with the real `icnctl backup`.
fn archive_from(source: &Path, output: &Path) {
    let out = icnctl(source).arg("backup").arg(output).output().unwrap();
    assert!(out.status.success(), "backup failed: {}", combined(&out));
}

#[cfg(unix)]
fn inode_at(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}

/// The refusal every holder of this domain produces.
fn assert_refused_by_the_domain(out: &Output, what: &str) {
    let text = combined(out);
    assert!(
        !out.status.success(),
        "{what} must be refused while another process holds the data root, but it succeeded:\n{text}"
    );
    assert!(
        text.contains("already holds"),
        "{what} must be refused by the exclusion domain and say so; got:\n{text}"
    );
}

// ---------------------------------------------------------------------------
// icn#2758 — restore
// ---------------------------------------------------------------------------

/// `icnctl restore --force` must not replace a root another ICN process holds.
///
/// Reproduces the reported sequence with the real command: a genuine
/// `DataDirLock` holder, then `restore --force` against the same root.
#[test]
fn restore_force_is_refused_while_a_real_holder_has_the_data_root() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);

    let holder = icn_core::DataDirLock::acquire(&dest, "a running daemon").unwrap();
    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert_refused_by_the_domain(&out, "restore --force");

    // The refusal must come before anything is moved.
    assert_eq!(
        std::fs::read(dest.join("store").join("marker")).unwrap(),
        b"original\n",
        "a refused restore must leave the data root untouched"
    );
    assert!(
        !std::fs::read_dir(scratch.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with("data.backup-")),
        "a refused restore must not have moved anything aside"
    );

    drop(holder);
    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "restore must proceed once the holder releases: {}",
        combined(&out)
    );
}

/// The exclusion anchor must survive a real `restore --force`.
///
/// The recorded witness on icn#2758 showed dev/ino `2049/3957598` travelling
/// into `data.backup-…` while `2049/3957606` appeared at the stable path. This
/// asserts the inode at that path is unchanged — a fresh one there is the
/// split, whether or not a second holder happens to arrive.
#[cfg(unix)]
#[test]
fn a_real_restore_leaves_the_exclusion_anchor_where_it_found_it() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);

    // Establish the anchor through a real acquisition, then release it so the
    // restore is allowed to run at all.
    let lock_path = {
        let held = icn_core::DataDirLock::acquire(&dest, "a daemon").unwrap();
        held.path().to_path_buf()
    };
    let before = inode_at(&lock_path).expect("the anchor must exist");

    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "restore must succeed: {}",
        combined(&out)
    );

    assert_eq!(
        inode_at(&lock_path),
        Some(before),
        "the data root's exclusion anchor must be the same file after a restore; a new inode \
         at this path is exactly the split icn#2758 reports"
    );
}

/// Nor when the archive itself carries one.
///
/// A backup taken after #2749 contains `.icn-data-dir.lock`, because
/// `append_dir_all` includes dotfiles. Unpacking that entry would replace the
/// very inode this restore is holding — the same split reached through
/// extraction instead of through `rename`. The entry is skipped, which costs
/// nothing: these files are always empty, so the one standing at that path
/// hashes identically to the archived one and the checksum still verifies.
#[cfg(unix)]
#[test]
fn a_restore_does_not_unpack_over_the_anchor_it_is_holding() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    // Mint and release, so the archive carries a coordination file.
    drop(icn_core::DataDirLock::acquire(&source, "an earlier command").unwrap());
    assert!(
        source.join(".icn-data-dir.lock").exists(),
        "the fixture archive must contain a coordination file"
    );
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);
    let lock_path = {
        let held = icn_core::DataDirLock::acquire(&dest, "a daemon").unwrap();
        held.path().to_path_buf()
    };
    let before = inode_at(&lock_path).expect("the anchor must exist");

    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "an archive carrying coordination files must still restore and verify: {}",
        combined(&out)
    );
    assert_eq!(
        inode_at(&lock_path),
        Some(before),
        "extracting an archived coordination file must not replace the anchor this restore \
         holds"
    );
}

/// Restoring one archive twice must not overwrite the first restore's backup.
///
/// The moved-aside directory is named after the archive's own timestamp, so the
/// second run picks the same name. Entry-by-entry moves would write into it file
/// by file; the command refuses by name instead.
#[test]
fn a_second_restore_of_the_same_archive_refuses_rather_than_overwriting_the_backup() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);
    std::fs::write(dest.join("only-in-the-first-root"), b"keep me\n").unwrap();

    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(out.status.success(), "first restore: {}", combined(&out));

    let backup = std::fs::read_dir(scratch.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with("data.backup-"))
        .expect("the first restore must have moved the old contents aside")
        .path();

    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "a second restore of the same archive must refuse: {}",
        combined(&out)
    );
    assert!(
        std::fs::read(backup.join("only-in-the-first-root")).unwrap() == b"keep me\n",
        "the first restore's moved-aside copy must be intact"
    );
}

// ---------------------------------------------------------------------------
// icn#2759 — the local store openers
// ---------------------------------------------------------------------------

/// Every command that opens this root's stores is refused while it is held.
///
/// One table rather than four tests: the property is that the set is *complete*,
/// and listing the commands together is what makes an omission visible.
#[test]
fn the_local_store_openers_are_refused_while_the_data_root_is_held() {
    let openers: [&[&str]; 5] = [
        &["coop", "entity-report", "--json"],
        &["coop", "entity-unknown-legacy-report", "--json"],
        &["coop", "entity-backfill-surrogates", "--json"],
        &["treasury", "entity-backfill-report", "--json"],
        &["treasury", "entity-backfill-apply", "--json"],
    ];

    for args in openers {
        let scratch = TempDir::new().unwrap();
        let data_dir = scratch.path().join("data");
        seeded_root(&data_dir);

        let holder =
            icn_core::DataDirLock::acquire(&data_dir, "runtime-root provisioning").unwrap();
        let out = icnctl(&data_dir).args(args).output().unwrap();
        assert_refused_by_the_domain(&out, &format!("`icnctl {}`", args.join(" ")));

        drop(holder);
        let out = icnctl(&data_dir).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "`icnctl {}` must succeed once the holder releases: {}",
            args.join(" "),
            combined(&out)
        );
    }
}

/// `init-coop` opens the trust store the ceremony writes its authority edges
/// into, and writes the `icn.toml` the ceremony publishes into. Both locks.
#[test]
fn init_coop_is_refused_while_the_data_root_is_held() {
    let scratch = TempDir::new().unwrap();
    let data_dir = scratch.path().join("data");
    seeded_root(&data_dir);

    let holder = icn_core::DataDirLock::acquire(&data_dir, "runtime-root provisioning").unwrap();
    let out = icnctl(&data_dir)
        .args(["init-coop", "--name", "Blocked", "--yes", "--no-start"])
        .output()
        .unwrap();
    assert_refused_by_the_domain(&out, "init-coop");
    drop(holder);
}

/// And by a ceremony holding only the *configuration* of that directory.
///
/// `init-coop`'s check-then-write on `icn.toml` is the stale-snapshot shape the
/// federation writers had before `ManagedConfigEdit`: it can find the file
/// absent, a ceremony can publish one carrying `[cooperative]`, and this can
/// then write over it. That writer contends for the configuration lock, not the
/// storage one.
#[test]
fn init_coop_is_refused_by_a_configuration_holder_alone() {
    let scratch = TempDir::new().unwrap();
    let data_dir = scratch.path().join("data");
    seeded_root(&data_dir);

    let publisher =
        icn_core::DataDirLock::acquire_config(&data_dir, "runtime-root provisioning").unwrap();
    let out = icnctl(&data_dir)
        .args(["init-coop", "--name", "Blocked", "--yes", "--no-start"])
        .output()
        .unwrap();
    assert_refused_by_the_domain(&out, "init-coop against a configuration publisher");
    drop(publisher);
}

/// A misconfigured `--data-dir` is the N2-A gate's refusal to make, for every
/// command that now joins the domain.
///
/// Joining happens before the gate — deliberately, so nothing can open these
/// stores between the gate's verdict and the work it clears. That ordering made
/// the join speak for a path that is not a directory: the ownership probe
/// creates a file, and creating a file *inside* a regular file reports `ENOTDIR`
/// named after the probe, burying the operator's actual mistake.
///
/// Both entry points are covered because they were wrong for different reasons
/// and were fixed at different places: `join` classified too late, and
/// `init-coop` ran the account guard at the call site *before* the classifying
/// constructor, which left that constructor's own guard unreachable.
#[test]
fn a_data_dir_that_is_not_a_directory_is_refused_by_the_gate_not_by_a_probe() {
    for args in [
        vec!["coop", "entity-report"],
        vec!["treasury", "entity-backfill-report"],
        vec!["init-coop", "--name", "X", "--yes", "--no-start"],
    ] {
        let scratch = TempDir::new().unwrap();
        let not_a_dir = scratch.path().join("data-dir-is-a-file");
        std::fs::write(&not_a_dir, b"oops").unwrap();

        let out = icnctl(&not_a_dir).args(&args).output().unwrap();
        let text = combined(&out);
        assert!(
            !out.status.success(),
            "`icnctl {}` must refuse a --data-dir that is not a directory:\n{text}",
            args.join(" ")
        );
        assert!(
            text.contains("N2-A startup gate refused"),
            "`icnctl {}` must be refused BY THE GATE, not by a coordination probe:\n{text}",
            args.join(" ")
        );
        assert!(
            !text.contains("identity-probe"),
            "`icnctl {}` must not report the ownership probe's ENOTDIR instead:\n{text}",
            args.join(" ")
        );
        // Nothing was created beside it, and the file itself is untouched.
        assert_eq!(std::fs::read(&not_a_dir).unwrap(), b"oops");
        let left: Vec<_> = std::fs::read_dir(scratch.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            left.len(),
            1,
            "a refused command must not have minted anything beside the path: {left:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The boundary: what deliberately stays outside the domain
// ---------------------------------------------------------------------------

/// Readers are not serialized by this repair.
///
/// `backup` walks the very tree `restore` replaces. Bringing it into the
/// exclusion would be convenient and wrong: #2749 twice had to remove creating
/// acquisition from read paths after an unconditional lock turned a mistaken
/// command into a permanent daemon startup failure. A torn archive taken
/// against a live daemon is a real but *different* question, recorded rather
/// than silently answered here.
#[test]
fn a_read_only_observer_is_still_admitted_while_the_root_is_held() {
    let scratch = TempDir::new().unwrap();
    let data_dir = scratch.path().join("data");
    seeded_root(&data_dir);

    let _holder = icn_core::DataDirLock::acquire(&data_dir, "a running daemon").unwrap();
    let out = icnctl(&data_dir)
        .arg("backup")
        .arg(scratch.path().join("out.tar"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "reading a held data root must not be refused: {}",
        combined(&out)
    );
}

/// A maintenance command must not mint a data directory just to hold a lock.
///
/// The read-only contract these commands already had: a root that does not
/// exist reports an empty result and creates nothing. The window that leaves is
/// closed at the store open, not by materializing a directory here.
#[test]
fn an_absent_data_root_still_reports_empty_and_creates_nothing() {
    let scratch = TempDir::new().unwrap();
    let absent = scratch.path().join("never-created");

    let out = icnctl(&absent)
        .args(["treasury", "entity-backfill-report", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "an absent data root must report empty, not fail: {}",
        combined(&out)
    );
    assert!(
        !absent.exists(),
        "a read-only report must not have created the data directory"
    );
}
/// `restore --force` with a `--data-dir` that ends in a separator -- what shell
/// completion produces -- must still move the existing contents *beside* the
/// root, never into it.
///
/// The backup name used to be formatted from the root's display string, so
/// `<root>/` yielded `<root>/.backup-<t>`: a directory inside the root being
/// emptied. Moving the root's entries into it then reached the backup itself,
/// which cannot be renamed into itself, and the restore stopped with the live
/// root's contents split between the two. (Before contents were moved
/// individually, the whole-root `rename` failed outright on the same path, so
/// nothing moved; this is the case that change has to keep safe.)
#[test]
fn restore_force_with_a_trailing_separator_moves_contents_beside_the_root() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);
    std::fs::write(dest.join("store").join("marker"), b"pre-restore\n").unwrap();

    let mut spelled = dest.clone().into_os_string();
    spelled.push("/");
    let spelled = PathBuf::from(spelled);

    let out = icnctl(&spelled)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "restore through `{}` must succeed: {}",
        spelled.display(),
        combined(&out)
    );

    let names = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    };
    let inside: Vec<String> = names(&dest)
        .into_iter()
        .filter(|n| n.contains(".backup-"))
        .collect();
    assert!(
        inside.is_empty(),
        "a backup directory was created inside the data root it was emptying: {inside:?}"
    );

    let siblings: Vec<String> = names(scratch.path())
        .into_iter()
        .filter(|n| n.starts_with("data.backup-"))
        .collect();
    assert_eq!(
        siblings.len(),
        1,
        "exactly one backup must sit beside the root: {siblings:?}"
    );
    assert_eq!(
        std::fs::read(
            scratch
                .path()
                .join(&siblings[0])
                .join("store")
                .join("marker")
        )
        .unwrap(),
        b"pre-restore\n",
        "the replaced contents must be in the sibling backup, whole"
    );
    assert_eq!(
        std::fs::read(dest.join("store").join("marker")).unwrap(),
        b"original\n",
        "the root must hold the restored archive"
    );
}

/// The same, spelled `--data-dir ./` from inside the root: no final component to
/// append to, so the backup name must come from the directory it resolves to.
#[test]
fn restore_force_with_a_dot_data_dir_moves_contents_beside_the_root() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);
    std::fs::write(dest.join("store").join("marker"), b"pre-restore\n").unwrap();

    let out = icnctl(Path::new("./"))
        .current_dir(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "restore through `./` must succeed: {}",
        combined(&out)
    );

    assert!(
        !std::fs::read_dir(&dest).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("backup-")),
        "a backup directory was created inside the data root it was emptying"
    );
    let siblings: Vec<PathBuf> = std::fs::read_dir(scratch.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("data.backup-"))
        })
        .collect();
    assert_eq!(
        siblings.len(),
        1,
        "exactly one backup beside the root: {siblings:?}"
    );
    assert_eq!(
        std::fs::read(siblings[0].join("store").join("marker")).unwrap(),
        b"pre-restore\n"
    );
    assert_eq!(
        std::fs::read(dest.join("store").join("marker")).unwrap(),
        b"original\n"
    );
}

/// A `--data-dir` that is a symlink to a directory *containing* it names a
/// location inside the very root it resolves to. Moving that root's contents
/// aside would move the name itself (and, computed lexically, put the backup
/// inside the root). Restore must refuse before moving anything.
#[cfg(unix)]
#[test]
fn restore_refuses_a_data_dir_that_lives_inside_the_root_it_resolves_to() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let root = scratch.path().join("icn");
    seeded_root(&root);
    std::fs::write(root.join("store").join("marker"), b"pre-restore\n").unwrap();
    let named = root.join("data");
    std::os::unix::fs::symlink(&root, &named).unwrap();
    // The root's contents. The two coordination files are excluded by name:
    // restore takes its locks before reading or moving anything, and any
    // participant in the domain creates and retains them.
    let contents = |dir: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != ".icn-data-dir.lock" && n != ".icn-config.lock")
            .collect();
        v.sort();
        v
    };
    let before = contents(&root);

    let out = icnctl(&named)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();

    assert!(
        !out.status.success(),
        "restore through a data dir inside its own root must be refused: {}",
        combined(&out)
    );
    assert!(
        combined(&out).contains("which contains it"),
        "refused for this reason, not another: {}",
        combined(&out)
    );
    let after = contents(&root);
    assert_eq!(
        after,
        before,
        "nothing may be moved, created or removed in the root: {}",
        combined(&out)
    );
    assert_eq!(
        std::fs::read(root.join("store").join("marker")).unwrap(),
        b"pre-restore\n"
    );
    let backups: Vec<String> = [scratch.path(), root.as_path()]
        .iter()
        .flat_map(|d| contents(d))
        .filter(|n| n.contains(".backup-"))
        .collect();
    assert!(
        backups.is_empty(),
        "no backup may be created anywhere: {backups:?}"
    );
}

/// An ordinary symlinked data dir: the backup is a sibling of the *real* root
/// it resolves to, not of the link, and the link still names the restored root.
#[cfg(unix)]
#[test]
fn restore_force_through_a_symlinked_data_dir_backs_up_beside_the_real_root() {
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let real = scratch.path().join("disk").join("icn");
    seeded_root(&real);
    std::fs::write(real.join("store").join("marker"), b"pre-restore\n").unwrap();
    let link = scratch.path().join("data");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let out = icnctl(&link)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "restore must succeed: {}",
        combined(&out)
    );

    let beside_real: Vec<PathBuf> = std::fs::read_dir(real.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("icn.backup-"))
        })
        .collect();
    assert_eq!(
        beside_real.len(),
        1,
        "one backup beside the real root: {beside_real:?}"
    );
    assert_eq!(
        std::fs::read(beside_real[0].join("store").join("marker")).unwrap(),
        b"pre-restore\n"
    );
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(link.join("store").join("marker")).unwrap(),
        b"original\n"
    );
}

/// Whether this process is exempt from directory permissions (root), in which
/// case a read-only directory proves nothing and the write-ordering tests below
/// must not claim to have checked it.
#[cfg(unix)]
fn permissions_bind_this_process(dir: &Path) -> bool {
    let probe = dir.join(".perm-check");
    let bound = std::fs::write(&probe, b"").is_err();
    let _ = std::fs::remove_file(&probe);
    bound
}

/// A contender the exclusion domain refuses must be refused *by the domain*,
/// before it writes anything inside the root another process holds. The root is
/// made read-only while held: a contender that checks the lock first gets the
/// domain's refusal; one that writes first fails on that write instead.
#[cfg(unix)]
#[test]
fn a_refused_store_opener_writes_nothing_inside_the_held_root() {
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = TempDir::new().unwrap();
    let data_dir = scratch.path().join("data");
    seeded_root(&data_dir);
    let holder = icn_core::DataDirLock::acquire(&data_dir, "runtime-root provisioning").unwrap();

    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let bound = permissions_bind_this_process(&data_dir);
    let out = icnctl(&data_dir)
        .args(["coop", "entity-report", "--json"])
        .output()
        .unwrap();
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(holder);

    if !bound {
        eprintln!("skipped: directory permissions do not bind this process (running as root)");
        return;
    }
    assert_refused_by_the_domain(&out, "`icnctl coop entity-report`");
}

/// The same for restore: refused by the domain before it writes in the held root.
#[cfg(unix)]
#[test]
fn a_refused_restore_writes_nothing_inside_the_held_root() {
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    seeded_root(&source);
    let archive = scratch.path().join("backup.tar");
    archive_from(&source, &archive);

    let dest = scratch.path().join("data");
    seeded_root(&dest);
    let holder = icn_core::DataDirLock::acquire(&dest, "a daemon").unwrap();

    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o555)).unwrap();
    let bound = permissions_bind_this_process(&dest);
    let out = icnctl(&dest)
        .arg("restore")
        .arg(&archive)
        .arg("--force")
        .output()
        .unwrap();
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(holder);

    if !bound {
        eprintln!("skipped: directory permissions do not bind this process (running as root)");
        return;
    }
    assert_refused_by_the_domain(&out, "restore --force");
}

/// And for `init-coop`, which takes a creating acquisition: refused by the
/// domain before it writes anything inside the root another process holds.
#[cfg(unix)]
#[test]
fn a_refused_init_coop_writes_nothing_inside_the_held_root() {
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = TempDir::new().unwrap();
    let data_dir = scratch.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let holder = icn_core::DataDirLock::acquire(&data_dir, "runtime-root provisioning").unwrap();

    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let bound = permissions_bind_this_process(&data_dir);
    let out = icnctl(&data_dir)
        .args(["init-coop", "--name", "Blocked", "--yes", "--no-start"])
        .output()
        .unwrap();
    std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    drop(holder);

    if !bound {
        eprintln!("skipped: directory permissions do not bind this process (running as root)");
        return;
    }
    assert_refused_by_the_domain(&out, "init-coop");
}
