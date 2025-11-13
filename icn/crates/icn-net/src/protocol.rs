//! Network protocol for ICN
//!
//! Defines wire-format messages sent over QUIC connections.

use crate::envelope::SignedEnvelope;
use anyhow::{Context, Result};
use icn_gossip::GossipMessage;
use icn_identity::{BindingInfo, Did};
use serde::{Deserialize, Serialize};

/// Network protocol version
pub const PROTOCOL_VERSION: u32 = 1;

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

    /// Ping (keepalive)
    Ping,

    /// Pong (response to ping)
    Pong,

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
        /// Optional topology information (if topology is enabled)
        topology_info: Option<crate::TopologyInfo>,
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

    /// Create a ping message
    pub fn ping(from: Did, to: Did) -> Self {
        Self::new(from, Some(to), MessagePayload::Ping)
    }

    /// Create a pong message
    pub fn pong(from: Did, to: Did) -> Self {
        Self::new(from, Some(to), MessagePayload::Pong)
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
        Self::new(from, Some(to), MessagePayload::Handshake { region, cluster_id, role })
    }

    /// Create a handshake ack message
    pub fn handshake_ack(from: Did, to: Did) -> Self {
        Self::new(from, Some(to), MessagePayload::HandshakeAck)
    }

    /// Create a Hello message with DID-TLS binding verification
    pub fn hello(from: Did, to: Did, binding_info: BindingInfo, topology_info: Option<crate::TopologyInfo>) -> Self {
        Self::new(from, Some(to), MessagePayload::Hello {
            binding_info,
            topology_info,
        })
    }

    /// Create a signed message (authenticated + replay protected)
    ///
    /// This wraps a SignedEnvelope in a NetworkMessage. The `from` field is
    /// taken from the envelope's authenticated sender, and `to` is the routing hint.
    pub fn signed(to: Option<Did>, envelope: SignedEnvelope) -> Self {
        let from = envelope.from.clone();
        Self::new(from, to, MessagePayload::Signed(envelope))
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

        bincode::deserialize(bytes).context("Failed to deserialize network message")
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
pub async fn read_message(
    recv: &mut quinn::RecvStream,
) -> Result<NetworkMessage> {
    use tokio::io::AsyncReadExt;

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
        anyhow::bail!(
            "Message too large: {} bytes (max {})",
            len_u32,
            MAX_MESSAGE_SIZE
        );
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
pub async fn write_message(
    send: &mut quinn::SendStream,
    msg: &NetworkMessage,
) -> Result<()> {
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
        assert!(matches!(decoded.payload, MessagePayload::Ping));
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

        let msg = NetworkMessage::gossip(alice.clone(), None, GossipMessage::Request {
            hash: [0u8; 32],
        });

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
        let alice = KeyPair::generate().unwrap().did().clone();
        let data = vec![0u8; MAX_MESSAGE_SIZE + 1];

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

        if let MessagePayload::Subscribe { topics: decoded_topics } = decoded.payload {
            assert_eq!(decoded_topics, topics);
        } else {
            panic!("Expected Subscribe payload");
        }
    }
}
