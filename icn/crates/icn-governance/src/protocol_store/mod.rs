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

// Module structure
pub mod state;
pub mod inmemory;
pub mod sled;

// Re-export public API
pub use state::{
    InMemoryParameterStore, 
    ParameterStoreError, 
    ProtocolParameterStore, 
    SledParameterStore,
    MAX_HISTORY_ENTRIES_PER_PARAM,
    GLOBAL_HISTORY_WARNING_THRESHOLD,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use state::*;
    use crate::protocol::{
        ParameterScope, ParameterValue,
        PendingChangeStatus, PendingParameterChange, ProtocolParameter,
    };
    use icn_entity::EntityId;

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
        assert_eq!(retrieved.value, ParameterValue::Integer(42));
        assert_eq!(retrieved.version, 0);
    }

    #[test]
    fn test_inmemory_version_increment() {
        let store = InMemoryParameterStore::new();

        // Initial set
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Update - should increment version
        let mut updated = test_param("test.value", 2);
        updated.version = 0; // Current version
        store.set(updated, None, None).unwrap();

        let retrieved = store.get("test.value").unwrap().unwrap();
        assert_eq!(retrieved.value, ParameterValue::Integer(2));
        assert_eq!(retrieved.version, 1);
    }

    #[test]
    fn test_inmemory_concurrent_modification() {
        let store = InMemoryParameterStore::new();

        // Initial set
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Try to update with wrong version
        let mut bad_update = test_param("test.value", 2);
        bad_update.version = 99; // Wrong version
        let result = store.set(bad_update, None, None);

        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("expected version 99"));
    }

    #[test]
    fn test_inmemory_list() {
        let store = InMemoryParameterStore::new();

        store.set(test_param("a.one", 1), None, None).unwrap();
        store.set(test_param("b.two", 2), None, None).unwrap();

        let params = store.list().unwrap();
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_inmemory_list_by_category() {
        let store = InMemoryParameterStore::new();

        store.set(test_param("alpha.one", 1), None, None).unwrap();
        store.set(test_param("alpha.two", 2), None, None).unwrap();
        store.set(test_param("beta.three", 3), None, None).unwrap();

        let alpha_params = store.list_by_category("alpha").unwrap();
        assert_eq!(alpha_params.len(), 2);

        let beta_params = store.list_by_category("beta").unwrap();
        assert_eq!(beta_params.len(), 1);
    }

    #[test]
    fn test_inmemory_history() {
        let store = InMemoryParameterStore::new();

        // Initial set - no history entry
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Update - creates history entry
        let mut updated = test_param("test.value", 2);
        updated.version = 0;
        store
            .set(updated, Some("prop-1".to_string()), Some("alice".to_string()))
            .unwrap();

        let history = store.get_history("test.value").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].old_value, ParameterValue::Integer(1));
        assert_eq!(history[0].new_value, ParameterValue::Integer(2));
        assert_eq!(history[0].proposal_id, Some("prop-1".to_string()));
        assert_eq!(history[0].changed_by, Some("alice".to_string()));
    }

    #[test]
    fn test_inmemory_history_pagination() {
        let store = InMemoryParameterStore::new();

        // Create parameter with multiple updates
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Create 5 history entries
        for i in 2..=6 {
            let mut updated = test_param("test.value", i);
            updated.version = (i - 2) as u64;
            store.set(updated, None, None).unwrap();
        }

        // Get first page (3 entries)
        let (page1, total) = store.get_history_paginated("test.value", 0, 3).unwrap();
        assert_eq!(total, 5);
        assert_eq!(page1.len(), 3);

        // Get second page (2 entries)
        let (page2, total) = store.get_history_paginated("test.value", 3, 3).unwrap();
        assert_eq!(total, 5);
        assert_eq!(page2.len(), 2);
    }

    #[test]
    fn test_inmemory_prune_history() {
        let store = InMemoryParameterStore::new();

        // Create parameter with multiple updates
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Create 10 history entries
        for i in 2..=11 {
            let mut updated = test_param("test.value", i);
            updated.version = (i - 2) as u64;
            store.set(updated, None, None).unwrap();
        }

        let history = store.get_history("test.value").unwrap();
        assert_eq!(history.len(), 10);

        // Prune to 5 entries
        let pruned = store.prune_history("test.value", 5).unwrap();
        assert_eq!(pruned, 5);

        let history = store.get_history("test.value").unwrap();
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_inmemory_delete() {
        let store = InMemoryParameterStore::new();

        store.set(test_param("test.value", 42), None, None).unwrap();
        assert!(store.get("test.value").unwrap().is_some());

        store.delete("test.value").unwrap();
        assert!(store.get("test.value").unwrap().is_none());
    }

    #[test]
    fn test_inmemory_exists() {
        let store = InMemoryParameterStore::new();

        assert!(!store.exists("test.value").unwrap());

        store.set(test_param("test.value", 42), None, None).unwrap();
        assert!(store.exists("test.value").unwrap());
    }

    #[test]
    fn test_inmemory_count() {
        let store = InMemoryParameterStore::new();

        assert_eq!(store.count().unwrap(), 0);

        store.set(test_param("a.one", 1), None, None).unwrap();
        store.set(test_param("b.two", 2), None, None).unwrap();

        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_inmemory_scoped_override() {
        let store = InMemoryParameterStore::new();

        // Set global parameter
        let global = test_param_with_override("test.value", 100, true);
        store.set(global.clone(), None, None).unwrap();

        // Set federation override
        let coop_id: EntityId = "entity:icn:cooperative:test-coop".parse().unwrap();
        let mut fed_override = test_param("test.value", 200);
        fed_override.scope = ParameterScope::Cooperative { id: coop_id.clone() };
        fed_override.constraints.allow_override = true;
        store.set(fed_override, None, None).unwrap();

        // get_effective should return the federation override
        let effective = store
            .get_effective("test.value", Some(&coop_id), None)
            .unwrap()
            .unwrap();
        assert_eq!(effective.value, ParameterValue::Integer(200));
    }

    #[test]
    fn test_inmemory_list_scoped_parameters() {
        let store = InMemoryParameterStore::new();

        // Global parameter
        let global = test_param_with_override("test.value", 100, true);
        store.set(global, None, None).unwrap();

        // Federation override
        let fed_id: EntityId = "entity:icn:federation:test-fed".parse().unwrap();
        let mut fed_override = test_param("test.value", 200);
        fed_override.scope = ParameterScope::Federation { id: fed_id };
        fed_override.constraints.allow_override = true;
        store.set(fed_override, None, None).unwrap();

        // Cooperative override
        let coop_id: EntityId = "entity:icn:cooperative:test-coop".parse().unwrap();
        let mut coop_override = test_param("test.value", 300);
        coop_override.scope = ParameterScope::Cooperative { id: coop_id };
        coop_override.constraints.allow_override = true;
        store.set(coop_override, None, None).unwrap();

        let scoped = store.list_scoped_parameters().unwrap();
        assert_eq!(scoped.len(), 2); // 2 overrides (not including global)
    }

    #[test]
    fn test_inmemory_delete_scoped_parameter() {
        let store = InMemoryParameterStore::new();

        // Global parameter
        let global = test_param_with_override("test.value", 100, true);
        store.set(global, None, None).unwrap();

        // Federation override
        let fed_id: EntityId = "entity:icn:federation:test-fed".parse().unwrap();
        let mut fed_override = test_param("test.value", 200);
        fed_override.scope = ParameterScope::Federation { id: fed_id.clone() };
        fed_override.constraints.allow_override = true;
        store.set(fed_override.clone(), None, None).unwrap();

        // Delete the override
        let deleted = store
            .delete_scoped_parameter("test.value", &fed_override.scope)
            .unwrap();
        assert!(deleted);

        // Global should still exist
        assert!(store.get("test.value").unwrap().is_some());

        // Override should be gone
        let effective = store
            .get_effective("test.value", None, Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(effective.value, ParameterValue::Integer(100)); // Falls back to global
    }

    // Test pending changes

    fn test_pending_change(param_id: &str, effective_at: u64) -> PendingParameterChange {
        PendingParameterChange {
            id: format!("pending:{param_id}:{effective_at}"),
            parameter_id: param_id.to_string(),
            new_value: ParameterValue::Integer(999),
            effective_at,
            scope: ParameterScope::Global,
            proposal_id: "test-proposal".to_string(),
            created_at: 1700000000,
            status: PendingChangeStatus::Pending,
            rationale: "Test rationale".to_string(),
            superseded_by: None,
            applied_at: None,
            cancellation_reason: None,
        }
    }

    #[test]
    fn test_inmemory_pending_change_add_and_get() {
        let store = InMemoryParameterStore::new();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.id, change.id);
        assert_eq!(retrieved.parameter_id, "test.param");
    }

    #[test]
    fn test_inmemory_list_pending_changes() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000100))
            .unwrap();

        let all = store.list_pending_changes().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_inmemory_list_pending_changes_for_parameter() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000200))
            .unwrap();

        let for_a = store.list_pending_changes_for_parameter("param.a").unwrap();
        assert_eq!(for_a.len(), 2);

        let for_b = store.list_pending_changes_for_parameter("param.b").unwrap();
        assert_eq!(for_b.len(), 1);
    }

    #[test]
    fn test_inmemory_get_changes_due_before() {
        let store = InMemoryParameterStore::new();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.c", 1700000200))
            .unwrap();

        // Get changes due before timestamp 1700000150
        let due = store.get_changes_due_before(1700000150).unwrap();
        assert_eq!(due.len(), 2); // param.a and param.b

        // Check ordering (should be ascending by effective_at)
        assert_eq!(due[0].effective_at, 1700000000);
        assert_eq!(due[1].effective_at, 1700000100);
    }

    #[test]
    fn test_inmemory_update_pending_change() {
        let store = InMemoryParameterStore::new();

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
    fn test_inmemory_cancel_pending_change() {
        let store = InMemoryParameterStore::new();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        store
            .cancel_pending_change(&change.id, "Test cancellation")
            .unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.status, PendingChangeStatus::Cancelled);
        assert_eq!(
            retrieved.cancellation_reason,
            Some("Test cancellation".to_string())
        );
    }

    #[test]
    fn test_inmemory_count_pending_changes() {
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

        // Count should now be 1 (only pending ones counted)
        assert_eq!(store.count_pending_changes().unwrap(), 1);
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
        assert_eq!(retrieved.value, ParameterValue::Integer(42));
        assert_eq!(retrieved.version, 0);
    }

    #[test]
    fn test_sled_version_increment() {
        let store = SledParameterStore::temporary().unwrap();

        // Initial set
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Update - should increment version
        let mut updated = test_param("test.value", 2);
        updated.version = 0; // Current version
        store.set(updated, None, None).unwrap();

        let retrieved = store.get("test.value").unwrap().unwrap();
        assert_eq!(retrieved.value, ParameterValue::Integer(2));
        assert_eq!(retrieved.version, 1);
    }

    #[test]
    fn test_sled_concurrent_modification() {
        let store = SledParameterStore::temporary().unwrap();

        // Initial set
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Try to update with wrong version
        let mut bad_update = test_param("test.value", 2);
        bad_update.version = 99; // Wrong version
        let result = store.set(bad_update, None, None);

        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("expected version 99"));
    }

    #[test]
    fn test_sled_list() {
        let store = SledParameterStore::temporary().unwrap();

        store.set(test_param("a.one", 1), None, None).unwrap();
        store.set(test_param("b.two", 2), None, None).unwrap();

        let params = store.list().unwrap();
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_sled_list_by_category() {
        let store = SledParameterStore::temporary().unwrap();

        store.set(test_param("alpha.one", 1), None, None).unwrap();
        store.set(test_param("alpha.two", 2), None, None).unwrap();
        store.set(test_param("beta.three", 3), None, None).unwrap();

        let alpha_params = store.list_by_category("alpha").unwrap();
        assert_eq!(alpha_params.len(), 2);

        let beta_params = store.list_by_category("beta").unwrap();
        assert_eq!(beta_params.len(), 1);
    }

    #[test]
    fn test_sled_history() {
        let store = SledParameterStore::temporary().unwrap();

        // Initial set - no history entry
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Update - creates history entry
        let mut updated = test_param("test.value", 2);
        updated.version = 0;
        store
            .set(updated, Some("prop-1".to_string()), Some("alice".to_string()))
            .unwrap();

        let history = store.get_history("test.value").unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].old_value, ParameterValue::Integer(1));
        assert_eq!(history[0].new_value, ParameterValue::Integer(2));
        assert_eq!(history[0].proposal_id, Some("prop-1".to_string()));
        assert_eq!(history[0].changed_by, Some("alice".to_string()));
    }

    #[test]
    fn test_sled_history_pagination() {
        let store = SledParameterStore::temporary().unwrap();

        // Create parameter with multiple updates
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Create 5 history entries
        for i in 2..=6 {
            let mut updated = test_param("test.value", i);
            updated.version = (i - 2) as u64;
            store.set(updated, None, None).unwrap();
        }

        // Get first page (3 entries)
        let (page1, total) = store.get_history_paginated("test.value", 0, 3).unwrap();
        assert_eq!(total, 5);
        assert_eq!(page1.len(), 3);

        // Get second page (2 entries)
        let (page2, total) = store.get_history_paginated("test.value", 3, 3).unwrap();
        assert_eq!(total, 5);
        assert_eq!(page2.len(), 2);
    }

    #[test]
    fn test_sled_prune_history() {
        let store = SledParameterStore::temporary().unwrap();

        // Create parameter with multiple updates
        let param = test_param("test.value", 1);
        store.set(param.clone(), None, None).unwrap();

        // Create 10 history entries
        for i in 2..=11 {
            let mut updated = test_param("test.value", i);
            updated.version = (i - 2) as u64;
            store.set(updated, None, None).unwrap();
        }

        let history = store.get_history("test.value").unwrap();
        assert_eq!(history.len(), 10);

        // Prune to 5 entries
        let pruned = store.prune_history("test.value", 5).unwrap();
        assert_eq!(pruned, 5);

        let history = store.get_history("test.value").unwrap();
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_sled_delete() {
        let store = SledParameterStore::temporary().unwrap();

        store.set(test_param("test.value", 42), None, None).unwrap();
        assert!(store.get("test.value").unwrap().is_some());

        store.delete("test.value").unwrap();
        assert!(store.get("test.value").unwrap().is_none());
    }

    #[test]
    fn test_sled_exists() {
        let store = SledParameterStore::temporary().unwrap();

        assert!(!store.exists("test.value").unwrap());

        store.set(test_param("test.value", 42), None, None).unwrap();
        assert!(store.exists("test.value").unwrap());
    }

    #[test]
    fn test_sled_count() {
        let store = SledParameterStore::temporary().unwrap();

        assert_eq!(store.count().unwrap(), 0);

        store.set(test_param("a.one", 1), None, None).unwrap();
        store.set(test_param("b.two", 2), None, None).unwrap();

        assert_eq!(store.count().unwrap(), 2);
    }

    #[test]
    fn test_sled_scoped_override() {
        let store = SledParameterStore::temporary().unwrap();

        // Set global parameter
        let global = test_param_with_override("test.value", 100, true);
        store.set(global.clone(), None, None).unwrap();

        // Set federation override
        let coop_id: EntityId = "entity:icn:cooperative:test-coop".parse().unwrap();
        let mut coop_override = test_param("test.value", 200);
        coop_override.scope = ParameterScope::Cooperative { id: coop_id.clone() };
        coop_override.constraints.allow_override = true;
        store.set(coop_override, None, None).unwrap();

        // get_effective should return the cooperative override
        let effective = store
            .get_effective("test.value", Some(&coop_id), None)
            .unwrap()
            .unwrap();
        assert_eq!(effective.value, ParameterValue::Integer(200));
    }

    #[test]
    fn test_sled_list_scoped_parameters() {
        let store = SledParameterStore::temporary().unwrap();

        // Global parameter
        let global = test_param_with_override("test.value", 100, true);
        store.set(global, None, None).unwrap();

        // Federation override
        let fed_id: EntityId = "entity:icn:federation:test-fed".parse().unwrap();
        let mut fed_override = test_param("test.value", 200);
        fed_override.scope = ParameterScope::Federation { id: fed_id };
        fed_override.constraints.allow_override = true;
        store.set(fed_override, None, None).unwrap();

        // Cooperative override
        let coop_id: EntityId = "entity:icn:cooperative:test-coop".parse().unwrap();
        let mut coop_override = test_param("test.value", 300);
        coop_override.scope = ParameterScope::Cooperative { id: coop_id };
        coop_override.constraints.allow_override = true;
        store.set(coop_override, None, None).unwrap();

        let scoped = store.list_scoped_parameters().unwrap();
        assert_eq!(scoped.len(), 2); // 2 overrides (not including global)
    }

    #[test]
    fn test_sled_delete_scoped_parameter() {
        let store = SledParameterStore::temporary().unwrap();

        // Global parameter
        let global = test_param_with_override("test.value", 100, true);
        store.set(global, None, None).unwrap();

        // Federation override
        let fed_id: EntityId = "entity:icn:federation:test-fed".parse().unwrap();
        let mut fed_override = test_param("test.value", 200);
        fed_override.scope = ParameterScope::Federation { id: fed_id.clone() };
        fed_override.constraints.allow_override = true;
        store.set(fed_override.clone(), None, None).unwrap();

        // Delete the override
        let deleted = store
            .delete_scoped_parameter("test.value", &fed_override.scope)
            .unwrap();
        assert!(deleted);

        // Global should still exist
        assert!(store.get("test.value").unwrap().is_some());

        // Override should be gone
        let effective = store
            .get_effective("test.value", None, Some(&fed_id))
            .unwrap()
            .unwrap();
        assert_eq!(effective.value, ParameterValue::Integer(100)); // Falls back to global
    }

    #[test]
    fn test_sled_pending_change_add_and_get() {
        let store = SledParameterStore::temporary().unwrap();

        let change = test_pending_change("test.param", 1700000000);
        store.add_pending_change(change.clone()).unwrap();

        let retrieved = store.get_pending_change(&change.id).unwrap().unwrap();
        assert_eq!(retrieved.id, change.id);
        assert_eq!(retrieved.parameter_id, "test.param");
    }

    #[test]
    fn test_sled_list_pending_changes() {
        let store = SledParameterStore::temporary().unwrap();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000100))
            .unwrap();

        let all = store.list_pending_changes().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_sled_list_pending_changes_for_parameter() {
        let store = SledParameterStore::temporary().unwrap();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.a", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000200))
            .unwrap();

        let for_a = store.list_pending_changes_for_parameter("param.a").unwrap();
        assert_eq!(for_a.len(), 2);

        let for_b = store.list_pending_changes_for_parameter("param.b").unwrap();
        assert_eq!(for_b.len(), 1);
    }

    #[test]
    fn test_sled_get_changes_due_before() {
        let store = SledParameterStore::temporary().unwrap();

        store
            .add_pending_change(test_pending_change("param.a", 1700000000))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.b", 1700000100))
            .unwrap();
        store
            .add_pending_change(test_pending_change("param.c", 1700000200))
            .unwrap();

        // Get changes due before timestamp 1700000150
        let due = store.get_changes_due_before(1700000150).unwrap();
        assert_eq!(due.len(), 2); // param.a and param.b

        // Check ordering (should be ascending by effective_at)
        assert_eq!(due[0].effective_at, 1700000000);
        assert_eq!(due[1].effective_at, 1700000100);
    }

    #[test]
    fn test_sled_update_pending_change() {
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
