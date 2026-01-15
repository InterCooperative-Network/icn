//! Federation services initialization
//!
//! Creates federation components for inter-cooperative communication:
//! - CooperativeRegistry for tracking known cooperatives
//! - ClearingManager for bilateral clearing agreements
//! - AttestationStore for trust attestations
//! - FederationGossipHandler for gossip-based federation messages

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::info;

use icn_gossip::GossipActor;
use icn_identity::{Did, KeyPair};
use icn_store::SledStore;

use crate::config::FederationConfig;

/// Services initialized during federation setup
pub struct FederationServices {
    /// The federation gossip handler for processing federation messages
    pub federation_handler: Arc<icn_federation::FederationGossipHandler>,
    /// The cooperative registry for tracking known cooperatives
    pub registry: Arc<icn_federation::CooperativeRegistry>,
    /// The clearing manager for bilateral clearing agreements
    pub clearing_manager: Arc<icn_federation::ClearingManager>,
    /// The attestation store for trust attestations
    pub attestation_store: Arc<icn_federation::AttestationStore>,
    /// The agreement manager for inter-cooperative agreements (persistent storage)
    pub agreement_manager:
        Arc<icn_federation::agreement::AgreementManager<icn_federation::agreement::AgreementStore>>,
}

/// Dependencies for federation initialization
pub struct FederationDeps {
    /// Gossip handle for sending federation messages
    pub gossip_handle: Arc<RwLock<GossipActor>>,
    /// Node's DID
    pub did: Did,
    /// Node's keypair for signing operations
    pub keypair: Arc<KeyPair>,
    /// Base store path for federation data
    pub store_path: std::path::PathBuf,
}

/// Initialize federation services
///
/// Creates all federation components including:
/// - Own cooperative info derived from config
/// - CooperativeRegistry with persistent storage
/// - ClearingManager for bilateral agreements
/// - AttestationStore for trust attestations
/// - FederationGossipHandler with gossip send callback
///
/// Returns None if federation is disabled in config.
pub async fn init_federation_services(
    config: &FederationConfig,
    deps: FederationDeps,
) -> Result<Option<FederationServices>> {
    if !config.enabled {
        return Ok(None);
    }

    // Derive coop_id and coop_name from config or defaults
    let coop_id = if config.coop_id.is_empty() {
        config.network_name.clone()
    } else {
        config.coop_id.clone()
    };

    let coop_name = if config.coop_name.is_empty() {
        config.network_name.clone()
    } else {
        config.coop_name.clone()
    };

    // Create own cooperative info
    let own_coop_info = icn_federation::CooperativeInfo::new(
        coop_id.clone(),
        coop_name.clone(),
        deps.did.clone(),
        icn_federation::FederationPolicy::default(),
    );

    // Create federation store
    let federation_store_path = deps.store_path.join("federation");
    let federation_store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&federation_store_path)?);

    // Create cooperative registry
    let registry = Arc::new(
        icn_federation::CooperativeRegistry::new(federation_store, own_coop_info.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create federation registry: {e}"))?,
    );

    // Create clearing manager
    let clearing_store_path = deps.store_path.join("clearing");
    let clearing_store: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&clearing_store_path)?);
    let clearing_manager = Arc::new(
        icn_federation::ClearingManager::new(clearing_store, coop_id.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create clearing manager: {e}"))?,
    );
    info!(
        "✓ Clearing manager initialized: store={}",
        clearing_store_path.display()
    );

    // Create attestation store
    let attestation_store_path = deps.store_path.join("attestations");
    let attestation_store_backend: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&attestation_store_path)?);
    let attestation_store = Arc::new(icn_federation::AttestationStore::new(
        attestation_store_backend,
    ));
    info!(
        "✓ Attestation store initialized: store={}",
        attestation_store_path.display()
    );

    // Create federation gossip handler
    let federation_handler = Arc::new(icn_federation::FederationGossipHandler::new(
        registry.clone(),
    ));

    // Set own coop info on handler
    federation_handler.set_own_coop(own_coop_info);

    // Set up send callback for federation messages
    let gossip_for_federation = deps.gossip_handle.clone();
    let federation_send_callback: icn_federation::gossip::GossipSendCallback =
        Arc::new(move |topic: &str, data: Vec<u8>| {
            let gossip = gossip_for_federation.clone();
            let topic_owned = topic.to_string();
            // Use a sync approach since we're in a sync callback
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let mut gossip = gossip.write().await;
                    gossip
                        .publish(&topic_owned, data)
                        .await
                        .map(|_| ())
                        .map_err(|e| {
                            icn_federation::FederationError::GossipPublishFailed(e.to_string())
                        })
                })
            })
        });
    federation_handler.set_send_callback(federation_send_callback);

    // Create agreement manager with persistent Sled storage
    let agreement_store_path = deps.store_path.join("agreements");
    let agreement_store_backend: Arc<dyn icn_store::Store> =
        Arc::new(SledStore::open(&agreement_store_path)?);
    let agreement_store = Arc::new(icn_federation::agreement::AgreementStore::new(
        agreement_store_backend,
    ));
    let agreement_manager = Arc::new(
        icn_federation::agreement::AgreementManager::new(
            agreement_store.clone(),
            coop_id.clone(),
            deps.did.clone(),
        )
        .with_keypair(deps.keypair.clone()),
    );
    info!(
        "✓ Agreement manager initialized: store={}",
        agreement_store_path.display()
    );

    info!(
        "✓ Federation enabled: coop_id={}, coop_name={}, store={}",
        coop_id,
        coop_name,
        federation_store_path.display()
    );

    Ok(Some(FederationServices {
        federation_handler,
        registry,
        clearing_manager,
        attestation_store,
        agreement_manager,
    }))
}

/// Spawn the periodic federation announcement task
///
/// Announces our cooperative presence on the federation topics every 5 minutes.
pub fn spawn_federation_announcement_task(
    handler: Arc<icn_federation::FederationGossipHandler>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    background_tasks: &mut tokio::task::JoinSet<()>,
) {
    // Announce immediately on startup
    if let Err(e) = handler.announce() {
        tracing::warn!("Failed to send initial federation announcement: {}", e);
    } else {
        info!("✓ Sent initial federation announcement");
    }

    // Spawn periodic announcement task
    let handler_for_task = handler.clone();
    background_tasks.spawn(async move {
        let mut interval = tokio::time::interval(icn_federation::defaults::ANNOUNCEMENT_INTERVAL);
        interval.tick().await; // Skip first tick (already announced above)

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = handler_for_task.announce() {
                        tracing::warn!("Failed to send periodic federation announcement: {}", e);
                    } else {
                        tracing::debug!("Sent periodic federation announcement");
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Federation announcement task shutting down");
                    break;
                }
            }
        }
    });

    info!(
        "Federation announcement task spawned (interval: {:?})",
        icn_federation::defaults::ANNOUNCEMENT_INTERVAL
    );
}
