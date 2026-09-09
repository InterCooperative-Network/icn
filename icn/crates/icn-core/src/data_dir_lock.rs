//! Process-level exclusion over the two things a daemon must not have moved
//! under it: the storage it mutates, and the configuration it has consumed.
//!
//! # Why this exists
//!
//! Several operations are documented as requiring "the daemon stopped": they
//! open the daemon's sled databases directly and rewrite authoritative state.
//! Until now that requirement was enforced only *incidentally*, by sled's own
//! per-database locks, and that is not an exclusion protocol:
//!
//! * sled locks are held only while a database handle is open. A maintenance
//!   ceremony that deliberately closes its handles — to re-read state through
//!   fresh ones, say — leaves a window in which nothing is locked at all.
//! * the N2-A startup gate takes those locks *and releases them when it
//!   returns*, so passing the gate says nothing about the next instant.
//! * `icnd` loads and validates its configuration **before** it opens any
//!   store, so it can read a configuration that a ceremony is about to replace
//!   and then keep running with it.
//!
//! That last sequence is the harmful one. A daemon can load the old
//! configuration, a ceremony can publish a new treasury and commit its
//! completion marker, and the daemon carries on with the node-DID fallback the
//! ceremony exists to remove — durable state saying one thing while the running
//! process does another.
//!
//! A lock respected only by the writer is not exclusion. Both parties take
//! these.
//!
//! # Two locks, because they protect two different things
//!
//! The storage root and the directory holding the configuration file are not
//! always the same directory: `icnd --config /A/icn.toml --data-dir /B` names
//! `/A` for one and `/B` for the other, and so does any configuration whose
//! `data_dir` points somewhere other than beside itself — the shipped two-node
//! demo does exactly that. One lock cannot cover both, and releasing the
//! configuration one after the read reintroduces precisely the race above.
//!
//! * [`DataDirLock::acquire`] — **storage**, `.icn-data-dir.lock` in the data
//!   root. Always exclusive: at most one process may mutate that state.
//! * [`DataDirLock::acquire_config`] /
//!   [`DataDirLock::acquire_config_shared_if_manageable`] — **configuration**,
//!   `.icn-config.lock` in the directory holding the configuration file.
//!
//! The configuration lock is shared/exclusive on purpose. A daemon *reads* a
//! configuration, so it takes the shared side and any number of daemons may
//! read from one directory. A ceremony *publishes* `<data_dir>/icn.toml`, so it
//! takes the exclusive side and is refused while any daemon still holds an
//! interpretation of bytes in that directory. Making the daemon's side
//! exclusive instead would refuse the second node of `scripts/demo-two-node.sh`,
//! whose two configurations live in one directory with different data roots.
//!
//! They are separate **files** rather than two modes of one file because
//! `flock(2)` is per open-file-description: a process holding one handle on a
//! path conflicts with its own second handle on that same path. Separate files
//! let a daemon whose two roots coincide hold both without contending with
//! itself.
//!
//! # Ordering
//!
//! Both actors take the configuration lock before the storage lock. Ordering is
//! stated for readability rather than for safety: every acquisition here is
//! non-blocking (`try_lock`/`try_lock_shared`), so a crossed pair of processes
//! gets an immediate refusal on one side. A deadlock is not reachable.
//!
//! # Mechanism
//!
//! An advisory lock on a regular file in the directory, via
//! `std::fs::File::try_lock`. The kernel releases it when the holder dies, so a
//! crash cannot strand the directory — the property a `create_new` marker file
//! or a PID file would not have, and the reason neither is used here.
//!
//! The path is derived from the **canonicalized** directory, so two spellings
//! of the same directory cannot produce two independent locks over one state.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// File name of the storage-exclusion lock, inside the data root.
const LOCK_FILE_NAME: &str = ".icn-data-dir.lock";

/// File name of the configuration-exclusion lock, inside the directory holding
/// the configuration file.
const CONFIG_LOCK_FILE_NAME: &str = ".icn-config.lock";

/// How many holders a lock admits at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sharing {
    /// One holder, excluding everybody: mutating storage, or publishing a
    /// configuration.
    Exclusive,
    /// Any number of concurrent holders, excluding only an exclusive one:
    /// reading a configuration.
    Shared,
}

/// Ownership of one lock, released on drop or on death.
#[derive(Debug)]
pub struct DataDirLock {
    _file: std::fs::File,
    path: PathBuf,
}

impl DataDirLock {
    /// The storage lock path for a data root, resolved through the real
    /// directory so aliases (`/a/b`, `/a/./b`, a symlinked parent) map to one
    /// identity.
    pub fn lock_path(data_dir: &Path) -> PathBuf {
        Self::lock_path_named(data_dir, LOCK_FILE_NAME)
    }

    /// The configuration lock path for the directory holding a configuration
    /// file, resolved the same way.
    pub fn config_lock_path(config_root: &Path) -> PathBuf {
        Self::lock_path_named(config_root, CONFIG_LOCK_FILE_NAME)
    }

    fn lock_path_named(root: &Path, name: &str) -> PathBuf {
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        root.join(name)
    }

    /// Take exclusive ownership of a data root's storage, or fail describing
    /// who holds it.
    ///
    /// `holder` names the caller in the refusal, so an operator learns which
    /// side to stop.
    pub fn acquire(data_dir: &Path, holder: &str) -> Result<Self> {
        Self::take(
            Self::lock_path(data_dir),
            Sharing::Exclusive,
            &format!(
                "Refusing to start {holder}: another ICN process already holds {}.\n\
                 A running daemon and a maintenance ceremony must not share a data directory — \
                 one would rewrite state the other has already read. Stop the other process and \
                 retry; the lock is released automatically when it exits, so nothing needs \
                 cleaning up by hand.",
                data_dir.display()
            ),
        )
    }

    /// Take exclusive ownership of a directory's configuration, as a writer.
    ///
    /// This is the ceremony's side: it publishes `<data_dir>/icn.toml`, so it
    /// must exclude every daemon that has already consumed bytes from that
    /// directory — including daemons whose *storage* is somewhere else entirely
    /// and which therefore do not contend for [`Self::acquire`] at all.
    pub fn acquire_config(config_root: &Path, holder: &str) -> Result<Self> {
        Self::take(
            Self::config_lock_path(config_root),
            Sharing::Exclusive,
            &Self::config_refusal(config_root, holder),
        )
    }

    /// Take the reader's share of a directory's configuration if this directory
    /// is one a ceremony could publish into, otherwise report that it is not.
    ///
    /// This is the daemon's side. It is *shared* because a daemon only reads:
    /// several daemons may hold configurations from one directory at once — the
    /// shipped two-node demo keeps both node configurations in `config/` with
    /// different data roots — while any ceremony publishing there is refused.
    ///
    /// The daemon holds it for its whole life, not just across the read. The
    /// interpretation it took from those bytes stays authoritative for as long
    /// as the process runs, so releasing it after `Config::from_file` would
    /// leave a ceremony free to rewrite the file underneath a daemon still
    /// acting on the old contents.
    ///
    /// # Why this is fail-closed where it matters
    ///
    /// An earlier version treated *any* `PermissionDenied` as "nothing to
    /// exclude". That is wrong once the lock file exists: a file this process
    /// cannot open for writing may be one another process is holding right now,
    /// and answering `Ok(None)` there would let a daemon consume configuration
    /// with no exclusion at all. The two cases are therefore separated:
    ///
    /// * **the lock file exists** — contend for it, and fail closed on any
    ///   error. Contention does not need write permission: `try_lock_shared` is
    ///   `flock(2)`, which operates on the descriptor irrespective of open mode.
    /// * **it does not exist and cannot be created** — report `Ok(None)`.
    ///   Nothing holds a lock that does not exist, and a ceremony that cannot
    ///   create a file here cannot publish a configuration here either. This is
    ///   what keeps a daemon whose configuration lives in a packaged read-only
    ///   directory — a Kubernetes ConfigMap mount, say — working unchanged.
    ///
    /// # Actor model, stated plainly
    ///
    /// This serializes cooperating ICN processes running under compatible
    /// filesystem authority. It is **not** a security boundary against a more
    /// privileged local adversary, who could remove the file or ignore advisory
    /// locking altogether.
    pub fn acquire_config_shared_if_manageable(
        config_root: &Path,
        holder: &str,
    ) -> Result<Option<Self>> {
        let path = Self::config_lock_path(config_root);
        let refusal = Self::config_refusal(config_root, holder);

        // Classify existence explicitly. `is_ok()` would collapse *every*
        // metadata error into "absent" — including a traversal or permission
        // error that leaves existence unknown — and being unable to determine
        // whether a lock exists is not evidence that none does.
        let exists = match std::fs::symlink_metadata(&path) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return Err(e).with_context(|| {
                    format!(
                        "Refusing to proceed: could not determine whether the configuration \
                         lock {} exists, so it cannot be established that no other ICN process \
                         holds this directory",
                        path.display()
                    )
                })
            }
        };
        if exists {
            return Self::take(path, Sharing::Shared, &refusal).map(Some);
        }

        let mut create = std::fs::OpenOptions::new();
        create.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            // An explicit mode, not `0666 & !umask`. These files are retained
            // after release, so a ceremony run under a restrictive umask would
            // otherwise leave a coordination file whose permissions depend on
            // the shell that happened to start it.
            create.mode(0o600);
        }
        match create.open(&path) {
            Ok(_) => Self::take(path, Sharing::Shared, &refusal).map(Some),
            // Created between the check and here — contend rather than assume.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::take(path, Sharing::Shared, &refusal).map(Some)
            }
            // A read-only *filesystem* is immutable to every actor on it — a
            // Kubernetes ConfigMap mount (`readOnly: true`), a packaged image
            // layer. No process on this machine can publish a configuration
            // there, so there is genuinely nothing to exclude and the daemon
            // starts exactly as before. This is the case the earlier
            // `PermissionDenied` carve-out was actually aiming at.
            Err(e) if e.kind() == std::io::ErrorKind::ReadOnlyFilesystem => Ok(None),
            // Permission is a different statement. It says only that *this*
            // process may not write here, and a more privileged one still can:
            // a daemon running as its service account over a root-owned
            // configuration directory cannot create this file, while a ceremony
            // run under `sudo` in the same directory can — and can republish
            // the configuration underneath it. Answering "nothing to exclude"
            // there is not a conclusion, it is an absence of one.
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(e).with_context(|| {
                    format!(
                        "Refusing to start {holder}: the configuration lock {} does not exist and \
                     this process may not create it, so it cannot be established that no other \
                     ICN process will republish this configuration.\n\
                     ICN maintenance runs under the same account as the daemon — the deployment \
                     scripts use `runuser -u icn` / `sudo -u icn` — so make this directory \
                     writable by that account, or point `--config` at a directory it owns. A \
                     configuration that is genuinely immutable to every actor (a read-only \
                     mount) is recognised as such and needs no lock.",
                        path.display()
                    )
                })
            }
            Err(e) => Err(e).with_context(|| {
                format!("Failed to create the configuration lock {}", path.display())
            }),
        }
    }

    fn config_refusal(config_root: &Path, holder: &str) -> String {
        format!(
            "Refusing to start {holder}: another ICN process already holds the configuration in \
             {}.\n\
             A daemon that has read a configuration keeps acting on it, so a ceremony must not \
             republish that file underneath it. Stop the other process and retry; the lock is \
             released automatically when it exits, so nothing needs cleaning up by hand.",
            config_root.display()
        )
    }

    fn take(path: PathBuf, sharing: Sharing, refusal: &str) -> Result<Self> {
        // Containment before creation: never open through a link, and never
        // treat a directory or device as the lock.
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() || !meta.is_file() {
                bail!(
                    "Refusing to take the lock: {} exists and is not a regular file.",
                    path.display()
                );
            }
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        // Open for writing when we can, but fall back to read-only: `try_lock`
        // is `flock(2)`, which locks the descriptor regardless of open mode, so
        // a process that may only read the file can still discover — and
        // contend for — ownership. Requiring write permission here would turn an
        // unwritable directory into a silent absence of exclusion.
        let mut open = std::fs::OpenOptions::new();
        open.create(true).truncate(false).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            // Deterministic, umask-independent. Applies only on creation.
            open.mode(0o600);
        }
        let file = match open.open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                std::fs::File::open(&path).with_context(|| {
                    format!(
                        "Failed to open the lock {} even read-only.\n\
                         These coordination files are retained after release, so one created by \
                         a process running under a different account can lock every later ICN \
                         process out of this directory. ICN maintenance is expected to run under \
                         the same account as the daemon (`runuser -u icn` / `sudo -u icn`); if \
                         this file belongs to another account, remove it or give it back to the \
                         daemon's.",
                        path.display()
                    )
                })?
            }
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("Failed to open the lock {}", path.display()))
            }
        };

        let taken = match sharing {
            Sharing::Exclusive => file.try_lock(),
            Sharing::Shared => file.try_lock_shared(),
        };
        match taken {
            Ok(()) => Ok(Self { _file: file, path }),
            Err(_) => bail!("{refusal}"),
        }
    }

    /// The lock file this guard holds.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DataDirLock {
    fn drop(&mut self) {
        // Releasing is closing the handle, which Drop does. The empty file is
        // left behind deliberately: unlinking it would race another process
        // that has just opened it and is about to lock.
        let _ = &self.path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_acquisition_is_refused_while_the_first_is_held() {
        let dir = tempfile::TempDir::new().unwrap();
        let first = DataDirLock::acquire(dir.path(), "test").expect("first must acquire");
        let second = DataDirLock::acquire(dir.path(), "daemon");
        assert!(second.is_err(), "a second holder must be refused");
        assert!(
            format!("{:#}", second.unwrap_err()).contains("already holds"),
            "the refusal must name the conflict"
        );
        drop(first);
        DataDirLock::acquire(dir.path(), "test").expect("must acquire after release");
    }

    #[test]
    fn alias_spellings_of_one_root_share_one_lock() {
        let dir = tempfile::TempDir::new().unwrap();
        let held = DataDirLock::acquire(dir.path(), "test").unwrap();

        // `/a/./b` and `/a/b` name the same directory and must not yield two
        // independent locks over the same authoritative state.
        let aliased = dir.path().join(".");
        assert!(
            DataDirLock::acquire(&aliased, "daemon").is_err(),
            "an alias spelling must contend with the same lock"
        );
        drop(held);
    }

    #[test]
    fn an_existing_held_lock_fails_closed_rather_than_reporting_absence() {
        let dir = tempfile::TempDir::new().unwrap();
        let held = DataDirLock::acquire_config(dir.path(), "a ceremony").unwrap();
        // The optional form must NOT answer "nothing to exclude" for a lock
        // somebody is holding — that would let a daemon proceed unguarded.
        assert!(
            DataDirLock::acquire_config_shared_if_manageable(dir.path(), "daemon").is_err(),
            "an existing, held lock must fail closed rather than return Ok(None)"
        );
        drop(held);
        assert!(
            DataDirLock::acquire_config_shared_if_manageable(dir.path(), "daemon")
                .unwrap()
                .is_some(),
            "and must acquire once released"
        );
    }

    #[test]
    fn dotted_and_relative_spellings_share_one_identity() {
        let dir = tempfile::TempDir::new().unwrap();
        for resolve in [
            DataDirLock::lock_path as fn(&Path) -> PathBuf,
            DataDirLock::config_lock_path as fn(&Path) -> PathBuf,
        ] {
            let canonical = resolve(dir.path());
            assert_eq!(
                canonical,
                resolve(&dir.path().join(".")),
                "`root/.` must not create a second exclusion domain"
            );

            // A symlink alias to the same root must also resolve to one identity.
            let alias_parent = tempfile::TempDir::new().unwrap();
            let alias = alias_parent.path().join("alias");
            std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
            assert_eq!(
                canonical,
                resolve(&alias),
                "a symlinked alias of the root must share its lock identity"
            );
        }
    }

    #[test]
    fn a_symlinked_lock_path_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let target = outside.path().join("elsewhere.lock");
        std::fs::write(&target, b"").unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join(LOCK_FILE_NAME)).unwrap();

        let err = DataDirLock::acquire(dir.path(), "test")
            .expect_err("a symlinked lock path must be refused");
        assert!(format!("{err:#}").contains("not a regular file"));
    }

    /// The configuration lock admits many readers and no writer beside them.
    ///
    /// This is the property that keeps `scripts/demo-two-node.sh` working:
    /// `config/icn-alpha.toml` and `config/icn-beta.toml` share a directory but
    /// name different data roots, so both daemons hold this one lock at once.
    /// An exclusive daemon-side lock would refuse the second node outright.
    #[test]
    fn many_daemons_may_read_one_directory_but_no_ceremony_may_publish_into_it() {
        let dir = tempfile::TempDir::new().unwrap();

        let alpha = DataDirLock::acquire_config_shared_if_manageable(dir.path(), "alpha")
            .unwrap()
            .expect("a writable directory is manageable");
        let beta = DataDirLock::acquire_config_shared_if_manageable(dir.path(), "beta")
            .unwrap()
            .expect("a second daemon must be admitted alongside the first");

        let publisher = DataDirLock::acquire_config(dir.path(), "runtime-root provisioning");
        assert!(
            publisher.is_err(),
            "a ceremony must not publish a configuration while a daemon holds one"
        );
        assert!(
            format!("{:#}", publisher.unwrap_err()).contains("already holds the configuration"),
            "the refusal must name the configuration, not the storage"
        );

        drop(alpha);
        assert!(
            DataDirLock::acquire_config(dir.path(), "a ceremony").is_err(),
            "one remaining reader is still enough to refuse a publisher"
        );
        drop(beta);
        DataDirLock::acquire_config(dir.path(), "a ceremony")
            .expect("with every reader gone the publisher may proceed");
    }

    /// A publisher excludes readers for as long as it holds the directory.
    #[test]
    fn a_publisher_excludes_a_daemon_that_would_read_the_same_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let ceremony =
            DataDirLock::acquire_config(dir.path(), "runtime-root provisioning").unwrap();
        assert!(
            DataDirLock::acquire_config_shared_if_manageable(dir.path(), "the daemon").is_err(),
            "a daemon must not consume a configuration that is being republished"
        );
        drop(ceremony);
        assert!(
            DataDirLock::acquire_config_shared_if_manageable(dir.path(), "the daemon")
                .unwrap()
                .is_some()
        );
    }

    /// Storage and configuration are separate identities, so one process can
    /// hold both for a directory that is simultaneously its data root and the
    /// home of its configuration file.
    ///
    /// This is why they are two files rather than two modes of one: `flock(2)`
    /// is per open-file-description, so a second handle on a single path would
    /// contend with the first *inside the same process*.
    #[test]
    fn one_process_may_hold_both_locks_on_a_single_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = DataDirLock::acquire_config_shared_if_manageable(dir.path(), "the daemon")
            .unwrap()
            .expect("manageable");
        let storage = DataDirLock::acquire(dir.path(), "the daemon")
            .expect("the storage lock must not contend with this process's own config lock");
        assert_ne!(
            config.path(),
            storage.path(),
            "the two locks must be distinct files"
        );

        // And both still exclude the ceremony's corresponding acquisition.
        assert!(DataDirLock::acquire(dir.path(), "a ceremony").is_err());
        assert!(DataDirLock::acquire_config(dir.path(), "a ceremony").is_err());
    }

    /// A directory this process cannot write to is not evidence that nobody can.
    ///
    /// The daemon runs as a service account; a configuration directory owned by
    /// `root` is readable by it and not writable. Answering `Ok(None)` there
    /// let the daemon consume a configuration that a `sudo` ceremony in the same
    /// directory was free to republish — the two are cooperating ICN processes
    /// with *unequal* filesystem authority, which the earlier actor model did
    /// not account for.
    #[cfg(unix)]
    #[test]
    fn an_unwritable_configuration_directory_fails_closed() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::TempDir::new().unwrap();
        let inner = dir.path().join("etc-icn");
        std::fs::create_dir(&inner).unwrap();
        std::fs::write(inner.join("icn.toml"), b"# readable, not writable\n").unwrap();

        // r-x: this process may read the directory and list it, but not create
        // the lock file in it. (Root ignores mode bits, so this witness only
        // means anything unprivileged — asserted below rather than assumed.)
        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o500)).unwrap();
        let creatable = std::fs::File::create(inner.join(".probe")).is_ok();
        if creatable {
            let _ = std::fs::remove_file(inner.join(".probe"));
            std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o700)).unwrap();
            eprintln!(
                "SKIPPED an_unwritable_configuration_directory_fails_closed: this process can \
                 write through a 0500 directory (running as root?), so the precondition cannot \
                 be built. Not evidence in this environment."
            );
            return;
        }

        let outcome = DataDirLock::acquire_config_shared_if_manageable(&inner, "the daemon");
        // Restore before asserting, so a failure does not leave an undeletable dir.
        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o700)).unwrap();

        let err = outcome.expect_err(
            "a directory this process cannot write must not be reported as having nothing to \
             exclude",
        );
        let msg = format!("{err:#}");
        assert!(
            msg.contains("may not create it"),
            "the refusal must say why it cannot conclude: {msg}"
        );
    }

    /// The coordination files are created with a deliberate mode, not the
    /// invoking shell's umask.
    ///
    /// They are retained after release, so a mode inherited from whatever
    /// started the process becomes a durable property of the directory.
    #[cfg(unix)]
    #[test]
    fn coordination_files_are_created_with_a_deliberate_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::TempDir::new().unwrap();

        let config = DataDirLock::acquire_config_shared_if_manageable(dir.path(), "the daemon")
            .unwrap()
            .expect("manageable");
        let storage = DataDirLock::acquire(dir.path(), "the daemon").unwrap();

        for path in [config.path(), storage.path()] {
            let mode = std::fs::symlink_metadata(path)
                .unwrap()
                .permissions()
                .mode()
                & 0o7777;
            assert_eq!(
                mode,
                0o600,
                "{} must be created with an explicit mode, not the process umask; got {mode:o}",
                path.display()
            );
        }
    }

    /// Holding the storage lock says nothing about the configuration directory.
    ///
    /// This is the gap the split closes: a daemon with `--config /A/icn.toml
    /// --data-dir /B` contends for `/B`'s storage and nobody else's, so a
    /// ceremony rooted at `/A` would sail past a storage-only protocol.
    #[test]
    fn the_storage_lock_alone_does_not_exclude_a_publisher_elsewhere() {
        let config_root = tempfile::TempDir::new().unwrap();
        let data_root = tempfile::TempDir::new().unwrap();
        let _storage = DataDirLock::acquire(data_root.path(), "the daemon").unwrap();
        DataDirLock::acquire_config(config_root.path(), "a ceremony")
            .expect("a storage lock on another root cannot protect this directory");
    }
}
