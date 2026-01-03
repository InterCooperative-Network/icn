//! Signed message handler - signature verification and replay protection
//!
//! Handles SignedEnvelope messages with:
//! - Ed25519 signature verification
//! - Message age checking
//! - Replay attack detection
//! - Byzantine fault recording

use super::ConnectionContext;
use crate::envelope::{PayloadType, SignedEnvelope};
use crate::protocol::NetworkMessage;
use tracing::{debug, warn};

impl ConnectionContext {
    /// Handle a Signed message envelope
    ///
    /// Performs:
    /// 1. Signature verification
    /// 2. Message age validation
    /// 3. Replay attack detection
    /// 4. Byzantine fault recording (on failure)
    /// 5. Forward to handler (on success)
    pub async fn handle_signed(&self, message: NetworkMessage, envelope: &SignedEnvelope) {
        // Verify signature and age first
        let sig_result = envelope.verify(300);

        if let Err(e) = sig_result {
            warn!(
                "Signature/age verification failed from {}: {}",
                envelope.from, e
            );

            // Record InvalidSignature violation
            if let Some(ref detector) = self.misbehavior_detector {
                let message_hash = compute_message_hash(envelope);

                let violation = icn_security::Violation::InvalidSignature {
                    message_hash: message_hash.clone().try_into().unwrap_or([0u8; 32]),
                };

                detector
                    .write()
                    .await
                    .record_violation(&envelope.from, violation, message_hash);
            }
            // Drop message (don't forward to handler)
            return;
        }

        // Signature valid, now check for replay attack
        match self.replay_guard.write().await.check(envelope) {
            Ok(()) => {
                debug!(
                    "Verified signed message from {} (seq={}, type={:?})",
                    envelope.from, envelope.sequence, envelope.payload_type
                );

                // Check if this is an encrypted payload that needs decryption
                if envelope.payload_type == PayloadType::Encrypted {
                    // Route to encrypted handler for decryption
                    self.handle_encrypted_payload(message, envelope).await;
                } else {
                    // Forward verified message to handler
                    self.forward_to_handler(message);
                }
            }
            Err(e) => {
                warn!("Replay attack detected from {}: {}", envelope.from, e);

                // Record ReplayAttack violation
                if let Some(ref detector) = self.misbehavior_detector {
                    let message_hash = compute_message_hash(envelope);

                    let violation = icn_security::Violation::ReplayAttack {
                        message_hash: message_hash.clone().try_into().unwrap_or([0u8; 32]),
                        sequence: envelope.sequence,
                    };

                    detector.write().await.record_violation(
                        &envelope.from,
                        violation,
                        message_hash,
                    );
                }
                // Drop message (don't forward to handler)
            }
        }
    }

    /// Handle an inner signed envelope from an encrypted payload
    ///
    /// This is called after decrypting an EncryptedEnvelope. It verifies the
    /// signature but SKIPS replay checking because:
    /// 1. The outer envelope already passed replay protection
    /// 2. The inner content is authenticated by ChaCha20-Poly1305
    /// 3. The inner and outer envelopes share the same sequence number
    ///
    /// Using the same sequence for both avoids consuming two sequence numbers
    /// per encrypted message.
    pub async fn handle_signed_inner(&self, message: NetworkMessage, envelope: &SignedEnvelope) {
        // Verify signature and age first (same as handle_signed)
        let sig_result = envelope.verify(300);

        if let Err(e) = sig_result {
            warn!(
                "Inner envelope signature/age verification failed from {}: {}",
                envelope.from, e
            );

            // Record InvalidSignature violation
            if let Some(ref detector) = self.misbehavior_detector {
                let message_hash = compute_message_hash(envelope);

                let violation = icn_security::Violation::InvalidSignature {
                    message_hash: message_hash.clone().try_into().unwrap_or([0u8; 32]),
                };

                detector
                    .write()
                    .await
                    .record_violation(&envelope.from, violation, message_hash);
            }
            return;
        }

        debug!(
            "Verified inner signed envelope from {} (seq={}, type={:?})",
            envelope.from, envelope.sequence, envelope.payload_type
        );

        // Skip replay check - the outer envelope already provided replay protection.
        // The inner content was inside authenticated encryption, so it couldn't have
        // been extracted and replayed separately.

        // Forward verified message to handler
        self.forward_to_handler(message);
    }
}

/// Compute a hash of the message for violation tracking
fn compute_message_hash(envelope: &SignedEnvelope) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(envelope.sequence.to_be_bytes());
    hasher.update(envelope.from.as_str().as_bytes());
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::PayloadType;
    use crate::protocol::MessagePayload;
    use crate::replay_guard::ReplayGuard;
    use crate::{RateLimitConfig, RateLimiter, SessionManager};
    use icn_identity::{IdentityBundle, KeyPair};
    use icn_security::{MisbehaviorDetector, MisbehaviorThresholds};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Create a minimal ConnectionContext for testing
    fn create_test_context(
        misbehavior_detector: Option<Arc<RwLock<MisbehaviorDetector>>>,
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
        let peer_connections = Arc::new(RwLock::new(HashMap::new()));

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
            misbehavior_detector,
            identity_bundle,
            own_did,
        };

        (ctx, forward_count)
    }

    fn create_signed_envelope(keypair: &KeyPair, sequence: u64) -> SignedEnvelope {
        SignedEnvelope::new(
            keypair.did(),
            keypair,
            sequence,
            PayloadType::Gossip,
            b"test payload".to_vec(),
        )
        .unwrap()
    }

    fn create_network_message(envelope: &SignedEnvelope) -> NetworkMessage {
        NetworkMessage {
            version: 1,
            from: envelope.from.clone(),
            to: None,
            trace_context: None,
            payload: MessagePayload::Signed(envelope.clone()),
        }
    }

    #[tokio::test]
    async fn test_valid_signature_forwarded() {
        let (ctx, forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();
        let envelope = create_signed_envelope(&sender, 1);
        let message = create_network_message(&envelope);

        ctx.handle_signed(message, &envelope).await;

        // Message should be forwarded
        assert_eq!(forward_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_invalid_signature_rejected() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, forward_count) = create_test_context(Some(detector.clone()));

        let sender = KeyPair::generate().unwrap();
        let mut envelope = create_signed_envelope(&sender, 1);

        // Tamper with the signature
        envelope.signature[0] ^= 0xFF;

        let message = create_network_message(&envelope);
        ctx.handle_signed(message, &envelope).await;

        // Message should NOT be forwarded
        assert_eq!(forward_count.load(Ordering::SeqCst), 0);

        // Should record an InvalidSignature violation
        let detector_guard = detector.read().await;
        let violations = detector_guard.get_violations(sender.did());
        assert!(!violations.is_empty());
        assert!(matches!(
            violations[0].violation,
            icn_security::Violation::InvalidSignature { .. }
        ));
    }

    #[tokio::test]
    async fn test_replay_attack_rejected() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, forward_count) = create_test_context(Some(detector.clone()));

        let sender = KeyPair::generate().unwrap();
        let envelope = create_signed_envelope(&sender, 1);
        let message = create_network_message(&envelope);

        // First message should be forwarded
        ctx.handle_signed(message.clone(), &envelope).await;
        assert_eq!(forward_count.load(Ordering::SeqCst), 1);

        // Replay should be rejected
        ctx.handle_signed(message.clone(), &envelope).await;
        assert_eq!(forward_count.load(Ordering::SeqCst), 1); // Still 1

        // Should record a ReplayAttack violation
        let detector_guard = detector.read().await;
        let violations = detector_guard.get_violations(sender.did());
        assert!(!violations.is_empty());
        assert!(matches!(
            violations[0].violation,
            icn_security::Violation::ReplayAttack { sequence: 1, .. }
        ));
    }

    #[tokio::test]
    async fn test_sequential_messages_forwarded() {
        let (ctx, forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();

        // Send messages with sequential sequence numbers
        for seq in 1..=5 {
            let envelope = create_signed_envelope(&sender, seq);
            let message = create_network_message(&envelope);
            ctx.handle_signed(message, &envelope).await;
        }

        // All 5 messages should be forwarded
        assert_eq!(forward_count.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn test_out_of_order_messages_forwarded() {
        let (ctx, forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();

        // Send messages out of order: 3, 1, 2
        for seq in [3, 1, 2] {
            let envelope = create_signed_envelope(&sender, seq);
            let message = create_network_message(&envelope);
            ctx.handle_signed(message, &envelope).await;
        }

        // All 3 messages should be forwarded (out of order is OK)
        assert_eq!(forward_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_multiple_senders_independent() {
        let (ctx, forward_count) = create_test_context(None);

        let alice = KeyPair::generate().unwrap();
        let bob = KeyPair::generate().unwrap();

        // Both can send sequence 1
        let envelope_alice = create_signed_envelope(&alice, 1);
        let envelope_bob = create_signed_envelope(&bob, 1);

        ctx.handle_signed(create_network_message(&envelope_alice), &envelope_alice)
            .await;
        ctx.handle_signed(create_network_message(&envelope_bob), &envelope_bob)
            .await;

        // Both should be forwarded
        assert_eq!(forward_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_no_detector_no_crash_on_invalid() {
        // Test that invalid messages are handled gracefully without detector
        let (ctx, forward_count) = create_test_context(None); // No detector

        let sender = KeyPair::generate().unwrap();
        let mut envelope = create_signed_envelope(&sender, 1);

        // Tamper with signature
        envelope.signature[0] ^= 0xFF;

        let message = create_network_message(&envelope);

        // Should not panic, just not forward
        ctx.handle_signed(message, &envelope).await;
        assert_eq!(forward_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_compute_message_hash_deterministic() {
        let sender = KeyPair::generate().unwrap();
        let envelope = create_signed_envelope(&sender, 42);

        let hash1 = compute_message_hash(&envelope);
        let hash2 = compute_message_hash(&envelope);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32); // SHA-256 output
    }

    #[test]
    fn test_compute_message_hash_unique_per_sequence() {
        let sender = KeyPair::generate().unwrap();
        let envelope1 = create_signed_envelope(&sender, 1);
        let envelope2 = create_signed_envelope(&sender, 2);

        let hash1 = compute_message_hash(&envelope1);
        let hash2 = compute_message_hash(&envelope2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_message_hash_unique_per_sender() {
        let sender1 = KeyPair::generate().unwrap();
        let sender2 = KeyPair::generate().unwrap();
        let envelope1 = create_signed_envelope(&sender1, 1);
        let envelope2 = create_signed_envelope(&sender2, 1);

        let hash1 = compute_message_hash(&envelope1);
        let hash2 = compute_message_hash(&envelope2);

        assert_ne!(hash1, hash2);
    }
}
