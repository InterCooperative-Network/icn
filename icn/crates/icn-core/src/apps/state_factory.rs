//! Namespaced State Factory
//!
//! Creates isolated state handles for apps. Each app gets its own namespace
//! and cannot access other apps' state without explicit capability.
//!
//! # Namespace Format
//!
//! `/{publisher}/{app}/{store}`
//!
//! Example: `/did:icn:foundation/trust/attestations`
//!
//! # Isolation
//!
//! - Apps can only access their own namespace
//! - Cross-app access requires explicit capability grants
//! - Kernel enforces namespace boundaries

use super::manifest::{LogOrdering, StateConfig};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Namespace identifier for an app.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct AppNamespace {
    /// Publisher DID
    pub publisher: String,
    /// App name
    pub app: String,
}

/// Error for invalid namespace components.
#[derive(Clone, Debug, thiserror::Error)]
pub enum NamespaceError {
    /// Path traversal attempt detected
    #[error("Invalid namespace component: path traversal detected in '{0}'")]
    PathTraversal(String),
    /// Null byte in component
    #[error("Invalid namespace component: null byte in '{0}'")]
    NullByte(String),
    /// Empty component
    #[error("Invalid namespace component: empty {0}")]
    Empty(&'static str),
}

impl AppNamespace {
    /// Create a new app namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if the publisher or app name contains:
    /// - Path traversal sequences (`..`, `/`, `\`)
    /// - Null bytes
    /// - Empty strings
    pub fn new(
        publisher: impl Into<String>,
        app: impl Into<String>,
    ) -> Result<Self, NamespaceError> {
        let publisher = publisher.into();
        let app = app.into();

        // Validate publisher
        Self::validate_component(&publisher, "publisher")?;

        // Validate app name
        Self::validate_component(&app, "app")?;

        Ok(Self { publisher, app })
    }

    /// Create a namespace without validation (for internal use only).
    ///
    /// # Safety
    ///
    /// Caller must ensure the components are already validated.
    #[cfg(test)]
    pub(crate) fn new_unchecked(publisher: impl Into<String>, app: impl Into<String>) -> Self {
        Self {
            publisher: publisher.into(),
            app: app.into(),
        }
    }

    /// Validate a namespace component for path traversal attacks.
    fn validate_component(component: &str, name: &'static str) -> Result<(), NamespaceError> {
        // Check for empty
        if component.is_empty() {
            return Err(NamespaceError::Empty(name));
        }

        // Check for null bytes
        if component.contains('\0') {
            return Err(NamespaceError::NullByte(component.to_string()));
        }

        // Check for path traversal sequences
        // Note: We allow ":" in DIDs like "did:icn:..."
        if component.contains("..") || component.contains('/') || component.contains('\\') {
            return Err(NamespaceError::PathTraversal(component.to_string()));
        }

        Ok(())
    }

    /// Get the namespace path.
    pub fn path(&self) -> String {
        format!("/{}/{}", self.publisher, self.app)
    }

    /// Get a full key path for a store.
    pub fn store_path(&self, store: &str) -> String {
        format!("/{}/{}/{}", self.publisher, self.app, store)
    }

    /// Get a full key path for a specific key within a store.
    pub fn key_path(&self, store: &str, key: &str) -> String {
        format!("/{}/{}/{}/{}", self.publisher, self.app, store, key)
    }
}

impl std::fmt::Display for AppNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path())
    }
}

/// State handles for an app.
///
/// Contains all the state resources declared in the app's manifest.
#[derive(Clone)]
pub struct AppState {
    /// App namespace
    pub namespace: AppNamespace,
    /// Log handles
    pub logs: HashMap<String, LogHandle>,
    /// KV handles
    pub kv: HashMap<String, KvHandle>,
    /// Blob handles
    pub blobs: HashMap<String, BlobHandle>,
}

impl AppState {
    /// Get a log handle by name.
    pub fn log(&self, name: &str) -> Option<&LogHandle> {
        self.logs.get(name)
    }

    /// Get a KV handle by name.
    pub fn kv(&self, name: &str) -> Option<&KvHandle> {
        self.kv.get(name)
    }

    /// Get a blob handle by name.
    pub fn blob(&self, name: &str) -> Option<&BlobHandle> {
        self.blobs.get(name)
    }
}

/// Handle to a namespaced log.
#[derive(Clone)]
pub struct LogHandle {
    /// Full path to this log
    pub path: String,
    /// Ordering guarantee
    pub ordering: LogOrdering,
    /// Internal storage
    storage: Arc<RwLock<LogStorage>>,
}

impl LogHandle {
    /// Create a new log handle.
    fn new(namespace: &AppNamespace, name: &str, ordering: LogOrdering) -> Self {
        Self {
            path: namespace.store_path(name),
            ordering,
            storage: Arc::new(RwLock::new(LogStorage::new())),
        }
    }

    /// Append an entry to the log.
    pub async fn append(&self, data: Vec<u8>) -> Result<u64, StateError> {
        let mut storage = self.storage.write().await;
        let offset = storage.entries.len() as u64;
        storage.entries.push(LogEntry {
            offset,
            data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        });
        Ok(offset)
    }

    /// Append with idempotency key.
    pub async fn append_idempotent(
        &self,
        data: Vec<u8>,
        idempotency_key: &str,
    ) -> Result<u64, StateError> {
        let mut storage = self.storage.write().await;

        // Check if we already have this key
        if let Some(&offset) = storage.idempotency_keys.get(idempotency_key) {
            return Ok(offset);
        }

        let offset = storage.entries.len() as u64;
        storage.entries.push(LogEntry {
            offset,
            data,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        });
        storage
            .idempotency_keys
            .insert(idempotency_key.to_string(), offset);
        Ok(offset)
    }

    /// Read entries from the log.
    pub async fn read(&self, from: u64, limit: usize) -> Result<Vec<LogEntry>, StateError> {
        let storage = self.storage.read().await;
        let from_idx = from as usize;
        if from_idx >= storage.entries.len() {
            return Ok(vec![]);
        }
        let end = std::cmp::min(from_idx + limit, storage.entries.len());
        Ok(storage.entries[from_idx..end].to_vec())
    }

    /// Get the current length of the log.
    pub async fn len(&self) -> u64 {
        self.storage.read().await.entries.len() as u64
    }

    /// Check if the log is empty.
    pub async fn is_empty(&self) -> bool {
        self.storage.read().await.entries.is_empty()
    }
}

/// Internal log storage.
struct LogStorage {
    entries: Vec<LogEntry>,
    idempotency_keys: HashMap<String, u64>,
}

impl LogStorage {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            idempotency_keys: HashMap::new(),
        }
    }
}

/// A log entry.
#[derive(Clone, Debug)]
pub struct LogEntry {
    /// Offset in the log
    pub offset: u64,
    /// Entry data
    pub data: Vec<u8>,
    /// Timestamp (milliseconds since epoch)
    pub timestamp: u64,
}

/// Handle to a namespaced KV store.
#[derive(Clone)]
pub struct KvHandle {
    /// Full path to this store
    pub path: String,
    /// Internal storage
    storage: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl KvHandle {
    /// Create a new KV handle.
    fn new(namespace: &AppNamespace, name: &str) -> Self {
        Self {
            path: namespace.store_path(name),
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get a value.
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StateError> {
        let storage = self.storage.read().await;
        Ok(storage.get(key).cloned())
    }

    /// Set a value.
    pub async fn set(&self, key: &str, value: Vec<u8>) -> Result<(), StateError> {
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), value);
        Ok(())
    }

    /// Delete a value.
    pub async fn delete(&self, key: &str) -> Result<bool, StateError> {
        let mut storage = self.storage.write().await;
        Ok(storage.remove(key).is_some())
    }

    /// Check if a key exists.
    pub async fn exists(&self, key: &str) -> Result<bool, StateError> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(key))
    }

    /// List keys with a prefix.
    pub async fn list(&self, prefix: &str) -> Result<Vec<String>, StateError> {
        let storage = self.storage.read().await;
        Ok(storage
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    /// Get all keys.
    pub async fn keys(&self) -> Result<Vec<String>, StateError> {
        let storage = self.storage.read().await;
        Ok(storage.keys().cloned().collect())
    }
}

/// Handle to a namespaced blob store.
#[derive(Clone)]
pub struct BlobHandle {
    /// Full path to this store
    pub path: String,
    /// Maximum blob size
    pub max_size: Option<u64>,
    /// Internal storage
    storage: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl BlobHandle {
    /// Create a new blob handle.
    fn new(namespace: &AppNamespace, name: &str, max_size: Option<u64>) -> Self {
        Self {
            path: namespace.store_path(name),
            max_size,
            storage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a blob.
    pub async fn put(&self, hash: &str, data: Vec<u8>) -> Result<(), StateError> {
        if let Some(max) = self.max_size {
            if data.len() as u64 > max {
                return Err(StateError::BlobTooLarge {
                    size: data.len() as u64,
                    max,
                });
            }
        }
        let mut storage = self.storage.write().await;
        storage.insert(hash.to_string(), data);
        Ok(())
    }

    /// Get a blob.
    pub async fn get(&self, hash: &str) -> Result<Option<Vec<u8>>, StateError> {
        let storage = self.storage.read().await;
        Ok(storage.get(hash).cloned())
    }

    /// Check if a blob exists.
    pub async fn exists(&self, hash: &str) -> Result<bool, StateError> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(hash))
    }

    /// Delete a blob.
    pub async fn delete(&self, hash: &str) -> Result<bool, StateError> {
        let mut storage = self.storage.write().await;
        Ok(storage.remove(hash).is_some())
    }
}

/// Factory for creating namespaced state handles.
#[derive(Clone, Default)]
pub struct StateFactory {
    /// Existing app states (for sharing)
    states: Arc<RwLock<HashMap<AppNamespace, AppState>>>,
}

impl StateFactory {
    /// Create a new state factory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create state handles for an app.
    pub async fn create_for_app(
        &self,
        namespace: AppNamespace,
        config: &StateConfig,
    ) -> Result<AppState, StateError> {
        // Check if already exists
        {
            let states = self.states.read().await;
            if states.contains_key(&namespace) {
                return Err(StateError::NamespaceExists(namespace.path()));
            }
        }

        // Create log handles
        let mut logs = HashMap::new();
        for log_config in &config.logs {
            logs.insert(
                log_config.name.clone(),
                LogHandle::new(&namespace, &log_config.name, log_config.ordering),
            );
        }

        // Create KV handles
        let mut kv = HashMap::new();
        for kv_config in &config.kv {
            kv.insert(
                kv_config.name.clone(),
                KvHandle::new(&namespace, &kv_config.name),
            );
        }

        // Create blob handles
        let mut blobs = HashMap::new();
        for blob_config in &config.blobs {
            blobs.insert(
                blob_config.name.clone(),
                BlobHandle::new(&namespace, &blob_config.name, blob_config.max_size),
            );
        }

        let state = AppState {
            namespace: namespace.clone(),
            logs,
            kv,
            blobs,
        };

        // Store for sharing
        {
            let mut states = self.states.write().await;
            states.insert(namespace, state.clone());
        }

        Ok(state)
    }

    /// Get existing state for an app.
    pub async fn get(&self, namespace: &AppNamespace) -> Option<AppState> {
        let states = self.states.read().await;
        states.get(namespace).cloned()
    }

    /// Remove state for an app (cleanup on uninstall).
    pub async fn remove(&self, namespace: &AppNamespace) -> bool {
        let mut states = self.states.write().await;
        states.remove(namespace).is_some()
    }

    /// List all namespaces.
    pub async fn namespaces(&self) -> Vec<AppNamespace> {
        let states = self.states.read().await;
        states.keys().cloned().collect()
    }
}

/// State operation errors.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// Namespace already exists
    #[error("Namespace already exists: {0}")]
    NamespaceExists(String),

    /// Blob too large
    #[error("Blob size {size} exceeds maximum {max}")]
    BlobTooLarge { size: u64, max: u64 },

    /// Store not found
    #[error("Store not found: {0}")]
    StoreNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::manifest::{KvConfig, LogConfig};

    #[test]
    fn test_app_namespace() {
        let ns = AppNamespace::new("did:icn:test", "echo").unwrap();
        assert_eq!(ns.path(), "/did:icn:test/echo");
        assert_eq!(ns.store_path("data"), "/did:icn:test/echo/data");
        assert_eq!(ns.key_path("data", "key1"), "/did:icn:test/echo/data/key1");
    }

    #[test]
    fn test_app_namespace_path_traversal_rejection() {
        // Test ".." path traversal
        assert!(matches!(
            AppNamespace::new("did:icn:../../root", "app"),
            Err(NamespaceError::PathTraversal(_))
        ));

        // Test "/" in component
        assert!(matches!(
            AppNamespace::new("did:icn:test/../../etc", "app"),
            Err(NamespaceError::PathTraversal(_))
        ));

        // Test backslash
        assert!(matches!(
            AppNamespace::new("did:icn:test", "app\\..\\.."),
            Err(NamespaceError::PathTraversal(_))
        ));

        // Test ".." in app name
        assert!(matches!(
            AppNamespace::new("did:icn:test", ".."),
            Err(NamespaceError::PathTraversal(_))
        ));
    }

    #[test]
    fn test_app_namespace_null_byte_rejection() {
        assert!(matches!(
            AppNamespace::new("did:icn:test\0evil", "app"),
            Err(NamespaceError::NullByte(_))
        ));

        assert!(matches!(
            AppNamespace::new("did:icn:test", "app\0"),
            Err(NamespaceError::NullByte(_))
        ));
    }

    #[test]
    fn test_app_namespace_empty_rejection() {
        assert!(matches!(
            AppNamespace::new("", "app"),
            Err(NamespaceError::Empty("publisher"))
        ));

        assert!(matches!(
            AppNamespace::new("did:icn:test", ""),
            Err(NamespaceError::Empty("app"))
        ));
    }

    #[test]
    fn test_app_namespace_valid_did_with_colons() {
        // DIDs contain colons which should be allowed
        let ns = AppNamespace::new("did:icn:abc123", "my-app").unwrap();
        assert_eq!(ns.publisher, "did:icn:abc123");
        assert_eq!(ns.app, "my-app");
    }

    #[tokio::test]
    async fn test_state_factory_create() {
        let factory = StateFactory::new();
        let namespace = AppNamespace::new("did:icn:test", "echo").unwrap();

        let config = StateConfig {
            logs: vec![LogConfig {
                name: "events".to_string(),
                ordering: LogOrdering::Total,
            }],
            kv: vec![KvConfig {
                name: "cache".to_string(),
                ..Default::default()
            }],
            blobs: vec![],
        };

        let state = factory
            .create_for_app(namespace.clone(), &config)
            .await
            .unwrap();

        assert!(state.log("events").is_some());
        assert!(state.kv("cache").is_some());
        assert!(state.log("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_state_factory_duplicate_namespace() {
        let factory = StateFactory::new();
        let namespace = AppNamespace::new("did:icn:test", "echo").unwrap();
        let config = StateConfig::default();

        factory
            .create_for_app(namespace.clone(), &config)
            .await
            .unwrap();

        let result = factory.create_for_app(namespace, &config).await;
        assert!(matches!(result, Err(StateError::NamespaceExists(_))));
    }

    #[tokio::test]
    async fn test_log_handle_append_read() {
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let log = LogHandle::new(&namespace, "events", LogOrdering::Total);

        // Append entries
        let offset1 = log.append(b"event1".to_vec()).await.unwrap();
        let offset2 = log.append(b"event2".to_vec()).await.unwrap();

        assert_eq!(offset1, 0);
        assert_eq!(offset2, 1);
        assert_eq!(log.len().await, 2);

        // Read entries
        let entries = log.read(0, 10).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].data, b"event1");
        assert_eq!(entries[1].data, b"event2");
    }

    #[tokio::test]
    async fn test_log_handle_idempotent() {
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let log = LogHandle::new(&namespace, "events", LogOrdering::Total);

        let offset1 = log
            .append_idempotent(b"event1".to_vec(), "key1")
            .await
            .unwrap();
        let offset2 = log
            .append_idempotent(b"event1-dup".to_vec(), "key1")
            .await
            .unwrap();

        // Same key should return same offset
        assert_eq!(offset1, offset2);
        assert_eq!(log.len().await, 1);
    }

    #[tokio::test]
    async fn test_kv_handle_crud() {
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let kv = KvHandle::new(&namespace, "cache");

        // Set and get
        kv.set("key1", b"value1".to_vec()).await.unwrap();
        let value = kv.get("key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));

        // Exists
        assert!(kv.exists("key1").await.unwrap());
        assert!(!kv.exists("key2").await.unwrap());

        // Delete
        assert!(kv.delete("key1").await.unwrap());
        assert!(!kv.exists("key1").await.unwrap());
    }

    #[tokio::test]
    async fn test_kv_handle_list() {
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let kv = KvHandle::new(&namespace, "cache");

        kv.set("user:1", b"alice".to_vec()).await.unwrap();
        kv.set("user:2", b"bob".to_vec()).await.unwrap();
        kv.set("other", b"other".to_vec()).await.unwrap();

        let user_keys = kv.list("user:").await.unwrap();
        assert_eq!(user_keys.len(), 2);
    }

    #[tokio::test]
    async fn test_blob_handle_size_limit() {
        let namespace = AppNamespace::new_unchecked("did:icn:test", "echo");
        let blob = BlobHandle::new(&namespace, "files", Some(100));

        // Small blob should work
        blob.put("hash1", vec![0u8; 50]).await.unwrap();

        // Large blob should fail
        let result = blob.put("hash2", vec![0u8; 200]).await;
        assert!(matches!(result, Err(StateError::BlobTooLarge { .. })));
    }
}
