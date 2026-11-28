//! The import-incomplete marker: refusal side only (ADR-0089 §6, §7).
//!
//! An offline authority import writes facts one at a time. If it stops part-way, the authority
//! store holds a prefix of a segment the operator was handed, and the node's retained tip is behind
//! the facts it was given. ADR-0089 makes that state fail closed: the import creates the marker
//! before its first write and removes it only after the whole segment is durable, and
//! [`crate::authority_registry::AuthorityRegistry::open`] refuses while it exists.
//!
//! **No importer exists yet.** This module only *inspects* the marker. Nothing here creates,
//! completes or removes one, and nothing here makes an import atomic or resumable: staging, write
//! ordering, the durability walk, resume and removal are later slices of ADR-0089.
//!
//! **Existence is the safety property**, as for the restore marker. Any entry at the marker's name —
//! a regular file of any content, a directory, a symlink (dangling or not), a FIFO, a socket, a
//! device — refuses, and so does any error other than "not found" while looking. Only a confirmed
//! `NotFound` from `symlink_metadata` is absence. The content is read only to name the import in
//! the refusal; content that cannot be read is reported as unreadable, never treated as absent. The
//! marker is never followed, never opened unless it is a regular file, never read past
//! [`MAX_IMPORT_MARKER_BYTES`], and never modified or removed here.
//!
//! The guarantees above are Unix ones: elsewhere the marker is opened without `O_NOFOLLOW` or
//! `O_NONBLOCK` and without the identity re-check. ICN nodes run on Unix.
//!
//! The import marker is separate from the restore marker
//! ([`crate::restore_marker`]): it is not checked at data-directory lock acquisition (resume must
//! take the lock), only when the authority registry opens.
//!
//! # Record format (PROPOSED v1, not frozen)
//!
//! The reader is crate-private on purpose: the grammar is not a public API. Only the approved
//! names and [`import_marker_path`] are public.
//!
//! The file name and header were approved by the maintainer for the fixture-only implementation.
//! The byte grammar below is a proposal; it is not a published format until it is reviewed and
//! approved (ADR-0089 "Irreversible commitments" #5).
//!
//! ```text
//! record     = header LF *( line LF )         ; 1 ..= 65536 bytes, every byte 0x20-0x7E or 0x0A
//! header     = "icn-import-incomplete v1"
//! line       = comment / field                ; no empty lines
//! comment    = "#" *( %x20-7E )               ; ignored by readers
//! field      = key SP value
//! key        = 1*( %x61-7A / DIGIT / "-" )    ; lowercase, digits, hyphen
//! value      = 1*( %x21-7E )                  ; no spaces
//! ```
//!
//! Every key appears at most once. The first three fields, in this order, are the **identity
//! prefix**:
//!
//! - `segment-id` — 64 lowercase hex: `SHA-256` of the canonical segment bytes (ADR-0089 §3).
//! - `subject` — 64 lowercase hex: the Subject's `SubjectId` bytes.
//! - `fact-count` — canonical decimal `u32`, at least 1: no sign, no leading zero. It is not
//!   bounded by the grammar: the per-import fact bound is provisional (ADR-0089 §8), and a
//!   marker written under one bound must stay readable under another. Resume checks it against
//!   the staged segment and applies the compiled bound as policy; nothing sizes an allocation
//!   from it.
//!
//! Fields after the identity prefix carry the retention entry and bounds snapshot (ADR-0089 §6,
//! §8). Their keys and encodings are **not yet specified**, because the retention file has no
//! format yet. This reader checks them lexically (grammar, no duplicates) and does not interpret
//! them; the future resume path must parse them fully.
//!
//! A record that does not satisfy the above, at any byte, is unreadable. A different version line
//! (`icn-import-incomplete v2`) is unreadable to this version: an older reader refuses rather than
//! guess.

use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};

use icn_identity::authority_log::SubjectId;

/// File name of the import-incomplete marker, inside the data directory.
pub const IMPORT_INCOMPLETE_FILE_NAME: &str = ".icn-import-incomplete";

/// File name of the staged segment copy, inside the data directory (ADR-0089 §6 step 2).
///
/// Reserved here so the name is defined once. Nothing reads or writes it yet.
pub const IMPORT_STAGED_FILE_NAME: &str = ".icn-import-staged";

/// First line of a v1 marker record.
pub(crate) const IMPORT_MARKER_HEADER: &str = "icn-import-incomplete v1";

/// Largest marker record read (ADR-0089: the marker record is parsed with a 64 KiB bound).
pub(crate) const MAX_IMPORT_MARKER_BYTES: usize = 64 * 1024;

const IDENTITY_KEYS: [&str; 3] = ["segment-id", "subject", "fact-count"];

/// The marker's path for a data root, resolved through the real directory as the restore
/// marker's is, so every spelling of the root names the same marker.
pub fn import_marker_path(root: &Path) -> PathBuf {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .join(IMPORT_INCOMPLETE_FILE_NAME)
}

/// The identity prefix of a readable marker: which import did not finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportMarkerIdentity {
    /// `SHA-256` of the canonical segment bytes being imported.
    pub(crate) segment_id: [u8; 32],
    /// The Subject the segment belongs to.
    pub(crate) subject: SubjectId,
    /// The number of facts the segment carries.
    pub(crate) fact_count: u32,
}

/// What is at the marker's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImportMarkerState {
    /// `symlink_metadata` reported `NotFound`. The only state that is not a refusal.
    Absent,
    /// A regular file whose record is readable: an import did not finish.
    Incomplete(ImportMarkerIdentity),
    /// Something is there, or looking failed, and it could not be read as a record. Still a
    /// refusal: an unreadable marker is not an absent one.
    Unreadable(String),
}

/// Inspect the marker for `root` without following it, blocking on it, or changing it.
pub(crate) fn inspect_import_marker(root: &Path) -> ImportMarkerState {
    // Resolve the root first, as the restore marker's refusal path does: a root that cannot be
    // resolved is not evidence that no marker is in it.
    let path = match std::fs::canonicalize(root) {
        Ok(real) => real.join(IMPORT_INCOMPLETE_FILE_NAME),
        Err(e) => {
            return ImportMarkerState::Unreadable(format!(
                "the data directory could not be resolved: {e}"
            ))
        }
    };
    let metadata = match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ImportMarkerState::Absent,
        Err(e) => return ImportMarkerState::Unreadable(format!("it could not be inspected: {e}")),
        Ok(metadata) => metadata,
    };
    if !metadata.file_type().is_file() {
        return ImportMarkerState::Unreadable(format!(
            "it is {}, not a regular file",
            describe(&metadata)
        ));
    }
    let bytes = match read_bounded(&path, &metadata) {
        Ok(bytes) => bytes,
        Err(reason) => return ImportMarkerState::Unreadable(reason),
    };
    match parse_import_marker(&bytes) {
        Ok(identity) => ImportMarkerState::Incomplete(identity),
        Err(reason) => ImportMarkerState::Unreadable(reason),
    }
}

/// Parse a marker record's bytes under the proposed v1 grammar (see the module docs).
pub(crate) fn parse_import_marker(bytes: &[u8]) -> Result<ImportMarkerIdentity, String> {
    if bytes.is_empty() {
        return Err("it is empty".to_string());
    }
    if bytes.len() > MAX_IMPORT_MARKER_BYTES {
        return Err(format!("it is larger than {MAX_IMPORT_MARKER_BYTES} bytes"));
    }
    if let Some(byte) = bytes
        .iter()
        .find(|&&b| !(b == b'\n' || (0x20..=0x7e).contains(&b)))
    {
        return Err(format!(
            "it contains byte 0x{byte:02x}, outside printable ASCII and line feed"
        ));
    }
    if bytes.last() != Some(&b'\n') {
        return Err("its last line is not terminated (truncated)".to_string());
    }
    let text = std::str::from_utf8(&bytes[..bytes.len() - 1])
        .map_err(|e| format!("it is not ASCII: {e}"))?;
    let mut lines = text.split('\n');
    match lines.next() {
        Some(IMPORT_MARKER_HEADER) => {}
        Some(other) if other.starts_with("icn-import-incomplete ") => {
            return Err(format!(
                "its version line {} is not supported",
                excerpt(other)
            ))
        }
        _ => return Err(format!("it does not start with `{IMPORT_MARKER_HEADER}`")),
    }

    let mut seen = BTreeSet::new();
    let mut fields = Vec::new();
    for line in lines {
        if line.is_empty() {
            return Err("it contains an empty line".to_string());
        }
        if line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(' ')
            .ok_or_else(|| format!("line {} is not `key value`", excerpt(line)))?;
        let key_ok = !key.is_empty()
            && key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        let value_ok = !value.is_empty() && value.bytes().all(|b| (0x21..=0x7e).contains(&b));
        if !key_ok || !value_ok {
            return Err(format!("line {} is not `key value`", excerpt(line)));
        }
        if !seen.insert(key) {
            return Err(format!("`{}` is recorded twice", excerpt(key)));
        }
        fields.push((key, value));
    }

    let mut identity = fields.iter();
    let mut next = |expected: &str| match identity.next() {
        Some((key, value)) if *key == expected => Ok(*value),
        Some((key, _)) => Err(format!("expected `{expected}`, found {}", excerpt(key))),
        None => Err(format!("`{expected}` is missing")),
    };
    let segment_id = hex32(next(IDENTITY_KEYS[0])?, IDENTITY_KEYS[0])?;
    let subject = SubjectId::from_bytes(hex32(next(IDENTITY_KEYS[1])?, IDENTITY_KEYS[1])?);
    let fact_count = fact_count(next(IDENTITY_KEYS[2])?)?;
    // Fields after the identity prefix are checked lexically above and not interpreted here: their
    // keys and encodings are not specified yet (see the module docs).
    Ok(ImportMarkerIdentity {
        segment_id,
        subject,
        fact_count,
    })
}

fn hex32(value: &str, key: &str) -> Result<[u8; 32], String> {
    let lowercase_hex = value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !lowercase_hex {
        return Err(format!("`{key}` is not 64 lowercase hex digits"));
    }
    let mut out = [0u8; 32];
    hex::decode_to_slice(value, &mut out).map_err(|e| format!("`{key}` is not hex: {e}"))?;
    Ok(out)
}

fn fact_count(value: &str) -> Result<u32, String> {
    let canonical = (1..=10).contains(&value.len())
        && value.bytes().all(|b| b.is_ascii_digit())
        && !value.starts_with('0');
    match canonical.then(|| value.parse::<u32>().ok()).flatten() {
        Some(n) => Ok(n),
        None => Err(format!(
            "`fact-count` {} is not a decimal u32 of at least 1 without leading zeros",
            excerpt(value)
        )),
    }
}

/// At most 64 characters of marker content, `Debug`-quoted, for a refusal reason: the content is
/// whatever is at the marker's name, so a reason never echoes more than this of it.
fn excerpt(text: &str) -> String {
    const LIMIT: usize = 64;
    match text.char_indices().nth(LIMIT) {
        Some((cut, _)) => format!("{:?}…", &text[..cut]),
        None => format!("{text:?}"),
    }
}

/// Read at most `MAX_IMPORT_MARKER_BYTES + 1` bytes from the regular file `metadata` describes,
/// refusing if what opens is not that file.
fn read_bounded(path: &Path, metadata: &Metadata) -> Result<Vec<u8>, String> {
    let file = open_without_following(path).map_err(|e| format!("it could not be opened: {e}"))?;
    let opened = file
        .metadata()
        .map_err(|e| format!("it could not be inspected once open: {e}"))?;
    if !opened.file_type().is_file() || !same_file(metadata, &opened) {
        return Err("it changed while being read".to_string());
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_MARKER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("it could not be read: {e}"))?;
    if bytes.len() > MAX_IMPORT_MARKER_BYTES {
        return Err(format!("it is larger than {MAX_IMPORT_MARKER_BYTES} bytes"));
    }
    Ok(bytes)
}

/// Open read-only without following a final symlink, without blocking on a FIFO, and without
/// acquiring a controlling terminal. The caller checks the opened descriptor's type and identity.
#[cfg(unix)]
fn open_without_following(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY)
        .open(path)
}

#[cfg(not(unix))]
fn open_without_following(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(unix)]
fn same_file(before: &Metadata, after: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    before.dev() == after.dev() && before.ino() == after.ino()
}

#[cfg(not(unix))]
fn same_file(_before: &Metadata, _after: &Metadata) -> bool {
    true
}

fn describe(metadata: &Metadata) -> &'static str {
    let file_type = metadata.file_type();
    if file_type.is_dir() {
        return "a directory";
    }
    if file_type.is_symlink() {
        return "a symbolic link";
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt as _;
        if file_type.is_fifo() {
            return "a FIFO";
        }
        if file_type.is_socket() {
            return "a socket";
        }
        if file_type.is_block_device() || file_type.is_char_device() {
            return "a device";
        }
    }
    "an entry of an unexpected type"
}

#[cfg(test)]
mod tests {
    // What is at the marker's name, and how it is read. The one property under test: only a
    // confirmed absence is absence; every other state refuses, and inspecting never changes the
    // marker. Vectors: `tests/reference/import_marker_vectors.txt`, also classified by the
    // independent `tests/reference/import_marker_reference.py`. Fixture values only.

    use std::path::Path;

    use super::*;

    const VECTORS: &str = include_str!("../tests/reference/import_marker_vectors.txt");
    const SUBJECT: &str = "a0068f2e8e052f31533edaac720f0911ef32aa24a4b056106b7bbb7002b4c04d";
    const SEGMENT: &str = "f320220a862f9e626da7edb0837e18198539ea27f04919d94441cc61897468ba";

    fn valid_record() -> Vec<u8> {
        format!("{IMPORT_MARKER_HEADER}\nsegment-id {SEGMENT}\nsubject {SUBJECT}\nfact-count 3\n")
            .into_bytes()
    }

    fn classify(bytes: &[u8]) -> String {
        match parse_import_marker(bytes) {
            Ok(id) => format!(
                "incomplete:{}:{}:{}",
                id.subject,
                hex::encode(id.segment_id),
                id.fact_count
            ),
            Err(_) => "unreadable".to_string(),
        }
    }

    /// Bytes, inode and modification time of the marker: what "unchanged" means here.
    #[cfg(unix)]
    fn fingerprint(path: &Path) -> (Vec<u8>, u64, i64, i64) {
        use std::os::unix::fs::MetadataExt as _;
        let meta = std::fs::symlink_metadata(path).unwrap();
        let bytes = if meta.file_type().is_file() {
            std::fs::read(path).unwrap()
        } else {
            Vec::new()
        };
        (bytes, meta.ino(), meta.mtime(), meta.mtime_nsec())
    }

    // ── the names ───────────────────────────────────────────────────────────────────────────────────

    #[test]
    fn the_names_are_the_approved_top_level_names() {
        assert_eq!(IMPORT_INCOMPLETE_FILE_NAME, ".icn-import-incomplete");
        assert_eq!(IMPORT_STAGED_FILE_NAME, ".icn-import-staged");
        assert_eq!(IMPORT_MARKER_HEADER, "icn-import-incomplete v1");
        assert_eq!(MAX_IMPORT_MARKER_BYTES, 65536);
        // Distinct from the restore marker, which keeps its own contract.
        assert_ne!(
            IMPORT_INCOMPLETE_FILE_NAME,
            crate::restore_marker::RESTORE_INCOMPLETE_FILE_NAME
        );
        let dir = tempfile::tempdir().unwrap();
        let path = import_marker_path(dir.path());
        assert_eq!(
            path.parent().unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn every_spelling_of_the_root_names_the_same_marker() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("data");
        std::fs::create_dir(&real).unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        std::fs::write(real.join(IMPORT_INCOMPLETE_FILE_NAME), valid_record()).unwrap();

        assert_eq!(import_marker_path(&alias), import_marker_path(&real));
        assert!(matches!(
            inspect_import_marker(&alias),
            ImportMarkerState::Incomplete(_)
        ));
    }

    // ── the record (shared vectors) ─────────────────────────────────────────────────────────────────

    #[test]
    fn every_shared_vector_classifies_as_recorded() {
        let mut count = 0;
        for line in VECTORS.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('\t');
            let (name, expected, hexed) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            assert!(parts.next().is_none(), "{name}: malformed vector line");
            let bytes = hex::decode(hexed).unwrap();
            assert_eq!(classify(&bytes), expected, "vector {name}");
            count += 1;
        }
        // Not vacuous: the file holds 6 readable and 33 unreadable vectors.
        assert_eq!(count, 39);
        assert_eq!(
            VECTORS
                .lines()
                .filter(|l| l.contains("\tincomplete:"))
                .count(),
            6
        );
    }

    #[test]
    fn the_size_bound_is_exactly_64_kib() {
        let prefix = format!(
            "{IMPORT_MARKER_HEADER}\nsegment-id {}\nsubject {}\nfact-count 1\n",
            "ab".repeat(32),
            "cd".repeat(32)
        );
        let padded = |total: usize| {
            let mut record = prefix.clone().into_bytes();
            record.push(b'#');
            record.extend(std::iter::repeat_n(b'x', total - prefix.len() - 2));
            record.push(b'\n');
            assert_eq!(record.len(), total);
            record
        };
        assert_eq!(
            classify(&padded(MAX_IMPORT_MARKER_BYTES)),
            format!("incomplete:{}:{}:1", "cd".repeat(32), "ab".repeat(32))
        );
        assert_eq!(classify(&padded(MAX_IMPORT_MARKER_BYTES + 1)), "unreadable");
    }

    #[test]
    fn a_readable_record_names_its_subject_and_segment() {
        let identity = parse_import_marker(&valid_record()).unwrap();
        assert_eq!(identity.subject.to_string(), SUBJECT);
        assert_eq!(hex::encode(identity.segment_id), SEGMENT);
        assert_eq!(identity.fact_count, 3);
    }

    // ── what is at the name ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn only_a_confirmed_absence_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(inspect_import_marker(dir.path()), ImportMarkerState::Absent);
    }

    #[test]
    fn a_readable_marker_is_an_incomplete_import() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(import_marker_path(dir.path()), valid_record()).unwrap();
        match inspect_import_marker(dir.path()) {
            ImportMarkerState::Incomplete(identity) => {
                assert_eq!(identity.subject.to_string(), SUBJECT)
            }
            other => panic!("expected an incomplete import, got {other:?}"),
        }
    }

    /// Every non-absent state other than a readable record is `Unreadable`, with a reason naming it.
    fn assert_unreadable(dir: &Path, reason_contains: &str) {
        match inspect_import_marker(dir) {
            ImportMarkerState::Unreadable(reason) => assert!(
                reason.contains(reason_contains),
                "reason {reason:?} should mention {reason_contains:?}"
            ),
            other => panic!("expected an unreadable marker, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_malformed_or_oversized_file_refuses_as_unreadable() {
        let cases: [(&str, Vec<u8>, &str); 4] = [
            ("empty", Vec::new(), "empty"),
            ("garbage", b"not a marker\n".to_vec(), "does not start with"),
            ("truncated", valid_record()[..40].to_vec(), "not terminated"),
            (
                "oversized",
                vec![b'#'; MAX_IMPORT_MARKER_BYTES + 1],
                "larger than",
            ),
        ];
        for (name, bytes, reason) in cases {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(import_marker_path(dir.path()), &bytes).unwrap();
            assert_unreadable(dir.path(), reason);
            assert_eq!(
                std::fs::read(import_marker_path(dir.path())).unwrap(),
                bytes,
                "{name}: the marker must be left as it was"
            );
        }
    }

    #[test]
    fn a_directory_at_the_name_refuses() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(import_marker_path(dir.path())).unwrap();
        assert_unreadable(dir.path(), "a directory");
        assert!(import_marker_path(dir.path()).is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_at_the_name_refuses_and_is_never_followed() {
        let dir = tempfile::tempdir().unwrap();
        // The link points at a perfectly valid record. Following it would read that record as this
        // node's marker; refusing as unreadable shows it was not followed.
        let target = dir.path().join("elsewhere");
        std::fs::write(&target, valid_record()).unwrap();
        std::os::unix::fs::symlink(&target, import_marker_path(dir.path())).unwrap();
        assert_unreadable(dir.path(), "a symbolic link");
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_at_the_name_refuses() {
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(dir.path().join("nowhere"), import_marker_path(dir.path()))
            .unwrap();
        assert!(
            !import_marker_path(dir.path()).exists(),
            "fixture: dangling"
        );
        assert_unreadable(dir.path(), "a symbolic link");
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_at_the_name_refuses_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let path = import_marker_path(dir.path());
        let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid NUL-terminated path and a mode; mkfifo has no other preconditions.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);

        // Nothing writes to the FIFO, so an open that blocked would hang this test. Bound it.
        let root = dir.path().to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(inspect_import_marker(&root)).unwrap());
        let state = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("inspecting a FIFO must not block");
        assert!(
            matches!(&state, ImportMarkerState::Unreadable(r) if r.contains("a FIFO")),
            "{state:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_at_the_name_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let _listener =
            std::os::unix::net::UnixListener::bind(import_marker_path(dir.path())).unwrap();
        assert_unreadable(dir.path(), "a socket");
    }

    #[test]
    fn an_inspection_error_other_than_not_found_refuses() {
        // The "data directory" is a regular file, so looking up a name inside it fails with
        // `ENOTDIR`, which is not absence.
        let dir = tempfile::tempdir().unwrap();
        let not_a_dir = dir.path().join("file");
        std::fs::write(&not_a_dir, b"").unwrap();
        assert_unreadable(&not_a_dir, "could not be inspected");
    }

    #[cfg(unix)]
    #[test]
    fn a_marker_this_account_cannot_read_refuses() {
        use std::os::unix::fs::PermissionsExt as _;
        // SAFETY: geteuid has no preconditions.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipped: root reads a mode-000 file");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = import_marker_path(dir.path());
        std::fs::write(&path, valid_record()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert_unreadable(dir.path(), "could not be opened");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inspecting_never_changes_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = import_marker_path(dir.path());
        for bytes in [valid_record(), b"garbage".to_vec()] {
            std::fs::write(&path, bytes).unwrap();
            let before = fingerprint(&path);
            for _ in 0..3 {
                let _ = inspect_import_marker(dir.path());
            }
            assert_eq!(fingerprint(&path), before);
        }
    }
}
