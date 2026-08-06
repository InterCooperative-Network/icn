//! Signed message handler - signature verification and replay protection
//!
//! Handles SignedEnvelope messages with:
//! - Ed25519 signature verification
//! - ML-DSA signature verification (for hybrid envelopes, when PQ key is cached)
//! - Message age checking
//! - Replay attack detection
//! - Byzantine fault recording

use super::ConnectionContext;
use crate::envelope::{PayloadType, SignedEnvelope};
use crate::protocol::NetworkMessage;
use tracing::{debug, error, warn};

impl ConnectionContext {
    /// Handle a Signed message envelope
    ///
    /// Performs:
    /// 1. Signature verification (Ed25519, and ML-DSA for hybrid if PQ key cached)
    /// 2. Message age validation
    /// 3. Replay attack detection
    /// 4. Byzantine fault recording (on failure)
    /// 5. Forward to handler (on success)
    pub async fn handle_signed(&self, message: NetworkMessage, envelope: &SignedEnvelope) {
        // Our own DID is not a remote sender (#2506). `ReplayGuard` tracks per-*remote*-peer
        // sequence high-water marks and `MisbehaviorDetector` scores *remote* peers, so admitting
        // our own DID into either corrupts what both structures mean — and after a restart it is
        // self-defeating: our resumed signing sequence sits below our own persisted replay floor,
        // so we score our own traffic as replay at severity 1.0 and ban ourselves.
        //
        // This is deliberately not a blanket "skip security checks for our own DID": it drops the
        // message rather than trusting it, and it is scoped to the network receive path, where a
        // self-sourced envelope can only be a self-connection. The connection layer refuses to
        // register the local DID as a peer, so reaching here means an earlier guard was bypassed.
        if envelope.from == self.own_did {
            warn!(
                sequence = envelope.sequence,
                "Dropping network message from our own DID without recording remote-peer state; \
                 a self-connection reached the signed-message path (#2506)"
            );
            return;
        }

        // Verify signature and age - use cached PQ key for hybrid envelopes
        let sig_result = self.verify_with_cached_pq_key(envelope, 300).await;

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

        // Which sequence namespace does this envelope's number belong to? (#2517)
        //
        // Read from the capabilities recorded for `envelope.from`, which
        // `handle_hello` writes only after binding the claimed DID to the certificate
        // on the live QUIC connection (#2520). So a `DurableV1` answer here means the
        // peer proved, on a connection it actually authenticated, that its signing
        // sequence is durable DID state — not merely that some Hello somewhere once
        // said so.
        //
        // Every other outcome is `LegacyOrUnproven`, deliberately: a peer with no
        // recorded connection info, a peer that predates the capability, and a
        // genuinely pre-#2510 ephemeral sender are indistinguishable from here, and
        // the safe reading is the one that does not promise durability.
        let observed_regime = {
            let connections = self.peer_connections.read().await;
            match connections.get(&envelope.from) {
                Some(info)
                    if info
                        .peer_capabilities
                        .contains(crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE) =>
                {
                    crate::replay_guard::ObservedSenderRegime::DurableV1
                }
                _ => crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            }
        };

        // Signature valid, now check for replay attack
        // Use check_replay_only since we already verified the signature above
        // This avoids redundant signature verification and ensures immediate PQ
        // verification (via verify_with_cached_pq_key) is the only path used
        match self
            .replay_guard
            .write()
            .await
            .check_replay_only(envelope, observed_regime)
        {
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
                // A local storage fault is not peer misbehaviour. The guard
                // fails closed when it cannot durably record an acceptance, and
                // rejects everything from a peer whose durable state it cannot
                // read. Scoring either against the sender would ban innocent
                // peers for our own disk problem — the exact false-positive
                // class #2514 was about.
                let local_fault = e
                    .downcast_ref::<crate::replay_guard::ReplayStateNotDurable>()
                    .map(|e| e.to_string())
                    .or_else(|| {
                        e.downcast_ref::<crate::replay_guard::ReplayStateUnreadable>()
                            .map(|e| e.to_string())
                    })
                    .or_else(|| {
                        // Migrating away from an obsolete semantic regime (#2517).
                        // The peer is sending legitimate traffic; we are the ones
                        // holding a number we can no longer interpret. Scoring this
                        // is what produced thousands of false severity-1.0 events
                        // and bans against legitimate traffic on the rehearsal
                        // federation.
                        e.downcast_ref::<crate::replay_guard::ReplayStateLegacy>()
                            .map(|e| e.to_string())
                    })
                    .or_else(|| {
                        // Replay state from a regime with no migration (#2517),
                        // i.e. this binary is older than the one that wrote its
                        // store. Also not peer misbehaviour — and unlike the
                        // others it will not clear on its own, so it is logged at
                        // error rather than warn below.
                        e.downcast_ref::<crate::replay_guard::ReplayStateUnsupportedVersion>()
                            .map(|e| e.to_string())
                    })
                    .or_else(|| {
                        // The sender changed sequence namespaces and we are retiring
                        // the old one (#2517). The peer is sending legitimate traffic
                        // under its new numbering; we are refusing it because
                        // envelopes from the *previous* numbering could still be
                        // fresh. Scoring this would ban every peer that upgrades.
                        e.downcast_ref::<crate::replay_guard::SenderRegimeTransition>()
                            .map(|e| e.to_string())
                    })
                    .or_else(|| {
                        // A peer that previously proved the durable regime no longer
                        // advertises it (#2517) — typically an operator rollback. A
                        // local incompatibility, and deliberately fail-closed: the
                        // alternative, discarding durable replay state on downgrade,
                        // would make replay-state reset reachable by downgrade.
                        e.downcast_ref::<crate::replay_guard::SenderRegimeDowngrade>()
                            .map(|e| e.to_string())
                    })
                    .or_else(|| {
                        // A persisted sender-regime tag with no meaning in this
                        // binary (#2517). Like the receiver-side unsupported version,
                        // this does not clear on its own.
                        e.downcast_ref::<crate::replay_guard::UnsupportedSenderRegime>()
                            .map(|e| e.to_string())
                    });
                if let Some(reason) = local_fault {
                    // Three of these local faults do not resolve themselves, so they
                    // are raised to error: everything else here is a bounded
                    // condition an operator can wait out, while these need them to
                    // act — upgrade a binary, or roll a downgraded peer forward.
                    let needs_operator = e
                        .downcast_ref::<crate::replay_guard::ReplayStateUnsupportedVersion>()
                        .is_some()
                        || e.downcast_ref::<crate::replay_guard::UnsupportedSenderRegime>()
                            .is_some()
                        || e.downcast_ref::<crate::replay_guard::SenderRegimeDowngrade>()
                            .is_some();
                    if needs_operator {
                        error!(
                            peer = %envelope.from,
                            seq = envelope.sequence,
                            "Dropping message: a local protocol-state incompatibility that will \
                             not clear on its own. Not peer misbehaviour — an operator must \
                             upgrade a binary or roll a downgraded peer forward: {reason}"
                        );
                    } else {
                        warn!(
                            peer = %envelope.from,
                            seq = envelope.sequence,
                            "Dropping message (local replay-state fault, not peer misbehaviour): {reason}"
                        );
                    }
                    return;
                }

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
    /// ## Security Trust Assumption
    ///
    /// Skipping inner replay protection is safe because ChaCha20-Poly1305 AEAD
    /// provides **ciphertext integrity**. An attacker cannot:
    /// - Extract the inner envelope from an encrypted message (no decryption key)
    /// - Modify the ciphertext without detection (Poly1305 tag verification fails)
    /// - Replay the outer encrypted message (outer envelope replay guard)
    ///
    /// The only way to obtain the inner envelope is through legitimate decryption,
    /// which requires the recipient's X25519 private key. Therefore, any inner
    /// envelope we process was necessarily inside a replay-protected outer envelope.
    ///
    /// Using the same sequence for both avoids consuming two sequence numbers
    /// per encrypted message.
    pub async fn handle_signed_inner(&self, message: NetworkMessage, envelope: &SignedEnvelope) {
        // Verify signature and age - use cached PQ key for hybrid envelopes
        let sig_result = self.verify_with_cached_pq_key(envelope, 300).await;

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
            icn_obs::metrics::network::encryption_rejected_inc("invalid_inner_signature");
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

    /// Verify a signed envelope, using cached PQ public key for hybrid envelopes
    ///
    /// For hybrid envelopes (Ed25519 + ML-DSA), this method:
    /// 1. Looks up the sender's ML-DSA public key from peer_connections cache
    /// 2. If found, performs full both-must-verify hybrid verification
    /// 3. If not found, falls back to deferred verification (Ed25519 only, PQ format check)
    ///
    /// For classical envelopes, this is equivalent to `envelope.verify()`.
    async fn verify_with_cached_pq_key(
        &self,
        envelope: &SignedEnvelope,
        max_age_secs: u64,
    ) -> anyhow::Result<()> {
        // For hybrid envelopes, try to use cached PQ key for full verification
        #[cfg(feature = "post-quantum")]
        if envelope.is_hybrid() {
            let connections = self.peer_connections.read().await;
            if let Some(peer_info) = connections.get(&envelope.from) {
                if let Some(ref ml_dsa_bytes) = peer_info.ml_dsa_public {
                    // We have the sender's PQ key - perform full hybrid verification
                    let pq_key = match icn_crypto_pq::MlDsaPublicKey::from_bytes(ml_dsa_bytes) {
                        Ok(key) => key,
                        Err(e) => {
                            icn_obs::metrics::network::hybrid_verification_failed_inc(
                                icn_obs::metrics::network::HybridVerificationFailure::InvalidPqKey,
                            );
                            return Err(anyhow::anyhow!("Invalid cached ML-DSA key: {e}"));
                        }
                    };

                    debug!(
                        "Performing full hybrid verification for {} using cached PQ key",
                        envelope.from
                    );

                    let result = envelope.verify_with_pq_key(max_age_secs, &pq_key);
                    match &result {
                        Ok(()) => {
                            icn_obs::metrics::network::hybrid_verification_cache_hit_inc();
                        }
                        Err(_) => {
                            // Distinguish classical vs PQ signature failures:
                            // If classical-only verify also fails -> classical signature issue
                            // If classical-only verify succeeds -> PQ-side issue
                            let failure_reason = if envelope.verify(max_age_secs).is_ok() {
                                icn_obs::metrics::network::HybridVerificationFailure::PqSignatureMismatch
                            } else {
                                icn_obs::metrics::network::HybridVerificationFailure::ClassicalSignatureFailed
                            };
                            icn_obs::metrics::network::hybrid_verification_failed_inc(
                                failure_reason,
                            );
                        }
                    }
                    return result;
                }
            }
            // No cached PQ key - fall through to deferred verification
            debug!(
                "No cached PQ key for {} - using deferred hybrid verification",
                envelope.from
            );
            icn_obs::metrics::network::hybrid_verification_cache_miss_inc();
        }

        // Classical envelope or no cached PQ key - use standard verification
        envelope.verify(max_age_secs)
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
        let (ctx, _keypair, count) = create_test_context_with_own_keypair(misbehavior_detector);
        (ctx, count)
    }

    /// Same as [`create_test_context`], but also hands back the keypair the context's own DID is
    /// derived from, so a test can sign traffic *as the local node* (#2506).
    fn create_test_context_with_own_keypair(
        misbehavior_detector: Option<Arc<RwLock<MisbehaviorDetector>>>,
    ) -> (ConnectionContext, KeyPair, Arc<AtomicUsize>) {
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
            session_manager,
            peer_connections,
            blob_registry: None,
            misbehavior_detector,
            identity_bundle,
            own_did,
            direction: crate::handlers::ConnectionDirection::Inbound,
            // Inbound: we did not choose this peer, so we expected nobody (#2533).
            expected_peer: None,
            expectation_mismatch_reported: std::sync::atomic::AtomicBool::new(false),
            hello_responded: std::sync::atomic::AtomicBool::new(false),
            authenticated_peer: tokio::sync::RwLock::new(None),
            peer_exchange_enabled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        (ctx, keypair, forward_count)
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

    /// Regression test for #2506.
    ///
    /// A signed envelope whose sender is our *own* DID is not remote peer traffic, so it must not
    /// create replay state or misbehaviour state keyed by the local DID. Live, a node dialed its
    /// own advertised endpoint, and after a restart its resumed signing sequence sat below its own
    /// persisted replay floor — so it scored its own messages as replays at severity 1.0,
    /// quarantined and then banned its own DID, and stopped receiving federation gossip for 26
    /// minutes until its counter climbed past its own floor.
    #[tokio::test]
    async fn test_own_did_envelope_creates_no_replay_or_misbehavior_state() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, own_keypair, forward_count) =
            create_test_context_with_own_keypair(Some(detector.clone()));
        let own_did = own_keypair.did().clone();

        // Two envelopes signed by *us*, arriving over the network on a self-connection. The
        // second reuses sequence 1, which is exactly what the replay guard exists to catch —
        // but from our own DID it must not be treated as a remote peer misbehaving.
        let envelope = create_signed_envelope(&own_keypair, 1);
        let message = create_network_message(&envelope);
        ctx.handle_signed(message.clone(), &envelope).await;
        ctx.handle_signed(message.clone(), &envelope).await;

        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            0,
            "#2506: our own DID's traffic must not be forwarded as remote peer traffic"
        );
        assert_eq!(
            ctx.replay_guard.read().await.peer_count(),
            0,
            "#2506: the replay guard must not hold a window keyed by the local DID"
        );
        assert!(
            detector.read().await.get_violations(&own_did).is_empty(),
            "#2506: the local DID must never accrue misbehaviour violations from a self-loop"
        );
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

    /// Test that hybrid envelopes are verified using cached PQ keys
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn test_hybrid_verification_with_cached_pq_key() {
        use crate::actor::PeerConnectionInfo;
        use crate::version::CapabilityFlags;

        let (ctx, forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();

        // Sender should have PQ keys (post-quantum feature enabled)
        assert!(sender.has_pq_keys(), "Sender should have PQ keys");

        // Get sender's PQ public key
        let ml_dsa_public = sender.pq_public_key().map(|pk| pk.as_bytes().to_vec());

        // Cache the sender's PQ key in peer_connections
        {
            let mut connections = ctx.peer_connections.write().await;
            connections.insert(
                sender.did().clone(),
                PeerConnectionInfo {
                    did: sender.did().clone(),
                    negotiated_version: 1,
                    peer_capabilities: CapabilityFlags::HYBRID_SIGNATURES,
                    peer_software: "test".to_string(),
                    x25519_key: [0u8; 32],
                    ml_dsa_public,
                    ml_kem_public: None,
                },
            );
        }

        // Create a hybrid envelope
        let envelope = SignedEnvelope::new_hybrid(
            sender.did(),
            &sender,
            1,
            PayloadType::Gossip,
            b"test hybrid".to_vec(),
        )
        .expect("Failed to create hybrid envelope");

        assert!(envelope.is_hybrid(), "Envelope should be hybrid");

        let message = NetworkMessage {
            version: 1,
            from: sender.did().clone(),
            to: None,
            trace_context: None,
            payload: MessagePayload::Signed(envelope.clone()),
        };

        // Handle the signed message - should use cached PQ key for full verification
        ctx.handle_signed(message, &envelope).await;

        // Message should be forwarded (verification passed)
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            1,
            "Hybrid envelope should be forwarded after full verification"
        );
    }

    /// Test that hybrid envelopes fall back to deferred verification when PQ key not cached
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn test_hybrid_verification_deferred_when_no_cached_key() {
        let (ctx, forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();

        // Sender should have PQ keys
        assert!(sender.has_pq_keys(), "Sender should have PQ keys");

        // Do NOT cache the sender's PQ key (peer_connections empty)

        // Create a hybrid envelope
        let envelope = SignedEnvelope::new_hybrid(
            sender.did(),
            &sender,
            1,
            PayloadType::Gossip,
            b"test deferred".to_vec(),
        )
        .expect("Failed to create hybrid envelope");

        assert!(envelope.is_hybrid(), "Envelope should be hybrid");

        let message = NetworkMessage {
            version: 1,
            from: sender.did().clone(),
            to: None,
            trace_context: None,
            payload: MessagePayload::Signed(envelope.clone()),
        };

        // Handle the signed message - should use deferred verification
        ctx.handle_signed(message, &envelope).await;

        // Message should still be forwarded (deferred verification accepts)
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            1,
            "Hybrid envelope should be forwarded with deferred verification"
        );
    }

    /// Test that hybrid envelopes fail verification when cached PQ key is invalid/corrupted
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn test_hybrid_verification_fails_with_invalid_cached_key() {
        use crate::actor::PeerConnectionInfo;
        use crate::version::CapabilityFlags;

        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, forward_count) = create_test_context(Some(detector.clone()));
        let sender = KeyPair::generate().unwrap();

        // Sender should have PQ keys
        assert!(sender.has_pq_keys(), "Sender should have PQ keys");

        // Cache an INVALID/corrupted ML-DSA public key (wrong size/format)
        // ML-DSA-65 public keys are 1952 bytes; this 4-byte value will fail from_bytes()
        {
            let mut connections = ctx.peer_connections.write().await;
            connections.insert(
                sender.did().clone(),
                PeerConnectionInfo {
                    did: sender.did().clone(),
                    negotiated_version: 1,
                    peer_capabilities: CapabilityFlags::HYBRID_SIGNATURES,
                    peer_software: "test".to_string(),
                    x25519_key: [0u8; 32],
                    ml_dsa_public: Some(vec![0xDE, 0xAD, 0xBE, 0xEF]), // Invalid: 4 bytes vs required 1952
                    ml_kem_public: None,
                },
            );
        }

        // Create a valid hybrid envelope
        let envelope = SignedEnvelope::new_hybrid(
            sender.did(),
            &sender,
            1,
            PayloadType::Gossip,
            b"test invalid key".to_vec(),
        )
        .expect("Failed to create hybrid envelope");

        assert!(envelope.is_hybrid(), "Envelope should be hybrid");

        let message = NetworkMessage {
            version: 1,
            from: sender.did().clone(),
            to: None,
            trace_context: None,
            payload: MessagePayload::Signed(envelope.clone()),
        };

        // Handle the signed message - should fail due to invalid cached key
        ctx.handle_signed(message, &envelope).await;

        // Message should NOT be forwarded (verification failed)
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            0,
            "Hybrid envelope should NOT be forwarded when cached PQ key is invalid"
        );

        // Should record an InvalidSignature violation for Byzantine fault detection
        let detector_guard = detector.read().await;
        let violations = detector_guard.get_violations(sender.did());
        assert!(
            !violations.is_empty(),
            "Should record a violation for invalid PQ key"
        );
        assert!(
            matches!(
                violations[0].violation,
                icn_security::Violation::InvalidSignature { .. }
            ),
            "Violation should be InvalidSignature"
        );
    }

    /// Test that hybrid envelopes fail verification when cached PQ key doesn't match sender
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn test_hybrid_verification_fails_with_wrong_cached_key() {
        use crate::actor::PeerConnectionInfo;
        use crate::version::CapabilityFlags;

        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, forward_count) = create_test_context(Some(detector.clone()));
        let sender = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap(); // Different keypair

        // Sender should have PQ keys
        assert!(sender.has_pq_keys(), "Sender should have PQ keys");
        assert!(other.has_pq_keys(), "Other should have PQ keys");

        // Cache the WRONG peer's PQ key (other's key instead of sender's)
        let wrong_ml_dsa_public = other.pq_public_key().map(|pk| pk.as_bytes().to_vec());
        {
            let mut connections = ctx.peer_connections.write().await;
            connections.insert(
                sender.did().clone(),
                PeerConnectionInfo {
                    did: sender.did().clone(),
                    negotiated_version: 1,
                    peer_capabilities: CapabilityFlags::HYBRID_SIGNATURES,
                    peer_software: "test".to_string(),
                    x25519_key: [0u8; 32],
                    ml_dsa_public: wrong_ml_dsa_public, // Wrong key!
                    ml_kem_public: None,
                },
            );
        }

        // Create a valid hybrid envelope signed by sender
        let envelope = SignedEnvelope::new_hybrid(
            sender.did(),
            &sender,
            1,
            PayloadType::Gossip,
            b"test wrong key".to_vec(),
        )
        .expect("Failed to create hybrid envelope");

        assert!(envelope.is_hybrid(), "Envelope should be hybrid");

        let message = NetworkMessage {
            version: 1,
            from: sender.did().clone(),
            to: None,
            trace_context: None,
            payload: MessagePayload::Signed(envelope.clone()),
        };

        // Handle the signed message - should fail because cached key is wrong
        ctx.handle_signed(message, &envelope).await;

        // Message should NOT be forwarded (ML-DSA signature won't verify with wrong key)
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            0,
            "Hybrid envelope should NOT be forwarded when cached PQ key doesn't match"
        );

        // Should record an InvalidSignature violation for Byzantine fault detection
        let detector_guard = detector.read().await;
        let violations = detector_guard.get_violations(sender.did());
        assert!(
            !violations.is_empty(),
            "Should record a violation for wrong PQ key"
        );
        assert!(
            matches!(
                violations[0].violation,
                icn_security::Violation::InvalidSignature { .. }
            ),
            "Violation should be InvalidSignature"
        );
    }

    /// #2517 (mutation control M1): the capability→regime resolution in this handler.
    ///
    /// `ReplayGuard` takes the sender regime as a parameter, so every unit test of the
    /// guard supplies it directly and none of them exercise the one place that
    /// *derives* it. That left the derivation — the single point where a missing
    /// capability could be read as durable — with no coverage at all.
    ///
    /// A peer with no recorded capabilities, or capabilities lacking
    /// `DURABLE_SIGNING_SEQUENCE`, must resolve to `LegacyOrUnproven`. The observable
    /// consequence is that its accepted high-water is *not* tagged durable-v1.
    #[tokio::test]
    async fn missing_durable_capability_resolves_to_unproven_not_durable() {
        let (ctx, forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();

        // A peer that is authenticated but advertises an unrelated capability set.
        ctx.peer_connections.write().await.insert(
            sender.did().clone(),
            crate::actor::PeerConnectionInfo {
                did: sender.did().clone(),
                negotiated_version: 1,
                peer_capabilities: crate::CapabilityFlags::E2E_ENCRYPTION,
                peer_software: "old".to_string(),
                x25519_key: [0u8; 32],
                ml_dsa_public: None,
                ml_kem_public: None,
            },
        );

        let envelope = create_signed_envelope(&sender, 1);
        ctx.handle_signed(create_network_message(&envelope), &envelope)
            .await;
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            1,
            "an unproven sender's traffic is still delivered; compatibility requires it"
        );

        // The observable proof of the attribution: a peer resolved as unproven cannot
        // reach durable-v1 steady state, so a *durable* attribution for the same DID
        // would now be a namespace change and be held. If the handler had resolved
        // DurableV1 above, this would instead be ordinary steady-state traffic.
        let held = ctx
            .replay_guard
            .write()
            .await
            .check_replay_only(
                &create_signed_envelope(&sender, 2),
                crate::replay_guard::ObservedSenderRegime::DurableV1,
            )
            .expect_err(
                "the first message must have established this peer as UNPROVEN; if the \
                 handler read a missing capability as durable, this would be steady state",
            );
        assert!(
            held.downcast_ref::<crate::replay_guard::SenderRegimeTransition>()
                .is_some(),
            "expected a namespace-change hold, got: {held}"
        );
    }

    /// The positive half: a peer that *does* advertise the capability resolves to
    /// `DurableV1`.
    ///
    /// Without this, the test above would pass on a build that hardcoded
    /// `LegacyOrUnproven` and never read capabilities at all.
    #[tokio::test]
    async fn advertised_durable_capability_resolves_to_durable() {
        let (ctx, _forward_count) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();

        ctx.peer_connections.write().await.insert(
            sender.did().clone(),
            crate::actor::PeerConnectionInfo {
                did: sender.did().clone(),
                negotiated_version: 1,
                peer_capabilities: crate::CapabilityFlags::E2E_ENCRYPTION
                    | crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE,
                peer_software: "current".to_string(),
                x25519_key: [0u8; 32],
                ml_dsa_public: None,
                ml_kem_public: None,
            },
        );

        let envelope = create_signed_envelope(&sender, 1);
        ctx.handle_signed(create_network_message(&envelope), &envelope)
            .await;

        // A durable attribution establishes the peer via the migration hold, so the
        // first message is held rather than delivered — the observable difference from
        // the unproven case above.
        let state = ctx
            .replay_guard
            .write()
            .await
            .check_replay_only(
                &create_signed_envelope(&sender, 2),
                crate::replay_guard::ObservedSenderRegime::DurableV1,
            )
            .expect_err("first establishment holds");
        assert!(
            state
                .downcast_ref::<crate::replay_guard::SenderRegimeTransition>()
                .is_some(),
            "a durable-advertising peer must be on the durable establishment path: {state}"
        );
    }

    /// A peer held through the #2517 migration must not be scored, quarantined or banned.
    ///
    /// This is the property that makes the migration safe to run on a live federation, and
    /// it is the exact failure #2514 produced from the other direction: a receiver that
    /// reads a legitimate sequence against a bound that never applied to it records replay
    /// at severity 1.0, and 2060 violation series became 2333 bans against traffic that was
    /// never malicious. A migration hold refuses *more* traffic than that did — every
    /// message for a full retirement horizon — so if it scored, it would be strictly worse.
    ///
    /// The peer here is doing nothing wrong. It authenticated, it advertises the durable
    /// regime honestly, and its sequences are valid. The receiver refuses them only because
    /// envelopes from the peer's *previous* namespace could still be inside their validity
    /// window. That is local migration uncertainty, not misbehaviour, and the distinction
    /// has to be observable rather than merely intended.
    #[tokio::test]
    async fn a_peer_held_through_the_migration_is_never_scored_quarantined_or_banned() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, forward_count) = create_test_context(Some(detector.clone()));

        let migrating = KeyPair::generate().unwrap();
        let migrating_did = migrating.did().clone();
        ctx.peer_connections.write().await.insert(
            migrating_did.clone(),
            crate::actor::PeerConnectionInfo {
                did: migrating_did.clone(),
                negotiated_version: 1,
                peer_capabilities: crate::CapabilityFlags::E2E_ENCRYPTION
                    | crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE,
                peer_software: "current".to_string(),
                x25519_key: [0u8; 32],
                ml_dsa_public: None,
                ml_kem_public: None,
            },
        );

        // Sustained legitimate traffic for the whole hold, not one message. A hold that
        // scored only after N refusals would pass a single-message test.
        for seq in 1..=12 {
            let envelope = create_signed_envelope(&migrating, seq);
            ctx.handle_signed(create_network_message(&envelope), &envelope)
                .await;
        }

        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            0,
            "#2517: held traffic must not be delivered during the retirement horizon"
        );
        assert!(
            detector
                .read()
                .await
                .get_violations(&migrating_did)
                .is_empty(),
            "#2517: a peer held for local migration uncertainty must accrue no violations; \
             scoring the hold would ban every peer that upgrades"
        );
        assert!(
            !detector.read().await.is_quarantined(&migrating_did),
            "#2517: a migration hold must not quarantine the peer"
        );
        assert!(
            !detector.read().await.is_banned(&migrating_did),
            "#2517: a migration hold must not ban the peer"
        );

        // Non-vacuity control. Without this the assertions above would pass on a build
        // where the detector is never consulted at all — which is indistinguishable from
        // one that correctly classifies the hold as a local fault. A genuine replay from a
        // different peer must still score, on this same context and this same detector.
        let attacker = KeyPair::generate().unwrap();
        let attacker_did = attacker.did().clone();
        let replayed = create_signed_envelope(&attacker, 1);
        let message = create_network_message(&replayed);
        ctx.handle_signed(message.clone(), &replayed).await;
        ctx.handle_signed(message.clone(), &replayed).await;
        assert!(
            !detector
                .read()
                .await
                .get_violations(&attacker_did)
                .is_empty(),
            "control: a real replay must still be scored, or the assertions above prove \
             nothing about how the migration hold is classified"
        );
    }
}
