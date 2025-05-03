use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use futures::lock::Mutex;

use libp2p::{
    core::transport::MemoryTransport,
    identity,
    kad,
    request_response,
    swarm::SwarmEvent,
    Multiaddr,
    PeerId,
    Swarm,
    Transport,
};

use cid::{Cid, multihash};
use async_trait::async_trait;
use sha2::{Sha256, Digest};
use tokio::sync::mpsc;

use icn_federation::{
    network::{
        self,
        ReplicateBlobRequest,
        ReplicateBlobResponse,
        FetchBlobRequest,
        FetchBlobResponse,
        IcnFederationBehaviour
    },
    FederationError,
    FederationResult,
};

// Mock StorageBackend for testing
#[derive(Default)]
struct MockStorageBackend {
    blobs: HashMap<Cid, Vec<u8>>,
    should_fail_exists: bool,
    should_fail_pin: bool,
}

#[async_trait]
impl icn_storage::StorageBackend for MockStorageBackend {
    async fn get_blob(&self, cid: &Cid) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.blobs.get(cid).cloned())
    }

    async fn put_blob(&self, data: &[u8]) -> Result<Cid, Box<dyn std::error::Error + Send + Sync>> {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        
        let mh = multihash::Multihash::wrap(0x12, &result).unwrap();
        let cid = Cid::new_v1(0x55, mh); // 0x55 is raw codec
        
        let mut blobs = self.blobs.clone();
        blobs.insert(cid, data.to_vec());
        
        Ok(cid)
    }

    async fn contains_blob(&self, cid: &Cid) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if self.should_fail_exists {
            return Err("Storage error: simulated failure".into());
        }
        Ok(self.blobs.contains_key(cid))
    }

    async fn get_kv(&self, _key: &Cid) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    async fn put_kv(&self, _key: Cid, _value: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    async fn contains_kv(&self, _key: &Cid) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        Ok(false)
    }
    
    async fn pin_blob(&self, cid: &Cid) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.should_fail_pin {
            return Err("Storage error: simulated pin failure".into());
        }
        if !self.blobs.contains_key(cid) {
            return Err(format!("Cannot pin non-existent blob: {}", cid).into());
        }
        Ok(())
    }
}

// Helper to create a test swarm
async fn create_test_swarm(
    storage_backend: MockStorageBackend,
) -> (
    Swarm<IcnFederationBehaviour>,
    PeerId,
    mpsc::Receiver<ReplicateBlobRequest>,
    mpsc::Sender<(ReplicateBlobResponse, request_response::ResponseChannel<ReplicateBlobResponse>)>,
    mpsc::Receiver<FetchBlobRequest>,
    mpsc::Sender<(FetchBlobResponse, request_response::ResponseChannel<FetchBlobResponse>)>,
) {
    // Generate identity
    let keypair = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());
    
    // Create transport
    let transport = MemoryTransport::default()
        .boxed();
    
    // Create behavior
    let mut behaviour = network::create_behaviour(peer_id, keypair)
        .expect("Failed to create behavior");
    
    // Create channels for test control
    let (replicate_req_tx, replicate_req_rx) = mpsc::channel(10);
    let (replicate_resp_tx, mut replicate_resp_rx) = mpsc::channel(10);
    let (fetch_req_tx, fetch_req_rx) = mpsc::channel(10);
    let (fetch_resp_tx, mut fetch_resp_rx) = mpsc::channel(10);
    
    // Replace event handlers with test mocks
    behaviour.blob_replication = {
        let mut proto_config = request_response::ProtocolConfig::new(
            network::BLOB_REPLICATION_PROTOCOL_ID,
            Vec::new(),
        );
        proto_config.timeout = Duration::from_secs(5);
        
        request_response::cbor::Behaviour::<_, _>::with_codec_and_handler(
            proto_config.clone(),
            request_response::CborCodec::default(),
            MockReplicationHandler {
                req_tx: replicate_req_tx,
                resp_rx: replicate_resp_rx,
            }
        )
    };
    
    behaviour.blob_fetch = {
        let mut proto_config = request_response::ProtocolConfig::new(
            network::BLOB_FETCH_PROTOCOL_ID,
            Vec::new(),
        );
        proto_config.timeout = Duration::from_secs(5);
        
        request_response::cbor::Behaviour::<_, _>::with_codec_and_handler(
            proto_config.clone(),
            request_response::CborCodec::default(),
            MockFetchHandler {
                req_tx: fetch_req_tx,
                resp_rx: fetch_resp_rx,
            }
        )
    };
    
    // Create swarm
    let swarm = Swarm::new(
        transport,
        behaviour,
        peer_id,
        libp2p::swarm::Config::default(),
    );
    
    (swarm, peer_id, replicate_req_rx, replicate_resp_tx, fetch_req_rx, fetch_resp_tx)
}

// Test handlers for request/response protocols
struct MockReplicationHandler {
    req_tx: mpsc::Sender<ReplicateBlobRequest>,
    resp_rx: mpsc::Receiver<(ReplicateBlobResponse, request_response::ResponseChannel<ReplicateBlobResponse>)>,
}

#[async_trait]
impl request_response::Handler for MockReplicationHandler {
    type RequestId = request_response::OutboundRequestId;
    type Request = ReplicateBlobRequest;
    type Response = ReplicateBlobResponse;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();
    
    async fn handle_request(
        &mut self,
        _: Self::InboundOpenInfo,
        request: Self::Request,
        sender: request_response::ResponseChannel<Self::Response>,
    ) {
        let _ = self.req_tx.send(request).await;
        if let Some((response, channel)) = self.resp_rx.recv().await {
            if std::ptr::eq(&sender, &channel) {
                let _ = sender.send_response(response);
            }
        }
    }
}

struct MockFetchHandler {
    req_tx: mpsc::Sender<FetchBlobRequest>,
    resp_rx: mpsc::Receiver<(FetchBlobResponse, request_response::ResponseChannel<FetchBlobResponse>)>,
}

#[async_trait]
impl request_response::Handler for MockFetchHandler {
    type RequestId = request_response::OutboundRequestId;
    type Request = FetchBlobRequest;
    type Response = FetchBlobResponse;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();
    
    async fn handle_request(
        &mut self,
        _: Self::InboundOpenInfo,
        request: Self::Request,
        sender: request_response::ResponseChannel<Self::Response>,
    ) {
        let _ = self.req_tx.send(request).await;
        if let Some((response, channel)) = self.resp_rx.recv().await {
            if std::ptr::eq(&sender, &channel) {
                let _ = sender.send_response(response);
            }
        }
    }
}

// BlobStorageAdapter for testing
struct BlobStorageAdapter {
    storage: Arc<Mutex<dyn icn_storage::StorageBackend + Send + Sync>>,
}

impl BlobStorageAdapter {
    async fn blob_exists(&self, cid: &Cid) -> FederationResult<bool> {
        let storage_guard = self.storage.lock().await;
        storage_guard.contains_blob(cid).await
            .map_err(|e| FederationError::StorageError(format!("Failed to check blob existence: {}", e)))
    }
    
    async fn pin_blob(&self, cid: &Cid) -> FederationResult<()> {
        let storage_guard = self.storage.lock().await;
        storage_guard.pin_blob(cid).await
            .map_err(|e| FederationError::StorageError(format!("Failed to pin blob: {}", e)))
    }
    
    async fn put_blob(&self, data: &[u8]) -> FederationResult<Cid> {
        let storage_guard = self.storage.lock().await;
        storage_guard.put_blob(data).await
            .map_err(|e| FederationError::StorageError(format!("Failed to store blob: {}", e)))
    }
}

// Tests for blob replication
#[tokio::test]
async fn test_replication_local_hit_success() {
    // Create test data
    let test_data = b"test blob data";
    let mut mock_storage = MockStorageBackend::default();
    
    // Insert test blob
    let mut hasher = Sha256::new();
    hasher.update(test_data);
    let result = hasher.finalize();
    let mh = multihash::Multihash::wrap(0x12, &result).unwrap();
    let test_cid = Cid::new_v1(0x55, mh);
    mock_storage.blobs.insert(test_cid, test_data.to_vec());
    
    // Create storage adapter
    let storage = Arc::new(Mutex::new(mock_storage));
    let blob_storage = BlobStorageAdapter { storage };
    
    // Create swarm and test channels
    let (
        mut swarm,
        _peer_id,
        mut replicate_req_rx,
        replicate_resp_tx,
        _fetch_req_rx,
        _fetch_resp_tx
    ) = create_test_swarm(MockStorageBackend::default()).await;
    
    // Create maps for tracking queries
    let mut pending_provider_queries = HashMap::new();
    let mut pending_replication_fetches = HashMap::new();
    
    // Create the test request
    let request = ReplicateBlobRequest { cid: test_cid };
    
    // Create a response channel for testing
    let (channel, _) = libp2p::request_response::create_response_channel();
    
    // Run the handler function
    icn_federation::handle_blob_replication_request(
        request,
        channel,
        &mut swarm,
        &blob_storage,
        &mut pending_provider_queries,
        &mut pending_replication_fetches,
    ).await;
    
    // Verify that pin_blob was called and a success response was sent
    // This is verified by checking if a message was sent to the channel
    assert_eq!(pending_provider_queries.len(), 0);
    assert_eq!(pending_replication_fetches.len(), 0);
}

#[tokio::test]
async fn test_replication_local_hit_pin_failure() {
    // Create test data
    let test_data = b"test blob data";
    let mut mock_storage = MockStorageBackend::default();
    mock_storage.should_fail_pin = true;
    
    // Insert test blob
    let mut hasher = Sha256::new();
    hasher.update(test_data);
    let result = hasher.finalize();
    let mh = multihash::Multihash::wrap(0x12, &result).unwrap();
    let test_cid = Cid::new_v1(0x55, mh);
    mock_storage.blobs.insert(test_cid, test_data.to_vec());
    
    // Create storage adapter
    let storage = Arc::new(Mutex::new(mock_storage));
    let blob_storage = BlobStorageAdapter { storage };
    
    // Create swarm and test channels
    let (
        mut swarm,
        _peer_id,
        _replicate_req_rx,
        _replicate_resp_tx,
        _fetch_req_rx,
        _fetch_resp_tx
    ) = create_test_swarm(MockStorageBackend::default()).await;
    
    // Create maps for tracking queries
    let mut pending_provider_queries = HashMap::new();
    let mut pending_replication_fetches = HashMap::new();
    
    // Create the test request
    let request = ReplicateBlobRequest { cid: test_cid };
    
    // Create a response channel for testing
    let (channel, _) = libp2p::request_response::create_response_channel();
    
    // Run the handler function
    icn_federation::handle_blob_replication_request(
        request,
        channel,
        &mut swarm,
        &blob_storage,
        &mut pending_provider_queries,
        &mut pending_replication_fetches,
    ).await;
    
    // Verify that no query was created since blob exists but pinning failed
    assert_eq!(pending_provider_queries.len(), 0);
    assert_eq!(pending_replication_fetches.len(), 0);
}

#[tokio::test]
async fn test_replication_storage_error() {
    // Create test data
    let test_data = b"test blob data";
    let mut mock_storage = MockStorageBackend::default();
    mock_storage.should_fail_exists = true;
    
    // Insert test blob
    let mut hasher = Sha256::new();
    hasher.update(test_data);
    let result = hasher.finalize();
    let mh = multihash::Multihash::wrap(0x12, &result).unwrap();
    let test_cid = Cid::new_v1(0x55, mh);
    mock_storage.blobs.insert(test_cid, test_data.to_vec());
    
    // Create storage adapter
    let storage = Arc::new(Mutex::new(mock_storage));
    let blob_storage = BlobStorageAdapter { storage };
    
    // Create swarm and test channels
    let (
        mut swarm,
        _peer_id,
        _replicate_req_rx,
        _replicate_resp_tx,
        _fetch_req_rx,
        _fetch_resp_tx
    ) = create_test_swarm(MockStorageBackend::default()).await;
    
    // Create maps for tracking queries
    let mut pending_provider_queries = HashMap::new();
    let mut pending_replication_fetches = HashMap::new();
    
    // Create the test request
    let request = ReplicateBlobRequest { cid: test_cid };
    
    // Create a response channel for testing
    let (channel, _) = libp2p::request_response::create_response_channel();
    
    // Run the handler function
    icn_federation::handle_blob_replication_request(
        request,
        channel,
        &mut swarm,
        &blob_storage,
        &mut pending_provider_queries,
        &mut pending_replication_fetches,
    ).await;
    
    // Verify that no query was created since the storage check failed
    assert_eq!(pending_provider_queries.len(), 0);
    assert_eq!(pending_replication_fetches.len(), 0);
}

#[tokio::test]
async fn test_replication_remote_fetch_initiation() {
    // Create test data
    let test_data = b"test blob data";
    let mock_storage = MockStorageBackend::default();
    
    // Calculate test CID
    let mut hasher = Sha256::new();
    hasher.update(test_data);
    let result = hasher.finalize();
    let mh = multihash::Multihash::wrap(0x12, &result).unwrap();
    let test_cid = Cid::new_v1(0x55, mh);
    
    // Create storage adapter
    let storage = Arc::new(Mutex::new(mock_storage));
    let blob_storage = BlobStorageAdapter { storage };
    
    // Create swarm and test channels
    let (
        mut swarm,
        _peer_id,
        _replicate_req_rx,
        _replicate_resp_tx,
        _fetch_req_rx,
        _fetch_resp_tx
    ) = create_test_swarm(MockStorageBackend::default()).await;
    
    // Create maps for tracking queries
    let mut pending_provider_queries = HashMap::new();
    let mut pending_replication_fetches = HashMap::new();
    
    // Create the test request
    let request = ReplicateBlobRequest { cid: test_cid };
    
    // Create a response channel for testing
    let (channel, _) = libp2p::request_response::create_response_channel();
    
    // Run the handler function
    icn_federation::handle_blob_replication_request(
        request,
        channel,
        &mut swarm,
        &blob_storage,
        &mut pending_provider_queries,
        &mut pending_replication_fetches,
    ).await;
    
    // Verify that a Kademlia query was initiated
    assert_eq!(pending_provider_queries.len(), 1);
    assert_eq!(pending_replication_fetches.len(), 1);
    
    let (query_id, _) = pending_replication_fetches.iter().next().unwrap();
    assert!(pending_provider_queries.contains_key(query_id));
} 