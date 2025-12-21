//! Federated Gossip Router (Phase F4)
//!
//! Routes gossip messages to appropriate federation channels based on scope.

use crate::channel::FederationChannel;
use crate::error::{FederationError, Result};
use crate::metrics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

/// Scope for gossip message routing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
#[derive(Default)]
pub enum GossipScope {
    /// Message stays within this cooperative only
    #[default]
    Local,

    /// Message goes to specific partner cooperatives
    Federation {
        /// List of cooperative IDs to send to
        partners: Vec<String>,
    },

    /// Message goes to all federated cooperatives
    Public,
}

impl GossipScope {
    /// Create a federation scope with specific partners
    pub fn federation(partners: Vec<String>) -> Self {
        GossipScope::Federation { partners }
    }

    /// Check if this scope allows sending to a specific cooperative
    pub fn allows(&self, coop_id: &str) -> bool {
        match self {
            GossipScope::Local => false,
            GossipScope::Federation { partners } => partners.contains(&coop_id.to_string()),
            GossipScope::Public => true,
        }
    }

    /// Check if the message should be sent to federation at all
    pub fn is_federated(&self) -> bool {
        !matches!(self, GossipScope::Local)
    }
}

/// Router for federated gossip messages
pub struct FederatedGossipRouter {
    /// Active channels to federated cooperatives (Arc-wrapped for lock-free forwarding)
    channels: RwLock<HashMap<String, Arc<FederationChannel>>>,

    /// Our cooperative ID
    own_coop_id: String,

    /// Maximum number of channels
    max_channels: usize,
}

impl FederatedGossipRouter {
    /// Create a new federated gossip router
    pub fn new(own_coop_id: String, max_channels: usize) -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
            own_coop_id,
            max_channels,
        }
    }

    /// Add a federation channel
    pub async fn add_channel(&self, coop_id: String, channel: FederationChannel) -> Result<()> {
        let mut channels = self.channels.write().unwrap_or_else(|poisoned| {
            warn!("Channels lock poisoned, recovering");
            poisoned.into_inner()
        });

        if channels.len() >= self.max_channels {
            return Err(FederationError::MaxChannelsExceeded(self.max_channels));
        }

        channels.insert(coop_id.clone(), Arc::new(channel));
        metrics::channel::active_set(channels.len());

        debug!("Added federation channel to {}", coop_id);
        Ok(())
    }

    /// Remove a federation channel
    pub async fn remove_channel(&self, coop_id: &str) -> Result<()> {
        let mut channels = self.channels.write().unwrap_or_else(|poisoned| {
            warn!("Channels lock poisoned, recovering");
            poisoned.into_inner()
        });
        channels.remove(coop_id);
        metrics::channel::active_set(channels.len());

        debug!("Removed federation channel to {}", coop_id);
        Ok(())
    }

    /// Get a reference to a channel
    pub fn get_channel(&self, coop_id: &str) -> Option<bool> {
        self.channels
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Channels lock poisoned, recovering");
                poisoned.into_inner()
            })
            .get(coop_id)
            .map(|c| c.is_connected())
    }

    /// List all channel IDs
    pub fn list_channels(&self) -> Vec<String> {
        self.channels
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Channels lock poisoned, recovering");
                poisoned.into_inner()
            })
            .keys()
            .cloned()
            .collect()
    }

    /// Publish a message with the given scope
    ///
    /// This method collects channel handles under the lock, then drops the lock
    /// before awaiting network operations to avoid blocking writers.
    pub async fn publish(&self, topic: &str, data: Vec<u8>, scope: GossipScope) -> Result<usize> {
        if !scope.is_federated() {
            return Ok(0);
        }

        // Collect channels to forward to while holding the lock briefly
        let channels_to_forward: Vec<(String, Arc<FederationChannel>)> = {
            let channels = self.channels.read().unwrap_or_else(|poisoned| {
                warn!("Channels lock poisoned, recovering");
                poisoned.into_inner()
            });

            channels
                .iter()
                .filter(|(coop_id, channel)| {
                    if !scope.allows(coop_id) {
                        return false;
                    }
                    if !channel.is_connected() {
                        warn!("Channel to {} not connected, skipping", coop_id);
                        return false;
                    }
                    true
                })
                .map(|(coop_id, channel)| (coop_id.clone(), Arc::clone(channel)))
                .collect()
        };
        // Lock is now dropped - safe to await

        let mut sent_count = 0;
        for (coop_id, channel) in channels_to_forward {
            match channel.forward(topic, data.clone()).await {
                Ok(()) => {
                    sent_count += 1;
                    metrics::gossip_routing::forwarded_inc(scope_to_str(&scope));
                }
                Err(e) => {
                    warn!("Failed to forward to {}: {}", coop_id, e);
                }
            }
        }

        debug!(
            "Published to {} federation channels with scope {:?}",
            sent_count, scope
        );
        Ok(sent_count)
    }

    /// Remove stale channels
    pub async fn cleanup_stale(&self, timeout_secs: u64) -> Vec<String> {
        let mut channels = self.channels.write().unwrap_or_else(|poisoned| {
            warn!("Channels lock poisoned, recovering");
            poisoned.into_inner()
        });
        let stale: Vec<String> = channels
            .iter()
            .filter(|(_, c)| c.is_timed_out(timeout_secs))
            .map(|(id, _)| id.clone())
            .collect();

        for coop_id in &stale {
            channels.remove(coop_id);
            metrics::channel::closed_inc(coop_id, "timeout");
        }

        metrics::channel::active_set(channels.len());
        stale
    }

    /// Get the number of active channels
    pub fn channel_count(&self) -> usize {
        self.channels
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Channels lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len()
    }

    /// Get our cooperative ID
    pub fn own_coop_id(&self) -> &str {
        &self.own_coop_id
    }
}

fn scope_to_str(scope: &GossipScope) -> &'static str {
    match scope {
        GossipScope::Local => "local",
        GossipScope::Federation { .. } => "federation",
        GossipScope::Public => "public",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> icn_identity::Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_gossip_scope_allows() {
        let local = GossipScope::Local;
        assert!(!local.allows("any-coop"));

        let public = GossipScope::Public;
        assert!(public.allows("any-coop"));

        let federation =
            GossipScope::federation(vec!["food-coop".to_string(), "tech-coop".to_string()]);
        assert!(federation.allows("food-coop"));
        assert!(federation.allows("tech-coop"));
        assert!(!federation.allows("other-coop"));
    }

    #[tokio::test]
    async fn test_router_add_channel() {
        let router = FederatedGossipRouter::new("test-coop".to_string(), 10);

        let channel = FederationChannel::new(
            "food-coop".to_string(),
            test_did(),
            "https://food-coop.example.com:8080".to_string(),
        );

        router
            .add_channel("food-coop".to_string(), channel)
            .await
            .unwrap();

        assert_eq!(router.channel_count(), 1);
        assert!(router.get_channel("food-coop").is_some());
    }

    #[tokio::test]
    async fn test_router_max_channels() {
        let router = FederatedGossipRouter::new("test-coop".to_string(), 2);

        for i in 0..2 {
            let channel = FederationChannel::new(
                format!("coop-{i}"),
                test_did(),
                format!("https://coop-{i}.example.com:8080"),
            );
            router
                .add_channel(format!("coop-{i}"), channel)
                .await
                .unwrap();
        }

        let channel = FederationChannel::new(
            "coop-overflow".to_string(),
            test_did(),
            "https://coop-overflow.example.com:8080".to_string(),
        );

        let result = router
            .add_channel("coop-overflow".to_string(), channel)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_publish_and_add_remove() {
        use std::sync::Arc;
        use tokio::time::{sleep, Duration};

        let router = Arc::new(FederatedGossipRouter::new("test-coop".to_string(), 100));

        // Add initial channels
        for i in 0..5 {
            let mut channel = FederationChannel::new(
                format!("coop-{i}"),
                test_did(),
                format!("https://coop-{i}.example.com:8080"),
            );
            channel.connect().await.unwrap();
            router
                .add_channel(format!("coop-{i}"), channel)
                .await
                .unwrap();
        }

        // Spawn concurrent tasks: publish, add, and remove channels
        let router_publish = Arc::clone(&router);
        let publish_task = tokio::spawn(async move {
            for _ in 0..10 {
                let _ = router_publish
                    .publish("test-topic", vec![1, 2, 3], GossipScope::Public)
                    .await;
                sleep(Duration::from_millis(1)).await;
            }
        });

        let router_add = Arc::clone(&router);
        let add_task = tokio::spawn(async move {
            for i in 10..15 {
                let mut channel = FederationChannel::new(
                    format!("coop-{i}"),
                    test_did(),
                    format!("https://coop-{i}.example.com:8080"),
                );
                channel.connect().await.unwrap();
                let _ = router_add.add_channel(format!("coop-{i}"), channel).await;
                sleep(Duration::from_millis(2)).await;
            }
        });

        let router_remove = Arc::clone(&router);
        let remove_task = tokio::spawn(async move {
            for i in 0..3 {
                let _ = router_remove.remove_channel(&format!("coop-{i}")).await;
                sleep(Duration::from_millis(3)).await;
            }
        });

        // All tasks should complete without deadlock or panic
        let (publish_result, add_result, remove_result) =
            tokio::join!(publish_task, add_task, remove_task);

        assert!(publish_result.is_ok(), "Publish task should not panic");
        assert!(add_result.is_ok(), "Add task should not panic");
        assert!(remove_result.is_ok(), "Remove task should not panic");
    }
}
