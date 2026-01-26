//! State Primitive
//!
//! Provides namespaced storage via three storage types:
//! - Logs: Append-only, ordered event streams
//! - Blobs: Content-addressed immutable storage
//! - KV: Mutable key-value with CAS (Compare-And-Swap)
//!
//! # Design
//!
//! All storage is namespaced following the pattern `/<org>/<app>/<sub>/`.
//! Cross-namespace access requires explicit capability grants.
//!
//! # Non-Goals
//!
//! - Projections/materialized views (runtime layer)
//! - Search/indexing (runtime layer)
//! - Domain-specific schemas (apps define these)
//! - "Ledger" or "governance" data types (apps, not kernel)

use crate::types::{Hash, Key, LogId, Namespace, Offset, SchemaRef, Subscription, Value, Version};

/// Replication policy for storage.
///
/// Determines how data is replicated across nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationPolicy {
    /// Single node, no replication
    LocalOnly,
    /// Consensus group, linearizable reads/writes
    ClusterStrong,
    /// Gossip/CRDT, eventually consistent
    FederationEventual,
    /// Durable archive, retained indefinitely
    Archive,
}

/// Event stored in a log.
#[derive(Clone, Debug)]
pub struct Event {
    /// Offset of this event in the log
    pub offset: Offset,
    /// Raw event data
    pub data: Vec<u8>,
    /// Timestamp when appended
    pub timestamp: u64,
    /// Optional schema reference
    pub schema: Option<SchemaRef>,
}

/// Log configuration.
#[derive(Clone, Debug)]
pub struct LogConfig {
    /// Schema for events in this log
    pub schema: Option<SchemaRef>,
    /// Replication policy
    pub replication: ReplicationPolicy,
    /// Maximum log size (bytes, 0 = unlimited)
    pub max_size: u64,
    /// Retention period (seconds, 0 = forever)
    pub retention_seconds: u64,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            schema: None,
            replication: ReplicationPolicy::LocalOnly,
            max_size: 0,
            retention_seconds: 0,
        }
    }
}

/// Append-only log service.
///
/// Logs store ordered sequences of events. They are the primary
/// storage mechanism for event-sourced applications.
///
/// # Ordering Guarantees
///
/// - Logs are ordered per-writer by default (cheap, scalable)
/// - Total ordering requires `ClusterStrong` replication (expensive)
/// - Apps that need global order must explicitly request it
pub trait LogService: Send + Sync {
    /// Create a new log in a namespace.
    fn create(
        &self,
        namespace: &Namespace,
        name: &str,
        config: LogConfig,
    ) -> Result<LogId, StateError>;

    /// Append an event to a log.
    ///
    /// Returns the offset of the new event.
    fn append(&self, log_id: &LogId, data: &[u8]) -> Result<Offset, StateError>;

    /// Read events from a log.
    ///
    /// Returns events from `from` (inclusive) to `to` (exclusive).
    fn read(&self, log_id: &LogId, from: Offset, to: Offset) -> Result<Vec<Event>, StateError>;

    /// Get the current end offset of a log.
    fn end_offset(&self, log_id: &LogId) -> Result<Offset, StateError>;

    /// Subscribe to new events in a log.
    ///
    /// Returns a subscription that will receive events from `from` onwards.
    fn subscribe(&self, log_id: &LogId, from: Offset) -> Result<Subscription, StateError>;

    /// Delete a log and all its data.
    fn delete(&self, log_id: &LogId) -> Result<(), StateError>;

    /// List logs in a namespace.
    fn list(&self, namespace: &Namespace) -> Result<Vec<LogId>, StateError>;
}

/// Content-addressed blob storage.
///
/// Blobs are immutable, content-addressed storage. The hash of
/// the content serves as the key, ensuring integrity and enabling
/// deduplication.
pub trait BlobService: Send + Sync {
    /// Store a blob and return its hash.
    fn put(&self, namespace: &Namespace, data: &[u8]) -> Result<Hash, StateError>;

    /// Retrieve a blob by hash.
    fn get(&self, hash: &Hash) -> Result<Vec<u8>, StateError>;

    /// Check if a blob exists.
    fn exists(&self, hash: &Hash) -> Result<bool, StateError>;

    /// Delete a blob.
    ///
    /// Note: Deletion may be deferred if the blob is referenced
    /// by other content.
    fn delete(&self, hash: &Hash) -> Result<(), StateError>;

    /// Get blob metadata (size, namespace, etc.).
    fn metadata(&self, hash: &Hash) -> Result<BlobMetadata, StateError>;
}

/// Metadata about a blob.
#[derive(Clone, Debug)]
pub struct BlobMetadata {
    /// Size in bytes
    pub size: u64,
    /// Namespace this blob belongs to
    pub namespace: Namespace,
    /// When the blob was stored
    pub created_at: u64,
    /// Content type (if known)
    pub content_type: Option<String>,
}

/// Key-value storage with CAS (Compare-And-Swap).
///
/// KV provides mutable storage with optimistic concurrency control.
/// All writes require the expected version to prevent lost updates.
pub trait KvService: Send + Sync {
    /// Get a value and its version.
    fn get(&self, namespace: &Namespace, key: &Key) -> Result<(Value, Version), StateError>;

    /// Put a value with optimistic concurrency check.
    ///
    /// If `expected_version` doesn't match, returns `VersionMismatch`.
    /// Use `Version::default()` (0) for the initial write.
    fn put(
        &self,
        namespace: &Namespace,
        key: &Key,
        value: &Value,
        expected_version: Version,
    ) -> Result<Version, StateError>;

    /// Delete a key with optimistic concurrency check.
    fn delete(
        &self,
        namespace: &Namespace,
        key: &Key,
        expected_version: Version,
    ) -> Result<(), StateError>;

    /// List keys with a prefix.
    fn list(&self, namespace: &Namespace, prefix: &Key) -> Result<Vec<Key>, StateError>;

    /// Check if a key exists.
    fn exists(&self, namespace: &Namespace, key: &Key) -> Result<bool, StateError>;

    /// Get multiple values in one call.
    fn get_batch(
        &self,
        namespace: &Namespace,
        keys: &[Key],
    ) -> Result<Vec<Option<(Value, Version)>>, StateError>;
}

/// Filter for log subscriptions.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Filter by schema
    pub schema: Option<SchemaRef>,
    /// Filter by custom predicate (serialized)
    pub predicate: Option<Vec<u8>>,
}

impl Filter {
    /// Create an empty filter (match all).
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by schema.
    pub fn with_schema(mut self, schema: SchemaRef) -> Self {
        self.schema = Some(schema);
        self
    }
}

/// Errors from state operations.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// Log not found
    #[error("Log not found: {0}")]
    LogNotFound(String),

    /// Blob not found
    #[error("Blob not found")]
    BlobNotFound,

    /// Key not found
    #[error("Key not found")]
    KeyNotFound,

    /// Version mismatch (CAS failure)
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: Version, actual: Version },

    /// Namespace access denied
    #[error("Access denied to namespace: {0}")]
    AccessDenied(String),

    /// Storage quota exceeded
    #[error("Storage quota exceeded")]
    QuotaExceeded,

    /// Invalid offset
    #[error("Invalid offset: {0}")]
    InvalidOffset(Offset),

    /// Log already exists
    #[error("Log already exists: {0}")]
    LogAlreadyExists(String),

    /// Storage backend error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Internal error
    #[error("Internal state error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_policy() {
        assert_eq!(ReplicationPolicy::LocalOnly, ReplicationPolicy::LocalOnly);
        assert_ne!(
            ReplicationPolicy::ClusterStrong,
            ReplicationPolicy::FederationEventual
        );
    }

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert!(config.schema.is_none());
        assert_eq!(config.replication, ReplicationPolicy::LocalOnly);
        assert_eq!(config.max_size, 0);
        assert_eq!(config.retention_seconds, 0);
    }

    #[test]
    fn test_filter_builder() {
        let filter = Filter::new().with_schema(SchemaRef::new("test", "1.0.0"));
        assert!(filter.schema.is_some());
        assert_eq!(filter.schema.as_ref().unwrap().name, "test");
    }

    #[test]
    fn test_state_error_display() {
        let err = StateError::VersionMismatch {
            expected: 1,
            actual: 2,
        };
        assert!(err.to_string().contains("expected 1"));
        assert!(err.to_string().contains("got 2"));
    }
}
