//! Supervisor for managing actors

use anyhow::Result;
use icn_gossip::GossipActor;
use icn_identity::KeyPair;
use icn_ledger::Ledger;
use icn_rpc::RpcServer;
use icn_store::SledStore;
use icn_trust::TrustClass;
use std::sync::Arc;
use tokio::select;
use tracing::{info, warn};

use crate::config::Config;
use crate::runtime::ShutdownTx;

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
        if let Err(e) = icn_obs::start_metrics_server(9090).await {
            warn!("Failed to start metrics server: {}", e);
        }

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn actors (requires keypair from unlocked keystore)
        let (network_handle, gossip_handle, ledger_handle) = if let Some(keypair) = &self.keypair {
            info!("Keypair available - spawning actors");

            let did = keypair.did().clone();

            // Spawn Gossip actor
            let trust_lookup = Arc::new(|_did: &icn_identity::Did| Some(TrustClass::Partner));
            let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

            info!("Gossip actor spawned");

            // Spawn Ledger
            let store_path = self.config.store_path().join("ledger");
            let store = Arc::new(SledStore::open(&store_path)?);
            let mut ledger = Ledger::new(store)?;
            ledger.set_gossip(gossip_handle.clone());
            let ledger_handle = Arc::new(tokio::sync::RwLock::new(ledger));

            info!("Ledger initialized at {}", store_path.display());

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
                        tokio::spawn(async move {
                            let mut gossip = gossip_handle.write().await;
                            if let Err(e) = gossip.handle_message(gossip_msg) {
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

            // Spawn RPC server with network handle
            let rpc_addr = "127.0.0.1:5050".parse()?;
            let mut rpc_server = RpcServer::new(rpc_addr);
            rpc_server.set_network_handle(network_handle.clone());

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
