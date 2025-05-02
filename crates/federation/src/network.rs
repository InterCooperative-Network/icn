use libp2p::{
    gossipsub, identify, kad, mdns, request_response, PeerId,
    swarm::NetworkBehaviour,
    StreamProtocol,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use icn_identity::TrustBundle;

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

/// Timeout for TrustBundle request/response
pub const TRUST_BUNDLE_TIMEOUT: Duration = Duration::from_secs(60);

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
    
    Ok(IcnFederationBehaviour {
        gossipsub,
        kademlia,
        mdns,
        identify,
        trust_bundle_sync,
    })
} 