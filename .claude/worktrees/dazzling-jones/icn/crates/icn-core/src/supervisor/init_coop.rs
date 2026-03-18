//! Cooperative actor initialization

use anyhow::Result;
use icn_coop::{CoopActor, CoopHandle, CoopStore, TreasuryManagerHandle};
use icn_gossip::GossipActor;
use icn_identity::Did;
use icn_store::SledStore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::Config;

/// Topic for cooperative state updates
pub const COOP_UPDATES_TOPIC: &str = "coop:updates";

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
/// * `node_did` - The DID of this node for gossip subscriptions
/// * `treasury_manager` - Optional treasury manager for ledger integration
///
/// # Returns
/// CoopServices containing handle and store references
pub async fn init_coop_services(
    config: &Config,
    gossip_handle: Arc<RwLock<GossipActor>>,
    node_did: Did,
) -> Result<CoopServices> {
    init_coop_services_with_treasury(config, gossip_handle, node_did, None).await
}

/// Initialize cooperative services with optional treasury manager
///
/// This variant allows wiring the CoopActor to the ledger's TreasuryManager,
/// enabling automatic treasury account creation when cooperatives create treasuries.
pub async fn init_coop_services_with_treasury(
    config: &Config,
    gossip_handle: Arc<RwLock<GossipActor>>,
    node_did: Did,
    treasury_manager: Option<TreasuryManagerHandle>,
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
    {
        let mut gossip = gossip_handle.write().await;
        if let Err(e) = gossip.subscribe(COOP_UPDATES_TOPIC, node_did).await {
            warn!("Failed to subscribe to coop:updates topic: {}", e);
        } else {
            info!("Subscribed to coop:updates topic");
        }
    }

    // Spawn CoopActor with store, gossip handle, and optional treasury manager
    let tx = CoopActor::spawn_with_treasury(coop_store, Some(gossip_handle), treasury_manager);
    let coop_handle = CoopHandle::new(tx);

    info!("✓ Cooperative actor spawned");

    Ok(CoopServices {
        coop_handle,
        coop_store: coop_store_for_gateway,
    })
}
