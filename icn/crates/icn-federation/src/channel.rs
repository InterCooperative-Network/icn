//! Federation Channel (Phase F4)
//!
//! Channels for communicating with federated cooperatives.

use crate::error::{FederationError, Result};
use crate::metrics;
use icn_identity::Did;
use std::collections::HashSet;
use std::time::Instant;

/// A channel to a federated cooperative
pub struct FederationChannel {
    /// Remote cooperative's ID
    pub remote_coop_id: String,

    /// Remote cooperative's DID
    pub remote_coop_did: Did,

    /// Gateway endpoint URL
    pub endpoint: String,

    /// Topics we're subscribed to on this channel
    subscribed_topics: HashSet<String>,

    /// When we last had activity on this channel
    pub last_activity: Instant,

    /// Whether the channel is currently connected
    connected: bool,
}

impl FederationChannel {
    /// Create a new federation channel
    pub fn new(remote_coop_id: String, remote_coop_did: Did, endpoint: String) -> Self {
        Self {
            remote_coop_id,
            remote_coop_did,
            endpoint,
            subscribed_topics: HashSet::new(),
            last_activity: Instant::now(),
            connected: false,
        }
    }

    /// Connect to the remote cooperative
    pub async fn connect(&mut self) -> Result<()> {
        // In a real implementation, this would establish a QUIC/WebSocket connection
        // For now, just mark as connected
        self.connected = true;
        self.last_activity = Instant::now();

        metrics::channel::opened_inc(&self.remote_coop_id);
        Ok(())
    }

    /// Disconnect from the remote cooperative
    pub async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        metrics::channel::closed_inc(&self.remote_coop_id, "explicit");
        Ok(())
    }

    /// Check if the channel is connected
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Subscribe to topics on this channel
    pub async fn subscribe(&mut self, topics: Vec<String>) -> Result<()> {
        for topic in topics {
            self.subscribed_topics.insert(topic);
        }
        self.last_activity = Instant::now();
        Ok(())
    }

    /// Unsubscribe from topics on this channel
    pub async fn unsubscribe(&mut self, topics: Vec<String>) -> Result<()> {
        for topic in topics {
            self.subscribed_topics.remove(&topic);
        }
        self.last_activity = Instant::now();
        Ok(())
    }

    /// Get subscribed topics
    pub fn subscribed_topics(&self) -> &HashSet<String> {
        &self.subscribed_topics
    }

    /// Forward a gossip entry to this channel
    pub async fn forward(&self, _topic: &str, _data: Vec<u8>) -> Result<()> {
        if !self.connected {
            return Err(FederationError::ChannelConnectionFailed(
                "Channel not connected".to_string(),
            ));
        }

        // In a real implementation, this would send the data over the connection
        metrics::channel::messages_sent_inc(&self.remote_coop_id);

        Ok(())
    }

    /// Check if the channel has timed out
    pub fn is_timed_out(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed().as_secs() > timeout_secs
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[tokio::test]
    async fn test_channel_creation() {
        let channel = FederationChannel::new(
            "food-coop".to_string(),
            test_did(),
            "https://food-coop.example.com:8080".to_string(),
        );

        assert_eq!(channel.remote_coop_id, "food-coop");
        assert!(!channel.is_connected());
    }

    #[tokio::test]
    async fn test_channel_connect() {
        let mut channel = FederationChannel::new(
            "food-coop".to_string(),
            test_did(),
            "https://food-coop.example.com:8080".to_string(),
        );

        channel.connect().await.unwrap();
        assert!(channel.is_connected());

        channel.disconnect().await.unwrap();
        assert!(!channel.is_connected());
    }

    #[tokio::test]
    async fn test_channel_subscriptions() {
        let mut channel = FederationChannel::new(
            "food-coop".to_string(),
            test_did(),
            "https://food-coop.example.com:8080".to_string(),
        );

        channel
            .subscribe(vec!["topic1".to_string(), "topic2".to_string()])
            .await
            .unwrap();

        assert!(channel.subscribed_topics().contains("topic1"));
        assert!(channel.subscribed_topics().contains("topic2"));

        channel
            .unsubscribe(vec!["topic1".to_string()])
            .await
            .unwrap();

        assert!(!channel.subscribed_topics().contains("topic1"));
        assert!(channel.subscribed_topics().contains("topic2"));
    }
}
