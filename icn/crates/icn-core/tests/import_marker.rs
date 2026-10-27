//! The import-incomplete marker, refusal side (ADR-0089 §6, §7): what is at the marker's name, and
//! how it is read.
//!
//! The one property under test is that only a confirmed absence is absence. Every other state —
//! any content, any entry type, any error while looking — is a refusal, and inspecting never
//! changes the marker. The record grammar is PROPOSED v1 (see `src/import_marker.rs`); its vectors
//! live in `tests/reference/import_marker_vectors.txt`, which the independent reader
//! `tests/reference/import_marker_reference.py` classifies from the grammar alone.
//!
//! Fixture values only: the subject and segment id are the merged PR1 history-segment vectors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use icn_core::import_marker::{
    import_marker_path, inspect_import_marker, parse_import_marker, ImportMarkerState,
    IMPORT_INCOMPLETE_FILE_NAME, IMPORT_MARKER_HEADER, IMPORT_STAGED_FILE_NAME,
    MAX_IMPORT_MARKER_BYTES,
};

const VECTORS: &str = include_str!("reference/import_marker_vectors.txt");
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
        icn_core::restore_marker::RESTORE_INCOMPLETE_FILE_NAME
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
    std::os::unix::fs::symlink(dir.path().join("nowhere"), import_marker_path(dir.path())).unwrap();
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
    let _listener = std::os::unix::net::UnixListener::bind(import_marker_path(dir.path())).unwrap();
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
