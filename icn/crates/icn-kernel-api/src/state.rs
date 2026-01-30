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

use serde::{Deserialize, Serialize};

use crate::scope::ScopeLevel;
use crate::types::{Hash, Key, LogId, Namespace, Offset, SchemaRef, Subscription, Value, Version};

/// Replication policy for storage.
///
/// Determines how data is replicated across nodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationPolicy {
    /// Single node, no replication
    LocalOnly,
    /// Consensus group, linearizable reads/writes
    ClusterStrong,
    /// Gossip/CRDT, eventually consistent
    FederationEventual,
    /// Durable archive, retained indefinitely
    Archive,
    /// Scope-aware: replicate with a target factor, scoped to a particular level.
    ///
    /// The `scope` field indicates the scope granularity for replication planning
    /// (e.g., Cell means "replicate within the cell"). The actual placement
    /// boundary is determined by [`ObjectReplication::max_scope`].
    Scoped {
        /// The scope granularity for replication planning.
        scope: ScopeLevel,
        /// Target number of replicas.
        factor: u8,
    },
}

/// Per-object replication configuration.
///
/// Combines a [`ReplicationPolicy`] with scope bounds that constrain
/// where replicas may be placed. The invariant `min_durability_scope <= max_scope`
/// is enforced at construction time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectReplication {
    /// The replication policy governing how this object is replicated.
    pub policy: ReplicationPolicy,
    /// The narrowest scope that must hold at least one replica for durability.
    pub min_durability_scope: ScopeLevel,
    /// The widest scope to which replicas may be distributed.
    pub max_scope: ScopeLevel,
}

impl ObjectReplication {
    /// Create a new `ObjectReplication` with validated scope bounds.
    ///
    /// Returns an error if `min_durability_scope > max_scope`.
    pub fn new(
        policy: ReplicationPolicy,
        min_durability_scope: ScopeLevel,
        max_scope: ScopeLevel,
    ) -> Result<Self, StateError> {
        let obj = Self {
            policy,
            min_durability_scope,
            max_scope,
        };
        obj.validate()?;
        Ok(obj)
    }

    /// Validate that the scope bounds are consistent.
    pub fn validate(&self) -> Result<(), StateError> {
        if self.min_durability_scope > self.max_scope {
            return Err(StateError::Internal(
                "min_durability_scope must be <= max_scope".into(),
            ));
        }
        Ok(())
    }

    /// Return the effective replication factor.
    ///
    /// For `Scoped` policies, returns the configured factor.
    /// For other policies, returns a sensible default:
    /// - `LocalOnly` → 1
    /// - `ClusterStrong` → 3
    /// - `FederationEventual` → 3
    /// - `Archive` → 5
    pub fn effective_factor(&self) -> usize {
        match self.policy {
            ReplicationPolicy::LocalOnly => 1,
            ReplicationPolicy::ClusterStrong => 3,
            ReplicationPolicy::FederationEventual => 3,
            ReplicationPolicy::Archive => 5,
            ReplicationPolicy::Scoped { factor, .. } => factor as usize,
        }
    }
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

/// Interface for a single log instance.
#[async_trait::async_trait]
pub trait Log: Send + Sync {
    /// Append an event to the log.
    async fn append(&self, data: Vec<u8>) -> Result<Offset, StateError>;

    /// Read events from the log.
    async fn read(&self, from: Offset, limit: usize) -> Result<Vec<Event>, StateError>;

    /// Get current length (offset) of log.
    async fn len(&self) -> u64;

    /// Check if the log is empty.
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// Interface for a single KV store.
#[async_trait::async_trait]
pub trait Kv: Send + Sync {
    /// Get a value.
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StateError>;

    /// Set a value.
    async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StateError>;

    /// Delete a value.
    async fn delete(&self, key: &str) -> Result<bool, StateError>;

    /// Check if key exists.
    async fn exists(&self, key: &str) -> Result<bool, StateError>;

    /// List keys with prefix.
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StateError>;
}

/// Interface for a single Blob store.
#[async_trait::async_trait]
pub trait Blob: Send + Sync {
    /// Store a blob.
    async fn put(&self, hash: &str, data: Vec<u8>) -> Result<(), StateError>;

    /// Get a blob.
    async fn get(&self, hash: &str) -> Result<Option<Vec<u8>>, StateError>;

    /// Check if blob exists.
    async fn exists(&self, hash: &str) -> Result<bool, StateError>;

    /// Delete a blob.
    async fn delete(&self, hash: &str) -> Result<bool, StateError>;
}

/// Abstract interface for application state.
///
/// This trait allows applications to access their namespaced state resources
/// without depending on the concrete kernel implementation.
pub trait AppState: Send + Sync {
    /// Get a log handle by name.
    fn log(&self, name: &str) -> Option<std::sync::Arc<dyn Log>>;

    /// Get a KV handle by name.
    fn kv(&self, name: &str) -> Option<std::sync::Arc<dyn Kv>>;

    /// Get a blob handle by name.
    fn blob(&self, name: &str) -> Option<std::sync::Arc<dyn Blob>>;
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

    #[test]
    fn test_scoped_policy_creation() {
        let policy = ReplicationPolicy::Scoped {
            scope: ScopeLevel::Cell,
            factor: 3,
        };
        match policy {
            ReplicationPolicy::Scoped { scope, factor } => {
                assert_eq!(scope, ScopeLevel::Cell);
                assert_eq!(factor, 3);
            }
            _ => panic!("Expected Scoped variant"),
        }
    }

    #[test]
    fn test_existing_policies_unchanged() {
        assert_eq!(ReplicationPolicy::LocalOnly, ReplicationPolicy::LocalOnly);
        assert_eq!(
            ReplicationPolicy::ClusterStrong,
            ReplicationPolicy::ClusterStrong
        );
        assert_eq!(
            ReplicationPolicy::FederationEventual,
            ReplicationPolicy::FederationEventual
        );
        assert_eq!(ReplicationPolicy::Archive, ReplicationPolicy::Archive);
        assert_ne!(
            ReplicationPolicy::LocalOnly,
            ReplicationPolicy::ClusterStrong
        );
    }

    #[test]
    fn test_scoped_policy_serde_roundtrip() {
        let policy = ReplicationPolicy::Scoped {
            scope: ScopeLevel::Org,
            factor: 5,
        };
        let json = serde_json::to_string(&policy).unwrap();
        let parsed: ReplicationPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, policy);

        // Also test unit variants roundtrip
        for p in [
            ReplicationPolicy::LocalOnly,
            ReplicationPolicy::ClusterStrong,
            ReplicationPolicy::FederationEventual,
            ReplicationPolicy::Archive,
        ] {
            let json = serde_json::to_string(&p).unwrap();
            let parsed: ReplicationPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, p);
        }
    }

    #[test]
    fn test_object_replication_valid() {
        let obj = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Cell,
                factor: 2,
            },
            ScopeLevel::Cell,
            ScopeLevel::Org,
        );
        assert!(obj.is_ok());
        let obj = obj.unwrap();
        assert_eq!(obj.min_durability_scope, ScopeLevel::Cell);
        assert_eq!(obj.max_scope, ScopeLevel::Org);
    }

    #[test]
    fn test_object_replication_invalid() {
        let obj = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Cell,
                factor: 2,
            },
            ScopeLevel::Federation,
            ScopeLevel::Cell,
        );
        assert!(obj.is_err());
    }

    #[test]
    fn test_effective_factor() {
        let local = ObjectReplication::new(
            ReplicationPolicy::LocalOnly,
            ScopeLevel::Local,
            ScopeLevel::Local,
        )
        .unwrap();
        assert_eq!(local.effective_factor(), 1);

        let cluster = ObjectReplication::new(
            ReplicationPolicy::ClusterStrong,
            ScopeLevel::Cell,
            ScopeLevel::Org,
        )
        .unwrap();
        assert_eq!(cluster.effective_factor(), 3);

        let archive = ObjectReplication::new(
            ReplicationPolicy::Archive,
            ScopeLevel::Local,
            ScopeLevel::Commons,
        )
        .unwrap();
        assert_eq!(archive.effective_factor(), 5);

        let scoped = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Org,
                factor: 7,
            },
            ScopeLevel::Cell,
            ScopeLevel::Federation,
        )
        .unwrap();
        assert_eq!(scoped.effective_factor(), 7);
    }

    #[test]
    fn test_object_replication_serde() {
        let obj = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Cell,
                factor: 3,
            },
            ScopeLevel::Cell,
            ScopeLevel::Org,
        )
        .unwrap();

        let json = serde_json::to_string(&obj).unwrap();
        let parsed: ObjectReplication = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, obj);
    }
}
