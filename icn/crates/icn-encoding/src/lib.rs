//! Encoding abstraction layer for ICN serialization.
//!
//! This crate provides a unified API for binary serialization, supporting
//! gradual migration from bincode to postcard (RUSTSEC-2025-0141).
//!
//! # Format Versions
//!
//! For persistent storage, use version-prefixed encoding:
//! - `0x00`: bincode legacy format (for backward compatibility)
//! - `0x01`: postcard format (new default)
//!
//! # Example
//!
//! ```
//! use icn_encoding::{encode, decode};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct MyData {
//!     value: u32,
//! }
//!
//! let data = MyData { value: 42 };
//! let bytes = encode(&data).unwrap();
//! let decoded: MyData = decode(&bytes).unwrap();
//! assert_eq!(data, decoded);
//! ```

use serde::{de::DeserializeOwned, Serialize};

/// Errors that can occur during encoding/decoding.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("bincode encode error: {0}")]
    BincodeEncode(#[from] bincode::error::EncodeError),

    #[error("bincode decode error: {0}")]
    BincodeDecode(#[from] bincode::error::DecodeError),

    #[error("postcard error: {0}")]
    Postcard(#[from] postcard::Error),

    #[error("unknown format version: {0}")]
    UnknownFormatVersion(u8),

    #[error("empty data")]
    EmptyData,
}

/// Result type for encoding operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Format version byte for version-prefixed storage.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    /// Original bincode legacy format.
    BincodeLegacy = 0,
    /// New postcard format (default for new data).
    Postcard = 1,
}

impl TryFrom<u8> for FormatVersion {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(FormatVersion::BincodeLegacy),
            1 => Ok(FormatVersion::Postcard),
            v => Err(Error::UnknownFormatVersion(v)),
        }
    }
}

// ============================================================================
// Wire Protocol Encoding (no version prefix)
// ============================================================================

/// Encode a value using postcard (new default for wire protocol).
///
/// Use this for network messages when both peers support postcard.
#[inline]
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(postcard::to_allocvec(value)?)
}

/// Decode a value using postcard.
///
/// Use this for network messages when both peers support postcard.
#[inline]
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    Ok(postcard::from_bytes(bytes)?)
}

/// Encode a value using bincode legacy format (for backward compatibility).
///
/// Use this when communicating with peers that don't support postcard.
/// Uses `bincode::serde` for compatibility with types using only serde derives.
#[inline]
pub fn encode_bincode_legacy<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(bincode::serde::encode_to_vec(
        value,
        bincode::config::legacy(),
    )?)
}

/// Decode a value using bincode legacy format.
///
/// Use this when receiving from peers that don't support postcard.
/// Uses `bincode::serde` for compatibility with types using only serde derives.
#[inline]
pub fn decode_bincode_legacy<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    let (value, _) = bincode::serde::decode_from_slice(bytes, bincode::config::legacy())?;
    Ok(value)
}

// ============================================================================
// Storage Encoding (version-prefixed for backward compatibility)
// ============================================================================

/// Encode a value with version prefix for persistent storage.
///
/// The output format is: `[version: u8][payload: variable]`
///
/// New data is encoded with postcard (version 1). The version prefix allows
/// reading old bincode data while writing new postcard data.
#[inline]
pub fn encode_versioned<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = vec![FormatVersion::Postcard as u8];
    buf.extend(postcard::to_allocvec(value)?);
    Ok(buf)
}

/// Decode a version-prefixed value from persistent storage.
///
/// Supports both old bincode format (version 0) and new postcard format (version 1).
/// Uses `bincode::serde` for bincode compatibility with types using only serde derives.
#[inline]
pub fn decode_versioned<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.is_empty() {
        return Err(Error::EmptyData);
    }

    let version = FormatVersion::try_from(bytes[0])?;
    let payload = &bytes[1..];

    match version {
        FormatVersion::BincodeLegacy => {
            let (value, _) = bincode::serde::decode_from_slice(payload, bincode::config::legacy())?;
            Ok(value)
        }
        FormatVersion::Postcard => Ok(postcard::from_bytes(payload)?),
    }
}

/// Encode using bincode legacy with version prefix (for testing/migration).
/// Uses `bincode::serde` for compatibility with types using only serde derives.
#[inline]
pub fn encode_versioned_bincode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = vec![FormatVersion::BincodeLegacy as u8];
    buf.extend(bincode::serde::encode_to_vec(
        value,
        bincode::config::legacy(),
    )?);
    Ok(buf)
}

// ============================================================================
// Migration Helpers
// ============================================================================

/// Check if data uses the legacy bincode format.
#[inline]
pub fn is_legacy_format(bytes: &[u8]) -> bool {
    bytes.first() == Some(&(FormatVersion::BincodeLegacy as u8))
}

/// Check if data uses the new postcard format.
#[inline]
pub fn is_postcard_format(bytes: &[u8]) -> bool {
    bytes.first() == Some(&(FormatVersion::Postcard as u8))
}

/// Re-encode data from legacy bincode to postcard format.
///
/// Returns `None` if data is already in postcard format.
/// Returns `Err` if decoding or re-encoding fails.
pub fn migrate_to_postcard<T>(bytes: &[u8]) -> Result<Option<Vec<u8>>>
where
    T: DeserializeOwned + Serialize,
{
    if bytes.is_empty() {
        return Err(Error::EmptyData);
    }

    if !is_legacy_format(bytes) {
        return Ok(None); // Already postcard or unknown
    }

    // Decode from bincode
    let value: T = decode_versioned(bytes)?;

    // Re-encode as postcard
    let new_bytes = encode_versioned(&value)?;

    Ok(Some(new_bytes))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bincode::Encode, bincode::Decode)]
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
    fn test_wire_encode_decode() {
        let data = sample_data();
        let bytes = encode(&data).expect("encode");
        let decoded: TestData = decode(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_bincode_legacy_encode_decode() {
        let data = sample_data();
        let bytes = encode_bincode_legacy(&data).expect("encode");
        let decoded: TestData = decode_bincode_legacy(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_versioned_postcard() {
        let data = sample_data();
        let bytes = encode_versioned(&data).expect("encode");

        assert!(is_postcard_format(&bytes));
        assert!(!is_legacy_format(&bytes));

        let decoded: TestData = decode_versioned(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_versioned_bincode_legacy() {
        let data = sample_data();
        let bytes = encode_versioned_bincode(&data).expect("encode");

        assert!(is_legacy_format(&bytes));
        assert!(!is_postcard_format(&bytes));

        let decoded: TestData = decode_versioned(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_cross_format_compatibility() {
        let data = sample_data();

        // Encode with bincode legacy
        let bincode_bytes = encode_versioned_bincode(&data).expect("bincode encode");

        // Decode with versioned decoder (should auto-detect bincode)
        let decoded: TestData = decode_versioned(&bincode_bytes).expect("decode");
        assert_eq!(data, decoded);

        // Encode with postcard
        let postcard_bytes = encode_versioned(&data).expect("postcard encode");

        // Decode with versioned decoder (should auto-detect postcard)
        let decoded2: TestData = decode_versioned(&postcard_bytes).expect("decode");
        assert_eq!(data, decoded2);
    }

    #[test]
    fn test_migrate_to_postcard() {
        let data = sample_data();

        // Create legacy bincode data
        let legacy_bytes = encode_versioned_bincode(&data).expect("encode");
        assert!(is_legacy_format(&legacy_bytes));

        // Migrate to postcard
        let migrated = migrate_to_postcard::<TestData>(&legacy_bytes)
            .expect("migrate")
            .expect("should return Some for legacy data");

        assert!(is_postcard_format(&migrated));

        // Verify data integrity after migration
        let decoded: TestData = decode_versioned(&migrated).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_migrate_already_postcard() {
        let data = sample_data();

        // Create postcard data
        let postcard_bytes = encode_versioned(&data).expect("encode");

        // Try to migrate - should return None (already postcard)
        let result = migrate_to_postcard::<TestData>(&postcard_bytes).expect("migrate");
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_data_error() {
        let result = decode_versioned::<TestData>(&[]);
        assert!(matches!(result, Err(Error::EmptyData)));
    }

    #[test]
    fn test_unknown_version_error() {
        let bad_data = vec![0xFF, 1, 2, 3];
        let result = decode_versioned::<TestData>(&bad_data);
        assert!(matches!(result, Err(Error::UnknownFormatVersion(0xFF))));
    }

    #[test]
    fn test_postcard_is_more_compact() {
        let data = sample_data();

        let bincode_bytes = encode_bincode_legacy(&data).expect("bincode");
        let postcard_bytes = encode(&data).expect("postcard");

        // Postcard is generally more compact for simple structures
        // This test documents the size difference
        println!(
            "bincode: {} bytes, postcard: {} bytes",
            bincode_bytes.len(),
            postcard_bytes.len()
        );

        // Both should successfully roundtrip
        let decoded_bincode: TestData = decode_bincode_legacy(&bincode_bytes).expect("decode");
        let decoded_postcard: TestData = decode(&postcard_bytes).expect("decode");

        assert_eq!(data, decoded_bincode);
        assert_eq!(data, decoded_postcard);
    }
}
