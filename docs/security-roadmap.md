# ICN Security Roadmap

## Overview

This document outlines the security architecture and implementation plan for ICN. It addresses critical vulnerabilities in the current implementation and provides a phased approach to building a secure, production-ready cooperative network.

**Status:** Planning phase - Not yet implemented

**Target:** Phase 8 - Security & Production Hardening

## Sequencing Strategy

Security work follows a layered approach, building from network foundations to application-level integrity:

### 1. Network Hardening
- Connection limits + backpressure
- Basic peer admission/rate limiting
- Bind TLS to DIDs

### 2. Message & Identity Integrity
- Signed envelopes at the protocol layer
- Basic replay protection window
- Clear story for internal vs external message trust

### 3. Trust System Hardening
- Cap/shape transitive trust
- Add decay
- Introduce basic evidence hooks for later "participation-based trust"

### 4. Key Lifecycle
- Rotation events + DID transitions
- Prepare for multi-device identities (even if not fully wired yet)

### 5. Ledger & Causal Consistency
- Minimal ledger sync over gossip
- Causal handling policy (even if it's "last writer wins + metrics" at first)

## 1. Connection Flooding / DoS Prevention

**Priority:** HIGH
**Effort:** LOW
**Risk:** Connection exhaustion, memory exhaustion, CPU exhaustion

### Current State

- `handle_incoming` in `icn-net` accepts all connections
- No maximum peer limit
- No eviction policy
- No per-peer rate limiting
- Memory usage grows unbounded with peer count

### Design Goal

Prevent a single malicious or malfunctioning node from:
- Exhausting file descriptors/sockets
- Consuming excessive CPU with handshakes
- Filling memory with connection state

Maintain bounded resources:
- Bounded number of active peers
- Bounded number of pending handshakes
- Bounded memory per peer

### Implementation Plan

#### 1.1 PeerRegistry + Limits

Create `icn-net/src/peer_registry.rs`:

```rust
pub struct PeerRegistry {
    /// Active peer connections
    active: HashMap<Did, PeerInfo>,

    /// Maximum number of active peers
    max_peers: usize,

    /// Maximum number of pending handshakes
    max_pending: usize,

    /// Currently pending handshakes
    pending: HashSet<SocketAddr>,
}

pub struct PeerInfo {
    pub did: Did,
    pub addr: SocketAddr,
    pub connected_at: Instant,
    pub last_activity: Instant,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub trust_score: f32,
}

impl PeerRegistry {
    pub fn new(max_peers: usize, max_pending: usize) -> Self;

    pub fn is_full(&self) -> bool;

    pub fn can_accept(&self) -> bool;

    pub fn add_pending(&mut self, addr: SocketAddr) -> Result<()>;

    pub fn promote_to_active(&mut self, addr: SocketAddr, did: Did) -> Result<()>;

    pub fn remove(&mut self, did: &Did);

    pub fn maybe_evict_for(&mut self, candidate: &PeerInfo) -> Option<Did>;
}
```

Configuration in `icn-core/src/config.rs`:

```toml
[network]
max_peers = 100           # Maximum active peers
max_pending_handshakes = 50  # Maximum concurrent handshakes
```

Integration in `icn-net/src/actor.rs`:

```rust
pub async fn handle_incoming(&mut self, mut incoming: quinn::Incoming) {
    while let Some(connecting) = incoming.next().await {
        // Check if we can accept more connections
        if !self.peer_registry.can_accept() {
            icn_obs::metrics::network::connections_rejected_inc();
            drop(connecting);
            continue;
        }

        // Add to pending set
        let addr = connecting.remote_address();
        if let Err(e) = self.peer_registry.add_pending(addr) {
            warn!("Cannot accept connection from {}: {}", addr, e);
            drop(connecting);
            continue;
        }

        // Spawn task to complete handshake
        let registry = self.peer_registry.clone();
        tokio::spawn(async move {
            match connecting.await {
                Ok(connection) => {
                    // Complete handshake, extract DID, promote to active
                    // ...
                }
                Err(e) => {
                    warn!("Handshake failed from {}: {}", addr, e);
                    registry.remove_pending(addr);
                }
            }
        });
    }
}
```

#### 1.2 Eviction Policy

Implement LRU-style eviction with trust awareness:

```rust
impl PeerRegistry {
    /// Evict lowest-value peer to make room for candidate
    pub fn maybe_evict_for(&mut self, candidate: &PeerInfo) -> Option<Did> {
        if self.active.len() < self.max_peers {
            return None; // No eviction needed
        }

        // Find eviction candidate
        let mut worst_peer: Option<(&Did, &PeerInfo)> = None;
        let mut worst_score = f32::MAX;

        for (did, info) in &self.active {
            // Compute eviction score (lower = more likely to evict)
            let score = self.compute_eviction_score(info);

            if score < worst_score {
                worst_score = score;
                worst_peer = Some((did, info));
            }
        }

        // Only evict if candidate is better than worst peer
        if self.compute_eviction_score(candidate) > worst_score {
            worst_peer.map(|(did, _)| did.clone())
        } else {
            None // Reject candidate
        }
    }

    fn compute_eviction_score(&self, info: &PeerInfo) -> f32 {
        // Higher score = keep peer
        // Lower score = evict peer

        let trust_weight = info.trust_score * 10.0;
        let activity_weight = info.activity_score() * 5.0;
        let age_penalty = info.connected_duration().as_secs() as f32 * 0.01;

        trust_weight + activity_weight - age_penalty
    }
}

impl PeerInfo {
    fn activity_score(&self) -> f32 {
        let seconds_since_activity = self.last_activity.elapsed().as_secs() as f32;
        1.0 / (1.0 + seconds_since_activity / 60.0) // Decay over minutes
    }

    fn connected_duration(&self) -> Duration {
        self.connected_at.elapsed()
    }
}
```

Prefer keeping peers with:
- Higher trust scores (from trust graph)
- Recent activity
- Stable connections

Evict peers with:
- Low trust scores
- No recent activity
- Connection issues

#### 1.3 Per-Peer Rate Limiting

Add per-peer bandwidth tracking and throttling:

```rust
pub struct PeerRateLimiter {
    /// Bytes per second limit
    bytes_per_sec: u64,

    /// Current window
    window_start: Instant,
    window_bytes: u64,
}

impl PeerRateLimiter {
    pub fn check_and_update(&mut self, bytes: u64) -> Result<(), RateLimitError> {
        // Reset window if needed
        let now = Instant::now();
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.window_bytes = 0;
        }

        // Check limit
        if self.window_bytes + bytes > self.bytes_per_sec {
            return Err(RateLimitError::ExceededLimit);
        }

        self.window_bytes += bytes;
        Ok(())
    }
}
```

Integrate into message handling:

```rust
async fn handle_peer_message(&mut self, peer: &Did, msg: NetworkMessage) -> Result<()> {
    let msg_size = msg.encoded_size();

    // Check per-peer rate limit
    if let Some(info) = self.peer_registry.get_mut(peer) {
        if let Err(e) = info.rate_limiter.check_and_update(msg_size) {
            warn!("Rate limited peer {}: {:?}", peer, e);
            icn_obs::metrics::network::peer_rate_limited_inc(peer);
            return Err(e.into());
        }

        info.bytes_received += msg_size;
        info.last_activity = Instant::now();
    }

    // Process message
    self.route_message(msg).await
}
```

#### 1.4 Metrics

Add Prometheus metrics for monitoring:

```rust
// In icn-obs/src/metrics/network.rs

/// Total connections accepted
pub fn connections_total_inc();

/// Total connections rejected (at capacity)
pub fn connections_rejected_inc();

/// Current active peer count
pub fn active_peers_set(count: usize);

/// Current pending handshake count
pub fn pending_handshakes_set(count: usize);

/// Per-peer rate limiting events
pub fn peer_rate_limited_inc(peer: &Did);

/// Peer eviction events
pub fn peer_evicted_inc(reason: &str);
```

### Testing

Create `icn-net/tests/connection_limits.rs`:

```rust
#[tokio::test]
async fn test_connection_limit_enforcement() {
    // Create node with max_peers = 3
    // Attempt to connect 5 peers
    // Verify only 3 are accepted
}

#[tokio::test]
async fn test_eviction_policy() {
    // Create node with max_peers = 2
    // Connect 3 peers with different trust scores
    // Verify lowest trust peer is evicted
}

#[tokio::test]
async fn test_pending_handshake_limit() {
    // Create node with max_pending = 2
    // Initiate 4 connections but don't complete handshake
    // Verify only 2 are pending, others rejected
}

#[tokio::test]
async fn test_per_peer_rate_limiting() {
    // Connect peer
    // Send messages rapidly
    // Verify rate limiting kicks in
}
```

## 2. DID-TLS Binding (MITM Prevention)

**Priority:** CRITICAL
**Effort:** MODERATE
**Risk:** Man-in-the-middle attacks, identity spoofing

### Current State

- Self-signed TLS certificates generated at runtime
- No cryptographic binding between TLS cert and DID
- Peer can claim any DID without proof of key ownership
- TLS provides channel security but not endpoint authentication

**This is the foundational vulnerability that undermines the entire trust model.**

### Design Goal

Establish cryptographic proof that:
- The TLS peer holds the private key for their claimed DID
- The TLS connection is authenticated by DID keys, not runtime-generated certs
- Any peer can verify the binding without external PKI

### Implementation Plan

#### 2.1 Identity Bundle Architecture

Create `icn-identity/src/bundle.rs`:

```rust
/// Cryptographically bound identity bundle
pub struct IdentityBundle {
    /// The DID for this identity
    pub did: Did,

    /// Ed25519 keypair for DID operations (signing, etc.)
    pub did_keypair: KeyPair,

    /// TLS certificate (self-signed)
    pub tls_cert: rustls::Certificate,

    /// TLS private key
    pub tls_key: rustls::PrivateKey,

    /// Binding signature proving ownership
    /// Signature = Sign_did_key(SHA256(tls_cert))
    pub tls_binding_sig: Vec<u8>,

    /// Timestamp when binding was created
    pub created_at: u64,
}

impl IdentityBundle {
    /// Generate new identity bundle with bound TLS cert
    pub fn generate() -> Result<Self> {
        // 1. Generate Ed25519 keypair for DID
        let did_keypair = KeyPair::generate()?;
        let did = did_keypair.did().clone();

        // 2. Generate TLS certificate
        // Option A: Use same Ed25519 key (convert to x25519 if needed)
        // Option B: Generate separate key and sign binding
        let (tls_cert, tls_key) = Self::generate_tls_cert(&did_keypair)?;

        // 3. Compute cert hash and sign with DID key
        let cert_hash = Self::hash_certificate(&tls_cert);
        let tls_binding_sig = did_keypair.sign(&cert_hash)?;

        Ok(IdentityBundle {
            did,
            did_keypair,
            tls_cert,
            tls_key,
            tls_binding_sig,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    /// Verify the TLS binding signature
    pub fn verify_binding(&self) -> Result<()> {
        let cert_hash = Self::hash_certificate(&self.tls_cert);
        self.did.verify(&cert_hash, &self.tls_binding_sig)?;
        Ok(())
    }

    fn generate_tls_cert(keypair: &KeyPair) -> Result<(rustls::Certificate, rustls::PrivateKey)> {
        // Generate self-signed cert with:
        // - Subject: DID as CN
        // - SAN extension: DID as URI
        // - Key: Derived from or separate from DID key

        use rcgen::{Certificate, CertificateParams, DnType};

        let mut params = CertificateParams::default();
        params.distinguished_name.push(
            DnType::CommonName,
            keypair.did().to_string()
        );
        params.subject_alt_names = vec![
            rcgen::SanType::URI(keypair.did().to_string()),
        ];

        // Use Ed25519 key directly or convert
        let cert = Certificate::from_params(params)?;
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();

        Ok((
            rustls::Certificate(cert_der),
            rustls::PrivateKey(key_der),
        ))
    }

    fn hash_certificate(cert: &rustls::Certificate) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&cert.0);
        hasher.finalize().into()
    }
}
```

#### 2.2 Protocol Handshake Extension

Define handshake message in `icn-net/src/protocol.rs`:

```rust
/// Initial protocol handshake message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloMessage {
    /// Claimed DID
    pub did: Did,

    /// SHA256 hash of TLS certificate
    pub tls_cert_hash: [u8; 32],

    /// Signature proving DID owns cert
    /// Signature = Sign_did_key(tls_cert_hash)
    pub tls_binding_sig: Vec<u8>,

    /// Protocol version
    pub protocol_version: u32,

    /// Optional: Topology info (region, cluster)
    pub topology_info: Option<TopologyInfo>,
}

impl HelloMessage {
    pub fn new(bundle: &IdentityBundle, topology: Option<TopologyInfo>) -> Self {
        let tls_cert_hash = IdentityBundle::hash_certificate(&bundle.tls_cert);

        HelloMessage {
            did: bundle.did.clone(),
            tls_cert_hash,
            tls_binding_sig: bundle.tls_binding_sig.clone(),
            protocol_version: 1,
            topology_info: topology,
        }
    }

    /// Verify the binding signature
    pub fn verify(&self, peer_cert: &rustls::Certificate) -> Result<()> {
        // 1. Hash the certificate we received via TLS
        let actual_hash = IdentityBundle::hash_certificate(peer_cert);

        // 2. Verify it matches claimed hash
        if actual_hash != self.tls_cert_hash {
            return Err(anyhow!("TLS certificate hash mismatch"));
        }

        // 3. Verify signature with DID public key
        self.did.verify(&self.tls_cert_hash, &self.tls_binding_sig)?;

        Ok(())
    }
}
```

#### 2.3 TLS Configuration Update

Update `icn-net/src/tls.rs`:

```rust
/// Create TLS config from identity bundle
pub fn create_tls_config(bundle: &IdentityBundle) -> Result<rustls::ServerConfig> {
    // Verify bundle integrity before using
    bundle.verify_binding()?;

    let cert_chain = vec![bundle.tls_cert.clone()];
    let key = bundle.tls_key.clone();

    let mut config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)?;

    // Use custom verifier that checks DID binding
    config.dangerous()
        .set_certificate_verifier(Arc::new(DidBindingVerifier::new()));

    Ok(config)
}

/// Custom certificate verifier that enforces DID binding
struct DidBindingVerifier {
    // Store pending verifications
    pending: Arc<RwLock<HashMap<Vec<u8>, HelloMessage>>>,
}

impl rustls::client::ServerCertVerifier for DidBindingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::Certificate,
        intermediates: &[rustls::Certificate],
        server_name: &rustls::ServerName,
        scts: &mut dyn Iterator<Item = &[u8]>,
        ocsp_response: &[u8],
        now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        // Accept the cert at TLS layer
        // We'll verify DID binding in the application handshake
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
```

#### 2.4 Connection Handshake Flow

Update `icn-net/src/actor.rs`:

```rust
async fn complete_connection_handshake(
    &mut self,
    connection: quinn::Connection,
) -> Result<Did> {
    // 1. Extract TLS certificate from QUIC connection
    let peer_cert = connection
        .peer_identity()
        .and_then(|id| id.downcast::<Vec<rustls::Certificate>>().ok())
        .and_then(|certs| certs.first().cloned())
        .ok_or_else(|| anyhow!("No peer certificate"))?;

    // 2. Send our HelloMessage
    let (mut send, mut recv) = connection.open_bi().await?;
    let our_hello = HelloMessage::new(&self.identity_bundle, self.topology_config.clone());
    write_message(&mut send, &our_hello).await?;

    // 3. Receive peer HelloMessage
    let peer_hello: HelloMessage = read_message(&mut recv).await?;

    // 4. Verify DID-TLS binding
    peer_hello.verify(&peer_cert).context("DID-TLS binding verification failed")?;

    // 5. Update topology if provided
    if let Some(topology) = peer_hello.topology_info {
        self.update_neighbor_sets(&peer_hello.did, topology);
    }

    info!("✅ Verified DID-TLS binding for {}", peer_hello.did);

    Ok(peer_hello.did)
}

async fn handle_incoming_connection(
    &mut self,
    connection: quinn::Connection,
) -> Result<()> {
    // Complete handshake and verify DID binding
    let peer_did = self.complete_connection_handshake(connection.clone()).await?;

    // Register peer
    self.peer_registry.add_peer(peer_did.clone(), connection)?;

    // Spawn message handler
    self.spawn_connection_handler(peer_did, connection).await;

    Ok(())
}
```

#### 2.5 Backwards Compatibility

Add configuration flag:

```toml
[network.security]
# Enforce DID-TLS binding (disable only for testing)
enforce_did_tls_binding = true

# Allow insecure connections in development
dev_mode = false
```

```rust
impl NetworkActor {
    async fn verify_peer(&self, hello: &HelloMessage, cert: &rustls::Certificate) -> Result<()> {
        if !self.config.enforce_did_tls_binding || self.config.dev_mode {
            warn!("⚠️  DID-TLS binding verification disabled (dev mode)");
            return Ok(());
        }

        hello.verify(cert)
    }
}
```

### Testing

Create `icn-identity/tests/bundle_tests.rs`:

```rust
#[test]
fn test_bundle_generation() {
    let bundle = IdentityBundle::generate().unwrap();
    assert!(bundle.verify_binding().is_ok());
}

#[test]
fn test_binding_tampering() {
    let mut bundle = IdentityBundle::generate().unwrap();

    // Tamper with signature
    bundle.tls_binding_sig[0] ^= 0xFF;

    assert!(bundle.verify_binding().is_err());
}
```

Create `icn-net/tests/did_tls_binding.rs`:

```rust
#[tokio::test]
async fn test_valid_did_tls_binding() {
    // Create two nodes with proper bindings
    // Connect them
    // Verify handshake succeeds
}

#[tokio::test]
async fn test_reject_mismatched_did() {
    // Create node A with DID_A
    // Create node B with DID_B
    // Node B claims DID_A in HelloMessage
    // Verify connection is rejected
}

#[tokio::test]
async fn test_reject_invalid_signature() {
    // Create valid bundle
    // Tamper with signature
    // Attempt connection
    // Verify rejection
}
```

## 3. Message Verification Between Nodes

**Priority:** CRITICAL
**Effort:** MODERATE
**Risk:** Message tampering, replay attacks, spoofing

### Current State

- QUIC/TLS provides channel integrity
- No application-level message signing
- No replay protection
- Cannot verify message origin beyond TLS session

### Design Goal

Every application-level message must:
- Be signed by sender's DID key
- Include freshness/sequence information
- Be verifiable by any receiver
- Prevent replay attacks

### Implementation Plan

#### 3.1 SignedEnvelope Protocol

Create `icn-net/src/envelope.rs`:

```rust
/// Application-level signed message envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    /// Sender DID (verified via signature)
    pub from: Did,

    /// Monotonic sequence number (per-sender)
    pub sequence: u64,

    /// Timestamp (milliseconds since epoch)
    pub timestamp: u64,

    /// Payload type discriminator
    pub payload_type: PayloadType,

    /// Serialized payload bytes
    pub payload: Vec<u8>,

    /// Ed25519 signature over canonical encoding
    /// Signature = Sign_from(sequence || timestamp || payload_type || payload)
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PayloadType {
    Gossip = 1,
    Ledger = 2,
    Trust = 3,
    Contract = 4,
    Rpc = 5,
}

impl SignedEnvelope {
    /// Create and sign a new envelope
    pub fn new(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;

        let mut envelope = SignedEnvelope {
            from: from.clone(),
            sequence,
            timestamp,
            payload_type,
            payload,
            signature: Vec::new(),
        };

        // Compute signature over canonical encoding
        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input)?;

        Ok(envelope)
    }

    /// Verify signature and freshness
    pub fn verify(&self, max_age_secs: u64) -> Result<()> {
        // 1. Verify signature
        let sig_input = self.canonical_encoding();
        self.from.verify(&sig_input, &self.signature)?;

        // 2. Check timestamp freshness
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;

        let age = now.saturating_sub(self.timestamp) / 1000;
        if age > max_age_secs {
            return Err(anyhow!("Message too old: {} seconds", age));
        }

        Ok(())
    }

    fn canonical_encoding(&self) -> Vec<u8> {
        // Deterministic encoding for signing
        let mut buf = Vec::new();
        buf.extend_from_slice(self.from.as_bytes());
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.push(self.payload_type as u8);
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserialize payload
    pub fn decode_payload<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        bincode::deserialize(&self.payload)
            .context("Failed to deserialize payload")
    }
}
```

#### 3.2 Sequence Number Tracking

Create `icn-net/src/replay_guard.rs`:

```rust
/// Per-peer sequence tracking for replay protection
pub struct ReplayGuard {
    /// Last seen sequence per peer
    sequences: HashMap<Did, SequenceWindow>,

    /// Maximum allowed clock skew (seconds)
    max_clock_skew: u64,
}

struct SequenceWindow {
    /// Highest sequence number seen
    max_seq: u64,

    /// Bloom filter of recent sequences (for out-of-order)
    recent: BloomFilter,

    /// Last update time
    last_update: Instant,
}

impl ReplayGuard {
    pub fn new(max_clock_skew: u64) -> Self {
        ReplayGuard {
            sequences: HashMap::new(),
            max_clock_skew,
        }
    }

    /// Check if message is fresh (not replayed)
    pub fn check(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        // Verify signature and age
        envelope.verify(self.max_clock_skew)?;

        // Get or create sequence window
        let window = self.sequences
            .entry(envelope.from.clone())
            .or_insert_with(|| SequenceWindow::new());

        // Check sequence number
        if envelope.sequence <= window.max_seq {
            // Could be replay or out-of-order
            if window.recent.contains(&envelope.sequence.to_be_bytes()) {
                return Err(anyhow!("Replay detected: sequence {}", envelope.sequence));
            }
        }

        // Update window
        window.max_seq = window.max_seq.max(envelope.sequence);
        window.recent.insert(&envelope.sequence.to_be_bytes());
        window.last_update = Instant::now();

        Ok(())
    }

    /// Periodic cleanup of stale windows
    pub fn cleanup(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.sequences.retain(|_, window| {
            now.duration_since(window.last_update) < max_age
        });
    }
}

impl SequenceWindow {
    fn new() -> Self {
        SequenceWindow {
            max_seq: 0,
            recent: BloomFilter::new(10000, 0.001), // 10k recent, 0.1% FP rate
            last_update: Instant::now(),
        }
    }
}
```

#### 3.3 Actor Integration

Update actors to use `SignedEnvelope`:

```rust
// In icn-net/src/actor.rs

/// Outgoing message with auto-signing
pub async fn send_message(&mut self, to: Did, payload_type: PayloadType, payload: Vec<u8>) -> Result<()> {
    // Get next sequence number
    let sequence = self.next_sequence();

    // Create signed envelope
    let envelope = SignedEnvelope::new(
        &self.identity_bundle.did,
        &self.identity_bundle.did_keypair,
        sequence,
        payload_type,
        payload,
    )?;

    // Send over QUIC
    self.send_envelope(to, envelope).await
}

/// Incoming message with verification
async fn handle_envelope(&mut self, from: Did, envelope: SignedEnvelope) -> Result<()> {
    // Verify signature and replay
    self.replay_guard.check(&envelope)?;

    // Route to appropriate actor based on payload_type
    match envelope.payload_type {
        PayloadType::Gossip => {
            let msg: GossipMessage = envelope.decode_payload()?;
            self.route_to_gossip(from, msg).await
        }
        PayloadType::Ledger => {
            let msg: LedgerMessage = envelope.decode_payload()?;
            self.route_to_ledger(from, msg).await
        }
        // ... other types
    }
}
```

#### 3.4 Internal Actor Messages

For internal actor communication (same process), define typed messages:

```rust
// In icn-core/src/messages.rs

/// Internal actor message enum (no crypto needed)
pub enum ActorMessage {
    Gossip(GossipMessage),
    Ledger(LedgerMessage),
    Trust(TrustMessage),
    Contract(ContractMessage),
    Network(NetworkMessage),
}

impl ActorMessage {
    pub fn gossip(msg: GossipMessage) -> Self {
        ActorMessage::Gossip(msg)
    }

    // ... constructors for each type
}

/// Actor message handler trait
#[async_trait]
pub trait ActorHandler {
    async fn handle(&mut self, msg: ActorMessage) -> Result<()>;
}
```

Replace closure-based callbacks with typed message passing:

```rust
// Before (closure)
let callback: Arc<dyn Fn(GossipMessage) + Send + Sync> = Arc::new(|msg| {
    // ...
});

// After (typed message)
actor.send(ActorMessage::gossip(msg)).await?;
```

### Testing

Create `icn-net/tests/signed_envelopes.rs`:

```rust
#[test]
fn test_envelope_signing() {
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().clone();

    let envelope = SignedEnvelope::new(
        &did,
        &keypair,
        1,
        PayloadType::Gossip,
        b"test payload".to_vec(),
    ).unwrap();

    assert!(envelope.verify(300).is_ok());
}

#[test]
fn test_tampered_envelope() {
    let keypair = KeyPair::generate().unwrap();
    let mut envelope = SignedEnvelope::new(
        keypair.did(),
        &keypair,
        1,
        PayloadType::Gossip,
        b"test payload".to_vec(),
    ).unwrap();

    // Tamper with payload
    envelope.payload[0] ^= 0xFF;

    assert!(envelope.verify(300).is_err());
}

#[tokio::test]
async fn test_replay_detection() {
    let mut guard = ReplayGuard::new(300);
    let keypair = KeyPair::generate().unwrap();

    let envelope = SignedEnvelope::new(
        keypair.did(),
        &keypair,
        1,
        PayloadType::Gossip,
        b"test".to_vec(),
    ).unwrap();

    // First delivery: OK
    assert!(guard.check(&envelope).is_ok());

    // Replay: Rejected
    assert!(guard.check(&envelope).is_err());
}
```

## 4. Trust Inflation & Decay

**Priority:** MEDIUM
**Effort:** MODERATE
**Risk:** Trust loops, unbounded trust, stale trust values

### Current State

- Weighted directed graph with simple path lookup
- No decay over time
- No cycle handling
- No cap on transitive trust
- No distinction between trust sources

### Design Goal

- Prevent "trust loops" where mutual vouching inflates scores
- Ground trust values in time and participation
- Cap transitive trust propagation
- Prepare for participation-based trust signals

### Implementation Plan

#### 4.1 Trust Edge Metadata

Update `icn-trust/src/types.rs`:

```rust
#[derive(Debug, Clone)]
pub struct TrustEdge {
    pub from: Did,
    pub to: Did,
    pub weight: f64,           // [0.0, 1.0]
    pub timestamp: u64,        // When edge was created/updated
    pub kind: TrustKind,       // Source of trust
    pub evidence: Vec<TrustEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustKind {
    /// Manually set by user
    Manual,

    /// Derived from participation (ledger, contracts, etc.)
    Participation,

    /// System-level trust (bootstrap nodes, etc.)
    System,
}

#[derive(Debug, Clone)]
pub enum TrustEvidence {
    LedgerCooperation {
        transaction_count: u64,
        total_value: i64,
    },
    ContractSuccess {
        contract_id: String,
        execution_count: u64,
    },
    ManualVouch {
        note: String,
    },
}
```

#### 4.2 Bounded Transitive Trust Computation

Update `icn-trust/src/compute.rs`:

```rust
pub struct TrustComputeConfig {
    /// Maximum path depth for transitive trust
    pub max_depth: usize,

    /// Per-hop decay factor (multiplied at each step)
    pub hop_decay: f64,

    /// Time decay rate (exponential)
    pub time_decay_lambda: f64,

    /// Minimum edge weight to consider
    pub min_edge_weight: f64,
}

impl Default for TrustComputeConfig {
    fn default() -> Self {
        TrustComputeConfig {
            max_depth: 3,              // Max 3 hops
            hop_decay: 0.7,            // 70% retained per hop
            time_decay_lambda: 0.0001, // Slow decay
            min_edge_weight: 0.1,      // Ignore weak edges
        }
    }
}

impl TrustGraph {
    pub fn compute_trust_with_config(
        &self,
        from: &Did,
        to: &Did,
        config: &TrustComputeConfig,
    ) -> f64 {
        let now = current_timestamp();

        // BFS with depth limit
        let mut visited = HashSet::new();
        let mut max_trust = 0.0;

        self.explore_paths(
            from,
            to,
            &config,
            0,      // current depth
            1.0,    // current trust (no decay yet)
            now,
            &mut visited,
            &mut max_trust,
        );

        max_trust.min(1.0)
    }

    fn explore_paths(
        &self,
        current: &Did,
        target: &Did,
        config: &TrustComputeConfig,
        depth: usize,
        path_trust: f64,
        now: u64,
        visited: &mut HashSet<Did>,
        max_trust: &mut f64,
    ) {
        // Base cases
        if depth >= config.max_depth {
            return;
        }
        if visited.contains(current) {
            return; // Prevent cycles
        }
        if path_trust < config.min_edge_weight {
            return; // Path too weak
        }

        visited.insert(current.clone());

        // Get outgoing edges
        for edge in self.get_outgoing_edges(current) {
            // Apply time decay to edge weight
            let age = now.saturating_sub(edge.timestamp);
            let decayed_weight = edge.weight * Self::time_decay(age, config.time_decay_lambda);

            if decayed_weight < config.min_edge_weight {
                continue;
            }

            // Apply hop decay
            let new_trust = path_trust * decayed_weight * config.hop_decay;

            if edge.to == *target {
                // Found target
                *max_trust = max_trust.max(new_trust);
            } else {
                // Continue exploring
                self.explore_paths(
                    &edge.to,
                    target,
                    config,
                    depth + 1,
                    new_trust,
                    now,
                    visited,
                    max_trust,
                );
            }
        }

        visited.remove(current);
    }

    fn time_decay(age_seconds: u64, lambda: f64) -> f64 {
        // Exponential decay: e^(-λ * t)
        let t = age_seconds as f64;
        (-lambda * t).exp()
    }
}
```

#### 4.3 Participation Evidence Hooks

Add API for future participation tracking:

```rust
// In icn-trust/src/events.rs

pub enum TrustEvent {
    /// Manual trust update
    ManualUpdate {
        from: Did,
        to: Did,
        delta: f64,
        note: String,
    },

    /// Ledger cooperation observed
    LedgerCoop {
        party_a: Did,
        party_b: Did,
        amount: i64,
    },

    /// Successful contract execution
    ContractSuccess {
        participants: Vec<Did>,
        contract_id: String,
    },

    /// Observed reliable message delivery
    NetworkReliability {
        peer: Did,
        messages_delivered: u64,
    },
}

impl TrustGraph {
    /// Process trust event and update graph
    pub fn process_event(&mut self, event: TrustEvent) -> Result<()> {
        match event {
            TrustEvent::ManualUpdate { from, to, delta, note } => {
                self.update_manual_trust(from, to, delta, note)?;
            }
            TrustEvent::LedgerCoop { party_a, party_b, amount } => {
                // Future: Derive trust from successful ledger cooperation
                // For now: no-op, just log
                info!("Ledger cooperation: {} <-> {} ({})", party_a, party_b, amount);
            }
            TrustEvent::ContractSuccess { participants, contract_id } => {
                // Future: Multi-party contract builds trust between all participants
                info!("Contract success: {:?} in {}", participants, contract_id);
            }
            TrustEvent::NetworkReliability { peer, messages_delivered } => {
                // Future: Reliable peers earn minor trust bump
                info!("Peer {} delivered {} messages", peer, messages_delivered);
            }
        }
        Ok(())
    }
}
```

Emit events from other subsystems:

```rust
// In icn-ledger/src/ledger.rs
impl Ledger {
    async fn apply_entry(&mut self, entry: JournalEntry) -> Result<()> {
        // ... existing logic ...

        // Emit trust event for successful cooperation
        if let Some(trust_graph) = &self.trust_graph {
            trust_graph.write().await.process_event(TrustEvent::LedgerCoop {
                party_a: entry.from.clone(),
                party_b: entry.to.clone(),
                amount: entry.amount,
            })?;
        }

        Ok(())
    }
}
```

#### 4.4 Trust Classes with Bounded Computation

Update trust class computation to use new bounded algorithm:

```rust
impl TrustGraph {
    pub fn classify_peer(&self, peer: &Did) -> TrustClass {
        let config = TrustComputeConfig::default();
        let score = self.compute_trust_with_config(&self.owner_did, peer, &config);

        match score {
            s if s >= 0.7 => TrustClass::Federated,
            s if s >= 0.4 => TrustClass::Partner,
            s if s >= 0.1 => TrustClass::Known,
            _ => TrustClass::Isolated,
        }
    }
}
```

### Testing

Create `icn-trust/tests/bounded_trust.rs`:

```rust
#[test]
fn test_depth_limited_paths() {
    // Create chain: A -> B -> C -> D -> E
    // With max_depth=3, A should not reach E
}

#[test]
fn test_cycle_prevention() {
    // Create cycle: A -> B -> C -> A
    // Verify computation terminates and doesn't inflate trust
}

#[test]
fn test_time_decay() {
    // Create edge with old timestamp
    // Verify trust score is reduced
}

#[test]
fn test_hop_decay() {
    // Create path: A -1.0-> B -1.0-> C
    // With hop_decay=0.7, trust(A,C) should be ~0.7, not 1.0
}
```

## 5. Key Rotation & DID Transitions

**Priority:** MEDIUM
**Effort:** MODERATE
**Risk:** Lost keys, identity continuity, trust graph consistency

### Current State

- Single Ed25519 keypair per DID
- No rotation mechanism
- No migration path if key is compromised
- No multi-device support

### Design Goal

- Allow keypairs to rotate without losing identity
- Maintain continuity of trust relationships
- Prepare for multi-device scenarios
- Preserve ledger and contract history

### Implementation Plan

#### 5.1 Rotation Event Type

Create `icn-identity/src/rotation.rs`:

```rust
/// Key rotation event proving ownership of both old and new keys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationEvent {
    /// Old DID being rotated away from
    pub old_did: Did,

    /// New DID being rotated to
    pub new_did: Did,

    /// Signature by old key over (old_did || new_did)
    pub old_key_signature: Vec<u8>,

    /// Signature by new key over (old_did || new_did)
    pub new_key_signature: Vec<u8>,

    /// Timestamp of rotation
    pub timestamp: u64,

    /// Optional reason/note
    pub reason: String,
}

impl KeyRotationEvent {
    pub fn create(
        old_keypair: &KeyPair,
        new_keypair: &KeyPair,
        reason: String,
    ) -> Result<Self> {
        let old_did = old_keypair.did().clone();
        let new_did = new_keypair.did().clone();

        // Canonical encoding for signing
        let mut sig_input = Vec::new();
        sig_input.extend_from_slice(old_did.as_bytes());
        sig_input.extend_from_slice(new_did.as_bytes());

        let old_key_signature = old_keypair.sign(&sig_input)?;
        let new_key_signature = new_keypair.sign(&sig_input)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs();

        Ok(KeyRotationEvent {
            old_did,
            new_did,
            old_key_signature,
            new_key_signature,
            timestamp,
            reason,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let mut sig_input = Vec::new();
        sig_input.extend_from_slice(self.old_did.as_bytes());
        sig_input.extend_from_slice(self.new_did.as_bytes());

        // Verify both signatures
        self.old_did.verify(&sig_input, &self.old_key_signature)?;
        self.new_did.verify(&sig_input, &self.new_key_signature)?;

        Ok(())
    }
}

/// Rotation chain tracking identity evolution
pub struct RotationChain {
    /// Current (most recent) DID
    pub current_did: Did,

    /// Historical rotations
    pub rotations: Vec<KeyRotationEvent>,
}

impl RotationChain {
    /// Resolve any historical DID to current DID
    pub fn resolve(&self, historical_did: &Did) -> Option<Did> {
        if historical_did == &self.current_did {
            return Some(self.current_did.clone());
        }

        // Search rotation history
        for rotation in &self.rotations {
            if &rotation.old_did == historical_did {
                // Follow chain forward
                return Some(self.current_did.clone());
            }
        }

        None
    }

    /// Add rotation event
    pub fn add_rotation(&mut self, event: KeyRotationEvent) -> Result<()> {
        event.verify()?;

        if event.old_did != self.current_did {
            return Err(anyhow!("Rotation chain broken: old_did mismatch"));
        }

        self.current_did = event.new_did.clone();
        self.rotations.push(event);

        Ok(())
    }
}
```

#### 5.2 Keystore Integration

Update `icn-identity/src/keystore.rs`:

```rust
impl AgeKeyStore {
    /// Rotate to new keypair and record transition
    pub fn rotate(&mut self, new_keypair: KeyPair, reason: String) -> Result<KeyRotationEvent> {
        let old_keypair = &self.keypair;

        // Create rotation event
        let event = KeyRotationEvent::create(old_keypair, &new_keypair, reason)?;

        // Update keypair
        self.keypair = new_keypair;

        // Store rotation event
        self.rotation_chain.add_rotation(event.clone())?;

        // Persist to disk
        self.save()?;

        Ok(event)
    }

    /// Export rotation history
    pub fn export_rotation_chain(&self) -> RotationChain {
        self.rotation_chain.clone()
    }
}
```

#### 5.3 Trust Graph Awareness

Update `icn-trust/src/trust_graph.rs`:

```rust
impl TrustGraph {
    /// Maintain DID alias mapping for rotations
    did_aliases: HashMap<Did, Did>, // old -> current

    /// Process rotation event
    pub fn apply_rotation(&mut self, event: KeyRotationEvent) -> Result<()> {
        event.verify()?;

        // Update alias mapping
        self.did_aliases.insert(event.old_did.clone(), event.new_did.clone());

        // Migrate edges
        self.migrate_edges(&event.old_did, &event.new_did)?;

        info!("Applied key rotation: {} -> {}", event.old_did, event.new_did);
        Ok(())
    }

    fn migrate_edges(&mut self, old_did: &Did, new_did: &Did) -> Result<()> {
        // Update all edges referencing old_did
        for edge in self.edges.iter_mut() {
            if &edge.from == old_did {
                edge.from = new_did.clone();
            }
            if &edge.to == old_did {
                edge.to = new_did.clone();
            }
        }
        Ok(())
    }

    /// Resolve DID through alias chain
    pub fn resolve_did(&self, did: &Did) -> Did {
        self.did_aliases.get(did).cloned().unwrap_or_else(|| did.clone())
    }

    /// Compute trust with alias resolution
    pub fn compute_trust(&self, from: &Did, to: &Did) -> f64 {
        let from_resolved = self.resolve_did(from);
        let to_resolved = self.resolve_did(to);

        self.compute_trust_resolved(&from_resolved, &to_resolved)
    }
}
```

#### 5.4 CLI Commands

Add to `icnctl`:

```rust
// In bins/icnctl/src/commands/identity.rs

pub async fn cmd_rotate(reason: Option<String>) -> Result<()> {
    // Load current keystore
    let mut keystore = load_keystore()?;

    println!("Current DID: {}", keystore.keypair.did());
    println!("⚠️  This will rotate to a new keypair.");
    println!("⚠️  Make sure to back up the new keystore!");

    if !confirm("Proceed with rotation?")? {
        println!("Rotation cancelled.");
        return Ok(());
    }

    // Generate new keypair
    let new_keypair = KeyPair::generate()?;
    println!("New DID: {}", new_keypair.did());

    // Perform rotation
    let event = keystore.rotate(
        new_keypair,
        reason.unwrap_or_else(|| "Manual rotation via icnctl".to_string()),
    )?;

    println!("✅ Rotation complete!");
    println!("Rotation event:");
    println!("{}", serde_json::to_string_pretty(&event)?);

    // Optionally gossip rotation to trusted peers
    if confirm("Broadcast rotation to network?")? {
        broadcast_rotation_event(event).await?;
    }

    Ok(())
}

pub async fn cmd_show_rotation_history() -> Result<()> {
    let keystore = load_keystore()?;
    let chain = keystore.export_rotation_chain();

    println!("Current DID: {}", chain.current_did);
    println!("\nRotation history:");

    for (i, event) in chain.rotations.iter().enumerate() {
        println!("\n#{} - {}", i + 1, format_timestamp(event.timestamp));
        println!("  {} -> {}", event.old_did, event.new_did);
        println!("  Reason: {}", event.reason);
    }

    Ok(())
}
```

Usage:

```bash
# Rotate keypair
icnctl identity rotate --reason "Scheduled rotation"

# View rotation history
icnctl identity history

# Export rotation chain (for verification)
icnctl identity export-chain > rotation-chain.json
```

#### 5.5 Multi-Device Preparation (Design Only)

Design for future multi-device support:

```rust
// Future: icn-identity/src/multidevice.rs

/// Root identity with multiple device keys
pub struct MultiDeviceIdentity {
    /// Root DID (primary identity)
    pub root_did: Did,

    /// Root keypair (kept offline/secure)
    root_keypair: Option<KeyPair>,

    /// Device DIDs signed by root
    pub devices: Vec<DeviceKey>,
}

pub struct DeviceKey {
    /// Device-specific DID
    pub device_did: Did,

    /// Device keypair
    pub keypair: KeyPair,

    /// Device name/description
    pub name: String,

    /// Root signature authorizing this device
    /// Signature = Sign_root(device_did || name || timestamp)
    pub authorization: Vec<u8>,

    /// Creation timestamp
    pub created_at: u64,
}
```

This allows:
- Root DID represents the person/organization
- Each device has its own DID and keypair
- Root key signs authorization for each device
- Devices can act on behalf of root identity
- Root key can revoke device authorizations

### Testing

Create `icn-identity/tests/rotation_tests.rs`:

```rust
#[test]
fn test_rotation_event_creation() {
    let old_kp = KeyPair::generate().unwrap();
    let new_kp = KeyPair::generate().unwrap();

    let event = KeyRotationEvent::create(&old_kp, &new_kp, "test".into()).unwrap();
    assert!(event.verify().is_ok());
}

#[test]
fn test_rotation_chain() {
    let mut chain = RotationChain::new(did_1);

    let event = KeyRotationEvent::create(&kp1, &kp2, "rotate".into()).unwrap();
    chain.add_rotation(event).unwrap();

    assert_eq!(chain.resolve(&did_1), Some(did_2));
}

#[tokio::test]
async fn test_trust_graph_rotation() {
    let mut tg = TrustGraph::new(store, owner);

    // Add trust edge to old_did
    tg.add_edge(TrustEdge::new(owner, old_did, 0.8)).unwrap();

    // Apply rotation
    tg.apply_rotation(rotation_event).unwrap();

    // Trust should follow to new_did
    let trust = tg.compute_trust(&owner, &new_did);
    assert!(trust > 0.7);
}
```

## 6. Minimal Ledger & Gossip Security

**Priority:** MEDIUM
**Effort:** MODERATE
**Risk:** Ledger forks, equivocation, hidden conflicts

### Current State

- No ledger sync over gossip
- No causal ordering enforcement
- No integration between ledger and gossip
- No detection of ledger forks or equivocation

### Design Goal

- Basic ledger anti-entropy via gossip
- Detect (but don't necessarily resolve) conflicts
- Surface causal anomalies via metrics
- Prepare for future conflict resolution policies

### Implementation Plan

#### 6.1 Ledger Gossip Messages

Create `icn-ledger/src/gossip_sync.rs`:

```rust
/// Ledger synchronization messages over gossip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LedgerGossipMsg {
    /// Announce ledger head(s)
    AnnounceHead {
        ledger_id: LedgerId,
        head_hash: Hash,
        entry_count: u64,
        timestamp: u64,
    },

    /// Request entries between two hashes
    RequestDiff {
        ledger_id: LedgerId,
        from_hash: Hash,
        to_hash: Hash,
    },

    /// Respond with entries
    Entries {
        ledger_id: LedgerId,
        entries: Vec<JournalEntry>,
    },

    /// Report detected conflict/fork
    ConflictReport {
        ledger_id: LedgerId,
        conflicting_heads: Vec<Hash>,
    },
}

pub type LedgerId = Did; // Each DID has their own ledger

impl Ledger {
    /// Start periodic anti-entropy
    pub async fn start_sync_loop(&self, gossip: Arc<RwLock<GossipActor>>) {
        let ledger = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                interval.tick().await;

                if let Err(e) = ledger.announce_heads(&gossip).await {
                    warn!("Failed to announce ledger heads: {}", e);
                }
            }
        });
    }

    /// Announce our ledger head(s) to the network
    async fn announce_heads(&self, gossip: &Arc<RwLock<GossipActor>>) -> Result<()> {
        let heads = self.get_heads()?;

        for (ledger_id, head) in heads {
            let msg = LedgerGossipMsg::AnnounceHead {
                ledger_id: ledger_id.clone(),
                head_hash: head.hash,
                entry_count: head.entry_count,
                timestamp: current_timestamp(),
            };

            // Publish to ledger sync topic
            let payload = bincode::serialize(&msg)?;
            gossip.write().await.publish("ledger:sync", payload)?;
        }

        Ok(())
    }

    /// Handle incoming ledger gossip message
    pub async fn handle_gossip_msg(&mut self, from: &Did, msg: LedgerGossipMsg) -> Result<()> {
        match msg {
            LedgerGossipMsg::AnnounceHead { ledger_id, head_hash, .. } => {
                self.handle_head_announcement(from, ledger_id, head_hash).await
            }
            LedgerGossipMsg::RequestDiff { ledger_id, from_hash, to_hash } => {
                self.handle_diff_request(from, ledger_id, from_hash, to_hash).await
            }
            LedgerGossipMsg::Entries { ledger_id, entries } => {
                self.handle_entries(from, ledger_id, entries).await
            }
            LedgerGossipMsg::ConflictReport { ledger_id, conflicting_heads } => {
                self.handle_conflict_report(from, ledger_id, conflicting_heads).await
            }
        }
    }

    async fn handle_head_announcement(
        &mut self,
        from: &Did,
        ledger_id: LedgerId,
        remote_head: Hash,
    ) -> Result<()> {
        // Check if we have this ledger
        let local_head = match self.get_head(&ledger_id) {
            Some(head) => head,
            None => {
                // We don't have this ledger, request full history
                self.request_full_ledger(from, &ledger_id).await?;
                return Ok(());
            }
        };

        if local_head.hash == remote_head {
            // Already synced
            return Ok(());
        }

        // Check if remote_head is in our history
        if self.has_entry(&remote_head)? {
            // Remote is behind us
            return Ok(());
        }

        // Check if local_head is in remote's history (need to ask)
        // For now: request diff
        self.request_diff(from, &ledger_id, local_head.hash, remote_head).await?;

        Ok(())
    }

    async fn handle_diff_request(
        &mut self,
        from: &Did,
        ledger_id: LedgerId,
        from_hash: Hash,
        to_hash: Hash,
    ) -> Result<()> {
        // Find entries between hashes
        let entries = self.get_entries_between(&ledger_id, &from_hash, &to_hash)?;

        if entries.is_empty() {
            // Potential fork: we don't have a path from from_hash to to_hash
            warn!("⚠️  Fork detected in ledger {}: no path from {} to {}",
                  ledger_id, from_hash, to_hash);

            icn_obs::metrics::ledger::fork_detected_inc(&ledger_id);

            // Send conflict report
            self.send_conflict_report(from, ledger_id, vec![from_hash, to_hash]).await?;
            return Ok(());
        }

        // Send entries
        let msg = LedgerGossipMsg::Entries { ledger_id, entries };
        self.send_gossip_msg(from, msg).await?;

        Ok(())
    }

    async fn handle_entries(
        &mut self,
        from: &Did,
        ledger_id: LedgerId,
        entries: Vec<JournalEntry>,
    ) -> Result<()> {
        // Validate and apply entries
        for entry in entries {
            // Verify signatures, causality, etc.
            if let Err(e) = self.validate_entry(&entry) {
                warn!("⚠️  Invalid entry from {}: {}", from, e);
                icn_obs::metrics::ledger::invalid_entry_inc(&ledger_id);
                continue;
            }

            // Apply entry
            match self.apply_entry(entry.clone()) {
                Ok(_) => {
                    icn_obs::metrics::ledger::entries_synced_inc(&ledger_id);
                }
                Err(e) => {
                    warn!("⚠️  Failed to apply entry: {}", e);
                    // Quarantine entry for manual review
                    self.quarantine_entry(entry)?;
                }
            }
        }

        Ok(())
    }
}
```

#### 6.2 Causal Ordering

Add vector clock validation:

```rust
impl Ledger {
    fn validate_entry(&self, entry: &JournalEntry) -> Result<()> {
        // 1. Verify signature
        entry.verify_signature()?;

        // 2. Check vector clock causality
        if let Some(expected_clock) = self.get_vector_clock(&entry.ledger_id) {
            if !entry.vector_clock.is_causal_successor(&expected_clock) {
                return Err(anyhow!("Causal violation: entry not a successor"));
            }
        }

        // 3. Check parent hash
        if let Some(parent) = &entry.parent_hash {
            if !self.has_entry(parent)? {
                return Err(anyhow!("Missing parent entry: {}", parent));
            }
        }

        Ok(())
    }

    /// Quarantine entry for later resolution
    fn quarantine_entry(&mut self, entry: JournalEntry) -> Result<()> {
        self.quarantined_entries.insert(entry.hash.clone(), entry);
        icn_obs::metrics::ledger::entries_quarantined_inc();
        Ok(())
    }
}
```

#### 6.3 Conflict Detection & Reporting

Add fork detection:

```rust
impl Ledger {
    /// Detect if ledger has forked
    pub fn detect_fork(&self, ledger_id: &LedgerId) -> Option<ForkInfo> {
        let heads = self.get_heads_for_ledger(ledger_id);

        if heads.len() > 1 {
            Some(ForkInfo {
                ledger_id: ledger_id.clone(),
                conflicting_heads: heads,
                detected_at: Instant::now(),
            })
        } else {
            None
        }
    }

    /// Periodic fork detection
    pub async fn check_for_forks(&self) {
        for ledger_id in self.ledgers.keys() {
            if let Some(fork) = self.detect_fork(ledger_id) {
                warn!("⚠️  Fork detected in ledger {}: {} conflicting heads",
                      ledger_id, fork.conflicting_heads.len());

                // Emit metric
                icn_obs::metrics::ledger::fork_detected_inc(ledger_id);

                // For now: just log and metric
                // Future: implement conflict resolution policy
            }
        }
    }
}
```

#### 6.4 Metrics

Add ledger sync metrics:

```rust
// In icn-obs/src/metrics/ledger.rs

pub fn entries_synced_inc(ledger_id: &LedgerId);
pub fn invalid_entry_inc(ledger_id: &LedgerId);
pub fn entries_quarantined_inc();
pub fn fork_detected_inc(ledger_id: &LedgerId);
pub fn causal_anomaly_inc(ledger_id: &LedgerId);
```

### Testing

Create `icn-ledger/tests/gossip_sync.rs`:

```rust
#[tokio::test]
async fn test_ledger_sync() {
    // Create two nodes
    // Node A adds entry
    // Wait for gossip propagation
    // Verify Node B has entry
}

#[tokio::test]
async fn test_fork_detection() {
    // Create two nodes
    // Both add conflicting entries to same ledger
    // Verify fork is detected and reported
}

#[tokio::test]
async fn test_causal_ordering() {
    // Send entries out of causal order
    // Verify rejected until dependencies available
}
```

## 7. Supervisor & Actor Failure Resilience

**Priority:** MEDIUM
**Effort:** LOW
**Risk:** Permanent actor failures, message loss, service unavailability

### Current State

- Actors fail permanently on panic
- No restart policy
- In-flight messages lost on crash
- No backoff or flap detection

### Design Goal

- Actors restart automatically on failure (with limits)
- Supervisor tracks failures and applies backoff
- Critical subsystems marked for aggressive restart
- Flapping actors detected and isolated

### Implementation Plan

#### 7.1 Actor Restart Policy

Update `icn-core/src/supervisor.rs`:

```rust
pub struct ActorSpec {
    pub name: &'static str,
    pub start: fn(SupervisorHandle) -> Pin<Box<dyn Future<Output = Result<()>> + Send>>,
    pub restart_on_fail: bool,
    pub restart_policy: RestartPolicy,
}

pub enum RestartPolicy {
    /// Never restart
    Never,

    /// Restart immediately
    Immediate,

    /// Restart with exponential backoff
    Backoff {
        initial_delay: Duration,
        max_delay: Duration,
        max_restarts: usize,
        window_secs: u64,
    },
}

impl Default for RestartPolicy {
    fn default() -> Self {
        RestartPolicy::Backoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            max_restarts: 5,
            window_secs: 300, // 5 minute window
        }
    }
}

struct ActorState {
    spec: ActorSpec,
    handle: Option<JoinHandle<()>>,
    failure_count: usize,
    last_failure: Option<Instant>,
    restart_attempts: Vec<Instant>,
}

impl Supervisor {
    async fn monitor_actor(&mut self, name: &'static str, mut state: ActorState) {
        loop {
            let handle = match state.handle.take() {
                Some(h) => h,
                None => {
                    // Spawn actor
                    let handle = tokio::spawn((state.spec.start)(self.handle.clone()));
                    state.handle = Some(handle);
                    state.handle.take().unwrap()
                }
            };

            // Wait for actor to exit
            match handle.await {
                Ok(Ok(())) => {
                    // Clean exit
                    info!("Actor {} exited cleanly", name);
                    break;
                }
                Ok(Err(e)) => {
                    // Actor returned error
                    error!("Actor {} failed: {}", name, e);
                    state.failure_count += 1;
                    state.last_failure = Some(Instant::now());
                }
                Err(e) => {
                    // Actor panicked
                    error!("Actor {} panicked: {}", name, e);
                    state.failure_count += 1;
                    state.last_failure = Some(Instant::now());
                }
            }

            // Check restart policy
            if !state.spec.restart_on_fail {
                error!("Actor {} failed and restart disabled", name);
                break;
            }

            // Check if flapping
            if self.is_flapping(&state) {
                error!("Actor {} is flapping, giving up", name);
                icn_obs::metrics::supervisor::actor_flapping_inc(name);
                break;
            }

            // Apply restart policy
            match state.spec.restart_policy {
                RestartPolicy::Never => break,
                RestartPolicy::Immediate => {
                    warn!("Restarting actor {} immediately", name);
                }
                RestartPolicy::Backoff { initial_delay, max_delay, max_restarts, window_secs } => {
                    // Clean old restart attempts
                    let cutoff = Instant::now() - Duration::from_secs(window_secs);
                    state.restart_attempts.retain(|t| *t > cutoff);

                    if state.restart_attempts.len() >= max_restarts {
                        error!("Actor {} exceeded max restarts ({}) in {}s",
                               name, max_restarts, window_secs);
                        icn_obs::metrics::supervisor::actor_restart_limit_inc(name);
                        break;
                    }

                    // Compute backoff delay
                    let attempt = state.restart_attempts.len();
                    let delay = initial_delay * 2u32.pow(attempt as u32);
                    let delay = delay.min(max_delay);

                    warn!("Restarting actor {} in {:?} (attempt {})", name, delay, attempt + 1);
                    tokio::time::sleep(delay).await;

                    state.restart_attempts.push(Instant::now());
                }
            }

            icn_obs::metrics::supervisor::actor_restarts_inc(name);
        }

        // Actor gave up
        self.failed_actors.insert(name);
    }

    fn is_flapping(&self, state: &ActorState) -> bool {
        // Flapping = multiple failures in short time
        if state.restart_attempts.len() < 3 {
            return false;
        }

        let recent = &state.restart_attempts[state.restart_attempts.len() - 3..];
        let time_span = recent.last().unwrap().duration_since(*recent.first().unwrap());

        time_span < Duration::from_secs(10) // 3 restarts in 10 seconds
    }
}
```

#### 7.2 Actor Specifications

Define restart policies for each actor:

```rust
const ACTOR_SPECS: &[ActorSpec] = &[
    ActorSpec {
        name: "network",
        start: start_network_actor,
        restart_on_fail: true,
        restart_policy: RestartPolicy::Backoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_restarts: 10, // Network is critical
            window_secs: 300,
        },
    },
    ActorSpec {
        name: "gossip",
        start: start_gossip_actor,
        restart_on_fail: true,
        restart_policy: RestartPolicy::Backoff {
            initial_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
            max_restarts: 5,
            window_secs: 300,
        },
    },
    ActorSpec {
        name: "ledger",
        start: start_ledger_actor,
        restart_on_fail: true,
        restart_policy: RestartPolicy::Backoff {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_restarts: 10, // Ledger is critical
            window_secs: 300,
        },
    },
    // ... other actors
];
```

#### 7.3 Metrics

Add supervisor metrics:

```rust
// In icn-obs/src/metrics/supervisor.rs

pub fn actor_restarts_inc(actor: &str);
pub fn actor_flapping_inc(actor: &str);
pub fn actor_restart_limit_inc(actor: &str);
pub fn actor_status_set(actor: &str, status: &str); // "running", "failed", "restarting"
```

#### 7.4 Health Endpoint

Expose actor status via RPC:

```rust
// In icn-rpc/src/server.rs

pub async fn get_system_health() -> Result<SystemHealth> {
    let supervisor_status = get_supervisor_status().await?;

    Ok(SystemHealth {
        actors: supervisor_status.actors.iter().map(|(name, state)| {
            ActorHealth {
                name: name.to_string(),
                status: state.status.clone(),
                failure_count: state.failure_count,
                last_failure: state.last_failure.map(|t| t.elapsed().as_secs()),
            }
        }).collect(),
        overall_status: compute_overall_status(&supervisor_status),
    })
}
```

### Testing

Create `icn-core/tests/supervisor_restart.rs`:

```rust
#[tokio::test]
async fn test_actor_restart() {
    // Create supervisor
    // Register actor that fails after 1 second
    // Verify it restarts
}

#[tokio::test]
async fn test_restart_limit() {
    // Create actor that always fails
    // Verify it's shut down after max_restarts
}

#[tokio::test]
async fn test_flap_detection() {
    // Create actor that fails rapidly
    // Verify flap detection and shutdown
}
```

## Implementation Roadmap

### Phase 1: Network Hardening (Week 1-2)
**Goal:** Prevent DoS and establish identity binding

1. **Connection Flooding** (#1)
   - [ ] Implement PeerRegistry
   - [ ] Add connection limits
   - [ ] Implement eviction policy
   - [ ] Add per-peer rate limiting
   - [ ] Write tests

2. **DID-TLS Binding** (#2)
   - [ ] Create IdentityBundle
   - [ ] Implement cert generation
   - [ ] Add HelloMessage protocol
   - [ ] Update connection handshake
   - [ ] Write tests

**Deliverable:** Nodes reject connection floods and cryptographically verify peer identities

### Phase 2: Message Integrity (Week 3)
**Goal:** Signed messages with replay protection

3. **Message Verification** (#3)
   - [ ] Implement SignedEnvelope
   - [ ] Add ReplayGuard
   - [ ] Update NetworkActor
   - [ ] Convert actors to typed messages
   - [ ] Write tests

**Deliverable:** All inter-node messages are signed and replay-protected

### Phase 3: Trust & Identity (Week 4)
**Goal:** Bounded trust computation and key rotation

4. **Trust Hardening** (#4)
   - [ ] Add trust edge metadata
   - [ ] Implement bounded computation
   - [ ] Add participation hooks
   - [ ] Write tests

5. **Key Rotation** (#5)
   - [ ] Implement KeyRotationEvent
   - [ ] Update keystore
   - [ ] Add trust graph aliases
   - [ ] Add icnctl commands
   - [ ] Write tests

**Deliverable:** Trust system with decay/bounds, key rotation support

### Phase 4: Ledger & Resilience (Week 5)
**Goal:** Ledger sync and failure recovery

6. **Ledger Gossip** (#6)
   - [ ] Add LedgerGossipMsg types
   - [ ] Implement anti-entropy
   - [ ] Add fork detection
   - [ ] Write tests

7. **Supervisor Restart** (#7)
   - [ ] Implement restart policies
   - [ ] Add flap detection
   - [ ] Add health endpoint
   - [ ] Write tests

**Deliverable:** Ledger sync over gossip, automatic actor restart

## Testing Strategy

### Unit Tests
- Each component tested in isolation
- Mock dependencies
- Focus on correctness of algorithms

### Integration Tests
- Multi-node scenarios
- Simulated attack scenarios
- Performance under load

### Security Tests
- Fuzz testing for message parsing
- Replay attack scenarios
- Connection flood scenarios
- Fork/conflict scenarios

### Regression Tests
- Ensure new security doesn't break existing features
- Run full test suite on each commit

## Metrics & Monitoring

### Security Metrics

#### Network
- `icn_network_connections_rejected_total` - Connection rejections
- `icn_network_connections_active` - Current active peers
- `icn_network_peer_rate_limited_total` - Rate limiting events
- `icn_network_peer_evicted_total{reason}` - Peer evictions

#### Identity
- `icn_identity_verification_failures_total` - DID-TLS binding failures
- `icn_identity_rotations_total` - Key rotations performed

#### Messages
- `icn_messages_signature_failures_total` - Signature verification failures
- `icn_messages_replay_detected_total` - Replay attempts
- `icn_messages_malformed_total` - Malformed messages

#### Ledger
- `icn_ledger_fork_detected_total` - Fork detections
- `icn_ledger_causal_anomaly_total` - Causal violations
- `icn_ledger_entries_quarantined_total` - Quarantined entries

#### Supervisor
- `icn_supervisor_actor_restarts_total{actor}` - Actor restarts
- `icn_supervisor_actor_flapping_total{actor}` - Flapping detections

### Alerts

**Critical:**
- Fork detected in own ledger
- DID-TLS verification failing for >50% of connections
- Multiple actors flapping

**Warning:**
- Connection rejection rate >10/sec
- Replay detection rate >1/min
- Actor restart rate >1/hour

## Security Assumptions

### What This Design Provides
✅ Connection DoS prevention
✅ Cryptographic identity binding
✅ Message integrity and authenticity
✅ Replay protection
✅ Bounded trust computation
✅ Key rotation capability
✅ Basic fork detection

### What This Design Does NOT Provide
❌ Byzantine fault tolerance
❌ Sybil attack prevention (requires external identity)
❌ Perfect conflict resolution (detects but doesn't resolve)
❌ Protection against compromised root keys
❌ Traffic analysis resistance
❌ Perfect forward secrecy (future work)

## References

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [production-hardening.md](production-hardening.md) - Production hardening measures
- [CLAUDE.md](../CLAUDE.md) - Development guidance
