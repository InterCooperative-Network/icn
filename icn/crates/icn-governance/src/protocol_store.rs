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
//! # Performance Characteristics
//!
//! - **Get**: O(1) for global, O(1) for scoped lookup
//! - **Set**: O(1) with reverse index maintenance
//! - **Delete**: O(k) where k is the number of scoped overrides for that parameter
//!   (uses reverse index for direct lookup instead of O(n) scan)
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
    ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, ProtocolParameter,
};
use anyhow::Result;
use icn_entity::EntityId;
use sled::Db;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

/// Maximum number of history entries to keep per parameter.
/// This prevents unbounded growth from DoS via repeated parameter changes.
/// History beyond this limit is automatically pruned on each set() call.
pub const MAX_HISTORY_ENTRIES_PER_PARAM: usize = 100;

/// Warning threshold for total history entries across all parameters.
/// When exceeded, a warning is logged to alert operators about potential
/// accumulation issues (e.g., from scoped overrides across many entities).
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

    /// Prune old history entries, keeping only the last `max_entries` per parameter
    ///
    /// Returns the number of entries removed.
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
        param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> Result<()> {
        let id = param.id.clone();
        let scope_key = Self::scope_key(&param.scope);

        // Validate the parameter value (prevents NaN, Infinity, and constraint violations)
        // This is a security check to ensure malformed values cannot bypass governance validation
        param
            .validate(&param.value)
            .map_err(|e| anyhow::anyhow!("Parameter validation failed for '{id}': {e}"))?;

        // Validate scope override permissions for non-global scopes
        if !matches!(param.scope, ParameterScope::Global) {
            if let Some(global_param) = self.get(&id)? {
                if !global_param.constraints.allow_override {
                    return Err(anyhow::anyhow!(
                        "Parameter '{id}' does not allow scope overrides"
                    ));
                }
            }
        }

        // Get old value for history
        let old_value = if matches!(param.scope, ParameterScope::Global) {
            self.get(&id)?.map(|p| p.value)
        } else {
            let scoped = self
                .scoped_params
                .read()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            scoped
                .get(&(scope_key.clone(), id.clone()))
                .map(|p| p.value.clone())
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

    fn prune_history(&self, id: &str, max_entries: usize) -> Result<usize> {
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
        use sled::transaction::ConflictableTransactionError;

        let id = param.id.clone();

        // Validate the parameter value (prevents NaN, Infinity, and constraint violations)
        // This is a security check to ensure malformed values cannot bypass governance validation
        param
            .validate(&param.value)
            .map_err(|e| anyhow::anyhow!("Parameter validation failed for '{id}': {e}"))?;

        // Validate scope override permissions for non-global scopes (outside transaction for efficiency)
        // This is safe because allow_override is immutable once set
        if !matches!(param.scope, ParameterScope::Global) {
            if let Some(global_param) = self.get(&id)? {
                if !global_param.constraints.allow_override {
                    return Err(anyhow::anyhow!(
                        "Parameter '{id}' does not allow scope overrides"
                    ));
                }
            }
        }

        // Prepare the key for this parameter
        let is_scoped = !matches!(param.scope, ParameterScope::Global);
        let param_key = if is_scoped {
            Self::scoped_param_key(&param.scope, &id)
        } else {
            Self::param_key(&id)
        };

        // Prepare serialized new parameter value
        let param_value = Self::serialize(&param)?;

        // Generate a unique nonce for history key (must be done before transaction)
        let nonce = self.db.generate_id()?;
        let now = icn_time::current_timestamp_secs();

        // Clone values needed inside the transaction closure
        let id_clone = id.clone();
        let new_value = param.value.clone();

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

        // Use transaction to atomically read old value and write new value + history + index
        self.db
            .transaction(|tx| {
                // Read old value INSIDE the transaction for atomicity
                let old_value = tx
                    .get(&param_key)?
                    .and_then(|bytes| Self::deserialize_param(&bytes).ok().map(|p| p.value));

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
        let prefix = b"param:";
        let mut params = Vec::new();

        for item in self.db.scan_prefix(prefix) {
            let (key, value) = item?;
            // Skip scoped params (they have param_scope: prefix)
            let key_str = String::from_utf8_lossy(&key);
            if key_str.starts_with("param:") && !key_str.starts_with("param_scope:") {
                params.push(Self::deserialize_param(&value)?);
            }
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

    fn prune_history(&self, id: &str, max_entries: usize) -> Result<usize> {
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
        let scoped_keys: Vec<Vec<u8>> = self
            .db
            .get(&index_key)?
            .and_then(|bytes| {
                let scope_strs: Vec<String> = serde_json::from_slice(&bytes).ok()?;
                Some(
                    scope_strs
                        .into_iter()
                        .map(|scope_str| format!("param_scope:{scope_str}:{id}").into_bytes())
                        .collect(),
                )
            })
            .unwrap_or_default();

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
        let prefix = b"param:";
        let mut count = 0;
        for item in self.db.scan_prefix(prefix) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.starts_with("param:") && !key_str.starts_with("param_scope:") {
                count += 1;
            }
        }
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
            let param = ProtocolParameter::new(
                "test.prune",
                "Prune Test",
                "Test pruning",
                ParameterValue::Integer(i),
            );
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
            let param = ProtocolParameter::new(
                "test.prune",
                "Prune Test",
                "Test pruning",
                ParameterValue::Integer(i),
            );
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
}
