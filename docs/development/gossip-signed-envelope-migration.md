# Gossip Message Migration to SignedEnvelope - Analysis Report

## Executive Summary

The ICN gossip system currently sends GossipMessage types (Announce, Request, Response, etc.) wrapped in NetworkMessage envelopes over QUIC. To add cryptographic authentication and replay protection, we need to migrate gossip messages to use SignedEnvelope, which adds:

- **64-byte Ed25519 signature** per message
- **~40-byte envelope overhead** (from, sequence, timestamp, payload_type)
- **Sequence tracking** per sender for replay detection
- **Timestamp-based freshness** checks (300s max age)

This analysis identifies the current architecture, serialization approach, and migration challenges.

---

## 1. GossipMessage Type Definition

**Location:** `/home/matt/projects/icn/icn/crates/icn-gossip/src/types.rs` (lines 111-164)

### Message Variants

```rust
pub enum GossipMessage {
    // Push announcement
    Announce {
        hash: ContentHash,        // 32 bytes
        author: Did,              // ~60 bytes (did:icn:...)
        clock: VectorClock,       // ~100 bytes (HashMap<Did, u64>)
        topic: String,            // variable
    },

    // Pull request
    Request { hash: ContentHash },

    // Full entry response
    Response { entry: GossipEntry },

    // Anti-entropy: request Bloom filter
    RequestBloomFilter { topic: String },

    // Anti-entropy: send Bloom filter
    SendBloomFilter {
        topic: String,
        filter: BloomFilterData,  // ~10KB for 10K entries
    },

    // Request missing entries by hash
    RequestMissing { hashes: Vec<ContentHash> },

    // Enhanced anti-entropy: digest with Bloom + vector clock
    Digest {
        topic: String,
        vector: VectorClock,
        bloom: BloomFilterData,
        hint_count: u32,
        nonce: u64,
    },

    // Targeted pull request
    PullRequest {
        topic: String,
        want_ids: Vec<ContentHash>,
        max_bytes: u32,
        nonce: u64,
    },

    // Pull response (may be truncated)
    PullResponse {
        topic: String,
        entries: Vec<GossipEntry>,
        truncated: bool,
        nonce: u64,
    },
}
```

### GossipEntry Structure

```rust
pub struct GossipEntry {
    pub hash: ContentHash,           // 32 bytes
    pub author: Did,                 // ~60 bytes
    pub clock: VectorClock,          // ~100 bytes
    pub topic: String,               // variable
    pub data: Vec<u8>,               // variable (compressed if > 1KB)
    pub compressed: bool,            // 1 byte
    pub timestamp: u64,              // 8 bytes
}
```

---

## 2. Current Serialization Format

**Location:** `/home/matt/projects/icn/icn/crates/icn-net/src/protocol.rs` (lines 146-172)

### Wire Format

**Serialization Method:** `bincode` (binary encoding)

```rust
impl NetworkMessage {
    /// Serialize to bytes using bincode
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let bytes = bincode::serialize(self)?;
        if bytes.len() > MAX_MESSAGE_SIZE {
            bail!("Message too large: {} bytes (max {})", bytes.len(), 10MB);
        }
        Ok(bytes)
    }

    /// Deserialize from bytes using bincode
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        bincode::deserialize(bytes)?
    }
}
```

### Payload Structure

```rust
pub enum MessagePayload {
    Gossip(GossipMessage),     // Currently unencrypted
    Ping,
    Pong,
    Subscribe { topics: Vec<String> },
    Unsubscribe { topics: Vec<String> },
    SubscribeAck { topics: Vec<String> },
    Hello { binding_info: BindingInfo, topology_info: Option<TopologyInfo> },
    Handshake { region, cluster_id, role },
    HandshakeAck,
    Signed(SignedEnvelope),    // For authenticated messages
}
```

**Encoding Overview:**
- NetworkMessage frame serialized with bincode
- GossipMessage serialized within that frame
- No per-message authentication or sequence tracking yet

---

## 3. Current Message Flow: Send/Receive

### Send Path

**Location:** `/home/matt/projects/icn/icn/crates/icn-core/src/supervisor.rs` (lines 314-343)

```
GossipActor.publish() 
    → send_message(recipient, GossipMessage)
    → SendMessageCallback closure
    → NetworkMessage::gossip(from, to, gossip_msg)
    → network_handle.send_message(target, net_msg)
    → QUIC/TLS transmission
```

**The Send Callback** (set at lines 314-343):

```rust
let send_callback: icn_gossip::SendMessageCallback = 
    Arc::new(move |recipient, gossip_msg| {
        // Track metrics
        match &gossip_msg {
            GossipMessage::Announce { .. } => announces_sent_inc(),
            GossipMessage::Request { .. } => requests_sent_inc(),
            GossipMessage::Response { .. } => responses_sent_inc(),
            _ => {}
        }

        // Spawn async task
        tokio::spawn(async move {
            let net_msg = if let Some(target_did) = recipient {
                // Unicast
                NetworkMessage::gossip(from_did, Some(target_did), gossip_msg)
            } else {
                // Broadcast
                NetworkMessage::gossip(from_did, None, gossip_msg)
            };

            net_handle.send_message(target_did, net_msg).await
        });
    });

gossip.set_send_callback(send_callback);
```

**Key Observations:**
- SendMessageCallback signature: `Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>`
- Messages are wrapped in NetworkMessage immediately before sending
- No sequence tracking per sender
- No signature generation

### Receive Path

**Location:** `/home/matt/projects/icn/icn/crates/icn-core/src/supervisor.rs` (lines 160-174)

```
NetworkActor receives bytes via QUIC
    → NetworkMessage::from_bytes()
    → incoming_handler callback
    → MessagePayload::Gossip(gossip_msg)
    → gossip_handle.write()
    → gossip.handle_message(sender_did, gossip_msg)
```

**The Incoming Handler** (lines 164-174):

```rust
match net_msg.payload {
    icn_net::MessagePayload::Gossip(gossip_msg) => {
        let gossip_handle = gossip_handle_clone.clone();
        let sender = sender_did.clone();
        
        tokio::spawn(async move {
            let mut gossip = gossip_handle.write().await;
            if let Err(e) = gossip.handle_message(&sender, gossip_msg) {
                warn!("Failed to handle gossip message: {}", e);
            }
        });
    }
    // ... other payload types
    icn_net::MessagePayload::Signed(ref envelope) => {
        info!("Received verified signed message from {} (seq={})", 
              envelope.from, envelope.sequence);
        // Currently just logged - no payload handling yet
    }
}
```

**Key Observations:**
- Messages arrive with `from` field from NetworkMessage (NOT authenticated)
- No replay protection
- No signature verification
- Signed messages exist but are not yet used for gossip

---

## 4. Message Size Estimation

### Typical Message Sizes (Bincode Encoded)

**Announce Message:**
```
hash:       32 bytes
author:     ~60 bytes (DID serialization)
clock:      ~100 bytes (VectorClock with map entries)
topic:      variable (e.g., "global:identity" = 16 bytes)
────────────────────────
Total:      ~210 bytes + topic length
Typical:    ~230 bytes
```

**Request Message:**
```
hash:       32 bytes
────────────
Total:      32 bytes
```

**Response Message (small):**
```
entry.hash:       32 bytes
entry.author:     ~60 bytes
entry.clock:      ~100 bytes
entry.topic:      variable
entry.data:       variable (usually >1KB, compressed)
entry.timestamp:  8 bytes
entry.compressed: 1 byte
────────────────────────
Small (512B data): ~210 bytes + data
Medium (2KB data): ~250 bytes + data
Large (10KB data): ~300 bytes + compressed_data (~500-1000B after zstd)

Typical range: 300 bytes - 2KB
```

**Digest Message:**
```
topic:      variable
vector:     ~100 bytes
bloom:      ~10KB (for 10K entries)
hint_count: 4 bytes
nonce:      8 bytes
────────────
Typical:    ~10KB
```

**PullResponse Message:**
```
Per entry: 300-2000 bytes (as above)
For N entries: N * (300-2000)
Typical N=10: 3-20KB
```

### NetworkMessage Wrapper Overhead

```
version:    4 bytes (u32)
from:       ~60 bytes (Did)
to:         ~60 bytes (Option<Did>)
payload:    bincode variant tag (1-2 bytes) + payload bytes
────────────────────────
Overhead:   ~125 bytes
```

### SignedEnvelope Overhead

```
from:               ~60 bytes (Did)
sequence:           8 bytes (u64)
timestamp:          8 bytes (u64)
payload_type:       1 byte (u8)
payload:            variable (original message bytes)
signature:          64 bytes (Ed25519)
────────────────────────
Envelope overhead:  141 bytes + payload
```

**Size Impact Examples:**
- Announce: 230 → 230 + 141 = 371 bytes (+61%)
- Request: 32 → 32 + 141 = 173 bytes (+441%, but absolute impact small)
- Small Response: 300 → 300 + 141 = 441 bytes (+47%)
- Large Response: 2KB → 2000 + 141 = 2141 bytes (+7%)

---

## 5. SignedEnvelope Implementation (Existing)

**Location:** `/home/matt/projects/icn/icn/crates/icn-net/src/envelope.rs`

### SignedEnvelope Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEnvelope {
    pub from: Did,                      // Authenticated sender
    pub sequence: u64,                  // Monotonic per-sender
    pub timestamp: u64,                 // Milliseconds since Unix epoch
    pub payload_type: PayloadType,      // Discriminator (Gossip, Ledger, etc.)
    pub payload: Vec<u8>,               // Serialized payload (bincode)
    pub signature: Vec<u8>,             // Ed25519 signature (64 bytes)
}

pub enum PayloadType {
    Gossip = 1,
    Ledger = 2,
    Trust = 3,
    Contract = 4,
    Rpc = 5,
    Control = 6,
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

        let sig_input = envelope.canonical_encoding();
        envelope.signature = keypair.sign(&sig_input).to_vec();
        Ok(envelope)
    }

    /// Verify signature and age
    pub fn verify(&self, max_age_secs: u64) -> Result<()> {
        // 1. Verify Ed25519 signature
        let sig_input = self.canonical_encoding();
        let verifying_key = self.from.to_verifying_key()?;
        let signature = ed25519_dalek::Signature::from_slice(&self.signature)?;
        verifying_key.verify(&sig_input, &signature)?;

        // 2. Verify age (default: 300s clock skew allowed)
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
        let age_ms = now.saturating_sub(self.timestamp);
        let max_age_ms = max_age_secs * 1000;
        
        if age_ms > max_age_ms {
            bail!("Message too old: {}ms (max {}ms)", age_ms, max_age_ms);
        }
        if self.timestamp > now + max_age_ms {
            bail!("Message from future (clock skew)");
        }

        Ok(())
    }

    /// Canonical encoding for signatures
    fn canonical_encoding(&self) -> Vec<u8> {
        // sequence (8 BE) || timestamp (8 BE) || payload_type (1) || payload
        let mut buf = Vec::with_capacity(17 + self.payload.len());
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.push(self.payload_type as u8);
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Helper to serialize and create envelope
    pub fn from_payload<T: serde::Serialize>(
        from: &Did,
        keypair: &KeyPair,
        sequence: u64,
        payload_type: PayloadType,
        payload: &T,
    ) -> Result<Self> {
        let payload_bytes = bincode::serialize(payload)?;
        Self::new(from, keypair, sequence, payload_type, payload_bytes)
    }
}
```

### Verification Integration in NetworkActor

The NetworkActor already verifies SignedEnvelopes when received:

```rust
// In incoming_handler (supervisor.rs):
icn_net::MessagePayload::Signed(ref envelope) => {
    info!("Received verified signed message from {} (seq={})", 
          envelope.from, envelope.sequence);
    // Verification already done by NetworkActor
}
```

---

## 6. ReplayGuard (Existing)

**Location:** `/home/matt/projects/icn/icn/crates/icn-net/src/replay_guard.rs`

### How It Works

```rust
pub struct ReplayGuard {
    sequences: HashMap<Did, SequenceWindow>,
    max_clock_skew: u64,
    max_peer_age_secs: u64,
}

struct SequenceWindow {
    max_seq: u64,                          // Highest sequence seen
    recent: BloomFilter,                   // ~10KB for 10K sequences
    last_update: Instant,
}

impl ReplayGuard {
    pub fn check(&mut self, envelope: &SignedEnvelope) -> Result<()> {
        // 1. Verify signature and age
        envelope.verify(self.max_clock_skew)?;

        // 2. Get or create sequence window for sender
        let window = self.sequences
            .entry(envelope.from.clone())
            .or_insert_with(SequenceWindow::new);

        // 3. Check sequence number
        if envelope.sequence <= window.max_seq {
            // Out-of-order or replay
            if window.recent.contains(&hash_sequence(envelope.sequence)) {
                bail!("Replay detected: seq {} already seen", envelope.sequence);
            }
            // Not in filter: accept as out-of-order
        } else {
            // In-order: update max_seq
            window.max_seq = envelope.sequence;
        }

        // 4. Update Bloom filter and timestamp
        window.recent.insert(&hash_sequence(envelope.sequence));
        window.last_update = Instant::now();

        Ok(())
    }
}
```

---

## 7. Current Message Flow Architecture

### Supervisor Setup (Key Points)

1. **GossipActor Creation** (line 110):
   - Created with trust lookup for access control
   - No sequence counter tracking yet

2. **Send Callback Setup** (lines 314-343):
   - Wraps GossipMessage in NetworkMessage::gossip()
   - No signing happens
   - Async spawn for network send

3. **Receive Handler Setup** (lines 160-174):
   - Unwraps MessagePayload::Gossip
   - Passes to gossip.handle_message()
   - No signature verification for gossip

4. **SignedEnvelope Support** (line 265-273):
   - Receives and logs verified signed messages
   - But doesn't route payload to gossip yet
   - Payload type discriminator exists but unused

---

## 8. Migration Challenges and Requirements

### Challenge 1: Sequence Number Management

**Problem:** GossipActor needs to track outgoing sequence numbers per peer (or globally).

**Current State:** No sequence tracking in GossipActor.

**Solution Required:**
- Add `sequence_counter: u64` to GossipActor
- Increment before sending each message
- Store in a persistent KV store (Sled) for restart recovery

### Challenge 2: SendMessageCallback Signature Change

**Current:**
```rust
pub type SendMessageCallback = Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>;
```

**Needed for Signed Messages:**
```rust
pub type SendMessageCallback = Arc<dyn Fn(Option<Did>, GossipMessage) + Send + Sync>;
// But the callback needs to sign and wrap in SignedEnvelope
```

**Options:**
A. Keep callback signature, add signing logic inside callback (requires keypair)
B. Change callback to return Result, let GossipActor handle signing
C. Add second callback for signed sends

**Best Option:** Approach C - Keep existing callback, add new signing flow that:
1. GossipActor has access to keypair (from IdentityBundle)
2. Callback receives already-signed NetworkMessage::Signed(envelope)
3. Or: GossipActor constructs SignedEnvelope internally

### Challenge 3: Backward Compatibility

**Current:** Gossip messages sent as `MessagePayload::Gossip(GossipMessage)`

**New:** Gossip messages sent as `MessagePayload::Signed(SignedEnvelope)` with PayloadType::Gossip

**Migration Strategy:**
- Phase 1: Accept both formats on receive
- Phase 2: Send all new messages as Signed
- Phase 3: Deprecate raw Gossip messages (after timeout)

### Challenge 4: Deserialization in Handler

**Current Receive Path:**
```rust
MessagePayload::Gossip(gossip_msg) => {
    gossip.handle_message(&sender_did, gossip_msg)?
}
```

**New Receive Path:**
```rust
MessagePayload::Signed(envelope) => {
    // Verify signature (already done by NetworkActor)
    if envelope.payload_type == PayloadType::Gossip {
        let gossip_msg: GossipMessage = envelope.decode_payload()?;
        gossip.handle_message(&envelope.from, gossip_msg)?
    }
}
```

**Key Issue:** sender_did comes from envelope.from (authenticated), not net_msg.from (untrusted)

### Challenge 5: Metrics Tracking

**Current:** Metrics tracked in send_callback before sending

**New:** Need to track in two places:
- After signing (for signed messages)
- Or defer to handle_message (for consistency)

### Challenge 6: Gossip Actor IdentityBundle Access

**Problem:** GossipActor doesn't currently have access to the keypair for signing.

**Current Flow:**
```
GossipActor.publish() 
    → send_callback
    → (callback wraps and sends - but can't sign)
```

**Solution Required:**
Either:
A. Pass IdentityBundle (or just keypair) to GossipActor at creation
B. Change sender to sign gossip messages (requires supervisor to handle)
C. Have GossipActor return unsigned messages, let supervisor sign

---

## 9. Test Coverage Observation

**Location:** `icn/crates/icn-gossip/src/gossip.rs` (integration tests)

Current test pattern:
```rust
#[test]
fn test_announce_and_request_response() {
    let sent_messages = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sent_messages_clone = sent_messages.clone();

    // Set callback to capture messages
    gossip2.set_send_callback(Arc::new(move |recipient, msg| {
        sent_messages_clone.lock().unwrap().push((recipient, msg));
    }));

    // Verify messages sent
    let messages = sent_messages.lock().unwrap();
    assert_eq!(messages.len(), 1);
    if let (Some(recipient), GossipMessage::Request { hash: req_hash }) = &messages[0] {
        assert_eq!(recipient, &did1);
    }
}
```

**Migration Impact:** Tests must be updated to expect SignedEnvelope in callback messages, or callback must transparently handle signing.

---

## 10. Summary Table

| Aspect | Current | With SignedEnvelope |
|--------|---------|-------------------|
| **Serialization** | bincode | bincode (inside envelope) |
| **Payload Type** | MessagePayload::Gossip | MessagePayload::Signed with PayloadType::Gossip |
| **Authentication** | None (trusts from field) | Ed25519 signature + verification |
| **Replay Protection** | None | ReplayGuard with sequence tracking |
| **Message Size Overhead** | ~125 bytes (NetworkMessage) | ~141 bytes (envelope) |
| **Send Path** | gossip.publish() → callback → network | gossip.publish() → sign → callback → network |
| **Receive Path** | network → unwrap Gossip → handle | network → verify → unwrap Gossip → handle |
| **Sequence Tracking** | None | Per-sender via ReplayGuard |
| **Metrics** | 3 message types tracked | Need to update for signed variants |

---

## Key Files Involved in Migration

### Files to Modify

1. **icn-gossip/src/gossip.rs**
   - Add IdentityBundle/keypair field
   - Add sequence counter
   - Modify send_message() to accept signed envelopes
   - Update tests

2. **icn-gossip/src/types.rs**
   - SendMessageCallback signature (or add new callback type)

3. **icn-core/src/supervisor.rs**
   - Pass IdentityBundle to GossipActor
   - Update send_callback to handle signed messages
   - Update incoming_handler to route Signed messages with PayloadType::Gossip

4. **icn-net/src/protocol.rs**
   - Add NetworkMessage::signed_gossip() helper

5. **icn-net/src/actor.rs**
   - Integrate ReplayGuard verification for gossip messages

### Files Already Supporting This

1. **icn-net/src/envelope.rs**
   - SignedEnvelope structure and signing logic (complete)

2. **icn-net/src/replay_guard.rs**
   - ReplayGuard implementation (complete)

3. **icn-net/src/protocol.rs**
   - MessagePayload::Signed variant (exists)
   - PayloadType::Gossip discriminator (exists)

---

## Estimated Effort

- **Sequence tracking**: 2-3 hours
- **IdentityBundle integration into GossipActor**: 1-2 hours
- **Send path changes**: 2-3 hours
- **Receive path integration**: 2-3 hours
- **Test updates**: 2-3 hours
- **Metrics updates**: 1 hour
- **Backward compatibility (if needed)**: 2-3 hours
- **Integration testing**: 3-4 hours

**Total: ~16-24 hours of focused development**

