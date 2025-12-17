//! Cooperative actor initialization

use anyhow::Result;
use icn_coop::{CoopActor, CoopHandle, CoopStore};
use icn_gossip::GossipActor;
use icn_store::SledStore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::Config;

/// Services provided by cooperative layer
pub struct CoopServices {
    /// Handle for interacting with cooperative actor
    pub coop_handle: CoopHandle,
    /// Direct access to cooperative store (for potential gateway use)
    pub coop_store: Arc<CoopStore>,
}

/// Initialize cooperative services
///
/// This spawns the CoopActor which manages cooperative lifecycle,
/// membership, and state. The actor uses persistent storage via
/// the provided Store and synchronizes state across nodes via gossip.
///
/// # Arguments
/// * `config` - Configuration with storage path
/// * `gossip_handle` - Handle to gossip actor for distributed sync
///
/// # Returns
/// CoopServices containing handle and store references
pub async fn init_coop_services(
    config: &Config,
    _gossip_handle: Arc<RwLock<GossipActor>>, // TODO: Wire up gossip sync
) -> Result<CoopServices> {
    info!("Initializing cooperative services");

    // Create cooperative store in dedicated subdirectory
    let store_path = config.store_path().join("cooperative");
    let sled_store = Arc::new(SledStore::open(&store_path)?);
    
    // CoopStore needs direct Sled Db access
    let db = Arc::new(sled_store.db().clone());
    let coop_store = CoopStore::new(db.clone());
    let coop_store_for_gateway = Arc::new(CoopStore::new(db));
    
    info!("Cooperative store initialized at {:?}", store_path);

    // Subscribe to gossip topic for distributed cooperative updates
    // This allows coops created on one node to sync to others
    // Note: We need a DID for subscription - use node's DID from config or identity
    // For now, skip gossip subscription and add it later when we wire up the notification handler
    // {
    //     let mut gossip = gossip_handle.write().await;
    //     gossip.subscribe("coop:updates", node_did)?;
    // }
    info!("Note: Gossip subscription for coop:updates will be added in next iteration");

    // Spawn CoopActor with store (gossip integration pending)
    let tx = CoopActor::spawn(coop_store, None);
    let coop_handle = CoopHandle::new(tx);

    info!("✓ Cooperative actor spawned");

    Ok(CoopServices {
        coop_handle,
        coop_store: coop_store_for_gateway,
    })
}
