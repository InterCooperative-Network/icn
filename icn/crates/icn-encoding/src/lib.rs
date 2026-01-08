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
/// - Format 0x00: Legacy bincode format
/// - Format 0x01: Postcard format
pub fn decode_versioned<T: for<'de> Deserialize<'de>>(data: &[u8]) -> Result<T> {
    if data.is_empty() {
        return Err(EncodingError::EmptyBuffer);
    }

    let format = data[0];
    let payload = &data[1..];

    match format {
        FORMAT_BINCODE => {
            // Legacy format: decode using bincode
            let (decoded, _) =
                bincode::serde::decode_from_slice(payload, bincode::config::legacy())?;
            Ok(decoded)
        }
        FORMAT_POSTCARD => {
            // Current format: decode using postcard
            let decoded = postcard::from_bytes(payload)?;
            Ok(decoded)
        }
        _ => Err(EncodingError::UnknownFormat(format)),
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
    fn test_unknown_format_error() {
        let data = vec![0xFF, 1, 2, 3]; // Invalid format byte
        let result: Result<TestStruct> = decode_versioned(&data);
        assert!(matches!(result, Err(EncodingError::UnknownFormat(0xFF))));
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
}
