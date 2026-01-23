//! Protocol Parameter Storage
//!
//! This module provides persistent storage for protocol parameters.
//! Parameters are stored in Sled with history tracking for audit trails.
//!
//! # Key Schema
//!
//! - `param:{id}` -> ProtocolParameter (JSON)
//! - `param_scope:{scope}:{id}` -> ProtocolParameter (JSON) for scoped overrides
//! - `param_idx:{id}` -> JSON array of scoped key strings (reverse index for O(1) delete)
//! - `history:{id}:{timestamp}` -> ParameterChange (JSON)
//!
//! # Scope Resolution
//!
//! When getting a parameter value, the store resolves scopes in order:
//! 1. Cooperative scope (most specific)
//! 2. Federation scope
//! 3. Global scope (default)
//!
//! # Version Management
//!
//! Each parameter scope maintains an **independent version counter**:
//!
//! - Global parameter: versions 0, 1, 2, ... (tracks global changes)
//! - Federation override: versions 0, 1, 2, ... (tracks federation-specific changes)
//! - Cooperative override: versions 0, 1, 2, ... (tracks cooperative-specific changes)
//!
//! This design choice has important implications:
//!
//! - **Independent auditing**: Each scope's version history is self-contained
//! - **No global-to-scope inheritance**: Creating a federation override starts at v0,
//!   not at the global parameter's current version
//! - **Conflict resolution**: Updates to the same scope conflict via version mismatch;
//!   updates to different scopes are independent
//!
//! For example, if global `governance.min_quorum` is at version 5, creating a
//! cooperative override for `coop:tech-workers` starts that override at version 0.
//! The global and cooperative versions evolve independently thereafter.
//!
//! # Performance Characteristics
//!
//! - **Get**: O(1) for global, O(1) for scoped lookup
//! - **Set**: O(1) with reverse index maintenance
//! - **Delete**: O(k) where k is the number of scoped overrides for that parameter
//!   (uses reverse index for direct lookup instead of O(n) scan)
//! - **History Pagination**: O(h) where h is history count for that parameter.
//!   All entries are loaded and sorted before pagination is applied. This is
//!   acceptable because history is bounded by `MAX_HISTORY_ENTRIES_PER_PARAM`.
//!
//! # Automatic History Pruning
//!
//! History entries are automatically pruned after each `set()` call to prevent
//! unbounded growth (DoS mitigation). The maximum entries per parameter is defined
//! by `MAX_HISTORY_ENTRIES_PER_PARAM` (default: 100).
//!
//! The `prune_history()` method can also be called manually if needed:
//!
//! ```ignore
//! // Keep only the last 50 history entries for a specific parameter
//! store.prune_history("governance.min_quorum", 50)?;
//! ```

use crate::protocol::{
    ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, PendingChangeId,
    PendingChangeStatus, PendingParameterChange, ProtocolParameter, KNOWN_PARAMETER_CATEGORIES,
};
use anyhow::Result;
use icn_entity::EntityId;
use sled::Db;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

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
fn warn_unknown_category(id: &str) {
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

// ============================================================================
// ProtocolParameterStore Trait
// ============================================================================

/// Trait for protocol parameter storage operations
pub trait ProtocolParameterStore: Send + Sync {
    /// Get a parameter by ID (global scope)
    fn get(&self, id: &str) -> Result<Option<ProtocolParameter>>;

    /// Get a parameter with scope resolution
    ///
    /// Resolves the effective value by checking scopes in order:
    /// Cooperative > Federation > Global
    fn get_effective(
        &self,
        id: &str,
        coop_id: Option<&EntityId>,
        fed_id: Option<&EntityId>,
    ) -> Result<Option<ProtocolParameter>>;

    /// Set a parameter value
    ///
    /// Records the change in history if proposal_id is provided.
    fn set(
        &self,
        param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> Result<()>;

    /// List all parameters (global scope only)
    fn list(&self) -> Result<Vec<ProtocolParameter>>;

    /// List all parameters in a category
    fn list_by_category(&self, category: &str) -> Result<Vec<ProtocolParameter>>;

    /// Get change history for a parameter
    fn get_history(&self, id: &str) -> Result<Vec<ParameterChange>>;

    /// Get paginated change history for a parameter
    ///
    /// Returns history entries in chronological order (oldest first).
    /// Use `offset` to skip entries and `limit` to cap the result size.
    /// Also returns the total count for pagination UI.
    ///
    /// # Arguments
    /// - `id`: Parameter ID
    /// - `offset`: Number of entries to skip (for pagination)
    /// - `limit`: Maximum entries to return
    ///
    /// # Returns
    /// Tuple of (entries, total_count)
    ///
    /// # Performance Note
    ///
    /// Current implementation is O(n) where n is total history entries for the parameter.
    /// All entries are loaded, sorted, and then paginated. This is acceptable because:
    /// - Per-parameter history is bounded by `MAX_HISTORY_ENTRIES_PER_PARAM` (100)
    /// - Auto-pruning on each `set()` prevents unbounded growth
    ///
    /// For parameters with high change frequency across many scopes, consider using
    /// `prune_history()` to reduce history size before pagination.
    fn get_history_paginated(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<ParameterChange>, usize)>;

    /// Prune old history entries, keeping only the last `max_entries` per parameter
    ///
    /// Returns the number of entries removed.
    ///
    /// # Arguments
    /// - `id`: Parameter ID whose history to prune
    /// - `max_entries`: Minimum entries to keep (must be >= 1 to prevent accidental
    ///   complete deletion; use `delete()` to remove a parameter entirely)
    ///
    /// # Errors
    /// Returns an error if `max_entries` is 0 to prevent accidental data loss.
    fn prune_history(&self, id: &str, max_entries: usize) -> Result<usize>;

    /// Delete a parameter (for testing/admin only)
    fn delete(&self, id: &str) -> Result<()>;

    /// Check if a parameter exists
    fn exists(&self, id: &str) -> Result<bool>;

    /// Count total parameters
    fn count(&self) -> Result<usize>;

    /// Count total history entries across all parameters
    ///
    /// This is useful for monitoring global history accumulation.
    fn total_history_count(&self) -> Result<usize>;

    /// Validate a new value against a parameter's constraints
    fn validate(
        &self,
        id: &str,
        new_value: &ParameterValue,
    ) -> Result<(), ParameterValidationError>;

    // ========================================
    // Scoped Parameter Operations (for cleanup)
    // ========================================

    /// List all scoped (non-global) parameters
    ///
    /// Returns all parameters that have a scope other than Global
    /// (i.e., Federation or Cooperative scoped overrides).
    /// This is useful for orphan detection and cleanup.
    fn list_scoped_parameters(&self) -> Result<Vec<ProtocolParameter>>;

    /// Delete a specific scoped parameter by ID and scope
    ///
    /// Removes a scoped override without affecting the global parameter.
    /// Returns Ok(true) if the parameter was deleted, Ok(false) if not found.
    ///
    /// # Arguments
    /// - `id`: The parameter ID (e.g., "governance.min_quorum")
    /// - `scope`: The specific scope to delete (must not be Global)
    ///
    /// # Errors
    /// Returns an error if scope is Global (use `delete()` for that).
    fn delete_scoped_parameter(&self, id: &str, scope: &ParameterScope) -> Result<bool>;

    // ========================================
    // Pending Change Methods (Delayed Execution)
    // ========================================

    /// Add a pending parameter change for delayed execution
    ///
    /// The change will be stored and applied when the scheduler runs
    /// after `effective_at` timestamp.
    fn add_pending_change(&self, change: PendingParameterChange) -> Result<()>;

    /// Get a pending change by ID
    fn get_pending_change(&self, id: &PendingChangeId) -> Result<Option<PendingParameterChange>>;

    /// List all pending changes (all statuses)
    fn list_pending_changes(&self) -> Result<Vec<PendingParameterChange>>;

    /// List pending changes for a specific parameter
    fn list_pending_changes_for_parameter(
        &self,
        parameter_id: &str,
    ) -> Result<Vec<PendingParameterChange>>;

    /// Get all pending changes that are due (effective_at <= timestamp)
    ///
    /// Returns only changes with status `Pending` that should be applied.
    /// Results are ordered by `effective_at` (earliest first) for consistent processing.
    fn get_changes_due_before(&self, timestamp: u64) -> Result<Vec<PendingParameterChange>>;

    /// Update a pending change (e.g., mark as applied, superseded)
    fn update_pending_change(&self, change: PendingParameterChange) -> Result<()>;

    /// Cancel a pending change
    ///
    /// Sets the status to `Cancelled` and records the reason.
    fn cancel_pending_change(&self, id: &PendingChangeId, reason: &str) -> Result<()>;

    /// Get count of active pending changes (status = Pending)
    fn count_pending_changes(&self) -> Result<usize>;
}

// ============================================================================
// InMemoryParameterStore
// ============================================================================

/// In-memory implementation for testing
#[derive(Clone, Default)]
pub struct InMemoryParameterStore {
    /// Global parameters: id -> ProtocolParameter
    params: Arc<RwLock<HashMap<String, ProtocolParameter>>>,
    /// Scoped parameters: (scope_key, id) -> ProtocolParameter
    scoped_params: Arc<RwLock<HashMap<(String, String), ProtocolParameter>>>,
    /// Reverse index: parameter_id -> Vec<scope_key> for O(1) delete
    scoped_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// History: id -> Vec<ParameterChange>
    history: Arc<RwLock<HashMap<String, Vec<ParameterChange>>>>,
    /// Pending changes for delayed execution: pending_change_id -> PendingParameterChange
    pending_changes: Arc<RwLock<HashMap<PendingChangeId, PendingParameterChange>>>,
}

impl InMemoryParameterStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self::default()
    }

    fn scope_key(scope: &ParameterScope) -> String {
        match scope {
            ParameterScope::Global => "global".to_string(),
            ParameterScope::Federation { id } => format!("fed:{}", id.as_str()),
            ParameterScope::Cooperative { id } => format!("coop:{}", id.as_str()),
        }
    }
}

impl ProtocolParameterStore for InMemoryParameterStore {
    fn get(&self, id: &str) -> Result<Option<ProtocolParameter>> {
        let params = self
            .params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(params.get(id).cloned())
    }

    fn get_effective(
        &self,
        id: &str,
        coop_id: Option<&EntityId>,
        fed_id: Option<&EntityId>,
    ) -> Result<Option<ProtocolParameter>> {
        let scoped = self
            .scoped_params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        // Try cooperative scope first
        if let Some(coop) = coop_id {
            let key = (format!("coop:{}", coop.as_str()), id.to_string());
            if let Some(param) = scoped.get(&key) {
                return Ok(Some(param.clone()));
            }
        }

        // Try federation scope
        if let Some(fed) = fed_id {
            let key = (format!("fed:{}", fed.as_str()), id.to_string());
            if let Some(param) = scoped.get(&key) {
                return Ok(Some(param.clone()));
            }
        }

        // Fall back to global scope
        drop(scoped);
        self.get(id)
    }

    fn set(
        &self,
        mut param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> Result<()> {
        use crate::protocol_validation::{
            validate_parameter_value, validate_scope_override, validate_version,
        };

        let id = param.id.clone();
        let scope_key = Self::scope_key(&param.scope);

        // Warn about unknown categories (non-blocking)
        warn_unknown_category(&id);

        // Validate and update parameter
        let stored_param = self.get(&id)?;
        let old_value = if let Some(ref stored) = stored_param {
            // Use shared validation helpers for constraint bypass prevention,
            // scope override permission, and optimistic locking
            validate_parameter_value(&param.value, &param, Some(stored))
                .map_err(|e| anyhow::anyhow!("Parameter validation failed for '{id}': {e}"))?;
            validate_scope_override(&id, &param.scope, Some(stored))
                .map_err(|e| anyhow::anyhow!("Parameter '{id}' scope validation failed: {e}"))?;
            validate_version(&id, param.version, stored.version)?;

            // Increment version for the update
            param.version = stored.version + 1;
            Some(stored.value.clone())
        } else {
            // For new parameters (initial setup), validate against the parameter's own constraints
            validate_parameter_value(&param.value, &param, None)
                .map_err(|e| anyhow::anyhow!("Parameter validation failed for '{id}': {e}"))?;
            // New parameter starts at version 0
            param.version = 0;
            None
        };

        // Store the parameter
        if matches!(param.scope, ParameterScope::Global) {
            let mut params = self
                .params
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            params.insert(id.clone(), param.clone());
        } else {
            // Store scoped parameter
            let mut scoped = self
                .scoped_params
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            scoped.insert((scope_key.clone(), id.clone()), param.clone());

            // Update reverse index for O(1) delete
            let mut index = self
                .scoped_index
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            let scopes = index.entry(id.clone()).or_default();
            if !scopes.contains(&scope_key) {
                scopes.push(scope_key);
            }
        }

        // Record history if we had an old value
        if let Some(old) = old_value {
            let change = ParameterChange {
                parameter_id: id.clone(),
                old_value: old,
                new_value: param.value,
                changed_at: icn_time::current_timestamp_secs(),
                proposal_id,
                changed_by,
            };

            let mut history = self
                .history
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            let entries = history.entry(id.clone()).or_default();
            entries.push(change);

            // Auto-prune history to prevent unbounded growth (DoS mitigation)
            if entries.len() > MAX_HISTORY_ENTRIES_PER_PARAM {
                // Sort by timestamp descending (newest first)
                entries.sort_by(|a, b| b.changed_at.cmp(&a.changed_at));
                let pruned = entries.len() - MAX_HISTORY_ENTRIES_PER_PARAM;
                entries.truncate(MAX_HISTORY_ENTRIES_PER_PARAM);
                debug!(parameter_id = %id, pruned_entries = pruned, "Auto-pruned history entries");
            }

            // Check global history size and warn if threshold exceeded
            let total: usize = history.values().map(|v| v.len()).sum();
            if total > GLOBAL_HISTORY_WARNING_THRESHOLD {
                warn!(
                    total_history_entries = total,
                    threshold = GLOBAL_HISTORY_WARNING_THRESHOLD,
                    "Global history size exceeds threshold. Consider reviewing parameter \
                     change frequency or running manual prune_history() on old parameters."
                );
            }
        }

        Ok(())
    }

    fn list(&self) -> Result<Vec<ProtocolParameter>> {
        let params = self
            .params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(params.values().cloned().collect())
    }

    fn list_by_category(&self, category: &str) -> Result<Vec<ProtocolParameter>> {
        let params = self
            .params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(params
            .values()
            .filter(|p| p.category() == category)
            .cloned()
            .collect())
    }

    fn get_history(&self, id: &str) -> Result<Vec<ParameterChange>> {
        let history = self
            .history
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let mut changes = history.get(id).cloned().unwrap_or_default();
        // Sort by timestamp (oldest first) for chronological audit trail
        changes.sort_by_key(|c| c.changed_at);
        Ok(changes)
    }

    fn get_history_paginated(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<ParameterChange>, usize)> {
        let history = self
            .history
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let mut changes = history.get(id).cloned().unwrap_or_default();
        let total = changes.len();

        // Sort by timestamp (oldest first) for chronological audit trail
        changes.sort_by_key(|c| c.changed_at);

        // Apply pagination
        let paginated: Vec<_> = changes.into_iter().skip(offset).take(limit).collect();

        Ok((paginated, total))
    }

    fn prune_history(&self, id: &str, max_entries: usize) -> Result<usize> {
        // Prevent accidental deletion of all history
        if max_entries == 0 {
            return Err(anyhow::anyhow!(
                "max_entries must be >= 1 to prevent accidental data loss. \
                 Use delete() to remove a parameter entirely."
            ));
        }

        let mut history = self
            .history
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(entries) = history.get_mut(id) {
            if entries.len() > max_entries {
                // Sort by timestamp descending (newest first)
                entries.sort_by(|a, b| b.changed_at.cmp(&a.changed_at));
                // Keep only the most recent max_entries
                let removed = entries.len() - max_entries;
                entries.truncate(max_entries);
                return Ok(removed);
            }
        }
        Ok(0)
    }

    fn delete(&self, id: &str) -> Result<()> {
        // Remove global parameter
        let mut params = self
            .params
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        params.remove(id);

        // Use reverse index for O(1) lookup of scoped keys to remove
        let mut index = self
            .scoped_index
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if let Some(scope_keys) = index.remove(id) {
            // Warn if deleting a parameter with active scoped overrides
            // This helps operators avoid accidentally removing cooperative customizations
            if !scope_keys.is_empty() {
                warn!(
                    parameter_id = %id,
                    scoped_overrides = scope_keys.len(),
                    "Deleting parameter with active scoped overrides. \
                     This will remove customizations for {} cooperative(s)/federation(s).",
                    scope_keys.len()
                );
            }

            let mut scoped = self
                .scoped_params
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            for scope_key in scope_keys {
                scoped.remove(&(scope_key, id.to_string()));
            }
        }

        Ok(())
    }

    fn exists(&self, id: &str) -> Result<bool> {
        let params = self
            .params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(params.contains_key(id))
    }

    fn count(&self) -> Result<usize> {
        let params = self
            .params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(params.len())
    }

    fn total_history_count(&self) -> Result<usize> {
        let history = self
            .history
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(history.values().map(|v| v.len()).sum())
    }

    fn validate(
        &self,
        id: &str,
        new_value: &ParameterValue,
    ) -> Result<(), ParameterValidationError> {
        let params = self
            .params
            .read()
            .map_err(|_| ParameterValidationError::NotFound(id.to_string()))?;

        let param = params
            .get(id)
            .ok_or_else(|| ParameterValidationError::NotFound(id.to_string()))?;

        param.validate(new_value)
    }

    fn list_scoped_parameters(&self) -> Result<Vec<ProtocolParameter>> {
        let scoped = self
            .scoped_params
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(scoped.values().cloned().collect())
    }

    fn delete_scoped_parameter(&self, id: &str, scope: &ParameterScope) -> Result<bool> {
        // Cannot use this method for global scope
        if matches!(scope, ParameterScope::Global) {
            anyhow::bail!(
                "Cannot delete global parameter via delete_scoped_parameter(). \
                 Use delete() to remove global parameters."
            );
        }

        let scope_key = Self::scope_key(scope);

        // Remove from scoped params
        let mut scoped = self
            .scoped_params
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let key = (scope_key.clone(), id.to_string());
        let removed = scoped.remove(&key).is_some();

        if removed {
            // Update reverse index
            let mut index = self
                .scoped_index
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

            if let Some(scopes) = index.get_mut(id) {
                scopes.retain(|s| s != &scope_key);
                // Remove the index entry entirely if no scopes remain
                if scopes.is_empty() {
                    index.remove(id);
                }
            }

            debug!(
                parameter_id = %id,
                scope = ?scope,
                "Deleted scoped parameter"
            );
        }

        Ok(removed)
    }

    // ========================================
    // Pending Change Methods (Delayed Execution)
    // ========================================

    fn add_pending_change(&self, change: PendingParameterChange) -> Result<()> {
        let mut pending = self
            .pending_changes
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if pending.contains_key(&change.id) {
            return Err(anyhow::anyhow!(
                "Pending change with ID '{}' already exists",
                change.id
            ));
        }

        debug!(
            pending_change_id = %change.id,
            parameter_id = %change.parameter_id,
            effective_at = change.effective_at,
            "Adding pending parameter change"
        );

        pending.insert(change.id.clone(), change);
        Ok(())
    }

    fn get_pending_change(&self, id: &PendingChangeId) -> Result<Option<PendingParameterChange>> {
        let pending = self
            .pending_changes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(pending.get(id).cloned())
    }

    fn list_pending_changes(&self) -> Result<Vec<PendingParameterChange>> {
        let pending = self
            .pending_changes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let mut changes: Vec<_> = pending.values().cloned().collect();
        // Sort by effective_at for consistent ordering
        changes.sort_by_key(|c| c.effective_at);
        Ok(changes)
    }

    fn list_pending_changes_for_parameter(
        &self,
        parameter_id: &str,
    ) -> Result<Vec<PendingParameterChange>> {
        let pending = self
            .pending_changes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let mut changes: Vec<_> = pending
            .values()
            .filter(|c| c.parameter_id == parameter_id)
            .cloned()
            .collect();
        changes.sort_by_key(|c| c.effective_at);
        Ok(changes)
    }

    fn get_changes_due_before(&self, timestamp: u64) -> Result<Vec<PendingParameterChange>> {
        let pending = self
            .pending_changes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let mut due: Vec<_> = pending
            .values()
            .filter(|c| c.status == PendingChangeStatus::Pending && c.effective_at <= timestamp)
            .cloned()
            .collect();
        // Sort by effective_at (earliest first), with tie-breakers for deterministic ordering.
        // This ensures consistent processing order even when multiple changes have the same
        // effective_at timestamp. Tie-breakers: created_at (earlier wins), then id (lexicographic).
        due.sort_by(|a, b| {
            a.effective_at
                .cmp(&b.effective_at)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(due)
    }

    fn update_pending_change(&self, change: PendingParameterChange) -> Result<()> {
        let mut pending = self
            .pending_changes
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if !pending.contains_key(&change.id) {
            return Err(anyhow::anyhow!(
                "Pending change with ID '{}' not found",
                change.id
            ));
        }

        debug!(
            pending_change_id = %change.id,
            status = %change.status,
            "Updating pending parameter change"
        );

        pending.insert(change.id.clone(), change);
        Ok(())
    }

    fn cancel_pending_change(&self, id: &PendingChangeId, reason: &str) -> Result<()> {
        let mut pending = self
            .pending_changes
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let change = pending
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Pending change with ID '{id}' not found"))?;

        if change.status != PendingChangeStatus::Pending {
            return Err(anyhow::anyhow!(
                "Cannot cancel pending change '{}' with status '{}'",
                id,
                change.status
            ));
        }

        debug!(
            pending_change_id = %id,
            reason = %reason,
            "Cancelling pending parameter change"
        );

        change.mark_cancelled(reason);
        Ok(())
    }

    fn count_pending_changes(&self) -> Result<usize> {
        let pending = self
            .pending_changes
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        Ok(pending
            .values()
            .filter(|c| c.status == PendingChangeStatus::Pending)
            .count())
    }
}

// ============================================================================
// SledParameterStore
// ============================================================================

/// Sled-backed persistent parameter store
///
/// This is the recommended implementation for production use.
pub struct SledParameterStore {
    db: Arc<Db>,
}

impl SledParameterStore {
    /// Create a new Sled-backed parameter store
    pub fn new(db: Arc<Db>) -> Result<Self> {
        debug!("SledParameterStore initialized");
        Ok(Self { db })
    }

    /// Create a temporary in-memory store for testing
    #[cfg(test)]
    pub fn temporary() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| anyhow::anyhow!("Failed to open temp db: {e}"))?;
        Self::new(Arc::new(db))
    }

    // Key generation
    fn param_key(id: &str) -> Vec<u8> {
        format!("param:{id}").into_bytes()
    }

    fn scoped_param_key(scope: &ParameterScope, id: &str) -> Vec<u8> {
        let scope_str = match scope {
            ParameterScope::Global => "global".to_string(),
            ParameterScope::Federation { id: eid } => format!("fed:{}", eid.as_str()),
            ParameterScope::Cooperative { id: eid } => format!("coop:{}", eid.as_str()),
        };
        format!("param_scope:{scope_str}:{id}").into_bytes()
    }

    fn history_key(id: &str, timestamp: u64, nonce: u64) -> Vec<u8> {
        // Include nonce to ensure uniqueness even if multiple updates happen in same second
        format!("history:{id}:{timestamp:020}:{nonce:020}").into_bytes()
    }

    fn history_prefix(id: &str) -> Vec<u8> {
        format!("history:{id}:").into_bytes()
    }

    /// Reverse index key: maps parameter ID to list of scoped keys
    fn param_index_key(id: &str) -> Vec<u8> {
        format!("param_idx:{id}").into_bytes()
    }

    /// Get the scope string for a scoped parameter key (for reverse index)
    fn scope_str(scope: &ParameterScope) -> String {
        match scope {
            ParameterScope::Global => "global".to_string(),
            ParameterScope::Federation { id: eid } => format!("fed:{}", eid.as_str()),
            ParameterScope::Cooperative { id: eid } => format!("coop:{}", eid.as_str()),
        }
    }

    // Serialization using JSON for flexibility with tagged enums
    fn serialize<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(value).map_err(|e| anyhow::anyhow!("Serialization failed: {e}"))
    }

    fn deserialize_param(bytes: &[u8]) -> Result<ProtocolParameter> {
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("Deserialization failed: {e}"))
    }

    fn deserialize_change(bytes: &[u8]) -> Result<ParameterChange> {
        serde_json::from_slice(bytes).map_err(|e| anyhow::anyhow!("Deserialization failed: {e}"))
    }

    // Pending change key generation
    // Key schema:
    // - `pending:{id}` - main storage
    // - `pending_idx:{effective_at:020}:{id}` - time-sorted index for scheduler

    fn pending_key(id: &str) -> Vec<u8> {
        format!("pending:{id}").into_bytes()
    }

    fn pending_prefix() -> Vec<u8> {
        b"pending:".to_vec()
    }

    fn pending_time_index_key(effective_at: u64, id: &str) -> Vec<u8> {
        // Use zero-padded timestamp for lexicographic ordering
        format!("pending_idx:{effective_at:020}:{id}").into_bytes()
    }

    fn pending_time_index_prefix() -> Vec<u8> {
        b"pending_idx:".to_vec()
    }

    fn deserialize_pending_change(bytes: &[u8]) -> Result<PendingParameterChange> {
        serde_json::from_slice(bytes)
            .map_err(|e| anyhow::anyhow!("Deserialization of pending change failed: {e}"))
    }
}

impl ProtocolParameterStore for SledParameterStore {
    fn get(&self, id: &str) -> Result<Option<ProtocolParameter>> {
        let key = Self::param_key(id);
        match self.db.get(&key)? {
            Some(bytes) => Ok(Some(Self::deserialize_param(&bytes)?)),
            None => Ok(None),
        }
    }

    fn get_effective(
        &self,
        id: &str,
        coop_id: Option<&EntityId>,
        fed_id: Option<&EntityId>,
    ) -> Result<Option<ProtocolParameter>> {
        // Try cooperative scope first
        if let Some(coop) = coop_id {
            let scope = ParameterScope::Cooperative { id: coop.clone() };
            let key = Self::scoped_param_key(&scope, id);
            if let Some(bytes) = self.db.get(&key)? {
                return Ok(Some(Self::deserialize_param(&bytes)?));
            }
        }

        // Try federation scope
        if let Some(fed) = fed_id {
            let scope = ParameterScope::Federation { id: fed.clone() };
            let key = Self::scoped_param_key(&scope, id);
            if let Some(bytes) = self.db.get(&key)? {
                return Ok(Some(Self::deserialize_param(&bytes)?));
            }
        }

        // Fall back to global scope
        self.get(id)
    }

    /// Set a protocol parameter value
    ///
    /// # Security Warning
    ///
    /// This is a privileged operation that directly modifies protocol parameters.
    /// In production, this method should ONLY be called from:
    /// - `handle_protocol_change()` after a governance proposal has been approved
    /// - Initial parameter loading during daemon startup
    ///
    /// Callers are responsible for ensuring proper authorization before invoking this method.
    fn set(
        &self,
        param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> Result<()> {
        use crate::protocol_validation::{validate_parameter_value, validate_scope_override};
        use sled::transaction::ConflictableTransactionError;

        let id = param.id.clone();
        let provided_version = param.version;

        // Warn about unknown categories (non-blocking)
        warn_unknown_category(&id);

        // Pre-validation for efficiency (additional verification happens inside transaction)
        // Use shared validation helpers for constraint bypass prevention and scope override checks.
        // NOTE: Version check is NOT done here - it must happen inside the transaction for atomicity.
        //
        // - Constraints (including allow_override) are immutable once a parameter is created
        // - The transaction verifies version hasn't changed, which implicitly verifies constraints
        // - For scoped parameters, we explicitly verify allow_override inside the transaction
        let global_param = self.get(&id)?;
        validate_parameter_value(&param.value, &param, global_param.as_ref())
            .map_err(|e| anyhow::anyhow!("Parameter validation failed for '{id}': {e}"))?;
        validate_scope_override(&id, &param.scope, global_param.as_ref())
            .map_err(|e| anyhow::anyhow!("Parameter '{id}' scope validation failed: {e}"))?;

        // Prepare the key for this parameter
        let is_scoped = !matches!(param.scope, ParameterScope::Global);
        let param_key = if is_scoped {
            Self::scoped_param_key(&param.scope, &id)
        } else {
            Self::param_key(&id)
        };

        // Generate a unique nonce for history key (must be done before transaction)
        let nonce = self.db.generate_id()?;
        let now = icn_time::current_timestamp_secs();

        // Clone values needed inside the transaction closure
        let id_clone = id.clone();
        let new_value = param.value.clone();
        let new_constraints = param.constraints.clone();
        let new_scope = param.scope.clone();
        let new_name = param.name.clone();
        let new_description = param.description.clone();
        let new_updated_by = param.updated_by.clone();

        // For scoped parameters, prepare reverse index update
        let (index_key, updated_index) = if is_scoped {
            let scope_str = Self::scope_str(&param.scope);
            let index_key = Self::param_index_key(&id);

            // Read current reverse index
            let mut scoped_keys: Vec<String> = self
                .db
                .get(&index_key)?
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();

            // Add new scope key if not already present
            if !scoped_keys.contains(&scope_str) {
                scoped_keys.push(scope_str);
            }

            let updated_index = Self::serialize(&scoped_keys)?;
            (Some(index_key), Some(updated_index))
        } else {
            (None, None)
        };

        // Track whether we validated as an update (for detecting TOCTOU race conditions)
        // - expected_update=true: We validated against stored constraints
        // - expected_update=false: We validated against the new parameter's constraints
        // If reality differs inside the transaction, we must error to prevent constraint bypass
        let expected_update = global_param.is_some();

        // For scoped parameters, capture the global param's version for TOCTOU detection
        // We validated against global_param.constraints, so we need to verify it hasn't changed
        let global_version_at_validation = global_param.as_ref().map(|p| p.version);
        let global_key = Self::param_key(&id);
        let global_key_clone = global_key.clone();

        // Use transaction to atomically verify version, update parameter, and record history.
        //
        // NOTE: Validation is intentionally duplicated between pre-validation (using shared helpers)
        // and transaction validation (inline). This is NOT redundant - it's required for security:
        //
        // - Pre-validation (above): Catches most errors quickly, uses shared helpers for consistency
        // - Transaction validation (below): Detects TOCTOU races where state changed between
        //   pre-validation and transaction. CANNOT use shared helpers because Sled transactions
        //   don't allow calling &self methods, and validation must happen atomically.
        //
        // The shared helpers ensure pre-validation logic stays consistent with InMemoryParameterStore.
        // The transaction validation ensures atomic correctness for concurrent operations.
        self.db
            .transaction(|tx| {
                // Read stored parameter INSIDE the transaction for atomic state check
                let stored_bytes = tx.get(&param_key)?;

                // Detect TOCTOU race conditions:
                // 1. Expected update but parameter was deleted → error (stale validation)
                // 2. Expected create but parameter now exists → error (validated against wrong constraints)
                // 3. State matches expectation → proceed with version check
                let is_update_now = stored_bytes.is_some();

                // Track stored constraints for updates (constraints are immutable after creation)
                let (old_value, new_version, effective_constraints) = if let Some(bytes) =
                    &stored_bytes
                {
                    let stored = Self::deserialize_param(bytes).map_err(|e| {
                        ConflictableTransactionError::Abort(anyhow::anyhow!(
                            "Failed to deserialize stored parameter: {e}"
                        ))
                    })?;

                    // CRITICAL: Verify version INSIDE the transaction
                    // This prevents the lost update race condition
                    if provided_version != stored.version {
                        return Err(ConflictableTransactionError::Abort(anyhow::anyhow!(
                            "Concurrent modification detected for parameter '{}': \
                             expected version {}, found {}. Please retry.",
                            id_clone,
                            provided_version,
                            stored.version
                        )));
                    }

                    // CRITICAL: Use STORED constraints for updates (constraints are immutable)
                    // This prevents constraint bypass attacks where a malicious update
                    // tries to relax constraints (e.g., change max from 50 to 100)
                    (Some(stored.value), stored.version + 1, stored.constraints)
                } else if expected_update && !is_scoped {
                    // Race condition: We validated against a global parameter that no longer exists
                    // Our validation may have used stale constraints
                    return Err(ConflictableTransactionError::Abort(anyhow::anyhow!(
                        "Parameter '{id_clone}' was deleted. Please retry."
                    )));
                } else if !expected_update && is_update_now && !is_scoped {
                    // Race condition: We expected to create a new global parameter, but one was
                    // created by another process. Our validation used our own constraints, not
                    // the stored ones. Must retry to validate against correct constraints.
                    return Err(ConflictableTransactionError::Abort(anyhow::anyhow!(
                        "Parameter '{id_clone}' was created concurrently. Please retry."
                    )));
                } else {
                    // New parameter (global or scoped override) starts at version 0
                    // For new params, use the submitted constraints
                    (None, 0, new_constraints.clone())
                };

                // CRITICAL: For scoped parameters, verify the global parameter hasn't changed
                // since we validated against its constraints. This prevents constraint bypass
                // where: 1) read global with max=100, 2) admin changes to max=50, 3) submit 75
                //
                // Also explicitly verify allow_override is still true (defense-in-depth).
                // While allow_override is immutable after creation, explicit verification
                // protects against bugs or future changes that might allow constraint mutation.
                if is_scoped && global_version_at_validation.is_some() {
                    let current_global = tx.get(&global_key_clone)?;
                    let current_global_param = current_global
                        .as_ref()
                        .and_then(|bytes| Self::deserialize_param(bytes).ok());

                    let current_version = current_global_param.as_ref().map(|p| p.version);

                    if current_version != global_version_at_validation {
                        return Err(ConflictableTransactionError::Abort(anyhow::anyhow!(
                            "Global parameter '{}' was modified (version {} -> {:?}). \
                             Please retry to validate against current constraints.",
                            id_clone,
                            global_version_at_validation.unwrap_or(0),
                            current_version
                        )));
                    }

                    // Defense-in-depth: Verify allow_override is still true inside transaction
                    // This catches any edge cases where constraints might have been modified
                    if let Some(ref gp) = current_global_param {
                        if !gp.constraints.allow_override {
                            return Err(ConflictableTransactionError::Abort(anyhow::anyhow!(
                                "Parameter '{id_clone}' does not allow scope overrides. \
                                 Constraint may have changed. Please retry."
                            )));
                        }
                    }
                }

                // Build the parameter with the correct version (computed inside transaction)
                // Note: effective_constraints uses STORED constraints for updates to enforce immutability
                let final_param = ProtocolParameter {
                    id: id_clone.clone(),
                    name: new_name.clone(),
                    description: new_description.clone(),
                    value: new_value.clone(),
                    constraints: effective_constraints,
                    scope: new_scope.clone(),
                    updated_at: now,
                    updated_by: new_updated_by.clone(),
                    version: new_version,
                };

                let param_value = Self::serialize(&final_param).map_err(|e| {
                    ConflictableTransactionError::Abort(anyhow::anyhow!(
                        "Failed to serialize parameter: {e}"
                    ))
                })?;

                // Store the new parameter
                tx.insert(param_key.as_slice(), param_value.as_slice())?;

                // Update reverse index for scoped parameters
                if let (Some(ref ikey), Some(ref ivalue)) = (&index_key, &updated_index) {
                    tx.insert(ikey.as_slice(), ivalue.as_slice())?;
                }

                // Record history if we had an old value
                if let Some(old) = old_value {
                    let change = ParameterChange {
                        parameter_id: id_clone.clone(),
                        old_value: old,
                        new_value: new_value.clone(),
                        changed_at: now,
                        proposal_id: proposal_id.clone(),
                        changed_by: changed_by.clone(),
                    };
                    let history_key = Self::history_key(&id_clone, now, nonce);
                    let history_value = Self::serialize(&change).map_err(|e| {
                        ConflictableTransactionError::Abort(anyhow::anyhow!(
                            "Failed to serialize history: {e}"
                        ))
                    })?;
                    tx.insert(history_key.as_slice(), history_value.as_slice())?;
                }

                Ok(())
            })
            .map_err(|e| match e {
                sled::transaction::TransactionError::Abort(err) => err,
                sled::transaction::TransactionError::Storage(e) => anyhow::anyhow!(
                    "Storage error during parameter update: {e}. This may indicate database corruption or I/O failure."
                ),
            })?;

        // Auto-prune history to prevent unbounded growth (DoS mitigation)
        // This runs outside the main transaction for performance, but it's safe
        // because history pruning is idempotent and doesn't affect correctness
        let pruned = self.prune_history(&id, MAX_HISTORY_ENTRIES_PER_PARAM)?;
        if pruned > 0 {
            debug!(parameter_id = %id, pruned_entries = pruned, "Auto-pruned history entries");
        }

        // Check global history size and warn if threshold exceeded
        // This helps operators detect accumulation from many scoped overrides
        if let Ok(total) = self.total_history_count() {
            if total > GLOBAL_HISTORY_WARNING_THRESHOLD {
                warn!(
                    total_history_entries = total,
                    threshold = GLOBAL_HISTORY_WARNING_THRESHOLD,
                    "Global history size exceeds threshold. Consider reviewing parameter \
                     change frequency or running manual prune_history() on old parameters."
                );
            }
        }

        debug!(parameter_id = %id, "Parameter updated");
        Ok(())
    }

    fn list(&self) -> Result<Vec<ProtocolParameter>> {
        // Prefix b"param:" only matches global parameters (param:{id})
        // It does NOT match scoped parameters (param_scope:{scope}:{id}) or
        // reverse indexes (param_idx:{id}) because the 6th character differs
        let prefix = b"param:";
        let mut params = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            params.push(Self::deserialize_param(&value)?);
        }

        Ok(params)
    }

    fn list_by_category(&self, category: &str) -> Result<Vec<ProtocolParameter>> {
        let all = self.list()?;
        Ok(all
            .into_iter()
            .filter(|p| p.category() == category)
            .collect())
    }

    fn get_history(&self, id: &str) -> Result<Vec<ParameterChange>> {
        let prefix = Self::history_prefix(id);
        let mut changes = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) = item?;
            changes.push(Self::deserialize_change(&value)?);
        }

        // Sort by timestamp (oldest first)
        changes.sort_by_key(|c| c.changed_at);
        Ok(changes)
    }

    fn get_history_paginated(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<ParameterChange>, usize)> {
        let prefix = Self::history_prefix(id);
        let mut changes = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) = item?;
            changes.push(Self::deserialize_change(&value)?);
        }

        let total = changes.len();

        // Sort by timestamp (oldest first)
        changes.sort_by_key(|c| c.changed_at);

        // Apply pagination
        let paginated: Vec<_> = changes.into_iter().skip(offset).take(limit).collect();

        Ok((paginated, total))
    }

    fn prune_history(&self, id: &str, max_entries: usize) -> Result<usize> {
        // Prevent accidental deletion of all history
        if max_entries == 0 {
            return Err(anyhow::anyhow!(
                "max_entries must be >= 1 to prevent accidental data loss. \
                 Use delete() to remove a parameter entirely."
            ));
        }

        let prefix = Self::history_prefix(id);
        let mut entries: Vec<(sled::IVec, u64)> = Vec::new();

        // Collect all history entries with their timestamps
        for item in self.db.scan_prefix(&prefix) {
            let (key, value) = item?;
            let change: ParameterChange = Self::deserialize_change(&value)?;
            entries.push((key, change.changed_at));
        }

        if entries.len() <= max_entries {
            return Ok(0);
        }

        // Sort by timestamp descending (newest first)
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove entries beyond max_entries
        let mut removed = 0;
        for (key, _) in entries.into_iter().skip(max_entries) {
            self.db.remove(&key)?;
            removed += 1;
        }

        Ok(removed)
    }

    fn delete(&self, id: &str) -> Result<()> {
        // Collect all keys to delete before transaction
        let global_key = Self::param_key(id);
        let index_key = Self::param_index_key(id);

        // Use reverse index for O(1) lookup of scoped keys (instead of O(n) scan)
        let (scoped_keys, scope_count): (Vec<Vec<u8>>, usize) = self
            .db
            .get(&index_key)?
            .and_then(|bytes| {
                let scope_strs: Vec<String> = serde_json::from_slice(&bytes).ok()?;
                let count = scope_strs.len();
                let keys = scope_strs
                    .into_iter()
                    .map(|scope_str| format!("param_scope:{scope_str}:{id}").into_bytes())
                    .collect();
                Some((keys, count))
            })
            .unwrap_or_default();

        // Warn if deleting a parameter with active scoped overrides
        // This helps operators avoid accidentally removing cooperative customizations
        if scope_count > 0 {
            warn!(
                parameter_id = %id,
                scoped_overrides = scope_count,
                "Deleting parameter with active scoped overrides. \
                 This will remove customizations for {} cooperative(s)/federation(s).",
                scope_count
            );
        }

        // Collect history keys (still O(h) where h is history count, but history is per-parameter)
        let history_prefix = Self::history_prefix(id);
        let history_keys: Vec<Vec<u8>> = self
            .db
            .scan_prefix(&history_prefix)
            .filter_map(|item| item.ok().map(|(key, _)| key.to_vec()))
            .collect();

        let id_str = id.to_string();

        // Use transaction for atomicity
        self.db
            .transaction(|tx| {
                // Delete global parameter
                tx.remove(global_key.as_slice())?;

                // Delete scoped parameters (using reverse index for direct access)
                for key in &scoped_keys {
                    tx.remove(key.as_slice())?;
                }

                // Delete reverse index
                tx.remove(index_key.as_slice())?;

                // Delete history
                for key in &history_keys {
                    tx.remove(key.as_slice())?;
                }

                Ok(())
            })
            .map_err(|e| match e {
                sled::transaction::TransactionError::Abort(err) => err,
                sled::transaction::TransactionError::Storage(e) => anyhow::anyhow!(
                    "Storage error during parameter deletion: {e}. This may indicate database corruption or I/O failure."
                ),
            })?;

        debug!(parameter_id = %id_str, "Parameter deleted");
        Ok(())
    }

    fn exists(&self, id: &str) -> Result<bool> {
        let key = Self::param_key(id);
        Ok(self.db.contains_key(&key)?)
    }

    fn count(&self) -> Result<usize> {
        // Prefix b"param:" only matches global parameters (see list() for details)
        let prefix = b"param:";
        let count = self.db.scan_prefix(prefix).filter(|r| r.is_ok()).count();
        Ok(count)
    }

    fn total_history_count(&self) -> Result<usize> {
        let prefix = b"history:";
        let mut count = 0;
        for item in self.db.scan_prefix(prefix) {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }

    fn validate(
        &self,
        id: &str,
        new_value: &ParameterValue,
    ) -> Result<(), ParameterValidationError> {
        let param = self
            .get(id)
            .map_err(|_| ParameterValidationError::NotFound(id.to_string()))?
            .ok_or_else(|| ParameterValidationError::NotFound(id.to_string()))?;

        param.validate(new_value)
    }

    fn list_scoped_parameters(&self) -> Result<Vec<ProtocolParameter>> {
        // Prefix b"param_scope:" matches all scoped parameters
        let prefix = b"param_scope:";
        let mut params = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            params.push(Self::deserialize_param(&value)?);
        }

        Ok(params)
    }

    fn delete_scoped_parameter(&self, id: &str, scope: &ParameterScope) -> Result<bool> {
        // Cannot use this method for global scope
        if matches!(scope, ParameterScope::Global) {
            anyhow::bail!(
                "Cannot delete global parameter via delete_scoped_parameter(). \
                 Use delete() to remove global parameters."
            );
        }

        let scoped_key = Self::scoped_param_key(scope, id);
        let index_key = Self::param_index_key(id);
        let scope_str = Self::scope_str(scope);

        // Use transaction for atomicity - existence check is inside transaction
        // to avoid TOCTOU race conditions
        let existed = self
            .db
            .transaction(|tx| {
                // Check if the parameter exists inside the transaction
                if tx.get(&scoped_key)?.is_none() {
                    return Ok(false);
                }

                // Remove the scoped parameter
                tx.remove(scoped_key.as_slice())?;

                // Update reverse index
                if let Some(index_bytes) = tx.get(&index_key)? {
                    let mut scope_strs: Vec<String> = serde_json::from_slice(&index_bytes)
                        .unwrap_or_else(|e| {
                            // Log warning for corrupted index but continue with empty list
                            // This allows cleanup to proceed even with corrupted state
                            warn!(
                                error = %e,
                                "Failed to deserialize scope index, treating as empty"
                            );
                            Vec::new()
                        });
                    scope_strs.retain(|s| s != &scope_str);

                    if scope_strs.is_empty() {
                        // Remove index entirely if no scopes remain
                        tx.remove(index_key.as_slice())?;
                    } else {
                        // Update with remaining scopes
                        let updated = serde_json::to_vec(&scope_strs).map_err(|e| {
                            sled::transaction::ConflictableTransactionError::Abort(anyhow::anyhow!(
                                "Failed to serialize index: {e}"
                            ))
                        })?;
                        tx.insert(index_key.as_slice(), updated.as_slice())?;
                    }
                }

                Ok(true)
            })
            .map_err(|e| match e {
                sled::transaction::TransactionError::Abort(err) => err,
                sled::transaction::TransactionError::Storage(e) => {
                    anyhow::anyhow!("Storage error during scoped parameter deletion: {e}")
                }
            })?;

        if existed {
            debug!(
                parameter_id = %id,
                scope = ?scope,
                "Deleted scoped parameter"
            );
        }

        Ok(existed)
    }

    // ========================================
    // Pending Change Methods (Delayed Execution)
    // ========================================

    fn add_pending_change(&self, change: PendingParameterChange) -> Result<()> {
        let pending_key = Self::pending_key(&change.id);
        let index_key = Self::pending_time_index_key(change.effective_at, &change.id);

        // Check if already exists
        if self.db.contains_key(&pending_key)? {
            return Err(anyhow::anyhow!(
                "Pending change with ID '{}' already exists",
                change.id
            ));
        }

        let pending_value = Self::serialize(&change)?;

        debug!(
            pending_change_id = %change.id,
            parameter_id = %change.parameter_id,
            effective_at = change.effective_at,
            "Adding pending parameter change"
        );

        // Use transaction to atomically insert both keys
        self.db
            .transaction(|tx| {
                tx.insert(pending_key.as_slice(), pending_value.as_slice())?;
                // Store only the ID in the index (the full data is in pending_key)
                tx.insert(index_key.as_slice(), change.id.as_bytes())?;
                Ok(())
            })
            .map_err(|e| match e {
                sled::transaction::TransactionError::Abort(err) => err,
                sled::transaction::TransactionError::Storage(e) => {
                    anyhow::anyhow!("Storage error adding pending change: {e}")
                }
            })?;

        Ok(())
    }

    fn get_pending_change(&self, id: &PendingChangeId) -> Result<Option<PendingParameterChange>> {
        let key = Self::pending_key(id);
        match self.db.get(&key)? {
            Some(bytes) => Ok(Some(Self::deserialize_pending_change(&bytes)?)),
            None => Ok(None),
        }
    }

    fn list_pending_changes(&self) -> Result<Vec<PendingParameterChange>> {
        let prefix = Self::pending_prefix();
        let mut changes = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) = item?;
            changes.push(Self::deserialize_pending_change(&value)?);
        }

        // Sort by effective_at for consistent ordering
        changes.sort_by_key(|c| c.effective_at);
        Ok(changes)
    }

    fn list_pending_changes_for_parameter(
        &self,
        parameter_id: &str,
    ) -> Result<Vec<PendingParameterChange>> {
        let prefix = Self::pending_prefix();
        let mut changes = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) = item?;
            let change = Self::deserialize_pending_change(&value)?;
            if change.parameter_id == parameter_id {
                changes.push(change);
            }
        }

        changes.sort_by_key(|c| c.effective_at);
        Ok(changes)
    }

    fn get_changes_due_before(&self, timestamp: u64) -> Result<Vec<PendingParameterChange>> {
        // Use the time-sorted index for efficient lookup
        // Keys are formatted as pending_idx:{effective_at:020}:{id}
        // Lexicographic scan stops at the timestamp boundary
        let prefix = Self::pending_time_index_prefix();
        let max_key = format!("pending_idx:{:020}:", timestamp + 1).into_bytes();

        let mut due = Vec::new();

        for item in self.db.scan_prefix(&prefix) {
            let (key, value) = item?;

            // Stop scanning once we're past the timestamp
            if key.as_ref() >= max_key.as_slice() {
                break;
            }

            // The index stores the pending change ID
            let id = String::from_utf8_lossy(&value).to_string();

            // Fetch the actual change
            if let Some(change) = self.get_pending_change(&id)? {
                // Only include pending changes
                if change.status == PendingChangeStatus::Pending {
                    due.push(change);
                }
            }
        }

        // Re-sort with tie-breakers for deterministic ordering when effective_at matches.
        // The index provides effective_at ordering, but for same-timestamp conflicts we need
        // additional tie-breakers: created_at (earlier wins), then id (lexicographic).
        due.sort_by(|a, b| {
            a.effective_at
                .cmp(&b.effective_at)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(due)
    }

    fn update_pending_change(&self, change: PendingParameterChange) -> Result<()> {
        let pending_key = Self::pending_key(&change.id);

        // Check if exists
        if !self.db.contains_key(&pending_key)? {
            return Err(anyhow::anyhow!(
                "Pending change with ID '{}' not found",
                change.id
            ));
        }

        debug!(
            pending_change_id = %change.id,
            status = %change.status,
            "Updating pending parameter change"
        );

        let pending_value = Self::serialize(&change)?;
        self.db.insert(&pending_key, pending_value)?;

        Ok(())
    }

    fn cancel_pending_change(&self, id: &PendingChangeId, reason: &str) -> Result<()> {
        let pending_key = Self::pending_key(id);

        let bytes = self
            .db
            .get(&pending_key)?
            .ok_or_else(|| anyhow::anyhow!("Pending change with ID '{id}' not found"))?;

        let mut change = Self::deserialize_pending_change(&bytes)?;

        if change.status != PendingChangeStatus::Pending {
            return Err(anyhow::anyhow!(
                "Cannot cancel pending change '{}' with status '{}'",
                id,
                change.status
            ));
        }

        debug!(
            pending_change_id = %id,
            reason = %reason,
            "Cancelling pending parameter change"
        );

        change.mark_cancelled(reason);
        let pending_value = Self::serialize(&change)?;
        self.db.insert(&pending_key, pending_value)?;

        Ok(())
    }

    fn count_pending_changes(&self) -> Result<usize> {
        let prefix = Self::pending_prefix();
        let mut count = 0;

        for item in self.db.scan_prefix(&prefix) {
            let (_, value) = item?;
            let change = Self::deserialize_pending_change(&value)?;
            if change.status == PendingChangeStatus::Pending {
                count += 1;
            }
        }

        Ok(count)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn test_param(id: &str, value: i64) -> ProtocolParameter {
        ProtocolParameter::new(
            id,
            "Test Parameter",
            "A test parameter",
            ParameterValue::Integer(value),
        )
    }

    fn test_param_with_override(id: &str, value: i64, allow_override: bool) -> ProtocolParameter {
        let mut param = ProtocolParameter::new(
            id,
            "Test Parameter",
            "A test parameter",
            ParameterValue::Integer(value),
        );
        param.constraints.allow_override = allow_override;
        param
    }

    // ========================================
    // InMemoryParameterStore tests
    // ========================================

    #[test]
    fn test_inmemory_set_and_get() {
        let store = InMemoryParameterStore::new();

        let param = test_param("test.value", 42);
        store.set(param.clone(), None, None).unwrap();

        let retrieved = store.get("test.value").unwrap().unwrap();
        assert_eq!(retrieved.id, "test.value");
        assert_eq!(retrieved.value, ParameterValue::Integer(42));
    }

    #[test]
    fn test_inmemory_list() {
        let store = InMemoryParameterStore::new();

        store.set(test_param("test.one", 1), None, None).unwrap();
        store.set(test_param("test.two", 2), None, None).unwrap();
        store.set(test_param("other.three", 3), None, None).unwrap();

        let all = store.list().unwrap();
        assert_eq!(all.len(), 3);

        let test_category = store.list_by_category("test").unwrap();
        assert_eq!(test_category.len(), 2);
    }

    #[test]
    fn test_inmemory_history() {
        let store = InMemoryParameterStore::new();

        // Initial set (no history since no previous value)
        store.set(test_param("test.hist", 1), None, None).unwrap();
        assert!(store.get_history("test.hist").unwrap().is_empty());

        // Update creates history
        store
            .set(
                test_param("test.hist", 2),
                Some("proposal-1".to_string()),
                None,
            )
            .unwrap();

        let history = store.get_history("test.hist").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].old_value, ParameterValue::Integer(1));
        assert_eq!(history[0].new_value, ParameterValue::Integer(2));
        assert_eq!(history[0].proposal_id, Some("proposal-1".to_string()));
    }

    #[test]
    fn test_inmemory_scope_resolution() {
        let store = InMemoryParameterStore::new();

        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Set global default with allow_override = true to allow scoped overrides
        store
            .set(
                test_param_with_override("test.scoped", 10, true),
                None,
                None,
            )
            .unwrap();

        // Set federation override
        let fed_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(20),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        // Set cooperative override
        let coop_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(30),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        // Global resolution
        let global = store
            .get_effective("test.scoped", None, None)
            .unwrap()
            .unwrap();
        assert_eq!(global.value, ParameterValue::Integer(10));

        // Federation resolution
        let fed = store
            .get_effective("test.scoped", None, Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(fed.value, ParameterValue::Integer(20));

        // Cooperative resolution (most specific)
        let coop = store
            .get_effective("test.scoped", Some(&coop_id), Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(coop.value, ParameterValue::Integer(30));
    }

    // ========================================
    // SledParameterStore tests
    // ========================================

    #[test]
    fn test_sled_set_and_get() {
        let store = SledParameterStore::temporary().unwrap();

        let param = test_param("test.value", 42);
        store.set(param.clone(), None, None).unwrap();

        let retrieved = store.get("test.value").unwrap().unwrap();
        assert_eq!(retrieved.id, "test.value");
        assert_eq!(retrieved.value, ParameterValue::Integer(42));
    }

    #[test]
    fn test_sled_list() {
        let store = SledParameterStore::temporary().unwrap();

        store.set(test_param("test.one", 1), None, None).unwrap();
        store.set(test_param("test.two", 2), None, None).unwrap();
        store.set(test_param("other.three", 3), None, None).unwrap();

        let all = store.list().unwrap();
        assert_eq!(all.len(), 3);

        let test_category = store.list_by_category("test").unwrap();
        assert_eq!(test_category.len(), 2);
    }

    #[test]
    fn test_sled_history() {
        let store = SledParameterStore::temporary().unwrap();

        // Initial set (no history)
        store.set(test_param("test.hist", 1), None, None).unwrap();
        assert!(store.get_history("test.hist").unwrap().is_empty());

        // Update creates history
        store
            .set(
                test_param("test.hist", 2),
                Some("proposal-1".to_string()),
                Some("did:icn:admin".to_string()),
            )
            .unwrap();

        let history = store.get_history("test.hist").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].old_value, ParameterValue::Integer(1));
        assert_eq!(history[0].new_value, ParameterValue::Integer(2));
        assert_eq!(history[0].proposal_id, Some("proposal-1".to_string()));
        assert_eq!(history[0].changed_by, Some("did:icn:admin".to_string()));
    }

    #[test]
    fn test_sled_scope_resolution() {
        let store = SledParameterStore::temporary().unwrap();

        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Set global default with allow_override = true to allow scoped overrides
        store
            .set(
                test_param_with_override("test.scoped", 10, true),
                None,
                None,
            )
            .unwrap();

        // Set federation override
        let fed_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(20),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        // Set cooperative override
        let coop_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(30),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        // Global resolution
        let global = store
            .get_effective("test.scoped", None, None)
            .unwrap()
            .unwrap();
        assert_eq!(global.value, ParameterValue::Integer(10));

        // Federation resolution
        let fed = store
            .get_effective("test.scoped", None, Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(fed.value, ParameterValue::Integer(20));

        // Cooperative resolution (most specific)
        let coop = store
            .get_effective("test.scoped", Some(&coop_id), Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(coop.value, ParameterValue::Integer(30));
    }

    #[test]
    fn test_sled_delete() {
        let store = SledParameterStore::temporary().unwrap();

        store.set(test_param("test.delete", 1), None, None).unwrap();
        assert!(store.exists("test.delete").unwrap());
        assert_eq!(store.count().unwrap(), 1);

        store.delete("test.delete").unwrap();
        assert!(!store.exists("test.delete").unwrap());
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_sled_validation() {
        let store = SledParameterStore::temporary().unwrap();

        let param = ProtocolParameter::new(
            "test.bounded",
            "Bounded",
            "Bounded param",
            ParameterValue::Integer(50),
        )
        .with_min(ParameterValue::Integer(10))
        .with_max(ParameterValue::Integer(100));

        store.set(param, None, None).unwrap();

        // Valid value
        assert!(store
            .validate("test.bounded", &ParameterValue::Integer(50))
            .is_ok());

        // Invalid - below minimum
        assert!(store
            .validate("test.bounded", &ParameterValue::Integer(5))
            .is_err());

        // Invalid - above maximum
        assert!(store
            .validate("test.bounded", &ParameterValue::Integer(150))
            .is_err());
    }

    #[test]
    fn test_sled_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("param_test");

        // First session: create parameter
        {
            let db = sled::open(&db_path).unwrap();
            let store = SledParameterStore::new(Arc::new(db)).unwrap();
            store
                .set(test_param("persist.test", 999), None, None)
                .unwrap();
            assert_eq!(store.count().unwrap(), 1);
        }

        // Second session: verify persistence
        {
            let db = sled::open(&db_path).unwrap();
            let store = SledParameterStore::new(Arc::new(db)).unwrap();
            assert_eq!(store.count().unwrap(), 1);
            let param = store.get("persist.test").unwrap().unwrap();
            assert_eq!(param.value, ParameterValue::Integer(999));
        }
    }

    #[test]
    fn test_scope_override_validation_inmemory() {
        let store = InMemoryParameterStore::new();

        // Create a parameter that does NOT allow overrides
        let mut param = test_param("test.no_override", 100);
        param.constraints.allow_override = false;
        store.set(param, None, None).unwrap();

        // Try to set a scoped override - should fail
        let fed_id = EntityId::federation("test-fed").unwrap();
        let scoped_param = ProtocolParameter::new(
            "test.no_override",
            "No Override",
            "Cannot be overridden",
            ParameterValue::Integer(200),
        )
        .with_scope(ParameterScope::Federation { id: fed_id });

        let result = store.set(scoped_param, None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not allow scope overrides"));
    }

    #[test]
    fn test_scope_override_validation_sled() {
        let store = SledParameterStore::temporary().unwrap();

        // Create a parameter that does NOT allow overrides
        let mut param = test_param("test.no_override", 100);
        param.constraints.allow_override = false;
        store.set(param, None, None).unwrap();

        // Try to set a scoped override - should fail
        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let scoped_param = ProtocolParameter::new(
            "test.no_override",
            "No Override",
            "Cannot be overridden",
            ParameterValue::Integer(200),
        )
        .with_scope(ParameterScope::Cooperative { id: coop_id });

        let result = store.set(scoped_param, None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not allow scope overrides"));
    }

    #[test]
    fn test_prune_history_inmemory() {
        let store = InMemoryParameterStore::new();

        // Create a parameter and update it several times
        store.set(test_param("test.prune", 1), None, None).unwrap();
        for i in 2..=10 {
            // Read current version before updating (optimistic locking pattern)
            let current = store.get("test.prune").unwrap().unwrap();
            let mut param = ProtocolParameter::new(
                "test.prune",
                "Prune Test",
                "Test pruning",
                ParameterValue::Integer(i),
            );
            param.version = current.version; // Use current version for optimistic locking
            store
                .set(param, Some(format!("proposal-{i}")), None)
                .unwrap();
        }

        // Should have 9 history entries (first set doesn't create history)
        let history = store.get_history("test.prune").unwrap();
        assert_eq!(history.len(), 9);

        // Prune to keep only last 3
        let removed = store.prune_history("test.prune", 3).unwrap();
        assert_eq!(removed, 6);

        // Verify only 3 remain
        let history = store.get_history("test.prune").unwrap();
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_prune_history_sled() {
        let store = SledParameterStore::temporary().unwrap();

        // Create a parameter and update it several times
        store.set(test_param("test.prune", 1), None, None).unwrap();
        for i in 2..=10 {
            // Read current version before updating (optimistic locking pattern)
            let current = store.get("test.prune").unwrap().unwrap();
            let mut param = ProtocolParameter::new(
                "test.prune",
                "Prune Test",
                "Test pruning",
                ParameterValue::Integer(i),
            );
            param.version = current.version; // Use current version for optimistic locking
            store
                .set(param, Some(format!("proposal-{i}")), None)
                .unwrap();
        }

        // Should have 9 history entries
        let history = store.get_history("test.prune").unwrap();
        assert_eq!(history.len(), 9);

        // Prune to keep only last 5
        let removed = store.prune_history("test.prune", 5).unwrap();
        assert_eq!(removed, 4);

        // Verify only 5 remain
        let history = store.get_history("test.prune").unwrap();
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_prune_history_zero_entries_rejected_inmemory() {
        let store = InMemoryParameterStore::new();
        store.set(test_param("test.prune", 1), None, None).unwrap();

        // Trying to prune to 0 entries should fail to prevent accidental data loss
        let result = store.prune_history("test.prune", 0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_entries must be >= 1"));
    }

    #[test]
    fn test_prune_history_zero_entries_rejected_sled() {
        let store = SledParameterStore::temporary().unwrap();
        store.set(test_param("test.prune", 1), None, None).unwrap();

        // Trying to prune to 0 entries should fail to prevent accidental data loss
        let result = store.prune_history("test.prune", 0);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("max_entries must be >= 1"));
    }

    // ========================================
    // Paginated history tests
    // ========================================

    #[test]
    fn test_paginated_history_inmemory() {
        let store = InMemoryParameterStore::new();

        // Create parameter with multiple history entries
        store.set(test_param("test.page", 1), None, None).unwrap();
        for i in 2..=10 {
            let current = store.get("test.page").unwrap().unwrap();
            let mut param = test_param("test.page", i);
            param.version = current.version;
            store.set(param, None, None).unwrap();
        }

        // Should have 9 history entries total
        let (all, total) = store.get_history_paginated("test.page", 0, 100).unwrap();
        assert_eq!(total, 9);
        assert_eq!(all.len(), 9);

        // Test first page (3 items)
        let (page1, total) = store.get_history_paginated("test.page", 0, 3).unwrap();
        assert_eq!(total, 9);
        assert_eq!(page1.len(), 3);
        // Oldest entries first (changes from 1->2, 2->3, 3->4)
        assert_eq!(page1[0].old_value, ParameterValue::Integer(1));
        assert_eq!(page1[0].new_value, ParameterValue::Integer(2));

        // Test second page (3 items)
        let (page2, total) = store.get_history_paginated("test.page", 3, 3).unwrap();
        assert_eq!(total, 9);
        assert_eq!(page2.len(), 3);
        // Changes from 4->5, 5->6, 6->7
        assert_eq!(page2[0].old_value, ParameterValue::Integer(4));

        // Test last page (3 items)
        let (page3, total) = store.get_history_paginated("test.page", 6, 3).unwrap();
        assert_eq!(total, 9);
        assert_eq!(page3.len(), 3);
        // Changes from 7->8, 8->9, 9->10
        assert_eq!(page3[2].new_value, ParameterValue::Integer(10));

        // Test beyond end
        let (empty, total) = store.get_history_paginated("test.page", 100, 10).unwrap();
        assert_eq!(total, 9);
        assert!(empty.is_empty());

        // Test nonexistent parameter
        let (none, total) = store
            .get_history_paginated("nonexistent.param", 0, 10)
            .unwrap();
        assert_eq!(total, 0);
        assert!(none.is_empty());
    }

    #[test]
    fn test_paginated_history_sled() {
        let store = SledParameterStore::temporary().unwrap();

        // Create parameter with multiple history entries
        store.set(test_param("test.page", 1), None, None).unwrap();
        for i in 2..=10 {
            let current = store.get("test.page").unwrap().unwrap();
            let mut param = test_param("test.page", i);
            param.version = current.version;
            store.set(param, None, None).unwrap();
        }

        // Should have 9 history entries total
        let (all, total) = store.get_history_paginated("test.page", 0, 100).unwrap();
        assert_eq!(total, 9);
        assert_eq!(all.len(), 9);

        // Test pagination
        let (page1, total) = store.get_history_paginated("test.page", 0, 3).unwrap();
        assert_eq!(total, 9);
        assert_eq!(page1.len(), 3);

        let (page2, _) = store.get_history_paginated("test.page", 3, 3).unwrap();
        assert_eq!(page2.len(), 3);

        // Test limit larger than remaining
        let (partial, total) = store.get_history_paginated("test.page", 7, 10).unwrap();
        assert_eq!(total, 9);
        assert_eq!(partial.len(), 2); // Only 2 entries remain after offset 7
    }

    #[test]
    fn test_sled_delete_with_scoped_overrides() {
        let store = SledParameterStore::temporary().unwrap();

        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Set global parameter with allow_override = true
        store
            .set(
                test_param_with_override("test.scoped_delete", 10, true),
                None,
                None,
            )
            .unwrap();

        // Set federation override
        let fed_param = ProtocolParameter::new(
            "test.scoped_delete",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(20),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        // Set cooperative override
        let coop_param = ProtocolParameter::new(
            "test.scoped_delete",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(30),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        // Verify all scopes are set
        assert!(store.exists("test.scoped_delete").unwrap());
        let global = store
            .get_effective("test.scoped_delete", None, None)
            .unwrap()
            .unwrap();
        assert_eq!(global.value, ParameterValue::Integer(10));
        let fed = store
            .get_effective("test.scoped_delete", None, Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(fed.value, ParameterValue::Integer(20));
        let coop = store
            .get_effective("test.scoped_delete", Some(&coop_id), Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(coop.value, ParameterValue::Integer(30));

        // Delete the parameter (should delete global + all scoped via reverse index)
        store.delete("test.scoped_delete").unwrap();

        // Verify all are deleted
        assert!(!store.exists("test.scoped_delete").unwrap());
        assert!(store
            .get_effective("test.scoped_delete", None, None)
            .unwrap()
            .is_none());
        assert!(store
            .get_effective("test.scoped_delete", None, Some(&fed_id))
            .unwrap()
            .is_none());
        assert!(store
            .get_effective("test.scoped_delete", Some(&coop_id), Some(&fed_id))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_inmemory_delete_with_scoped_overrides() {
        let store = InMemoryParameterStore::new();

        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Set global parameter with allow_override = true
        store
            .set(
                test_param_with_override("test.scoped_delete", 10, true),
                None,
                None,
            )
            .unwrap();

        // Set federation and cooperative overrides
        let fed_param = ProtocolParameter::new(
            "test.scoped_delete",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(20),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        let coop_param = ProtocolParameter::new(
            "test.scoped_delete",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(30),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        // Delete using reverse index
        store.delete("test.scoped_delete").unwrap();

        // Verify all are deleted
        assert!(!store.exists("test.scoped_delete").unwrap());
        assert!(store
            .get_effective("test.scoped_delete", None, None)
            .unwrap()
            .is_none());
        assert!(store
            .get_effective("test.scoped_delete", None, Some(&fed_id))
            .unwrap()
            .is_none());
        assert!(store
            .get_effective("test.scoped_delete", Some(&coop_id), Some(&fed_id))
            .unwrap()
            .is_none());
    }

    // ========================================
    // Security tests: Constraint bypass prevention
    // ========================================

    #[test]
    fn test_inmemory_constraint_bypass_prevention() {
        // This test verifies that attackers cannot bypass constraints by
        // submitting a parameter with modified constraints
        let store = InMemoryParameterStore::new();

        // Create a parameter with strict constraints (max = 100)
        let mut param = test_param("test.constrained", 50);
        param.constraints.min = Some(ParameterValue::Integer(0));
        param.constraints.max = Some(ParameterValue::Integer(100));
        store.set(param, None, None).unwrap();

        // Now try to bypass by submitting a parameter with modified constraints
        // (attacker sets max = 1000 to allow value 500)
        let mut attack_param = test_param("test.constrained", 500);
        attack_param.constraints.min = Some(ParameterValue::Integer(0));
        attack_param.constraints.max = Some(ParameterValue::Integer(1000)); // Attacker's modified constraint

        // This should FAIL because we validate against the STORED constraints, not the new ones
        let result = store.set(attack_param, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("above maximum"));

        // Verify the value is still the original
        let stored = store.get("test.constrained").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(50));
    }

    #[test]
    fn test_sled_constraint_bypass_prevention() {
        // Same test for SledParameterStore
        let store = SledParameterStore::temporary().unwrap();

        // Create a parameter with strict constraints (max = 100)
        let mut param = test_param("test.constrained", 50);
        param.constraints.min = Some(ParameterValue::Integer(0));
        param.constraints.max = Some(ParameterValue::Integer(100));
        store.set(param, None, None).unwrap();

        // Try constraint bypass attack
        let mut attack_param = test_param("test.constrained", 500);
        attack_param.constraints.min = Some(ParameterValue::Integer(0));
        attack_param.constraints.max = Some(ParameterValue::Integer(1000));

        // Should fail
        let result = store.set(attack_param, None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("above maximum"));

        // Value unchanged
        let stored = store.get("test.constrained").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(50));
    }

    #[test]
    fn test_new_param_uses_own_constraints() {
        // Verify that new parameters (not updates) can use their own constraints
        let store = InMemoryParameterStore::new();

        // Create a NEW parameter with specific constraints
        let mut param = test_param("test.new_param", 50);
        param.constraints.min = Some(ParameterValue::Integer(0));
        param.constraints.max = Some(ParameterValue::Integer(100));

        // This should succeed because it's a new parameter
        store.set(param, None, None).unwrap();

        let stored = store.get("test.new_param").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(50));
    }

    // ========================================
    // Optimistic locking tests
    // ========================================

    #[test]
    fn test_optimistic_locking_inmemory() {
        let store = InMemoryParameterStore::new();

        // Create initial parameter (version 0)
        store.set(test_param("test.lock", 100), None, None).unwrap();
        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.version, 0);

        // Update with correct version (should succeed, increment to version 1)
        let mut update1 = test_param("test.lock", 200);
        update1.version = 0; // Match current version
        store.set(update1, None, None).unwrap();

        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(200));
        assert_eq!(stored.version, 1);

        // Update with stale version (should fail)
        let mut stale_update = test_param("test.lock", 300);
        stale_update.version = 0; // Stale version
        let result = store.set(stale_update, None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Concurrent modification detected"));

        // Value should be unchanged
        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(200));
        assert_eq!(stored.version, 1);

        // Update with correct version (should succeed)
        let mut update2 = test_param("test.lock", 300);
        update2.version = 1; // Correct version
        store.set(update2, None, None).unwrap();

        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(300));
        assert_eq!(stored.version, 2);
    }

    #[test]
    fn test_optimistic_locking_sled() {
        let store = SledParameterStore::temporary().unwrap();

        // Create initial parameter (version 0)
        store.set(test_param("test.lock", 100), None, None).unwrap();
        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.version, 0);

        // Update with correct version (should succeed)
        let mut update1 = test_param("test.lock", 200);
        update1.version = 0;
        store.set(update1, None, None).unwrap();

        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(200));
        assert_eq!(stored.version, 1);

        // Update with stale version (should fail)
        let mut stale_update = test_param("test.lock", 300);
        stale_update.version = 0; // Stale version
        let result = store.set(stale_update, None, None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Concurrent modification detected"));

        // Value should be unchanged
        let stored = store.get("test.lock").unwrap().unwrap();
        assert_eq!(stored.value, ParameterValue::Integer(200));
        assert_eq!(stored.version, 1);
    }

    #[test]
    fn test_version_starts_at_zero_for_new_params() {
        let store = InMemoryParameterStore::new();

        // New parameter should start at version 0
        store.set(test_param("test.new", 1), None, None).unwrap();
        let stored = store.get("test.new").unwrap().unwrap();
        assert_eq!(stored.version, 0);
    }

    // ========================================
    // Reverse index cleanup tests
    // ========================================

    #[test]
    fn test_sled_reverse_index_cleanup_on_delete() {
        let store = SledParameterStore::temporary().unwrap();

        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Set global parameter with scoped overrides
        store
            .set(
                test_param_with_override("test.index_cleanup", 10, true),
                None,
                None,
            )
            .unwrap();

        let fed_param = ProtocolParameter::new(
            "test.index_cleanup",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(20),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        let coop_param = ProtocolParameter::new(
            "test.index_cleanup",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(30),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        // Verify reverse index exists before delete
        let index_key = SledParameterStore::param_index_key("test.index_cleanup");
        assert!(
            store.db.contains_key(&index_key).unwrap(),
            "Reverse index should exist before delete"
        );

        // Delete the parameter
        store.delete("test.index_cleanup").unwrap();

        // Verify reverse index is cleaned up
        assert!(
            !store.db.contains_key(&index_key).unwrap(),
            "Reverse index should be cleaned up after delete"
        );

        // Also verify scoped parameters are gone by checking raw keys
        let fed_key = SledParameterStore::scoped_param_key(
            &ParameterScope::Federation { id: fed_id },
            "test.index_cleanup",
        );
        let coop_key = SledParameterStore::scoped_param_key(
            &ParameterScope::Cooperative { id: coop_id },
            "test.index_cleanup",
        );
        assert!(
            !store.db.contains_key(&fed_key).unwrap(),
            "Federation scoped param should be deleted"
        );
        assert!(
            !store.db.contains_key(&coop_key).unwrap(),
            "Cooperative scoped param should be deleted"
        );
    }

    #[test]
    fn test_inmemory_reverse_index_cleanup_on_delete() {
        let store = InMemoryParameterStore::new();

        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Set global parameter with scoped overrides
        store
            .set(
                test_param_with_override("test.index_cleanup", 10, true),
                None,
                None,
            )
            .unwrap();

        let fed_param = ProtocolParameter::new(
            "test.index_cleanup",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(20),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        let coop_param = ProtocolParameter::new(
            "test.index_cleanup",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(30),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        // Verify reverse index exists before delete
        {
            let index = store.scoped_index.read().unwrap();
            assert!(
                index.contains_key("test.index_cleanup"),
                "Reverse index should exist before delete"
            );
            assert_eq!(
                index.get("test.index_cleanup").unwrap().len(),
                2,
                "Should have 2 scoped entries in reverse index"
            );
        }

        // Delete the parameter
        store.delete("test.index_cleanup").unwrap();

        // Verify reverse index is cleaned up
        {
            let index = store.scoped_index.read().unwrap();
            assert!(
                !index.contains_key("test.index_cleanup"),
                "Reverse index should be cleaned up after delete"
            );
        }

        // Verify scoped params are gone
        {
            let scoped = store.scoped_params.read().unwrap();
            assert!(scoped.is_empty(), "All scoped params should be deleted");
        }
    }

    // ========================================
    // Error categorization tests
    // ========================================

    #[test]
    fn test_error_retryability() {
        // Concurrent modification is retryable
        let err = ParameterStoreError::concurrent_modification("test.param", 0, 1);
        assert!(err.is_retryable());
        assert!(err.to_string().contains("Concurrent modification"));

        // Transient storage error is retryable
        let err = ParameterStoreError::storage("disk full", true);
        assert!(err.is_retryable());

        // Permanent storage error is not retryable
        let err = ParameterStoreError::storage("database corrupted", false);
        assert!(!err.is_retryable());

        // Validation error is not retryable
        let validation_err = ParameterValidationError::NotFound("test.param".to_string());
        let err = ParameterStoreError::Validation(validation_err);
        assert!(!err.is_retryable());

        // Scope override not allowed is not retryable
        let err = ParameterStoreError::ScopeOverrideNotAllowed {
            parameter_id: "test.param".to_string(),
        };
        assert!(!err.is_retryable());

        // Lock poisoned is not retryable
        let err = ParameterStoreError::LockPoisoned("mutex poisoned".to_string());
        assert!(!err.is_retryable());

        // Not found is not retryable
        let err = ParameterStoreError::NotFound("test.param".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_error_display_messages() {
        let err = ParameterStoreError::concurrent_modification("test.param", 5, 10);
        let msg = err.to_string();
        assert!(msg.contains("test.param"));
        assert!(msg.contains("5"));
        assert!(msg.contains("10"));
        assert!(msg.contains("Please retry"));

        let err = ParameterStoreError::ScopeOverrideNotAllowed {
            parameter_id: "governance.threshold".to_string(),
        };
        assert!(err.to_string().contains("governance.threshold"));
        assert!(err.to_string().contains("scope overrides"));
    }

    // ========================================
    // Scoped parameter listing and deletion tests
    // ========================================

    #[test]
    fn test_inmemory_list_scoped_parameters() {
        let store = InMemoryParameterStore::new();
        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // No scoped params initially
        assert!(store.list_scoped_parameters().unwrap().is_empty());

        // Create global param with overrides enabled
        store
            .set(
                test_param_with_override("test.scoped", 100, true),
                None,
                None,
            )
            .unwrap();

        // Still no scoped params (only global)
        assert!(store.list_scoped_parameters().unwrap().is_empty());

        // Add federation override
        let fed_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(200),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        let scoped = store.list_scoped_parameters().unwrap();
        assert_eq!(scoped.len(), 1);

        // Add cooperative override
        let coop_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(300),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        let scoped = store.list_scoped_parameters().unwrap();
        assert_eq!(scoped.len(), 2);
    }

    #[test]
    fn test_inmemory_delete_scoped_parameter() {
        let store = InMemoryParameterStore::new();
        let coop_id = EntityId::cooperative("test-coop").unwrap();

        // Create global param with overrides enabled
        store
            .set(
                test_param_with_override("test.delete_scoped", 100, true),
                None,
                None,
            )
            .unwrap();

        // Add cooperative override
        let coop_param = ProtocolParameter::new(
            "test.delete_scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(200),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        assert_eq!(store.list_scoped_parameters().unwrap().len(), 1);

        // Delete the scoped override
        let scope = ParameterScope::Cooperative {
            id: coop_id.clone(),
        };
        let deleted = store
            .delete_scoped_parameter("test.delete_scoped", &scope)
            .unwrap();
        assert!(deleted);

        // Verify it's gone
        assert!(store.list_scoped_parameters().unwrap().is_empty());

        // Global still exists
        assert!(store.exists("test.delete_scoped").unwrap());

        // Deleting again returns false
        let deleted = store
            .delete_scoped_parameter("test.delete_scoped", &scope)
            .unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_inmemory_delete_scoped_global_fails() {
        let store = InMemoryParameterStore::new();
        store
            .set(test_param("test.global", 100), None, None)
            .unwrap();

        let result = store.delete_scoped_parameter("test.global", &ParameterScope::Global);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot delete global"));
    }

    #[test]
    fn test_sled_list_scoped_parameters() {
        let store = SledParameterStore::temporary().unwrap();
        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // No scoped params initially
        assert!(store.list_scoped_parameters().unwrap().is_empty());

        // Create global param with overrides enabled
        store
            .set(
                test_param_with_override("test.scoped", 100, true),
                None,
                None,
            )
            .unwrap();

        // Add overrides
        let fed_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(200),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        let coop_param = ProtocolParameter::new(
            "test.scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(300),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        let scoped = store.list_scoped_parameters().unwrap();
        assert_eq!(scoped.len(), 2);
    }

    #[test]
    fn test_sled_delete_scoped_parameter() {
        let store = SledParameterStore::temporary().unwrap();
        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Create global param with overrides enabled
        store
            .set(
                test_param_with_override("test.delete_scoped", 100, true),
                None,
                None,
            )
            .unwrap();

        // Add two scoped overrides
        let fed_param = ProtocolParameter::new(
            "test.delete_scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(200),
        )
        .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_param, None, None).unwrap();

        let coop_param = ProtocolParameter::new(
            "test.delete_scoped",
            "Scoped",
            "Scoped param",
            ParameterValue::Integer(300),
        )
        .with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_param, None, None).unwrap();

        assert_eq!(store.list_scoped_parameters().unwrap().len(), 2);

        // Delete only the federation override
        let fed_scope = ParameterScope::Federation { id: fed_id.clone() };
        let deleted = store
            .delete_scoped_parameter("test.delete_scoped", &fed_scope)
            .unwrap();
        assert!(deleted);

        // Verify only coop remains
        let remaining = store.list_scoped_parameters().unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(
            matches!(&remaining[0].scope, ParameterScope::Cooperative { id } if id == &coop_id)
        );

        // Global still exists
        assert!(store.exists("test.delete_scoped").unwrap());
    }

    #[test]
    fn test_sled_delete_scoped_global_fails() {
        let store = SledParameterStore::temporary().unwrap();
        store
            .set(test_param("test.global", 100), None, None)
            .unwrap();

        let result = store.delete_scoped_parameter("test.global", &ParameterScope::Global);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Cannot delete global"));
    }

    // ========================================
    // PendingParameterChange tests (InMemory)
    // ========================================

    fn test_pending_change(param_id: &str, effective_at: u64) -> PendingParameterChange {
        PendingParameterChange::new(
            format!("pending:{param_id}:{effective_at}"),
            param_id,
            ParameterValue::Integer(100),
            effective_at,
            ParameterScope::Global,
            "proposal-123",
            "Test rationale",
        )
    }

    #[test]
    fn test_inmemory_pending_change_add_and_get() {
        let store = InMemoryParameterStore::new();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.parameter_id, "test.param");
        assert_eq!(retrieved.effective_at, 1700000000);
        assert_eq!(retrieved.status, PendingChangeStatus::Pending);
    }

    #[test]
    fn test_inmemory_pending_change_duplicate_rejected() {
        let store = InMemoryParameterStore::new();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        // Trying to add same ID again should fail
        let result = store.add_pending_change(change);
        assert!(result.is_err());
    }

    #[test]
    fn test_inmemory_pending_change_list() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.c", 1700000200))
            .unwrap();

        let all = store.list_pending_changes().unwrap();
        assert_eq!(all.len(), 3);
        // Should be sorted by effective_at
        assert_eq!(all[0].parameter_id, "param.b");
        assert_eq!(all[1].parameter_id, "param.a");
        assert_eq!(all[2].parameter_id, "param.c");
    }

    #[test]
    fn test_inmemory_pending_change_list_for_parameter() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.a", 1700000200))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000000))
            .unwrap();

        let a_changes = store.list_pending_changes_for_parameter("param.a").unwrap();
        assert_eq!(a_changes.len(), 2);
        assert!(a_changes.iter().all(|c| c.parameter_id == "param.a"));
    }

    #[test]
    fn test_inmemory_pending_change_get_due() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000200))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.c", 1700000300))
            .unwrap();

        // Get changes due before 1700000150
        let due = store.get_changes_due_before(1700000150).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].parameter_id, "param.a");

        // Get changes due before 1700000250
        let due = store.get_changes_due_before(1700000250).unwrap();
        assert_eq!(due.len(), 2);
    }

    #[test]
    fn test_inmemory_pending_change_cancel() {
        let store = InMemoryParameterStore::new();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        store
            .cancel_pending_change(&change.id, "Cancelled by admin")
            .unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.status, PendingChangeStatus::Cancelled);
        assert_eq!(
            retrieved.cancellation_reason,
            Some("Cancelled by admin".to_string())
        );
    }

    #[test]
    fn test_inmemory_pending_change_count() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000100))
            .unwrap();

        assert_eq!(store.count_pending_changes().unwrap(), 2);

        // Cancel one
        store
            .cancel_pending_change(&"pending:param.a:1700000000".to_string(), "test")
            .unwrap();

        // Count should now be 1
        assert_eq!(store.count_pending_changes().unwrap(), 1);
    }

    // ========================================
    // PendingParameterChange tests (Sled)
    // ========================================

    #[test]
    fn test_sled_pending_change_add_and_get() {
        let store = SledParameterStore::temporary().unwrap();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.parameter_id, "test.param");
        assert_eq!(retrieved.effective_at, 1700000000);
        assert_eq!(retrieved.status, PendingChangeStatus::Pending);
    }

    #[test]
    fn test_sled_pending_change_duplicate_rejected() {
        let store = SledParameterStore::temporary().unwrap();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        // Trying to add same ID again should fail
        let result = store.add_pending_change(change);
        assert!(result.is_err());
    }

    #[test]
    fn test_sled_pending_change_list() {
        let store = SledParameterStore::temporary().unwrap();

        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.c", 1700000200))
            .unwrap();

        let all = store.list_pending_changes().unwrap();
        assert_eq!(all.len(), 3);
        // Should be sorted by effective_at
        assert_eq!(all[0].parameter_id, "param.b");
        assert_eq!(all[1].parameter_id, "param.a");
        assert_eq!(all[2].parameter_id, "param.c");
    }

    #[test]
    fn test_sled_pending_change_get_due() {
        let store = SledParameterStore::temporary().unwrap();

        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000200))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.c", 1700000300))
            .unwrap();

        // Get changes due before 1700000150
        let due = store.get_changes_due_before(1700000150).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].parameter_id, "param.a");

        // Get changes due before 1700000250
        let due = store.get_changes_due_before(1700000250).unwrap();
        assert_eq!(due.len(), 2);
    }

    #[test]
    fn test_sled_pending_change_cancel() {
        let store = SledParameterStore::temporary().unwrap();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        store
            .cancel_pending_change(&change.id, "Cancelled by admin")
            .unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.status, PendingChangeStatus::Cancelled);
        assert_eq!(
            retrieved.cancellation_reason,
            Some("Cancelled by admin".to_string())
        );
    }

    #[test]
    fn test_sled_pending_change_update() {
        let store = SledParameterStore::temporary().unwrap();

        let mut change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        // Mark as applied
        change.mark_applied();
        store.update_pending_change(change.clone()).unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.status, PendingChangeStatus::Applied);
        assert!(retrieved.applied_at.is_some());
    }

    #[test]
    fn test_sled_pending_change_count() {
        let store = SledParameterStore::temporary().unwrap();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000100))
            .unwrap();

        assert_eq!(store.count_pending_changes().unwrap(), 2);

        // Cancel one
        store
            .cancel_pending_change(&"pending:param.a:1700000000".to_string(), "test")
            .unwrap();

        // Count should now be 1
        assert_eq!(store.count_pending_changes().unwrap(), 1);
    }

    #[test]
    fn test_sled_pending_change_cancelled_not_due() {
        let store = SledParameterStore::temporary().unwrap();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        // Cancel it
        store
            .cancel_pending_change(&change.id, "Cancelled")
            .unwrap();

        // It should not be returned as due
        let due = store.get_changes_due_before(1800000000).unwrap();
        assert!(due.is_empty());
    }
}
