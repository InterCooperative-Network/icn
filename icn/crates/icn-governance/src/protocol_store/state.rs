//! Protocol Parameter Storage - Core State Machine
//!
//! This module defines the core state machine for protocol parameter storage:
//! - Error types and their classification
//! - Storage trait definition
//! - State structs for both implementations
//! - Constants and helper functions

use crate::protocol::{
    ParameterChange, ParameterScope, ParameterValidationError, PendingChangeId,
    PendingParameterChange, ProtocolParameter, KNOWN_PARAMETER_CATEGORIES,
};
use anyhow::Result;
use sled::Db;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::warn;

// ============================================================================
// ParameterStoreError
// ============================================================================

/// Errors that can occur during parameter store operations
///
/// This error type categorizes errors to help callers determine appropriate
/// handling (e.g., retry for transient errors, fail fast for validation errors).
#[derive(Debug, thiserror::Error)]
pub enum ParameterStoreError {
    /// Concurrent modification detected (RETRYABLE)
    ///
    /// Another process updated the parameter between read and write.
    /// Caller should re-read the parameter and retry with the new version.
    #[error("Concurrent modification detected for parameter '{parameter_id}': expected version {expected_version}, found {actual_version}. Please retry.")]
    ConcurrentModification {
        /// The parameter that had a version conflict
        parameter_id: String,
        /// The version the caller expected
        expected_version: u64,
        /// The actual version in storage
        actual_version: u64,
    },

    /// Validation failed (NOT RETRYABLE)
    ///
    /// The parameter value violates constraints.
    /// Caller must fix the input value before retrying.
    #[error("Validation failed: {0}")]
    Validation(#[from] ParameterValidationError),

    /// Scope override not allowed (NOT RETRYABLE)
    ///
    /// The parameter does not allow overrides at the requested scope.
    #[error("Parameter '{parameter_id}' does not allow scope overrides")]
    ScopeOverrideNotAllowed {
        /// The parameter that cannot be overridden
        parameter_id: String,
    },

    /// Storage error (POTENTIALLY RETRYABLE)
    ///
    /// An I/O or database error occurred. May be transient (disk full, network issue)
    /// or permanent (corruption). Check the inner error for details.
    #[error("Storage error: {message}")]
    Storage {
        /// Human-readable error description
        message: String,
        /// Whether this error is likely transient and retryable
        is_transient: bool,
    },

    /// Lock poisoned (NOT RETRYABLE)
    ///
    /// A thread panicked while holding a lock, leaving the store in an
    /// inconsistent state. This indicates a bug in the application.
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),

    /// Parameter not found (NOT RETRYABLE)
    ///
    /// The requested parameter does not exist.
    #[error("Parameter not found: '{0}'")]
    NotFound(String),
}

impl ParameterStoreError {
    /// Returns true if this error is transient and the operation may succeed on retry
    ///
    /// # Retryable errors:
    /// - `ConcurrentModification`: Re-read and retry with correct version
    /// - `Storage` with `is_transient: true`: Wait and retry
    ///
    /// # Non-retryable errors:
    /// - `Validation`: Fix the input before retrying
    /// - `ScopeOverrideNotAllowed`: Cannot override this parameter
    /// - `LockPoisoned`: Application bug, requires restart
    /// - `NotFound`: Parameter doesn't exist
    /// - `Storage` with `is_transient: false`: Likely corruption or config issue
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::ConcurrentModification { .. } => true,
            Self::Storage { is_transient, .. } => *is_transient,
            Self::Validation(_) => false,
            Self::ScopeOverrideNotAllowed { .. } => false,
            Self::LockPoisoned(_) => false,
            Self::NotFound(_) => false,
        }
    }

    /// Create a storage error from an anyhow error, classifying transience
    pub fn storage(err: impl std::fmt::Display, is_transient: bool) -> Self {
        Self::Storage {
            message: err.to_string(),
            is_transient,
        }
    }

    /// Create a concurrent modification error
    pub fn concurrent_modification(
        parameter_id: impl Into<String>,
        expected_version: u64,
        actual_version: u64,
    ) -> Self {
        Self::ConcurrentModification {
            parameter_id: parameter_id.into(),
            expected_version,
            actual_version,
        }
    }
}

/// Maximum number of history entries to keep per parameter ID.
///
/// This prevents unbounded growth from DoS via repeated parameter changes.
/// History beyond this limit is automatically pruned on each set() call.
///
/// IMPORTANT: History is shared across ALL scopes for the same parameter ID.
/// For example, changes to "gossip.fanout" for Global, Federation:A, and
/// Cooperative:B all share the same 100-entry history pool. This means:
/// - 24 parameters × 100 entries = 2,400 max entries (not 24 × 100 × N scopes)
/// - Memory impact: ~2,400 entries × ~200 bytes = ~500KB worst case
/// - This shared design prevents memory exhaustion from many scoped overrides
///
/// Rationale for 100 entries:
/// - Sufficient for audit trail (typically shows last few months of changes)
/// - Small enough to prevent memory issues with many parameters
/// - Matches common governance cadence (quarterly reviews = ~4 changes/year)
pub const MAX_HISTORY_ENTRIES_PER_PARAM: usize = 100;

/// Warn if a parameter uses an unknown category
///
/// This doesn't prevent the operation but logs a warning to help catch
/// typos or non-standard parameter naming.
pub(crate) fn warn_unknown_category(id: &str) {
    if let Some(category) = id.split('.').next() {
        if !KNOWN_PARAMETER_CATEGORIES.contains(&category) {
            warn!(
                parameter_id = %id,
                category = %category,
                known_categories = ?KNOWN_PARAMETER_CATEGORIES,
                "Parameter uses unknown category. Consider using a known category \
                 or adding this category to KNOWN_PARAMETER_CATEGORIES if intentional."
            );
        }
    }
}

/// Warning threshold for total history entries across all parameters.
///
/// When exceeded, a warning is logged to alert operators about potential
/// accumulation issues. Note that with MAX_HISTORY_ENTRIES_PER_PARAM = 100
/// and history shared across scopes, reaching 10,000 entries requires:
/// - ~100 distinct parameters all at their 100-entry limit, OR
/// - Rapid parameter churn faster than the auto-prune can handle
///
/// Rationale for 10,000:
/// - 4x the expected maximum (24 params × 100 entries = 2,400)
/// - Provides headroom for future parameter additions
/// - Triggers investigation before reaching concerning levels (~50KB+ of history)
pub const GLOBAL_HISTORY_WARNING_THRESHOLD: usize = 10_000;

// ProtocolParameterStore trait is defined in icn-kernel-api::protocol_params
// and re-exported via crate::protocol

// ============================================================================
// InMemoryParameterStore State
// ============================================================================

/// In-memory implementation for testing
#[derive(Clone, Default)]
pub struct InMemoryParameterStore {
    /// Global parameters: id -> ProtocolParameter
    pub(super) params: Arc<RwLock<HashMap<String, ProtocolParameter>>>,
    /// Scoped parameters: (scope_key, id) -> ProtocolParameter
    pub(super) scoped_params: Arc<RwLock<HashMap<(String, String), ProtocolParameter>>>,
    /// Reverse index: parameter_id -> Vec<scope_key> for O(1) delete
    pub(super) scoped_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// History: id -> Vec<ParameterChange>
    pub(super) history: Arc<RwLock<HashMap<String, Vec<ParameterChange>>>>,
    /// Pending changes for delayed execution: pending_change_id -> PendingParameterChange
    pub(super) pending_changes: Arc<RwLock<HashMap<PendingChangeId, PendingParameterChange>>>,
}

impl InMemoryParameterStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn scope_key(scope: &ParameterScope) -> String {
        match scope {
            ParameterScope::Global => "global".to_string(),
            ParameterScope::Federation { id } => format!("fed:{}", id.as_str()),
            ParameterScope::Cooperative { id } => format!("coop:{}", id.as_str()),
        }
    }
}

// ============================================================================
// SledParameterStore State
// ============================================================================

/// Sled-backed persistent parameter store
///
/// This is the recommended implementation for production use.
pub struct SledParameterStore {
    pub(super) db: Arc<Db>,
}

impl SledParameterStore {
    /// Create a new Sled-backed parameter store
    pub fn new(db: Arc<Db>) -> Result<Self> {
        tracing::debug!("SledParameterStore initialized");
        Ok(Self { db })
    }

    /// Create a temporary on-disk store for testing.
    ///
    /// The database path is created under `ICN_TEST_TMPDIR`, `TMPDIR`, or `/tmp`.
    #[cfg(test)]
    pub fn temporary() -> Result<Self> {
        let base = std::env::var_os("ICN_TEST_TMPDIR")
            .or_else(|| std::env::var_os("TMPDIR"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let unique = format!(
            "icn-governance-protocol-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = base.join(unique);
        std::fs::create_dir_all(&path)
            .map_err(|e| anyhow::anyhow!("Failed to create temp db directory {:?}: {e}", path))?;

        let db = sled::Config::new()
            .path(&path)
            .temporary(true)
            .open()
            .map_err(|e| anyhow::anyhow!("Failed to open temp db: {e}"))?;
        Self::new(Arc::new(db))
    }

    // Key generation
    pub(super) fn param_key(id: &str) -> Vec<u8> {
        format!("param:{id}").into_bytes()
    }

    pub(super) fn scoped_param_key(scope: &ParameterScope, id: &str) -> Vec<u8> {
        let scope_str = match scope {
            ParameterScope::Global => "global".to_string(),
            ParameterScope::Federation { id: eid } => format!("fed:{}", eid.as_str()),
            ParameterScope::Cooperative { id: eid } => format!("coop:{}", eid.as_str()),
        };
        format!("param_scope:{scope_str}:{id}").into_bytes()
    }

    pub(super) fn history_key(id: &str, timestamp: u64, nonce: u64) -> Vec<u8> {
        // Include nonce to ensure uniqueness even if multiple updates happen in same second
        format!("history:{id}:{timestamp:020}:{nonce:020}").into_bytes()
    }

    pub(super) fn history_prefix(id: &str) -> Vec<u8> {
        format!("history:{id}:").into_bytes()
    }

    /// Reverse index key: maps parameter ID to list of scoped keys
    pub(super) fn param_index_key(id: &str) -> Vec<u8> {
        format!("param_idx:{id}").into_bytes()
    }

    /// Get the scope string for a scoped parameter key (for reverse index)
    pub(super) fn scope_str(scope: &ParameterScope) -> String {
        match scope {
            ParameterScope::Global => "global".to_string(),
            ParameterScope::Federation { id: eid } => format!("fed:{}", eid.as_str()),
            ParameterScope::Cooperative { id: eid } => format!("coop:{}", eid.as_str()),
        }
    }

    // Serialization using JSON for flexibility with tagged enums
    pub(super) fn serialize<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(value).map_err(|e| anyhow::anyhow!("Serialization failed: {e}"))
    }

    pub(super) fn deserialize_param(bytes: &[u8]) -> Result<ProtocolParameter> {
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("Deserialization failed: {e}"))
    }

    pub(super) fn deserialize_change(bytes: &[u8]) -> Result<ParameterChange> {
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("Deserialization failed: {e}"))
    }

    // Pending change key generation
    // Key schema:
    // - `pending:{id}` - main storage
    // - `pending_idx:{effective_at:020}:{id}` - time-sorted index for scheduler

    pub(super) fn pending_key(id: &str) -> Vec<u8> {
        format!("pending:{id}").into_bytes()
    }

    pub(super) fn pending_prefix() -> Vec<u8> {
        b"pending:".to_vec()
    }

    pub(super) fn pending_time_index_key(effective_at: u64, id: &str) -> Vec<u8> {
        // Use zero-padded timestamp for lexicographic ordering
        format!("pending_idx:{effective_at:020}:{id}").into_bytes()
    }

    pub(super) fn pending_time_index_prefix() -> Vec<u8> {
        b"pending_idx:".to_vec()
    }

    pub(super) fn deserialize_pending_change(bytes: &[u8]) -> Result<PendingParameterChange> {
        serde_json::from_slice(bytes)
            .map_err(|e| anyhow::anyhow!("Deserialization of pending change failed: {e}"))
    }
}
