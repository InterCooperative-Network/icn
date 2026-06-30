//! Content hashing for appliance build artifacts.

use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `bytes`.
///
/// Matches the `sha256sum` output recorded by `deploy/appliance/build-image.sh`,
/// so an emitted manifest's hashes are comparable byte-for-byte with the shell
/// build path (and re-checkable by a later `verify` rung).
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
