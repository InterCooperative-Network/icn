//! Test node implementation with full ICN stack

use anyhow::{Context, Result};
use icn_gossip::{AccessControl, GossipActor, GossipEntry, GossipMessage, Topic};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_net::{
    IncomingMessageHandler, MessagePayload, NetworkActor, NetworkHandle, NetworkMessage,
};
use icn_store::SledStore;
use icn_trust::TrustClass;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch, Mutex, RwLock};
use tracing::{debug, info};

use crate::util::pick_port;

/// Configuration for spawning a test node
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Port to listen on (if None, auto-assigned)
    pub port: Option<u16>,
    /// Default trust class for unknown peers
    pub default_trust_class: TrustClass,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            port: None,
            default_trust_class: TrustClass::Partner,
        }
    }
}

/// A fully-functional test node with isolated state
///
/// TestNode provides a complete ICN node suitable for integration testing.
/// Each node has its own identity, network connection, gossip actor, and storage.
///
/// # Example
///
/// ```rust,ignore
/// let node = TestNode::spawn(NodeConfig::default()).await?;
/// node.create_topic("test:events", AccessControl::Public).await;
/// let hash = node.publish("test:events", b"hello world").await?;
/// node.shutdown().await;
/// ```
pub struct TestNode {
    /// The node's keypair
    pub keypair: KeyPair,
    /// The node's DID
    pub did: Did,
    /// Network handle for sending messages
    pub network: NetworkHandle,
    /// Gossip actor handle
    pub gossip: Arc<RwLock<GossipActor>>,
    /// Storage backend
    pub store: Arc<SledStore>,
    /// Network address this node is listening on
    pub addr: SocketAddr,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Received notifications: topic -> [(hash, from_did)]
    notifications: Arc<Mutex<HashMap<String, Vec<([u8; 32], Did)>>>>,
    /// Node index (for cluster identification)
    pub index: usize,
}

impl TestNode {
    /// Spawn a new test node with the given configuration
    pub async fn spawn(config: NodeConfig) -> Result<Self> {
        Self::spawn_with_index(config, 0).await
    }

    /// Spawn a new test node with an index (used by TestCluster)
    pub(crate) async fn spawn_with_index(config: NodeConfig, index: usize) -> Result<Self> {
        crate::util::install_crypto_provider();

        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let port = config.port.unwrap_or_else(pick_port);

        info!("Spawning test node {} with DID: {}", index, did);

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel(16);

        // Create storage
        let store = Arc::new(SledStore::temporary().context("Failed to create temp storage")?);

        // Create trust lookup with configurable default
        let default_trust = config.default_trust_class;
        let trust_lookup = Arc::new(move |_did: &Did| Some(default_trust));

        // Spawn gossip actor
        let gossip = GossipActor::spawn(did.clone(), trust_lookup);

        // Set up notification tracking
        let notifications = Arc::new(Mutex::new(HashMap::<String, Vec<([u8; 32], Did)>>::new()));
        let notifications_clone = notifications.clone();

        // Configure notification callback
        {
            let mut gossip_guard = gossip.write().await;
            let callback = Arc::new(
                move |topic: String, entry: GossipEntry, subscriber_did: Did| {
                    let hash = entry.hash;
                    let notifs = notifications_clone.clone();
                    tokio::spawn(async move {
                        let mut map = notifs.lock().await;
                        map.entry(topic).or_default().push((hash, subscriber_did));
                    });
                },
            );
            gossip_guard.set_notification_callback(callback);
        }

        // Set up network message handler
        // Use watch channel to safely share network handle with message handler
        // This eliminates the race condition where messages could arrive before
        // the handle is stored in the RwLock
        let gossip_clone = gossip.clone();
        let (network_tx, network_rx) = watch::channel(None::<NetworkHandle>);
        let own_did = did.clone();

        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender = net_msg.from.clone();

            match net_msg.payload {
                MessagePayload::Gossip(gossip_msg) => {
                    let gossip = gossip_clone.clone();
                    let sender = sender.clone();
                    tokio::spawn(async move {
                        let mut g = gossip.write().await;
                        if let Err(e) = g.handle_message(&sender, gossip_msg).await {
                            debug!("Gossip message handling error: {}", e);
                        }
                    });
                }

                MessagePayload::Subscribe { topics } => {
                    let gossip = gossip_clone.clone();
                    let sender = sender.clone();
                    let mut net_rx = network_rx.clone();
                    let own = own_did.clone();
                    tokio::spawn(async move {
                        let mut g = gossip.write().await;
                        let mut acked = Vec::new();
                        for topic in &topics {
                            if g.subscribe(topic, sender.clone()).await.is_ok() {
                                acked.push(topic.clone());
                            }
                        }
                        if !acked.is_empty() {
                            // Wait for network handle to be available (fixes race condition)
                            // Timeout after 1 second to avoid hanging forever
                            let net = tokio::time::timeout(Duration::from_secs(1), async {
                                loop {
                                    if let Some(net) = net_rx.borrow().clone() {
                                        return Some(net);
                                    }
                                    if net_rx.changed().await.is_err() {
                                        break;
                                    }
                                }
                                // Channel closed, return current value if any
                                net_rx.borrow().clone()
                            })
                            .await;

                            match net {
                                Ok(Some(net)) => {
                                    let ack =
                                        NetworkMessage::subscribe_ack(own, sender.clone(), acked);
                                    if let Err(e) = net.send_message(sender, ack).await {
                                        debug!("Failed to send SubscribeAck: {}", e);
                                    }
                                }
                                Ok(None) => {
                                    debug!("Network handle not available for SubscribeAck");
                                }
                                Err(_) => {
                                    debug!("Timeout waiting for network handle");
                                }
                            }
                        }
                    });
                }

                MessagePayload::Unsubscribe { topics } => {
                    let gossip = gossip_clone.clone();
                    let sender = sender.clone();
                    tokio::spawn(async move {
                        let mut g = gossip.write().await;
                        for topic in &topics {
                            if let Err(e) = g.unsubscribe(topic, &sender) {
                                debug!("Failed to unsubscribe {} from {}: {}", sender, topic, e);
                            }
                        }
                    });
                }

                MessagePayload::SubscribeAck { topics: _ } => {
                    // Just log
                    debug!("Received SubscribeAck from {}", sender);
                }

                _ => {}
            }
        });

        // Spawn network actor
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone())?;
        let network = NetworkActor::spawn(
            identity_bundle,
            addr,
            shutdown_tx.clone(),
            Some(incoming_handler),
            None, // No trust graph
            None, // No trust-gated config
            None, // No fallback config
            None, // No topology config
            None, // No STUN servers
            None, // No TURN config
            None, // No misbehavior detector
            None, // No store
        )
        .await?;

        // Send network handle to message handler (fixes race condition)
        let _ = network_tx.send(Some(network.clone()));

        // Set up gossip send callback
        {
            let mut g = gossip.write().await;
            let net = network.clone();
            let from = did.clone();

            let send_callback = Arc::new(move |recipient: Option<Did>, msg: GossipMessage| {
                let net = net.clone();
                let from = from.clone();
                tokio::spawn(async move {
                    let net_msg = NetworkMessage::gossip(from, recipient.clone(), msg);
                    let result = if let Some(to) = recipient {
                        net.send_message(to, net_msg).await
                    } else {
                        net.broadcast(net_msg).await
                    };
                    if let Err(e) = result {
                        debug!("Failed to send gossip: {}", e);
                    }
                });
            });

            g.set_send_callback(send_callback);
        }

        info!("Test node {} listening on {}", index, addr);

        Ok(TestNode {
            keypair,
            did,
            network,
            gossip,
            store,
            addr,
            shutdown_tx,
            notifications,
            index,
        })
    }

    /// Shutdown the test node gracefully
    pub async fn shutdown(self) {
        info!("Shutting down test node {}", self.index);
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    /// Connect to another test node
    pub async fn connect(&self, other: &TestNode) -> Result<()> {
        self.network.dial(other.addr, other.did.clone()).await?;
        // Brief delay to allow connection establishment
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }

    /// Create a gossip topic on this node
    pub async fn create_topic(&self, name: &str, access: AccessControl) {
        let mut g = self.gossip.write().await;
        g.create_topic(Topic::new(name.to_string(), access));
    }

    /// Publish data to a topic
    pub async fn publish(&self, topic: &str, data: &[u8]) -> Result<[u8; 32]> {
        let mut g = self.gossip.write().await;
        g.publish(topic, data.to_vec())
            .await
            .context("Failed to publish")
    }

    /// Subscribe to a topic on a remote node
    pub async fn subscribe_to(&self, topic: &str, remote: &TestNode) -> Result<()> {
        let msg = NetworkMessage::subscribe(
            self.did.clone(),
            remote.did.clone(),
            vec![topic.to_string()],
        );
        self.network.send_message(remote.did.clone(), msg).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(())
    }

    /// Check if this node has a specific entry
    pub async fn has_entry(&self, topic: &str, hash: &[u8; 32]) -> bool {
        let g = self.gossip.read().await;
        g.get_entry(topic, hash).is_some()
    }

    /// Get all entries for a topic
    pub async fn get_entries(&self, topic: &str) -> Vec<GossipEntry> {
        let g = self.gossip.read().await;
        g.get_entries(topic)
    }

    /// Get entry count for a topic
    pub async fn entry_count(&self, topic: &str) -> usize {
        let g = self.gossip.read().await;
        g.get_entries(topic).len()
    }

    /// Get all received notifications for a topic
    pub async fn get_notifications(&self, topic: &str) -> Vec<([u8; 32], Did)> {
        let map = self.notifications.lock().await;
        map.get(topic).cloned().unwrap_or_default()
    }

    /// Clear notification history
    pub async fn clear_notifications(&self) {
        let mut map = self.notifications.lock().await;
        map.clear();
    }

    /// Get the number of connected peers
    pub async fn peer_count(&self) -> usize {
        self.network.get_peers().await.map(|p| p.len()).unwrap_or(0)
    }

    /// Check if connected to a specific peer
    pub async fn is_connected_to(&self, did: &Did) -> bool {
        self.network.is_peer_connected(did).await.unwrap_or(false)
    }

    /// Wait for connection to a specific peer
    pub async fn wait_connected(&self, did: &Did, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if self.network.is_peer_connected(did).await.unwrap_or(false) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        anyhow::bail!("Timeout waiting for connection to {}", did)
    }
}

impl std::fmt::Debug for TestNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestNode")
            .field("index", &self.index)
            .field("did", &self.did.to_string())
            .field("addr", &self.addr)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_spawn_node() -> Result<()> {
        let node = TestNode::spawn(NodeConfig::default()).await?;
        assert!(!node.did.to_string().is_empty());
        node.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_create_topic_and_publish() -> Result<()> {
        let node = TestNode::spawn(NodeConfig::default()).await?;
        node.create_topic("test:events", AccessControl::Public)
            .await;

        let hash = node.publish("test:events", b"hello").await?;
        assert!(node.has_entry("test:events", &hash).await);
        assert_eq!(node.entry_count("test:events").await, 1);

        node.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_two_nodes_connect() -> Result<()> {
        let node1 = TestNode::spawn(NodeConfig::default()).await?;
        let node2 = TestNode::spawn(NodeConfig::default()).await?;

        node1.connect(&node2).await?;

        // Allow time for bidirectional connection
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert!(node1.is_connected_to(&node2.did).await);

        node1.shutdown().await;
        node2.shutdown().await;
        Ok(())
    }
}
