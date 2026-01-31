//! Resource Access Enforcer Actor initialization
//!
//! Spawns the ResourceAccessEnforcerActor with LedgerService and gossip integration.

use anyhow::Result;
use tracing::info;

use crate::resource_enforcer_actor::{
    ResourceAccessEnforcerActor, ResourceEnforcerConfig, ResourceEnforcerDeps,
    ResourceEnforcerHandle,
};
use crate::runtime::ShutdownTx;

/// Spawn the resource access enforcer actor
///
/// # Arguments
/// * `config` - Enforcement configuration
/// * `deps` - Dependencies (LedgerService + optional gossip)
/// * `shutdown_tx` - Shutdown signal transmitter
///
/// # Returns
/// Handle for interacting with the enforcer actor
pub fn spawn_resource_enforcer(
    config: &ResourceEnforcerConfig,
    deps: ResourceEnforcerDeps,
    shutdown_tx: &ShutdownTx,
) -> Result<ResourceEnforcerHandle> {
    info!(
        "Spawning ResourceAccessEnforcerActor (enabled={}, interval={}s)",
        config.enabled, config.check_interval_seconds
    );

    let shutdown_rx = shutdown_tx.subscribe();
    let handle = ResourceAccessEnforcerActor::spawn(config.clone(), deps, shutdown_rx);

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::services::LedgerService;
    use std::sync::Arc;

    /// Minimal LedgerService for testing the enforcer spawning
    struct StubLedgerService;

    impl LedgerService for StubLedgerService {
        fn oracle(&self) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
            unimplemented!("not needed for enforcer tests")
        }

        fn balance(&self, _account: &icn_kernel_api::types::Did, _currency: &str) -> i64 {
            0
        }

        fn credit_limit(&self, _account: &icn_kernel_api::types::Did, _currency: &str) -> i64 {
            0
        }

        fn record_event(&self, _event: icn_kernel_api::services::LedgerEvent) {}
    }

    #[tokio::test]
    async fn test_spawn_resource_enforcer() {
        // Use small interval so startup jitter is 0 (jitter = interval/10)
        let config = ResourceEnforcerConfig {
            check_interval_seconds: 1,
            batch_size: 100,
            enabled: true,
        };

        let deps = ResourceEnforcerDeps {
            ledger_service: Arc::new(StubLedgerService),
            gossip_handle: None,
        };

        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel(1);

        let handle =
            spawn_resource_enforcer(&config, deps, &shutdown_tx).expect("Failed to spawn enforcer");

        // Get stats to verify actor is running
        let stats = handle.get_stats().await.expect("Failed to get stats");
        assert_eq!(stats.checks_performed, 0);
        assert_eq!(stats.total_revocations, 0);

        // Signal shutdown
        let _ = shutdown_tx.send(());

        // Give actor time to shut down
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}
