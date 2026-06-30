//! Content hashing for appliance build artifacts.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::ApplianceError;

/// Read-buffer size for streaming file hashing (64 KiB).
const HASH_BUF_LEN: usize = 64 * 1024;

/// Lowercase hex SHA-256 of `bytes`.
///
/// Matches the `sha256sum` output recorded by `deploy/appliance/build-image.sh`,
/// so an emitted manifest's hashes are comparable byte-for-byte with the shell
/// build path (and re-checkable by a later `verify` rung).
pub fn sha256_hex(bytes: &[u8]) -> String {
    to_hex(&Sha256::digest(bytes))
}

/// Lowercase hex SHA-256 of the file at `path`, computed by streaming.
///
/// Reads the file incrementally through a [`BufReader`] into the hasher instead
/// of loading it into memory, so multi-gigabyte appliance artifacts (QCOW2/raw
/// images, staged base images) hash with bounded memory. The result is identical
/// to [`sha256_hex`] of the same bytes and to `sha256sum`.
pub fn sha256_file_hex(path: impl AsRef<Path>) -> Result<String, ApplianceError> {
    let file = File::open(path.as_ref())?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; HASH_BUF_LEN];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

/// Lowercase hex encoding of a digest's bytes.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Fail-closed: stream-hash the file at `path` and compare to `expected`
/// (lowercase hex SHA-256).
///
/// Returns [`ApplianceError::MissingArtifact`] when the file is missing or
/// unreadable, and [`ApplianceError::HashMismatch`] when the digest differs.
pub fn verify_file_hash(path: &Path, expected: &str) -> Result<(), ApplianceError> {
    let actual = sha256_file_hex(path).map_err(|e| ApplianceError::MissingArtifact {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    if actual != expected {
        return Err(ApplianceError::HashMismatch {
            path: path.display().to_string(),
            expected: expected.to_string(),
            found: actual,
        });
    }
    Ok(())
}
