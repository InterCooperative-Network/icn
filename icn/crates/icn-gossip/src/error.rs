//! Error types for ICN Gossip.

use thiserror::Error;

/// Result type for gossip operations
pub type Result<T> = std::result::Result<T, GossipError>;

/// Errors that can occur in the gossip layer
#[derive(Debug, Error)]
pub enum GossipError {
    /// Topic not found
    #[error("topic not found: {0}")]
    TopicNotFound(String),

    /// Access denied to topic
    #[error("access denied to topic {topic}: required {required:?}, got {actual:?}")]
    AccessDenied {
        topic: String,
        required: String,
        actual: String,
    },

    /// A network-originated subscription-control request claimed this node's own DID.
    ///
    /// Distinct from an operational subscribe/unsubscribe failure so callers can log it
    /// at a level a remote peer cannot use to drive log volume. `NetworkMessage.from` is
    /// self-declared, so this is fully remote-triggerable and repeatable.
    #[error("refusing network {action} claiming this node's own DID for topic {topic}")]
    SubscriptionControlSpoofRejected { topic: String, action: &'static str },

    /// Entry not found
    #[error("entry not found: {0}")]
    EntryNotFound(String),

    /// Duplicate entry
    #[error("duplicate entry: {0}")]
    DuplicateEntry(String),

    /// Vector clock conflict
    #[error("vector clock conflict")]
    VectorClockConflict,

    /// Partition detected
    #[error("partition detected: {0}")]
    PartitionDetected(String),

    /// Varint encoding error
    #[error("varint error: {0}")]
    Varint(String),

    /// Message too large
    #[error("message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    /// Invalid message format
    #[error("invalid message format: {0}")]
    InvalidMessage(String),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Channel closed
    #[error("channel closed")]
    ChannelClosed,

    /// Rate limited
    #[error("rate limited")]
    RateLimited,

    /// Internal error
    #[error("internal error: {0}")]
    Internal(String),
}
