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
use cid::Cid;

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
    /// Get connected peers
    GetConnectedPeers {
        respond_to: tokio::sync::oneshot::Sender<FederationResult<Vec<String>>>,
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
    #[allow(dead_code)]
    _event_loop_handle: JoinHandle<()>,
    /// Known peers
    known_peers: HashMap<PeerId, Multiaddr>,
    /// Configuration
    config: FederationManagerConfig,
    /// Storage backend for storing TrustBundles
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
}

impl Clone for FederationManager {
    fn clone(&self) -> Self {
        // Create a dummy JoinHandle since it can't be cloned
        let dummy_handle = tokio::spawn(async {});
        
        Self {
            local_peer_id: self.local_peer_id,
            keypair: self.keypair.clone(),
            sender: self.sender.clone(),
            _event_loop_handle: dummy_handle,
            known_peers: self.known_peers.clone(),
            config: self.config.clone(),
            storage: self.storage.clone(),
        }
    }
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

    /// Get the listen addresses for this federation node
    pub fn get_listen_addresses(&self) -> Vec<Multiaddr> {
        // Return a clone of the listen addresses from the config
        self.config.listen_addresses.clone()
    }

    /// Get connected peers
    pub async fn get_connected_peers(&self) -> FederationResult<Vec<String>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        // Create a custom message for getting connected peers
        // We'll add this variant to the FederationManagerMessage enum separately
        self.sender.send(FederationManagerMessage::GetConnectedPeers {
            respond_to: tx,
        }).await
        .map_err(|e| FederationError::NetworkError(format!("Failed to send connected peers request: {}", e)))?;
        
        rx.await
            .map_err(|e| FederationError::NetworkError(format!("Failed to receive connected peers: {}", e)))?
    }

    /// Initialize the debug API for development and testing
    /// 
    /// This method is only compiled when the "testing" feature is enabled.
    /// It creates a BasicDebugApi instance and registers HTTP routes for debugging.
    #[cfg(feature = "testing")]
    pub fn init_debug_api(&self) -> FederationResult<Arc<dyn debug_api::DebugApi>> {
        // Create a new BasicDebugApi instance with storage and federation manager
        let debug_api = Arc::new(debug_api::BasicDebugApi::new(
            self.storage.clone(),
            Arc::new(self.clone()),
        ));
        
        // Return the debug API
        Ok(debug_api)
    }
}

/// Dummy implementation of DebugApi that does nothing
#[cfg(feature = "testing")]
struct DummyDebugApi;

#[cfg(feature = "testing")]
#[async_trait::async_trait]
impl debug_api::DebugApi for DummyDebugApi {
    async fn query_proposal_status(&self, _proposal_cid: &cid::Cid) -> FederationResult<debug_api::ProposalStatusResponse> {
        Ok(debug_api::ProposalStatusResponse {
            exists: false,
            status: "NotImplemented".to_string(),
            created_at: None,
            finalized_at: None,
            vote_count: 0,
            executed: false,
        })
    }
    
    async fn query_dag_node(&self, _node_cid: &cid::Cid) -> FederationResult<Option<debug_api::DagNodeResponse>> {
        Ok(None)
    }
    
    async fn query_federation_status(&self) -> FederationResult<debug_api::FederationStatusResponse> {
        Ok(debug_api::FederationStatusResponse {
            current_epoch: 0,
            node_count: 0,
            connected_peers: 0,
            validator_count: 0,
            guardian_count: 0,
            observer_count: 0,
        })
    }
    
    async fn query_connected_peers(&self) -> FederationResult<Vec<String>> {
        Ok(Vec::new())
    }
    
    async fn query_current_trust_bundle(&self) -> FederationResult<Option<TrustBundle>> {
        Ok(None)
    }
}

// Create SHA-256 multihash from data
pub fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    
    // Create multihash (0x12 = SHA-256)
    cid::multihash::Multihash::wrap(0x12, hash.as_slice())
        .expect("Failed to create multihash")
}

// The event loop for the federation node
async fn run_event_loop(
    mut swarm: Swarm<network::IcnFederationBehaviour>,
    mut command_receiver: mpsc::Receiver<FederationManagerMessage>,
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    mut blob_receiver: mpsc::Receiver<cid::Cid>,
    mut fed_cmd_receiver: mpsc::Receiver<FederationCommand>,
) {
    // Track inflight Kademlia queries for providers
    let mut pending_provider_queries: HashMap<kad::QueryId, cid::Cid> = HashMap::new();
    
    // Track pending blob replication requests
    // Maps QueryId -> (CID, ResponseChannel)
    let mut pending_replication_fetches: HashMap<
        kad::QueryId, 
        (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)
    > = HashMap::new();
    
    // Track pending fetch operations
    let mut pending_blob_fetches: HashMap<
        request_response::OutboundRequestId, 
        (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)
    > = HashMap::new();
    
    // Create a blob storage adapter to interact with storage
    let blob_storage = BlobStorageAdapter { storage: storage.clone() };
    
    loop {
        tokio::select! {
            // Handle incoming events from the swarm
            event = swarm.select_next_some() => {
                match event {
                    // Handle behavior events
                    SwarmEvent::Behaviour(behavior_event) => {
                        match behavior_event {
                            // Handle incoming blob replication requests
                            network::IcnFederationBehaviourEvent::BlobReplication(
                                request_response::Event::Message { 
                                    message: request_response::Message::Request { 
                                        request, 
                                        channel, 
                                        .. 
                                    }, 
                                    .. 
                                }
                            ) => {
                                handle_blob_replication_request(
                                    request,
                                    channel,
                                    &mut swarm,
                                    &blob_storage,
                                    &mut pending_provider_queries,
                                    &mut pending_replication_fetches,
                                ).await;
                            },
                            // Handle Kademlia provider results
                            network::IcnFederationBehaviourEvent::Kademlia(
                                kad::Event::OutboundQueryProgressed { 
                                    id, 
                                    result: kad::QueryResult::GetProviders(Ok(providers)), 
                                    .. 
                                }
                            ) => {
                                handle_kademlia_get_providers_ok(
                                    id,
                                    providers,
                                    &mut swarm,
                                    &mut pending_provider_queries,
                                    &mut pending_replication_fetches,
                                    &mut pending_blob_fetches,
                                ).await;
                            },
                            // Handle Kademlia provider query failure
                            network::IcnFederationBehaviourEvent::Kademlia(
                                kad::Event::OutboundQueryProgressed { 
                                    id, 
                                    result: kad::QueryResult::GetProviders(Err(e)), 
                                    .. 
                                }
                            ) => {
                                handle_kademlia_get_providers_error(
                                    id,
                                    e,
                                    &mut swarm,
                                    &mut pending_provider_queries,
                                    &mut pending_replication_fetches,
                                ).await;
                            },
                            // Handle blob fetch responses
                            network::IcnFederationBehaviourEvent::BlobFetch(
                                request_response::Event::Message { 
                                    message: request_response::Message::Response { 
                                        request_id, 
                                        response, 
                                    }, 
                                    .. 
                                }
                            ) => {
                                handle_blob_fetch_response(
                                    request_id,
                                    response,
                                    &mut swarm,
                                    &blob_storage,
                                    &mut pending_blob_fetches,
                                ).await;
                            },
                            // Handle blob fetch request failure
                            network::IcnFederationBehaviourEvent::BlobFetch(
                                request_response::Event::OutboundFailure { 
                                    request_id, 
                                    error, 
                                    .. 
                                }
                            ) => {
                                handle_blob_fetch_failure(
                                    request_id,
                                    error,
                                    &mut swarm,
                                    &mut pending_blob_fetches,
                                ).await;
                            },
                            // Handle other Kademlia events like closest peers for replication
                            network::IcnFederationBehaviourEvent::Kademlia(
                                kad::Event::OutboundQueryProgressed { 
                                    id, 
                                    result: kad::QueryResult::GetClosestPeers(Ok(peers)), 
                                    .. 
                                }
                            ) => {
                                // This would be handled by the identify_replication_targets function
                                debug!("Received closest peers result: {:?}", peers);
                            },
                            // Handle other events
                            _ => {
                                // Not handling other events in this implementation
                            }
                        }
                    },
                    // Handle other swarm events
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        info!(%peer_id, "Connection established");
                    },
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        info!(%peer_id, "Connection closed");
                    },
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(%address, "Listening on new address");
                    },
                    _ => {
                        // Ignore other events
                    }
                }
            },
            
            // Handle incoming blob announcements
            Some(cid) = blob_receiver.recv() => {
                debug!(%cid, "Received blob announcement");
                
                // Announce that we are a provider for this CID
                if let Err(e) = swarm.behaviour_mut().kademlia.start_providing(kad::RecordKey::new(&cid.to_bytes())) {
                    error!(%cid, "Failed to announce as provider: {}", e);
                }
            },
            
            // Handle federation commands
            Some(command) = fed_cmd_receiver.recv() => {
                match command {
                    FederationCommand::AnnounceBlob(cid) => {
                        debug!(%cid, "Announcing blob as provider");
                        
                        // Announce that we are a provider for this CID
                        if let Err(e) = swarm.behaviour_mut().kademlia.start_providing(kad::RecordKey::new(&cid.to_bytes())) {
                            error!(%cid, "Failed to announce as provider: {}", e);
                        }
                    },
                    FederationCommand::IdentifyReplicationTargets { cid, policy, context_id, .. } => {
                        handle_identify_replication_targets(
                            cid,
                            policy,
                            context_id,
                            &mut swarm,
                            &storage,
                        ).await;
                    },
                }
            },
            
            // Handle manager messages
            Some(message) = command_receiver.recv() => {
                match message {
                    FederationManagerMessage::RequestTrustBundle { epoch, respond_to } => {
                        debug!("Requesting trust bundle for epoch {}", epoch);
                        // TODO: Implement trust bundle request logic
                        let _ = respond_to.send(Ok(None));
                    },
                    FederationManagerMessage::PublishTrustBundle { bundle, respond_to } => {
                        debug!("Publishing trust bundle for epoch {}", bundle.epoch_id);
                        // TODO: Implement trust bundle publish logic
                        let _ = respond_to.send(Ok(()));
                    },
                    FederationManagerMessage::AnnounceBlob { cid, respond_to } => {
                        debug!(%cid, "Announcing blob as provider");
                        
                        // Announce that we are a provider for this CID
                        match swarm.behaviour_mut().kademlia.start_providing(kad::RecordKey::new(&cid.to_bytes())) {
                            Ok(_) => {
                                if let Some(sender) = respond_to {
                                    let _ = sender.send(Ok(()));
                                }
                            },
                            Err(e) => {
                                error!(%cid, "Failed to announce as provider: {}", e);
                                if let Some(sender) = respond_to {
                                    let _ = sender.send(Err(FederationError::NetworkError(format!(
                                        "Failed to announce as provider: {}", e
                                    ))));
                                }
                            }
                        }
                    },
                    FederationManagerMessage::IdentifyReplicationTargets { cid, policy, context_id, respond_to } => {
                        // Delegate to the handler function
                        let target_peers = 
                            handle_identify_replication_targets(cid, policy, context_id, &mut swarm, &storage).await;
                        
                        // Send the response if a channel was provided
                        if let Some(sender) = respond_to {
                            let _ = sender.send(Ok(target_peers));
                        }
                    },
                    FederationManagerMessage::Shutdown { respond_to } => {
                        info!("Received shutdown request");
                        
                        // Send confirmation and exit the loop
                        let _ = respond_to.send(());
                        break;
                    },
                    FederationManagerMessage::GetConnectedPeers { respond_to } => {
                        // Implement the logic to get connected peers
                        let connected_peers = get_connected_peers().await;
                        let _ = respond_to.send(Ok(connected_peers));
                    },
                }
            },
        }
    }
}

/// Handle incoming blob replication requests
pub async fn handle_blob_replication_request(
    request: network::ReplicateBlobRequest,
    channel: request_response::ResponseChannel<network::ReplicateBlobResponse>,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    blob_storage: &BlobStorageAdapter,
    pending_provider_queries: &mut HashMap<kad::QueryId, cid::Cid>,
    pending_replication_fetches: &mut HashMap<kad::QueryId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
) {
    let cid = request.cid;
    debug!(%cid, "Received ReplicateBlobRequest");
    
    // Check if we already have the blob
    match blob_storage.blob_exists(&cid).await {
        Ok(true) => {
            // Blob is present, try to pin it
            info!(%cid, "Blob exists locally, pinning it");
            match blob_storage.pin_blob(&cid).await {
                Ok(_) => {
                    // Successfully pinned, send success response
                    info!(%cid, "Successfully pinned blob");
                    let response = network::ReplicateBlobResponse {
                        success: true,
                        error_msg: None,
                    };
                    if let Err(e) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                        error!(%cid, "Failed to send success response: {:?}", e);
                    }
                },
                Err(e) => {
                    // Failed to pin, send error response
                    error!(%cid, "Failed to pin blob: {}", e);
                    let response = network::ReplicateBlobResponse {
                        success: false,
                        error_msg: Some(format!("Failed to pin blob: {}", e)),
                    };
                    if let Err(send_err) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                        error!(%cid, "Failed to send error response: {:?}", send_err);
                    }
                }
            }
        },
        Ok(false) => {
            // Blob not present, initiate Kademlia query for providers
            info!(%cid, "Blob not found locally, searching for providers");
            
            // Start a Kademlia query for providers of this CID
            let record_key = kad::RecordKey::new(&cid.to_bytes());
            let query_id = swarm.behaviour_mut().kademlia.get_providers(record_key);
            
            // Store the query ID, CID, and response channel
            pending_replication_fetches.insert(query_id, (cid, channel));
            pending_provider_queries.insert(query_id, cid);
            debug!(%cid, ?query_id, "Started Kademlia query for providers");
        },
        Err(e) => {
            // Error checking blob existence, send error response
            error!(%cid, "Error checking if blob exists: {}", e);
            let response = network::ReplicateBlobResponse {
                success: false,
                error_msg: Some(format!("Storage error: {}", e)),
            };
            if let Err(send_err) = swarm.behaviour_mut().blob_replication.send_response(channel, response) {
                error!(%cid, "Failed to send error response: {:?}", send_err);
            }
        }
    }
}

/// Handle successful Kademlia GetProviders query results
async fn handle_kademlia_get_providers_ok(
    id: kad::QueryId,
    providers_result: kad::GetProvidersOk,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    pending_provider_queries: &mut HashMap<kad::QueryId, cid::Cid>,
    pending_replication_fetches: &mut HashMap<kad::QueryId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
    pending_blob_fetches: &mut HashMap<request_response::OutboundRequestId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
) {
    // Extract the providers from the correct enum variant
    let providers = match providers_result {
        kad::GetProvidersOk::FoundProviders { providers, .. } => providers,
        kad::GetProvidersOk::FinishedWithNoAdditionalRecord { closest_peers } => {
            // If we didn't find any providers, use the closest peers as potential providers
            closest_peers.into_iter().collect()
        }
    };
    
    // Check if this is for a replication fetch
    if let Some((cid, channel)) = pending_replication_fetches.remove(&id) {
        if providers.is_empty() {
            // No providers found, send failure response
            warn!(%cid, "No providers found for blob");
            let response = network::ReplicateBlobResponse {
                success: false,
                error_msg: Some("No providers found for this blob".to_string()),
            };
            let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
            return;
        }
        
        // Found providers, attempt to fetch the blob from the first provider
        let provider = providers.iter().next().unwrap();
        info!(%cid, %provider, "Found provider for blob, initiating fetch");
        
        // Create a fetch request
        let fetch_request = network::FetchBlobRequest { cid };
        
        // Send the request to the provider
        let request_id = swarm
            .behaviour_mut()
            .blob_fetch
            .send_request(provider, fetch_request);
        
        // Store the request ID, CID, and the original response channel
        pending_blob_fetches.insert(request_id, (cid, channel));
        
        // Also remove from pending provider queries
        pending_provider_queries.remove(&id);
    } else if let Some(cid) = pending_provider_queries.remove(&id) {
        // This was just a provider query, not a replication fetch
        info!(%cid, "Found {} providers for blob", providers.len());
    }
}

/// Handle Kademlia GetProviders query errors
async fn handle_kademlia_get_providers_error(
    id: kad::QueryId,
    error: kad::GetProvidersError,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    pending_provider_queries: &mut HashMap<kad::QueryId, cid::Cid>,
    pending_replication_fetches: &mut HashMap<kad::QueryId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
) {
    // Check if this is for a replication fetch
    if let Some((cid, channel)) = pending_replication_fetches.remove(&id) {
        // Failed to find providers, send failure response
        error!(%cid, "Failed to find providers: {}", error);
        let response = network::ReplicateBlobResponse {
            success: false,
            error_msg: Some(format!("Failed to find providers: {}", error)),
        };
        let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
        
        // Also remove from pending provider queries
        pending_provider_queries.remove(&id);
    } else if let Some(cid) = pending_provider_queries.remove(&id) {
        // This was just a provider query, not a replication fetch
        warn!(%cid, "Failed to find providers: {}", error);
    }
}

/// Handle blob fetch response for a replication fetch
async fn handle_blob_fetch_response(
    request_id: request_response::OutboundRequestId,
    response: network::FetchBlobResponse,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    blob_storage: &BlobStorageAdapter,
    pending_blob_fetches: &mut HashMap<request_response::OutboundRequestId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
) {
    // Check if this is a response to a pending fetch
    if let Some((cid, channel)) = pending_blob_fetches.remove(&request_id) {
        if let Some(data) = response.data {
            // Got the data, store it and pin it
            info!(%cid, "Received blob data, storing and pinning");
            
            // Store the blob
            match blob_storage.put_blob(&data).await {
                Ok(stored_cid) => {
                    if stored_cid != cid {
                        // CID mismatch, send failure response
                        error!(%cid, actual_cid = %stored_cid, "CID mismatch for fetched blob");
                        let response = network::ReplicateBlobResponse {
                            success: false,
                            error_msg: Some("CID mismatch for fetched blob".to_string()),
                        };
                        let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
                        return;
                    }
                    
                    // Pin the blob
                    match blob_storage.pin_blob(&cid).await {
                        Ok(_) => {
                            // Successfully stored and pinned, send success response
                            info!(%cid, "Successfully stored and pinned fetched blob");
                            let response = network::ReplicateBlobResponse {
                                success: true,
                                error_msg: None,
                            };
                            let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
                        },
                        Err(e) => {
                            // Failed to pin, send error response
                            error!(%cid, "Failed to pin fetched blob: {}", e);
                            let response = network::ReplicateBlobResponse {
                                success: false,
                                error_msg: Some(format!("Failed to pin fetched blob: {}", e)),
                            };
                            let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
                        }
                    }
                },
                Err(e) => {
                    // Failed to store, send error response
                    error!(%cid, "Failed to store fetched blob: {}", e);
                    let response = network::ReplicateBlobResponse {
                        success: false,
                        error_msg: Some(format!("Failed to store fetched blob: {}", e)),
                    };
                    let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
                }
            }
        } else {
            // No data received, send failure response
            warn!(%cid, "No data received for fetched blob");
            let response = network::ReplicateBlobResponse {
                success: false,
                error_msg: response.error_msg.or(Some("No data received from provider".to_string())),
            };
            let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
        }
    }
}

/// Handle blob fetch failures
async fn handle_blob_fetch_failure(
    request_id: request_response::OutboundRequestId,
    error: request_response::OutboundFailure,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    pending_blob_fetches: &mut HashMap<request_response::OutboundRequestId, (cid::Cid, request_response::ResponseChannel<network::ReplicateBlobResponse>)>,
) {
    // Check if this is a failure for a pending fetch
    if let Some((cid, channel)) = pending_blob_fetches.remove(&request_id) {
        // Failed to fetch blob, send failure response
        error!(%cid, "Failed to fetch blob: {}", error);
        let response = network::ReplicateBlobResponse {
            success: false,
            error_msg: Some(format!("Failed to fetch blob: {}", error)),
        };
        let _ = swarm.behaviour_mut().blob_replication.send_response(channel, response);
    }
}

/// Handle identifying replication targets
async fn handle_identify_replication_targets(
    cid: cid::Cid,
    policy: ReplicationPolicy,
    context_id: Option<String>,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    storage: &Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
) -> Vec<PeerId> {
    // Get the context ID or use a default
    let ctx_id = context_id.unwrap_or("default".to_string());
    
    debug!(%cid, context = %ctx_id, "Identifying replication targets");
    
    // Lookup replication policy for this context if using a default policy
    let effective_policy = match &policy {
        ReplicationPolicy::Factor(0) | ReplicationPolicy::None => {
            if let Ok(context_policy) = roles::get_replication_policy(&ctx_id, storage.clone()).await {
                debug!(%cid, context = %ctx_id, "Using context policy: {:?}", context_policy);
                context_policy
            } else {
                debug!(%cid, context = %ctx_id, "Using provided policy: {:?}", policy);
                policy
            }
        },
        _ => {
            debug!(%cid, context = %ctx_id, "Using provided policy: {:?}", policy);
            policy
        }
    };
    
    // For simplicity, we're using the list of connected peers directly
    // Get the list of connected peers directly from the swarm
    let connected_peers: Vec<PeerId> = swarm
        .connected_peers()
        .map(|peer_id| *peer_id)
        .collect();
    
    // Select target peers based on the policy
    let target_peers = replication::identify_target_peers(
        &cid,
        &effective_policy,
        connected_peers,
        swarm.local_peer_id(),
    ).await;
    
    // Initiate replication to the target peers
    if !target_peers.is_empty() {
        if let Err(e) = replication::replicate_to_peers(&cid, &target_peers, swarm).await {
            error!(%cid, "Failed to initiate replication: {}", e);
        }
    }
    
    // Return the selected target peers
    target_peers
}

/// Helper struct for interacting with storage
pub struct BlobStorageAdapter {
    pub storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
}

impl BlobStorageAdapter {
    /// Check if a blob exists in storage
    pub async fn blob_exists(&self, cid: &cid::Cid) -> FederationResult<bool> {
        let storage_guard = self.storage.lock().await;
        storage_guard.contains_blob(cid).await
            .map_err(|e| FederationError::StorageError(format!("Failed to check blob existence: {}", e)))
    }
    
    /// Pin a blob in storage
    pub async fn pin_blob(&self, cid: &cid::Cid) -> FederationResult<()> {
        let storage_guard = self.storage.lock().await;
        
        // First check if the blob exists
        let blob_exists = storage_guard.contains_blob(cid).await
            .map_err(|e| FederationError::StorageError(format!("Failed to check blob existence: {}", e)))?;
        
        if !blob_exists {
            return Err(FederationError::StorageError(format!("Blob not found: {}", cid)));
        }
        
        // Actual pinning would be implemented via a DistributedStorage trait
        // For now, we'll just return success if the blob exists
        debug!(%cid, "Successfully pinned blob (simulated)");
        
        Ok(())
    }
    
    /// Store a blob in storage
    pub async fn put_blob(&self, data: &[u8]) -> FederationResult<cid::Cid> {
        let storage_guard = self.storage.lock().await;
        storage_guard.put_blob(data).await
            .map_err(|e| FederationError::StorageError(format!("Failed to store blob: {}", e)))
    }
    
    /// Get a blob from storage
    pub async fn get_blob(&self, cid: &cid::Cid) -> FederationResult<Option<Vec<u8>>> {
        let storage_guard = self.storage.lock().await;
        storage_guard.get_blob(cid).await
            .map_err(|e| FederationError::StorageError(format!("Failed to get blob: {}", e)))
    }
}

// Placeholder for get_latest_known_epoch
// This is just a temporary stub to make the code compile
async fn get_latest_known_epoch() -> u64 {
    // In a real implementation, this would query storage for the latest epoch
    // For now, just return 0
    0
}

// Placeholder for request_trust_bundle_from_network
// This is just a temporary stub to make the code compile
async fn request_trust_bundle_from_network(
    _sender: mpsc::Sender<FederationManagerMessage>,
    _epoch: u64,
) -> FederationResult<()> {
    // In a real implementation, this would request a trust bundle from peers
    // For now, just return Ok
    Ok(())
}

/// Helper function to get a list of connected peer IDs
async fn get_connected_peers() -> Vec<String> {
    // In a full implementation, this would access the swarm and get connected peers
    // For now, this is a placeholder implementation
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_blob_storage_adapter_basics() {
        // This test simply passes to ensure the module compiles
    }
}