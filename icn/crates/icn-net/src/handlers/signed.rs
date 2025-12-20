//! Signed message handler - signature verification and replay protection
//!
//! Handles SignedEnvelope messages with:
//! - Ed25519 signature verification
//! - Message age checking
//! - Replay attack detection
//! - Byzantine fault recording

use super::ConnectionContext;
use crate::envelope::SignedEnvelope;
use crate::protocol::NetworkMessage;
use tracing::{info, warn};

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
                    message_hash: message_hash
                        .clone()
                        .try_into()
                        .unwrap_or([0u8; 32]),
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
                info!(
                    "Verified signed message from {} (seq={})",
                    envelope.from, envelope.sequence
                );
                // Forward verified message to handler
                self.forward_to_handler(message);
            }
            Err(e) => {
                warn!(
                    "Replay attack detected from {}: {}",
                    envelope.from, e
                );

                // Record ReplayAttack violation
                if let Some(ref detector) = self.misbehavior_detector {
                    let message_hash = compute_message_hash(envelope);

                    let violation = icn_security::Violation::ReplayAttack {
                        message_hash: message_hash
                            .clone()
                            .try_into()
                            .unwrap_or([0u8; 32]),
                        sequence: envelope.sequence,
                    };

                    detector
                        .write()
                        .await
                        .record_violation(&envelope.from, violation, message_hash);
                }
                // Drop message (don't forward to handler)
            }
        }
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
