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

    /// Validate that the scope bounds and policy parameters are consistent.
    ///
    /// For `Scoped` policies, also checks that `factor >= 1` and that the
    /// policy's `scope` lies within `[min_durability_scope, max_scope]`.
    pub fn validate(&self) -> Result<(), StateError> {
        if self.min_durability_scope > self.max_scope {
            return Err(StateError::Internal(
                "min_durability_scope must be <= max_scope".into(),
            ));
        }
        if let ReplicationPolicy::Scoped { scope, factor } = self.policy {
            if factor == 0 {
                return Err(StateError::Internal(
                    "Scoped replication factor must be >= 1".into(),
                ));
            }
            if scope < self.min_durability_scope || scope > self.max_scope {
                return Err(StateError::Internal(format!(
                    "Scoped scope {:?} must be within [{:?}, {:?}]",
                    scope, self.min_durability_scope, self.max_scope
                )));
            }
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

    /// Blob exceeds maximum allowed size
    #[error("Blob exceeds size limit: {size} bytes (max {max})")]
    BlobOversize {
        /// Actual blob size
        size: u64,
        /// Maximum allowed size
        max: u64,
    },

    /// Blob integrity check failed (hash mismatch)
    #[error("Blob integrity error: expected {expected}, got {actual}")]
    BlobIntegrity {
        /// Expected hash (hex)
        expected: String,
        /// Actual hash (hex)
        actual: String,
    },

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

// ============================================================================
// State Generalization (A1)
// ============================================================================
//
// These types provide a generic state access layer that apps use to interact
// with scoped storage. The kernel provides storage mechanics without knowing
// what apps store. Domain-specific schemas (governance, ledger, coop) belong
// in apps, not here.
//
// See: GitHub Issue #1135, relates to #858

/// Generic key for state access.
///
/// Keys are hierarchical, using `/` as a separator. The kernel treats keys as
/// opaque byte sequences; apps define key schemas.
///
/// # Key Format Convention
///
/// Apps should use namespaced keys to avoid collisions:
/// - `/<app>/<entity>/<id>` for entity records
/// - `/<app>/<entity>/<id>/<attr>` for entity attributes
/// - `/<app>/meta/<name>` for metadata
///
/// # Examples
///
/// ```
/// use icn_kernel_api::state::StateKey;
///
/// let key = StateKey::new("/ledger/accounts/alice");
/// assert_eq!(key.as_str(), "/ledger/accounts/alice");
///
/// let prefixed = StateKey::with_prefix("ledger", "accounts/alice");
/// assert_eq!(prefixed.as_str(), "/ledger/accounts/alice");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StateKey(String);

impl StateKey {
    /// Create a new state key from a string.
    ///
    /// Keys should start with `/` for consistency, but this is not enforced.
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// Create a key with a prefix (adds leading `/`).
    ///
    /// Useful for constructing namespaced keys.
    pub fn with_prefix(prefix: &str, suffix: &str) -> Self {
        Self(format!("/{prefix}/{suffix}"))
    }

    /// Get the key as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the key as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Check if this key starts with the given prefix.
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }
}

impl std::fmt::Display for StateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StateKey {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for StateKey {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Generic value wrapper for state storage.
///
/// Values are opaque byte sequences. The kernel does not interpret value
/// contents; apps define their own serialization formats.
///
/// # Serialization
///
/// Apps are responsible for serializing domain types to bytes. Recommended:
/// - Use `bincode` for compact binary encoding
/// - Use `serde_json` when human-readability is needed
/// - Include version prefixes for schema evolution
///
/// # Examples
///
/// ```
/// use icn_kernel_api::state::StateValue;
///
/// let value = StateValue::new(vec![1, 2, 3, 4]);
/// assert_eq!(value.as_bytes().len(), 4);
///
/// // From string (for simple cases)
/// let text = StateValue::from_string("hello");
/// assert_eq!(text.as_bytes(), b"hello");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateValue(Vec<u8>);

impl StateValue {
    /// Create a new state value from bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Create a state value from a string slice.
    pub fn from_string(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }

    /// Get the value as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume and return the inner bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Check if the value is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the length of the value in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl From<Vec<u8>> for StateValue {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for StateValue {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

/// State operation types for generic state access.
///
/// These operations represent the fundamental CRUD operations on state.
/// The kernel implements these operations; apps compose them into
/// higher-level domain operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateOp {
    /// Get a value by key.
    Get { key: StateKey },

    /// Put a value at a key (upsert semantics).
    Put { key: StateKey, value: StateValue },

    /// Delete a value by key.
    Delete { key: StateKey },

    /// List keys with a prefix.
    List { prefix: String },
}

impl StateOp {
    /// Create a Get operation.
    pub fn get(key: impl Into<StateKey>) -> Self {
        Self::Get { key: key.into() }
    }

    /// Create a Put operation.
    pub fn put(key: impl Into<StateKey>, value: impl Into<StateValue>) -> Self {
        Self::Put {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Create a Delete operation.
    pub fn delete(key: impl Into<StateKey>) -> Self {
        Self::Delete { key: key.into() }
    }

    /// Create a List operation.
    pub fn list(prefix: impl Into<String>) -> Self {
        Self::List {
            prefix: prefix.into(),
        }
    }
}

/// Scope for state access.
///
/// State is scoped to control visibility and replication. The kernel uses
/// scopes for access control and data placement decisions without
/// understanding the domain semantics of what's stored.
///
/// # Scope Hierarchy
///
/// - `Cell`: Local to the HA cluster (fastest, most private)
/// - `Coop`: Shared within a cooperative (replicated within org)
/// - `Federation`: Shared across federated cooperatives
/// - `Commons`: Public, shared with all reachable nodes
///
/// # Access Rules
///
/// By default:
/// - Writers can only write to their own scope or narrower
/// - Readers can read from their scope or wider (with appropriate caps)
/// - Cross-scope writes require explicit capability grants
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StateScope {
    /// Local to the cell (HA cluster) - not replicated outside.
    #[default]
    Cell = 0,
    /// Shared within the cooperative.
    Coop = 1,
    /// Shared across federated cooperatives.
    Federation = 2,
    /// Public commons - globally visible.
    Commons = 3,
}

impl StateScope {
    /// Convert to the corresponding ScopeLevel.
    ///
    /// Maps state scopes to the general scope hierarchy used elsewhere.
    pub fn to_scope_level(&self) -> ScopeLevel {
        match self {
            StateScope::Cell => ScopeLevel::Cell,
            StateScope::Coop => ScopeLevel::Org,
            StateScope::Federation => ScopeLevel::Federation,
            StateScope::Commons => ScopeLevel::Commons,
        }
    }

    /// Create from a ScopeLevel.
    ///
    /// Maps the general scope hierarchy to state scopes.
    /// `Local` maps to `Cell` (narrowest state scope).
    pub fn from_scope_level(level: ScopeLevel) -> Self {
        match level {
            ScopeLevel::Local | ScopeLevel::Cell => StateScope::Cell,
            ScopeLevel::Org => StateScope::Coop,
            ScopeLevel::Federation => StateScope::Federation,
            ScopeLevel::Commons => StateScope::Commons,
        }
    }

    /// Check if this scope includes another scope.
    ///
    /// Wider scopes include narrower ones.
    pub fn includes(&self, other: StateScope) -> bool {
        *self >= other
    }
}

impl std::fmt::Display for StateScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateScope::Cell => write!(f, "cell"),
            StateScope::Coop => write!(f, "coop"),
            StateScope::Federation => write!(f, "federation"),
            StateScope::Commons => write!(f, "commons"),
        }
    }
}

// ============================================================================
// StateBackend Trait
// ============================================================================

/// Generic state backend interface for scoped key-value storage.
///
/// This trait provides a uniform interface for state access across different
/// scopes. Implementations handle the actual storage, replication, and
/// consistency guarantees.
///
/// # Design Principles
///
/// 1. **Scope-aware**: All operations are scoped. The kernel routes requests
///    to appropriate storage backends based on scope.
///
/// 2. **Opaque values**: The backend treats keys and values as opaque bytes.
///    Domain semantics (schemas, validation) belong in apps.
///
/// 3. **Async-first**: Operations are async to support networked storage
///    and replication without blocking.
///
/// 4. **Error handling**: Errors are returned as `StateError` variants.
///    Apps decide how to handle errors (retry, fallback, propagate).
///
/// # Implementation Notes
///
/// Implementors should:
/// - Respect scope boundaries (don't leak data across scopes)
/// - Handle concurrent access safely
/// - Provide appropriate durability guarantees per scope
/// - Emit metrics for observability
///
/// # Examples
///
/// ```ignore
/// use icn_kernel_api::state::{StateBackend, StateScope, StateKey, StateValue};
///
/// async fn example(backend: &dyn StateBackend) -> Result<(), StateError> {
///     // Store a value in coop scope
///     let key = StateKey::new("/accounts/alice/balance");
///     let value = StateValue::new(100i64.to_le_bytes().to_vec());
///     backend.put(StateScope::Coop, key.clone(), value).await?;
///
///     // Read it back
///     if let Some(stored) = backend.get(StateScope::Coop, &key).await? {
///         println!("Balance: {} bytes", stored.len());
///     }
///
///     Ok(())
/// }
/// ```
#[async_trait::async_trait]
pub trait StateBackend: Send + Sync {
    /// Get a value by key within a scope.
    ///
    /// Returns `None` if the key doesn't exist.
    async fn get(&self, scope: StateScope, key: &StateKey) -> Result<Option<StateValue>, StateError>;

    /// Put a value at a key within a scope.
    ///
    /// This is an upsert operation: creates if not exists, updates if exists.
    async fn put(&self, scope: StateScope, key: StateKey, value: StateValue) -> Result<(), StateError>;

    /// Delete a value by key within a scope.
    ///
    /// Returns `Ok(())` even if the key didn't exist (idempotent).
    async fn delete(&self, scope: StateScope, key: &StateKey) -> Result<(), StateError>;

    /// List keys with a prefix within a scope.
    ///
    /// Returns all keys that start with the given prefix.
    /// Order is implementation-defined but should be consistent.
    async fn list(&self, scope: StateScope, prefix: &str) -> Result<Vec<StateKey>, StateError>;

    /// Check if a key exists within a scope.
    ///
    /// Default implementation uses `get`, but backends may override
    /// for efficiency.
    async fn exists(&self, scope: StateScope, key: &StateKey) -> Result<bool, StateError> {
        Ok(self.get(scope, key).await?.is_some())
    }

    /// Get multiple values in one call.
    ///
    /// Returns a vector of optional values, one per key in the same order.
    /// Default implementation calls `get` for each key, but backends may
    /// override for efficiency.
    async fn get_batch(
        &self,
        scope: StateScope,
        keys: &[StateKey],
    ) -> Result<Vec<Option<StateValue>>, StateError> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            results.push(self.get(scope, key).await?);
        }
        Ok(results)
    }
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
    fn test_scoped_factor_zero_rejected() {
        let obj = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Cell,
                factor: 0,
            },
            ScopeLevel::Cell,
            ScopeLevel::Org,
        );
        assert!(obj.is_err());
    }

    #[test]
    fn test_scoped_scope_outside_bounds_rejected() {
        // scope (Org) > max_scope (Cell) → invalid
        let obj = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Org,
                factor: 2,
            },
            ScopeLevel::Cell,
            ScopeLevel::Cell,
        );
        assert!(obj.is_err());

        // scope (Local) < min_durability_scope (Cell) → invalid
        let obj2 = ObjectReplication::new(
            ReplicationPolicy::Scoped {
                scope: ScopeLevel::Local,
                factor: 2,
            },
            ScopeLevel::Cell,
            ScopeLevel::Org,
        );
        assert!(obj2.is_err());
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

    // ========================================================================
    // State Generalization (A1) Tests
    // ========================================================================

    #[test]
    fn test_state_key_new() {
        let key = StateKey::new("/ledger/accounts/alice");
        assert_eq!(key.as_str(), "/ledger/accounts/alice");
        assert_eq!(key.to_string(), "/ledger/accounts/alice");
    }

    #[test]
    fn test_state_key_with_prefix() {
        let key = StateKey::with_prefix("ledger", "accounts/alice");
        assert_eq!(key.as_str(), "/ledger/accounts/alice");
    }

    #[test]
    fn test_state_key_starts_with() {
        let key = StateKey::new("/ledger/accounts/alice");
        assert!(key.starts_with("/ledger"));
        assert!(key.starts_with("/ledger/accounts"));
        assert!(!key.starts_with("/governance"));
    }

    #[test]
    fn test_state_key_as_bytes() {
        let key = StateKey::new("/test");
        assert_eq!(key.as_bytes(), b"/test");
    }

    #[test]
    fn test_state_key_from() {
        let key1: StateKey = String::from("/test").into();
        let key2: StateKey = "/test".into();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_state_key_serde() {
        let key = StateKey::new("/ledger/accounts/alice");
        let json = serde_json::to_string(&key).unwrap();
        let parsed: StateKey = serde_json::from_str(&json).unwrap();
        assert_eq!(key, parsed);
    }

    #[test]
    fn test_state_value_new() {
        let value = StateValue::new(vec![1, 2, 3, 4]);
        assert_eq!(value.as_bytes(), &[1, 2, 3, 4]);
        assert_eq!(value.len(), 4);
        assert!(!value.is_empty());
    }

    #[test]
    fn test_state_value_from_string() {
        let value = StateValue::from_string("hello");
        assert_eq!(value.as_bytes(), b"hello");
    }

    #[test]
    fn test_state_value_into_bytes() {
        let value = StateValue::new(vec![1, 2, 3]);
        let bytes = value.into_bytes();
        assert_eq!(bytes, vec![1, 2, 3]);
    }

    #[test]
    fn test_state_value_empty() {
        let empty = StateValue::new(vec![]);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_state_value_from() {
        let v1: StateValue = vec![1, 2, 3].into();
        let v2: StateValue = [1u8, 2, 3].as_slice().into();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_state_value_serde() {
        let value = StateValue::new(vec![1, 2, 3, 4]);
        let json = serde_json::to_string(&value).unwrap();
        let parsed: StateValue = serde_json::from_str(&json).unwrap();
        assert_eq!(value, parsed);
    }

    #[test]
    fn test_state_op_constructors() {
        let get = StateOp::get("/key");
        assert!(matches!(get, StateOp::Get { key } if key.as_str() == "/key"));

        let put = StateOp::put("/key", vec![1, 2, 3]);
        assert!(matches!(put, StateOp::Put { key, value } if key.as_str() == "/key" && value.len() == 3));

        let delete = StateOp::delete("/key");
        assert!(matches!(delete, StateOp::Delete { key } if key.as_str() == "/key"));

        let list = StateOp::list("/prefix");
        assert!(matches!(list, StateOp::List { prefix } if prefix == "/prefix"));
    }

    #[test]
    fn test_state_op_serde() {
        let ops = vec![
            StateOp::get("/test"),
            StateOp::put("/test", vec![1, 2]),
            StateOp::delete("/test"),
            StateOp::list("/prefix"),
        ];

        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let parsed: StateOp = serde_json::from_str(&json).unwrap();
            assert_eq!(op, parsed);
        }
    }

    #[test]
    fn test_state_scope_ordering() {
        assert!(StateScope::Cell < StateScope::Coop);
        assert!(StateScope::Coop < StateScope::Federation);
        assert!(StateScope::Federation < StateScope::Commons);
    }

    #[test]
    fn test_state_scope_includes() {
        assert!(StateScope::Commons.includes(StateScope::Cell));
        assert!(StateScope::Commons.includes(StateScope::Coop));
        assert!(StateScope::Coop.includes(StateScope::Coop));
        assert!(!StateScope::Cell.includes(StateScope::Coop));
    }

    #[test]
    fn test_state_scope_display() {
        assert_eq!(StateScope::Cell.to_string(), "cell");
        assert_eq!(StateScope::Coop.to_string(), "coop");
        assert_eq!(StateScope::Federation.to_string(), "federation");
        assert_eq!(StateScope::Commons.to_string(), "commons");
    }

    #[test]
    fn test_state_scope_default() {
        assert_eq!(StateScope::default(), StateScope::Cell);
    }

    #[test]
    fn test_state_scope_to_scope_level() {
        assert_eq!(StateScope::Cell.to_scope_level(), ScopeLevel::Cell);
        assert_eq!(StateScope::Coop.to_scope_level(), ScopeLevel::Org);
        assert_eq!(StateScope::Federation.to_scope_level(), ScopeLevel::Federation);
        assert_eq!(StateScope::Commons.to_scope_level(), ScopeLevel::Commons);
    }

    #[test]
    fn test_state_scope_from_scope_level() {
        assert_eq!(StateScope::from_scope_level(ScopeLevel::Local), StateScope::Cell);
        assert_eq!(StateScope::from_scope_level(ScopeLevel::Cell), StateScope::Cell);
        assert_eq!(StateScope::from_scope_level(ScopeLevel::Org), StateScope::Coop);
        assert_eq!(StateScope::from_scope_level(ScopeLevel::Federation), StateScope::Federation);
        assert_eq!(StateScope::from_scope_level(ScopeLevel::Commons), StateScope::Commons);
    }

    #[test]
    fn test_state_scope_serde() {
        for scope in [
            StateScope::Cell,
            StateScope::Coop,
            StateScope::Federation,
            StateScope::Commons,
        ] {
            let json = serde_json::to_string(&scope).unwrap();
            let parsed: StateScope = serde_json::from_str(&json).unwrap();
            assert_eq!(scope, parsed);
        }
    }
}
