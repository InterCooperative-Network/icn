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
    QuorumProof, QuorumConfig, Signature
};
use icn_storage::{FederationCommand, ReplicationPolicy, DistributedStorage};
use multihash::{self, MultihashDigest};
use tracing::{debug, info, error, warn};
use thiserror::Error;
use sha2::{Sha256, Digest};

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

// Export error types
pub mod errors;

// Re-export error types from the errors module
pub use errors::{FederationError, FederationResult, FederationResultExt, TrustBundleErrorKind};

// Add the debug_api module
pub mod debug_api;

// Re-export debug API types for integration testing
pub use debug_api::{DebugApi, ProposalStatusResponse, DagNodeResponse, FederationStatusResponse};

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
            .map_err(|e| FederationError::AuthenticationError(e.to_string()))
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
    /// Period between bootstrap attempts (connecting to known peers)
    pub bootstrap_period: Duration,
    /// Interval between peer discovery/sync operations
    pub peer_sync_interval: Duration,
    /// Interval between trust bundle synchronization attempts
    pub trust_bundle_sync_interval: Duration,
    /// Maximum number of peers to maintain connections with
    pub max_peers: usize,
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
            bootstrap_period: Duration::from_secs(30),
            peer_sync_interval: Duration::from_secs(60),
            trust_bundle_sync_interval: Duration::from_secs(300),
            max_peers: 25,
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
            .map_err(|e| FederationError::NetworkError(format!("Failed to create behavior: {}", e)))?;
        
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
                .map_err(|e| FederationError::NetworkError(format!("Failed to dial {}: {}", peer_addr, e)))?;
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
        // Get the storage lock
        let storage = self.storage.clone();
        let storage_guard = storage.lock().await;
        
        // Define the key for the latest epoch metadata
        let meta_key = "federation::latest_epoch";
        let meta_hash = create_sha256_multihash(meta_key.as_bytes());
        let meta_cid = cid::Cid::new_v1(0x71, meta_hash); // Raw codec
        
        // Try to get the latest epoch value from storage
        let result = storage_guard.get_kv(&meta_cid).await;
        
        match result {
            Ok(Some(epoch_bytes)) => {
                // Parse the stored epoch value
                let epoch_str = String::from_utf8_lossy(&epoch_bytes);
                epoch_str.parse::<u64>()
                    .map_err(|e| FederationError::InternalError(format!("Failed to parse epoch: {}", e)))
            },
            Ok(None) => {
                // No stored epoch value, return 0 as the initial epoch
                Ok(0)
            },
            Err(e) => {
                Err(FederationError::StorageError(format!("Failed to retrieve latest epoch: {}", e)))
            }
        }
    }
    
    /// Update the latest known epoch in storage
    pub async fn update_latest_known_epoch(&self, epoch: u64) -> FederationResult<()> {
        // Get current latest epoch
        let current_epoch = self.get_latest_known_epoch().await?;
        
        // Only update if the new epoch is higher than the current one
        if epoch <= current_epoch {
            return Ok(());
        }
        
        // Get the storage lock
        let storage = self.storage.clone();
        let storage_guard = storage.lock().await;
        
        // Define the key for the latest epoch metadata
        let meta_key = "federation::latest_epoch";
        let meta_hash = create_sha256_multihash(meta_key.as_bytes());
        let meta_cid = cid::Cid::new_v1(0x71, meta_hash); // Raw codec
        
        // Store the new epoch value
        let epoch_bytes = epoch.to_string().into_bytes();
        storage_guard.put_kv(meta_cid, epoch_bytes).await
            .map_err(|e| FederationError::StorageError(format!("Failed to update latest epoch: {}", e)))
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
                        let key_hash = create_sha256_multihash(key_str.as_bytes());
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
                                        let _ = respond_to.send(Err(FederationError::SerializationError(
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
            let key_hash = create_sha256_multihash(key_str.as_bytes());
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
                        
                        // Check received_bundle.epoch_id against local state for replay protection
                        // Create a FederationManager instance from the swarm to access the storage
                        let fed_manager = FederationManager {
                            local_peer_id: swarm.local_peer_id().clone(),
                            keypair: identity::Keypair::generate_ed25519(), // Create a new keypair since we can't access the existing one
                            sender: mpsc::channel(1).0, // Dummy sender, not used in this context
                            _event_loop_handle: tokio::spawn(async {}), // Dummy task, not used
                            known_peers: HashMap::new(),
                            config: FederationManagerConfig::default(),
                            storage: Arc::clone(&storage),
                        };
                        
                        // Get latest known epoch
                        let current_latest_epoch = match fed_manager.get_latest_known_epoch().await {
                            Ok(epoch) => epoch,
                            Err(e) => {
                                error!("Failed to get latest known epoch: {}", e);
                                0 // Default to 0 in case of error
                            }
                        };
                        
                        // Only process if this is a new epoch
                        if received_bundle.epoch_id <= current_latest_epoch {
                            warn!("Received TrustBundle for epoch {} is not newer than our current latest epoch {}, ignoring", 
                                 received_bundle.epoch_id, current_latest_epoch);
                            return;
                        }
                        
                        // Serialize the bundle for storage
                        match serde_json::to_vec(received_bundle) {
                            Ok(bundle_bytes) => {
                                // Generate the storage key based on epoch_id
                                let key_str = format!("trustbundle::epoch::{}", received_bundle.epoch_id);
                                let key_hash = create_sha256_multihash(key_str.as_bytes());
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
                                        
                                        // Update latest known epoch
                                        if let Err(e) = fed_manager.update_latest_known_epoch(received_bundle.epoch_id).await {
                                            error!("Failed to update latest known epoch: {}", e);
                                        } else {
                                            info!("Updated latest known epoch to {}", received_bundle.epoch_id);
                                            
                                            // Optionally notify other systems about the new TrustBundle
                                            // (could emit an event or callback here)
                                        }
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
                        error!("TrustBundle verification failed for epoch {}", received_bundle.epoch_id);
                    },
                    Err(e) => {
                        error!("Failed to verify TrustBundle: {}", e);
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
                        let mh = create_sha256_multihash(&blob_data);
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
                Ok(Some(bundle)) => {
                    debug!("Successfully received TrustBundle for epoch {}", epoch);
                    // Try getting the next epoch, but we need to use Box::pin to handle the recursion
                    let next_epoch = epoch + 1;
                    let next_request = request_trust_bundle_from_network(sender.clone(), next_epoch);
                    if let Err(e) = Box::pin(next_request).await {
                        debug!("No more TrustBundles found after epoch {}: {}", epoch, e);
                    }
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
    use cid::Cid;
    use icn_storage::{AsyncInMemoryStorage, DistributedStorage};
    use libp2p::PeerId;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn test_request_response_types() {
        // Test TrustBundleRequest
        let request = network::TrustBundleRequest { epoch: 42 };
        assert_eq!(request.epoch, 42);
        
        // Test TrustBundleResponse with None
        let response_none = network::TrustBundleResponse { bundle: None };
        assert!(response_none.bundle.is_none());
        
        // Test ReplicateBlobRequest
        let cid = Cid::new_v0(create_sha256_multihash(b"test_blob")).unwrap();
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
        // Create test data
        let test_content = b"This is test content for blob replication protocol".to_vec();
        let mh = create_sha256_multihash(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create mock storage with the test blob
        let storage = icn_storage::AsyncInMemoryStorage::new();
        
        // Create blob store adapter
        let blob_store_adapter = BlobStorageAdapter::new(Arc::new(Mutex::new(storage)));
        
        // Put and pin the test blob
        blob_store_adapter.put_blob(&test_content).await.unwrap();
        blob_store_adapter.pin_blob(&cid).await.unwrap();
        
        // Create replication request
        let _request = network::ReplicateBlobRequest { cid };
        
        // Create response
        let _response = network::ReplicateBlobResponse {
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
        // Create test data
        let test_content = b"This is test content for P2P blob fetch".to_vec();
        let mh = create_sha256_multihash(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create storage and adapter
        let storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let blob_storage = Arc::new(BlobStorageAdapter::new(storage.clone()));
        
        // Create a mock query ID using a simple wrapper type
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        struct MockQueryId(u64);
        let _query_id = MockQueryId(42);
        
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
    async fn test_blob_fetch_protocol() {
        // Create test data
        let test_content = b"This is test content for blob fetch protocol".to_vec();
        let mh = create_sha256_multihash(&test_content);
        let cid = Cid::new_v0(mh).unwrap();
        
        // Create storage and adapter for provider
        let provider_storage = Arc::new(Mutex::new(AsyncInMemoryStorage::new()));
        let provider_blob_storage = Arc::new(BlobStorageAdapter::new(provider_storage.clone()));
        
        // Add the blob to the provider's storage
        provider_blob_storage.put_blob(&test_content).await.unwrap();
        provider_blob_storage.pin_blob(&cid).await.unwrap();
        
        // Create a fetch request
        let fetch_request = network::FetchBlobRequest {
            cid,
        };
        
        // Create a channel to simulate the fetch protocol
        let (fetch_resp_sender, mut response_receiver) = 
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
                
                fetch_resp_sender.send(response).await.unwrap();
            },
            Ok(None) => {
                let response = network::FetchBlobResponse {
                    data: None,
                    error_msg: Some("Blob not found".to_string()),
                };
                
                fetch_resp_sender.send(response).await.unwrap();
            },
            Err(e) => {
                let response = network::FetchBlobResponse {
                    data: None,
                    error_msg: Some(format!("Error: {}", e)),
                };
                
                fetch_resp_sender.send(response).await.unwrap();
            }
        }
        
        // Receive the response
        let fetch_response = response_receiver.recv().await.unwrap();
        
        // Verify the response
        assert!(fetch_response.data.is_some(), "Fetch response should contain blob data");
        assert!(fetch_response.error_msg.is_none(), "Fetch response should not have an error");
        
        // Verify the data hash
        let blob_data = fetch_response.data.unwrap();
        let calculated_mh = create_sha256_multihash(&blob_data);
        let calculated_cid = Cid::new_v0(calculated_mh).unwrap();
        assert_eq!(calculated_cid, cid, "CID of fetched data should match original");
    }
    
    #[tokio::test]
    async fn test_p2p_blob_fetch_for_replication() {
        // Create test data
        let test_content = b"This is test content for P2P blob fetch via replication handler".to_vec();
        let mh = create_sha256_multihash(&test_content);
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
                    let calculated_mh = create_sha256_multihash(blob_data);
                    let calculated_cid = Cid::new_v0(calculated_mh).unwrap();
                    
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

/// Helper function to create a multihash using SHA-256
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
} 