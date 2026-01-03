//! Encrypted message handler - E2E decryption and forwarding
//!
//! Handles EncryptedEnvelope payloads within SignedEnvelope messages.
//! After successful decryption, the inner payload is unwrapped and forwarded
//! to the standard signed message handler.
//!
//! ## Security Properties
//!
//! - Payload confidentiality: Only intended recipient can read content
//! - Authenticated encryption: Tampering is detected via Poly1305 tag
//! - Replay protection: Inherited from outer SignedEnvelope
//! - Nonce uniqueness: Derived from sequence + DIDs (no transmission overhead)

use super::ConnectionContext;
use crate::encryption::EncryptedEnvelope;
use crate::envelope::{PayloadType, SignedEnvelope};
use crate::protocol::{MessagePayload, NetworkMessage};
use tracing::{debug, warn};

impl ConnectionContext {
    /// Handle decryption of an encrypted payload
    ///
    /// Called after SignedEnvelope verification succeeds for `PayloadType::Encrypted`.
    /// Decrypts the inner EncryptedEnvelope and forwards the decrypted payload
    /// to the standard message handler.
    ///
    /// ## Flow
    ///
    /// 1. Deserialize `EncryptedEnvelope` from signed payload
    /// 2. Verify we are the intended recipient
    /// 3. Get sender's X25519 public key from peer_connections
    /// 4. Decrypt using `EncryptedEnvelope::decrypt()`
    /// 5. Deserialize inner payload (typically another SignedEnvelope or raw data)
    /// 6. Forward decrypted message to handler
    pub async fn handle_encrypted_payload(
        &self,
        message: NetworkMessage,
        envelope: &SignedEnvelope,
    ) {
        // 1. Deserialize EncryptedEnvelope from the signed payload
        let encrypted: EncryptedEnvelope =
            match bincode::serde::decode_from_slice(&envelope.payload, bincode::config::legacy()) {
                Ok((enc, _)) => enc,
                Err(e) => {
                    warn!(
                        "Failed to deserialize EncryptedEnvelope from {}: {}",
                        envelope.from, e
                    );
                    return;
                }
            };

        // 2. Verify we are the intended recipient
        if encrypted.to != self.own_did {
            warn!(
                "Encrypted message not for us: to={}, own_did={}",
                encrypted.to, self.own_did
            );
            return;
        }

        // 3. Get sender's X25519 public key from peer_connections
        let sender_x25519_bytes = {
            let connections = self.peer_connections.read().await;
            match connections.get(&envelope.from) {
                Some(info) => info.x25519_key,
                None => {
                    warn!(
                        "Cannot decrypt: no X25519 key for sender {} (Hello not exchanged?)",
                        envelope.from
                    );
                    return;
                }
            }
        };
        let sender_x25519_public = x25519_dalek::PublicKey::from(sender_x25519_bytes);

        // 4. Get our X25519 secret key and decrypt
        let our_x25519_secret = self.identity_bundle.x25519_secret();

        let plaintext = match encrypted.decrypt(&our_x25519_secret, &sender_x25519_public) {
            Ok(pt) => pt,
            Err(e) => {
                warn!(
                    "Decryption failed from {} (seq={}): {}",
                    envelope.from, encrypted.sequence, e
                );
                // Could record Byzantine violation here for decryption failures
                // (may indicate tampering or key mismatch)
                return;
            }
        };

        debug!(
            "Decrypted {} bytes from {} (seq={})",
            plaintext.len(),
            envelope.from,
            encrypted.sequence
        );

        // 5. Try to deserialize as inner SignedEnvelope (sign-encrypt-sign pattern)
        // If that fails, treat plaintext as raw application data
        match bincode::serde::decode_from_slice::<SignedEnvelope, _>(
            &plaintext,
            bincode::config::legacy(),
        ) {
            Ok((inner_envelope, _)) => {
                // Inner envelope found - verify and forward via signed handler
                debug!(
                    "Decrypted inner SignedEnvelope from {} (inner_seq={})",
                    inner_envelope.from, inner_envelope.sequence
                );

                // Construct a NetworkMessage wrapping the inner envelope
                let decrypted_message = NetworkMessage {
                    version: message.version,
                    from: message.from.clone(),
                    to: message.to.clone(),
                    trace_context: message.trace_context.clone(),
                    payload: MessagePayload::Signed(inner_envelope.clone()),
                };

                // Forward to inner handler for signature verification only (no replay check).
                // Replay protection is already provided by the outer envelope.
                // Use Box::pin to break the async recursion (signed -> encrypted -> signed_inner)
                Box::pin(self.handle_signed_inner(decrypted_message, &inner_envelope)).await;
            }
            Err(_) => {
                // No inner envelope - encrypted raw data without inner signing
                // This is reserved for future use cases. For now, we require sign-encrypt-sign.
                warn!(
                    "Decrypted raw payload ({} bytes) from {} - discarding (sign-encrypt-sign required)",
                    plaintext.len(),
                    envelope.from
                );
                // Future: Could add MessagePayload::RawEncrypted variant to support this case
            }
        }
    }

    /// Check if a signed envelope contains an encrypted payload
    #[allow(dead_code)] // Utility function for future use
    pub fn is_encrypted_payload(envelope: &SignedEnvelope) -> bool {
        envelope.payload_type == PayloadType::Encrypted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::PayloadType;
    use crate::replay_guard::ReplayGuard;
    use crate::{RateLimitConfig, RateLimiter, SessionManager};
    use icn_identity::{IdentityBundle, KeyPair};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Create a minimal ConnectionContext for testing with peer info
    fn create_test_context_with_peer(
        peer_did: &icn_identity::Did,
        peer_x25519: [u8; 32],
    ) -> (ConnectionContext, Arc<AtomicUsize>) {
        let keypair = KeyPair::generate().unwrap();
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone()).unwrap();
        let own_did = keypair.did().clone();

        // Counter for forwarded messages
        let forward_count = Arc::new(AtomicUsize::new(0));
        let forward_count_clone = Arc::clone(&forward_count);

        let handler: crate::IncomingMessageHandler = Arc::new(move |_msg| {
            forward_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));
        let replay_guard = Arc::new(RwLock::new(ReplayGuard::new(300, 3600)));
        let session_manager = Arc::new(RwLock::new(SessionManager::new()));

        // Set up peer connections with X25519 key
        let mut peer_connections_map = HashMap::new();
        peer_connections_map.insert(
            peer_did.clone(),
            crate::actor::PeerConnectionInfo {
                did: peer_did.clone(),
                negotiated_version: 1,
                peer_capabilities: crate::version::CapabilityFlags::E2E_ENCRYPTION,
                peer_software: "test".to_string(),
                x25519_key: peer_x25519,
            },
        );
        let peer_connections = Arc::new(RwLock::new(peer_connections_map));

        let ctx = ConnectionContext {
            handler,
            rate_limiter,
            replay_guard,
            neighbor_sets: None,
            topology_config: None,
            trust_graph: None,
            session_manager,
            peer_connections,
            blob_registry: None,
            misbehavior_detector: None,
            identity_bundle,
            own_did,
        };

        (ctx, forward_count)
    }

    #[tokio::test]
    async fn test_decrypt_and_forward() {
        // Create sender identity
        let sender_bundle = IdentityBundle::generate().unwrap();

        // Create receiver context with sender's X25519 key
        let (ctx, forward_count) = create_test_context_with_peer(
            sender_bundle.did(),
            *sender_bundle.x25519_public_bytes(),
        );

        // Create an inner signed message
        let inner_envelope = SignedEnvelope::new(
            sender_bundle.did(),
            sender_bundle.keypair(),
            1,
            PayloadType::Gossip,
            b"secret gossip data".to_vec(),
        )
        .unwrap();

        // Serialize the inner envelope
        let inner_bytes =
            bincode::serde::encode_to_vec(&inner_envelope, bincode::config::legacy()).unwrap();

        // Encrypt for the receiver
        let receiver_x25519_public =
            x25519_dalek::PublicKey::from(*ctx.identity_bundle.x25519_public_bytes());

        let encrypted = EncryptedEnvelope::encrypt(
            sender_bundle.did(),
            &ctx.own_did,
            1,
            &sender_bundle.x25519_secret(),
            &receiver_x25519_public,
            &inner_bytes,
        )
        .unwrap();

        // Create outer signed envelope
        let encrypted_bytes =
            bincode::serde::encode_to_vec(&encrypted, bincode::config::legacy()).unwrap();
        let outer_envelope = SignedEnvelope::new(
            sender_bundle.did(),
            sender_bundle.keypair(),
            1,
            PayloadType::Encrypted,
            encrypted_bytes,
        )
        .unwrap();

        // Create network message
        let message = NetworkMessage {
            version: 1,
            from: sender_bundle.did().clone(),
            to: Some(ctx.own_did.clone()),
            trace_context: None,
            payload: MessagePayload::Signed(outer_envelope.clone()),
        };

        // Handle the encrypted message
        ctx.handle_encrypted_payload(message, &outer_envelope).await;

        // Message should be forwarded after decryption
        assert_eq!(forward_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_wrong_recipient_rejected() {
        // Create sender and wrong recipient
        let sender_bundle = IdentityBundle::generate().unwrap();
        let wrong_recipient = IdentityBundle::generate().unwrap();

        // Create receiver context
        let (ctx, forward_count) = create_test_context_with_peer(
            sender_bundle.did(),
            *sender_bundle.x25519_public_bytes(),
        );

        // Encrypt for wrong recipient (not us)
        let encrypted = EncryptedEnvelope::encrypt(
            sender_bundle.did(),
            wrong_recipient.did(), // Wrong recipient!
            1,
            &sender_bundle.x25519_secret(),
            &wrong_recipient.x25519_public(),
            b"secret data",
        )
        .unwrap();

        let encrypted_bytes =
            bincode::serde::encode_to_vec(&encrypted, bincode::config::legacy()).unwrap();
        let outer_envelope = SignedEnvelope::new(
            sender_bundle.did(),
            sender_bundle.keypair(),
            1,
            PayloadType::Encrypted,
            encrypted_bytes,
        )
        .unwrap();

        let message = NetworkMessage {
            version: 1,
            from: sender_bundle.did().clone(),
            to: Some(wrong_recipient.did().clone()),
            trace_context: None,
            payload: MessagePayload::Signed(outer_envelope.clone()),
        };

        ctx.handle_encrypted_payload(message, &outer_envelope).await;

        // Should NOT be forwarded (wrong recipient)
        assert_eq!(forward_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_missing_peer_key_rejected() {
        // Create sender
        let sender_bundle = IdentityBundle::generate().unwrap();

        // Create receiver context WITHOUT sender in peer_connections
        let keypair = KeyPair::generate().unwrap();
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone()).unwrap();
        let own_did = keypair.did().clone();

        let forward_count = Arc::new(AtomicUsize::new(0));
        let forward_count_clone = Arc::clone(&forward_count);

        let handler: crate::IncomingMessageHandler = Arc::new(move |_msg| {
            forward_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let ctx = ConnectionContext {
            handler,
            rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig::default())),
            replay_guard: Arc::new(RwLock::new(ReplayGuard::new(300, 3600))),
            neighbor_sets: None,
            topology_config: None,
            trust_graph: None,
            session_manager: Arc::new(RwLock::new(SessionManager::new())),
            peer_connections: Arc::new(RwLock::new(HashMap::new())), // Empty!
            blob_registry: None,
            misbehavior_detector: None,
            identity_bundle,
            own_did: own_did.clone(),
        };

        // Create encrypted message
        let receiver_x25519_public =
            x25519_dalek::PublicKey::from(*ctx.identity_bundle.x25519_public_bytes());

        let encrypted = EncryptedEnvelope::encrypt(
            sender_bundle.did(),
            &own_did,
            1,
            &sender_bundle.x25519_secret(),
            &receiver_x25519_public,
            b"secret data",
        )
        .unwrap();

        let encrypted_bytes =
            bincode::serde::encode_to_vec(&encrypted, bincode::config::legacy()).unwrap();
        let outer_envelope = SignedEnvelope::new(
            sender_bundle.did(),
            sender_bundle.keypair(),
            1,
            PayloadType::Encrypted,
            encrypted_bytes,
        )
        .unwrap();

        let message = NetworkMessage {
            version: 1,
            from: sender_bundle.did().clone(),
            to: Some(own_did),
            trace_context: None,
            payload: MessagePayload::Signed(outer_envelope.clone()),
        };

        ctx.handle_encrypted_payload(message, &outer_envelope).await;

        // Should NOT be forwarded (missing peer X25519 key)
        assert_eq!(forward_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_is_encrypted_payload() {
        let keypair = KeyPair::generate().unwrap();

        let encrypted_env = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Encrypted,
            b"encrypted data".to_vec(),
        )
        .unwrap();

        let gossip_env = SignedEnvelope::new(
            keypair.did(),
            &keypair,
            1,
            PayloadType::Gossip,
            b"gossip data".to_vec(),
        )
        .unwrap();

        assert!(ConnectionContext::is_encrypted_payload(&encrypted_env));
        assert!(!ConnectionContext::is_encrypted_payload(&gossip_env));
    }
}
