/*!
# ICN Federation System

This crate implements the federation system for the ICN Runtime, including federation
sync, quorum, guardian mandates, and blob replication policies.

## Architectural Tenets
- Federation = protocol mesh (libp2p) for trust replay, quorum negotiation, epoch anchoring
- Guardians = mandate-bound, quorum-signed constitutional interventions
- TrustBundles for federation state synchronization
*/

use std::collections::HashMap;
use std::time::Duration;
use futures::StreamExt;
use futures::lock::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use std::sync::Arc;

use libp2p::{
    core::transport::upgrade,
    gossipsub, identity, kad, mdns, noise, request_response, tcp, yamux,
    PeerId, Swarm, Transport, Multiaddr, swarm::SwarmEvent, swarm::Config,
};

use icn_dag::DagNode;
use icn_identity::{
    IdentityId, IdentityScope, TrustBundle, 
    QuorumProof, QuorumConfig
};
use icn_storage::{FederationCommand, ReplicationPolicy};
use multihash::{self, MultihashDigest};
use tracing::{debug, info, error, warn};
use thiserror::Error;

// Export network module
pub mod network;

// Export signing module
pub mod signing;

// Export roles module
pub mod roles;

// Export replication module
pub mod replication;

/// Errors that can occur during federation operations
#[derive(Debug, Error)]
pub enum FederationError {
    #[error("Invalid guardian mandate: {0}")]
    InvalidMandate(String),
    
    #[error("Quorum not reached: {0}")]
    QuorumNotReached(String),
    
    #[error("Sync failed: {0}")]
    SyncFailed(String),
    
    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Transport error: {0}")]
    TransportError(String),
    
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Configuration not found: {0}")]
    ConfigNotFound(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for federation operations
pub type FederationResult<T> = Result<T, FederationError>;

/// Represents a guardian mandate
#[derive(Debug, Clone)]
pub struct GuardianMandate {
    /// The scope of this mandate
    pub scope: IdentityScope,
    
    /// The identifier of the scope
    pub scope_id: IdentityId,
    
    /// The action to be taken
    pub action: String,
    
    /// The reason for this mandate
    pub reason: String,
    
    /// The guardian issuing this mandate
    pub guardian: IdentityId,
    
    /// The quorum proof
    pub quorum_proof: QuorumProof,
    
    /// The DAG node representing this mandate
    pub dag_node: DagNode,
}

impl GuardianMandate {
    /// Create a new guardian mandate
    pub fn new(
        scope: IdentityScope,
        scope_id: IdentityId,
        action: String,
        reason: String,
        guardian: IdentityId,
        quorum_proof: QuorumProof,
        dag_node: DagNode,
    ) -> Self {
        Self {
            scope,
            scope_id,
            action,
            reason,
            guardian,
            quorum_proof,
            dag_node,
        }
    }
    
    /// Verify this mandate
    pub async fn verify(&self, storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>) -> FederationResult<bool> {
        // Recalculate the mandate content hash
        let mandate_hash = signing::calculate_mandate_hash(
            &self.action, 
            &self.reason, 
            &self.scope, 
            &self.scope_id, 
            &self.guardian
        );
        
        // Look up authorized guardians for this mandate's scope
        let authorized_guardians = roles::get_authorized_guardians(
            self.scope_id.0.as_str(), 
            Arc::clone(&storage)
        ).await?;
        
        if authorized_guardians.is_empty() {
            warn!("No authorized guardians found for scope: {}", self.scope_id.0);
        }
        
        // Verify the quorum proof against the authorized guardians
        self.quorum_proof.verify(&mandate_hash, &authorized_guardians).await
            .map_err(|e| FederationError::InvalidMandate(e.to_string()))
    }
}

/// Messages that can be sent to the federation manager
#[derive(Debug)]
pub enum FederationManagerMessage {
    /// Request a trust bundle from the network
    RequestTrustBundle {
        epoch: u64,
        respond_to: tokio::sync::oneshot::Sender<FederationResult<Option<TrustBundle>>>,
    },
    /// Publish a trust bundle to the network
    PublishTrustBundle {
        bundle: TrustBundle,
        respond_to: tokio::sync::oneshot::Sender<FederationResult<()>>,
    },
    /// Announce a blob (by CID) as a provider on the Kademlia DHT
    AnnounceBlob {
        cid: cid::Cid,
        respond_to: Option<tokio::sync::oneshot::Sender<FederationResult<()>>>,
    },
    /// Identify potential replication targets for a blob
    IdentifyReplicationTargets {
        cid: cid::Cid,
        policy: ReplicationPolicy,
        context_id: Option<String>,
        respond_to: Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>,
    },
    /// Stop the federation manager
    Shutdown {
        respond_to: tokio::sync::oneshot::Sender<()>,
    },
}

/// Configuration for the federation manager
#[derive(Debug, Clone)]
pub struct FederationManagerConfig {
    /// Bootstrap peers to connect to
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Listen addresses
    pub listen_addresses: Vec<Multiaddr>,
    /// Gossipsub heartbeat interval
    pub gossipsub_heartbeat_interval: Duration,
    /// Gossipsub validation mode
    pub gossipsub_validation_mode: gossipsub::ValidationMode,
}

impl Default for FederationManagerConfig {
    fn default() -> Self {
        Self {
            bootstrap_peers: Vec::new(),
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".parse().unwrap()],
            gossipsub_heartbeat_interval: Duration::from_secs(1),
            gossipsub_validation_mode: gossipsub::ValidationMode::Strict,
        }
    }
}

/// Manages federation network operations
#[allow(dead_code)]
pub struct FederationManager {
    /// Local peer ID
    pub local_peer_id: PeerId,
    /// Local keypair
    keypair: identity::Keypair,
    /// Channel for sending messages to the event loop
    sender: mpsc::Sender<FederationManagerMessage>,
    /// Event loop task handle
    _event_loop_handle: JoinHandle<()>,
    /// Known peers
    known_peers: HashMap<PeerId, Multiaddr>,
    /// Configuration
    config: FederationManagerConfig,
    /// Storage backend for storing TrustBundles
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
}

impl FederationManager {
    /// Start a new federation node
    pub async fn start_node(
        config: FederationManagerConfig, 
        storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    ) -> FederationResult<(Self, mpsc::Sender<cid::Cid>, mpsc::Sender<FederationCommand>)> {
        // Generate a new local keypair
        let keypair = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());
        
        info!("Starting federation node with peer ID: {}", local_peer_id);
        
        // Create the transport
        let transport = tcp::tokio::Transport::default()
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::Config::new(&keypair).expect("Failed to create noise config"))
            .multiplex(yamux::Config::default())
            .boxed();
        
        // Create the network behavior
        let behavior = network::create_behaviour(local_peer_id, keypair.clone())
            .map_err(|e| FederationError::ProtocolError(format!("Failed to create behavior: {}", e)))?;
        
        // Create the libp2p swarm
        let mut swarm = Swarm::new(
            transport,
            behavior,
            local_peer_id,
            Config::with_tokio_executor(),
        );
        
        // Listen on the configured addresses
        for addr in &config.listen_addresses {
            swarm.listen_on(addr.clone())
                .map_err(|e| FederationError::NetworkError(format!("Failed to listen on {}: {}", addr, e)))?;
        }
        
        // Connect to bootstrap peers if provided
        for peer_addr in &config.bootstrap_peers {
            info!("Dialing bootstrap peer: {}", peer_addr);
            swarm.dial(peer_addr.clone())
                .map_err(|e| FederationError::ConnectionError(format!("Failed to dial {}: {}", peer_addr, e)))?;
        }
        
        // Prepare channels for communication with the event loop
        let (sender, receiver) = mpsc::channel(100);
        
        // Create a channel for Kademlia blob announcements
        let (blob_sender, blob_receiver) = mpsc::channel::<cid::Cid>(100);
        
        // Create a channel for federation commands from storage layer
        let (fed_cmd_sender, fed_cmd_receiver) = mpsc::channel::<FederationCommand>(100);
        
        // Clone storage for event loop
        let event_loop_storage = storage.clone();
        
        // Spawn the event loop
        let event_loop_handle = tokio::spawn(run_event_loop(
            swarm, 
            receiver, 
            event_loop_storage, 
            blob_receiver,
            fed_cmd_receiver
        ));
        
        // Create sync command sender clone
        let sync_sender = sender.clone();
        
        // Spawn the periodic sync task
        tokio::spawn(async move {
            let sync_interval = Duration::from_secs(60); // Sync every 60 seconds
            
            loop {
                tokio::time::sleep(sync_interval).await;
                debug!("Running periodic TrustBundle sync task");
                
                // Request latest TrustBundle
                // For now, we just request the latest epoch we know of + 1
                // TODO(V3-MVP): Add a more sophisticated way to track latest epochs
                let latest_known_epoch = get_latest_known_epoch().await;
                let next_epoch = latest_known_epoch + 1;
                
                if let Err(e) = request_trust_bundle_from_network(sync_sender.clone(), next_epoch).await {
                    error!("Periodic TrustBundle sync failed: {}", e);
                }
            }
        });
        
        let manager = Self {
            local_peer_id,
            keypair,
            sender,
            _event_loop_handle: event_loop_handle,
            known_peers: HashMap::new(),
            config,
            storage,
        };
        
        Ok((manager, blob_sender, fed_cmd_sender))
    }
    
    /// Request a trust bundle from the network
    pub async fn request_trust_bundle(&self, epoch: u64) -> FederationResult<Option<TrustBundle>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.sender.send(FederationManagerMessage::RequestTrustBundle {
            epoch,
            respond_to: tx,
        }).await
        .map_err(|e| FederationError::NetworkError(format!("Failed to send request: {}", e)))?;
        
        rx.await
            .map_err(|e| FederationError::NetworkError(format!("Failed to receive response: {}", e)))?
    }
    
    /// Publish a trust bundle to the network
    pub async fn publish_trust_bundle(&self, bundle: TrustBundle) -> FederationResult<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.sender.send(FederationManagerMessage::PublishTrustBundle {
            bundle,
            respond_to: tx,
        }).await
        .map_err(|e| FederationError::NetworkError(format!("Failed to send request: {}", e)))?;
        
        rx.await
            .map_err(|e| FederationError::NetworkError(format!("Failed to receive response: {}", e)))?
    }
    
    /// Shutdown the federation manager
    pub async fn shutdown(self) -> FederationResult<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.sender.send(FederationManagerMessage::Shutdown {
            respond_to: tx,
        }).await
        .map_err(|e| FederationError::NetworkError(format!("Failed to send shutdown request: {}", e)))?;
        
        rx.await
            .map_err(|e| FederationError::NetworkError(format!("Failed to receive shutdown confirmation: {}", e)))?;
        
        Ok(())
    }
    
    /// Get the latest known epoch from storage
    pub async fn get_latest_known_epoch(&self) -> FederationResult<u64> {
        // For MVP, just return 0 as a placeholder
        // TODO(V3-MVP): Implement proper epoch tracking
        Ok(0)
    }
    
    /// Announce blob as a provider on the Kademlia DHT
    pub async fn announce_blob(&self, cid: cid::Cid) -> FederationResult<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.sender.send(FederationManagerMessage::AnnounceBlob {
            cid,
            respond_to: Some(tx),
        }).await
        .map_err(|e| FederationError::NetworkError(format!("Failed to send announcement request: {}", e)))?;
        
        rx.await
            .map_err(|e| FederationError::NetworkError(format!("Failed to receive announcement confirmation: {}", e)))?
    }
    
    /// Identify potential replication targets for a blob
    pub async fn identify_replication_targets(
        &self, 
        cid: cid::Cid,
        policy: ReplicationPolicy,
        context_id: Option<String>
    ) -> FederationResult<Vec<PeerId>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.sender.send(FederationManagerMessage::IdentifyReplicationTargets {
            cid,
            policy,
            context_id,
            respond_to: Some(tx),
        }).await
        .map_err(|e| FederationError::NetworkError(format!("Failed to send replication target request: {}", e)))?;
        
        rx.await
            .map_err(|e| FederationError::NetworkError(format!("Failed to receive replication targets: {}", e)))?
    }
}

/// Run the event loop for the federation network
async fn run_event_loop(
    mut swarm: Swarm<network::IcnFederationBehaviour>,
    mut command_receiver: mpsc::Receiver<FederationManagerMessage>,
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    mut blob_receiver: mpsc::Receiver<cid::Cid>,
    mut fed_cmd_receiver: mpsc::Receiver<FederationCommand>,
) {
    info!("Starting federation network event loop");
    
    // Track active Kademlia queries to handle responses
    let mut active_replication_queries: HashMap<kad::QueryId, (cid::Cid, ReplicationPolicy, Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>)> = HashMap::new();
    
    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                handle_swarm_event(&mut swarm, event, storage.clone(), &mut active_replication_queries).await;
            },
            command = command_receiver.recv() => {
                match command {
                    Some(FederationManagerMessage::RequestTrustBundle { epoch, respond_to }) => {
                        debug!("Received request to fetch trust bundle for epoch {}", epoch);
                        
                        // Generate a key for the requested TrustBundle based on epoch
                        let key_str = format!("trustbundle::epoch::{}", epoch);
                        let key_hash = multihash::Code::Sha2_256.digest(key_str.as_bytes());
                        let _key_cid = cid::Cid::new_v1(0x71, key_hash); // Raw codec
                        
                        // First check if we have it locally
                        let storage_lock = storage.lock().await;
                        let local_result = storage_lock.get(&_key_cid).await;
                        drop(storage_lock);
                        
                        match local_result {
                            Ok(Some(bundle_bytes)) => {
                                // We have it locally, deserialize and return
                                match serde_json::from_slice::<TrustBundle>(&bundle_bytes) {
                                    Ok(bundle) => {
                                        debug!("Found TrustBundle for epoch {} locally", epoch);
                                        let _ = respond_to.send(Ok(Some(bundle)));
                                    },
                                    Err(e) => {
                                        error!("Failed to deserialize local TrustBundle: {}", e);
                                        let _ = respond_to.send(Err(FederationError::SyncFailed(
                                            format!("Failed to deserialize local TrustBundle: {}", e)
                                        )));
                                    }
                                }
                            },
                            Ok(None) | Err(_) => {
                                // We don't have it locally, request from peers
                                debug!("TrustBundle for epoch {} not found locally, requesting from peers", epoch);
                                
                                // For MVP, just return None as we haven't properly implemented
                                // peer discovery or request/response handling yet
                                debug!("Peer request not fully implemented - returning None for now");
                        let _ = respond_to.send(Ok(None));
                                
                                // TODO(V3-MVP): Implement peer selection and await response properly
                            }
                        }
                    },
                    Some(FederationManagerMessage::PublishTrustBundle { bundle, respond_to }) => {
                        debug!("Received request to publish trust bundle for epoch {}", bundle.epoch_id);
                        // TODO(V3-MVP): Implement actual trust bundle publication via gossipsub
                        let _ = respond_to.send(Ok(()));
                    },
                    Some(FederationManagerMessage::AnnounceBlob { cid, respond_to }) => {
                        debug!(%cid, "Announcing as provider for blob via Kademlia");
                        match announce_as_provider(&mut swarm, cid).await {
                            Ok(_) => {
                                debug!(%cid, "Successfully started Kademlia provider announcement");
                                if let Some(tx) = respond_to {
                                    let _ = tx.send(Ok(()));
                                }
                            },
                            Err(e) => {
                                error!(%cid, "Failed to announce as provider: {}", e);
                                if let Some(tx) = respond_to {
                                    let _ = tx.send(Err(e));
                                }
                            }
                        }
                    },
                    Some(FederationManagerMessage::IdentifyReplicationTargets { cid, policy, context_id, respond_to }) => {
                        debug!(%cid, ?policy, "Identifying replication targets for blob");
                        
                        // If a context ID was provided, look up the policy
                        let actual_policy = if let Some(context_id) = context_id {
                            match roles::get_replication_policy(&context_id, Arc::clone(&storage)).await {
                                Ok(policy) => {
                                    debug!(%cid, ?policy, context_id, "Retrieved replication policy from governance config");
                                    policy
                                },
                                Err(e) => {
                                    error!(%cid, context_id, "Failed to get replication policy: {}", e);
                                    // Fall back to the provided policy
                                    policy
                                }
                            }
                        } else {
                            policy
                        };
                        
                        // Proceed based on the policy
                        match actual_policy {
                            ReplicationPolicy::None => {
                                debug!(%cid, "Replication policy is None, not identifying targets");
                                if let Some(tx) = respond_to {
                                    let _ = tx.send(Ok(Vec::new()));
                                }
                            },
                            ReplicationPolicy::Factor(_) | ReplicationPolicy::Peers(_) => {
                                // Start a Kademlia query to find closest peers
                                let query_id = swarm.behaviour_mut().kademlia.get_closest_peers(cid.to_bytes());
                                debug!(%cid, ?query_id, "Started Kademlia query for closest peers");
                                // Store the query ID and associated information
                                active_replication_queries.insert(query_id, (cid, actual_policy, respond_to));
                            }
                        }
                    },
                    Some(FederationManagerMessage::Shutdown { respond_to }) => {
                        info!("Shutting down federation network event loop");
                        let _ = respond_to.send(());
                        break;
                    },
                    None => {
                        info!("Command channel closed, shutting down event loop");
                        break;
                    }
                }
            },
            maybe_cid = blob_receiver.recv() => {
                if let Some(cid) = maybe_cid {
                    debug!(%cid, "Received blob announcement from storage layer");
                    match announce_as_provider(&mut swarm, cid).await {
                        Ok(_) => debug!(%cid, "Successfully started Kademlia provider announcement from direct channel"),
                        Err(e) => error!(%cid, "Failed to announce as provider from direct channel: {}", e)
                    }
                }
            },
            maybe_cmd = fed_cmd_receiver.recv() => {
                if let Some(cmd) = maybe_cmd {
                    match cmd {
                        FederationCommand::AnnounceBlob(cid) => {
                            debug!(%cid, "Received blob announcement command from federation");
                            match announce_as_provider(&mut swarm, cid).await {
                                Ok(_) => debug!(%cid, "Successfully started Kademlia provider announcement from federation command"),
                                Err(e) => error!(%cid, "Failed to announce as provider from federation command: {}", e)
                            }
                        },
                        FederationCommand::IdentifyReplicationTargets { cid, policy, context_id } => {
                            debug!(%cid, ?policy, "Received replication target identification command");
                            
                            // Convert to internal message format and handle
                            let query_id = swarm.behaviour_mut().kademlia.get_closest_peers(cid.to_bytes());
                            debug!(%cid, ?query_id, "Started Kademlia query for closest peers from federation command");
                            // Store the query ID and associated information
                            active_replication_queries.insert(query_id, (cid, policy, None));
                        }
                    }
                }
            }
        }
    }
    
    info!("Federation network event loop terminated");
}

/// Handle swarm events
async fn handle_swarm_event(
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    event: SwarmEvent<network::IcnFederationBehaviourEvent>,
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    active_replication_queries: &mut HashMap<kad::QueryId, (cid::Cid, ReplicationPolicy, Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>)>,
) {
    match event {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!("Listening on {:?}", address);
        },
        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
            info!("Connection established to {} via {:?}", peer_id, endpoint.get_remote_address());
        },
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            info!("Connection closed to {}", peer_id);
        },
        SwarmEvent::IncomingConnection { local_addr, .. } => {
            debug!("Incoming connection on local address: {}", local_addr);
        },
        SwarmEvent::Behaviour(behavior_event) => {
            handle_behavior_event(swarm, behavior_event, storage, active_replication_queries).await;
        },
        _ => {}
    }
}

/// Handle behavior events
async fn handle_behavior_event(
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    event: network::IcnFederationBehaviourEvent,
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    active_replication_queries: &mut HashMap<kad::QueryId, (cid::Cid, ReplicationPolicy, Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>)>,
) {
    match event {
        network::IcnFederationBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
            for (peer, addr) in peers {
                info!("mDNS discovered a new peer: {} at {}", peer, addr);
                swarm.behaviour_mut().kademlia.add_address(&peer, addr.clone());
            }
        },
        network::IcnFederationBehaviourEvent::Identify(libp2p::identify::Event::Received { peer_id, info, .. }) => {
            info!("Identified peer {}: {} with addresses: {:?}", peer_id, info.agent_version, info.listen_addrs);
            
            // Add the peer's addresses to Kademlia
            for addr in info.listen_addrs {
                swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
            }
        },
        network::IcnFederationBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { 
            id, 
            result: kad::QueryResult::GetClosestPeers(Ok(closest_peers)), 
            .. 
        }) => {
            info!("Found {} closest peers", closest_peers.peers.len());
            
            // Check if this is a replication query we're tracking
            if let Some((cid, policy, respond_to)) = active_replication_queries.remove(&id) {
                // Use the replication module to identify target peers
                let local_peer_id = swarm.local_peer_id().clone();
                let target_peers = replication::identify_target_peers(
                    &cid, 
                    &policy, 
                    closest_peers.peers, 
                    &local_peer_id
                ).await;
                
                info!(%cid, ?policy, count = target_peers.len(), "Identified potential replication targets");
                
                // If we have a response channel, send back the list of targets
                if let Some(tx) = respond_to {
                    let _ = tx.send(Ok(target_peers.clone()));
                }
                
                // Initiate replication to the target peers
                if let Err(e) = replication::replicate_to_peers(&cid, &target_peers, swarm).await {
                    error!(%cid, "Failed to replicate blob: {}", e);
                }
            } else {
                // General closest peers query not related to replication
                for peer in closest_peers.peers {
                    debug!("Found closest peer: {}", peer);
                }
            }
        },
        network::IcnFederationBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { 
            id, 
            result: kad::QueryResult::GetClosestPeers(Err(err)), 
            .. 
        }) => {
            error!("GetClosestPeers query failed: {}", err);
            
            // Handle failure for replication queries
            if let Some((cid, _, respond_to)) = active_replication_queries.remove(&id) {
                error!(%cid, "Failed to identify replication targets: {}", err);
                
                if let Some(tx) = respond_to {
                    let _ = tx.send(Err(FederationError::NetworkError(
                        format!("Failed to identify replication targets: {}", err)
                    )));
                }
            }
        },
        network::IcnFederationBehaviourEvent::Gossipsub(gossipsub::Event::Message { 
            propagation_source, 
            message_id, 
            message: _ 
        }) => {
            debug!("Received gossipsub message from {} with id: {}", propagation_source, message_id);
            // TODO(V3-MVP): Parse message and take appropriate action based on topic/content
        },
        network::IcnFederationBehaviourEvent::TrustBundleSync(request_response::Event::Message { 
            peer, 
            message: request_response::Message::Request { request, channel, .. },
            ..
        }) => {
            debug!("Received TrustBundle request from {} for epoch {}", peer, request.epoch);
            
            // Generate a key for the requested TrustBundle based on epoch
            let key_str = format!("trustbundle::epoch::{}", request.epoch);
            let key_hash = multihash::Code::Sha2_256.digest(key_str.as_bytes());
            let _key_cid = cid::Cid::new_v1(0x71, key_hash); // Raw codec
            
            // Attempt to retrieve the TrustBundle from storage
            let storage_lock = storage.lock().await;
            let bundle_result = storage_lock.get(&_key_cid).await;
            drop(storage_lock); // Release lock as soon as possible
            
            let response = match bundle_result {
                Ok(Some(bundle_bytes)) => {
                    // Try to deserialize the TrustBundle
                    match serde_json::from_slice::<TrustBundle>(&bundle_bytes) {
                        Ok(bundle) => {
                            info!("Found TrustBundle for epoch {} in storage", request.epoch);
                            network::TrustBundleResponse { bundle: Some(bundle) }
                        },
                        Err(e) => {
                            error!("Failed to deserialize TrustBundle: {}", e);
                            network::TrustBundleResponse { bundle: None }
                        }
                    }
                },
                Ok(None) => {
                    debug!("TrustBundle for epoch {} not found in storage", request.epoch);
                    network::TrustBundleResponse { bundle: None }
                },
                Err(e) => {
                    error!("Storage error when retrieving TrustBundle: {}", e);
                    network::TrustBundleResponse { bundle: None }
                }
            };
            
            if let Err(e) = swarm.behaviour_mut().trust_bundle_sync.send_response(channel, response) {
                error!("Failed to send TrustBundle response: {:?}", e);
            }
        },
        network::IcnFederationBehaviourEvent::TrustBundleSync(request_response::Event::Message { 
            message: request_response::Message::Response { request_id, response, .. },
            ..
        }) => {
            debug!("Received TrustBundle response for request {}", request_id);
            if let Some(received_bundle) = &response.bundle {
                info!("Received TrustBundle for epoch {}", received_bundle.epoch_id);
                
                // Check if the received bundle has a proof
                if received_bundle.proof.is_none() {
                    error!("Received TrustBundle has no proof, rejecting: epoch {}", received_bundle.epoch_id);
                    return;
                }
                
                // Calculate the hash of the received bundle for later verification
                let bundle_hash = received_bundle.calculate_hash();
                
                // Look up authorized guardians for this federation
                let authorized_guardians = match roles::get_authorized_guardians(
                    &received_bundle.federation_id, 
                    Arc::clone(&storage)
                ).await {
                    Ok(guardians) => guardians,
                    Err(e) => {
                        error!("Failed to look up authorized guardians for federation {}: {}", 
                               received_bundle.federation_id, e);
                        return;
                    }
                };
                
                if authorized_guardians.is_empty() {
                    warn!("No authorized guardians found for federation: {}", received_bundle.federation_id);
                }
                
                // Verify the bundle with full cryptographic validation
                match received_bundle.verify(&authorized_guardians).await {
                    Ok(true) => {
                        info!("TrustBundle validation passed for epoch {} (hash prefix: {:02x}{:02x}{:02x}{:02x})", 
                            received_bundle.epoch_id, 
                            bundle_hash[0], bundle_hash[1], bundle_hash[2], bundle_hash[3]);
                        
                        // TODO(V3): Check received_bundle.epoch_id against local state for replay protection
                        // TODO(V3): Check received_bundle.federation_id matches expected federation
                        // TODO(V3): Potentially check DAG root consistency with local state
                        
                        // Serialize the bundle for storage
                        match serde_json::to_vec(received_bundle) {
                            Ok(bundle_bytes) => {
                                // Generate the storage key based on epoch_id
                                let key_str = format!("trustbundle::epoch::{}", received_bundle.epoch_id);
                                let key_hash = multihash::Code::Sha2_256.digest(key_str.as_bytes());
                                let key_cid = cid::Cid::new_v1(0x71, key_hash); // Raw codec
                                
                                // Store the bundle
                                let storage_lock = storage.lock().await;
                                match storage_lock.put(&bundle_bytes).await {
                                    Ok(stored_cid) => {
                                        info!("Successfully stored TrustBundle for epoch {} with CID {} (key: {})", 
                                             received_bundle.epoch_id, stored_cid, key_cid);
                                        
                                        // Update latest known epoch if this is newer
                                        // TODO(V3): Implement proper epoch tracking
                                    },
                                    Err(e) => {
                                        error!("Failed to store TrustBundle: {}", e);
                                    }
                                }
                            },
                            Err(e) => {
                                error!("Failed to serialize TrustBundle: {}", e);
                            }
                        }
                    },
                    Ok(false) => {
                        warn!("TrustBundle validation failed for epoch {} (invalid quorum or signatures)", received_bundle.epoch_id);
                        if let Some(proof) = &received_bundle.proof {
                            warn!("Quorum config: {:?}, votes: {}", proof.config, proof.votes.len());
                        }
                    },
                    Err(e) => {
                        error!("TrustBundle cryptographic validation error for epoch {}: {}", received_bundle.epoch_id, e);
                    }
                }
            } else {
                debug!("Received empty TrustBundle response (None)");
            }
        },
        
        // Handle blob replication requests
        network::IcnFederationBehaviourEvent::BlobReplication(request_response::Event::Message { 
            peer, 
            message: request_response::Message::Request { request, channel, .. },
            ..
        }) => {
            info!(peer = %peer, cid = %request.cid, "Received ReplicateBlobRequest");
            
            // Check if the blob exists locally
            let storage_lock = storage.lock().await;
            let blob_exists = storage_lock.blob_exists(&request.cid).await;
            drop(storage_lock); // Release the lock as soon as possible
            
            match blob_exists {
                // The blob exists
                Ok(true) => {
                    debug!(cid = %request.cid, "Blob already exists locally, attempting to pin");
                    
                    // Pin the blob
                    let storage_lock = storage.lock().await;
                    match storage_lock.pin_blob(&request.cid).await {
                        Ok(_) => {
                            info!(cid = %request.cid, "Successfully pinned blob for replication");
                            
                            // Send successful response
                            let response = network::ReplicateBlobResponse {
                                success: true,
                                error_msg: None,
                            };
                            if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                                error!(cid = %request.cid, "Failed to send replication response: {:?}", e);
                            }
                        },
                        Err(e) => {
                            error!(cid = %request.cid, "Failed to pin existing blob: {}", e);
                            
                            // Send error response
                            let response = network::ReplicateBlobResponse {
                                success: false,
                                error_msg: Some(format!("Failed to pin existing blob: {}", e)),
                            };
                            if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                                error!(cid = %request.cid, "Failed to send replication response: {:?}", e);
                            }
                        }
                    }
                },
                // The blob doesn't exist
                Ok(false) => {
                    warn!(cid = %request.cid, "Blob not found locally, cannot fulfill replication request (P2P fetch not implemented)");
                    
                    // Send error response
                    let response = network::ReplicateBlobResponse {
                        success: false,
                        error_msg: Some("Blob not available locally".to_string()),
                    };
                    if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                        error!(cid = %request.cid, "Failed to send replication response: {:?}", e);
                    }
                    
                    // TODO(V3-MVP): Implement P2P blob fetch logic here if blob doesn't exist locally.
                },
                // Error checking if the blob exists
                Err(e) => {
                    error!(cid = %request.cid, "Error checking if blob exists: {}", e);
                    
                    // Send error response
                    let response = network::ReplicateBlobResponse {
                        success: false,
                        error_msg: Some(format!("Error checking blob existence: {}", e)),
                    };
                    if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                        error!(cid = %request.cid, "Failed to send replication response: {:?}", e);
                    }
                }
            }
        },
        
        // Handle blob replication responses
        network::IcnFederationBehaviourEvent::BlobReplication(request_response::Event::Message { 
            peer, 
            message: request_response::Message::Response { response, .. },
            ..
        }) => {
            info!(
                peer = %peer, 
                success = response.success, 
                error = ?response.error_msg, 
                "Received ReplicateBlobResponse"
            );
            
            // TODO(V3-MVP): Update replication tracking state based on response.
        },
        _ => {}
    }
}

/// Helper function to announce the local node as a provider for a CID
async fn announce_as_provider(
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    cid: cid::Cid
) -> FederationResult<()> {
    // Use the CID bytes directly as the Kademlia key
    // This method doesn't need to access the private kad::record module
    let cid_bytes = cid.to_bytes();
    
    // Announce ourselves as a provider for this key
    swarm.behaviour_mut().kademlia.start_providing(cid_bytes.into())
        .map_err(|e| FederationError::NetworkError(format!("Failed to start Kademlia provider record: {}", e)))?;
    
    Ok(())
}

/// Gets the latest known epoch from local state
/// This is a temporary placeholder implementation that will be replaced with FederationManager::get_latest_known_epoch
async fn get_latest_known_epoch() -> u64 {
    // TODO(V3-MVP): Implement actual latest epoch tracking using FederationManager::get_latest_known_epoch
    // For now, just return a hardcoded value for testing
    0
}

/// Sends a request to the network to fetch a TrustBundle for the specified epoch
async fn request_trust_bundle_from_network(
    sender: mpsc::Sender<FederationManagerMessage>,
    epoch: u64,
) -> FederationResult<()> {
    debug!("Requesting TrustBundle for epoch {} from network", epoch);
    
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    sender.send(FederationManagerMessage::RequestTrustBundle {
        epoch,
        respond_to: tx,
    }).await
    .map_err(|e| FederationError::NetworkError(format!("Failed to send request: {}", e)))?;
    
    match rx.await {
        Ok(result) => {
            match result {
                Ok(Some(_)) => {
                    debug!("Successfully received TrustBundle for epoch {}", epoch);
                },
                Ok(None) => {
                    debug!("No TrustBundle available for epoch {}", epoch);
                },
                Err(e) => {
                    debug!("Failed to get TrustBundle for epoch {}: {}", epoch, e);
                }
            }
            Ok(())
        },
        Err(e) => {
            Err(FederationError::NetworkError(format!("Failed to receive response: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_response_types() {
        // Test TrustBundleRequest
        let request = network::TrustBundleRequest { epoch: 42 };
        assert_eq!(request.epoch, 42);
        
        // Test TrustBundleResponse with None
        let response_none = network::TrustBundleResponse { bundle: None };
        assert!(response_none.bundle.is_none());
        
        // Test ReplicateBlobRequest
        let cid = cid::Cid::new_v0(multihash::Code::Sha2_256.digest(b"test_blob")).unwrap();
        let replicate_request = network::ReplicateBlobRequest { cid };
        assert_eq!(replicate_request.cid, cid);
        
        // Test ReplicateBlobResponse success
        let success_response = network::ReplicateBlobResponse { 
            success: true, 
            error_msg: None 
        };
        assert!(success_response.success);
        assert!(success_response.error_msg.is_none());
        
        // Test ReplicateBlobResponse failure
        let error_response = network::ReplicateBlobResponse { 
            success: false, 
            error_msg: Some("Blob not found".to_string()) 
        };
        assert!(!error_response.success);
        assert_eq!(error_response.error_msg.unwrap(), "Blob not found");
    }
    
    #[tokio::test]
    async fn test_blob_replication_protocol() {
        use std::time::Duration;
        use cid::Cid;
        use icn_storage::{InMemoryBlobStore, DistributedStorage};
        use multihash::{Code, MultihashDigest};
        
        // Create test data
        let test_content = b"This is test content for blob replication protocol".to_vec();
        let mh = Code::Sha2_256.digest(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create mock storage with the test blob
        let mut storage = icn_storage::AsyncInMemoryStorage::new();
        
        // Create blob store with the test blob
        let blob_store = InMemoryBlobStore::new();
        
        // Put and pin the test blob
        blob_store.put_blob(&test_content).await.unwrap();
        blob_store.pin_blob(&cid).await.unwrap();
        
        // Create replication request
        let request = network::ReplicateBlobRequest { cid };
        
        // Verify the request serialization/deserialization
        let serialized = serde_cbor::to_vec(&request).unwrap();
        let deserialized: network::ReplicateBlobRequest = serde_cbor::from_slice(&serialized).unwrap();
        assert_eq!(deserialized.cid, cid);
        
        // Create response
        let response = network::ReplicateBlobResponse {
            success: true,
            error_msg: None,
        };
        
        // Verify the response serialization/deserialization
        let serialized = serde_cbor::to_vec(&response).unwrap();
        let deserialized: network::ReplicateBlobResponse = serde_cbor::from_slice(&serialized).unwrap();
        assert_eq!(deserialized.success, response.success);
        assert_eq!(deserialized.error_msg, response.error_msg);
        
        // Create a mock peer ID for testing
        let mock_peer_id = PeerId::random();
        
        // Test target peer identification
        let peers = vec![mock_peer_id];
        let policy = ReplicationPolicy::Factor(1);
        let local_peer_id = PeerId::random();
        
        let targets = replication::identify_target_peers(
            &cid, 
            &policy, 
            peers.clone(), 
            &local_peer_id
        ).await;
        
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0], mock_peer_id);
    }
    
    #[tokio::test]
    async fn test_federation_manager_start() {
        // Use a default configuration for testing
        let config = FederationManagerConfig::default();
        
        // Attempt to start a federation node
        let result = FederationManager::start_node(config, Arc::new(Mutex::new(icn_storage::AsyncInMemoryStorage::new()))).await;
        
        // Check that we can create a federation manager without panicking
        assert!(result.is_ok(), "Failed to start federation node: {:?}", result.err());
        
        // Clean up by shutting down
        if let Ok((manager, _blob_sender, _fed_cmd_sender)) = result {
            let shutdown_result = manager.shutdown().await;
            assert!(shutdown_result.is_ok(), "Failed to shutdown federation manager: {:?}", shutdown_result.err());
        }
    }
    
    #[tokio::test]
    async fn test_blob_announcement() {
        use cid::Cid;
        use std::time::Duration;
        use icn_storage::{InMemoryBlobStore, DistributedStorage};
        use multihash::{Code, MultihashDigest};
        
        // Use a default configuration for testing
        let config = FederationManagerConfig::default();
        
        // Attempt to start a federation node
        let result = FederationManager::start_node(
            config, 
            Arc::new(Mutex::new(icn_storage::AsyncInMemoryStorage::new()))
        ).await;
        
        // Check that we can create a federation manager without panicking
        assert!(result.is_ok(), "Failed to start federation node: {:?}", result.err());
        
        if let Ok((manager, blob_sender, _fed_cmd_sender)) = result {
            // Create an in-memory blob store with the announcer channel
            let blob_store = InMemoryBlobStore::with_announcer(blob_sender);
            
            // Create a test blob to store
            let test_content = b"This is a test blob for Kademlia announcement".to_vec();
            
            // Store the blob, which should trigger an announcement
            let cid = blob_store.put_blob(&test_content).await.unwrap();
            
            // Allow some time for the announcement to be processed
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // We don't have a great way to verify the announcement was processed
            // in a unit test without mocking the swarm, but we can at least
            // verify the proper CID was generated
            let expected_mh = Code::Sha2_256.digest(&test_content);
            let expected_cid = Cid::new_v0(expected_mh).unwrap();
            assert_eq!(cid, expected_cid, "CID should match expected value");
            
            // Clean up
            let shutdown_result = manager.shutdown().await;
            assert!(shutdown_result.is_ok(), "Failed to shutdown federation manager");
        }
    }
    
    #[tokio::test]
    async fn test_blob_replication_trigger() {
        use cid::Cid;
        use std::time::Duration;
        use icn_storage::{InMemoryBlobStore, DistributedStorage, FederationCommand};
        use multihash::{Code, MultihashDigest};
        
        // Use a default configuration for testing
        let config = FederationManagerConfig::default();
        
        // Attempt to start a federation node
        let result = FederationManager::start_node(
            config, 
            Arc::new(Mutex::new(icn_storage::AsyncInMemoryStorage::new()))
        ).await;
        
        // Check that we can create a federation manager without panicking
        assert!(result.is_ok(), "Failed to start federation node: {:?}", result.err());
        
        if let Ok((manager, blob_sender, fed_cmd_sender)) = result {
            // Create an in-memory blob store with the federation command sender
            let blob_store = InMemoryBlobStore::with_federation(blob_sender, fed_cmd_sender);
            
            // Create a test blob to store
            let test_content = b"This is a test blob for replication".to_vec();
            
            // Store the blob
            let cid = blob_store.put_blob(&test_content).await.unwrap();
            
            // Pin the blob, which should trigger replication
            blob_store.pin_blob(&cid).await.unwrap();
            
            // Allow some time for the replication process to be initiated
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // We don't have a great way to verify the replication targets were identified
            // in a unit test without mocking the swarm, but we can at least
            // verify the proper CID was generated and pinned
            let expected_mh = Code::Sha2_256.digest(&test_content);
            let expected_cid = Cid::new_v0(expected_mh).unwrap();
            assert_eq!(cid, expected_cid, "CID should match expected value");
            
            // Verify the blob is pinned
            let is_pinned = blob_store.is_pinned(&cid).await.unwrap();
            assert!(is_pinned, "Blob should be pinned");
            
            // Clean up
            let shutdown_result = manager.shutdown().await;
            assert!(shutdown_result.is_ok(), "Failed to shutdown federation manager");
        }
    }
} 