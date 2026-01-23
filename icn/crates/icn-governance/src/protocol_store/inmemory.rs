//! In-Memory Protocol Parameter Store Implementation
//!
//! This module provides an in-memory implementation of the ProtocolParameterStore
//! trait, primarily used for testing.

use super::state::*;
use crate::protocol::{
    ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, PendingChangeId,
    PendingChangeStatus, PendingParameterChange, ProtocolParameter,
};
use anyhow::Result;
use icn_entity::EntityId;
use tracing::{debug, warn};

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
