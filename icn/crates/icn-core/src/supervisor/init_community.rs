//! Community actor initialization
//!
//! This module initializes the CommunityActor for the civic engine.
//! Communities are non-economic civic organizations that can include
//! both individuals and cooperatives as members.

use anyhow::Result;
use icn_community::{CommunityActor, CommunityHandle, CommunityStore, COMMUNITY_TOPIC};
use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_store::SledStore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::Config;

/// Services provided by community layer
pub struct CommunityServices {
    /// Handle for interacting with community actor
    pub community_handle: CommunityHandle,
    /// Direct access to community store (for potential gateway use)
    pub community_store: Arc<CommunityStore>,
}

/// Initialize community services
///
/// This spawns the CommunityActor which manages community lifecycle,
/// membership, and resources. The actor uses persistent storage and
/// synchronizes state across nodes via gossip.
///
/// # Arguments
/// * `config` - Configuration with storage path
/// * `gossip_handle` - Handle to gossip actor for distributed sync
/// * `node_did` - The DID of this node for gossip subscriptions
///
/// # Returns
/// CommunityServices containing handle and store references
pub async fn init_community_services(
    config: &Config,
    gossip_handle: Arc<RwLock<GossipActor>>,
    node_did: Did,
) -> Result<CommunityServices> {
    info!("Initializing community services (civic engine)");

    // Create community store in dedicated subdirectory
    let store_path = config.store_path().join("community");
    let sled_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&store_path)?);

    // CommunityStore uses the generic Store trait
    let community_store = CommunityStore::new(sled_store.clone());
    let community_store_for_gateway = Arc::new(CommunityStore::new(sled_store));

    info!("Community store initialized at {:?}", store_path);

    // Subscribe to gossip topic for distributed community updates
    // This allows communities created on one node to sync to others
    {
        let mut gossip = gossip_handle.write().await;
        if let Err(e) = gossip.subscribe(COMMUNITY_TOPIC, node_did) {
            warn!("Failed to subscribe to community:updates topic: {}", e);
        } else {
            info!("Subscribed to community:updates topic");
        }
    }

    // Spawn CommunityActor with store and gossip handle for distributed sync
    let tx = CommunityActor::spawn(community_store, Some(gossip_handle));
    let community_handle = CommunityHandle::new(tx);

    info!("✓ Community actor spawned (civic engine)");

    Ok(CommunityServices {
        community_handle,
        community_store: community_store_for_gateway,
    })
}
