//! Sled-backed Protocol Parameter Store Implementation
//!
//! This module provides a persistent Sled-backed implementation of the
//! ProtocolParameterStore trait for production use.

use super::state::*;
use crate::protocol::{
    ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, PendingChangeId,
    PendingChangeStatus, PendingParameterChange, ProtocolParameter,
};
use anyhow::Result;
use icn_entity::EntityId;
use tracing::{debug, warn};

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
