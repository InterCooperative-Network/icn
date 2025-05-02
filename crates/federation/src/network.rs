use libp2p::{
    gossipsub, identify, kad, mdns, request_response, PeerId,
    swarm::NetworkBehaviour,
    StreamProtocol,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use icn_identity::TrustBundle;
use cid::Cid;

/// Request for a TrustBundle by epoch
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustBundleRequest {
    /// The epoch to retrieve the TrustBundle for
    pub epoch: u64,
}

/// Response containing a TrustBundle (or None if not found)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundleResponse {
    /// The requested TrustBundle, if found
    pub bundle: Option<TrustBundle>,
}

/// Protocol name for TrustBundle sync
pub const TRUST_BUNDLE_PROTOCOL_ID: StreamProtocol = StreamProtocol::new("/icn/trustbundle/1.0.0");

/// Protocol name for Blob Replication
pub const BLOB_REPLICATION_PROTOCOL_ID: StreamProtocol = StreamProtocol::new("/icn/blob-replicate/1.0.0");

/// Protocol name for Blob Fetch
pub const BLOB_FETCH_PROTOCOL_ID: StreamProtocol = StreamProtocol::new("/icn/blob-fetch/1.0.0");

/// Timeout for TrustBundle request/response
pub const TRUST_BUNDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for Blob Replication request/response
pub const BLOB_REPLICATION_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for Blob Fetch request/response
pub const BLOB_FETCH_TIMEOUT: Duration = Duration::from_secs(180);

/// Request to replicate a blob
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicateBlobRequest {
    /// The CID of the blob to replicate
    pub cid: Cid,
}

/// Response to a blob replication request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicateBlobResponse {
    /// Whether the replication was successful
    pub success: bool,
    /// Error message if the replication failed
    pub error_msg: Option<String>,
}

/// Request to fetch a blob by CID
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchBlobRequest {
    /// The CID of the blob to fetch
    pub cid: Cid,
}

/// Response containing the requested blob data
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FetchBlobResponse {
    /// The blob data, if found
    pub data: Option<Vec<u8>>,
    /// Error message if the fetch failed
    pub error_msg: Option<String>,
}

/// Represents all networking behaviors for the ICN federation
#[derive(NetworkBehaviour)]
pub struct IcnFederationBehaviour {
    /// Gossipsub for publishing/subscribing to topics
    pub gossipsub: gossipsub::Behaviour,
    
    /// Kademlia for DHT operations
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    
    /// mDNS for local peer discovery
    pub mdns: mdns::tokio::Behaviour,
    
    /// Identify protocol for peer info exchange
    pub identify: identify::Behaviour,
    
    /// TrustBundle sync request/response protocol
    pub trust_bundle_sync: request_response::json::Behaviour<TrustBundleRequest, TrustBundleResponse>,
    
    /// Blob replication request/response protocol
    #[behaviour(event_process = false)]
    pub blob_replication: request_response::cbor::Behaviour<ReplicateBlobRequest, ReplicateBlobResponse>,
    
    /// Blob fetch request/response protocol
    #[behaviour(event_process = false)]
    pub blob_fetch_protocol: request_response::cbor::Behaviour<FetchBlobRequest, FetchBlobResponse>,
}

/// Creates a new instance of IcnFederationBehaviour
pub fn create_behaviour(
    local_peer_id: PeerId,
    keypair: libp2p::identity::Keypair,
) -> Result<IcnFederationBehaviour, Box<dyn std::error::Error>> {
    // Create Kademlia DHT behavior
    let kademlia_store = kad::store::MemoryStore::new(local_peer_id);
    let mut kademlia_config = kad::Config::default();
    kademlia_config.set_query_timeout(Duration::from_secs(300));
    let kademlia = kad::Behaviour::with_config(local_peer_id, kademlia_store, kademlia_config);
    
    // Create mDNS behavior for local peer discovery
    let mdns = mdns::tokio::Behaviour::new(
        mdns::Config::default(),
        local_peer_id,
    )?;
    
    // Create identify behavior for peer information exchange
    let identify = identify::Behaviour::new(identify::Config::new(
        "icn-federation/1.0.0".to_string(),
        keypair.public(),
    ));
    
    // Create gossipsub behavior
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .build()?;
    
    let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(keypair),
        gossipsub_config,
    )?;
    
    // Create TrustBundle request/response behavior
    let trust_bundle_sync = request_response::json::Behaviour::<TrustBundleRequest, TrustBundleResponse>::new(
        [(TRUST_BUNDLE_PROTOCOL_ID, request_response::ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(TRUST_BUNDLE_TIMEOUT),
    );
    
    // Create Blob Replication request/response behavior
    let blob_replication = request_response::cbor::Behaviour::<ReplicateBlobRequest, ReplicateBlobResponse>::new(
        [(BLOB_REPLICATION_PROTOCOL_ID, request_response::ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(BLOB_REPLICATION_TIMEOUT),
    );
    
    // Create Blob Fetch request/response behavior
    let blob_fetch_protocol = request_response::cbor::Behaviour::<FetchBlobRequest, FetchBlobResponse>::new(
        [(BLOB_FETCH_PROTOCOL_ID, request_response::ProtocolSupport::Full)],
        request_response::Config::default().with_request_timeout(BLOB_FETCH_TIMEOUT),
    );
    
    Ok(IcnFederationBehaviour {
        gossipsub,
        kademlia,
        mdns,
        identify,
        trust_bundle_sync,
        blob_replication,
        blob_fetch_protocol,
    })
} 