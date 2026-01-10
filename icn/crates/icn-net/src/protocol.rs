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

/// Compression threshold in bytes (1KB) - messages below this are not compressed
pub const COMPRESSION_THRESHOLD: usize = 1024;

/// Compression format marker bytes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionFormat {
    /// No compression - raw bincode follows
    None = 0,
    /// Zstd compression - compressed bincode follows
    Zstd = 1,
}

impl TryFrom<u8> for CompressionFormat {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(CompressionFormat::None),
            1 => Ok(CompressionFormat::Zstd),
            _ => anyhow::bail!("Unknown compression format: {value}"),
        }
    }
}

/// Wire format encoding type
///
/// Used in combination with compression to indicate how messages are serialized.
/// The wire format byte combines encoding and compression:
/// - Low nibble (bits 0-3): compression format
/// - High nibble (bits 4-7): encoding format
///
/// This allows backward compatibility: old nodes only check the low nibble,
/// while new nodes can distinguish between bincode and postcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EncodingFormat {
    /// Bincode encoding (legacy, compatible with all nodes)
    Bincode = 0,
    /// Postcard encoding (more compact, requires POSTCARD_ENCODING capability)
    Postcard = 1,
}

impl EncodingFormat {
    /// Create wire format byte from encoding and compression
    pub fn to_wire_byte(self, compression: CompressionFormat) -> u8 {
        ((self as u8) << 4) | (compression as u8)
    }

    /// Parse encoding format from wire format byte
    pub fn from_wire_byte(byte: u8) -> Result<(Self, CompressionFormat)> {
        let encoding = match byte >> 4 {
            0 => EncodingFormat::Bincode,
            1 => EncodingFormat::Postcard,
            n => anyhow::bail!("Unknown encoding format: {n}"),
        };
        let compression = CompressionFormat::try_from(byte & 0x0F)?;
        Ok((encoding, compression))
    }
}

impl TryFrom<u8> for EncodingFormat {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(EncodingFormat::Bincode),
            1 => Ok(EncodingFormat::Postcard),
            _ => anyhow::bail!("Unknown encoding format: {value}"),
        }
    }
}

/// Re-export TraceContext from icn-obs for distributed tracing propagation
pub use icn_obs::TraceContext;

/// Post-quantum key binding proof
///
/// Cryptographic proof that the sender controls both the Ed25519 key (DID)
/// and the ML-DSA key. This prevents an attacker from claiming someone else's
/// PQ public key in a Hello message.
///
/// The proof is an ML-DSA signature over the binding message:
/// `"DID-PQ-BINDING-V1:<did>:<timestamp_millis>"`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PqBindingProof {
    /// Unix timestamp in milliseconds when the proof was created
    pub timestamp_millis: u64,
    /// ML-DSA signature over the binding message
    pub signature: Vec<u8>,
}

impl PqBindingProof {
    /// Binding message version prefix
    pub const VERSION_PREFIX: &'static str = "DID-PQ-BINDING-V1";

    /// Maximum age of a binding proof (5 minutes)
    /// This prevents replay attacks while allowing for clock skew
    pub const MAX_AGE_MILLIS: u64 = 5 * 60 * 1000;

    /// Future tolerance for clock skew (1 minute)
    /// Proofs with timestamps up to this far in the future are accepted
    pub const FUTURE_TOLERANCE_MILLIS: u64 = 60_000;

    /// Get current timestamp in milliseconds with safe u128->u64 conversion
    #[allow(dead_code)] // Used by post-quantum feature
    fn current_timestamp_millis() -> Result<u64, anyhow::Error> {
        use anyhow::anyhow;
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| anyhow!("System time error: {e}"))
            .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
    }

    /// Create the canonical binding message to sign
    pub fn binding_message(did: &Did, timestamp_millis: u64) -> Vec<u8> {
        format!("{}:{}:{}", Self::VERSION_PREFIX, did, timestamp_millis).into_bytes()
    }

    /// Create a new binding proof
    #[cfg(feature = "post-quantum")]
    pub fn create(did: &Did, keypair: &icn_identity::KeyPair) -> Option<Self> {
        let pq_keypair = keypair.pq_keypair()?;
        let timestamp_millis = Self::current_timestamp_millis().ok()?;

        let message = Self::binding_message(did, timestamp_millis);
        let signature = pq_keypair.sign(&message).ok()?.as_bytes().to_vec();

        Some(Self {
            timestamp_millis,
            signature,
        })
    }

    /// Verify a binding proof against a DID and ML-DSA public key
    #[cfg(feature = "post-quantum")]
    pub fn verify(&self, did: &Did, ml_dsa_public: &[u8]) -> Result<(), anyhow::Error> {
        use anyhow::anyhow;

        let now_millis = Self::current_timestamp_millis()?;

        // Check timestamp is not in the future (beyond clock skew tolerance)
        if self.timestamp_millis > now_millis + Self::FUTURE_TOLERANCE_MILLIS {
            return Err(anyhow!("Binding proof timestamp is in the future"));
        }

        // Check proof is not too old
        if now_millis.saturating_sub(self.timestamp_millis) > Self::MAX_AGE_MILLIS {
            return Err(anyhow!(
                "Binding proof expired (older than {} seconds)",
                Self::MAX_AGE_MILLIS / 1000
            ));
        }

        // Parse the ML-DSA public key
        let pq_pubkey = icn_crypto_pq::MlDsaPublicKey::from_bytes(ml_dsa_public)
            .map_err(|e| anyhow!("Invalid ML-DSA public key: {e}"))?;

        // Reconstruct the binding message
        let message = Self::binding_message(did, self.timestamp_millis);

        // Parse the signature
        let signature = icn_crypto_pq::MlDsaSignature::from_bytes(&self.signature)
            .map_err(|e| anyhow!("Invalid ML-DSA signature format: {e}"))?;

        // Verify the signature using static method
        if !icn_crypto_pq::MlDsaKeypair::verify(&pq_pubkey, &message, &signature) {
            return Err(anyhow!("Binding proof signature verification failed"));
        }
        Ok(())
    }

    /// Verify without post-quantum feature (always fails)
    #[cfg(not(feature = "post-quantum"))]
    pub fn verify(&self, _did: &Did, _ml_dsa_public: &[u8]) -> Result<(), anyhow::Error> {
        Err(anyhow::anyhow!(
            "Post-quantum feature not enabled, cannot verify binding proof"
        ))
    }
}

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

    /// Trace context for distributed tracing (optional for backward compatibility).
    ///
    /// `#[serde(default)]` ensures that **new** code can deserialize **old** messages
    /// that don't have this field (it will simply be `None`). We deliberately do
    /// **not** use `skip_serializing_if` here because bincode uses positional
    /// encoding: if **new** code were to skip serializing this field, **old**
    /// deserializers expecting a value in this position would fail to decode.
    /// Always serializing the field (even when `None`) preserves compatibility.
    #[serde(default)]
    pub trace_context: Option<TraceContext>,
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
        /// ML-DSA public key for hybrid signatures (post-quantum)
        /// Sent when HYBRID_SIGNATURES capability is advertised
        #[serde(default)]
        ml_dsa_public: Option<Vec<u8>>,
        /// ML-KEM public key for hybrid encryption (post-quantum)
        /// Sent when HYBRID_KEM capability is advertised
        #[serde(default)]
        ml_kem_public: Option<Vec<u8>>,
        /// DID-PQ key binding proof (post-quantum)
        /// ML-DSA signature over "DID-PQ-BINDING-V1:<did>:<timestamp>" proving
        /// the sender controls both Ed25519 (DID) and ML-DSA keys
        #[serde(default)]
        pq_binding_proof: Option<PqBindingProof>,
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

    /// Onion-routed message for privacy-preserving communication
    /// Multi-hop routing hides sender/receiver relationships
    Onion(icn_privacy::OnionMessage),
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
            MessagePayload::Onion(_) => "Onion",
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
            trace_context: None,
        }
    }

    /// Create a new network message with trace context
    pub fn new_with_trace(
        from: Did,
        to: Option<Did>,
        payload: MessagePayload,
        trace_context: Option<TraceContext>,
    ) -> Self {
        NetworkMessage {
            version: PROTOCOL_VERSION,
            from,
            to,
            payload,
            trace_context,
        }
    }

    /// Attach trace context from the current span
    pub fn with_current_trace_context(mut self) -> Self {
        self.trace_context = Some(TraceContext::from_current());
        self
    }

    /// Create a gossip message
    pub fn gossip(from: Did, to: Option<Did>, gossip_msg: GossipMessage) -> Self {
        Self::new(from, to, MessagePayload::Gossip(gossip_msg))
    }

    /// Create a ping message with current timestamp
    pub fn ping(from: Did, to: Did) -> Self {
        let sent_at = icn_time::current_timestamp_millis();
        Self::new(from, Some(to), MessagePayload::Ping { sent_at })
    }

    /// Create a pong message echoing ping timestamp
    pub fn pong(from: Did, to: Did, ping_sent_at: u64) -> Self {
        let pong_sent_at = icn_time::current_timestamp_millis();
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
    ///
    /// # Arguments
    /// * `from` - Sender DID
    /// * `to` - Recipient DID
    /// * `binding_info` - DID-TLS binding proof
    /// * `version_info` - Protocol version and capabilities
    /// * `topology_info` - Optional topology information
    /// * `x25519_public` - X25519 public key for classical encryption
    /// * `ml_dsa_public` - Optional ML-DSA public key for hybrid signatures
    /// * `ml_kem_public` - Optional ML-KEM public key for hybrid encryption
    pub fn hello(
        from: Did,
        to: Did,
        binding_info: BindingInfo,
        version_info: VersionInfo,
        topology_info: Option<crate::TopologyInfo>,
        x25519_public: [u8; 32],
        ml_dsa_public: Option<Vec<u8>>,
        ml_kem_public: Option<Vec<u8>>,
    ) -> Self {
        Self::new(
            from,
            Some(to),
            MessagePayload::Hello {
                binding_info,
                version_info: Some(version_info),
                topology_info,
                x25519_public,
                ml_dsa_public,
                ml_kem_public,
                pq_binding_proof: None,
            },
        )
    }

    /// Create a Hello message with post-quantum key binding proof
    ///
    /// This is the preferred method for nodes with hybrid capabilities.
    /// The binding proof cryptographically proves the sender controls both
    /// the Ed25519 key (DID) and the ML-DSA key.
    #[cfg(feature = "post-quantum")]
    pub fn hello_with_binding(
        from: Did,
        to: Did,
        binding_info: BindingInfo,
        version_info: VersionInfo,
        topology_info: Option<crate::TopologyInfo>,
        x25519_public: [u8; 32],
        ml_dsa_public: Option<Vec<u8>>,
        ml_kem_public: Option<Vec<u8>>,
        keypair: &icn_identity::KeyPair,
    ) -> Self {
        // Generate binding proof if we have PQ keys
        let pq_binding_proof = if ml_dsa_public.is_some() {
            PqBindingProof::create(&from, keypair)
        } else {
            None
        };

        Self::new(
            from,
            Some(to),
            MessagePayload::Hello {
                binding_info,
                version_info: Some(version_info),
                topology_info,
                x25519_public,
                ml_dsa_public,
                ml_kem_public,
                pq_binding_proof,
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

    /// Create an onion-routed message
    ///
    /// The `from` field is set to a placeholder DID since the sender is anonymous.
    /// The actual sender identity is hidden within the onion layers.
    pub fn onion(first_hop: Did, onion_msg: icn_privacy::OnionMessage) -> Self {
        // Use the first hop as the routing target
        // The 'from' field uses the first_hop as a placeholder since
        // onion routing hides the true sender
        Self::new(
            first_hop.clone(), // Placeholder - true sender is hidden
            Some(first_hop),   // Route to first hop
            MessagePayload::Onion(onion_msg),
        )
    }

    /// Serialize to bytes using bincode (legacy format, no compression header)
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let bytes = bincode::serde::encode_to_vec(self, bincode::config::legacy())
            .context("Failed to serialize network message")?;

        if bytes.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                bytes.len(),
                MAX_MESSAGE_SIZE
            );
        }

        Ok(bytes)
    }

    /// Serialize to bytes with compression header
    ///
    /// Wire format: [compression_format: 1 byte][payload: variable]
    /// - If compression enabled and message >= threshold: compresses with zstd
    /// - Otherwise: includes uncompressed payload with None marker
    ///
    /// Use this for peers that support MESSAGE_COMPRESSION capability.
    pub fn to_bytes_compressed(&self, enable_compression: bool) -> Result<Vec<u8>> {
        let raw_bytes = bincode::serde::encode_to_vec(self, bincode::config::legacy())
            .context("Failed to serialize network message")?;

        // Track bytes before compression for metrics
        let bytes_before = raw_bytes.len();

        // Only compress if enabled, above threshold, and would reduce size
        if enable_compression && raw_bytes.len() >= COMPRESSION_THRESHOLD {
            let compressed =
                zstd::encode_all(raw_bytes.as_slice(), 3).context("Failed to compress message")?;

            // Only use compression if it actually reduces size
            if compressed.len() < raw_bytes.len() {
                let mut result = Vec::with_capacity(1 + compressed.len());
                result.push(CompressionFormat::Zstd as u8);
                result.extend(compressed);

                if result.len() > MAX_MESSAGE_SIZE {
                    anyhow::bail!(
                        "Compressed message too large: {} bytes (max {})",
                        result.len(),
                        MAX_MESSAGE_SIZE
                    );
                }

                // Record compression metrics
                icn_obs::metrics::gossip::bytes_before_compression_add(bytes_before as u64);
                icn_obs::metrics::gossip::bytes_after_compression_add(result.len() as u64);
                icn_obs::metrics::gossip::compression_ratio_record(
                    bytes_before as f64 / result.len() as f64,
                );

                return Ok(result);
            }
        }

        // No compression - use None marker
        let mut result = Vec::with_capacity(1 + raw_bytes.len());
        result.push(CompressionFormat::None as u8);
        result.extend(raw_bytes);

        if result.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                result.len(),
                MAX_MESSAGE_SIZE
            );
        }

        // Track uncompressed messages too
        icn_obs::metrics::gossip::bytes_before_compression_add(bytes_before as u64);
        icn_obs::metrics::gossip::bytes_after_compression_add(result.len() as u64);

        Ok(result)
    }

    /// Deserialize from bytes using bincode (legacy format, no compression header)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                bytes.len(),
                MAX_MESSAGE_SIZE
            );
        }

        let msg: NetworkMessage =
            bincode::serde::decode_from_slice(bytes, bincode::config::legacy())
                .map(|(v, _)| v)
                .context("Failed to deserialize network message")?;

        // Validate protocol version
        Self::validate_version(msg.version)?;

        Ok(msg)
    }

    /// Deserialize from bytes with compression header
    ///
    /// Expects wire format: [compression_format: 1 byte][payload: variable]
    /// Automatically decompresses if zstd format is indicated.
    ///
    /// Use this for peers that support MESSAGE_COMPRESSION capability.
    pub fn from_bytes_compressed(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            anyhow::bail!("Empty message");
        }

        if bytes.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                bytes.len(),
                MAX_MESSAGE_SIZE
            );
        }

        let compression_format = CompressionFormat::try_from(bytes[0])?;
        let payload = &bytes[1..];

        let decompressed = match compression_format {
            CompressionFormat::None => payload.to_vec(),
            CompressionFormat::Zstd => {
                zstd::decode_all(payload).context("Failed to decompress message")?
            }
        };

        let msg: NetworkMessage =
            bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy())
                .map(|(v, _)| v)
                .context("Failed to deserialize network message")?;

        Self::validate_version(msg.version)?;
        Ok(msg)
    }

    /// Serialize to bytes with encoding and compression negotiation
    ///
    /// Wire format: [format_byte: 1 byte][payload: variable]
    /// - format_byte high nibble (bits 4-7): encoding format (0=bincode, 1=postcard)
    /// - format_byte low nibble (bits 0-3): compression format (0=none, 1=zstd)
    ///
    /// Use this for peers that support POSTCARD_ENCODING capability.
    /// Falls back to bincode for peers without this capability.
    pub fn to_bytes_negotiated(
        &self,
        use_postcard: bool,
        enable_compression: bool,
    ) -> Result<Vec<u8>> {
        let encoding = if use_postcard {
            EncodingFormat::Postcard
        } else {
            EncodingFormat::Bincode
        };

        let raw_bytes = if use_postcard {
            postcard::to_allocvec(self).context("Failed to serialize with postcard")?
        } else {
            bincode::serde::encode_to_vec(self, bincode::config::legacy())
                .context("Failed to serialize with bincode")?
        };

        let bytes_before = raw_bytes.len();

        // Only compress if enabled, above threshold, and would reduce size
        if enable_compression && raw_bytes.len() >= COMPRESSION_THRESHOLD {
            let compressed =
                zstd::encode_all(raw_bytes.as_slice(), 3).context("Failed to compress message")?;

            if compressed.len() < raw_bytes.len() {
                let format_byte = encoding.to_wire_byte(CompressionFormat::Zstd);
                let mut result = Vec::with_capacity(1 + compressed.len());
                result.push(format_byte);
                result.extend(compressed);

                if result.len() > MAX_MESSAGE_SIZE {
                    anyhow::bail!(
                        "Compressed message too large: {} bytes (max {})",
                        result.len(),
                        MAX_MESSAGE_SIZE
                    );
                }

                icn_obs::metrics::gossip::bytes_before_compression_add(bytes_before as u64);
                icn_obs::metrics::gossip::bytes_after_compression_add(result.len() as u64);
                icn_obs::metrics::gossip::compression_ratio_record(
                    bytes_before as f64 / result.len() as f64,
                );

                return Ok(result);
            }
        }

        // No compression
        let format_byte = encoding.to_wire_byte(CompressionFormat::None);
        let mut result = Vec::with_capacity(1 + raw_bytes.len());
        result.push(format_byte);
        result.extend(raw_bytes);

        if result.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                result.len(),
                MAX_MESSAGE_SIZE
            );
        }

        icn_obs::metrics::gossip::bytes_before_compression_add(bytes_before as u64);
        icn_obs::metrics::gossip::bytes_after_compression_add(result.len() as u64);

        Ok(result)
    }

    /// Deserialize from bytes with encoding and compression negotiation
    ///
    /// Auto-detects encoding format from wire format byte and handles both
    /// bincode and postcard encoded messages.
    ///
    /// This method is backward-compatible: it can decode messages from both
    /// old nodes (bincode-only) and new nodes (postcard-capable).
    pub fn from_bytes_negotiated(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            anyhow::bail!("Empty message");
        }

        if bytes.len() > MAX_MESSAGE_SIZE {
            anyhow::bail!(
                "Message too large: {} bytes (max {})",
                bytes.len(),
                MAX_MESSAGE_SIZE
            );
        }

        let (encoding, compression) = EncodingFormat::from_wire_byte(bytes[0])?;
        let payload = &bytes[1..];

        let decompressed = match compression {
            CompressionFormat::None => payload.to_vec(),
            CompressionFormat::Zstd => {
                zstd::decode_all(payload).context("Failed to decompress message")?
            }
        };

        let msg: NetworkMessage = match encoding {
            EncodingFormat::Bincode => {
                bincode::serde::decode_from_slice(&decompressed, bincode::config::legacy())
                    .map(|(v, _)| v)
                    .context("Failed to deserialize with bincode")?
            }
            EncodingFormat::Postcard => postcard::from_bytes(&decompressed)
                .context("Failed to deserialize with postcard")?,
        };

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

/// Helper for reading length-prefixed messages from QUIC streams (legacy format)
/// Read a length-prefixed message from a QUIC stream
/// Returns the message and the number of bytes read (including 4-byte length prefix)
pub async fn read_message(recv: &mut quinn::RecvStream) -> Result<(NetworkMessage, usize)> {
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

    let msg = NetworkMessage::from_bytes(&buf)?;
    // Return total bytes: 4 (length prefix) + message body
    Ok((msg, 4 + len))
}

/// Helper for reading length-prefixed messages with compression support
/// Use this for peers that support MESSAGE_COMPRESSION capability.
/// Returns the message and the number of bytes read (including 4-byte length prefix)
pub async fn read_message_compressed(
    recv: &mut quinn::RecvStream,
) -> Result<(NetworkMessage, usize)> {
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

    let msg = NetworkMessage::from_bytes_compressed(&buf)?;
    // Return total bytes: 4 (length prefix) + message body
    Ok((msg, 4 + len))
}

/// Helper for writing length-prefixed messages to QUIC streams (legacy format)
/// Returns the number of bytes written (including 4-byte length prefix)
pub async fn write_message(send: &mut quinn::SendStream, msg: &NetworkMessage) -> Result<usize> {
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

    // Return total bytes: 4 (length prefix) + message body
    Ok(4 + bytes.len())
}

/// Helper for writing length-prefixed messages with compression support
/// Use this for peers that support MESSAGE_COMPRESSION capability.
/// Returns the number of bytes written (including 4-byte length prefix)
pub async fn write_message_compressed(
    send: &mut quinn::SendStream,
    msg: &NetworkMessage,
    enable_compression: bool,
) -> Result<usize> {
    use tokio::io::AsyncWriteExt;

    let bytes = msg.to_bytes_compressed(enable_compression)?;
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

    // Return total bytes: 4 (length prefix) + message body
    Ok(4 + bytes.len())
}

/// Helper for reading length-prefixed messages with negotiated encoding
///
/// Use this for peers that support both POSTCARD_ENCODING and MESSAGE_COMPRESSION capabilities.
/// Auto-detects encoding format from wire format byte.
/// Returns the message and the number of bytes read (including 4-byte length prefix)
pub async fn read_message_negotiated(
    recv: &mut quinn::RecvStream,
) -> Result<(NetworkMessage, usize)> {
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

    let msg = NetworkMessage::from_bytes_negotiated(&buf)?;
    // Return total bytes: 4 (length prefix) + message body
    Ok((msg, 4 + len))
}

/// Helper for writing length-prefixed messages with negotiated encoding
///
/// Use this for peers that support both POSTCARD_ENCODING and MESSAGE_COMPRESSION capabilities.
/// Returns the number of bytes written (including 4-byte length prefix)
///
/// # Arguments
/// * `send` - QUIC send stream
/// * `msg` - Message to send
/// * `use_postcard` - Use postcard encoding (true if peer supports POSTCARD_ENCODING)
/// * `enable_compression` - Enable zstd compression (true if peer supports MESSAGE_COMPRESSION)
pub async fn write_message_negotiated(
    send: &mut quinn::SendStream,
    msg: &NetworkMessage,
    use_postcard: bool,
    enable_compression: bool,
) -> Result<usize> {
    use tokio::io::AsyncWriteExt;

    let bytes = msg.to_bytes_negotiated(use_postcard, enable_compression)?;
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

    // Return total bytes: 4 (length prefix) + message body
    Ok(4 + bytes.len())
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
        let bytes = bincode::serde::encode_to_vec(&msg, bincode::config::legacy()).unwrap();

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

    // Issue #123: Compression tests

    #[test]
    fn test_compression_format_conversion() {
        assert_eq!(
            CompressionFormat::try_from(0).unwrap(),
            CompressionFormat::None
        );
        assert_eq!(
            CompressionFormat::try_from(1).unwrap(),
            CompressionFormat::Zstd
        );
        assert!(CompressionFormat::try_from(2).is_err());
        assert!(CompressionFormat::try_from(255).is_err());
    }

    #[test]
    fn test_compressed_roundtrip_small_message() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Small message - should NOT be compressed
        let msg = NetworkMessage::ping(alice, bob);

        let bytes = msg.to_bytes_compressed(true).unwrap();

        // First byte should be 0 (no compression) for small messages
        assert_eq!(bytes[0], 0);

        let decoded = NetworkMessage::from_bytes_compressed(&bytes).unwrap();
        assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));
    }

    #[test]
    fn test_compressed_roundtrip_large_message() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create a large, compressible gossip message
        let mut clock = VectorClock::new();
        clock.increment(&alice);

        // Create a large entry with repetitive data (compresses well)
        let large_data = vec![42u8; 4096]; // 4KB of same byte
        let gossip_msg = GossipMessage::Response {
            entry: icn_gossip::types::GossipEntry {
                hash: [1u8; 32],
                author: alice.clone(),
                clock,
                topic: "test".to_string(),
                data: large_data,
                compressed: false,
                timestamp: 0,
                replica_offered: None,
            },
        };

        let msg = NetworkMessage::gossip(alice, Some(bob), gossip_msg);

        // With compression enabled, should compress
        let bytes = msg.to_bytes_compressed(true).unwrap();

        // First byte should be 1 (zstd) for large compressible messages
        assert_eq!(bytes[0], 1);

        // Compressed should be smaller than uncompressed
        let uncompressed = msg.to_bytes_compressed(false).unwrap();
        assert!(bytes.len() < uncompressed.len());

        let decoded = NetworkMessage::from_bytes_compressed(&bytes).unwrap();
        assert!(matches!(decoded.payload, MessagePayload::Gossip(_)));
    }

    #[test]
    fn test_compressed_with_compression_disabled() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice, bob);

        // Even with compression disabled, should use new format with header
        let bytes = msg.to_bytes_compressed(false).unwrap();

        // First byte should be 0 (no compression)
        assert_eq!(bytes[0], 0);

        let decoded = NetworkMessage::from_bytes_compressed(&bytes).unwrap();
        assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));
    }

    #[test]
    fn test_legacy_format_still_works() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice.clone(), bob.clone());

        // Legacy format (no compression header)
        let bytes = msg.to_bytes().unwrap();
        let decoded = NetworkMessage::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.from, alice);
        assert_eq!(decoded.to, Some(bob));
        assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));
    }

    #[test]
    fn test_compression_threshold() {
        // Verify the threshold constant
        assert_eq!(COMPRESSION_THRESHOLD, 1024);
    }

    // Issue #528: DID-PQ key binding verification tests

    #[test]
    fn test_pq_binding_message_format() {
        let alice = KeyPair::generate().unwrap();
        let timestamp = 1704067200000u64; // 2024-01-01 00:00:00 UTC

        let message = PqBindingProof::binding_message(alice.did(), timestamp);
        let expected = format!(
            "{}:{}:{}",
            PqBindingProof::VERSION_PREFIX,
            alice.did(),
            timestamp
        );

        assert_eq!(message, expected.as_bytes());
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_create_and_verify() {
        // KeyPair::generate() creates hybrid keypairs when post-quantum feature is enabled
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create binding proof
        let proof = PqBindingProof::create(&did, &keypair);
        assert!(
            proof.is_some(),
            "Should create binding proof for hybrid keypair"
        );

        let proof = proof.unwrap();
        assert!(!proof.signature.is_empty(), "Signature should not be empty");

        // Verify the binding proof
        let ml_dsa_public = keypair.pq_public_key().unwrap().as_bytes().to_vec();
        let result = proof.verify(&did, &ml_dsa_public);
        assert!(
            result.is_ok(),
            "Valid binding proof should verify: {:?}",
            result
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_fails_for_wrong_did() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create binding proof
        let proof = PqBindingProof::create(&did, &keypair).unwrap();

        // Try to verify with a different DID
        let other_keypair = KeyPair::generate().unwrap();
        let ml_dsa_public = keypair.pq_public_key().unwrap().as_bytes().to_vec();

        let result = proof.verify(other_keypair.did(), &ml_dsa_public);
        assert!(result.is_err(), "Should fail for wrong DID");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("verification failed"),
            "Error should mention verification failure"
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_fails_for_wrong_key() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create binding proof
        let proof = PqBindingProof::create(&did, &keypair).unwrap();

        // Try to verify with a different ML-DSA public key
        let other_keypair = KeyPair::generate().unwrap();
        let wrong_ml_dsa_public = other_keypair.pq_public_key().unwrap().as_bytes().to_vec();

        let result = proof.verify(&did, &wrong_ml_dsa_public);
        assert!(result.is_err(), "Should fail for wrong public key");
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_fails_for_invalid_key_bytes() {
        // Test with invalid ML-DSA key bytes
        let keypair = KeyPair::generate().unwrap();
        let proof = PqBindingProof {
            timestamp_millis: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            signature: vec![0u8; 100], // Garbage signature
        };

        // Invalid key bytes should fail
        let result = proof.verify(keypair.did(), &[0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid ML-DSA public key"),
            "Error should mention invalid key: {}",
            err
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_non_hybrid_keypair_returns_none() {
        // Create a legacy (classical-only) keypair using from_bytes
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let secret_bytes: [u8; 32] = signing_key.to_bytes();
        let public_bytes: [u8; 32] = signing_key.verifying_key().to_bytes();

        // from_bytes creates a keypair WITHOUT PQ component
        let keypair = KeyPair::from_bytes(&secret_bytes, &public_bytes).unwrap();

        // Classical-only keypair should return None
        let proof = PqBindingProof::create(keypair.did(), &keypair);
        assert!(
            proof.is_none(),
            "Non-hybrid keypair should not create binding proof"
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_rejects_future_timestamp() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create a proof with timestamp far in the future (beyond 1 minute tolerance)
        let future_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + PqBindingProof::FUTURE_TOLERANCE_MILLIS
            + 60_000; // 2 minutes in future

        let message = PqBindingProof::binding_message(&did, future_timestamp);
        let signature = keypair
            .pq_keypair()
            .unwrap()
            .sign(&message)
            .unwrap()
            .as_bytes()
            .to_vec();

        let proof = PqBindingProof {
            timestamp_millis: future_timestamp,
            signature,
        };

        let ml_dsa_public = keypair.pq_public_key().unwrap().as_bytes().to_vec();
        let result = proof.verify(&did, &ml_dsa_public);
        assert!(result.is_err(), "Should reject future timestamp");
        assert!(
            result.unwrap_err().to_string().contains("in the future"),
            "Error should mention future timestamp"
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_rejects_expired_timestamp() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create a proof with expired timestamp (6 minutes ago, beyond 5 minute max age)
        let expired_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - PqBindingProof::MAX_AGE_MILLIS
            - 60_000; // 6 minutes ago

        let message = PqBindingProof::binding_message(&did, expired_timestamp);
        let signature = keypair
            .pq_keypair()
            .unwrap()
            .sign(&message)
            .unwrap()
            .as_bytes()
            .to_vec();

        let proof = PqBindingProof {
            timestamp_millis: expired_timestamp,
            signature,
        };

        let ml_dsa_public = keypair.pq_public_key().unwrap().as_bytes().to_vec();
        let result = proof.verify(&did, &ml_dsa_public);
        assert!(result.is_err(), "Should reject expired timestamp");
        assert!(
            result.unwrap_err().to_string().contains("expired"),
            "Error should mention expiration"
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_accepts_edge_of_tolerance() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create a proof at the edge of future tolerance (should be accepted)
        let edge_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + PqBindingProof::FUTURE_TOLERANCE_MILLIS
            - 1000; // Just under 1 minute in future

        let message = PqBindingProof::binding_message(&did, edge_timestamp);
        let signature = keypair
            .pq_keypair()
            .unwrap()
            .sign(&message)
            .unwrap()
            .as_bytes()
            .to_vec();

        let proof = PqBindingProof {
            timestamp_millis: edge_timestamp,
            signature,
        };

        let ml_dsa_public = keypair.pq_public_key().unwrap().as_bytes().to_vec();
        let result = proof.verify(&did, &ml_dsa_public);
        assert!(
            result.is_ok(),
            "Should accept timestamp at edge of tolerance: {:?}",
            result
        );
    }

    #[test]
    #[cfg(feature = "post-quantum")]
    fn test_pq_binding_proof_accepts_edge_of_max_age() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        // Create a proof at the edge of max age (should be accepted)
        let edge_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - PqBindingProof::MAX_AGE_MILLIS
            + 1000; // Just under 5 minutes ago

        let message = PqBindingProof::binding_message(&did, edge_timestamp);
        let signature = keypair
            .pq_keypair()
            .unwrap()
            .sign(&message)
            .unwrap()
            .as_bytes()
            .to_vec();

        let proof = PqBindingProof {
            timestamp_millis: edge_timestamp,
            signature,
        };

        let ml_dsa_public = keypair.pq_public_key().unwrap().as_bytes().to_vec();
        let result = proof.verify(&did, &ml_dsa_public);
        assert!(
            result.is_ok(),
            "Should accept timestamp at edge of max age: {:?}",
            result
        );
    }

    // Issue #539: Wire protocol negotiated encoding tests

    #[test]
    fn test_encoding_format_conversion() {
        assert_eq!(
            EncodingFormat::try_from(0).unwrap(),
            EncodingFormat::Bincode
        );
        assert_eq!(
            EncodingFormat::try_from(1).unwrap(),
            EncodingFormat::Postcard
        );
        assert!(EncodingFormat::try_from(2).is_err());
    }

    #[test]
    fn test_wire_byte_roundtrip() {
        // Test all combinations of encoding and compression
        let combinations = [
            (EncodingFormat::Bincode, CompressionFormat::None, 0x00),
            (EncodingFormat::Bincode, CompressionFormat::Zstd, 0x01),
            (EncodingFormat::Postcard, CompressionFormat::None, 0x10),
            (EncodingFormat::Postcard, CompressionFormat::Zstd, 0x11),
        ];

        for (encoding, compression, expected_byte) in combinations {
            let wire_byte = encoding.to_wire_byte(compression);
            assert_eq!(
                wire_byte, expected_byte,
                "Wire byte mismatch for {encoding:?}/{compression:?}",
            );

            let (parsed_encoding, parsed_compression) =
                EncodingFormat::from_wire_byte(wire_byte).unwrap();
            assert_eq!(
                parsed_encoding, encoding,
                "Encoding mismatch for byte {wire_byte:#x}",
            );
            assert_eq!(
                parsed_compression, compression,
                "Compression mismatch for byte {wire_byte:#x}",
            );
        }
    }

    #[test]
    fn test_negotiated_bincode_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice.clone(), bob.clone());

        // Bincode without compression
        let bytes = msg.to_bytes_negotiated(false, false).unwrap();
        assert_eq!(bytes[0], 0x00, "Should be bincode without compression");

        let decoded = NetworkMessage::from_bytes_negotiated(&bytes).unwrap();
        assert_eq!(decoded.from, alice);
        assert_eq!(decoded.to, Some(bob));
        assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));
    }

    #[test]
    fn test_negotiated_postcard_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice.clone(), bob.clone());

        // Postcard without compression
        let bytes = msg.to_bytes_negotiated(true, false).unwrap();
        assert_eq!(bytes[0], 0x10, "Should be postcard without compression");

        let decoded = NetworkMessage::from_bytes_negotiated(&bytes).unwrap();
        assert_eq!(decoded.from, alice);
        assert_eq!(decoded.to, Some(bob));
        assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));
    }

    #[test]
    fn test_postcard_is_more_compact() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let msg = NetworkMessage::ping(alice, bob);

        let bincode_bytes = msg.to_bytes_negotiated(false, false).unwrap();
        let postcard_bytes = msg.to_bytes_negotiated(true, false).unwrap();

        // Postcard is typically more compact
        println!(
            "Bincode: {} bytes, Postcard: {} bytes",
            bincode_bytes.len(),
            postcard_bytes.len()
        );

        // Both should decode correctly
        let decoded_bincode = NetworkMessage::from_bytes_negotiated(&bincode_bytes).unwrap();
        let decoded_postcard = NetworkMessage::from_bytes_negotiated(&postcard_bytes).unwrap();

        assert!(matches!(
            decoded_bincode.payload,
            MessagePayload::Ping { .. }
        ));
        assert!(matches!(
            decoded_postcard.payload,
            MessagePayload::Ping { .. }
        ));
    }

    #[test]
    fn test_negotiated_postcard_with_compression() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create a large, compressible message
        let mut clock = VectorClock::new();
        clock.increment(&alice);

        let large_data = vec![42u8; 4096]; // 4KB of same byte
        let gossip_msg = GossipMessage::Response {
            entry: icn_gossip::types::GossipEntry {
                hash: [1u8; 32],
                author: alice.clone(),
                clock,
                topic: "test".to_string(),
                data: large_data,
                compressed: false,
                timestamp: 0,
                replica_offered: None,
            },
        };

        let msg = NetworkMessage::gossip(alice.clone(), Some(bob.clone()), gossip_msg);

        // Postcard with compression
        let bytes = msg.to_bytes_negotiated(true, true).unwrap();
        assert_eq!(bytes[0], 0x11, "Should be postcard with zstd compression");

        // Should be smaller than without compression
        let uncompressed = msg.to_bytes_negotiated(true, false).unwrap();
        assert!(
            bytes.len() < uncompressed.len(),
            "Compressed ({}) should be smaller than uncompressed ({})",
            bytes.len(),
            uncompressed.len()
        );

        let decoded = NetworkMessage::from_bytes_negotiated(&bytes).unwrap();
        assert_eq!(decoded.from, alice);
        assert!(matches!(decoded.payload, MessagePayload::Gossip(_)));
    }

    #[test]
    fn test_negotiated_bincode_with_compression() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Create a large, compressible message
        let mut clock = VectorClock::new();
        clock.increment(&alice);

        let large_data = vec![42u8; 4096];
        let gossip_msg = GossipMessage::Response {
            entry: icn_gossip::types::GossipEntry {
                hash: [1u8; 32],
                author: alice.clone(),
                clock,
                topic: "test".to_string(),
                data: large_data,
                compressed: false,
                timestamp: 0,
                replica_offered: None,
            },
        };

        let msg = NetworkMessage::gossip(alice.clone(), Some(bob.clone()), gossip_msg);

        // Bincode with compression
        let bytes = msg.to_bytes_negotiated(false, true).unwrap();
        assert_eq!(bytes[0], 0x01, "Should be bincode with zstd compression");

        let decoded = NetworkMessage::from_bytes_negotiated(&bytes).unwrap();
        assert_eq!(decoded.from, alice);
        assert!(matches!(decoded.payload, MessagePayload::Gossip(_)));
    }

    #[test]
    fn test_negotiated_unknown_encoding_rejected() {
        // Create a message with invalid encoding format (0x20)
        let wire_byte = 0x20; // encoding = 2 (invalid)
        let result = EncodingFormat::from_wire_byte(wire_byte);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown encoding"));
    }

    #[test]
    fn test_negotiated_unknown_compression_rejected() {
        // Create a message with invalid compression format (0x02)
        let wire_byte = 0x02; // encoding = 0, compression = 2 (invalid)
        let result = EncodingFormat::from_wire_byte(wire_byte);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown compression"));
    }

    #[test]
    fn test_negotiated_empty_message_rejected() {
        let result = NetworkMessage::from_bytes_negotiated(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty message"));
    }

    #[test]
    #[allow(clippy::erasing_op)] // Intentional: documenting that old nodes see compression = 0
    fn test_backward_compatible_wire_byte() {
        // Verify that old nodes reading new wire format bytes correctly parse
        // the compression format (low nibble) even if they ignore the encoding (high nibble)

        // 0x00 = bincode + none -> old nodes see compression = 0 (none)
        assert_eq!(0x00 & 0x0F, 0);

        // 0x01 = bincode + zstd -> old nodes see compression = 1 (zstd)
        assert_eq!(0x01 & 0x0F, 1);

        // 0x10 = postcard + none -> old nodes see compression = 0 (none)
        // They'll fail to decode postcard, but at least won't crash on decompression
        assert_eq!(0x10 & 0x0F, 0);

        // 0x11 = postcard + zstd -> old nodes see compression = 1 (zstd)
        // They'll decompress successfully, then fail to decode postcard
        assert_eq!(0x11 & 0x0F, 1);
    }
}
