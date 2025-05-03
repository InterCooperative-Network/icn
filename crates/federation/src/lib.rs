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

    /// Initialize the debug API for development and testing
    /// 
    /// This method is only compiled when the "testing" feature is enabled.
    /// It creates a BasicDebugApi instance and registers HTTP routes for debugging.
    #[cfg(feature = "testing")]
    pub fn init_debug_api(&self) -> FederationResult<Arc<dyn debug_api::DebugApi>> {
        // Placeholder implementation that just logs and returns a dummy implementation
        // This will be properly implemented in a future update
        info!("Debug API initialization requested but disabled in this version");
        
        // Return a dummy implementation that does nothing
        Ok(Arc::new(DummyDebugApi {}))
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

// Placeholder for the run_event_loop function
// This is just a temporary stub to make the code compile
async fn run_event_loop(
    _swarm: Swarm<network::IcnFederationBehaviour>,
    _command_receiver: mpsc::Receiver<FederationManagerMessage>,
    _storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
    _blob_receiver: mpsc::Receiver<cid::Cid>,
    _fed_cmd_receiver: mpsc::Receiver<FederationCommand>,
) {
    // In a real implementation, this would process events from the libp2p swarm
    // For now, just loop forever to keep the task alive
    loop {
        // Sleep to avoid busy wait
        tokio::time::sleep(Duration::from_secs(1)).await;
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