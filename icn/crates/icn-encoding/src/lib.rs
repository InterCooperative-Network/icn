//! Versioned encoding/decoding for ICN storage layer
//!
//! This module provides backward-compatible serialization migration from bincode to postcard.
//!
//! ## Format
//!
//! All encoded data starts with a single format byte:
//! - `0x00`: Legacy bincode format (pre-migration)
//! - `0x01`: Postcard format (current)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use icn_encoding::{encode_versioned, decode_versioned};
//!
//! // Encode (always uses postcard format 0x01)
//! let data = MyStruct { ... };
//! let bytes = encode_versioned(&data)?;
//!
//! // Decode (automatically handles both formats)
//! let decoded: MyStruct = decode_versioned(&bytes)?;
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Format version byte prefixes
const FORMAT_BINCODE: u8 = 0x00;
const FORMAT_POSTCARD: u8 = 0x01;

/// Encoding errors
#[derive(Debug, Error)]
pub enum EncodingError {
    #[error("Bincode encoding error: {0}")]
    BincodeEncode(#[from] bincode::error::EncodeError),

    #[error("Bincode decoding error: {0}")]
    BincodeDecode(#[from] bincode::error::DecodeError),

    #[error("Postcard encoding error: {0}")]
    PostcardEncode(#[from] postcard::Error),

    #[error("Unknown format version: {0:#x}")]
    UnknownFormat(u8),

    #[error("Empty data buffer")]
    EmptyBuffer,
}

pub type Result<T> = std::result::Result<T, EncodingError>;

/// Encode data using postcard format with version prefix
///
/// Output format: `[0x01, ...postcard_data]`
pub fn encode_versioned<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = vec![FORMAT_POSTCARD];
    let encoded = postcard::to_allocvec(value)?;
    buf.extend_from_slice(&encoded);
    Ok(buf)
}

/// Decode data, automatically detecting format version
///
/// Supports:
/// - Legacy data: Raw bincode format (no version prefix)
/// - Format 0x00: Bincode with version prefix (for future use)
/// - Format 0x01: Postcard format
///
/// Migration strategy: If first byte is not a known format marker (0x00 or 0x01),
/// treats the entire buffer as legacy bincode data.
pub fn decode_versioned<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T> {
    if data.is_empty() {
        return Err(EncodingError::EmptyBuffer);
    }

    let format = data[0];

    match format {
        FORMAT_BINCODE => {
            // Versioned bincode format: decode using bincode, skip version byte
            let payload = &data[1..];
            let (decoded, _) =
                bincode::serde::decode_from_slice(payload, bincode::config::legacy())?;
            Ok(decoded)
        }
        FORMAT_POSTCARD => {
            // Current format: decode using postcard, skip version byte
            let payload = &data[1..];
            let decoded = postcard::from_bytes(payload)?;
            Ok(decoded)
        }
        _ => {
            // Unknown format byte OR legacy data without version prefix
            // Try decoding entire buffer as legacy bincode
            let (decoded, _) =
                bincode::serde::decode_from_slice(data, bincode::config::legacy())?;
            Ok(decoded)
        }
    }
}

/// Encode data using legacy bincode format with version prefix (for testing/migration)
///
/// Output format: `[0x00, ...bincode_data]`
#[cfg(test)]
fn encode_versioned_bincode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut buf = vec![FORMAT_BINCODE];
    let encoded = bincode::serde::encode_to_vec(value, bincode::config::legacy())?;
    buf.extend_from_slice(&encoded);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestStruct {
        id: u64,
        name: String,
        values: Vec<i32>,
    }

    #[test]
    fn test_encode_decode_postcard() {
        let data = TestStruct {
            id: 42,
            name: "test".to_string(),
            values: vec![1, 2, 3],
        };

        // Encode using postcard
        let encoded = encode_versioned(&data).unwrap();

        // Verify format byte
        assert_eq!(encoded[0], FORMAT_POSTCARD);

        // Decode
        let decoded: TestStruct = decode_versioned(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_decode_legacy_bincode() {
        let data = TestStruct {
            id: 123,
            name: "legacy".to_string(),
            values: vec![10, 20, 30],
        };

        // Encode using legacy bincode format
        let encoded = encode_versioned_bincode(&data).unwrap();

        // Verify format byte
        assert_eq!(encoded[0], FORMAT_BINCODE);

        // Decode should work
        let decoded: TestStruct = decode_versioned(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_backward_compatibility() {
        let data = TestStruct {
            id: 999,
            name: "compat".to_string(),
            values: vec![5, 10, 15, 20],
        };

        // Encode as legacy bincode
        let bincode_encoded = encode_versioned_bincode(&data).unwrap();

        // Encode as new postcard
        let postcard_encoded = encode_versioned(&data).unwrap();

        // Both should decode to the same value
        let from_bincode: TestStruct = decode_versioned(&bincode_encoded).unwrap();
        let from_postcard: TestStruct = decode_versioned(&postcard_encoded).unwrap();

        assert_eq!(from_bincode, data);
        assert_eq!(from_postcard, data);
        assert_eq!(from_bincode, from_postcard);
    }

    #[test]
    fn test_empty_buffer_error() {
        let result: Result<TestStruct> = decode_versioned(&[]);
        assert!(matches!(result, Err(EncodingError::EmptyBuffer)));
    }

    #[test]
    fn test_invalid_bincode_data_error() {
        // Data that is neither valid bincode nor valid postcard
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF]; // Garbage data
        let result: Result<TestStruct> = decode_versioned(&data);
        // Should fail to decode as bincode (fallback)
        assert!(result.is_err());
    }

    #[test]
    fn test_roundtrip_complex_types() {
        use std::collections::HashMap;

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Complex {
            map: HashMap<String, u64>,
            optional: Option<String>,
            nested: Vec<TestStruct>,
        }

        let mut map = HashMap::new();
        map.insert("key1".to_string(), 100);
        map.insert("key2".to_string(), 200);

        let data = Complex {
            map,
            optional: Some("value".to_string()),
            nested: vec![
                TestStruct {
                    id: 1,
                    name: "first".to_string(),
                    values: vec![1],
                },
                TestStruct {
                    id: 2,
                    name: "second".to_string(),
                    values: vec![2, 3],
                },
            ],
        };

        let encoded = encode_versioned(&data).unwrap();
        let decoded: Complex = decode_versioned(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_legacy_bincode_without_version_prefix() {
        // Simulate existing stored data: raw bincode without version prefix
        let data = TestStruct {
            id: 42,
            name: "legacy".to_string(),
            values: vec![1, 2, 3],
        };

        // Encode using raw bincode (no version prefix) - simulates existing storage
        let raw_bincode = bincode::serde::encode_to_vec(&data, bincode::config::legacy()).unwrap();

        // Should be able to decode this legacy data
        let decoded: TestStruct = decode_versioned(&raw_bincode).unwrap();
        assert_eq!(decoded, data);
    }
}
