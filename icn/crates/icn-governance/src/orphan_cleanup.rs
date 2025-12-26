//! Orphan Parameter Cleanup
//!
//! This module provides functionality to detect and clean up orphan scoped parameters.
//! A scoped parameter becomes orphaned when its target entity (cooperative or federation)
//! is deleted while the parameter override still exists.
//!
//! # Background
//!
//! There's a narrow TOCTOU window where scoped parameters may become orphaned:
//! 1. Governance validates entity exists
//! 2. Entity is deleted (rare, requires governance approval)
//! 3. Parameter is persisted with now-deleted entity scope
//!
//! While rare due to governed entity deletion, cleanup is needed for data hygiene.
//!
//! # Usage
//!
//! ```ignore
//! use icn_governance::orphan_cleanup::{cleanup_orphan_parameters, OrphanCleanupConfig};
//!
//! // Run cleanup with a function that checks entity existence
//! let result = cleanup_orphan_parameters(
//!     &parameter_store,
//!     |entity_id| async move { entity_registry.exists(entity_id).await },
//!     OrphanCleanupConfig::default(),
//! ).await?;
//!
//! println!("Found {} orphans, deleted {}", result.orphans_found, result.orphans_deleted);
//! ```

use crate::protocol::ParameterScope;
use crate::protocol_store::ProtocolParameterStore;
use anyhow::Result;
use icn_entity::EntityId;
use std::future::Future;
use tracing::{debug, info, warn};

/// Configuration for orphan cleanup behavior
#[derive(Debug, Clone)]
pub struct OrphanCleanupConfig {
    /// Whether to actually delete orphans (false = dry run, report only)
    pub delete_orphans: bool,
    /// Maximum number of orphans to delete in one run (0 = unlimited)
    pub max_deletions: usize,
}

impl Default for OrphanCleanupConfig {
    fn default() -> Self {
        Self {
            delete_orphans: true,
            max_deletions: 0, // unlimited
        }
    }
}

impl OrphanCleanupConfig {
    /// Create a dry-run config that only reports orphans without deleting
    pub fn dry_run() -> Self {
        Self {
            delete_orphans: false,
            max_deletions: 0,
        }
    }
}

/// Result of an orphan cleanup operation
#[derive(Debug, Clone, Default)]
pub struct OrphanCleanupResult {
    /// Total number of scoped parameters checked
    pub total_scoped_params: usize,
    /// Number of orphans found (entity no longer exists)
    pub orphans_found: usize,
    /// Number of orphans successfully deleted
    pub orphans_deleted: usize,
    /// Number of orphans that failed to delete
    pub deletion_failures: usize,
    /// Details of each orphan found
    pub orphan_details: Vec<OrphanDetail>,
    /// Whether this was a dry run (no actual deletions)
    pub dry_run: bool,
}

/// Details about a single orphan parameter
#[derive(Debug, Clone)]
pub struct OrphanDetail {
    /// The parameter ID
    pub parameter_id: String,
    /// The orphaned scope
    pub scope: ParameterScope,
    /// The entity ID that no longer exists
    pub missing_entity_id: EntityId,
    /// Whether deletion was attempted
    pub deletion_attempted: bool,
    /// Whether deletion succeeded (None if not attempted)
    pub deletion_succeeded: Option<bool>,
    /// Error message if deletion failed
    pub error: Option<String>,
}

/// Clean up orphan scoped parameters
///
/// Scans all scoped parameters and checks if their target entities still exist.
/// Parameters referencing deleted entities are considered orphans and can be
/// removed based on the configuration.
///
/// # Arguments
///
/// * `store` - The protocol parameter store to scan and clean
/// * `entity_exists` - An async function that checks if an entity exists
/// * `config` - Configuration controlling cleanup behavior
///
/// # Returns
///
/// An `OrphanCleanupResult` containing statistics and details about found/deleted orphans.
///
/// # Example
///
/// ```ignore
/// let result = cleanup_orphan_parameters(
///     &store,
///     |id| async move { registry.exists(id).await.unwrap_or(false) },
///     OrphanCleanupConfig::default(),
/// ).await?;
/// ```
pub async fn cleanup_orphan_parameters<F, Fut>(
    store: &dyn ProtocolParameterStore,
    entity_exists: F,
    config: OrphanCleanupConfig,
) -> Result<OrphanCleanupResult>
where
    F: Fn(&EntityId) -> Fut,
    Fut: Future<Output = bool>,
{
    let mut result = OrphanCleanupResult {
        dry_run: !config.delete_orphans,
        ..Default::default()
    };

    // Get all scoped parameters
    let scoped_params = store.list_scoped_parameters()?;
    result.total_scoped_params = scoped_params.len();

    if scoped_params.is_empty() {
        debug!("No scoped parameters found, nothing to clean up");
        return Ok(result);
    }

    info!(
        total_scoped_params = result.total_scoped_params,
        dry_run = result.dry_run,
        "Starting orphan parameter cleanup"
    );

    // Check each scoped parameter
    for param in scoped_params {
        let entity_id = match &param.scope {
            ParameterScope::Global => continue, // Skip global params
            ParameterScope::Federation { id } | ParameterScope::Cooperative { id } => id.clone(),
        };

        // Check if entity still exists
        let exists = entity_exists(&entity_id).await;

        if !exists {
            // Found an orphan!
            result.orphans_found += 1;

            let mut detail = OrphanDetail {
                parameter_id: param.id.clone(),
                scope: param.scope.clone(),
                missing_entity_id: entity_id.clone(),
                deletion_attempted: false,
                deletion_succeeded: None,
                error: None,
            };

            warn!(
                parameter_id = %param.id,
                scope = ?param.scope,
                entity_id = %entity_id.as_str(),
                "Found orphan parameter (entity no longer exists)"
            );

            // Delete if configured to do so
            if config.delete_orphans {
                // Check max deletions limit
                if config.max_deletions > 0 && result.orphans_deleted >= config.max_deletions {
                    debug!(
                        max_deletions = config.max_deletions,
                        "Reached max deletions limit, skipping remaining orphans"
                    );
                    result.orphan_details.push(detail);
                    continue;
                }

                detail.deletion_attempted = true;

                match store.delete_scoped_parameter(&param.id, &param.scope) {
                    Ok(true) => {
                        result.orphans_deleted += 1;
                        detail.deletion_succeeded = Some(true);
                        info!(
                            parameter_id = %param.id,
                            scope = ?param.scope,
                            "Deleted orphan parameter"
                        );
                    }
                    Ok(false) => {
                        // Parameter was already deleted (race condition)
                        detail.deletion_succeeded = Some(false);
                        detail.error = Some("Parameter not found (already deleted?)".to_string());
                    }
                    Err(e) => {
                        result.deletion_failures += 1;
                        detail.deletion_succeeded = Some(false);
                        detail.error = Some(e.to_string());
                        warn!(
                            parameter_id = %param.id,
                            scope = ?param.scope,
                            error = %e,
                            "Failed to delete orphan parameter"
                        );
                    }
                }
            }

            result.orphan_details.push(detail);
        }
    }

    info!(
        total_scoped = result.total_scoped_params,
        orphans_found = result.orphans_found,
        orphans_deleted = result.orphans_deleted,
        deletion_failures = result.deletion_failures,
        dry_run = result.dry_run,
        "Completed orphan parameter cleanup"
    );

    Ok(result)
}

/// Synchronous wrapper for cleanup when async is not needed
///
/// This is useful for CLI commands or batch jobs where the entity check
/// can be done synchronously.
pub fn cleanup_orphan_parameters_sync<F>(
    store: &dyn ProtocolParameterStore,
    entity_exists: F,
    config: OrphanCleanupConfig,
) -> Result<OrphanCleanupResult>
where
    F: Fn(&EntityId) -> bool,
{
    let mut result = OrphanCleanupResult {
        dry_run: !config.delete_orphans,
        ..Default::default()
    };

    // Get all scoped parameters
    let scoped_params = store.list_scoped_parameters()?;
    result.total_scoped_params = scoped_params.len();

    if scoped_params.is_empty() {
        return Ok(result);
    }

    // Check each scoped parameter
    for param in scoped_params {
        let entity_id = match &param.scope {
            ParameterScope::Global => continue,
            ParameterScope::Federation { id } | ParameterScope::Cooperative { id } => id.clone(),
        };

        if !entity_exists(&entity_id) {
            result.orphans_found += 1;

            let mut detail = OrphanDetail {
                parameter_id: param.id.clone(),
                scope: param.scope.clone(),
                missing_entity_id: entity_id,
                deletion_attempted: false,
                deletion_succeeded: None,
                error: None,
            };

            if config.delete_orphans {
                if config.max_deletions > 0 && result.orphans_deleted >= config.max_deletions {
                    result.orphan_details.push(detail);
                    continue;
                }

                detail.deletion_attempted = true;
                match store.delete_scoped_parameter(&param.id, &param.scope) {
                    Ok(true) => {
                        result.orphans_deleted += 1;
                        detail.deletion_succeeded = Some(true);
                    }
                    Ok(false) => {
                        detail.deletion_succeeded = Some(false);
                        detail.error = Some("Parameter not found".to_string());
                    }
                    Err(e) => {
                        result.deletion_failures += 1;
                        detail.deletion_succeeded = Some(false);
                        detail.error = Some(e.to_string());
                    }
                }
            }

            result.orphan_details.push(detail);
        }
    }

    Ok(result)
}

/// Count orphan parameters without deleting (quick check)
///
/// This is a lightweight operation that only counts orphans without
/// modifying any data.
pub async fn count_orphan_parameters<F, Fut>(
    store: &dyn ProtocolParameterStore,
    entity_exists: F,
) -> Result<usize>
where
    F: Fn(&EntityId) -> Fut,
    Fut: Future<Output = bool>,
{
    let scoped_params = store.list_scoped_parameters()?;
    let mut count = 0;

    for param in scoped_params {
        let entity_id = match &param.scope {
            ParameterScope::Global => continue,
            ParameterScope::Federation { id } | ParameterScope::Cooperative { id } => id,
        };

        if !entity_exists(entity_id).await {
            count += 1;
        }
    }

    Ok(count)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::protocol::{ParameterValue, ProtocolParameter};
    use crate::protocol_store::InMemoryParameterStore;
    use std::collections::HashSet;

    fn test_param(id: &str, value: i64) -> ProtocolParameter {
        ProtocolParameter::new(
            id,
            "Test Parameter",
            "A test parameter",
            ParameterValue::Integer(value),
        )
    }

    fn test_param_with_override(id: &str, value: i64) -> ProtocolParameter {
        let mut param = test_param(id, value);
        param.constraints.allow_override = true;
        param
    }

    #[test]
    fn test_no_scoped_params() {
        let store = InMemoryParameterStore::new();
        store
            .set(test_param("test.global", 100), None, None)
            .unwrap();

        let result = cleanup_orphan_parameters_sync(
            &store,
            |_| true, // All entities exist
            OrphanCleanupConfig::default(),
        )
        .unwrap();

        assert_eq!(result.total_scoped_params, 0);
        assert_eq!(result.orphans_found, 0);
    }

    #[test]
    fn test_all_entities_exist() {
        let store = InMemoryParameterStore::new();
        let coop_id = EntityId::cooperative("test-coop").unwrap();

        // Create global param that allows overrides
        store
            .set(test_param_with_override("test.scoped", 100), None, None)
            .unwrap();

        // Create scoped override
        let scoped = test_param("test.scoped", 200).with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(scoped, None, None).unwrap();

        // All entities exist
        let result =
            cleanup_orphan_parameters_sync(&store, |_| true, OrphanCleanupConfig::default())
                .unwrap();

        assert_eq!(result.total_scoped_params, 1);
        assert_eq!(result.orphans_found, 0);
        assert_eq!(result.orphans_deleted, 0);
    }

    #[test]
    fn test_detect_orphans() {
        let store = InMemoryParameterStore::new();
        let coop_id = EntityId::cooperative("deleted-coop").unwrap();
        let fed_id = EntityId::federation("deleted-fed").unwrap();

        // Create global param that allows overrides
        store
            .set(test_param_with_override("test.orphan", 100), None, None)
            .unwrap();

        // Create scoped overrides
        let coop_scoped = test_param("test.orphan", 200).with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(coop_scoped, None, None).unwrap();

        let fed_scoped = test_param("test.orphan", 300)
            .with_scope(ParameterScope::Federation { id: fed_id.clone() });
        store.set(fed_scoped, None, None).unwrap();

        // Dry run - no deletions
        let result = cleanup_orphan_parameters_sync(
            &store,
            |_| false, // No entities exist
            OrphanCleanupConfig::dry_run(),
        )
        .unwrap();

        assert_eq!(result.total_scoped_params, 2);
        assert_eq!(result.orphans_found, 2);
        assert_eq!(result.orphans_deleted, 0);
        assert!(result.dry_run);
    }

    #[test]
    fn test_delete_orphans() {
        let store = InMemoryParameterStore::new();
        let coop_id = EntityId::cooperative("deleted-coop").unwrap();

        // Create global param that allows overrides
        store
            .set(test_param_with_override("test.orphan", 100), None, None)
            .unwrap();

        // Create scoped override
        let scoped = test_param("test.orphan", 200).with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(scoped, None, None).unwrap();

        // Verify it exists
        assert_eq!(store.list_scoped_parameters().unwrap().len(), 1);

        // Delete orphans
        let result = cleanup_orphan_parameters_sync(
            &store,
            |_| false, // No entities exist
            OrphanCleanupConfig::default(),
        )
        .unwrap();

        assert_eq!(result.orphans_found, 1);
        assert_eq!(result.orphans_deleted, 1);
        assert_eq!(result.deletion_failures, 0);
        assert!(!result.dry_run);

        // Verify it's deleted
        assert_eq!(store.list_scoped_parameters().unwrap().len(), 0);
    }

    #[test]
    fn test_partial_orphans() {
        let store = InMemoryParameterStore::new();
        let existing_coop = EntityId::cooperative("existing-coop").unwrap();
        let deleted_coop = EntityId::cooperative("deleted-coop").unwrap();

        let existing_entities: HashSet<String> =
            [existing_coop.as_str().to_string()].into_iter().collect();

        // Create global param
        store
            .set(test_param_with_override("test.partial", 100), None, None)
            .unwrap();

        // Create two scoped overrides
        let existing_scoped =
            test_param("test.partial", 200).with_scope(ParameterScope::Cooperative {
                id: existing_coop.clone(),
            });
        store.set(existing_scoped, None, None).unwrap();

        let orphan_scoped =
            test_param("test.partial", 300).with_scope(ParameterScope::Cooperative {
                id: deleted_coop.clone(),
            });
        store.set(orphan_scoped, None, None).unwrap();

        assert_eq!(store.list_scoped_parameters().unwrap().len(), 2);

        // Delete only the orphan
        let result = cleanup_orphan_parameters_sync(
            &store,
            |id| existing_entities.contains(id.as_str()),
            OrphanCleanupConfig::default(),
        )
        .unwrap();

        assert_eq!(result.total_scoped_params, 2);
        assert_eq!(result.orphans_found, 1);
        assert_eq!(result.orphans_deleted, 1);

        // Verify only the existing one remains
        let remaining = store.list_scoped_parameters().unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(
            matches!(&remaining[0].scope, ParameterScope::Cooperative { id } if id == &existing_coop)
        );
    }

    #[test]
    fn test_max_deletions_limit() {
        let store = InMemoryParameterStore::new();

        // Create global param
        store
            .set(test_param_with_override("test.limit", 100), None, None)
            .unwrap();

        // Create 5 orphan scoped overrides
        for i in 0..5 {
            let coop_id = EntityId::cooperative(&format!("deleted-coop-{i}")).unwrap();
            let scoped = test_param("test.limit", (i + 1) * 100)
                .with_scope(ParameterScope::Cooperative { id: coop_id });
            store.set(scoped, None, None).unwrap();
        }

        assert_eq!(store.list_scoped_parameters().unwrap().len(), 5);

        // Limit to 2 deletions
        let result = cleanup_orphan_parameters_sync(
            &store,
            |_| false,
            OrphanCleanupConfig {
                delete_orphans: true,
                max_deletions: 2,
            },
        )
        .unwrap();

        assert_eq!(result.orphans_found, 5);
        assert_eq!(result.orphans_deleted, 2);

        // Verify 3 remain
        assert_eq!(store.list_scoped_parameters().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn test_async_cleanup() {
        let store = InMemoryParameterStore::new();
        let coop_id = EntityId::cooperative("deleted-coop").unwrap();

        store
            .set(test_param_with_override("test.async", 100), None, None)
            .unwrap();

        let scoped = test_param("test.async", 200).with_scope(ParameterScope::Cooperative {
            id: coop_id.clone(),
        });
        store.set(scoped, None, None).unwrap();

        let result =
            cleanup_orphan_parameters(&store, |_| async { false }, OrphanCleanupConfig::default())
                .await
                .unwrap();

        assert_eq!(result.orphans_found, 1);
        assert_eq!(result.orphans_deleted, 1);
    }

    #[tokio::test]
    async fn test_count_orphans() {
        let store = InMemoryParameterStore::new();

        store
            .set(test_param_with_override("test.count", 100), None, None)
            .unwrap();

        for i in 0..3 {
            let coop_id = EntityId::cooperative(&format!("deleted-coop-{i}")).unwrap();
            let scoped = test_param("test.count", (i + 1) * 100)
                .with_scope(ParameterScope::Cooperative { id: coop_id });
            store.set(scoped, None, None).unwrap();
        }

        let count = count_orphan_parameters(&store, |_| async { false })
            .await
            .unwrap();
        assert_eq!(count, 3);

        // Verify nothing was deleted
        assert_eq!(store.list_scoped_parameters().unwrap().len(), 3);
    }
}
