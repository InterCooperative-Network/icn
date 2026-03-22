//! Test node implementation with full ICN stack

use anyhow::{Context, Result};
use icn_gossip::{
    start_digest_emitter, AccessControl, GossipActor, GossipEntry, GossipMessage, Topic,
};
use icn_identity::{Did, IdentityBundle, KeyPair};
use icn_kernel_api::authz::{ConstraintSet, Domain, PolicyDecision, PolicyOracle, PolicyRequest};
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
use tracing::{debug, info, warn};

use crate::simulation::NetworkCondition;
use crate::util::{pick_port, poll_with_backoff, BackoffConfig};

/// Guard that triggers shutdown on drop unless disarmed
///
/// This ensures that if spawn_with_index fails after NetworkActor starts,
/// the network actor is properly cleaned up.
struct ShutdownGuard {
    shutdown_tx: Option<broadcast::Sender<()>>,
    index: usize,
}

impl ShutdownGuard {
    fn new(shutdown_tx: broadcast::Sender<()>, index: usize) -> Self {
        Self {
            shutdown_tx: Some(shutdown_tx),
            index,
        }
    }

    /// Disarm the guard - call this when spawn succeeds
    fn disarm(&mut self) {
        self.shutdown_tx = None;
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        if let Some(ref tx) = self.shutdown_tx {
            warn!(
                "TestNode {} spawn failed after NetworkActor started, triggering cleanup",
                self.index
            );
            let _ = tx.send(());
        }
    }
}

/// Type alias for notification storage: topic -> [(hash, subscriber_did)]
type NotificationMap = HashMap<String, Vec<([u8; 32], Did)>>;

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

/// Simple PolicyOracle for tests that applies default trust class logic
pub struct TestPolicyOracle {
    default_trust: TrustClass,
}

impl TestPolicyOracle {
    pub fn new(default_trust: TrustClass) -> Self {
        Self { default_trust }
    }
}

impl PolicyOracle for TestPolicyOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        let mut constraints = ConstraintSet::default();

        // Populate standard fields based on default trust class
        match self.default_trust {
            TrustClass::Federated => {
                constraints = constraints
                    .with_max_subscriptions(1000)
                    .with_max_outstanding_requests(3)
                    .with_max_message_size(1024 * 1024);
            }
            TrustClass::Partner => {
                constraints = constraints
                    .with_max_subscriptions(500)
                    .with_max_outstanding_requests(3)
                    .with_max_message_size(1024 * 1024);
            }
            TrustClass::Known => {
                constraints = constraints
                    .with_max_subscriptions(100)
                    .with_max_outstanding_requests(2)
                    .with_max_message_size(256 * 1024);
            }
            TrustClass::Isolated => {
                constraints = constraints
                    .with_max_subscriptions(10)
                    .with_max_outstanding_requests(1)
                    .with_max_message_size(64 * 1024);
            }
        }

        // Also inject trust score for compatibility with any custom checks
        let score = match self.default_trust {
            TrustClass::Federated => 0.8,
            TrustClass::Partner => 0.5,
            TrustClass::Known => 0.2,
            TrustClass::Isolated => 0.05,
        };
        constraints
            .custom
            .insert("trust_score".to_string(), score.into());

        PolicyDecision::Allow {
            constraints,
            policy_domain: None,
            evaluated_at: None,
        }
    }

    fn domain(&self) -> Domain {
        Domain::trust()
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
    notifications: Arc<Mutex<NotificationMap>>,
    /// Node index (for cluster identification)
    pub index: usize,
    /// Network condition for simulation (latency, packet loss, etc.)
    network_condition: Arc<RwLock<Option<NetworkCondition>>>,
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

        // Create policy oracle with configurable default
        let oracle = Arc::new(TestPolicyOracle::new(config.default_trust_class));

        // Spawn gossip actor
        // Note: GossipActor::spawn is gone, use GossipActor::new + manual wrapping?
        // Or if GossipActor::spawn is updated to take oracle, use that.
        // The error message said: associated function defined here --> .../gossip.rs:1291:12 pub fn spawn...
        // So GossipActor::spawn exists but takes Option<Arc<dyn PolicyOracle>>.
        let gossip = GossipActor::spawn(did.clone(), Some(oracle));

        // Set up notification tracking
        let notifications = Arc::new(Mutex::new(NotificationMap::new()));
        let notifications_clone = notifications.clone();

        // Configure notification callback with shutdown handling
        {
            let mut gossip_guard = gossip.write().await;
            let shutdown = shutdown_tx.clone();
            let callback = Arc::new(
                move |topic: String, entry: GossipEntry, subscriber_did: Did| {
                    let hash = entry.hash;
                    let notifs = notifications_clone.clone();
                    let mut shutdown_rx = shutdown.subscribe();
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = shutdown_rx.recv() => {
                                // Node shutting down, abort notification storage
                            }
                            _ = async {
                                let mut map = notifs.lock().await;
                                map.entry(topic).or_default().push((hash, subscriber_did));
                            } => {}
                        }
                    });
                },
            );
            gossip_guard.set_notification_callback(callback);
        }

        // Set up network message handler with shutdown handling
        // Use watch channel to safely share network handle with message handler
        // This eliminates the race condition where messages could arrive before
        // the handle is stored in the RwLock
        let gossip_clone = gossip.clone();
        let (network_tx, network_rx) = watch::channel(None::<NetworkHandle>);
        let own_did = did.clone();
        let handler_shutdown = shutdown_tx.clone();

        let incoming_handler: IncomingMessageHandler = Arc::new(move |net_msg| {
            let sender = net_msg.from.clone();

            match net_msg.payload {
                MessagePayload::Gossip(gossip_msg) => {
                    let gossip = gossip_clone.clone();
                    let sender = sender.clone();
                    let mut shutdown_rx = handler_shutdown.subscribe();
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = shutdown_rx.recv() => {
                                // Node shutting down, abort gossip handling
                            }
                            result = async {
                                let mut g = gossip.write().await;
                                g.handle_message(&sender, gossip_msg).await
                            } => {
                                if let Err(e) = result {
                                    debug!("Gossip message handling error: {}", e);
                                }
                            }
                        }
                    });
                }

                MessagePayload::Subscribe { topics } => {
                    let gossip = gossip_clone.clone();
                    let sender = sender.clone();
                    let mut net_rx = network_rx.clone();
                    let own = own_did.clone();
                    let mut shutdown_rx = handler_shutdown.subscribe();
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = shutdown_rx.recv() => {
                                // Node shutting down, abort subscribe handling
                            }
                            _ = async {
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
                            } => {}
                        }
                    });
                }

                MessagePayload::Unsubscribe { topics } => {
                    let gossip = gossip_clone.clone();
                    let sender = sender.clone();
                    let mut shutdown_rx = handler_shutdown.subscribe();
                    tokio::spawn(async move {
                        tokio::select! {
                            _ = shutdown_rx.recv() => {
                                // Node shutting down, abort unsubscribe handling
                            }
                            _ = async {
                                let mut g = gossip.write().await;
                                for topic in &topics {
                                    if let Err(e) = g.unsubscribe(topic, &sender) {
                                        debug!("Failed to unsubscribe {} from {}: {}", sender, topic, e);
                                    }
                                }
                            } => {}
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
            None, // No PolicyOracle
            None, // No fallback config
            None, // No topology config
            None, // No STUN servers
            None, // No TURN config
            None, // No misbehavior detector
            None, // No store
            None, // No personhood store (Sybil resistance)
            None, // No anchor rate config
            None, // No advertised addr override
        )
        .await?;

        // Create cleanup guard - ensures NetworkActor is shut down if subsequent setup fails
        let mut cleanup_guard = ShutdownGuard::new(shutdown_tx.clone(), index);

        // Send network handle to message handler (fixes race condition)
        let _ = network_tx.send(Some(network.clone()));

        // Create network condition holder for simulation
        let network_condition = Arc::new(RwLock::new(None::<NetworkCondition>));

        // Set up gossip send callback with shutdown handling and network condition simulation
        {
            let mut g = gossip.write().await;
            let net = network.clone();
            let from = did.clone();
            let send_shutdown = shutdown_tx.clone();
            let condition = network_condition.clone();

            let send_callback = Arc::new(move |recipient: Option<Did>, msg: GossipMessage| {
                let net = net.clone();
                let from = from.clone();
                let mut shutdown_rx = send_shutdown.subscribe();
                let condition = condition.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = shutdown_rx.recv() => {
                            // Node shutting down, abort send
                        }
                        result = async {
                            // Apply network condition simulation
                            if let Some(cond) = condition.read().await.as_ref() {
                                // Simulate packet loss
                                if cond.packet_loss > 0.0 {
                                    let roll: f64 = rand::random();
                                    if roll < cond.packet_loss {
                                        debug!("Simulated packet loss: dropping gossip message");
                                        return Ok(());
                                    }
                                }

                                // Simulate latency with jitter
                                if cond.latency > Duration::ZERO {
                                    let jitter_factor: f64 = if cond.jitter > 0.0 {
                                        let jitter_roll: f64 = rand::random();
                                        // Symmetric distribution around 1.0
                                        1.0 - cond.jitter + (jitter_roll * 2.0 * cond.jitter)
                                    } else {
                                        1.0
                                    };
                                    let delay = cond.latency.mul_f64(jitter_factor.max(0.0));
                                    tokio::time::sleep(delay).await;
                                }
                            }

                            let net_msg = NetworkMessage::gossip(from, recipient.clone(), msg);
                            if let Some(to) = recipient {
                                net.send_message(to, net_msg).await
                            } else {
                                net.broadcast(net_msg).await
                            }
                        } => {
                            if let Err(e) = result {
                                debug!("Failed to send gossip: {}", e);
                            }
                        }
                    }
                });
            });

            g.set_send_callback(send_callback);
        }

        // Start periodic digest emitter so gossip entries propagate between
        // test nodes via the pull-based anti-entropy protocol (Digest -> PullRequest
        // -> PullResponse). Uses a short interval for fast convergence in tests.
        let _digest_handle = start_digest_emitter(
            gossip.clone(),
            200, // 200ms interval (production uses 10s+)
            0,   // no jitter for deterministic test timing
            shutdown_tx.subscribe(),
        );

        info!("Test node {} listening on {}", index, addr);

        // Spawn succeeded - disarm the cleanup guard
        cleanup_guard.disarm();

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
            network_condition,
        })
    }

    /// Shutdown the test node gracefully
    pub async fn shutdown(self) {
        info!("Shutting down test node {}", self.index);
        let _ = self.shutdown_tx.send(());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    /// Set network conditions for simulation
    ///
    /// This enables simulating various network conditions on outgoing gossip
    /// messages. Currently implemented:
    /// - **Latency**: Adds configurable delay before sending
    /// - **Packet loss**: Random message dropping based on probability
    /// - **Jitter**: Random variation of latency (symmetric around base latency)
    ///
    /// Note: `bandwidth_limit` in `NetworkCondition` is not yet implemented
    /// and will be ignored.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use icn_testkit::{NetworkCondition, TestNode};
    ///
    /// node.set_network_condition(NetworkCondition::poor_internet()).await;
    /// // Messages will now have ~200ms latency and 5% packet loss
    /// ```
    pub async fn set_network_condition(&self, condition: NetworkCondition) {
        let mut cond = self.network_condition.write().await;
        *cond = Some(condition);
    }

    /// Clear network conditions (return to normal operation)
    pub async fn clear_network_condition(&self) {
        let mut cond = self.network_condition.write().await;
        *cond = None;
    }

    /// Get the current network condition (if any)
    pub async fn get_network_condition(&self) -> Option<NetworkCondition> {
        self.network_condition.read().await.clone()
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
    ///
    /// Uses exponential backoff starting at 10ms, capped at 100ms between checks.
    pub async fn wait_connected(&self, did: &Did, timeout: Duration) -> Result<()> {
        let network = self.network.clone();
        let target_did = did.clone();

        poll_with_backoff(
            move || {
                let net = network.clone();
                let did = target_did.clone();
                async move { net.is_peer_connected(&did).await.unwrap_or(false) }
            },
            BackoffConfig::new(Duration::from_millis(10), Duration::from_millis(100)),
            timeout,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for connection to {did}"))
    }

    /// Disconnect from a specific peer
    ///
    /// This closes the QUIC connection to the peer. Returns true if a
    /// connection was found and closed.
    pub async fn disconnect(&self, other: &TestNode) -> bool {
        self.disconnect_did(&other.did).await
    }

    /// Disconnect from a peer by DID
    pub async fn disconnect_did(&self, did: &Did) -> bool {
        self.network
            .disconnect_peer(did, Some("test disconnect"))
            .await
    }

    /// Wait for disconnection from a specific peer
    ///
    /// Waits until the peer is no longer connected or the timeout expires.
    /// Uses exponential backoff starting at 10ms, capped at 100ms between checks.
    /// If the connection state cannot be determined (error), treats as disconnected
    /// since the peer handle may have been cleaned up.
    pub async fn wait_disconnected(&self, did: &Did, timeout: Duration) -> Result<()> {
        let network = self.network.clone();
        let target_did = did.clone();

        poll_with_backoff(
            move || {
                let net = network.clone();
                let did = target_did.clone();
                async move {
                    // Return true (done) if disconnected or if we can't determine state
                    match net.is_peer_connected(&did).await {
                        Ok(false) => true, // Confirmed disconnected
                        Ok(true) => false, // Still connected, keep waiting
                        Err(_) => true,    // Can't determine - treat as disconnected
                    }
                }
            },
            BackoffConfig::new(Duration::from_millis(10), Duration::from_millis(100)),
            timeout,
        )
        .await
        .map_err(|_| anyhow::anyhow!("Timeout waiting for disconnection from {did}"))
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

    #[test]
    fn test_shutdown_guard_triggers_on_drop() {
        let (tx, mut rx) = broadcast::channel(1);

        // Create guard and let it drop without disarming
        {
            let _guard = ShutdownGuard::new(tx, 42);
            // Guard drops here
        }

        // Should have received shutdown signal
        assert!(
            rx.try_recv().is_ok(),
            "ShutdownGuard should send shutdown signal on drop"
        );
    }

    #[test]
    fn test_shutdown_guard_no_trigger_when_disarmed() {
        let (tx, mut rx) = broadcast::channel(1);

        // Create guard and disarm before drop
        {
            let mut guard = ShutdownGuard::new(tx, 42);
            guard.disarm();
            // Guard drops here
        }

        // Should NOT have received shutdown signal
        assert!(
            rx.try_recv().is_err(),
            "Disarmed ShutdownGuard should not send shutdown signal"
        );
    }

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

    #[tokio::test]
    async fn test_set_network_condition() -> Result<()> {
        use crate::simulation::NetworkCondition;

        let node = TestNode::spawn(NodeConfig::default()).await?;

        // Initially no condition
        assert!(node.get_network_condition().await.is_none());

        // Set poor internet condition
        node.set_network_condition(NetworkCondition::poor_internet())
            .await;
        let cond = node.get_network_condition().await;
        assert!(cond.is_some());
        let cond = cond.unwrap();
        assert!(cond.latency >= Duration::from_millis(100));
        assert!(cond.packet_loss > 0.0);

        // Clear condition
        node.clear_network_condition().await;
        assert!(node.get_network_condition().await.is_none());

        node.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_network_condition_simulation_code_path() -> Result<()> {
        use crate::simulation::NetworkCondition;

        let node = TestNode::spawn(NodeConfig::default()).await?;
        node.create_topic("test:latency", AccessControl::Public)
            .await;

        // Verify condition is initially None
        assert!(
            node.get_network_condition().await.is_none(),
            "Should start with no network condition"
        );

        // Set a latency condition
        let condition = NetworkCondition::default()
            .with_latency(Duration::from_millis(100))
            .with_jitter(0.1)
            .with_packet_loss(0.0); // No packet loss for this test
        node.set_network_condition(condition).await;

        // Verify condition is set
        let retrieved = node.get_network_condition().await;
        assert!(retrieved.is_some(), "Network condition should be set");
        let cond = retrieved.as_ref().expect("should have condition");
        assert_eq!(cond.latency, Duration::from_millis(100));
        assert!((cond.jitter - 0.1).abs() < 0.001);

        // Test that latency calculation produces expected range
        // With 100ms base latency and 10% jitter, delay should be 90-110ms
        for _ in 0..10 {
            let jitter_roll: f64 = rand::random();
            let jitter_factor = 1.0 - cond.jitter + (jitter_roll * 2.0 * cond.jitter);
            let delay = cond.latency.mul_f64(jitter_factor);
            assert!(
                delay >= Duration::from_millis(90) && delay <= Duration::from_millis(110),
                "Delay {delay:?} should be in 90-110ms range"
            );
        }

        // Clear condition
        node.clear_network_condition().await;
        assert!(
            node.get_network_condition().await.is_none(),
            "Network condition should be cleared"
        );

        node.shutdown().await;
        Ok(())
    }

    /// Integration test for network condition with actual message delivery
    ///
    /// This test verifies that network conditions affect actual gossip delivery.
    #[tokio::test]
    async fn test_network_condition_latency_integration() -> Result<()> {
        use crate::simulation::NetworkCondition;
        use std::time::Instant;

        // Create two connected nodes - latency is applied when sending to peers
        let node_a = TestNode::spawn_with_index(NodeConfig::default(), 0).await?;
        let node_b = TestNode::spawn_with_index(NodeConfig::default(), 1).await?;

        node_a.connect(&node_b).await?;

        // Wait for connection to establish
        node_a
            .wait_connected(&node_b.did, Duration::from_secs(5))
            .await?;

        // Create topic on both nodes
        let topic = "test:latency";
        node_a.create_topic(topic, AccessControl::Public).await;
        node_b.create_topic(topic, AccessControl::Public).await;

        // Allow connection to stabilize
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Set a measurable latency condition (150ms) on node_a
        node_a
            .set_network_condition(
                NetworkCondition::default()
                    .with_latency(Duration::from_millis(150))
                    .with_jitter(0.0), // No jitter for predictable timing
            )
            .await;

        // Publish from node_a and measure time until node_b receives it
        let start = Instant::now();
        let hash = node_a.publish(topic, b"latency-test-message").await?;

        // Poll until node_b receives the notification
        let timeout = Duration::from_secs(5);
        let poll_interval = Duration::from_millis(10);
        let deadline = start + timeout;
        let mut received = false;

        while std::time::Instant::now() < deadline {
            if node_b.has_entry(topic, &hash).await {
                received = true;
                break;
            }
            tokio::time::sleep(poll_interval).await;
        }
        let delivery_time = start.elapsed();

        assert!(
            received,
            "Node B should have received the message within timeout"
        );

        // The delivery time should include at least the simulated latency (150ms)
        // Allow some margin for test infrastructure overhead
        assert!(
            delivery_time >= Duration::from_millis(100),
            "Expected delivery to take at least 100ms due to latency simulation, took {delivery_time:?}"
        );

        node_a.shutdown().await;
        node_b.shutdown().await;
        Ok(())
    }
}
