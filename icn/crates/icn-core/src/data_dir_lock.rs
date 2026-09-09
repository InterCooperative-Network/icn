//! Process-level exclusion over one ICN data directory.
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
//! A lock respected only by the writer is not exclusion. Both parties take this
//! one.
//!
//! # Mechanism
//!
//! An advisory lock on a regular file in the data root, via
//! `std::fs::File::try_lock`. The kernel releases it when the holder dies, so a
//! crash cannot strand the directory — the property a `create_new` marker file
//! or a PID file would not have, and the reason neither is used here.
//!
//! The path is derived from the **canonicalized** data root, so two spellings
//! of the same directory cannot produce two independent locks over one state.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// File name of the exclusion lock, inside the data root.
const LOCK_FILE_NAME: &str = ".icn-data-dir.lock";

/// Exclusive ownership of one data directory, released on drop or on death.
#[derive(Debug)]
pub struct DataDirLock {
    _file: std::fs::File,
    path: PathBuf,
}

impl DataDirLock {
    /// The lock path for a data root, resolved through the real directory so
    /// aliases (`/a/b`, `/a/./b`, a symlinked parent) map to one identity.
    pub fn lock_path(data_dir: &Path) -> PathBuf {
        let root = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        root.join(LOCK_FILE_NAME)
    }

    /// Take ownership if this root is one a maintenance ceremony could manage,
    /// otherwise report that it is not.
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
    ///   error. Contention does not need write permission: `try_lock` is
    ///   `flock(2)`, which operates on the descriptor irrespective of open mode.
    /// * **it does not exist and cannot be created** — report `Ok(None)`.
    ///   Nothing holds a lock that does not exist, and a ceremony that cannot
    ///   create a file here cannot provision here either. This is what keeps a
    ///   daemon whose configuration lives in a packaged read-only directory
    ///   working unchanged.
    ///
    /// # Actor model, stated plainly
    ///
    /// This serializes cooperating ICN processes running under compatible
    /// filesystem authority. It is **not** a security boundary against a more
    /// privileged local adversary, who could remove the file or ignore advisory
    /// locking altogether.
    pub fn acquire_if_manageable(data_dir: &Path, holder: &str) -> Result<Option<Self>> {
        let path = Self::lock_path(data_dir);

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
                        "Refusing to proceed: could not determine whether the data-directory \
                         lock {} exists, so it cannot be established that no other ICN process \
                         holds this root",
                        path.display()
                    )
                })
            }
        };
        if exists {
            return Self::acquire(data_dir, holder).map(Some);
        }

        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(_) => Self::acquire(data_dir, holder).map(Some),
            // Created between the check and here — contend rather than assume.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::acquire(data_dir, holder).map(Some)
            }
            // Genuinely absent and uncreatable: nothing holds a lock that does
            // not exist, and a ceremony that cannot write here cannot provision
            // here either.
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(None),
            Err(e) => Err(e).with_context(|| {
                format!(
                    "Failed to create the data-directory lock {}",
                    path.display()
                )
            }),
        }
    }

    /// Take exclusive ownership, or fail describing who holds it.
    ///
    /// `holder` names the caller in the refusal, so an operator learns which
    /// side to stop.
    pub fn acquire(data_dir: &Path, holder: &str) -> Result<Self> {
        let path = Self::lock_path(data_dir);

        // Containment before creation: never open through a link, and never
        // treat a directory or device as the lock.
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() || !meta.is_file() {
                bail!(
                    "Refusing to take the data-directory lock: {} exists and is not a regular \
                     file.",
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
        let file = match std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                std::fs::File::open(&path).with_context(|| {
                    format!(
                        "Failed to open the data-directory lock {} even read-only",
                        path.display()
                    )
                })?
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("Failed to open the data-directory lock {}", path.display())
                })
            }
        };

        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file, path }),
            Err(_) => bail!(
                "Refusing to start {holder}: another ICN process already holds {}.\n\
                 A running daemon and a maintenance ceremony must not share a data directory — \
                 one would rewrite state the other has already read. Stop the other process and \
                 retry; the lock is released automatically when it exits, so nothing needs \
                 cleaning up by hand.",
                data_dir.display()
            ),
        }
    }
}

impl DataDirLock {
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
        let held = DataDirLock::acquire(dir.path(), "first").unwrap();
        // The optional form must NOT answer "nothing to exclude" for a lock
        // somebody is holding — that would let a daemon proceed unguarded.
        assert!(
            DataDirLock::acquire_if_manageable(dir.path(), "daemon").is_err(),
            "an existing, held lock must fail closed rather than return Ok(None)"
        );
        drop(held);
        assert!(
            DataDirLock::acquire_if_manageable(dir.path(), "daemon")
                .unwrap()
                .is_some(),
            "and must acquire once released"
        );
    }

    #[test]
    fn dotted_and_relative_spellings_share_one_identity() {
        let dir = tempfile::TempDir::new().unwrap();
        let canonical = DataDirLock::lock_path(dir.path());
        assert_eq!(
            canonical,
            DataDirLock::lock_path(&dir.path().join(".")),
            "`root/.` must not create a second exclusion domain"
        );

        // A symlink alias to the same root must also resolve to one identity.
        let alias_parent = tempfile::TempDir::new().unwrap();
        let alias = alias_parent.path().join("alias");
        std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
        assert_eq!(
            canonical,
            DataDirLock::lock_path(&alias),
            "a symlinked alias of the root must share its lock identity"
        );
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
}
