//! Contract Registry actor initialization
//!
//! This module extracts the contract registry actor setup from the supervisor,
//! providing a cleaner separation of concerns for contract management services.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use icn_ccl::{
    ContractRegistry, ContractRegistryActor, ContractRegistryHandle, ContractRegistryMessage,
    TOPIC_CONTRACTS, TOPIC_CONTRACTS_DEPLOY, TOPIC_CONTRACTS_REVOKE,
};
use icn_identity::Did;
use icn_store::SledStore;

use crate::config::Config;

/// Type alias for the gossip handle
pub type GossipHandle = Arc<RwLock<icn_gossip::GossipActor>>;

/// Dependencies required for contract registry initialization
pub struct ContractRegistryDeps {
    /// Handle to the gossip actor for message synchronization
    pub gossip_handle: GossipHandle,
    /// Shutdown signal receiver
    pub shutdown_rx: tokio::sync::broadcast::Receiver<()>,
}

/// Services returned from contract registry initialization
pub struct ContractRegistryServices {
    /// Handle to the contract registry actor
    pub registry_handle: ContractRegistryHandle,
    /// Contract registry store (for potential direct access)
    pub registry_store: Arc<dyn icn_store::Store>,
}

/// Initialize contract registry services
///
/// Creates:
/// - ContractRegistryActor for contract storage and gossip sync
/// - Persistent storage at store_path/contract_registry
/// - Gossip topic subscriptions for contract sync
pub async fn init_contract_registry_services(
    config: &Config,
    did: Did,
    deps: ContractRegistryDeps,
) -> anyhow::Result<ContractRegistryServices> {
    // Create contract registry topics before spawning actor
    {
        let mut gossip = deps.gossip_handle.write().await;

        // Main contracts topic for queries/responses
        gossip.create_topic(icn_gossip::Topic::new(
            TOPIC_CONTRACTS.to_string(),
            icn_gossip::AccessControl::Public,
        ));

        // Deployment announcements topic
        gossip.create_topic(icn_gossip::Topic::new(
            TOPIC_CONTRACTS_DEPLOY.to_string(),
            icn_gossip::AccessControl::Public,
        ));

        // Revocation announcements topic
        gossip.create_topic(icn_gossip::Topic::new(
            TOPIC_CONTRACTS_REVOKE.to_string(),
            icn_gossip::AccessControl::Public,
        ));
    }

    // Open persistent store for contract registry
    let registry_store_path = config.store_path().join("contract_registry");
    let registry_store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&registry_store_path)?);

    // Create the contract registry with persistent storage
    let contract_registry = Arc::new(ContractRegistry::with_store(registry_store.clone()));

    // Create send callback for gossip messages
    let gossip_for_send = deps.gossip_handle.clone();
    let send_callback = Arc::new(move |msg: ContractRegistryMessage| {
        let gossip = gossip_for_send.clone();
        let topic = match &msg {
            ContractRegistryMessage::Deployed { .. } => TOPIC_CONTRACTS_DEPLOY,
            ContractRegistryMessage::Revoked { .. } => TOPIC_CONTRACTS_REVOKE,
            _ => TOPIC_CONTRACTS,
        };

        tokio::spawn(async move {
            let bytes = match msg.to_bytes() {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("Failed to serialize contract registry message: {}", e);
                    return;
                }
            };

            let mut gossip = gossip.write().await;
            if let Err(e) = gossip.publish(topic, bytes) {
                tracing::warn!("Failed to publish contract registry message: {}", e);
            }
        });
    });

    // Create the actor with the contract registry
    let mut actor = ContractRegistryActor::new(did.to_string(), contract_registry);
    actor.set_send_callback(send_callback);

    // Spawn the actor
    let registry_handle = actor.spawn();

    info!(
        "✓ Contract registry actor spawned at {}",
        registry_store_path.display()
    );
    icn_obs::metrics::supervisor::actor_spawned_inc("contract_registry");

    // Subscribe to contract registry topics
    {
        let mut gossip = deps.gossip_handle.write().await;

        // Subscribe to all contract topics
        if let Err(e) = gossip.subscribe(TOPIC_CONTRACTS, did.clone()) {
            tracing::warn!("Failed to subscribe to {}: {}", TOPIC_CONTRACTS, e);
        }

        if let Err(e) = gossip.subscribe(TOPIC_CONTRACTS_DEPLOY, did.clone()) {
            tracing::warn!("Failed to subscribe to {}: {}", TOPIC_CONTRACTS_DEPLOY, e);
        }

        if let Err(e) = gossip.subscribe(TOPIC_CONTRACTS_REVOKE, did.clone()) {
            tracing::warn!("Failed to subscribe to {}: {}", TOPIC_CONTRACTS_REVOKE, e);
        }

        info!("✓ Subscribed to contract registry gossip topics");
    }

    Ok(ContractRegistryServices {
        registry_handle,
        registry_store,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_registry_deps_struct() {
        // Verify ContractRegistryDeps fields are accessible
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GossipHandle>();
    }
}
