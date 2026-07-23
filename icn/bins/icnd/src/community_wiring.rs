//! Community subsystem construction — daemon composition root wiring.
//!
//! This factory function bridges the community actor (kernel-transported
//! handle) with `icn-community` (domain crate). It lives here in the daemon
//! binary so `icn-core` (kernel) never imports `icn-community`.
//!
//! Unlike the ledger-backed compute callbacks in `compute_wiring.rs` (which
//! close over resources already available at daemon-startup time),
//! community construction needs the LIVE gossip actor, which only exists
//! partway through the supervisor's actor-initialization sequence. The
//! factory built here therefore captures nothing live — it is handed to the
//! kernel via `BootstrapHandles::community_factory` and invoked by the
//! supervisor once gossip is up, receiving the inputs it needs at that
//! point (mirrors `effect_subscription_factory`'s timing pattern).
//!
//! The resulting handle is type-erased (`Box<dyn Any + Send + Sync>`) before
//! being returned to the kernel, which transports it onward to the gateway
//! without interpretation. The meaning firewall stays intact.

use icn_community::{CommunityActor, CommunityHandle, CommunityStore};
use icn_core::supervisor::actors::{CommunityFactory, CommunityFactoryInputs};
use std::any::Any;
use std::sync::Arc;
use tracing::{info, warn};

/// Build the community-domain factory registered on `BootstrapHandles`.
pub fn create_community_factory() -> CommunityFactory {
    Box::new(|inputs: CommunityFactoryInputs| {
        Box::pin(async move { init_community_services(inputs).await })
    })
}

/// Construct the community store, actor, gossip subscription, and
/// notification callback.
///
/// Behavior preserved verbatim from the former `icn-core`
/// `init_community::init_community_services` (moved here as part of
/// migration B0): same store subdirectory, same two-store split (one for the
/// actor, one returned for gateway use), same best-effort subscribe with an
/// identical warning on failure, same actor-spawn arguments. The only
/// addition is registering the gossip-update notification callback here
/// (previously wired separately, at a later point, inside `icn-core`'s
/// monolithic dispatcher) via the same multi-subscriber
/// `add_notification_callback` seam governance and steward already use
/// independently.
async fn init_community_services(
    inputs: CommunityFactoryInputs,
) -> anyhow::Result<Box<dyn Any + Send + Sync>> {
    info!("Initializing community services (civic engine)");

    // Create community store in dedicated subdirectory
    let store_path = inputs.store_path.join("community");
    let sled_store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::open(&store_path)?);

    // Create CommunityStore instances that share the same underlying SledStore
    // One for the actor, one for the gateway (both reference the same data)
    let store_for_actor = CommunityStore::new(Arc::clone(&sled_store));
    let community_store = Arc::new(CommunityStore::new(sled_store));

    info!("Community store initialized at {:?}", store_path);

    // Subscribe to gossip topic for distributed community updates
    // This allows communities created on one node to sync to others
    // Note: incoming updates are processed by the notification callback
    // registered below (last-write-wins merge strategy).
    {
        let mut gossip = inputs.gossip.write().await;
        if let Err(e) = gossip
            .subscribe(icn_community::COMMUNITY_TOPIC, inputs.node_did.clone())
            .await
        {
            warn!("Failed to subscribe to community:updates topic: {}", e);
        } else {
            info!("Subscribed to community:updates topic");
        }
        gossip.add_notification_callback(icn_community::gossip_update_callback(
            community_store.clone(),
        ));
    }

    // Spawn CommunityActor with store and gossip handle for distributed sync
    let tx = CommunityActor::spawn(store_for_actor, Some(inputs.gossip.clone()));
    let community_handle = CommunityHandle::new(tx);

    info!("✓ Community actor spawned (civic engine)");

    Ok(Box::new(community_handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_community::{Community, CommunityType};
    use icn_gossip::GossipActor;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// End-to-end factory test against a real in-process `GossipActor`.
    ///
    /// `GossipActor::store_entry` only fans out to registered notification
    /// callbacks for topics that have at least one active subscriber
    /// (`icn-gossip/src/gossip.rs`, the `notification_callbacks` loop is
    /// gated on `self.subscriptions.get(topic)`), so this test pre-creates
    /// the topic (test-only setup — production has no such call, a
    /// pre-existing, out-of-B0-scope gap documented in the design record)
    /// to let the factory's internal `subscribe` succeed, then exercises
    /// the REAL path: factory construction -> downcast -> publish a gossip
    /// entry -> notification callback fires -> decode+merge -> readable via
    /// the returned `CommunityHandle`. This proves the moved composition
    /// glue actually works, not just that it type-checks.
    #[tokio::test]
    async fn factory_constructs_and_syncs_gossip_update() -> anyhow::Result<()> {
        let tempdir = tempfile::tempdir()?;
        let keypair = icn_identity::KeyPair::generate()?;
        let node_did = keypair.did().clone();

        let gossip = Arc::new(RwLock::new(GossipActor::new(node_did.clone(), None)));
        {
            let mut g = gossip.write().await;
            g.create_topic(icn_gossip::Topic::new(
                icn_community::COMMUNITY_TOPIC.to_string(),
                icn_gossip::AccessControl::Public,
            ));
        }

        let factory = create_community_factory();
        let result = factory(CommunityFactoryInputs {
            gossip: gossip.clone(),
            node_did,
            store_path: tempdir.path().to_path_buf(),
        })
        .await?;

        let handle = match result.downcast::<CommunityHandle>() {
            Ok(h) => *h,
            Err(_) => anyhow::bail!("factory output must downcast to CommunityHandle"),
        };

        let community = Community::new(
            "test-community-1".to_string(),
            "Test Community".to_string(),
            CommunityType::Geographic,
            "test-domain".to_string(),
        );
        let encoded = icn_encoding::encode(&community)?;

        gossip
            .write()
            .await
            .publish(icn_community::COMMUNITY_TOPIC, encoded)
            .await?;

        // Notification callback spawns the merge asynchronously; give it a
        // bounded window to complete (matches the sleep-after-spawn pattern
        // used elsewhere in this workspace's integration tests).
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let synced = handle.get("test-community-1".to_string()).await?;
        assert_eq!(synced.id, "test-community-1");
        assert_eq!(synced.name, "Test Community");

        Ok(())
    }
}
