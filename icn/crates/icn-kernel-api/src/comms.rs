//! Communication Primitive
//!
//! Provides pub/sub, request/response, and streaming communication.
//!
//! # Design
//!
//! Three communication patterns:
//! - **Pub/Sub**: At-least-once delivery to subscribers
//! - **Request/Response**: Synchronous routing to handlers
//! - **Streams**: Long-lived bidirectional channels
//!
//! # Ordering Guarantees
//!
//! - Per-topic partition ordering (not global)
//! - Message IDs are stable and signed
//! - Consumer offsets are explicit
//! - Replay is first-class (projections can rebuild)
//!
//! # Non-Goals
//!
//! - Message semantics (what messages mean)
//! - Moderation/filtering logic (apps via oracles)
//! - Exactly-once delivery (apps build this if needed)

use crate::types::{
    BidirectionalStream, Did, Endpoint, HandlerId, MessageId, Offset, Path, Subscription, TopicId,
};

/// Published message.
#[derive(Clone, Debug)]
pub struct Message {
    /// Unique message identifier
    pub id: MessageId,
    /// Message payload
    pub data: Vec<u8>,
    /// Publisher DID
    pub publisher: Did,
    /// Message timestamp
    pub timestamp: u64,
    /// Message offset in topic
    pub offset: Offset,
}

/// Topic configuration.
#[derive(Clone, Debug)]
pub struct TopicConfig {
    /// Number of partitions (0 = single partition)
    pub partitions: u32,
    /// Message retention (seconds, 0 = forever)
    pub retention_seconds: u64,
    /// Maximum message size (bytes)
    pub max_message_size: u64,
}

impl Default for TopicConfig {
    fn default() -> Self {
        Self {
            partitions: 1,
            retention_seconds: 7 * 24 * 60 * 60, // 7 days
            max_message_size: 1024 * 1024,       // 1 MB
        }
    }
}

/// Pub/sub messaging.
///
/// Topics are the primary mechanism for async communication.
/// Messages are delivered at-least-once to all subscribers.
pub trait PubSub: Send + Sync {
    /// Create a new topic.
    fn create_topic(&self, name: &str, config: TopicConfig) -> Result<TopicId, CommsError>;

    /// Delete a topic.
    fn delete_topic(&self, topic: &TopicId) -> Result<(), CommsError>;

    /// Publish a message to a topic.
    ///
    /// Returns the message ID for tracking.
    fn publish(&self, topic: &TopicId, message: &[u8]) -> Result<MessageId, CommsError>;

    /// Subscribe to a topic.
    ///
    /// Returns a subscription starting from `from` offset.
    fn subscribe(&self, topic: &TopicId, from: Offset) -> Result<Subscription, CommsError>;

    /// Unsubscribe from a topic.
    fn unsubscribe(&self, subscription: &Subscription) -> Result<(), CommsError>;

    /// Get current end offset of a topic.
    fn end_offset(&self, topic: &TopicId) -> Result<Offset, CommsError>;

    /// List all topics.
    fn list_topics(&self) -> Result<Vec<TopicId>, CommsError>;
}

/// Request/response RPC.
///
/// Provides synchronous routing to registered handlers.
/// Used for queries and operations that need immediate response.
pub trait RequestResponse: Send + Sync {
    /// Register a handler for a path.
    fn register(&self, path: &Path, handler: HandlerId) -> Result<(), CommsError>;

    /// Unregister a handler.
    fn unregister(&self, path: &Path) -> Result<(), CommsError>;

    /// Send a request and wait for response.
    ///
    /// The endpoint includes the target node and path.
    fn request(&self, endpoint: &Endpoint, request: &[u8]) -> Result<Vec<u8>, CommsError>;

    /// Send a request with a timeout.
    fn request_with_timeout(
        &self,
        endpoint: &Endpoint,
        request: &[u8],
        timeout: std::time::Duration,
    ) -> Result<Vec<u8>, CommsError>;

    /// List registered handlers.
    fn list_handlers(&self) -> Result<Vec<Path>, CommsError>;
}

/// Bidirectional streaming.
///
/// Long-lived connections for real-time communication.
/// Each stream is a full-duplex channel.
pub trait Streams: Send + Sync {
    /// Open a stream to an endpoint.
    fn open(&self, endpoint: &Endpoint) -> Result<BidirectionalStream, CommsError>;

    /// Close a stream.
    fn close(&self, stream: &BidirectionalStream) -> Result<(), CommsError>;

    /// Send data on a stream.
    fn send(&self, stream: &BidirectionalStream, data: &[u8]) -> Result<(), CommsError>;

    /// Receive data from a stream (blocking).
    fn receive(&self, stream: &BidirectionalStream) -> Result<Vec<u8>, CommsError>;

    /// Check if a stream is open.
    fn is_open(&self, stream: &BidirectionalStream) -> bool;

    /// List open streams.
    fn list_streams(&self) -> Result<Vec<BidirectionalStream>, CommsError>;
}

/// External protocol adapter.
///
/// Adapters bridge ICN communication to external protocols
/// (email, webhooks, push notifications, etc.).
pub trait Adapter: Send + Sync {
    /// Get the protocol this adapter handles.
    fn protocol(&self) -> &str;

    /// Send a message via this protocol.
    fn send(&self, destination: &str, payload: &[u8]) -> Result<(), CommsError>;

    /// Check if this adapter can reach a destination.
    fn can_reach(&self, destination: &str) -> bool;
}

/// Adapter registry for external protocols.
pub trait AdapterRegistry: Send + Sync {
    /// Register an adapter.
    fn register(&self, adapter: Box<dyn Adapter>) -> Result<(), CommsError>;

    /// Get an adapter by protocol name.
    fn get(&self, protocol: &str) -> Option<&dyn Adapter>;

    /// List registered protocols.
    fn protocols(&self) -> Vec<String>;
}

/// Errors from communication operations.
#[derive(Debug, thiserror::Error)]
pub enum CommsError {
    /// Topic not found
    #[error("Topic not found: {0}")]
    TopicNotFound(String),

    /// Topic already exists
    #[error("Topic already exists: {0}")]
    TopicAlreadyExists(String),

    /// Handler not found
    #[error("Handler not found: {0}")]
    HandlerNotFound(String),

    /// Endpoint unreachable
    #[error("Endpoint unreachable: {0}")]
    Unreachable(String),

    /// Request timeout
    #[error("Request timeout")]
    Timeout,

    /// Stream closed
    #[error("Stream closed")]
    StreamClosed,

    /// Message too large
    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: u64, max: u64 },

    /// Invalid offset
    #[error("Invalid offset: {0}")]
    InvalidOffset(Offset),

    /// Protocol not supported
    #[error("Protocol not supported: {0}")]
    ProtocolNotSupported(String),

    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Internal error
    #[error("Comms error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_config_default() {
        let config = TopicConfig::default();
        assert_eq!(config.partitions, 1);
        assert_eq!(config.retention_seconds, 7 * 24 * 60 * 60);
        assert_eq!(config.max_message_size, 1024 * 1024);
    }

    #[test]
    fn test_message() {
        let msg = Message {
            id: "msg-1".to_string(),
            data: vec![1, 2, 3],
            publisher: "did:icn:test".to_string(),
            timestamp: 12345,
            offset: 0,
        };
        assert_eq!(msg.id, "msg-1");
        assert_eq!(msg.data, vec![1, 2, 3]);
    }

    #[test]
    fn test_comms_error_display() {
        let err = CommsError::MessageTooLarge {
            size: 2000000,
            max: 1000000,
        };
        assert!(err.to_string().contains("2000000"));
        assert!(err.to_string().contains("1000000"));
    }
}
