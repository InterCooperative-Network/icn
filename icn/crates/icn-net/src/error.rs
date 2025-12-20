//! Error types for ICN Net.

use thiserror::Error;

/// Result type for network operations
pub type Result<T> = std::result::Result<T, NetError>;

/// Errors that can occur in the network layer
#[derive(Debug, Error)]
pub enum NetError {
    /// Connection failed
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    /// Session not found
    #[error("session not found: {0}")]
    SessionNotFound(String),

    /// Encryption failed
    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    /// Decryption failed
    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    /// TLS error
    #[error("TLS error: {0}")]
    Tls(String),

    /// Onion routing error
    #[error("onion routing error: {0}")]
    OnionRouting(String),

    /// Message too large
    #[error("message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    /// Invalid message format
    #[error("invalid message format: {0}")]
    InvalidMessage(String),

    /// Signature verification failed
    #[error("signature verification failed: {0}")]
    SignatureInvalid(String),

    /// Replay attack detected
    #[error("replay attack detected")]
    ReplayDetected,

    /// Rate limited
    #[error("rate limited: {0}")]
    RateLimited(String),

    /// Discovery error
    #[error("discovery error: {0}")]
    Discovery(String),

    /// STUN error
    #[error("STUN error: {0}")]
    Stun(String),

    /// TURN error
    #[error("TURN error: {0}")]
    Turn(String),

    /// Timeout
    #[error("operation timed out")]
    Timeout,

    /// Channel closed
    #[error("channel closed")]
    ChannelClosed,

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<quinn::ConnectionError> for NetError {
    fn from(e: quinn::ConnectionError) -> Self {
        NetError::ConnectionFailed(e.to_string())
    }
}

impl From<quinn::WriteError> for NetError {
    fn from(e: quinn::WriteError) -> Self {
        NetError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }
}

impl From<quinn::ReadExactError> for NetError {
    fn from(e: quinn::ReadExactError) -> Self {
        NetError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }
}
