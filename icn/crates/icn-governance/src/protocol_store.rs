//! Protocol Parameter Storage
//!
//! This module provides persistent storage for protocol parameters.
//! Parameters are stored in Sled with history tracking for audit trails.
//!
//! # Key Schema
//!
//! - `param:{id}` -> ProtocolParameter (JSON)
//! - `param_scope:{scope}:{id}` -> ProtocolParameter (JSON) for scoped overrides
//! - `history:{id}:{timestamp}` -> ParameterChange (JSON)
//!
//! # Scope Resolution
//!
//! When getting a parameter value, the store resolves scopes in order:
//! 1. Cooperative scope (most specific)
//! 2. Federation scope
//! 3. Global scope (default)
//!
//! # Known Limitations
//!
//! - **Atomicity**: The `set()` operation performs read-modify-write without
//!   transactions. Concurrent updates could interleave. A future enhancement
//!   should use Sled transactions.
//! - **Delete Performance**: The `delete()` method scans all scoped parameters
//!   (O(n) complexity). For large parameter sets, consider a reverse index.
//! - **History Growth**: Parameter history is unbounded. Consider adding a
//!   cleanup mechanism for very old history entries.

use crate::protocol::{
    ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, ProtocolParameter,
};
use anyhow::Result;
use icn_entity::EntityId;
use sled::Db;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::debug;

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
            let mut scoped = self
                .scoped_params
                .write()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
            scoped.insert((scope_key, id.clone()), param.clone());
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
            history.entry(id).or_default().push(change);
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
        Ok(history.get(id).cloned().unwrap_or_default())
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
        let mut params = self
            .params
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        params.remove(id);

        // Also remove scoped versions
        let mut scoped = self
            .scoped_params
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        scoped.retain(|(_, param_id), _| param_id != id);

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

    fn set(
        &self,
        param: ProtocolParameter,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) -> Result<()> {
        let id = param.id.clone();

        // Validate scope override permissions for non-global scopes
        if !matches!(param.scope, ParameterScope::Global) {
            // Check if the global parameter allows overrides
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
            let key = Self::scoped_param_key(&param.scope, &id);
            self.db
                .get(&key)?
                .and_then(|bytes| Self::deserialize_param(&bytes).ok())
                .map(|p| p.value)
        };

        // Store the parameter
        let value = Self::serialize(&param)?;
        if matches!(param.scope, ParameterScope::Global) {
            let key = Self::param_key(&id);
            self.db.insert(&key, value)?;
        } else {
            let key = Self::scoped_param_key(&param.scope, &id);
            self.db.insert(&key, value)?;
        }

        // Record history if we had an old value
        if let Some(old) = old_value {
            let now = icn_time::current_timestamp_secs();
            let nonce = self.db.generate_id()?;
            let change = ParameterChange {
                parameter_id: id.clone(),
                old_value: old,
                new_value: param.value,
                changed_at: now,
                proposal_id,
                changed_by,
            };

            let history_key = Self::history_key(&id, now, nonce);
            let history_value = Self::serialize(&change)?;
            self.db.insert(&history_key, history_value)?;
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
        // Delete global parameter
        let key = Self::param_key(id);
        self.db.remove(&key)?;

        // Delete scoped parameters
        let scoped_prefix = b"param_scope:";
        for item in self.db.scan_prefix(scoped_prefix) {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.ends_with(&format!(":{id}")) {
                self.db.remove(&key)?;
            }
        }

        // Delete history
        let history_prefix = Self::history_prefix(id);
        for item in self.db.scan_prefix(&history_prefix) {
            let (key, _) = item?;
            self.db.remove(&key)?;
        }

        debug!(parameter_id = %id, "Parameter deleted");
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
}
