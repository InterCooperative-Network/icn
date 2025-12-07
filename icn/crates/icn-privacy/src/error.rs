//! Error types for privacy operations

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PrivacyError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("Invalid ciphertext: {0}")]
    InvalidCiphertext(String),

    #[error("Bloom filter error: {0}")]
    BloomFilterError(String),

    #[error("Onion routing error: {0}")]
    OnionRoutingError(String),

    #[error("Traffic obfuscation error: {0}")]
    ObfuscationError(String),

    #[error("Topic not found in encrypted set")]
    TopicNotFound,
}

pub type Result<T> = std::result::Result<T, PrivacyError>;
