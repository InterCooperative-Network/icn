//! Encoding abstraction layer for ICN serialization.
//!
//! This crate standardizes binary encoding across the workspace.
//!
//! - Wire format: postcard (no prefix) via `encode` / `decode`
//! - Storage format: version-prefixed postcard via `encode_versioned` / `decode_versioned`

use serde::{de::DeserializeOwned, Serialize};

/// Errors that can occur during encoding/decoding.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("postcard error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("unknown format version: {0}")]
    UnknownFormatVersion(u8),

    #[error("empty data")]
    EmptyData,
}

/// Result type for encoding operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Storage format version byte.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    /// Postcard format (current).
    Postcard = 1,
}

impl TryFrom<u8> for FormatVersion {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(FormatVersion::Postcard),
            v => Err(Error::UnknownFormatVersion(v)),
        }
    }
}

// ============================================================================
// Wire Protocol Encoding (no version prefix)
// ============================================================================

/// Encode a value using postcard.
#[inline]
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(postcard::to_allocvec(value)?)
}

/// Decode a value using postcard.
#[inline]
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(postcard::from_bytes(bytes)?)
}

// ============================================================================
// Storage Encoding (version-prefixed)
// ============================================================================

/// Encode a value with version prefix for persistent storage.
///
/// Output: `[version: u8][payload: postcard]`
#[inline]
pub fn encode_versioned<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = vec![FormatVersion::Postcard as u8];
    buf.extend(postcard::to_allocvec(value)?);
    Ok(buf)
}

/// Decode a version-prefixed value from persistent storage.
#[inline]
pub fn decode_versioned<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.is_empty() {
        return Err(Error::EmptyData);
    }
    let version = FormatVersion::try_from(bytes[0])?;
    let payload = &bytes[1..];

    match version {
        FormatVersion::Postcard => Ok(postcard::from_bytes(payload)?),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: u64,
        nested: Vec<u8>,
    }

    fn sample_data() -> TestData {
        TestData {
            name: "test".to_string(),
            value: 12345,
            nested: vec![1, 2, 3, 4, 5],
        }
    }

    #[test]
    fn test_wire_roundtrip() {
        let data = sample_data();
        let bytes = encode(&data).expect("encode");
        let decoded: TestData = decode(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_storage_roundtrip() {
        let data = sample_data();
        let bytes = encode_versioned(&data).expect("encode_versioned");
        let decoded: TestData = decode_versioned(&bytes).expect("decode_versioned");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_storage_empty_data_error() {
        let result = decode_versioned::<TestData>(&[]);
        assert!(matches!(result, Err(Error::EmptyData)));
    }

    #[test]
    fn test_storage_unknown_version_error() {
        let bad = vec![0xFF, 1, 2, 3];
        let result = decode_versioned::<TestData>(&bad);
        assert!(matches!(result, Err(Error::UnknownFormatVersion(0xFF))));
    }
}
