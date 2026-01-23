//! Encoding abstraction layer for ICN serialization.
//!
//! This crate provides a unified API for binary serialization using postcard.
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
    #[error("postcard error: {0}")]
    Postcard(#[from] postcard::Error),
}

/// Result type for encoding operations.
pub type Result<T> = std::result::Result<T, Error>;

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
    fn test_encode_decode() {
        let data = sample_data();
        let bytes = encode(&data).expect("encode");
        let decoded: TestData = decode(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_roundtrip_empty_vec() {
        let data = TestData {
            name: String::new(),
            value: 0,
            nested: vec![],
        };
        let bytes = encode(&data).expect("encode");
        let decoded: TestData = decode(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_roundtrip_large_data() {
        let data = TestData {
            name: "large".repeat(100),
            value: u64::MAX,
            nested: vec![0xFF; 1000],
        };
        let bytes = encode(&data).expect("encode");
        let decoded: TestData = decode(&bytes).expect("decode");
        assert_eq!(data, decoded);
    }
}
