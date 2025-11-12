//! Supervisor for managing actors

use anyhow::{bail, Context, Result};
use icn_gossip::GossipActor;
use icn_identity::{Did, KeyPair};
use icn_ledger::Ledger;
use icn_rpc::RpcServer;
use icn_store::SledStore;
use icn_trust::TrustClass;
use serde_json;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::select;
use tracing::{info, warn};

use crate::config::Config;
use crate::runtime::ShutdownTx;

/// Parse bootstrap peer URL in format: icn://did:icn:PUBKEY@IP:PORT
/// Returns (DID, SocketAddr) on success
///
/// Note: Currently only supports IP addresses. DNS hostname resolution can be added later.
fn parse_bootstrap_peer(url: &str) -> Result<(Did, SocketAddr)> {
    // Check for icn:// prefix
    let url = url.strip_prefix("icn://")
        .context("Bootstrap peer URL must start with 'icn://'")?;

    // Split on @ to get DID and address
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        bail!("Invalid bootstrap peer format, expected icn://DID@IP:PORT");
    }

    let did_str = parts[0];
    let addr_str = parts[1];

    // Parse DID
    let did: Did = serde_json::from_value(serde_json::Value::String(did_str.to_string()))
        .context("Failed to parse DID")?;

    // Parse socket address (IP:PORT)
    // Note: This requires an IP address, not a DNS hostname
    let addr: SocketAddr = addr_str.parse()
        .context("Failed to parse socket address (must be IP:PORT, DNS names not yet supported)")?;

    Ok((did, addr))
}

/// Supervisor manages all actors and restarts them on failure
pub struct Supervisor {
    config: Config,
    keypair: Option<KeyPair>,
    shutdown_tx: ShutdownTx,
}

impl Supervisor {
    /// Create a new supervisor
    pub fn new(config: Config, keypair: Option<KeyPair>, shutdown_tx: ShutdownTx) -> Self {
        Supervisor {
            config,
            keypair,
            shutdown_tx,
        }
    }

    /// Run the supervisor
    pub async fn run(self) -> Result<()> {
        info!("Supervisor starting");

        // Initialize metrics
        icn_obs::init_metrics()?;

        // Start metrics server
        let metrics_port = self.config.observability.metrics_port;
        if let Err(e) = icn_obs::start_metrics_server(metrics_port).await {
            warn!("Failed to start metrics server: {}", e);
        }

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn actors (requires keypair from unlocked keystore)
        let (network_handle, gossip_handle, ledger_handle) = if let Some(keypair) = &self.keypair {
            info!("Keypair available - spawning actors");

            let did = keypair.did().clone();

            // Create trust graph
            let trust_store_path = self.config.store_path().join("trust");
            let trust_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::open(&trust_store_path)?);
            let trust_graph = icn_trust::TrustGraph::new(trust_store, did.clone());
            let trust_graph_handle = Arc::new(tokio::sync::RwLock::new(trust_graph));

            info!("Trust graph initialized at {}", trust_store_path.display());

            // Create trust lookup closure for gossip actor
            let trust_graph_for_gossip = trust_graph_handle.clone();
            let trust_lookup = Arc::new(move |peer_did: &icn_identity::Did| {
                // Use a blocking task since we're in a sync context
                let graph = trust_graph_for_gossip.clone();
                let peer = peer_did.clone();
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mut graph = graph.write().await;
                        graph.trust_class(&peer).ok()
                    })
                })
            });

            // Spawn Gossip actor
            let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

            info!("Gossip actor spawned");

            // Spawn Ledger
            let store_path = self.config.store_path().join("ledger");
            let store = Arc::new(SledStore::open(&store_path)?);
            let mut ledger = Ledger::new(store)?;
            ledger.set_gossip(gossip_handle.clone());
            let ledger_handle = Arc::new(tokio::sync::RwLock::new(ledger));

            info!("Ledger initialized at {}", store_path.display());

            // Initialize Contract Runtime
            let contract_runtime = icn_ccl::ContractRuntime::new(ledger_handle.clone());
            let contract_runtime_handle = Arc::new(tokio::sync::RwLock::new(contract_runtime));

            info!("Contract runtime initialized");

            // TODO: Spawn Identity actor
            // let identity_handle = IdentityActor::spawn(
            //     self.config.keystore_path(),
            //     self.config.store_path(),
            //     keypair.clone(),
            //     self.shutdown_tx.clone()
            // )?;

            // Spawn Network actor with gossip bridge
            let listen_addr: std::net::SocketAddr = self.config.network.listen_addr.parse()?;

            // Create incoming message handler that routes to gossip
            let gossip_handle_clone = gossip_handle.clone();
            let network_handle_for_handler = Arc::new(tokio::sync::RwLock::new(None::<icn_net::NetworkHandle>));
            let network_handle_for_handler_clone = network_handle_for_handler.clone();
            let own_did_for_handler = did.clone();

            let incoming_handler: icn_net::IncomingMessageHandler = Arc::new(move |net_msg| {
                let sender_did = net_msg.from.clone();

                match net_msg.payload {
                    icn_net::MessagePayload::Gossip(gossip_msg) => {
                        // Spawn async task to avoid blocking the callback thread
                        let gossip_handle = gossip_handle_clone.clone();
                        let sender = sender_did.clone();
                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                                warn!("Failed to handle gossip message: {}", e);
                            }
                        });
                    }

                    icn_net::MessagePayload::Subscribe { topics } => {
                        info!("Received Subscribe from {} for topics: {:?}", sender_did, topics);
                        icn_obs::metrics::gossip::subscribes_received_inc();

                        // Spawn async task to avoid blocking the callback thread
                        let gossip_handle = gossip_handle_clone.clone();
                        let network_handle_lock = network_handle_for_handler_clone.clone();
                        let own_did = own_did_for_handler.clone();

                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            let mut acked_topics = Vec::new();

                            for topic in &topics {
                                match gossip.subscribe(&topic, sender_did.clone()) {
                                    Ok(_) => {
                                        info!("Subscribed {} to topic: {}", sender_did, topic);
                                        acked_topics.push(topic.clone());
                                    }
                                    Err(e) => {
                                        warn!("Failed to subscribe {} to topic {}: {}", sender_did, topic, e);
                                    }
                                }
                            }

                            // Send SubscribeAck back if we have any successful subscriptions
                            if !acked_topics.is_empty() {
                                icn_obs::metrics::gossip::subscribe_acks_sent_inc();
                                if let Some(net_handle) = network_handle_lock.read().await.as_ref() {
                                    let ack_msg = icn_net::NetworkMessage::subscribe_ack(
                                        own_did,
                                        sender_did.clone(),
                                        acked_topics
                                    );
                                    if let Err(e) = net_handle.send_message(sender_did, ack_msg).await {
                                        warn!("Failed to send SubscribeAck: {}", e);
                                    }
                                }
                            }
                        });
                    }

                    icn_net::MessagePayload::Unsubscribe { topics } => {
                        info!("Received Unsubscribe from {} for topics: {:?}", sender_did, topics);
                        icn_obs::metrics::gossip::unsubscribes_received_inc();

                        // Spawn async task to avoid blocking the callback thread
                        let gossip_handle = gossip_handle_clone.clone();
                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            for topic in &topics {
                                match gossip.unsubscribe(&topic, &sender_did) {
                                    Ok(_) => {
                                        info!("Unsubscribed {} from topic: {}", sender_did, topic);
                                    }
                                    Err(e) => {
                                        warn!("Failed to unsubscribe {} from topic {}: {}", sender_did, topic, e);
                                    }
                                }
                            }
                        });
                    }

                    icn_net::MessagePayload::SubscribeAck { topics } => {
                        info!("Received SubscribeAck from {} for topics: {:?}", sender_did, topics);
                        // Track successful subscription acknowledgment
                        // In a full implementation, this could update a local subscription registry
                    }

                    icn_net::MessagePayload::Ping => {
                        // Ping/Pong handled by network actor
                    }

                    icn_net::MessagePayload::Pong => {
                        // Ping/Pong handled by network actor
                    }
                }
            });

            let network_handle = icn_net::NetworkActor::spawn(
                keypair,
                listen_addr,
                self.shutdown_tx.clone(),
                Some(incoming_handler),
                Some(trust_graph_handle.clone()), // Enable trust-gated rate limiting
            )
            .await?;

            // Initialize network handle for the incoming message handler
            *network_handle_for_handler.write().await = Some(network_handle.clone());

            info!("Network actor spawned on {}", listen_addr);

            // Set send callback on gossip actor to enable request/response
            {
                let mut gossip = gossip_handle.write().await;
                let network_handle_clone = network_handle.clone();
                let own_did_clone = did.clone();

                let send_callback: icn_gossip::SendMessageCallback = Arc::new(move |recipient, gossip_msg| {
                    let net_handle = network_handle_clone.clone();
                    let from_did = own_did_clone.clone();

                    // Track metrics based on message type
                    use icn_gossip::GossipMessage;
                    match &gossip_msg {
                        GossipMessage::Announce { .. } => icn_obs::metrics::gossip::announces_sent_inc(),
                        GossipMessage::Request { .. } => icn_obs::metrics::gossip::requests_sent_inc(),
                        GossipMessage::Response { .. } => icn_obs::metrics::gossip::responses_sent_inc(),
                        _ => {} // Other message types
                    }

                    // Spawn async task to send message
                    tokio::spawn(async move {
                        let result = if let Some(target_did) = recipient {
                            // Unicast
                            let net_msg = icn_net::NetworkMessage::gossip(from_did, Some(target_did.clone()), gossip_msg);
                            net_handle.send_message(target_did, net_msg).await
                        } else {
                            // Broadcast
                            let net_msg = icn_net::NetworkMessage::gossip(from_did, None, gossip_msg);
                            net_handle.broadcast(net_msg).await
                        };

                        if let Err(e) = result {
                            warn!("Failed to send gossip message: {}", e);
                        }
                    });
                });

                gossip.set_send_callback(send_callback);
            }

            info!("Gossip send callback configured");

            // Dial bootstrap peers for WAN connectivity
            if !self.config.network.bootstrap_peers.is_empty() {
                info!("Dialing {} bootstrap peers", self.config.network.bootstrap_peers.len());
                for peer_url in &self.config.network.bootstrap_peers {
                    match parse_bootstrap_peer(peer_url) {
                        Ok((peer_did, peer_addr)) => {
                            info!("Connecting to bootstrap peer: {} at {}", peer_did, peer_addr);
                            match network_handle.dial(peer_addr, peer_did.clone()).await {
                                Ok(_) => info!("✓ Connected to bootstrap peer: {}", peer_did),
                                Err(e) => warn!("Failed to connect to bootstrap peer {}: {}", peer_did, e),
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse bootstrap peer URL '{}': {}", peer_url, e);
                        }
                    }
                }
            }

            // Spawn RPC server with network, ledger, and contract handles
            let rpc_port = self.config.network.rpc_port;
            let rpc_addr = format!("127.0.0.1:{}", rpc_port).parse()?;
            let mut rpc_server = RpcServer::new(rpc_addr);
            rpc_server.set_network_handle(network_handle.clone());
            rpc_server.set_ledger_handle(ledger_handle.clone());
            rpc_server.set_contract_runtime(contract_runtime_handle.clone());

            tokio::spawn(async move {
                if let Err(e) = rpc_server.run().await {
                    warn!("RPC server error: {}", e);
                }
            });

            info!("RPC server spawned on {}", rpc_addr);

            // Spawn anti-entropy task
            let anti_entropy_config = crate::anti_entropy::AntiEntropyConfig::default();
            let _anti_entropy_handle = crate::anti_entropy::spawn_anti_entropy_task(
                gossip_handle.clone(),
                network_handle.clone(),
                did.clone(),
                anti_entropy_config,
                self.shutdown_tx.subscribe(),
            );

            info!("Anti-entropy task spawned");

            // Start periodic digest emitter
            let _digest_emitter_handle = icn_gossip::start_digest_emitter(
                gossip_handle.clone(),
                10_000, // 10 seconds base interval
                2_000,  // ±2 seconds jitter
                self.shutdown_tx.subscribe(),
            );

            info!("Digest emitter spawned");

            // Spawn metrics update task
            let start_time = std::time::Instant::now();
            let network_handle_metrics = network_handle.clone();
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Update uptime
                            let uptime_secs = start_time.elapsed().as_secs();
                            icn_obs::metrics::system::uptime_seconds_set(uptime_secs);

                            // Count active actors (network + gossip + ledger + rpc + anti-entropy + digest-emitter = 6)
                            icn_obs::metrics::system::actors_active_set(6);

                            // Update network stats (this also updates metrics via GetStats handler)
                            let _ = network_handle_metrics.get_stats().await;
                        }
                        _ = metrics_shutdown.recv() => {
                            break;
                        }
                    }
                }
            });

            info!("Metrics update task spawned");

            (Some(network_handle), Some(gossip_handle), Some(ledger_handle))
        } else {
            warn!("No keypair available - actors not spawned");
            warn!("Run 'icnctl id init' to create an identity");

            // Still spawn metrics update task for system metrics
            let start_time = std::time::Instant::now();
            let mut metrics_shutdown = self.shutdown_tx.subscribe();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            // Update system metrics even without actors
                            let uptime_secs = start_time.elapsed().as_secs();
                            icn_obs::metrics::system::uptime_seconds_set(uptime_secs);
                            icn_obs::metrics::system::actors_active_set(0);
                        }
                        _ = metrics_shutdown.recv() => {
                            break;
                        }
                    }
                }
            });

            info!("Metrics update task spawned (system metrics only)");

            (None, None, None)
        };

        // Wait for shutdown signal
        select! {
            _ = shutdown_rx.recv() => {
                info!("Supervisor received shutdown signal");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Supervisor received Ctrl+C");
                let _ = self.shutdown_tx.send(());
            }
        }

        // Graceful shutdown of actors
        info!("Supervisor shutting down actors");

        // Network actor will shut down gracefully via the shutdown signal
        // The actor's run loop listens for shutdown_rx and cleans up properly
        if network_handle.is_some() {
            info!("Network actor will shut down via shutdown signal");
        }

        // Gossip and Ledger are wrapped in Arc<RwLock> and will be dropped when
        // all references are released
        if gossip_handle.is_some() {
            info!("Gossip actor will be dropped when all references are released");
        }
        if ledger_handle.is_some() {
            info!("Ledger will be dropped when all references are released");
        }

        info!("Supervisor stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bootstrap_peer_valid() {
        let url = "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
        assert_eq!(addr.to_string(), "203.0.113.50:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_ipv4() {
        let url = "icn://did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH@192.168.1.100:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_ok());

        let (did, addr) = result.unwrap();
        assert_eq!(did.as_str(), "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH");
        assert_eq!(addr.to_string(), "192.168.1.100:7777");
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_prefix() {
        let url = "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must start with 'icn://'"));
    }

    #[test]
    fn test_parse_bootstrap_peer_missing_at() {
        let url = "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK_203.0.113.50:7777";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("expected icn://DID@IP:PORT"));
    }

    #[test]
    fn test_parse_bootstrap_peer_invalid_port() {
        let url = "icn://did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK@203.0.113.50:invalid";
        let result = parse_bootstrap_peer(url);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse socket address"));
    }
}
