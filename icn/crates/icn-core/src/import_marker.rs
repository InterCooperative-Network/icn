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
pub const IMPORT_MARKER_HEADER: &str = "icn-import-incomplete v1";

/// Largest marker record read (ADR-0089: the marker record is parsed with a 64 KiB bound).
pub const MAX_IMPORT_MARKER_BYTES: usize = 64 * 1024;

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
pub struct ImportMarkerIdentity {
    /// `SHA-256` of the canonical segment bytes being imported.
    pub segment_id: [u8; 32],
    /// The Subject the segment belongs to.
    pub subject: SubjectId,
    /// The number of facts the segment carries.
    pub fact_count: u32,
}

/// What is at the marker's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportMarkerState {
    /// `symlink_metadata` reported `NotFound`. The only state that is not a refusal.
    Absent,
    /// A regular file whose record is readable: an import did not finish.
    Incomplete(ImportMarkerIdentity),
    /// Something is there, or looking failed, and it could not be read as a record. Still a
    /// refusal: an unreadable marker is not an absent one.
    Unreadable(String),
}

/// Inspect the marker for `root` without following it, blocking on it, or changing it.
pub fn inspect_import_marker(root: &Path) -> ImportMarkerState {
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
pub fn parse_import_marker(bytes: &[u8]) -> Result<ImportMarkerIdentity, String> {
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
