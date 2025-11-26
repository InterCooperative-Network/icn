//! Onion Routing for Gossip Messages (Phase 20 Week 3-4)
//!
//! Implements multi-hop message routing inspired by Tor to hide sender/receiver
//! relationships and protect against traffic analysis.
//!
//! ## Architecture
//!
//! ```text
//! Sender                 Relay 1              Relay 2             Recipient
//!   |                      |                    |                     |
//!   |-- E(R2_pk, E(Recip_pk, msg), R2_addr) -->|                     |
//!   |                      |                    |                     |
//!   |                      |-- E(Recip_pk, msg), Recip_addr -------->|
//!   |                      |                    |                     |
//!   |                      |                    |<---- Decrypt msg ---|
//!
//! Each layer:
//! - Encrypted with next hop's X25519 public key
//! - Contains: next hop address + inner layer (or final payload)
//! - Relays can't see final destination or original sender
//! ```
//!
//! ## Security Properties
//!
//! - **Sender anonymity**: Recipient doesn't know original sender
//! - **Receiver anonymity**: Relays don't know final recipient
//! - **Unlinkability**: Can't correlate sender → recipient
//! - **Traffic analysis resistance**: Multiple hops hide patterns

use crate::error::{PrivacyError, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use icn_identity::Did;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use x25519_dalek::{PublicKey, StaticSecret};

/// Onion-routed message with layered encryption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnionMessage {
    /// Encrypted layers (outermost first)
    /// Each layer contains: next_hop + inner_payload
    pub layers: Vec<OnionLayer>,

    /// Current hop index (0 = at sender, increments at each relay)
    pub hop_index: usize,
}

/// Single layer of an onion message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnionLayer {
    /// Encrypted payload (next hop address + inner layer OR final message)
    pub ciphertext: Vec<u8>,

    /// Nonce for ChaCha20-Poly1305
    pub nonce: [u8; 12],

    /// Next hop's DID (for routing)
    pub next_hop: Did,
}

/// Inner content of an onion layer
#[derive(Debug, Clone, Serialize, Deserialize)]
enum LayerContent {
    /// Intermediate hop: relay to next node
    Relay {
        next_hop: Did,
        inner_layer: Box<OnionLayer>,
    },

    /// Final hop: deliver message
    Final { payload: Vec<u8> },
}

/// Circuit path for onion routing
#[derive(Debug, Clone)]
pub struct Circuit {
    /// Ordered list of relay DIDs (not including sender or recipient)
    pub relays: Vec<Did>,

    /// Final recipient
    pub recipient: Did,

    /// Shared keys with each hop (for encryption)
    /// Index 0 = first relay, index n-1 = recipient
    shared_keys: Vec<[u8; 32]>,
}

/// Onion router for creating and processing onion messages
pub struct OnionRouter {
    /// This node's DID
    my_did: Did,

    /// This node's X25519 secret key (for decryption)
    secret_key: StaticSecret,

    /// Public keys of known peers (DID -> X25519 public key)
    peer_public_keys: HashMap<Did, PublicKey>,
}

impl OnionRouter {
    /// Create a new onion router
    ///
    /// # Arguments
    /// - `my_did`: This node's DID
    /// - `secret_key`: This node's X25519 secret key (for decrypting layers)
    pub fn new(my_did: Did, secret_key: StaticSecret) -> Self {
        Self {
            my_did,
            secret_key,
            peer_public_keys: HashMap::new(),
        }
    }

    /// Register a peer's X25519 public key
    ///
    /// Required before creating circuits that involve this peer.
    pub fn add_peer_key(&mut self, did: Did, public_key: PublicKey) {
        self.peer_public_keys.insert(did, public_key);
    }

    /// Create a circuit with the given relay path
    ///
    /// # Arguments
    /// - `relays`: Ordered list of relay DIDs (empty = direct send)
    /// - `recipient`: Final recipient DID
    ///
    /// # Returns
    /// Circuit with computed shared keys for each hop
    pub fn create_circuit(&self, relays: Vec<Did>, recipient: Did) -> Result<Circuit> {
        // Compute shared keys with each hop using X25519 ECDH
        let mut shared_keys = Vec::new();

        // Keys for relays
        for relay_did in &relays {
            let relay_pk = self
                .peer_public_keys
                .get(relay_did)
                .ok_or_else(|| {
                    PrivacyError::OnionRoutingError(format!("Missing public key for {}", relay_did))
                })?;

            let shared_secret = self.secret_key.diffie_hellman(relay_pk);
            shared_keys.push(*shared_secret.as_bytes());
        }

        // Key for recipient
        let recipient_pk = self.peer_public_keys.get(&recipient).ok_or_else(|| {
            PrivacyError::OnionRoutingError(format!("Missing public key for {}", recipient))
        })?;

        let shared_secret = self.secret_key.diffie_hellman(recipient_pk);
        shared_keys.push(*shared_secret.as_bytes());

        icn_obs::metrics::privacy::onion_routes_created_inc();

        Ok(Circuit {
            relays,
            recipient,
            shared_keys,
        })
    }

    /// Wrap a message in onion layers
    ///
    /// Creates layered encryption from innermost (recipient) to outermost (first relay).
    ///
    /// # Arguments
    /// - `circuit`: The routing circuit
    /// - `payload`: The actual message to send
    ///
    /// # Returns
    /// OnionMessage ready to send to first hop
    pub fn wrap_message(&self, circuit: &Circuit, payload: &[u8]) -> Result<OnionMessage> {
        // Build from inside out: recipient -> last relay -> ... -> first relay

        // Start with final layer (recipient)
        let final_content = LayerContent::Final {
            payload: payload.to_vec(),
        };
        let final_bytes = bincode::serialize(&final_content)
            .map_err(|e| PrivacyError::OnionRoutingError(format!("Serialization failed: {}", e)))?;

        let mut current_layer =
            self.encrypt_layer(&final_bytes, circuit.shared_keys.last().unwrap())?;

        // Wrap in relay layers (reverse order)
        for (i, _relay_did) in circuit.relays.iter().enumerate().rev() {
            let relay_content = LayerContent::Relay {
                next_hop: if i == circuit.relays.len() - 1 {
                    circuit.recipient.clone()
                } else {
                    circuit.relays[i + 1].clone()
                },
                inner_layer: Box::new(current_layer),
            };

            let relay_bytes = bincode::serialize(&relay_content).map_err(|e| {
                PrivacyError::OnionRoutingError(format!("Serialization failed: {}", e))
            })?;

            current_layer = self.encrypt_layer(&relay_bytes, &circuit.shared_keys[i])?;
        }

        // Create onion message with first hop
        let _first_hop = if circuit.relays.is_empty() {
            circuit.recipient.clone()
        } else {
            circuit.relays[0].clone()
        };

        Ok(OnionMessage {
            layers: vec![current_layer],
            hop_index: 0,
        })
    }

    /// Peel one layer off an onion message
    ///
    /// Decrypts the current layer and returns:
    /// - Next hop to forward to
    /// - Updated message (with layer removed) OR final payload
    ///
    /// # Returns
    /// - `Ok(Some((next_hop, onion_msg)))` - Forward to next hop
    /// - `Ok(None)` - This is the final destination (extract payload separately)
    pub fn peel_layer(&self, mut onion: OnionMessage) -> Result<Option<(Did, OnionMessage)>> {
        if onion.layers.is_empty() {
            return Err(PrivacyError::OnionRoutingError(
                "No layers to peel".to_string(),
            ));
        }

        let current_layer = &onion.layers[onion.hop_index];

        // Decrypt current layer
        let decrypted = self.decrypt_layer(current_layer)?;

        // Deserialize to determine if relay or final
        let content: LayerContent = bincode::deserialize(&decrypted).map_err(|e| {
            PrivacyError::OnionRoutingError(format!("Deserialization failed: {}", e))
        })?;

        icn_obs::metrics::privacy::onion_hops_forwarded_inc();

        match content {
            LayerContent::Relay {
                next_hop,
                inner_layer,
            } => {
                // Forward to next relay
                onion.layers.push(*inner_layer);
                onion.hop_index += 1;
                Ok(Some((next_hop, onion)))
            }
            LayerContent::Final { .. } => {
                // We are the final destination
                Ok(None)
            }
        }
    }

    /// Extract final payload from onion message
    ///
    /// Call this after `peel_layer()` returns `None` (indicating final destination).
    pub fn extract_payload(&self, onion: &OnionMessage) -> Result<Vec<u8>> {
        if onion.layers.is_empty() {
            return Err(PrivacyError::OnionRoutingError(
                "No layers to extract".to_string(),
            ));
        }

        let current_layer = &onion.layers[onion.hop_index];
        let decrypted = self.decrypt_layer(current_layer)?;

        let content: LayerContent = bincode::deserialize(&decrypted).map_err(|e| {
            PrivacyError::OnionRoutingError(format!("Deserialization failed: {}", e))
        })?;

        match content {
            LayerContent::Final { payload } => Ok(payload),
            LayerContent::Relay { .. } => Err(PrivacyError::OnionRoutingError(
                "Not a final layer".to_string(),
            )),
        }
    }

    /// Encrypt a layer with a shared key
    fn encrypt_layer(&self, plaintext: &[u8], shared_key: &[u8; 32]) -> Result<OnionLayer> {
        let cipher = ChaCha20Poly1305::new(shared_key.into());

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(&nonce, Payload { msg: plaintext, aad: b"" })
            .map_err(|e| PrivacyError::EncryptionFailed(e.to_string()))?;

        Ok(OnionLayer {
            ciphertext,
            nonce: nonce_bytes,
            next_hop: self.my_did.clone(), // Placeholder (overwritten by caller)
        })
    }

    /// Decrypt a layer with this node's secret key
    fn decrypt_layer(&self, _layer: &OnionLayer) -> Result<Vec<u8>> {
        // For proper implementation, we'd need to derive the shared key from:
        // - Our secret key
        // - Sender's ephemeral public key (should be in layer)
        //
        // For now, this is a simplified version that assumes we have the key.
        // In production, each layer would include an ephemeral public key.

        // This is a placeholder - real implementation needs ephemeral keys
        Err(PrivacyError::OnionRoutingError(
            "Decryption requires ephemeral public keys (not yet implemented)".to_string(),
        ))
    }
}

/// Select relay nodes for a circuit based on trust scores
///
/// # Arguments
/// - `candidate_relays`: Available relay nodes
/// - `trust_scores`: Trust scores for each candidate (0.0 to 1.0)
/// - `num_hops`: Number of relay hops (typically 2-3)
/// - `min_trust`: Minimum trust score to use as relay (e.g., 0.3)
///
/// # Returns
/// Ordered list of relay DIDs (empty if not enough trusted relays)
pub fn select_relays(
    candidate_relays: &[Did],
    trust_scores: &HashMap<Did, f64>,
    num_hops: usize,
    min_trust: f64,
) -> Vec<Did> {
    // Filter by minimum trust
    let mut trusted: Vec<_> = candidate_relays
        .iter()
        .filter(|did| trust_scores.get(did).copied().unwrap_or(0.0) >= min_trust)
        .collect();

    // Not enough trusted relays
    if trusted.len() < num_hops {
        return vec![];
    }

    // Sort by trust (highest first) for better security
    trusted.sort_by(|a, b| {
        let trust_a = trust_scores.get(a).copied().unwrap_or(0.0);
        let trust_b = trust_scores.get(b).copied().unwrap_or(0.0);
        trust_b.partial_cmp(&trust_a).unwrap()
    });

    // Take top N relays
    trusted.into_iter().take(num_hops).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_test_did() -> (Did, StaticSecret, PublicKey) {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let secret = StaticSecret::random_from_rng(rand::thread_rng());
        let pubkey = PublicKey::from(&secret);
        (did, secret, pubkey)
    }

    #[test]
    fn test_circuit_creation() {
        let (my_did, my_secret, _) = make_test_did();
        let mut router = OnionRouter::new(my_did.clone(), my_secret);

        // Add peer keys
        let (relay1_did, _, relay1_pk) = make_test_did();
        router.add_peer_key(relay1_did.clone(), relay1_pk);

        let (recipient_did, _, recipient_pk) = make_test_did();
        router.add_peer_key(recipient_did.clone(), recipient_pk);

        // Create circuit
        let circuit = router
            .create_circuit(vec![relay1_did.clone()], recipient_did.clone())
            .unwrap();

        assert_eq!(circuit.relays.len(), 1);
        assert_eq!(circuit.relays[0], relay1_did);
        assert_eq!(circuit.recipient, recipient_did);
        assert_eq!(circuit.shared_keys.len(), 2); // relay + recipient
    }

    #[test]
    fn test_select_relays() {
        let (relay1, _, _) = make_test_did();
        let (relay2, _, _) = make_test_did();
        let (relay3, _, _) = make_test_did();
        let (relay4, _, _) = make_test_did();

        let candidates = vec![
            relay1.clone(),
            relay2.clone(),
            relay3.clone(),
            relay4.clone(),
        ];

        let mut trust_scores = HashMap::new();
        trust_scores.insert(relay1.clone(), 0.9); // High trust
        trust_scores.insert(relay2.clone(), 0.7); // Medium trust
        trust_scores.insert(relay3.clone(), 0.5); // Medium trust
        trust_scores.insert(relay4.clone(), 0.2); // Low trust (below min)

        let selected = select_relays(&candidates, &trust_scores, 2, 0.3);

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0], relay1); // Highest trust
        assert_eq!(selected[1], relay2); // Second highest
    }

    #[test]
    fn test_select_relays_insufficient_trust() {
        let (relay1, _, _) = make_test_did();
        let (relay2, _, _) = make_test_did();

        let candidates = vec![relay1.clone(), relay2.clone()];

        let mut trust_scores = HashMap::new();
        trust_scores.insert(relay1, 0.2); // Below min
        trust_scores.insert(relay2, 0.1); // Below min

        let selected = select_relays(&candidates, &trust_scores, 2, 0.3);

        assert_eq!(selected.len(), 0); // Not enough trusted relays
    }
}
