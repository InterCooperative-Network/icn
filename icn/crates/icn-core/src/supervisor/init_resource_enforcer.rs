//! Resource Access Enforcer Actor initialization
//!
//! Spawns the ResourceAccessEnforcerActor with storage backend.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::resource_enforcer_actor::{
    ResourceAccessEnforcerActor, ResourceAccessStore, ResourceEnforcerConfig,
    ResourceEnforcerHandle, RevocationEvent,
};
use crate::runtime::ShutdownTx;

/// Spawn the resource access enforcer actor
///
/// # Arguments
/// * `config` - Enforcement configuration
/// * `store` - Storage backend for ResourceAccess entries
/// * `shutdown_tx` - Shutdown signal transmitter
///
/// # Returns
/// Handle for interacting with the enforcer actor
pub fn spawn_resource_enforcer(
    config: &ResourceEnforcerConfig,
    store: Arc<RwLock<dyn ResourceAccessStore>>,
    shutdown_tx: &ShutdownTx,
) -> Result<ResourceEnforcerHandle> {
    info!(
        "Spawning ResourceAccessEnforcerActor (enabled={}, interval={}s)",
        config.enabled, config.check_interval_seconds
    );

    let shutdown_rx = shutdown_tx.subscribe();
    let handle = ResourceAccessEnforcerActor::spawn(config.clone(), store, shutdown_rx);

    Ok(handle)
}

/// Null storage implementation for when no persistent storage is available
///
/// This is used as a placeholder when the ledger or storage backend
/// is not yet implemented or not available.
pub struct NullResourceAccessStore;

impl ResourceAccessStore for NullResourceAccessStore {
    fn list_all(&self) -> Result<Vec<(String, icn_ledger::ResourceAccess)>> {
        // Return empty list - no resources to check
        Ok(Vec::new())
    }

    fn update(&mut self, _resource_id: &str, _access: &icn_ledger::ResourceAccess) -> Result<()> {
        // No-op: nothing to persist
        Ok(())
    }

    fn emit_revocation(&self, _event: RevocationEvent) -> Result<()> {
        // No-op: no event bus to emit to
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::EntityId;
    use icn_identity::KeyPair;
    use icn_ledger::{AccessModel, ResourceAccess};
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct TestStore {
        resources: Mutex<HashMap<String, ResourceAccess>>,
    }

    impl TestStore {
        fn new() -> Self {
            Self {
                resources: Mutex::new(HashMap::new()),
            }
        }

        fn add(&self, id: String, access: ResourceAccess) {
            self.resources
                .lock()
                .map(|mut r| r.insert(id, access))
                .ok();
        }
    }

    impl ResourceAccessStore for TestStore {
        fn list_all(&self) -> Result<Vec<(String, ResourceAccess)>> {
            let resources = self
                .resources
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            Ok(resources.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }

        fn update(&mut self, resource_id: &str, access: &ResourceAccess) -> Result<()> {
            let mut resources = self
                .resources
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            resources.insert(resource_id.to_string(), access.clone());
            Ok(())
        }

        fn emit_revocation(&self, _event: RevocationEvent) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_spawn_resource_enforcer() {
        let config = ResourceEnforcerConfig {
            check_interval_seconds: 3600,
            batch_size: 100,
            enabled: true,
        };

        let store = Arc::new(RwLock::new(TestStore::new()));
        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel(1);

        let handle = spawn_resource_enforcer(&config, store.clone(), &shutdown_tx)
            .expect("Failed to spawn enforcer");

        // Get stats to verify actor is running
        let stats = handle.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.checks_performed, 0);
        assert_eq!(stats.total_revocations, 0);

        // Signal shutdown
        let _ = shutdown_tx.send(());

        // Give actor time to shut down
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    #[test]
    fn test_null_store() {
        let store = NullResourceAccessStore;
        let resources = store.list_all().unwrap();
        assert!(resources.is_empty());

        let entity = EntityId::from_did(KeyPair::generate().unwrap().did());
        let access = ResourceAccess::new(
            "test".to_string(),
            entity,
            AccessModel::UseAccess {
                duration_seconds: 3600,
                renewable: false,
                max_accumulated: 1,
            },
        );

        let mut store = NullResourceAccessStore;
        assert!(store.update("test", &access).is_ok());
        assert!(store
            .emit_revocation(RevocationEvent {
                resource_id: "test".to_string(),
                holder: access.holder,
                reason: "test".to_string(),
                timestamp: 0,
                idle_seconds: 0,
            })
            .is_ok());
    }
}
