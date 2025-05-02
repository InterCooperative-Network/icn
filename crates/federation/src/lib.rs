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
use std::collections::HashSet;
use async_trait::async_trait;

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
use icn_storage::{FederationCommand, ReplicationPolicy, DistributedStorage};
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

// Add this import at the top where other imports are
use libp2p::request_response::OutboundRequestId;

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
    pub async fn new(
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
    
    // Create a BlobStorageAdapter to handle blob operations
    let blob_storage = Arc::new(BlobStorageAdapter::new(Arc::clone(&storage)));
    
    // Track active Kademlia queries to handle responses
    let mut active_replication_queries: HashMap<kad::QueryId, (cid::Cid, ReplicationPolicy, Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>)> = HashMap::new();
    
    // Track pending replication fetches initiated by incoming requests
    let mut pending_replication_fetches: HashMap<kad::QueryId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)> = HashMap::new();
    
    // Track pending blob fetches initiated by fetch requests
    let mut pending_blob_fetches: HashMap<OutboundRequestId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)> = HashMap::new();

    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                handle_swarm_event(&mut swarm, event, Arc::clone(&storage), Arc::clone(&blob_storage), &mut active_replication_queries, &mut pending_replication_fetches, &mut pending_blob_fetches).await;
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
                        let storage_clone = storage.clone();
                        let bundle_result = {
                            let storage_lock = storage_clone.lock().await;
                            storage_lock.get_kv(&_key_cid).await
                        };

                        match bundle_result {
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
                            Ok(None) => {
                                debug!("TrustBundle for epoch {} not found locally, requesting from peers", epoch);
                                
                                // TODO(V3-MVP): Implement peer selection and await response properly
                                let _ = respond_to.send(Ok(None));
                            },
                            Err(e) => {
                                error!("Failed to retrieve TrustBundle from storage: {}", e);
                                let _ = respond_to.send(Err(FederationError::StorageError(
                                    format!("Failed to retrieve TrustBundle from storage: {}", e)
                                )));
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
    blob_storage: Arc<BlobStorageAdapter>,
    active_replication_queries: &mut HashMap<kad::QueryId, (cid::Cid, ReplicationPolicy, Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>)>,
    pending_replication_fetches: &mut HashMap<kad::QueryId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
    pending_blob_fetches: &mut HashMap<OutboundRequestId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
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
            handle_behavior_event(swarm, behavior_event, storage, blob_storage, active_replication_queries, pending_replication_fetches, pending_blob_fetches).await;
        },
        _ => {}
    }
}

/// Handle behavior events
async fn handle_behavior_event(
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    event: network::IcnFederationBehaviourEvent,
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    blob_storage: Arc<BlobStorageAdapter>,
    active_replication_queries: &mut HashMap<kad::QueryId, (cid::Cid, ReplicationPolicy, Option<tokio::sync::oneshot::Sender<FederationResult<Vec<PeerId>>>>)>,
    pending_replication_fetches: &mut HashMap<kad::QueryId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
    pending_blob_fetches: &mut HashMap<OutboundRequestId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
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
            result: kad::QueryResult::GetProviders(result), 
            .. 
        }) => {
            // Check if this is a replication fetch query we're tracking
            if let Some((original_cid, response_channel)) = pending_replication_fetches.remove(&id) {
                match result {
                    // Providers found
                    Ok(ok_result) => {
                        // Check if we have providers (using pattern matching to access different variants)
                        match ok_result {
                            kad::GetProvidersOk::FoundProviders { providers, .. } if !providers.is_empty() => {
                                info!(
                                    cid = %original_cid, 
                                    provider_count = providers.len(),
                                    "Found providers for requested replication blob"
                                );
                                
                                // Select a suitable provider to fetch from (for simplicity, take the first one)
                                let provider_peer_id = *providers.iter().next().unwrap();
                                info!(
                                    cid = %original_cid,
                                    peer = %provider_peer_id,
                                    "Initiating blob fetch from provider"
                                );
                                
                                // Create a fetch request
                                let fetch_request = network::FetchBlobRequest {
                                    cid: original_cid,
                                };
                                
                                // Send the fetch request to the provider
                                let request_id = swarm.behaviour_mut()
                                    .blob_fetch
                                    .send_request(&provider_peer_id, fetch_request);
                                
                                // Store the request ID along with the original CID and response channel
                                pending_blob_fetches.insert(request_id, (original_cid, response_channel));
                                
                                debug!(
                                    cid = %original_cid,
                                    peer = %provider_peer_id,
                                    ?request_id,
                                    "Sent FetchBlobRequest"
                                );
                            },
                            _ => {
                                warn!(cid = %original_cid, "No providers found for requested replication blob");
                                
                                // Send error response
                                let response = network::ReplicateBlobResponse {
                                    success: false,
                                    error_msg: Some("No providers found for blob".to_string()),
                                };
                                if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(response_channel, response) {
                                    error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                                }
                            }
                        }
                    },
                    // Kademlia error
                    Err(e) => {
                        error!(cid = %original_cid, "Kademlia get_providers query failed: {:?}", e);
                        
                        // Send error response
                        let response = network::ReplicateBlobResponse {
                            success: false,
                            error_msg: Some(format!("Kademlia query failed: {:?}", e)),
                        };
                        if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(response_channel, response) {
                            error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                        }
                    }
                }
            } else {
                // This wasn't a query we were tracking
                debug!("Received GetProviders result for an untracked query");
            }
        },
        network::IcnFederationBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { 
            result: kad::QueryResult::PutRecord(result), 
            .. 
        }) => {
            match result {
                Ok(_) => {
                    debug!("Successfully put Kademlia record");
                },
                Err(e) => {
                    warn!("Failed to put Kademlia record: {:?}", e);
                }
            }
        },
        network::IcnFederationBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed { 
            result: kad::QueryResult::StartProviding(result), 
            .. 
        }) => {
            match result {
                Ok(_) => {
                    debug!("Successfully started providing Kademlia record");
                },
                Err(e) => {
                    warn!("Failed to start providing Kademlia record: {:?}", e);
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
            let storage_clone = storage.clone();
            let bundle_result = {
                let storage_lock = storage_clone.lock().await;
                storage_lock.get_kv(&_key_cid).await
            };
            
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
                                let storage_clone = storage.clone();
                                let store_result = {
                                    let storage_lock = storage_clone.lock().await;
                                    storage_lock.put_kv(key_cid, bundle_bytes).await
                                };

                                match store_result {
                                    Ok(_) => {
                                        info!("Successfully stored TrustBundle for epoch {} (key: {})", 
                                             received_bundle.epoch_id, key_cid);
                                        
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
        
        // Handle blob fetch requests
        network::IcnFederationBehaviourEvent::BlobFetch(request_response::Event::Message { 
            peer, 
            message: request_response::Message::Request { request, channel, .. },
            ..
        }) => {
            info!(peer = %peer, cid = %request.cid, "Received FetchBlobRequest");
            
            // Try to get the blob data from local storage
            match blob_storage.get_blob(&request.cid).await {
                Ok(Some(data)) => {
                    info!(cid = %request.cid, size = data.len(), "Found requested blob in local storage, sending");
                    
                    // Send the blob data
                    let response = network::FetchBlobResponse {
                        data: Some(data),
                        error_msg: None,
                    };
                    
                    if let Err(e) = swarm.behaviour_mut().blob_fetch.send_response(channel, response) {
                        error!(cid = %request.cid, "Failed to send blob fetch response: {:?}", e);
                    }
                },
                Ok(None) => {
                    warn!(cid = %request.cid, "Requested blob not found in local storage");
                    
                    // Send error response
                    let response = network::FetchBlobResponse {
                        data: None,
                        error_msg: Some("Blob not found in local storage".to_string()),
                    };
                    
                    if let Err(e) = swarm.behaviour_mut().blob_fetch.send_response(channel, response) {
                        error!(cid = %request.cid, "Failed to send blob fetch response: {:?}", e);
                    }
                },
                Err(e) => {
                    error!(cid = %request.cid, "Error retrieving blob from storage: {}", e);
                    
                    // Send error response
                    let response = network::FetchBlobResponse {
                        data: None,
                        error_msg: Some(format!("Error retrieving blob: {}", e)),
                    };
                    
                    if let Err(e) = swarm.behaviour_mut().blob_fetch.send_response(channel, response) {
                        error!(cid = %request.cid, "Failed to send blob fetch response: {:?}", e);
                    }
                }
            }
        },
        
        // Handle blob fetch responses
        network::IcnFederationBehaviourEvent::BlobFetch(request_response::Event::Message { 
            peer, 
            message: request_response::Message::Response { request_id, response },
            ..
        }) => {
            // Find the original context for this request
            if let Some((original_cid, replication_response_channel)) = pending_blob_fetches.remove(&request_id) {
                info!(
                    peer = %peer,
                    cid = ?original_cid,
                    has_data = response.data.is_some(),
                    error = ?response.error_msg,
                    "Received FetchBlobResponse"
                );
                
                match response.data {
                    Some(blob_data) => {
                        // Verify the hash matches
                        let mh = multihash::Code::Sha2_256.digest(&blob_data);
                        let calculated_cid = cid::Cid::new_v0(mh);
                        
                        if let Ok(calc_cid) = calculated_cid {
                            if calc_cid == original_cid {
                                debug!(
                                    cid = %original_cid,
                                    size = blob_data.len(),
                                    "Fetched blob hash verification succeeded, storing and pinning"
                                );
                                
                                // Store the blob
                                match blob_storage.put_blob(&blob_data).await {
                                    Ok(_) => {
                                        // Pin the blob
                                        match blob_storage.pin_blob(&original_cid).await {
                                            Ok(_) => {
                                                info!(cid = %original_cid, "Successfully stored and pinned fetched blob");
                                                
                                                // Send success response
                                                let response = network::ReplicateBlobResponse {
                                                    success: true,
                                                    error_msg: None,
                                                };
                                                
                                                if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                                                    error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                                                }
                                            },
                                            Err(e) => {
                                                error!(cid = %original_cid, "Failed to pin fetched blob: {}", e);
                                                
                                                // Send error response
                                                let response = network::ReplicateBlobResponse {
                                                    success: false,
                                                    error_msg: Some(format!("Failed to pin fetched blob: {}", e)),
                                                };
                                                
                                                if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                                                    error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        error!(cid = %original_cid, "Failed to store fetched blob: {}", e);
                                        
                                        // Send error response
                                        let response = network::ReplicateBlobResponse {
                                            success: false,
                                            error_msg: Some(format!("Failed to store fetched blob: {}", e)),
                                        };
                                        
                                        if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                                            error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                                        }
                                    }
                                }
                            } else {
                                error!(
                                    expected_cid = ?original_cid,
                                    actual_cid = ?calc_cid,
                                    "Fetched blob hash mismatch"
                                );
                                
                                // Send error response
                                let response = network::ReplicateBlobResponse {
                                    success: false,
                                    error_msg: Some(format!("Fetched blob hash mismatch, expected {:?}, got {:?}", original_cid, calc_cid)),
                                };
                                
                                if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                                    error!(cid = ?original_cid, "Failed to send replication response: {:?}", e);
                                }
                            }
                        } else {
                            error!(
                                expected_cid = ?original_cid,
                                error = ?calculated_cid.err(),
                                "Failed to calculate CID for fetched blob"
                            );
                            
                            // Send error response
                            let response = network::ReplicateBlobResponse {
                                success: false,
                                error_msg: Some("Failed to calculate CID for fetched blob".to_string()),
                            };
                            
                            if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                                error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                            }
                        }
                    },
                    None => {
                        error!(
                            peer = %peer,
                            cid = %original_cid,
                            error = ?response.error_msg,
                            "Blob fetch response contained no data"
                        );
                        
                        // Send error response
                        let response = network::ReplicateBlobResponse {
                            success: false,
                            error_msg: Some(format!("Blob fetch response contained no data: {:?}", response.error_msg)),
                        };
                        
                        if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                            error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                        }
                    }
                }
            }
        },
        
        // Handle blob fetch outbound failures
        network::IcnFederationBehaviourEvent::BlobFetch(request_response::Event::OutboundFailure { 
            peer, 
            error,
            request_id,
            ..
        }) => {
            // Check if this is a fetch we're tracking
            if let Some((original_cid, replication_response_channel)) = pending_blob_fetches.remove(&request_id) {
                error!(
                    peer = %peer,
                    cid = %original_cid,
                    error = ?error,
                    "Blob fetch outbound failure"
                );
                
                // Send error response
                let response = network::ReplicateBlobResponse {
                    success: false,
                    error_msg: Some(format!("Blob fetch outbound failure: {:?}", error)),
                };
                
                if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(replication_response_channel, response) {
                    error!(cid = %original_cid, "Failed to send replication response: {:?}", e);
                }
            }
        },
        
        // Handle blob fetch inbound failures
        network::IcnFederationBehaviourEvent::BlobFetch(request_response::Event::InboundFailure { 
            peer, 
            error, 
            ..
        }) => {
            warn!(
                peer = %peer,
                error = ?error,
                "Blob fetch inbound failure"
            );
        },
        
        // Handle blob replication requests
        network::IcnFederationBehaviourEvent::BlobReplication(request_response::Event::Message { 
            peer, 
            message: request_response::Message::Request { request, channel, .. },
            ..
        }) => {
            info!(peer = %peer, cid = %request.cid, "Received ReplicateBlobRequest");
            
            // Check if the blob exists locally
            let blob_exists = blob_storage.blob_exists(&request.cid).await;
            
            match blob_exists {
                // The blob exists
                Ok(true) => {
                    debug!(cid = %request.cid, "Blob already exists locally, attempting to pin");
                    
                    // Pin the blob
                    match blob_storage.pin_blob(&request.cid).await {
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
                    warn!(cid = %request.cid, "Blob not found locally, initiating Kademlia get_providers query");
                    
                    // Initiate a Kademlia query to find providers with the blob
                    let query_id = swarm.behaviour_mut().kademlia.get_providers(request.cid.to_bytes().into());
                    debug!(cid = %request.cid, ?query_id, "Started Kademlia get_providers query");
                    
                    // Store the request context for when we get the query result
                    pending_replication_fetches.insert(query_id, (request.cid, channel));
                    
                    // Note: We don't send a response yet - that will happen after the Kademlia query completes
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
        
        // Other event handlers...
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

/// Adapter that allows a regular StorageBackend to be used for blob storage operations
/// This bridges the gap between the StorageBackend and DistributedStorage traits
pub struct BlobStorageAdapter {
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    pinned_blobs: Arc<Mutex<HashSet<cid::Cid>>>,
}

impl BlobStorageAdapter {
    /// Create a new BlobStorageAdapter
    pub fn new(storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>) -> Self {
        Self {
            storage,
            pinned_blobs: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[async_trait]
impl icn_storage::DistributedStorage for BlobStorageAdapter {
    async fn put_blob(&self, content: &[u8]) -> icn_storage::StorageResult<cid::Cid> {
        // Clone the storage to use in async context
        let storage = self.storage.clone();
        
        // Get a lock, perform operation, then release lock
        let result = {
            let storage_guard = storage.lock().await;
            storage_guard.put_blob(content).await
        };
        
        // Return the result after the lock is dropped
        result
    }
    
    async fn get_blob(&self, cid: &cid::Cid) -> icn_storage::StorageResult<Option<Vec<u8>>> {
        // Clone the storage to use in async context
        let storage = self.storage.clone();
        
        // Get a lock, perform operation, then release lock
        let result = {
            let storage_guard = storage.lock().await;
            storage_guard.get_blob(cid).await
        };
        
        // Return the result after the lock is dropped
        result
    }
    
    async fn blob_exists(&self, cid: &cid::Cid) -> icn_storage::StorageResult<bool> {
        // Clone the storage to use in async context
        let storage = self.storage.clone();
        
        // Get a lock, perform operation, then release lock
        let result = {
            let storage_guard = storage.lock().await;
            storage_guard.contains_blob(cid).await
        };
        
        // Return the result after the lock is dropped
        result
    }
    
    async fn blob_size(&self, cid: &cid::Cid) -> icn_storage::StorageResult<Option<u64>> {
        // Clone the storage to use in async context
        let storage = self.storage.clone();
        
        // Get a lock, perform operation, then release lock
        let result = {
            let storage_guard = storage.lock().await;
            storage_guard.get_blob(cid).await
        };
        
        // Process the result after the lock is dropped
        match result {
            Ok(Some(content)) => Ok(Some(content.len() as u64)),
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }
    
    async fn is_pinned(&self, cid: &cid::Cid) -> icn_storage::StorageResult<bool> {
        // Get the pinned blobs set
        let pinned_blobs = self.pinned_blobs.lock().await;
        
        // Check if the CID is in the set (an immediate operation, no need for special handling)
        Ok(pinned_blobs.contains(cid))
    }
    
    async fn pin_blob(&self, cid: &cid::Cid) -> icn_storage::StorageResult<()> {
        // First check if the blob exists
        let exists = self.blob_exists(cid).await?;
        
        if !exists {
            return Err(icn_storage::StorageError::BlobNotFound(cid.to_string()));
        }
        
        // Get the pinned blobs set
        let mut pinned_blobs = self.pinned_blobs.lock().await;
        
        // Add the CID to the set (immediate operation, no await point)
        pinned_blobs.insert(*cid);
        
        Ok(())
    }
    
    async fn unpin_blob(&self, cid: &cid::Cid) -> icn_storage::StorageResult<()> {
        // Get the pinned blobs set
        let mut pinned_blobs = self.pinned_blobs.lock().await;
        
        // Remove the CID from the set (immediate operation, no await point)
        pinned_blobs.remove(cid);
        
        Ok(())
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
        let storage = icn_storage::AsyncInMemoryStorage::new();
        
        // Create blob store adapter
        let blob_store_adapter = BlobStorageAdapter::new(Arc::new(Mutex::new(storage)));
        
        // Put and pin the test blob
        blob_store_adapter.put_blob(&test_content).await.unwrap();
        blob_store_adapter.pin_blob(&cid).await.unwrap();
        
        // Create replication request
        let request = network::ReplicateBlobRequest { cid };
        
        // Create response
        let response = network::ReplicateBlobResponse {
            success: true,
            error_msg: None,
        };
        
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
    async fn test_p2p_blob_fetch() {
        use std::time::Duration;
        use cid::Cid;
        use icn_storage::{AsyncInMemoryStorage, StorageResult, StorageBackend};
        use multihash::{Code, MultihashDigest};
        
        // Create test data
        let test_content = b"This is test content for P2P blob fetch".to_vec();
        let mh = Code::Sha2_256.digest(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create storage and adapter
        let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let blob_storage = Arc::new(BlobStorageAdapter::new(storage.clone()));
        
        // Create a mock query ID using a simple wrapper type
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct MockQueryId(u64);
        let query_id = MockQueryId(42);
        
        // Simulate a provider giving us the data by adding it to storage
        {
            let storage_lock = blob_storage.storage.lock().await;
            storage_lock.put_blob(&test_content).await.unwrap();
        }
        
        // Verify we can access the blob
        let maybe_data = blob_storage.get_blob(&cid).await;
        
        match maybe_data {
            Ok(Some(blob_data)) => {
                // Simulate storing and pinning the blob
                blob_storage.put_blob(&blob_data).await.unwrap();
                blob_storage.pin_blob(&cid).await.unwrap();
                
                // Verify the blob is pinned
                let is_pinned = blob_storage.is_pinned(&cid).await.unwrap();
                assert!(is_pinned, "Blob should be pinned after fetch and pin");
            },
            _ => {
                panic!("Failed to get blob data that should have been available");
            }
        }
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

    #[tokio::test]
    async fn test_blob_fetch_protocol() {
        use std::time::Duration;
        use cid::Cid;
        use icn_storage::{AsyncInMemoryStorage, StorageResult, StorageBackend, DistributedStorage};
        use multihash::{Code, MultihashDigest};
        use libp2p::{request_response, PeerId};
        
        // Create test data
        let test_content = b"This is test content for blob fetch protocol".to_vec();
        let mh = Code::Sha2_256.digest(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create storage and adapter for provider
        let provider_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let provider_blob_storage = Arc::new(BlobStorageAdapter::new(provider_storage.clone()));
        
        // Create storage and adapter for requester
        let requester_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let requester_blob_storage = Arc::new(BlobStorageAdapter::new(requester_storage.clone()));
        
        // Add the blob to the provider's storage
        provider_blob_storage.put_blob(&test_content).await.unwrap();
        provider_blob_storage.pin_blob(&cid).await.unwrap();
        
        // Create a fetch request
        let fetch_request = network::FetchBlobRequest {
            cid,
        };
        
        // Test the fetch request handler
        let provider_peer_id = PeerId::random();
        
        // Create a mock channel for the fetch response
        let (response_sender, mut response_receiver) = 
            tokio::sync::mpsc::channel::<network::FetchBlobResponse>(1);
        
        // Simulate FetchBlobRequest handling as if from the provider side
        let request_result = provider_blob_storage.get_blob(&cid).await;
        
        match request_result {
            Ok(Some(data)) => {
                // Create fetch response with the blob data
                let response = network::FetchBlobResponse {
                    data: Some(data),
                    error_msg: None,
                };
                
                response_sender.send(response).await.unwrap();
            },
            Ok(None) => {
                let response = network::FetchBlobResponse {
                    data: None,
                    error_msg: Some("Blob not found".to_string()),
                };
                
                response_sender.send(response).await.unwrap();
            },
            Err(e) => {
                let response = network::FetchBlobResponse {
                    data: None,
                    error_msg: Some(format!("Error: {}", e)),
                };
                
                response_sender.send(response).await.unwrap();
            }
        }
        
        // Receive the response
        let fetch_response = response_receiver.recv().await.unwrap();
        
        // Verify the response
        assert!(fetch_response.data.is_some(), "Fetch response should contain blob data");
        assert!(fetch_response.error_msg.is_none(), "Fetch response should not have an error");
        
        // Verify the data hash
        let blob_data = fetch_response.data.unwrap();
        let mh = Code::Sha2_256.digest(&blob_data);
        let calculated_cid = Cid::new_v0(mh).unwrap();
        assert_eq!(calculated_cid, cid, "CID of fetched data should match original");
        
        // Simulate storage of the fetched blob at the requester side
        requester_blob_storage.put_blob(&blob_data).await.unwrap();
        requester_blob_storage.pin_blob(&cid).await.unwrap();
        
        // Verify the blob is now in the requester's storage
        let exists = requester_blob_storage.blob_exists(&cid).await.unwrap();
        assert!(exists, "Blob should exist in requester's storage after fetch");
        
        // Verify the blob is pinned
        let is_pinned = requester_blob_storage.is_pinned(&cid).await.unwrap();
        assert!(is_pinned, "Blob should be pinned in requester's storage after fetch");
    }

    #[tokio::test]
    async fn test_blob_replication_with_fetch() {
        use std::time::Duration;
        use cid::Cid;
        use icn_storage::{AsyncInMemoryStorage, StorageResult, StorageBackend, DistributedStorage, ReplicationPolicy};
        use multihash::{Code, MultihashDigest};
        use libp2p::{request_response, PeerId};
        
        // Create test data
        let test_content = b"This is test content for blob replication with fetch".to_vec();
        let mh = Code::Sha2_256.digest(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create storage and adapter for provider
        let provider_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let provider_blob_storage = Arc::new(BlobStorageAdapter::new(provider_storage.clone()));
        
        // Create storage and adapter for requester
        let requester_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let requester_blob_storage = Arc::new(BlobStorageAdapter::new(requester_storage.clone()));
        
        // Add the blob to the provider's storage
        provider_blob_storage.put_blob(&test_content).await.unwrap();
        provider_blob_storage.pin_blob(&cid).await.unwrap();
        
        // Create mock PeerIDs
        let provider_peer_id = PeerId::random();
        let requester_peer_id = PeerId::random();
        
        // Simulate full replication flow:
        
        // 1. Node A sends ReplicateBlobRequest to Node B
        let replicate_request = network::ReplicateBlobRequest { cid };
        
        // 2. Node B checks if it has the blob (it doesn't)
        let blob_exists = requester_blob_storage.blob_exists(&cid).await.unwrap();
        assert!(!blob_exists, "Requester should not have the blob initially");
        
        // 3. Node B initiates Kademlia get_providers query
        // Create a mock query ID using a simple wrapper type
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct MockQueryId(u64);
        let query_id = MockQueryId(42);
        
        // Create response channels - using a real response channel type
        // This is a simplification since we don't have the actual libp2p ResponseChannel implementation
        let (replicate_resp_sender, mut replicate_resp_receiver) = 
            tokio::sync::mpsc::channel::<network::ReplicateBlobResponse>(1);
        
        // Store the query state
        let mut pending_replication_fetches = HashMap::new();
        
        // Simulate storing in the pending_replication_fetches HashMap with a mock ResponseChannel
        struct MockResponseChannel;
        
        // Store the query context with our mock response channel
        pending_replication_fetches.insert(query_id, (cid, MockResponseChannel));
        
        // 4. Simulate Kademlia GetProvidersOk response with provider_peer_id
        let providers = vec![provider_peer_id];
        
        // 5. Extract the pending state - instead of unwrap() we'll just simulate this
        // by creating new values since we can't actually extract our MockResponseChannel
        let original_cid = cid;
        
        // 6. Node B sends FetchBlobRequest to Node C (provider)
        let fetch_request = network::FetchBlobRequest { cid: original_cid };
        
        // Create a channel to simulate the fetch protocol
        let (fetch_resp_sender, mut response_receiver) = 
            tokio::sync::mpsc::channel::<network::FetchBlobResponse>(1);
        
        // Store pending fetch state with mock request ID and response channel
        struct MockRequestId;
        let request_id = MockRequestId;
        let mut pending_blob_fetches: HashMap<MockRequestId, (cid::Cid, MockResponseChannel)> = HashMap::new();
        
        // We don't need to actually insert to the HashMap since we'll just simulate the fetch directly
        
        // 7. Node C processes FetchBlobRequest
        let provider_result = provider_blob_storage.get_blob(&cid).await;
        assert!(provider_result.is_ok(), "Provider should be able to retrieve the blob");
        let provider_data = provider_result.unwrap();
        assert!(provider_data.is_some(), "Provider should have the blob data");
        
        // 8. Node C sends FetchBlobResponse with data
        let fetch_response = network::FetchBlobResponse {
            data: provider_data,
            error_msg: None,
        };
        
        // Send the response via our channel
        fetch_resp_sender.send(fetch_response.clone()).await.unwrap();
        
        // 9. Receive the fetch response at Node B
        let received_fetch_response = response_receiver.recv().await.unwrap();
        
        // 10. Node B verifies hash, stores and pins the blob
        let blob_data = received_fetch_response.data.as_ref().unwrap();
        let mh = Code::Sha2_256.digest(blob_data);
        let calculated_cid = Cid::new_v0(mh).unwrap();
        assert_eq!(calculated_cid, original_cid, "CID of fetched data should match original");
        
        // Store the blob
        requester_blob_storage.put_blob(blob_data).await.unwrap();
        requester_blob_storage.pin_blob(&original_cid).await.unwrap();
        
        // Verify the blob is now in the requester's storage
        let exists = requester_blob_storage.blob_exists(&original_cid).await.unwrap();
        assert!(exists, "Blob should exist in requester's storage after fetch");
        
        // 11. Node B sends success response to Node A
        let success_response = network::ReplicateBlobResponse {
            success: true,
            error_msg: None,
        };
        
        // Send the response through our channel
        replicate_resp_sender.send(success_response).await.unwrap();
        
        // Verify a response was received
        let received_success_response = replicate_resp_receiver.recv().await.unwrap();
        assert!(received_success_response.success, "Replication response should indicate success");
        
        // Verify the full flow succeeded
        assert!(requester_blob_storage.is_pinned(&cid).await.unwrap(), 
                "Blob should be pinned in requester's storage after replication");
    }

    #[tokio::test]
    async fn test_p2p_blob_fetch_for_replication() {
        use cid::Cid;
        use icn_storage::{AsyncInMemoryStorage, DistributedStorage};
        use multihash::{Code, MultihashDigest};
        use std::collections::HashMap;
        use std::sync::Arc;
        use futures::lock::Mutex;
        use libp2p::PeerId;
        
        // Create test data
        let test_content = b"This is test content for P2P blob fetch via replication handler".to_vec();
        let mh = Code::Sha2_256.digest(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create storage and adapter for "local node"
        let local_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let local_blob_storage = Arc::new(BlobStorageAdapter::new(local_storage.clone()));
        
        // Create another storage to simulate a remote peer that has the blob
        let remote_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let remote_blob_storage = Arc::new(BlobStorageAdapter::new(remote_storage.clone()));
        
        // Put the blob in the remote storage
        remote_blob_storage.put_blob(&test_content).await.unwrap();
        remote_blob_storage.pin_blob(&cid).await.unwrap();
        
        // Verify the remote storage has the blob
        assert!(remote_blob_storage.blob_exists(&cid).await.unwrap());
        
        // Create simple identifiers for our mock network components
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct QueryId(u64);
        
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct RequestId(u64);
        
        // Create state tracking maps like in the real implementation
        let mut pending_replication_fetches = HashMap::<QueryId, (Cid, ())>::new();
        let mut pending_blob_fetches = HashMap::<RequestId, (Cid, ())>::new();
        
        // First, simulate storing the request in pending_replication_fetches
        let query_id = QueryId(1);
        pending_replication_fetches.insert(query_id, (cid, ()));
        
        // Now simulate receiving a Kademlia GetProviders result with a provider
        let provider_peer_id = PeerId::random();
        
        // Simulate sending the fetch request and storing it
        let fetch_request = network::FetchBlobRequest { cid };
        let fetch_request_id = RequestId(2);
        pending_blob_fetches.insert(fetch_request_id, (cid, ()));
        
        // Now simulate receiving a successful fetch response with the blob data
        let fetch_response = network::FetchBlobResponse {
            data: Some(test_content.clone()),
            error_msg: None,
        };
        
        // Simulate processing the response and storing it in the local storage
        if let Some((original_cid, _)) = pending_blob_fetches.remove(&fetch_request_id) {
            match &fetch_response.data {
                Some(blob_data) => {
                    // Verify the hash matches
                    let mh = Code::Sha2_256.digest(blob_data);
                    let calculated_cid = Cid::new_v0(mh).unwrap();
                    
                    assert_eq!(calculated_cid, original_cid, "Fetched blob hash should match original CID");
                    
                    // Store the blob
                    local_blob_storage.put_blob(blob_data).await.unwrap();
                    local_blob_storage.pin_blob(&original_cid).await.unwrap();
                    
                    // Verify the blob was stored and pinned
                    assert!(local_blob_storage.blob_exists(&original_cid).await.unwrap());
                    assert!(local_blob_storage.is_pinned(&original_cid).await.unwrap());
                },
                None => {
                    panic!("Fetch response should have data");
                }
            }
        } else {
            panic!("Could not find pending blob fetch for request ID");
        }
        
        // Final verification: the blob should be retrievable from local storage
        let retrieved_blob = local_blob_storage.get_blob(&cid).await.unwrap();
        assert!(retrieved_blob.is_some(), "Blob should be retrievable from local storage");
        assert_eq!(retrieved_blob.unwrap(), test_content, "Retrieved blob should match original content");
    }
} 