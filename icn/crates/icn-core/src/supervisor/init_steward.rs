//! Steward actor initialization
//!
//! This module extracts the steward actor setup from the supervisor,
//! providing a cleaner separation of concerns for SDIS (Steward-Driven Identity System).

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::StewardNodeConfig;
use crate::runtime::ShutdownTx;
use icn_identity::{Did, KeyPair};

/// Type alias for the gossip handle
pub type GossipHandle = Arc<RwLock<icn_gossip::GossipActor>>;

/// Type alias for the steward handle holder
pub type StewardHandleHolder = Arc<RwLock<Option<icn_steward::StewardHandle>>>;

/// Dependencies required for steward initialization
pub struct StewardDeps {
    /// Gossip handle for message routing
    pub gossip_handle: GossipHandle,
    /// Steward handle holder (filled after spawn)
    pub steward_handle_holder: StewardHandleHolder,
    /// This node's DID
    pub did: Did,
    /// Keypair for signing
    pub keypair: KeyPair,
    /// Shutdown signal
    pub shutdown_tx: ShutdownTx,
}

/// Result of steward initialization
pub struct StewardServices {
    /// Handle to the steward actor (if spawned)
    pub steward_handle: Option<icn_steward::StewardHandle>,
}

/// Create the gossip send callback for steward messages
fn create_send_callback(gossip_handle: GossipHandle) -> icn_steward::actor::SendGossipCallback {
    Arc::new(move |steward_msg| {
        let gossip = gossip_handle.clone();

        tokio::spawn(async move {
            // Determine topic based on message type
            let topic = match &steward_msg {
                icn_steward::StewardMessage::Announce(_) => icn_steward::topics::STEWARD_ANNOUNCE,
                icn_steward::StewardMessage::Enrollment(_) => icn_steward::topics::ENROLLMENT,
                icn_steward::StewardMessage::Recovery(_) => icn_steward::topics::RECOVERY,
                icn_steward::StewardMessage::VuiSync(_) => icn_steward::topics::VUI_SYNC,
            };

            // Serialize message
            let data = match bincode::serde::encode_to_vec(&steward_msg, bincode::config::legacy())
            {
                Ok(d) => d,
                Err(e) => {
                    warn!("Failed to serialize steward message: {}", e);
                    return;
                }
            };

            // Publish via gossip
            {
                let mut gossip = gossip.write().await;
                if let Err(e) = gossip.publish(topic, data).await {
                    warn!("Failed to publish steward message: {}", e);
                }
            }
        });
    })
}

/// Subscribe to steward gossip topics
async fn subscribe_steward_topics(gossip_handle: &GossipHandle, did: &Did) {
    let mut gossip = gossip_handle.write().await;
    for topic in &[
        icn_steward::topics::STEWARD_ANNOUNCE,
        icn_steward::topics::VUI_SYNC,
        icn_steward::topics::ENROLLMENT,
        icn_steward::topics::RECOVERY,
    ] {
        if let Err(e) = gossip.subscribe(topic, did.clone()).await {
            warn!("Failed to subscribe to steward topic {}: {}", topic, e);
        } else {
            info!("Subscribed to steward topic: {}", topic);
        }
    }
}

/// Set up notification callback for steward messages
fn setup_steward_notification_callback(
    gossip_handle: &mut icn_gossip::GossipActor,
    steward_handle_holder: StewardHandleHolder,
) {
    gossip_handle.set_notification_callback(Arc::new(move |topic, entry, _subscriber_did| {
        // Only process steward topics
        if !topic.starts_with("steward:") {
            return;
        }

        let steward_holder = steward_handle_holder.clone();
        let data = entry.data.clone();
        let topic_clone = topic.clone();

        tokio::spawn(async move {
            let steward_guard = steward_holder.read().await;
            if steward_guard.is_none() {
                return;
            }

            // Parse steward message
            match bincode::serde::decode_from_slice::<icn_steward::StewardMessage, _>(
                &data,
                bincode::config::legacy(),
            )
            .map(|(v, _)| v)
            {
                Ok(msg) => {
                    debug!(
                        "Received steward message on topic {}: {:?}",
                        topic_clone, msg
                    );
                    // Message handling would be routed to StewardActor here
                    // via handle methods in a full implementation
                }
                Err(e) => {
                    debug!(
                        "Failed to deserialize steward message on {}: {}",
                        topic_clone, e
                    );
                }
            }
        });
    }));
}

/// Initialize steward services if enabled
///
/// Creates:
/// - Steward actor with gossip integration
/// - Subscribes to steward gossip topics
/// - Sets up notification callback for incoming messages
pub async fn init_steward_services(
    config: &StewardNodeConfig,
    deps: StewardDeps,
) -> anyhow::Result<StewardServices> {
    if !config.enabled {
        return Ok(StewardServices {
            steward_handle: None,
        });
    }

    let steward_config = config.to_steward_config();

    // Create gossip send callback for steward messages
    let send_gossip_callback = create_send_callback(deps.gossip_handle.clone());

    // Spawn steward actor
    match icn_steward::StewardActor::spawn(
        deps.did.clone(),
        deps.keypair,
        steward_config,
        deps.shutdown_tx,
        Some(send_gossip_callback),
    )
    .await
    {
        Ok(handle) => {
            info!("✓ Steward actor spawned for DID: {}", deps.did);

            // Fill steward handle holder
            *deps.steward_handle_holder.write().await = Some(handle.clone());

            // Subscribe to steward gossip topics
            subscribe_steward_topics(&deps.gossip_handle, &deps.did).await;

            // Set up notification callback for steward messages
            {
                let mut gossip = deps.gossip_handle.write().await;
                setup_steward_notification_callback(
                    &mut gossip,
                    deps.steward_handle_holder.clone(),
                );
            }

            info!(
                "Steward network participation enabled (threshold: {}-of-{})",
                config.vui_threshold, config.vui_total_shares
            );

            Ok(StewardServices {
                steward_handle: Some(handle),
            })
        }
        Err(e) => {
            error!("Failed to spawn steward actor: {}", e);
            icn_obs::metrics::supervisor::actor_spawn_failed_inc("steward");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steward_deps_struct() {
        // Verify types are accessible
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<GossipHandle>();
        assert_send_sync::<StewardHandleHolder>();
    }
}
