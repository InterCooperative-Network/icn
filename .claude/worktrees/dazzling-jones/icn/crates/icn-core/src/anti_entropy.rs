//! Anti-entropy task for periodic gossip synchronization
//!
//! This module implements a background task that periodically ensures
//! nodes stay in sync by exchanging bloom filters and requesting missing entries.

use anyhow::Result;
use icn_gossip::{GossipHandle, GossipMessage};
use icn_net::{NetworkHandle, NetworkMessage};
use std::time::Duration;
use tracing::{debug, info};

/// Anti-entropy configuration
pub struct AntiEntropyConfig {
    /// How often to run anti-entropy (default: 30 seconds)
    pub interval: Duration,

    /// Maximum number of missing entries to request per round
    pub max_missing_per_round: usize,
}

impl Default for AntiEntropyConfig {
    fn default() -> Self {
        AntiEntropyConfig {
            interval: Duration::from_secs(30),
            max_missing_per_round: 100,
        }
    }
}

/// Spawn anti-entropy background task
///
/// This task periodically exchanges bloom filters with connected peers
/// to detect and sync missing entries.
pub fn spawn_anti_entropy_task(
    gossip_handle: GossipHandle,
    network_handle: NetworkHandle,
    own_did: icn_identity::Did,
    config: AntiEntropyConfig,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!(
            "Anti-entropy task starting (interval: {:?})",
            config.interval
        );

        let mut interval = tokio::time::interval(config.interval);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = run_anti_entropy_round(
                        &gossip_handle,
                        &network_handle,
                        &own_did,
                        &config,
                    )
                    .await
                    {
                        debug!("Anti-entropy round error: {}", e);
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Anti-entropy task received shutdown signal");
                    break;
                }
            }
        }

        info!("Anti-entropy task stopped");
    })
}

/// Run a single anti-entropy round
async fn run_anti_entropy_round(
    gossip_handle: &GossipHandle,
    network_handle: &NetworkHandle,
    own_did: &icn_identity::Did,
    _config: &AntiEntropyConfig,
) -> Result<()> {
    debug!("Running anti-entropy round");

    // Get list of connected peers
    let stats = network_handle.get_stats().await?;
    if stats.connections_active == 0 {
        debug!("No active connections, skipping anti-entropy");
        return Ok(());
    }

    // Get all topics from gossip
    let topics = {
        let gossip = gossip_handle.read().await;
        gossip.get_topics()
    };

    if topics.is_empty() {
        debug!("No topics to sync, skipping anti-entropy");
        return Ok(());
    }

    debug!(
        "Anti-entropy: {} topics, {} peers",
        topics.len(),
        stats.connections_active
    );

    // For demonstration, request bloom filter for first topic from all peers
    // In a full implementation, this would iterate over all peers and topics
    if let Some(first_topic) = topics.first() {
        debug!("Requesting bloom filter for topic: {}", first_topic);

        let request = GossipMessage::RequestBloomFilter {
            topic: first_topic.clone(),
        };

        let net_msg = NetworkMessage::gossip(own_did.clone(), None, request);

        // Broadcast to all connected peers
        network_handle.broadcast(net_msg).await?;

        debug!("Bloom filter request broadcast to all peers");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AntiEntropyConfig::default();
        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.max_missing_per_round, 100);
    }

    #[test]
    fn test_custom_config() {
        let config = AntiEntropyConfig {
            interval: Duration::from_secs(60),
            max_missing_per_round: 50,
        };
        assert_eq!(config.interval, Duration::from_secs(60));
        assert_eq!(config.max_missing_per_round, 50);
    }
}
