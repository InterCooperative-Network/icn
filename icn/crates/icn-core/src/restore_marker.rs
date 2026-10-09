//! The restore-incomplete marker: a durable record that a restore of a data
//! root began and has not finished.
//!
//! [`DataDirLock`](crate::DataDirLock) is an advisory lock the kernel releases
//! when its holder dies. That is the property a lock needs, and exactly the
//! wrong one here. A restore that dies part-way leaves its root split between
//! the root and the directory its contents were moved into, or holding part of
//! an archive, and none of that goes away with the process. `icnd` opens the
//! stores it finds and creates the missing ones empty, so a daemon started on
//! such a root comes up under the same identity with its state gone. This
//! marker is what outlives the crash (on storage that honours `fsync`, see
//! below): every storage acquisition refuses while
//! it exists, and it is removed only by a restore that finished, a rollback
//! that put everything back, or a recovery that verified the root.
//!
//! **Existence is the safety property.** Any entry at the name — a file, a
//! directory, a dangling symlink — refuses, and so does any error other than
//! "not found" while looking. The content is a record for recovery: what the
//! root held before anything moved (the inventory, by name and by the identity
//! of each entry), where it was moved, and how far the restore got. Content
//! that cannot be read is reported, never guessed at.
//!
//! **Removal only through [`RestoreInProgress::finish`].** There is no `Drop`
//! that clears it: a handle dropped on an error path leaves the marker exactly
//! where it is, which is the point.
//!
//! # Durability: what this does, and what it relies on
//!
//! What is done: the marker is created, written and `fsync`ed, and its
//! directory `fsync`ed, before the restore changes anything. Before the marker
//! is removed, every restored file and directory is `fsync`ed (or, after a
//! rollback or recovery, the directories entries moved between), and the
//! removal is followed by an `fsync` of the directory.
//!
//! What that relies on: a filesystem and storage stack that honour `fsync` --
//! file and directory -- by the time it returns. ext4, XFS and btrfs with their
//! default options do. Network and user-space filesystems (NFS, SMB, FUSE),
//! and storage with a volatile write cache that ignores flushes, may not, and
//! there the ordering promised here may not hold after a power loss. Where a
//! directory cannot be opened for syncing (non-Unix), [`sync_directory`] does
//! nothing.
//!
//! What is tested: process death at each step, by injecting `SIGKILL` into a
//! real restore and a real recovery (outside the automated suite, which cannot
//! schedule it). Power loss at the block layer is not tested.
//!
//! Known windows, each failing safe: a crash between the marker's `unlink` and
//! its directory's `fsync` can bring the marker back over a restore that had
//! finished -- recovery then finds the root matching the archive exactly and
//! clears it, provided the archive at the recorded path is still readable and
//! still declares the same backup (its declared checksum is recorded);
//! otherwise it lists the restored entries for the operator and the marker
//! stays. A crash
//! between creating the marker and finishing its write can leave an unreadable
//! one with nothing changed, which every participant refuses until the
//! operator, having checked, removes it.

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// File name of the marker, inside the data root.
pub const RESTORE_INCOMPLETE_FILE_NAME: &str = ".icn-restore-incomplete";

const HEADER: &str = "icn-restore-incomplete v1";

/// The largest record this reads. An inventory lists the root's top-level
/// entries, so a real one is a few kilobytes.
const MAX_RECORD_BYTES: u64 = 1 << 20;

/// What kind of restore left the marker. Written once, before the first
/// change: rewriting it in place later would risk losing the inventory to a
/// torn write at exactly the moment it matters, and recovery does not need it
/// -- it reads where things are now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestorePhase {
    /// A restore that moves the root's contents aside and then extracts: the
    /// previous state may be split between the root and the move-aside
    /// directory, and the root may hold part of the archive.
    Moving,
    /// A restore into a root that held nothing to move aside: the root may hold
    /// part of the archive.
    Extracting,
}

impl RestorePhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Moving => "moving",
            Self::Extracting => "extracting",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "moving" => Some(Self::Moving),
            "extracting" => Some(Self::Extracting),
            _ => None,
        }
    }

    /// What this phase means for the root, in an operator's terms.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Moving => {
                "it stopped while moving the existing contents aside or while extracting the \
                 archive, so they may be split between the data directory and the move-aside \
                 directory, and the data directory may hold part of the archive"
            }
            Self::Extracting => {
                "it stopped while extracting the archive into a data directory that held \
                 nothing to move aside, so it may hold part of the archive"
            }
        }
    }
}

/// One entry of the root as it stood before anything moved: its name, and the
/// identity of the entry itself (not followed), so recovery can tell the same
/// object back in place from a different one under the same name.
///
/// The identity is the device and inode **and the birth time**. Device and
/// inode alone are not enough: a filesystem may give a deleted entry's inode
/// number to the next one created (ext4 does so immediately), so a replacement
/// can wear the original's numbers. Its birth time it cannot: `rename(2)` keeps
/// an inode's birth time, and a new inode gets its own. Where a filesystem
/// reports no birth time, `birth_ns` is `None` on both sides and the identity
/// is device and inode only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryEntry {
    pub name: OsString,
    pub dev: u64,
    pub ino: u64,
    pub birth_ns: Option<u128>,
}

/// What a restore records before it changes anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreRecord {
    pub phase: RestorePhase,
    /// The archive being restored, for the operator.
    pub archive: PathBuf,
    /// The checksum the archive's metadata declares for the tree it holds --
    /// the one restore verifies the root against. A path names whatever is
    /// there now: an archive declaring a different checksum is not the backup
    /// this restore extracted. (A declared checksum is not proof of contents;
    /// recovery compares contents itself before it calls anything removable.)
    pub archive_checksum: String,
    /// Where the previous contents were moved, if the restore replaced a root
    /// that held any.
    pub move_aside: Option<PathBuf>,
    /// The root's entries before anything moved, other than its coordination
    /// files and this marker.
    pub inventory: Vec<InventoryEntry>,
}

impl RestoreRecord {
    /// The record as stored: a header, then one `key value` line each, with a
    /// readable comment beside every byte-exact (hex) value.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        out.push_str(
            "# A restore of this data directory did not finish. Nothing may open its stores\n\
             # until `icnctl restore --recover-incomplete` verifies it is whole again. Remove\n\
             # this file by hand only if you have recovered the directory yourself.\n",
        );
        out.push_str(&format!("phase {}\n", self.phase.as_str()));
        out.push_str(&format!("archive {}\n", encode(self.archive.as_os_str())));
        out.push_str(&format!("# archive: {:?}\n", self.archive));
        out.push_str(&format!(
            "archive-checksum {}\n",
            encode(OsStr::new(&self.archive_checksum))
        ));
        out.push_str(&format!(
            "# archive-checksum: {:?}\n",
            self.archive_checksum
        ));
        match &self.move_aside {
            Some(dir) => {
                out.push_str(&format!("move-aside {}\n", encode(dir.as_os_str())));
                out.push_str(&format!("# move-aside: {dir:?}\n"));
            }
            None => out.push_str("move-aside -\n"),
        }
        for entry in &self.inventory {
            out.push_str(&format!(
                "entry {} {} {} {}\n",
                entry.dev,
                entry.ino,
                entry
                    .birth_ns
                    .map(|ns| ns.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                encode(&entry.name)
            ));
            out.push_str(&format!("# entry: {:?}\n", entry.name));
        }
        out
    }

    /// Parse a stored record. Anything unexpected is an error: a record this
    /// cannot read is reported to the operator, never interpreted loosely.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next() != Some(HEADER) {
            bail!("it does not start with `{HEADER}`");
        }
        let (mut phase, mut archive, mut archive_checksum, mut move_aside, mut inventory) =
            (None, None, None, None, Vec::<InventoryEntry>::new());
        let once = |seen: bool, key: &str| -> Result<()> {
            if seen {
                bail!("{key} recorded twice");
            }
            Ok(())
        };
        for line in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once(' ')
                .with_context(|| format!("unreadable line {line:?}"))?;
            match key {
                "phase" => {
                    once(phase.is_some(), "phase")?;
                    phase = Some(
                        RestorePhase::parse(value)
                            .with_context(|| format!("unknown phase {value:?}"))?,
                    )
                }
                "archive" => {
                    once(archive.is_some(), "archive")?;
                    archive = Some(PathBuf::from(decode(value)?));
                }
                "archive-checksum" => {
                    once(archive_checksum.is_some(), "archive-checksum")?;
                    archive_checksum = Some(
                        decode(value)?
                            .into_string()
                            .map_err(|_| anyhow::anyhow!("archive checksum is not UTF-8"))?,
                    );
                }
                "move-aside" => {
                    once(move_aside.is_some(), "move-aside")?;
                    move_aside = Some(match value {
                        "-" => None,
                        hex => Some(PathBuf::from(decode(hex)?)),
                    });
                }
                "entry" => {
                    let mut fields = value.splitn(4, ' ');
                    let (Some(dev), Some(ino), Some(birth), Some(name)) =
                        (fields.next(), fields.next(), fields.next(), fields.next())
                    else {
                        bail!("unreadable entry {value:?}");
                    };
                    let name = decode(name)?;
                    if inventory.iter().any(|e| e.name == name) {
                        bail!("entry {name:?} recorded twice");
                    }
                    inventory.push(InventoryEntry {
                        name,
                        dev: dev
                            .parse()
                            .with_context(|| format!("bad device in {value:?}"))?,
                        ino: ino
                            .parse()
                            .with_context(|| format!("bad inode in {value:?}"))?,
                        birth_ns: match birth {
                            "-" => None,
                            ns => Some(
                                ns.parse()
                                    .with_context(|| format!("bad birth time in {value:?}"))?,
                            ),
                        },
                    });
                }
                other => bail!("unknown field {other:?}"),
            }
        }
        Ok(Self {
            phase: phase.context("no phase recorded")?,
            archive: archive.context("no archive recorded")?,
            archive_checksum: archive_checksum.context("no archive checksum recorded")?,
            move_aside: move_aside.context("no move-aside directory recorded")?,
            inventory,
        })
    }
}

/// Hex of the name's bytes: byte-exact on Unix, where names need not be
/// UTF-8.
fn encode(name: &OsStr) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt as _;
        name.as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let bytes = name.to_string_lossy().into_owned().into_bytes();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode(hex: &str) -> Result<OsString> {
    if !hex.len().is_multiple_of(2) {
        bail!("odd-length hex {hex:?}");
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| {
            hex.get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .with_context(|| format!("bad hex {hex:?}"))
        })
        .collect::<Result<Vec<u8>>>()?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        Ok(OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    Ok(OsString::from(
        String::from_utf8(bytes).context("not UTF-8")?,
    ))
}

/// The marker's path for a data root, resolved through the real directory as
/// the lock paths are, so every spelling of the root names the same marker.
pub fn restore_marker_path(root: &Path) -> PathBuf {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .join(RESTORE_INCOMPLETE_FILE_NAME)
}

/// `fsync` a directory, so a rename or unlink in it is on disk once this
/// returns -- on storage that honours it (see the module's durability note). A
/// no-op where directories cannot be opened for syncing.
pub fn sync_directory(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .with_context(|| format!("Failed to flush {} to disk", dir.display()))?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

/// Read the record of an incomplete restore of `root`, if there is one.
///
/// `Ok(None)` only when nothing is at the marker's name. A marker that is not
/// a regular file, or whose record cannot be read, is an error.
pub fn read_restore_record(root: &Path) -> Result<Option<RestoreRecord>> {
    let path = restore_marker_path(root);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("Failed to inspect {}", path.display())),
    };
    if !meta.file_type().is_file() {
        bail!("{} is not a regular file", path.display());
    }
    if meta.len() > MAX_RECORD_BYTES {
        bail!("{} is larger than any record this writes", path.display());
    }
    // Read what was inspected, and no more: the open file must be the same
    // regular file the inspection saw (not something swapped in for it since),
    // and the read is bounded whatever it turns out to be.
    let file =
        std::fs::File::open(&path).with_context(|| format!("Failed to open {}", path.display()))?;
    let opened = file
        .metadata()
        .with_context(|| format!("Failed to inspect {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if (opened.dev(), opened.ino()) != (meta.dev(), meta.ino()) {
            bail!("{} changed while it was being read", path.display());
        }
    }
    if !opened.file_type().is_file() {
        bail!("{} is not a regular file", path.display());
    }
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut std::io::Read::take(file, MAX_RECORD_BYTES + 1),
        &mut text,
    )
    .with_context(|| format!("Failed to read {}", path.display()))?;
    if text.len() as u64 > MAX_RECORD_BYTES {
        bail!("{} is larger than any record this writes", path.display());
    }
    RestoreRecord::parse(&text)
        .map(Some)
        .with_context(|| format!("{} cannot be read as a restore record", path.display()))
}

/// Proof, for one data root, that a restore marker was there when it was asked.
///
/// The only key to the marker-tolerant acquisitions
/// ([`crate::DataDirLock::acquire_for_restore_recovery`] and its configuration
/// counterpart): code that has not found a marker on that root cannot use them
/// to step around the refusal everything else gets.
#[derive(Debug)]
pub struct MarkerSeen {
    root: PathBuf,
}

impl MarkerSeen {
    /// Whether this was seen on `root`, however that root is spelled.
    pub fn is_for(&self, root: &Path) -> bool {
        std::fs::canonicalize(root).ok().as_deref() == Some(self.root.as_path())
    }
}

/// A [`MarkerSeen`] for `root` if anything is at its marker's name, `None` if
/// nothing is. Being unable to tell is an error, as everywhere here.
pub fn marker_seen(root: &Path) -> Result<Option<MarkerSeen>> {
    let path = restore_marker_path(root);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Failed to inspect {}", path.display())),
        Ok(_) => Ok(Some(MarkerSeen {
            root: std::fs::canonicalize(root)
                .with_context(|| format!("Failed to resolve {}", root.display()))?,
        })),
    }
}

/// Refuse to let `holder` proceed while a restore of `root` is incomplete.
///
/// Fail-closed: anything at the marker's name refuses, readable or not, and so
/// does being unable to tell.
pub fn refuse_if_restore_incomplete(root: &Path, holder: &str) -> Result<()> {
    let path = restore_marker_path(root);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Refusing to start {holder}: could not tell whether a restore of {} is \
                     incomplete, because {} could not be inspected",
                    root.display(),
                    path.display()
                )
            })
        }
        Ok(_) => {}
    }
    let detail = match read_restore_record(root) {
        Ok(Some(record)) => {
            let aside = record
                .move_aside
                .as_ref()
                .map(|d| format!(" Its previous contents may be in {d:?}."))
                .unwrap_or_default();
            format!("{}.{aside}", record.phase.describe())
        }
        Ok(None) => "the marker disappeared while being read".to_string(),
        Err(e) => format!("its record could not be read: {e:#}"),
    };
    bail!(
        "Refusing to start {holder}: a restore of {} did not finish; {detail}\n\
         Nothing may open this data directory's stores until it is recovered. Run \
         `icnctl --data-dir {} restore --recover-incomplete`; see \"If a restore does \
         not finish\" in the backup and recovery guide. {} is never removed automatically.",
        root.display(),
        root.display(),
        path.display()
    )
}

/// A restore's marker, held while the restore changes the root.
///
/// Removed only by [`Self::finish`]. Dropping the handle leaves the marker.
#[derive(Debug)]
pub struct RestoreInProgress {
    path: PathBuf,
    root: PathBuf,
    record: RestoreRecord,
}

impl RestoreInProgress {
    /// Record, durably, that a restore of `root` is about to change it.
    ///
    /// Refuses if a marker is already there (`O_EXCL`): an earlier restore that
    /// did not finish is recovered, never overwritten. Returns only once the
    /// record and its directory entry have been `fsync`ed, so -- on storage that
    /// honours that -- nothing the restore does afterwards can survive a crash
    /// without the marker surviving too.
    pub fn begin(root: &Path, record: RestoreRecord) -> Result<Self> {
        let path = restore_marker_path(root);
        let text = record.to_text();
        if text.len() as u64 > MAX_RECORD_BYTES {
            bail!(
                "Refusing to restore {}: its contents are too many to record ({} bytes) for a \
                 later recovery to read. Nothing has been changed.",
                root.display(),
                text.len()
            );
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = match options.open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => bail!(
                "Refusing to restore {}: {} is already there, so an earlier restore of it did not \
                 finish. Recover it first (`icnctl restore --recover-incomplete`).",
                root.display(),
                path.display()
            ),
            Err(e) => {
                return Err(e).with_context(|| format!("Failed to create {}", path.display()))
            }
        };
        let root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());
        let durable = (|| -> Result<()> {
            // `open(2)`'s mode is masked by the umask; the record must stay
            // readable by the account that will recover it.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                    .with_context(|| format!("Failed to set the mode of {}", path.display()))?;
            }
            file.write_all(text.as_bytes())
                .and_then(|()| file.sync_all())
                .with_context(|| format!("Failed to write {}", path.display()))?;
            sync_directory(&root)
        })();
        if let Err(e) = durable {
            // Nothing has changed yet, and a marker that is not fully on disk
            // would lock an intact root for nothing. This call created it
            // (`O_EXCL`), so removing it cannot take anyone else's.
            let _ = std::fs::remove_file(&path);
            let _ = sync_directory(&root);
            return Err(e.context(
                "The restore's marker could not be made durable; nothing has been changed",
            ));
        }
        Ok(Self { path, root, record })
    }

    /// Take over the marker an earlier restore left, to finish it after
    /// recovery has verified the root.
    pub fn resume(root: &Path) -> Result<Self> {
        let record = read_restore_record(root)?
            .with_context(|| format!("No restore of {} is incomplete", root.display()))?;
        let path = restore_marker_path(root);
        let root = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf());
        Ok(Self { path, root, record })
    }

    pub fn record(&self) -> &RestoreRecord {
        &self.record
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Remove the marker, durably. Call only once the root is verified: the
    /// restore finished and its data is on disk, a rollback put back exactly
    /// what was there, or a recovery confirmed it.
    pub fn finish(self) -> Result<()> {
        std::fs::remove_file(&self.path)
            .with_context(|| format!("Failed to remove {}", self.path.display()))?;
        sync_directory(&self.root).with_context(|| {
            format!(
                "{} was removed, but the removal could not be flushed to disk; after a crash \
                 it could come back",
                self.path.display()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn record() -> RestoreRecord {
        RestoreRecord {
            phase: RestorePhase::Moving,
            archive: PathBuf::from("/srv/backups/icn backup.tar"),
            archive_checksum: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
                .into(),
            move_aside: Some(PathBuf::from("/var/lib/icn.backup-7")),
            inventory: vec![
                InventoryEntry {
                    name: OsString::from("store"),
                    dev: 2049,
                    ino: 131,
                    birth_ns: Some(1_790_000_000_123_456_789),
                },
                InventoryEntry {
                    name: OsString::from("icn.toml"),
                    dev: 2049,
                    ino: 77,
                    birth_ns: None,
                },
            ],
        }
    }

    #[test]
    fn a_record_reads_back_exactly() {
        let r = record();
        assert_eq!(RestoreRecord::parse(&r.to_text()).unwrap(), r);
        let bare = RestoreRecord {
            move_aside: None,
            inventory: vec![],
            phase: RestorePhase::Extracting,
            ..record()
        };
        assert_eq!(RestoreRecord::parse(&bare.to_text()).unwrap(), bare);
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_utf8_reads_back_byte_for_byte() {
        use std::os::unix::ffi::OsStringExt as _;
        let mut r = record();
        r.inventory[0].name = OsString::from_vec(b"st\xffre with space".to_vec());
        assert_eq!(RestoreRecord::parse(&r.to_text()).unwrap(), r);
    }

    /// A name, path or archive spelled with newlines cannot add, replace or
    /// break a field: the data lines are hex, and the readable comments are
    /// written escaped.
    #[test]
    fn a_name_with_a_newline_cannot_inject_a_field() {
        let mut r = record();
        r.archive = PathBuf::from("/srv/x\nmove-aside -");
        r.archive_checksum = "x\narchive-checksum y".into();
        r.inventory[0].name = OsString::from("x\nmove-aside -\nphase extracting\n\u{1b}[2J");
        let text = r.to_text();
        assert!(
            !text.contains('\u{1b}'),
            "no raw control characters: {text:?}"
        );
        assert_eq!(text.lines().filter(|l| l.starts_with("phase ")).count(), 1);
        assert_eq!(
            text.lines()
                .filter(|l| l.starts_with("archive-checksum "))
                .count(),
            1
        );
        assert_eq!(RestoreRecord::parse(&text).unwrap(), r);
    }

    #[test]
    fn a_record_that_cannot_be_read_is_an_error_not_a_guess() {
        assert!(RestoreRecord::parse("").is_err());
        assert!(RestoreRecord::parse("something else\nphase moving\n").is_err());
        let text = record().to_text();
        assert!(RestoreRecord::parse(&text.replace("phase moving", "phase sideways")).is_err());
        assert!(RestoreRecord::parse(&text.replace("phase moving\n", "")).is_err());
        let without_checksum: String = text
            .lines()
            .filter(|l| !l.starts_with("archive-checksum "))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(RestoreRecord::parse(&without_checksum).is_err());
        assert!(RestoreRecord::parse(&format!("{text}mystery field\n")).is_err());
        // A field recorded twice -- an injected line, say -- is not "last wins".
        assert!(RestoreRecord::parse(&format!("{text}move-aside -\n")).is_err());
        assert!(RestoreRecord::parse(&format!("{text}phase extracting\n")).is_err());
        assert!(RestoreRecord::parse(&format!("{text}archive-checksum 30\n")).is_err());
        let first_entry = text.lines().find(|l| l.starts_with("entry ")).unwrap();
        assert!(RestoreRecord::parse(&format!("{text}{first_entry}\n")).is_err());
        assert!(RestoreRecord::parse(&text.replacen("entry 2049 131", "entry x 131", 1)).is_err());
        assert!(
            RestoreRecord::parse(&text.replacen(" 1790000000123456789 ", " soon ", 1)).is_err()
        );
    }

    #[test]
    fn nothing_at_the_name_lets_a_holder_proceed() {
        let dir = tempfile::TempDir::new().unwrap();
        refuse_if_restore_incomplete(dir.path(), "the daemon").unwrap();
        assert!(read_restore_record(dir.path()).unwrap().is_none());
    }

    #[test]
    fn a_marker_refuses_and_names_the_recovery() {
        let dir = tempfile::TempDir::new().unwrap();
        let marker = RestoreInProgress::begin(dir.path(), record()).unwrap();
        let err = format!(
            "{:#}",
            refuse_if_restore_incomplete(dir.path(), "the daemon").unwrap_err()
        );
        assert!(err.contains("Refusing to start the daemon"), "{err}");
        assert!(err.contains("--recover-incomplete"), "{err}");
        assert!(err.contains("/var/lib/icn.backup-7"), "{err}");
        assert!(err.contains("never removed automatically"), "{err}");
        drop(marker);
    }

    #[cfg(unix)]
    #[test]
    fn anything_at_the_name_refuses_readable_or_not() {
        let unreadable = tempfile::TempDir::new().unwrap();
        std::fs::write(
            unreadable.path().join(RESTORE_INCOMPLETE_FILE_NAME),
            b"garbage",
        )
        .unwrap();
        let err = format!(
            "{:#}",
            refuse_if_restore_incomplete(unreadable.path(), "x").unwrap_err()
        );
        assert!(err.contains("could not be read"), "{err}");

        let directory = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(directory.path().join(RESTORE_INCOMPLETE_FILE_NAME)).unwrap();
        assert!(refuse_if_restore_incomplete(directory.path(), "x").is_err());

        let dangling = tempfile::TempDir::new().unwrap();
        std::os::unix::fs::symlink(
            dangling.path().join("nowhere"),
            dangling.path().join(RESTORE_INCOMPLETE_FILE_NAME),
        )
        .unwrap();
        assert!(refuse_if_restore_incomplete(dangling.path(), "x").is_err());
    }

    /// Being unable to look is not evidence that nothing is there.
    #[cfg(unix)]
    #[test]
    fn being_unable_to_look_for_the_marker_refuses() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o600)).unwrap();
        let blind = std::fs::symlink_metadata(root.join(RESTORE_INCOMPLETE_FILE_NAME))
            .is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied);
        let verdict = refuse_if_restore_incomplete(&root, "the daemon");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        if !blind {
            eprintln!("skipped: directory permissions do not bind this process (running as root)");
            return;
        }
        let err = format!("{:#}", verdict.unwrap_err());
        assert!(err.contains("could not tell"), "{err}");
    }

    #[test]
    fn a_second_restore_cannot_overwrite_an_unfinished_one() {
        let dir = tempfile::TempDir::new().unwrap();
        let _first = RestoreInProgress::begin(dir.path(), record()).unwrap();
        let err = format!(
            "{:#}",
            RestoreInProgress::begin(dir.path(), record()).unwrap_err()
        );
        assert!(err.contains("did not finish"), "{err}");
        assert_eq!(read_restore_record(dir.path()).unwrap().unwrap(), record());
    }

    #[test]
    fn dropping_the_handle_keeps_the_marker_and_only_finish_removes_it() {
        let dir = tempfile::TempDir::new().unwrap();
        let marker = RestoreInProgress::begin(dir.path(), record()).unwrap();
        drop(marker);
        assert!(read_restore_record(dir.path()).unwrap().is_some());

        let resumed = RestoreInProgress::resume(dir.path()).unwrap();
        assert_eq!(resumed.record(), &record());
        resumed.finish().unwrap();
        assert!(read_restore_record(dir.path()).unwrap().is_none());
        refuse_if_restore_incomplete(dir.path(), "x").unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_marker_is_owner_only_whatever_the_umask() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::TempDir::new().unwrap();
        let marker = RestoreInProgress::begin(dir.path(), record()).unwrap();
        let mode = std::fs::metadata(marker.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
