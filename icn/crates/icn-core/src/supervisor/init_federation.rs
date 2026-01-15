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

    // Set up send callback for agreement gossip (reuse federation callback pattern)
    let gossip_for_agreements = deps.gossip_handle.clone();
    let agreement_send_callback: icn_federation::agreement::AgreementGossipCallback =
        Arc::new(move |topic: &str, data: Vec<u8>| {
            let gossip = gossip_for_agreements.clone();
            let topic_owned = topic.to_string();
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

    // Create agreement gossip handler with send callback
    // Note: set_send_callback requires &mut self, so we configure before wrapping in Arc
    let mut agreement_gossip_handler = icn_federation::agreement::AgreementGossipHandler::new(
        agreement_store.clone(),
        deps.did.clone(),
        coop_id.clone(),
    );
    agreement_gossip_handler.set_send_callback(agreement_send_callback);
    let agreement_gossip_handler = Arc::new(agreement_gossip_handler);

    // Register agreement handler with federation handler for message routing
    federation_handler.register_agreement_handler(agreement_gossip_handler.clone());
    info!("✓ Agreement gossip handler registered");

    // Create agreement manager with event callback to broadcast to gossip
    // The callback fetches full objects from the store since events only contain IDs
    let gossip_handler_for_events = agreement_gossip_handler.clone();
    let store_for_events = agreement_store.clone();
    let agreement_event_callback: icn_federation::agreement::AgreementEventCallback =
        Box::new(move |event| {
            use icn_federation::agreement::{AgreementEvent, AgreementStoreOps};
            match &event {
                AgreementEvent::Proposed { agreement_id, .. } => {
                    // Fetch full agreement from store and broadcast
                    match store_for_events.get_agreement(agreement_id) {
                        Ok(Some(agreement)) => {
                            if let Err(e) = gossip_handler_for_events.broadcast_proposed(&agreement)
                            {
                                tracing::warn!("Failed to broadcast proposed agreement: {}", e);
                            }
                        }
                        Ok(None) => {
                            tracing::warn!("Agreement {} not found for broadcast", agreement_id);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to fetch agreement for broadcast: {}", e);
                        }
                    }
                }
                AgreementEvent::Signed {
                    agreement_id,
                    signer,
                    ..
                } => {
                    // Fetch agreement to get the signature
                    match store_for_events.get_agreement(agreement_id) {
                        Ok(Some(agreement)) => {
                            if let Some(sig) =
                                agreement.signatures.iter().find(|s| &s.signer == signer)
                            {
                                if let Err(e) = gossip_handler_for_events
                                    .broadcast_signature(agreement_id, sig.clone())
                                {
                                    tracing::warn!("Failed to broadcast signature: {}", e);
                                }
                            }
                        }
                        Ok(None) => {
                            tracing::warn!(
                                "Agreement {} not found for signature broadcast",
                                agreement_id
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to fetch agreement for signature broadcast: {}",
                                e
                            );
                        }
                    }
                }
                AgreementEvent::Suspended { agreement_id, .. }
                | AgreementEvent::Resumed { agreement_id, .. }
                | AgreementEvent::Terminated { agreement_id, .. }
                | AgreementEvent::Activated { agreement_id, .. } => {
                    // Fetch agreement to get current status
                    match store_for_events.get_agreement(agreement_id) {
                        Ok(Some(agreement)) => {
                            if let Err(e) = gossip_handler_for_events
                                .broadcast_status_update(agreement_id, agreement.status.clone())
                            {
                                tracing::warn!("Failed to broadcast status update: {}", e);
                            }
                        }
                        Ok(None) => {
                            tracing::warn!(
                                "Agreement {} not found for status broadcast",
                                agreement_id
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Failed to fetch agreement for status broadcast: {}", e);
                        }
                    }
                }
                AgreementEvent::AmendmentProposed {
                    agreement_id,
                    amendment_id,
                    ..
                } => {
                    // Fetch amendment from store and broadcast
                    match store_for_events.get_amendments(agreement_id) {
                        Ok(amendments) => {
                            if let Some(amendment) =
                                amendments.iter().find(|a| &a.id == amendment_id)
                            {
                                if let Err(e) =
                                    gossip_handler_for_events.broadcast_amendment(amendment)
                                {
                                    tracing::warn!("Failed to broadcast amendment: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to fetch amendments for broadcast: {}", e);
                        }
                    }
                }
                // Other events don't need gossip broadcast
                _ => {}
            }
        });

    let agreement_manager = Arc::new(
        icn_federation::agreement::AgreementManager::new(
            agreement_store.clone(),
            coop_id.clone(),
            deps.did.clone(),
        )
        .with_keypair(deps.keypair.clone())
        .with_event_callback(agreement_event_callback),
    );
    info!(
        "✓ Agreement manager initialized with gossip sync: store={}",
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
