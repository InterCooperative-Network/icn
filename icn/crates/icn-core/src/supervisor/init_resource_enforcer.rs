//! Resource Access Enforcer Actor initialization
//!
//! Spawns the ResourceAccessEnforcerActor with storage backend and gossip integration.

use anyhow::Result;
use icn_gossip::GossipActor;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::resource_enforcer_actor::{
    ResourceAccessEnforcerActor, ResourceAccessStore, ResourceEnforcerConfig,
    ResourceEnforcerHandle, RevocationEvent, RESOURCE_REVOCATIONS_TOPIC,
};
use crate::runtime::ShutdownTx;

/// Spawn the resource access enforcer actor
///
/// # Arguments
/// * `config` - Enforcement configuration
/// * `store` - Storage backend for ResourceAccess entries (already wrapped with gossip if needed)
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

    fn emit_revocation(&mut self, _event: RevocationEvent) -> Result<()> {
        // No-op: no event bus to emit to
        Ok(())
    }
}

/// Gossip-aware resource access store wrapper
///
/// This wraps an underlying storage implementation and publishes
/// revocation events to a gossip topic for cluster-wide notification.
///
/// # Visibility
///
/// This type is public for use in integration tests and internal supervisor
/// wiring, but is not re-exported from `icn-core`'s root. External crates
/// should not depend on this type directly; it is an internal implementation
/// detail of the resource enforcer subsystem.
///
/// # Example (internal testing)
///
/// ```ignore
/// use icn_core::supervisor::init_resource_enforcer::GossipResourceAccessStore;
/// let store = GossipResourceAccessStore::new(inner, gossip_handle);
/// ```
pub struct GossipResourceAccessStore {
    /// Underlying storage implementation
    inner: Box<dyn ResourceAccessStore>,
    /// Gossip actor handle for publishing revocations
    gossip_handle: Arc<RwLock<GossipActor>>,
}

impl GossipResourceAccessStore {
    /// Create a new gossip-aware resource access store
    pub fn new(
        inner: Box<dyn ResourceAccessStore>,
        gossip_handle: Arc<RwLock<GossipActor>>,
    ) -> Self {
        Self {
            inner,
            gossip_handle,
        }
    }
}

impl ResourceAccessStore for GossipResourceAccessStore {
    fn list_all(&self) -> Result<Vec<(String, icn_ledger::ResourceAccess)>> {
        self.inner.list_all()
    }

    fn update(&mut self, resource_id: &str, access: &icn_ledger::ResourceAccess) -> Result<()> {
        self.inner.update(resource_id, access)
    }

    fn emit_revocation(&mut self, event: RevocationEvent) -> Result<()> {
        // Clone once for the async gossip task before passing ownership to inner store
        let event_for_gossip = event.clone();

        // First emit through the inner store (for local audit trail)
        self.inner.emit_revocation(event)?;

        // Then publish to gossip for cluster-wide notification
        let gossip_handle = self.gossip_handle.clone();

        // Spawn async task to publish to gossip (don't block the enforcer)
        // Note: Revocation gossip publication is best-effort. If gossip is unavailable,
        // the event may not reach other cluster nodes until the next enforcement cycle.
        tokio::spawn(async move {
            let serialized = match serde_json::to_vec(&event_for_gossip) {
                Ok(data) => data,
                Err(e) => {
                    warn!("Failed to serialize revocation event: {}", e);
                    metrics::counter!("icn_resource_revocation_gossip_failures_total", "reason" => "serialization").increment(1);
                    return;
                }
            };

            let mut gossip = gossip_handle.write().await;
            if let Err(e) = gossip.publish(RESOURCE_REVOCATIONS_TOPIC, serialized).await {
                warn!("Failed to publish revocation to gossip: {}", e);
                metrics::counter!("icn_resource_revocation_gossip_failures_total", "reason" => "publish").increment(1);
            } else {
                info!(
                    resource_id = %event_for_gossip.resource_id,
                    holder = %event_for_gossip.holder,
                    "Published revocation event to gossip"
                );
                metrics::counter!("icn_resource_revocation_gossip_published_total").increment(1);
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_entity::EntityId;
    // use icn_gossip::gossip::TrustLookup;
    type TrustLookup = std::sync::Arc<
        dyn Fn(&icn_identity::Did) -> Option<icn_trust::TrustClass> + Send + Sync + 'static,
    >;
    use icn_identity::KeyPair;
    use icn_ledger::{AccessModel, ResourceAccess};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Wait time for async gossip publication tasks to complete.
    /// 100ms provides sufficient margin for:
    /// - tokio::spawn task scheduling
    /// - Gossip actor lock acquisition
    /// - Serialization and publication
    const ASYNC_PUBLISH_WAIT_MS: u64 = 100;

    struct TestStore {
        resources: Mutex<HashMap<String, ResourceAccess>>,
    }

    impl TestStore {
        fn new() -> Self {
            Self {
                resources: Mutex::new(HashMap::new()),
            }
        }

        #[allow(dead_code)]
        fn add(&self, id: String, access: ResourceAccess) {
            self.resources.lock().map(|mut r| r.insert(id, access)).ok();
        }
    }

    impl ResourceAccessStore for TestStore {
        fn list_all(&self) -> Result<Vec<(String, ResourceAccess)>> {
            let resources = self
                .resources
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            Ok(resources
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect())
        }

        fn update(&mut self, resource_id: &str, access: &ResourceAccess) -> Result<()> {
            let mut resources = self
                .resources
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            resources.insert(resource_id.to_string(), access.clone());
            Ok(())
        }

        fn emit_revocation(&mut self, _event: RevocationEvent) -> Result<()> {
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

    #[tokio::test]
    async fn test_gossip_store_publishes_revocations() {
        use icn_gossip::GossipActor;
        use std::sync::{Arc, Mutex as StdMutex};

        // Create a mock store that tracks revocation events
        let events_log: Arc<StdMutex<Vec<RevocationEvent>>> = Arc::new(StdMutex::new(Vec::new()));

        struct MockStore {
            events: Arc<StdMutex<Vec<RevocationEvent>>>,
        }

        impl ResourceAccessStore for MockStore {
            fn list_all(&self) -> Result<Vec<(String, ResourceAccess)>> {
                Ok(Vec::new())
            }

            fn update(&mut self, _resource_id: &str, _access: &ResourceAccess) -> Result<()> {
                Ok(())
            }

            fn emit_revocation(&mut self, event: RevocationEvent) -> Result<()> {
                self.events.lock().unwrap().push(event);
                Ok(())
            }
        }

        // Create gossip actor
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did();
        let trust_lookup: TrustLookup = Arc::new(move |_did| Some(icn_trust::TrustClass::Known));
        let gossip_handle = GossipActor::spawn_with_trust_graph(did.clone(), trust_lookup, None);

        // Set keypair for signing and create the revocation topic
        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_keypair(keypair);
            gossip.create_topic(icn_gossip::Topic::new(
                RESOURCE_REVOCATIONS_TOPIC.to_string(),
                icn_gossip::AccessControl::Public,
            ));
        }

        // Create gossip-aware store
        let inner_store = Box::new(MockStore {
            events: events_log.clone(),
        });
        let mut gossip_store = GossipResourceAccessStore::new(inner_store, gossip_handle.clone());

        // Create and emit a revocation event
        let entity = EntityId::from_did(KeyPair::generate().unwrap().did());
        let event = RevocationEvent {
            resource_id: "test-resource".to_string(),
            holder: entity,
            reason: "Idle for too long".to_string(),
            timestamp: icn_time::current_timestamp_secs(),
            idle_seconds: 86400, // 1 day
        };

        // Emit the revocation
        gossip_store.emit_revocation(event.clone()).unwrap();

        // Wait for async publication task
        tokio::time::sleep(tokio::time::Duration::from_millis(ASYNC_PUBLISH_WAIT_MS)).await;

        // Verify event was logged
        let logged_events = events_log.lock().unwrap();
        assert_eq!(logged_events.len(), 1);
        assert_eq!(logged_events[0].resource_id, "test-resource");
        assert_eq!(logged_events[0].reason, "Idle for too long");

        // Verify event was published to gossip
        // (We can't easily check this without subscribing, but the log should show it)
    }
}
