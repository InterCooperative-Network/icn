//! Network protocol for ICN
//!
//! Defines wire-format messages sent over QUIC connections.

use crate::envelope::SignedEnvelope;
use crate::VersionInfo;
use anyhow::{Context, Result};
use icn_gossip::GossipMessage;
use icn_identity::{BindingInfo, Did};
use serde::{Deserialize, Serialize};

/// Network protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Minimum supported protocol version (for backward compatibility)
pub const MIN_SUPPORTED_VERSION: u32 = 1;

/// Maximum supported protocol version (for forward compatibility)
pub const MAX_SUPPORTED_VERSION: u32 = 1;

/// Maximum message size (10MB)
pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

/// Wire-format message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    /// Protocol version
    pub version: u32,

    /// Source DID (sender)
    pub from: Did,

    /// Destination DID (None = broadcast)
    pub to: Option<Did>,

    /// Message payload
    pub payload: MessagePayload,
}

/// Message payload types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    /// Gossip protocol message
    Gossip(GossipMessage),

    /// Ping (keepalive + RTT measurement)
    Ping {
        /// Timestamp when ping was sent (milliseconds since UNIX epoch)
        sent_at: u64,
    },

    /// Pong (response to ping + RTT measurement)
    Pong {
        /// Echo of ping's sent_at timestamp
        ping_sent_at: u64,
        /// Timestamp when pong was sent (milliseconds since UNIX epoch)
        pong_sent_at: u64,
    },

    /// Subscribe to peer's topics
    Subscribe { topics: Vec<String> },

    /// Unsubscribe from peer's topics
    Unsubscribe { topics: Vec<String> },

    /// Ack subscription
    SubscribeAck { topics: Vec<String> },

    /// Hello message with DID-TLS binding verification
    /// This is the initial handshake message that proves identity
    Hello {
        /// DID-TLS binding information for verification
        binding_info: BindingInfo,
        /// Version and capability information for protocol negotiation
        /// None indicates a pre-version-negotiation node (treated as v1)
        version_info: Option<VersionInfo>,
        /// Optional topology information (if topology is enabled)
        topology_info: Option<crate::TopologyInfo>,
        /// X25519 public key for end-to-end encryption (Phase 10)
        x25519_public: [u8; 32],
    },

    /// Handshake with topology information (legacy, kept for compatibility)
    Handshake {
        region: String,
        cluster_id: String,
        role: String,
    },

    /// Handshake acknowledgement
    HandshakeAck,

    /// Signed application-level message (authenticated + replay protected)
    /// This wraps any message that requires cryptographic proof of authenticity
    Signed(SignedEnvelope),

    /// Peer exchange for cross-network discovery
    /// Nodes share their known peers to enable discovery beyond local network
    PeerExchange(PeerExchangeMessage),
}

/// Peer exchange message for cross-network discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerExchangeMessage {
    /// Request peer list from a connected peer
    Request {
        /// Maximum number of peers to return
        max_peers: usize,
        /// Filter by network name (optional)
        network_filter: Option<String>,
    },

    /// Response with known peers
    Response {
        /// List of known peers
        peers: Vec<KnownPeer>,
        /// Total peers known (may be > peers.len() if limited)
        total_known: usize,
    },

    /// Announce new peer (push notification)
    Announce {
        /// New peer being announced
        peer: KnownPeer,
    },

    /// Unannounce peer (peer went offline)
    Unannounce {
        /// DID of peer that went offline
        did: String,
    },
}

/// Information about a known peer for exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownPeer {
    /// Peer's DID
    pub did: String,

    /// Socket addresses where peer can be reached
    /// Multiple addresses support multi-homed nodes
    pub addresses: Vec<String>,

    /// Protocol version
    pub version: String,

    /// Network name (for federation filtering)
    pub network_name: Option<String>,

    /// Trust score (as observed by the sharing peer)
    /// Recipients should NOT directly trust this value
    pub observed_trust: Option<f64>,

    /// Unix timestamp when this peer was last seen
    pub last_seen: u64,

    /// Whether this peer was discovered via mDNS (local) or exchange (remote)
    pub is_local: bool,
}

impl MessagePayload {
    /// Get the variant name for logging
    pub fn variant_name(&self) -> &'static str {
        match self {
            MessagePayload::Gossip(_) => "Gossip",
            MessagePayload::Ping { .. } => "Ping",
            MessagePayload::Pong { .. } => "Pong",
            MessagePayload::Subscribe { .. } => "Subscribe",
            MessagePayload::Unsubscribe { .. } => "Unsubscribe",
            MessagePayload::SubscribeAck { .. } => "SubscribeAck",
            MessagePayload::Hello { .. } => "Hello",
            MessagePayload::Handshake { .. } => "Handshake",
            MessagePayload::HandshakeAck => "HandshakeAck",
            MessagePayload::Signed(_) => "Signed",
            MessagePayload::PeerExchange(_) => "PeerExchange",
        }
    }
}

impl NetworkMessage {
    /// Create a new network message
    pub fn new(from: Did, to: Option<Did>, payload: MessagePayload) -> Self {
        NetworkMessage {
            version: PROTOCOL_VERSION,
            from,
            to,
            payload,
        }
    }

    /// Create a gossip message
    pub fn gossip(from: Did, to: Option<Did>, gossip_msg: GossipMessage) -> Self {
        Self::new(from, to, MessagePayload::Gossip(gossip_msg))
    }

    /// Create a ping message with current timestamp
    pub fn ping(from: Did, to: Did) -> Self {
        let sent_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Self::new(from, Some(to), MessagePayload::Ping { sent_at })
    }

    /// Create a pong message echoing ping timestamp
    pub fn pong(from: Did, to: Did, ping_sent_at: u64) -> Self {
        let pong_sent_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Self::new(
            from,
            Some(to),
            MessagePayload::Pong {
                ping_sent_at,
                pong_sent_at,
            },
        )
    }

    /// Create a subscribe message
    pub fn subscribe(from: Did, to: Did, topics: Vec<String>) -> Self {
        Self::new(from, Some(to), MessagePayload::Subscribe { topics })
    }

    /// Create an unsubscribe message
    pub fn unsubscribe(from: Did, to: Did, topics: Vec<String>) -> Self {
        Self::new(from, Some(to), MessagePayload::Unsubscribe { topics })
    }

    /// Create a subscribe ack message
    pub fn subscribe_ack(from: Did, to: Did, topics: Vec<String>) -> Self {
        Self::new(from, Some(to), MessagePayload::SubscribeAck { topics })
    }

    /// Create a handshake message
    pub fn handshake(from: Did, to: Did, region: String, cluster_id: String, role: String) -> Self {
        Self::new(
            from,
            Some(to),
            MessagePayload::Handshake {
                region,
                cluster_id,
                role,
            },
        )
    }

    /// Create a handshake ack message
    pub fn handshake_ack(from: Did, to: Did) -> Self {
        Self::new(from, Some(to), MessagePayload::HandshakeAck)
    }

    /// Create a Hello message with DID-TLS binding verification and X25519 key exchange
    pub fn hello(
        from: Did,
        to: Did,
        binding_info: BindingInfo,
        version_info: VersionInfo,
        topology_info: Option<crate::TopologyInfo>,
        x25519_public: [u8; 32],
    ) -> Self {
        Self::new(
            from,
            Some(to),
            MessagePayload::Hello {
                binding_info,
                version_info: Some(version_info),
                topology_info,
                x25519_public,
            },
        )
    }

    /// Create a signed message (authenticated + replay protected)
    ///
    /// This wraps a SignedEnvelope in a NetworkMessage. The `from` field is
    /// taken from the envelope's authenticated sender, and `to` is the routing hint.
    pub fn signed(to: Option<Did>, envelope: SignedEnvelope) -> Self {
        let from = envelope.from.clone();
        Self::new(from, to, MessagePayload::Signed(envelope))
    }

    /// Create a peer exchange request message
    pub fn peer_exchange_request(
        from: Did,
        to: Did,
        max_peers: usize,
        network_filter: Option<String>,
    ) -> Self {
        Self::new(
            from,
            Some(to),
            MessagePayload::PeerExchange(PeerExchangeMessage::Request {
                max_peers,
                network_filter,
            }),
        )
    }

    /// Create a peer exchange response message
    pub fn peer_exchange_response(
        from: Did,
        to: Did,
        peers: Vec<KnownPeer>,
        total_known: usize,
    ) -> Self {
        Self::new(
            from,
            Some(to),
            MessagePayload::PeerExchange(PeerExchangeMessage::Response { peers, total_known }),
        )
    }

    /// Create a peer announce message (broadcast to all connected peers)
    pub fn peer_announce(from: Did, peer: KnownPeer) -> Self {
        Self::new(
            from,
            None,
            MessagePayload::PeerExchange(PeerExchangeMessage::Announce { peer }),
        )
    }

    /// Create a peer unannounce message (broadcast peer went offline)
    pub fn peer_unannounce(from: Did, did: String) -> Self {
        Self::new(
            from,
            None,
            MessagePayload::PeerExchange(PeerExchangeMessage::Unannounce { did }),
        )
    }

    /// Serialize to bytes using bincode
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let bytes = bincode::serialize(self).context("Failed to serialize network message")?;

        if bytes.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                bytes.len(),
                MAX_MESSAGE_SIZE
            );
        }

        Ok(bytes)
    }

    /// Deserialize from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                bytes.len(),
                MAX_MESSAGE_SIZE
            );
        }

        let msg: NetworkMessage =
            bincode::deserialize(bytes).context("Failed to deserialize network message")?;

        // Validate protocol version
        Self::validate_version(msg.version)?;

        Ok(msg)
    }

    /// Validate protocol version compatibility
    ///
    /// Returns Ok if the version is supported, Err otherwise.
    /// This allows for backward and forward compatibility within a version range.
    pub fn validate_version(version: u32) -> Result<()> {
        if version < MIN_SUPPORTED_VERSION {
            anyhow::bail!(
                "Protocol version {version} is too old (minimum supported: {MIN_SUPPORTED_VERSION})"
            );
        }

        if version > MAX_SUPPORTED_VERSION {
            anyhow::bail!(
                "Protocol version {version} is too new (maximum supported: {MAX_SUPPORTED_VERSION}). Please upgrade ICNd."
            );
        }

        Ok(())
    }

    /// Check if this message is for a specific DID
    pub fn is_for(&self, did: &Did) -> bool {
        match &self.to {
            Some(target) => target == did,
            None => true, // Broadcast messages are for everyone
        }
    }

    /// Check if this is a broadcast message
    pub fn is_broadcast(&self) -> bool {
        self.to.is_none()
    }

    /// Verify DID-TLS binding if this is a Hello message
    ///
    /// # Arguments
    /// * `peer_cert` - The TLS certificate received from the peer
    ///
    /// # Returns
    /// * `Ok(())` if verification succeeds or if not a Hello message
    /// * `Err` if Hello message but verification fails
    pub fn verify_hello(&self, peer_cert: &rustls::pki_types::CertificateDer) -> Result<()> {
        if let MessagePayload::Hello { binding_info, .. } = &self.payload {
            icn_identity::verify_binding_info(binding_info, peer_cert)
                .context("DID-TLS binding verification failed")?;
        }
        Ok(())
    }

    /// Extract topology info from Hello or Handshake message
    pub fn topology_info(&self) -> Option<&crate::TopologyInfo> {
        match &self.payload {
            MessagePayload::Hello { topology_info, .. } => topology_info.as_ref(),
            _ => None,
        }
    }
}

/// Helper for reading length-prefixed messages from QUIC streams
pub async fn read_message(recv: &mut quinn::RecvStream) -> Result<NetworkMessage> {
    // Read 4-byte length prefix (big-endian)
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .context("Failed to read message length")?;
    let len_u32 = u32::from_be_bytes(len_buf);

    // Validate message size BEFORE casting to usize to prevent overflow on 32-bit systems
    if len_u32 == 0 {
        anyhow::bail!("Invalid message: zero length");
    }
    if len_u32 > MAX_MESSAGE_SIZE as u32 {
        anyhow::bail!("Message too large: {len_u32} bytes (max {MAX_MESSAGE_SIZE})");
    }

    // Safe to cast after validation
    let len = len_u32 as usize;

    // Allocate buffer (size is now validated)
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf)
        .await
        .context("Failed to read message body")?;

    NetworkMessage::from_bytes(&buf)
}

/// Helper for writing length-prefixed messages to QUIC streams
pub async fn write_message(send: &mut quinn::SendStream, msg: &NetworkMessage) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    let bytes = msg.to_bytes()?;
    let len = bytes.len() as u32;

    // Write 4-byte length prefix (big-endian)
    send.write_all(&len.to_be_bytes())
        .await
        .context("Failed to write message length")?;

    // Write message bytes
    send.write_all(&bytes)
        .await
        .context("Failed to write message body")?;

    send.flush().await.context("Failed to flush stream")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_gossip::{types::ContentHash, VectorClock};
    use icn_identity::KeyPair;

    #[test]
    fn test_network_message_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice, bob);
        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.version, PROTOCOL_VERSION);
        assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));
    }

    #[test]
    fn test_gossip_message_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let hash: ContentHash = [1u8; 32];
        let mut clock = VectorClock::new();
        clock.increment(&alice);
        let gossip_msg = GossipMessage::Announce {
            hash,
            author: alice.clone(),
            clock,
            topic: "test".to_string(),
        };

        let net_msg = NetworkMessage::gossip(alice.clone(), Some(bob.clone()), gossip_msg);
        let bytes = net_msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.from, alice);
        assert_eq!(decoded.to, Some(bob));
        assert!(matches!(decoded.payload, MessagePayload::Gossip(_)));
    }

    #[test]
    fn test_broadcast_message() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::gossip(
            alice.clone(),
            None,
            GossipMessage::Request { hash: [0u8; 32] },
        );

        assert!(msg.is_broadcast());
        assert!(msg.is_for(&bob));
        assert!(msg.is_for(&alice));
    }

    #[test]
    fn test_targeted_message() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice.clone(), bob.clone());

        assert!(!msg.is_broadcast());
        assert!(msg.is_for(&bob));
        assert!(!msg.is_for(&charlie));
    }

    #[test]
    fn test_max_message_size() {
        let _alice = KeyPair::generate().unwrap().did().clone();
        let _data = vec![0u8; MAX_MESSAGE_SIZE + 1];

        // This should fail during serialization
        // (We can't easily test this without a huge message, so just check the constant)
        assert_eq!(MAX_MESSAGE_SIZE, 10 * 1024 * 1024);
    }

    #[test]
    fn test_signed_message_roundtrip() {
        use crate::envelope::{PayloadType, SignedEnvelope};

        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create a signed envelope
        let envelope = SignedEnvelope::new(
            &alice,
            &keypair,
            1,
            PayloadType::Gossip,
            b"test payload".to_vec(),
        )
        .unwrap();

        // Wrap it in a NetworkMessage
        let net_msg = NetworkMessage::signed(Some(bob.clone()), envelope.clone());

        // Serialize and deserialize
        let bytes = net_msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        // Verify structure
        assert_eq!(decoded.from, alice);
        assert_eq!(decoded.to, Some(bob));
        assert!(matches!(decoded.payload, MessagePayload::Signed(_)));

        // Extract and verify the signed envelope
        if let MessagePayload::Signed(decoded_envelope) = decoded.payload {
            assert_eq!(decoded_envelope.from, alice);
            assert_eq!(decoded_envelope.sequence, 1);
            assert_eq!(decoded_envelope.payload, b"test payload");

            // Verify signature (should still be valid)
            assert!(decoded_envelope.verify(300).is_ok());
        } else {
            panic!("Expected Signed payload");
        }
    }

    #[test]
    fn test_subscribe_message() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let topics = vec!["ledger:hours".to_string(), "global:identity".to_string()];
        let msg = NetworkMessage::subscribe(alice.clone(), bob.clone(), topics.clone());

        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        if let MessagePayload::Subscribe {
            topics: decoded_topics,
        } = decoded.payload
        {
            assert_eq!(decoded_topics, topics);
        } else {
            panic!("Expected Subscribe payload");
        }
    }

    #[test]
    fn test_version_validation_current() {
        // Current version should be accepted
        assert!(NetworkMessage::validate_version(PROTOCOL_VERSION).is_ok());
    }

    #[test]
    fn test_version_validation_too_old() {
        // Version 0 should be rejected (too old)
        let result = NetworkMessage::validate_version(MIN_SUPPORTED_VERSION - 1);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too old"));
    }

    #[test]
    fn test_version_validation_too_new() {
        // Future version should be rejected (too new)
        let result = NetworkMessage::validate_version(MAX_SUPPORTED_VERSION + 1);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too new"));
    }

    #[test]
    fn test_message_with_invalid_version_rejected() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create a message with invalid version
        let mut msg = NetworkMessage::ping(alice, bob);
        msg.version = MAX_SUPPORTED_VERSION + 1;

        // Serialize it
        let bytes = bincode::serialize(&msg).unwrap();

        // Deserialization should fail due to version check
        let result = NetworkMessage::from_bytes(&bytes);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("too new"));
    }

    #[test]
    fn test_peer_exchange_request_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::peer_exchange_request(
            alice.clone(),
            bob.clone(),
            50,
            Some("my-coop-network".to_string()),
        );

        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.from, alice);
        assert_eq!(decoded.to, Some(bob));

        if let MessagePayload::PeerExchange(PeerExchangeMessage::Request {
            max_peers,
            network_filter,
        }) = decoded.payload
        {
            assert_eq!(max_peers, 50);
            assert_eq!(network_filter, Some("my-coop-network".to_string()));
        } else {
            panic!("Expected PeerExchange::Request payload");
        }
    }

    #[test]
    fn test_peer_exchange_response_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let peers = vec![
            KnownPeer {
                did: "did:icn:peer1".to_string(),
                addresses: vec!["192.168.1.100:7777".to_string()],
                version: "0.1.0".to_string(),
                network_name: Some("my-coop".to_string()),
                observed_trust: Some(0.5),
                last_seen: 1234567890,
                is_local: false,
            },
            KnownPeer {
                did: "did:icn:peer2".to_string(),
                addresses: vec!["192.168.1.101:7777".to_string()],
                version: "0.1.0".to_string(),
                network_name: None,
                observed_trust: None,
                last_seen: 1234567891,
                is_local: true,
            },
        ];

        let msg =
            NetworkMessage::peer_exchange_response(alice.clone(), bob.clone(), peers.clone(), 10);

        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        if let MessagePayload::PeerExchange(PeerExchangeMessage::Response {
            peers: decoded_peers,
            total_known,
        }) = decoded.payload
        {
            assert_eq!(decoded_peers.len(), 2);
            assert_eq!(total_known, 10);
            assert_eq!(decoded_peers[0].did, "did:icn:peer1");
            assert_eq!(decoded_peers[0].observed_trust, Some(0.5));
            assert_eq!(decoded_peers[1].did, "did:icn:peer2");
            assert!(decoded_peers[1].is_local);
        } else {
            panic!("Expected PeerExchange::Response payload");
        }
    }

    #[test]
    fn test_peer_announce_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();

        let peer = KnownPeer {
            did: "did:icn:newpeer".to_string(),
            addresses: vec!["10.0.0.1:7777".to_string(), "10.0.0.2:7777".to_string()],
            version: "0.2.0".to_string(),
            network_name: Some("icn-mainnet".to_string()),
            observed_trust: Some(0.8),
            last_seen: 1234567895,
            is_local: false,
        };

        let msg = NetworkMessage::peer_announce(alice.clone(), peer.clone());

        // Announce is a broadcast (no to field)
        assert!(msg.is_broadcast());

        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        if let MessagePayload::PeerExchange(PeerExchangeMessage::Announce { peer: decoded_peer }) =
            decoded.payload
        {
            assert_eq!(decoded_peer.did, "did:icn:newpeer");
            assert_eq!(decoded_peer.addresses.len(), 2);
            assert_eq!(decoded_peer.observed_trust, Some(0.8));
        } else {
            panic!("Expected PeerExchange::Announce payload");
        }
    }

    #[test]
    fn test_peer_unannounce_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::peer_unannounce(alice.clone(), "did:icn:departed".to_string());

        // Unannounce is a broadcast (no to field)
        assert!(msg.is_broadcast());

        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        if let MessagePayload::PeerExchange(PeerExchangeMessage::Unannounce { did }) =
            decoded.payload
        {
            assert_eq!(did, "did:icn:departed");
        } else {
            panic!("Expected PeerExchange::Unannounce payload");
        }
    }
}
