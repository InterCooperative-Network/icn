//! Upgrade Coordinator Actor
//!
//! Coordinates network-wide protocol upgrades using gossip and governance.
//!
//! # Architecture
//! - Monitors upgrade proposals via governance events
//! - Tracks version adoption across network peers
//! - Coordinates staged rollouts with backoff
//! - Provides upgrade status via RPC/Gateway

use anyhow::{Context, Result};
use icn_gossip::GossipHandle;
use icn_identity::Did;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

use crate::runtime::ShutdownRx;
use crate::supervisor::version_tracker::VersionTracker;

/// Topic for upgrade coordination messages
pub const UPGRADE_TOPIC: &str = "system:upgrade";

/// Minimum network adoption percentage to trigger auto-upgrade
const MIN_ADOPTION_PERCENT: f32 = 0.75; // 75% of peers

/// Upgrade status report
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpgradeStatus {
    pub current_version: String,
    pub target_version: Option<String>,
    pub network_adoption: HashMap<String, usize>, // version -> peer count
    pub upgrade_ready: bool,
    pub upgrade_scheduled_at: Option<u64>,
}

/// Upgrade coordination message
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum UpgradeMessage {
    /// Announce version running
    VersionAnnounce { version: String, did: Did },

    /// Propose network upgrade
    UpgradeProposal {
        target_version: String,
        proposed_by: Did,
        activation_time: u64,
    },

    /// Acknowledge upgrade readiness
    UpgradeReady { version: String, did: Did },
}

pub enum UpgradeActorMsg {
    GetStatus {
        reply: tokio::sync::oneshot::Sender<UpgradeStatus>,
    },
    ProposeUpgrade {
        target_version: String,
        activation_time: u64,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
}

/// Handle for interacting with UpgradeActor
#[derive(Clone)]
pub struct UpgradeHandle {
    tx: mpsc::Sender<UpgradeActorMsg>,
}

impl UpgradeHandle {
    /// Get current upgrade status
    pub async fn get_status(&self) -> Result<UpgradeStatus> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(UpgradeActorMsg::GetStatus { reply: reply_tx })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {e}"))?;
        reply_rx
            .await
            .map_err(|e| anyhow::anyhow!("Failed to receive reply: {e}"))
    }

    /// Propose a network-wide upgrade
    pub async fn propose_upgrade(
        &self,
        target_version: String,
        activation_time: u64,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(UpgradeActorMsg::ProposeUpgrade {
                target_version,
                activation_time,
                reply: reply_tx,
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send message: {e}"))?;
        reply_rx
            .await
            .map_err(|e| anyhow::anyhow!("Failed to receive reply: {e}"))?
    }
}

/// Upgrade coordinator actor
pub struct UpgradeActor {
    own_did: Did,
    current_version: String,
    _version_tracker: Arc<RwLock<VersionTracker>>,
    gossip_handle: GossipHandle,

    // State
    target_version: Option<String>,
    activation_time: Option<u64>,
    peer_versions: HashMap<Did, String>,
    ready_peers: HashMap<Did, String>,
}

impl UpgradeActor {
    /// Spawn the upgrade coordinator actor
    pub fn spawn(
        own_did: Did,
        current_version: String,
        version_tracker: Arc<RwLock<VersionTracker>>,
        gossip_handle: GossipHandle,
        mut shutdown_rx: ShutdownRx,
    ) -> UpgradeHandle {
        let (tx, mut rx) = mpsc::channel::<UpgradeActorMsg>(32);

        let mut actor = UpgradeActor {
            own_did: own_did.clone(),
            current_version: current_version.clone(),
            _version_tracker: version_tracker,
            gossip_handle: gossip_handle.clone(),
            target_version: None,
            activation_time: None,
            peer_versions: HashMap::new(),
            ready_peers: HashMap::new(),
        };

        tokio::spawn(async move {
            info!("UpgradeActor started for version {}", current_version);

            // Subscribe to upgrade gossip topic
            {
                let mut gossip = gossip_handle.write().await;
                if let Err(e) = gossip.subscribe(UPGRADE_TOPIC, own_did.clone()).await {
                    warn!("Failed to subscribe to upgrade topic: {}", e);
                } else {
                    debug!("Subscribed to {} topic", UPGRADE_TOPIC);
                }
            }

            // Announce own version periodically
            let mut announce_interval = interval(Duration::from_secs(300)); // 5 minutes

            // Check upgrade readiness periodically
            let mut check_interval = interval(Duration::from_secs(60)); // 1 minute

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        info!("UpgradeActor shutting down");
                        break;
                    }

                    Some(msg) = rx.recv() => {
                        actor.handle_message(msg).await;
                    }

                    _ = announce_interval.tick() => {
                        if let Err(e) = actor.announce_version().await {
                            warn!("Failed to announce version: {}", e);
                        }
                    }

                    _ = check_interval.tick() => {
                        actor.check_upgrade_readiness().await;
                    }
                }
            }

            info!("UpgradeActor stopped");
        });

        UpgradeHandle { tx }
    }

    async fn handle_message(&mut self, msg: UpgradeActorMsg) {
        match msg {
            UpgradeActorMsg::GetStatus { reply } => {
                let status = self.get_status();
                let _ = reply.send(status);
            }

            UpgradeActorMsg::ProposeUpgrade {
                target_version,
                activation_time,
                reply,
            } => {
                let result = self.propose_upgrade(target_version, activation_time).await;
                let _ = reply.send(result);
            }
        }
    }

    async fn announce_version(&self) -> Result<()> {
        let msg = UpgradeMessage::VersionAnnounce {
            version: self.current_version.clone(),
            did: self.own_did.clone(),
        };

        let payload = icn_encoding::encode_bincode_legacy(&msg)?;

        let mut gossip = self.gossip_handle.write().await;
        gossip
            .publish(UPGRADE_TOPIC, payload)
            .await
            .context("Failed to publish version announcement")?;

        debug!("Announced version: {}", self.current_version);
        Ok(())
    }

    async fn propose_upgrade(
        &mut self,
        target_version: String,
        activation_time: u64,
    ) -> Result<()> {
        info!(
            "Proposing upgrade to version {} at {}",
            target_version, activation_time
        );

        self.target_version = Some(target_version.clone());
        self.activation_time = Some(activation_time);

        let msg = UpgradeMessage::UpgradeProposal {
            target_version,
            proposed_by: self.own_did.clone(),
            activation_time,
        };

        let payload = icn_encoding::encode_bincode_legacy(&msg)?;

        let mut gossip = self.gossip_handle.write().await;
        gossip
            .publish(UPGRADE_TOPIC, payload)
            .await
            .context("Failed to publish upgrade proposal")?;

        Ok(())
    }

    async fn check_upgrade_readiness(&mut self) {
        if let Some(ref target) = self.target_version {
            let total_peers = self.peer_versions.len().max(1);
            let target_count = self.peer_versions.values().filter(|v| *v == target).count();

            let adoption_rate = target_count as f32 / total_peers as f32;

            if adoption_rate >= MIN_ADOPTION_PERCENT {
                info!(
                    "Upgrade readiness threshold met: {:.1}% of peers on version {}",
                    adoption_rate * 100.0,
                    target
                );

                // Announce readiness
                if let Err(e) = self.announce_ready().await {
                    warn!("Failed to announce upgrade readiness: {}", e);
                }
            } else {
                debug!(
                    "Upgrade adoption: {:.1}% ({}/{})",
                    adoption_rate * 100.0,
                    target_count,
                    total_peers
                );
            }
        }
    }

    async fn announce_ready(&self) -> Result<()> {
        if let Some(ref target) = self.target_version {
            let msg = UpgradeMessage::UpgradeReady {
                version: target.clone(),
                did: self.own_did.clone(),
            };

            let payload = icn_encoding::encode_bincode_legacy(&msg)?;

            let mut gossip = self.gossip_handle.write().await;
            gossip
                .publish(UPGRADE_TOPIC, payload)
                .await
                .context("Failed to publish upgrade ready message")?;

            info!("Announced upgrade readiness for version {}", target);
        }
        Ok(())
    }

    fn get_status(&self) -> UpgradeStatus {
        let mut adoption = HashMap::new();
        for version in self.peer_versions.values() {
            *adoption.entry(version.clone()).or_insert(0) += 1;
        }

        let upgrade_ready = if let Some(ref target) = self.target_version {
            let total = self.peer_versions.len().max(1);
            let target_count = adoption.get(target).copied().unwrap_or(0);
            (target_count as f32 / total as f32) >= MIN_ADOPTION_PERCENT
        } else {
            false
        };

        UpgradeStatus {
            current_version: self.current_version.clone(),
            target_version: self.target_version.clone(),
            network_adoption: adoption,
            upgrade_ready,
            upgrade_scheduled_at: self.activation_time,
        }
    }

    /// Handle incoming upgrade message from gossip
    pub fn handle_upgrade_message(&mut self, _from: &Did, msg: UpgradeMessage) {
        match msg {
            UpgradeMessage::VersionAnnounce { version, did } => {
                debug!("Peer {} running version {}", did, version);
                self.peer_versions.insert(did, version);
            }

            UpgradeMessage::UpgradeProposal {
                target_version,
                proposed_by,
                activation_time,
            } => {
                info!(
                    "Upgrade proposal received from {}: {} at {}",
                    proposed_by, target_version, activation_time
                );

                // Accept proposal if we don't have one yet or this is newer
                if self.target_version.is_none()
                    || self.activation_time.is_none_or(|t| activation_time > t)
                {
                    self.target_version = Some(target_version);
                    self.activation_time = Some(activation_time);
                }
            }

            UpgradeMessage::UpgradeReady { version, did } => {
                debug!("Peer {} ready for upgrade to {}", did, version);
                self.ready_peers.insert(did, version);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upgrade_status_serialization() {
        let mut adoption = HashMap::new();
        adoption.insert("v1.0.0".to_string(), 5);
        adoption.insert("v1.1.0".to_string(), 3);

        let status = UpgradeStatus {
            current_version: "v1.0.0".to_string(),
            target_version: Some("v1.1.0".to_string()),
            network_adoption: adoption,
            upgrade_ready: false,
            upgrade_scheduled_at: Some(1234567890),
        };

        let serialized = serde_json::to_string(&status).unwrap();
        let deserialized: UpgradeStatus = serde_json::from_str(&serialized).unwrap();

        assert_eq!(status.current_version, deserialized.current_version);
        assert_eq!(status.target_version, deserialized.target_version);
        assert!(!deserialized.upgrade_ready);
    }
}
