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
        //
        // Compared by *key*, not by spelling (#2640). `Did` equality is string equality and
        // `Did::from_str` accepts any multibase base, so a re-spelled copy of our own DID
        // walked straight past a `==` here — the same primitive that gave a re-spelled
        // captured envelope its own replay window. The comparison now asks the question the
        // guard beneath it asks: is this the same signing key?
        //
        // Both sides are derivable by construction — `own_did` is `keypair.did()`, and a wire
        // `from` has already been through `Did::deserialize`, which requires a valid Ed25519
        // public key — so the error arm is unreachable from the network. It is written as
        // "drop" rather than "admit" anyway, because the cost of a wrong drop is one message
        // and the cost of a wrong admit is our own DID inside remote-peer replay and
        // misbehaviour state (#2506), which is self-defeating after a restart.
        //
        // Kept rather than discarded once the comparison is done (#2644). Every later
        // security decision keyed to *who sent this* has to ask the same question with the
        // same equivalence class, and the capability lookup below was still asking it
        // textually — so the answer to "is this my own key" and the answer to "what did this
        // key prove" could disagree about the same sender.
        let sender_principal = match (
            crate::replay_guard::SenderPrincipal::from_did(&envelope.from),
            crate::replay_guard::SenderPrincipal::from_did(&self.own_did),
        ) {
            (Ok(sender), Ok(own)) if sender != own => Some(sender),
            (Ok(_), Ok(_)) => None,
            _ => {
                error!(
                    from = %envelope.from,
                    "Could not derive a signing key for the sender or for our own DID; \
                     dropping rather than risking self-sourced traffic entering remote-peer \
                     state (#2506/#2640)"
                );
                None
            }
        };
        let Some(sender_principal) = sender_principal else {
            warn!(
                sequence = envelope.sequence,
                "Dropping network message from our own DID without recording remote-peer state; \
                 a self-connection reached the signed-message path (#2506)"
            );
            return;
        };

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
        // Answered from the capability claims live connections are *currently* holding, so a
        // `DurableV1` answer means some connection this node holds right now has authenticated
        // a spelling of this key (#2520) and advertised `DURABLE_SIGNING_SEQUENCE` on it.
        //
        // Every other outcome is `LegacyOrUnproven`, deliberately: a peer with no current
        // claim, a peer that predates the capability, and a genuinely pre-#2510 ephemeral
        // sender are indistinguishable from here, and the safe reading is the one that does
        // not promise durability.
        //
        // # Asked about the sender's *key*, not the spelling of `from` (#2640)
        //
        // This was `peer_connections.get(&envelope.from)` — `Did`'s string equality, the same
        // primitive #2640 removed from the replay guard, left standing one line above the guard
        // it feeds. That map is keyed by the wire spelling the peer used in its Hello, so
        // re-spelling a captured envelope's `from` made the lookup miss and the sender read as
        // `LegacyOrUnproven`. Against an **empty** replay window that is not a redundant
        // refusal, it is the accept path: `(LegacyOrUnproven, LegacyOrUnproven)` is steady
        // state, so the captured pre-upgrade envelope was forwarded to the application instead
        // of entering the retirement hold that `(LegacyOrUnproven, DurableV1)` installs. The
        // floor that rejects this replay in an established window did not exist yet, so nothing
        // else refused it.
        //
        // The registry is indexed by `SenderPrincipal`, so this asks the same question of the
        // same equivalence class as the signature check above and the guard below, and one
        // signing principal cannot select two different capability states. There is
        // deliberately no textual fallback — that fallback *is* the defect.
        //
        // # Naming the right principal is necessary but not sufficient: the claim must be *current*
        //
        // Keying by principal alone over `peer_connections` re-opened the defect from the other
        // side, because that map is a cache rather than a live-session registry. Nothing removes
        // a row when a connection ends — the connection handler returns on both the
        // application-close and the error path without touching it — and `restore_state`
        // recreates rows from a snapshot at startup with their capability bits intact. So a peer
        // that proved the capability under spelling A, closed that connection, and came back
        // under spelling B *without* it — an operator rollback, or a lost signing store — still
        // read as `DurableV1` from A's abandoned row, since B's Hello only replaces B's own key.
        // That is precisely the `(DurableV1, LegacyOrUnproven)` state the guard exists to
        // refuse, and skipping it is not a missed alarm but an accept: a captured old-namespace
        // sequence above the retained durable floor is then compared to that floor as though it
        // were a durable number, admitted, and promoted into it.
        //
        // `capability_registry` holds only claims a live connection is leasing, so the two axes
        // are separated by construction: the index answers *which* key, and the lease's lifetime
        // answers *whether it is still true*. That restores the meaning
        // `ObservedSenderRegime::DurableV1` documents — "the peer authenticated on the current
        // connection" — and that `check_replay_only` relies on when it calls `observed_regime`
        // "what the current authenticated connection proves". See `capability_evidence` for why
        // the lease, rather than a second index somebody has to remember to update, is what
        // makes that hold.
        //
        // # Several live connections can claim one key, and any one of them proves it
        //
        // A key holder can authenticate under more than one spelling of itself — the #2520
        // DID-TLS checks compare the binding's DID to `from` as strings and then verify with
        // that DID's own key, so every spelling it signs for passes — and cross-dialling gives
        // one pair two connections at once. The registry reference-counts those claims, so the
        // join across them is **any**: one live connection proving the capability makes the
        // principal durable.
        //
        // That is the sound direction. The capability describes the sender's signing *store*
        // (#2510: crash-safe, monotonic, never reissued), which is per-key state rather than per
        // connection, and `any` is the only join that cannot be *suppressed* by adding a claim —
        // which is precisely what a key holder can do. `all`, first-wins and last-wins each let
        // one connection that simply does not mention the capability erase a proof another
        // connection is still making.
        //
        // Cost: one hash lookup, whatever the peer population. The predecessor walked
        // `peer_connections` per envelope, and since nothing prunes that map, a peer
        // reconnecting under one-off DIDs could grow it without bound and make every other
        // peer's traffic pay for the scan.
        let observed_regime = if self
            .capability_registry
            .proves_durable_signing_sequence(&sender_principal)
        {
            crate::replay_guard::ObservedSenderRegime::DurableV1
        } else {
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven
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
                    })
                    .or_else(|| {
                        // Our own persisted replay state could not be loaded at all
                        // (#2644). The most local fault of the lot: the guard is
                        // uninitialized, so it refuses *every* peer, and the peer whose
                        // message happened to trigger the retry did nothing whatsoever.
                        // Since #2640 the load performs storage writes while canonicalizing
                        // spelling-distinct rows, so an ordinary disk problem reaches here —
                        // and before this arm existed it arrived untyped and was scored as a
                        // replay attack against every honest sender in turn.
                        e.downcast_ref::<crate::replay_guard::ReplayStateInitializationFailed>()
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
                    // Separate from `needs_operator` because the remedy is different and the
                    // blast radius is larger: this one is not a per-peer incompatibility but a
                    // node that is refusing *all* signed traffic, and the fix is repairing
                    // storage rather than moving a binary version (#2644). It does clear by
                    // itself once the store works, so it is not folded into the message above.
                    if e.downcast_ref::<crate::replay_guard::ReplayStateInitializationFailed>()
                        .is_some()
                    {
                        error!(
                            peer = %envelope.from,
                            seq = envelope.sequence,
                            "Dropping message: this node's persisted replay state cannot be \
                             loaded, so it is refusing ALL signed traffic until the store is \
                             usable. Not peer misbehaviour — repair or replace the replay \
                             store: {reason}"
                        );
                    } else if needs_operator {
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
    use icn_identity::{Did, IdentityBundle, KeyPair};
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

    /// Same as [`create_test_context`], but with a caller-supplied replay guard.
    ///
    /// The only way to reach the handler's local-fault classifier with a *persistent* guard,
    /// which is what #2644 is about: an in-memory guard has no store to fail.
    fn create_test_context_with_guard(
        misbehavior_detector: Option<Arc<RwLock<MisbehaviorDetector>>>,
        guard: ReplayGuard,
    ) -> (ConnectionContext, Arc<AtomicUsize>) {
        let (mut ctx, _keypair, count) = create_test_context_with_own_keypair(misbehavior_detector);
        ctx.replay_guard = Arc::new(RwLock::new(guard));
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
            capability_registry: Arc::new(crate::capability_evidence::LiveCapabilityRegistry::new()),
            durable_claim: std::sync::Mutex::new(None),
            blob_registry: None,
            misbehavior_detector,
            identity_bundle,
            own_did,
            direction: crate::handlers::ConnectionDirection::Inbound,
            // Inbound: we did not choose this peer, so we expected nobody (#2533).
            expected_peer: None,
            expectation_mismatch_reported: std::sync::atomic::AtomicBool::new(false),
            pre_auth_limiter: crate::rate_limit::PreAuthBudget::Connection(
                crate::rate_limit::PreAuthRateLimiter::new(),
            ),
            // No inbound admission slot: these contexts are built directly, not by the
            // accept loop (#2547).
            admission_guard: std::sync::Mutex::new(None),
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

    /// Regression test for #2640's self-DID sub-instance.
    ///
    /// The #2506 drop above was `envelope.from == self.own_did`, and `Did` equality is string
    /// equality, so a re-spelled copy of our own DID walked straight past it — the same
    /// unsigned, un-canonicalized `from` field that gave a captured envelope a second replay
    /// window. Nothing but a key comparison closes it: the alias is a different string that
    /// verifies under the same key.
    #[tokio::test]
    async fn test_respelled_own_did_envelope_is_still_dropped() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, own_keypair, forward_count) =
            create_test_context_with_own_keypair(Some(detector.clone()));
        let own_did = own_keypair.did().clone();

        // The base16-lower spelling of our own key. `f` is multibase's base16-lower code.
        let hex: String = own_did
            .to_verifying_key()
            .unwrap()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let alias = icn_identity::Did::from_str(&format!("did:icn:f{hex}")).unwrap();
        assert_ne!(
            alias.as_str(),
            own_did.as_str(),
            "CONTROL: the alias must be a different string, or the string comparison would \
             already have caught it and this test would prove nothing"
        );

        let mut envelope = create_signed_envelope(&own_keypair, 1);
        envelope.from = alias.clone();
        assert!(
            envelope.verify(3600).is_ok(),
            "CONTROL: the re-spelled envelope must still verify, or it would be rejected by \
             the signature check rather than by the self-DID drop"
        );

        let message = create_network_message(&envelope);
        ctx.handle_signed(message.clone(), &envelope).await;
        ctx.handle_signed(message, &envelope).await;

        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            0,
            "#2640: a re-spelled own DID is still our own DID and must not be forwarded"
        );
        assert_eq!(
            ctx.replay_guard.read().await.peer_count(),
            0,
            "#2640: a re-spelled own DID must not open a remote-peer replay window"
        );
        assert!(
            detector.read().await.get_violations(&alias).is_empty(),
            "#2640: no misbehaviour may be recorded against a spelling of our own DID"
        );
        assert!(
            detector.read().await.get_violations(&own_did).is_empty(),
            "#2506: nor against its canonical spelling"
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
    // ---------------------------------------------------------------------------------
    // #2644 — capability attribution runs on the sender's key, not on its spelling
    // ---------------------------------------------------------------------------------

    /// Every base `Did::from_str` accepts other than the canonical base58btc.
    ///
    /// The same class `tests/respelled_envelope_replay.rs` drives against the guard, restated
    /// here because that file is a separate test binary and `ConnectionContext` is private to
    /// this crate. `Identity` is absent because `multibase::encode` panics on non-UTF-8 key
    /// bytes; `Base58Btc` is absent because it *is* the canonical spelling. If that file's
    /// list ever grows, this one narrows silently rather than going green wrongly — the count
    /// control below is what makes the narrowing visible.
    const ALTERNATE_SPELLINGS: [(&str, multibase::Base); 22] = [
        ("base2", multibase::Base::Base2),
        ("base8", multibase::Base::Base8),
        ("base10", multibase::Base::Base10),
        ("base16-lower", multibase::Base::Base16Lower),
        ("base16-upper", multibase::Base::Base16Upper),
        ("base32-lower", multibase::Base::Base32Lower),
        ("base32-upper", multibase::Base::Base32Upper),
        ("base32-pad-lower", multibase::Base::Base32PadLower),
        ("base32-pad-upper", multibase::Base::Base32PadUpper),
        ("base32-hex-lower", multibase::Base::Base32HexLower),
        ("base32-hex-upper", multibase::Base::Base32HexUpper),
        ("base32-hex-pad-lower", multibase::Base::Base32HexPadLower),
        ("base32-hex-pad-upper", multibase::Base::Base32HexPadUpper),
        ("base32-z", multibase::Base::Base32Z),
        ("base36-lower", multibase::Base::Base36Lower),
        ("base36-upper", multibase::Base::Base36Upper),
        ("base58-flickr", multibase::Base::Base58Flickr),
        ("base64", multibase::Base::Base64),
        ("base64-pad", multibase::Base::Base64Pad),
        ("base64-url", multibase::Base::Base64Url),
        ("base64-url-pad", multibase::Base::Base64UrlPad),
        ("base256-emoji", multibase::Base::Base256Emoji),
    ];

    /// Re-spell one key under one base, asserting the two controls that make the result mean
    /// anything: a *different string* that decodes to the *same key*.
    fn alias_in(base: multibase::Base, label: &str, canonical: &Did) -> Did {
        let key = canonical.to_verifying_key().expect("canonical DID decodes");
        let alias = Did::from_str(&format!(
            "did:icn:{}",
            multibase::encode(base, key.as_bytes())
        ))
        .unwrap_or_else(|e| {
            panic!(
                "the {label} spelling is accepted by `Did::from_str` under current policy \
                     and this suite's coverage depends on that; it was rejected: {e}"
            )
        });
        assert_ne!(
            alias.as_str(),
            canonical.as_str(),
            "CONTROL: the {label} alias must be a different string, or the case proves nothing"
        );
        assert_eq!(
            alias.to_verifying_key().unwrap().as_bytes(),
            key.as_bytes(),
            "CONTROL: the {label} alias must decode to the same key, or it is another sender"
        );
        alias
    }

    /// Rewrite only the spelling of `from` on a captured envelope. The whole attacker
    /// capability: no key material, no re-signing.
    fn respell(captured: &SignedEnvelope, alias: &Did) -> SignedEnvelope {
        let mut forged = captured.clone();
        forged.from = alias.clone();
        assert_eq!(
            forged.signature, captured.signature,
            "CONTROL: the attacker must not have touched the signature bytes"
        );
        assert!(
            forged.verify(3600).is_ok(),
            "CONTROL: the re-spelled envelope must still verify, or the signature layer would \
             already be handling this"
        );
        forged
    }

    /// The row `handle_hello` writes once a peer has authenticated.
    ///
    /// `handlers::hello` stores `connections.insert(from.clone(), PeerConnectionInfo { did:
    /// from.clone(), peer_capabilities: common_caps, .. })` — keyed by the *wire spelling* of
    /// `from`, after the three #2520 DID-TLS checks. Seeded directly rather than through a
    /// live QUIC Hello, which would add a handshake to every case below without changing the
    /// row under test; the sweeps compensate by never assuming *which* spelling that is —
    /// they drive the stored spelling across the whole accepted class in both directions, so
    /// no case rests on a claim about what `handle_hello` happened to write.
    fn authenticated_row(
        spelling: &Did,
        caps: crate::CapabilityFlags,
    ) -> crate::actor::PeerConnectionInfo {
        crate::actor::PeerConnectionInfo {
            did: spelling.clone(),
            negotiated_version: 1,
            peer_capabilities: caps,
            peer_software: "seeded".to_string(),
            x25519_key: [0u8; 32],
            ml_dsa_public: None,
            ml_kem_public: None,
        }
    }

    /// What kind of `peer_connections` row this is, and whether a live connection is still
    /// claiming the capability behind it — the axis #2644 is about.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Row {
        /// A connection that authenticated and is still up: the cache row *and* a held lease,
        /// exactly what `handle_hello` leaves behind while the peer is here.
        Live,
        /// The peer authenticated and then went away. The cache row survives — nothing removes
        /// it — but the lease its connection held has been released.
        Closed,
        /// A cache row with no connection behind it, ever: what
        /// `NetworkHandle::restore_state` writes at startup from a snapshot.
        Snapshot,
    }

    /// Put one peer into the state `kind` describes, returning the lease so the caller can keep
    /// the connection "up" for the duration of the case.
    ///
    /// The cache row is written for **every** kind, including the ones that must not count. That
    /// is deliberate: it is what makes these tests evidence that the handler stopped reading the
    /// cache, rather than evidence that the cache happened to be empty.
    async fn install_row(
        ctx: &ConnectionContext,
        spelling: &Did,
        caps: crate::CapabilityFlags,
        kind: Row,
    ) -> Option<crate::capability_evidence::LiveCapabilityClaim> {
        ctx.peer_connections
            .write()
            .await
            .insert(spelling.clone(), authenticated_row(spelling, caps));

        if kind == Row::Snapshot {
            // Restored from disk. No connection ever authenticated in this process, so there is
            // nothing to have claimed anything.
            return None;
        }

        // What `handle_hello` does once the #2520 DID-TLS checks have passed: claim the durable
        // regime for the key it just authenticated, keyed by principal rather than spelling.
        let claim = caps
            .contains(DURABLE)
            .then(|| crate::replay_guard::SenderPrincipal::from_did(spelling).ok())
            .flatten()
            .map(|principal| ctx.capability_registry.claim_durable(principal));

        match kind {
            Row::Live => claim,
            // The connection ends here. Returning `None` drops the lease, which is the release —
            // the claim really was made and really was given back, rather than never made.
            Row::Closed => None,
            Row::Snapshot => unreachable!("returned above"),
        }
    }

    /// Which regime the handler attributed to `envelope`, read off the only difference that
    /// is visible from outside it.
    ///
    /// On an **empty** replay window the two attributions are behaviourally opposite:
    /// `DurableV1` is `(LegacyOrUnproven, DurableV1)`, the #2517 namespace change, which
    /// installs the retirement hold and refuses the message; `LegacyOrUnproven` is steady
    /// state, which accepts it and forwards it. Forwarding is therefore an exact oracle for
    /// the attribution, and it is the security-relevant one: the captured envelope either
    /// reaches the application or it does not.
    ///
    /// A fresh context per call, because the empty window is half of the property.
    ///
    /// Each row states not just what it advertises but whether its connection is still up
    /// (#2644), because that is now half of what the handler reads. `Row::Live` is the shape
    /// `handle_hello` leaves behind for a peer that is still here.
    async fn attributed_regime(
        rows: &[(Did, crate::CapabilityFlags, Row)],
        envelope: &SignedEnvelope,
    ) -> crate::replay_guard::ObservedSenderRegime {
        let (ctx, forwarded) = create_test_context(None);
        let mut held = Vec::new();
        for (spelling, caps, kind) in rows {
            held.push(install_row(&ctx, spelling, *caps, *kind).await);
        }
        ctx.handle_signed(create_network_message(envelope), envelope)
            .await;
        match forwarded.load(Ordering::SeqCst) {
            0 => crate::replay_guard::ObservedSenderRegime::DurableV1,
            1 => crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            n => panic!("one envelope cannot be forwarded {n} times"),
        }
    }

    const DURABLE: crate::CapabilityFlags = crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE;
    const NOT_DURABLE: crate::CapabilityFlags = crate::CapabilityFlags::E2E_ENCRYPTION;

    /// THE REPORTED BYPASS (#2644).
    ///
    /// A sender that has authenticated and proved `DURABLE_SIGNING_SEQUENCE` under one
    /// spelling; a still-fresh envelope captured from its *pre-upgrade* numbering; and a
    /// replay window this receiver has never established anything in. Re-spelling `from`
    /// alone made the capability lookup miss, which read the sender as `LegacyOrUnproven`,
    /// which on an empty window is steady state — so the captured envelope was accepted and
    /// forwarded instead of entering the retirement hold that exists precisely because
    /// old-namespace envelopes can still be inside their validity window.
    ///
    /// Two distinct captured sequences, because one refusal could be an artefact of a hold
    /// installed by something else; the bypass forwards *both*.
    #[tokio::test]
    async fn a_respelled_envelope_cannot_launder_a_durable_sender_into_the_legacy_steady_state() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let (ctx, forwarded) = create_test_context(Some(detector.clone()));
        let sender = KeyPair::generate().unwrap();

        // Post-Hello authenticated state under the spelling the peer used, on a connection
        // the peer is still holding: capability row and live session together, which is the
        // only shape that counts as current evidence (#2644).
        let _live = install_row(&ctx, sender.did(), DURABLE, Row::Live).await;

        let alias = alias_in(multibase::Base::Base16Lower, "base16-lower", sender.did());
        for captured_sequence in [42, 43] {
            let captured = create_signed_envelope(&sender, captured_sequence);
            let forged = respell(&captured, &alias);
            ctx.handle_signed(create_network_message(&forged), &forged)
                .await;
        }

        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            0,
            "a captured pre-upgrade envelope must not reach the application merely because \
             `from` was re-spelled: the sender proved DURABLE_SIGNING_SEQUENCE, so an empty \
             window owes it the #2517 retirement hold"
        );

        // The refusal is the migration hold, not a replay verdict: the peer did nothing, and
        // the spelling the attacker chose must not become a reputation lever against it
        // either. Checked under both spellings because the detector is keyed by
        // `envelope.from`, which the attacker picked.
        for did in [sender.did(), &alias] {
            assert!(
                detector.read().await.get_violations(did).is_empty(),
                "the hold is a local migration, not peer misbehaviour ({did})"
            );
        }
    }

    /// SAME-SPELLING CONTROL. The stored spelling must reach the identical security result,
    /// or the test above would pass on a build that simply refused everything re-spelled.
    #[tokio::test]
    async fn the_stored_spelling_reaches_the_same_security_result() {
        let (ctx, forwarded) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();
        let _live = install_row(&ctx, sender.did(), DURABLE, Row::Live).await;

        for captured_sequence in [42, 43] {
            let captured = create_signed_envelope(&sender, captured_sequence);
            ctx.handle_signed(create_network_message(&captured), &captured)
                .await;
        }

        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            0,
            "the canonical spelling already entered the retirement hold before this fix; it \
             must still"
        );
    }

    /// ALL ACCEPTED SPELLINGS, IN BOTH DIRECTIONS.
    ///
    /// The invariant is not "base16 is handled". It is that the authenticated capability a
    /// sender proved is invariant under every respelling that leaves its key unchanged — so
    /// the *stored* spelling is swept too, not just the envelope's. A peer can authenticate
    /// under any of these: the #2520 DID-TLS checks compare the binding's DID to `from` as
    /// strings and then verify with that DID's own key, so a key holder can bind, and be
    /// stored under, any spelling of itself.
    #[tokio::test]
    async fn every_accepted_spelling_selects_the_same_durable_capability() {
        assert_eq!(
            ALTERNATE_SPELLINGS.len(),
            22,
            "the accepted spelling class is 22 alternates plus canonical; a smaller list here \
             is narrower coverage, not a smaller attack surface"
        );

        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (label, base) in ALTERNATE_SPELLINGS {
            let alias = alias_in(base, label, &canonical);
            let captured = create_signed_envelope(&sender, 42);

            // Stored canonical, envelope re-spelled — the reported direction.
            assert_eq!(
                attributed_regime(
                    &[(canonical.clone(), DURABLE, Row::Live)],
                    &respell(&captured, &alias)
                )
                .await,
                crate::replay_guard::ObservedSenderRegime::DurableV1,
                "{label}: a re-spelled envelope must still find the capability its sender proved"
            );

            // Stored under the alias, envelope canonical — the same miss, mirrored.
            assert_eq!(
                attributed_regime(&[(alias.clone(), DURABLE, Row::Live)], &captured).await,
                crate::replay_guard::ObservedSenderRegime::DurableV1,
                "{label}: a peer that authenticated under {label} proved the same capability"
            );
        }
    }

    /// DIFFERENT-KEY CONTROL. The join must not be "some peer somewhere is durable".
    #[tokio::test]
    async fn a_different_key_never_inherits_a_durable_peers_capability() {
        let durable = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap();
        assert_ne!(
            durable.did().to_verifying_key().unwrap().as_bytes(),
            other.did().to_verifying_key().unwrap().as_bytes(),
            "CONTROL: two generated keys must differ"
        );

        assert_eq!(
            attributed_regime(
                &[(durable.did().clone(), DURABLE, Row::Live)],
                &create_signed_envelope(&other, 1)
            )
            .await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "a durable row for one principal says nothing about another"
        );
    }

    /// LEGACY-ONLY CONTROL. An authenticated row that genuinely lacks the capability stays
    /// unproven — the fix must widen the *identity*, never invent evidence.
    #[tokio::test]
    async fn an_authenticated_row_without_the_capability_stays_unproven_under_every_spelling() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let captured = create_signed_envelope(&sender, 1);

        assert_eq!(
            attributed_regime(&[(canonical.clone(), NOT_DURABLE, Row::Live)], &captured).await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "no capability, no durable attribution"
        );

        let alias = alias_in(multibase::Base::Base32Lower, "base32-lower", &canonical);
        assert_eq!(
            attributed_regime(&[(alias, NOT_DURABLE, Row::Live)], &captured).await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "matching a principal is not evidence of what that principal advertised"
        );
    }

    /// NO CAPABILITY ENTRY AT ALL — the pre-existing safe answer, unchanged.
    #[tokio::test]
    async fn a_sender_with_no_authenticated_row_is_still_unproven() {
        let sender = KeyPair::generate().unwrap();
        assert_eq!(
            attributed_regime(&[], &create_signed_envelope(&sender, 1)).await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "an empty map proves nothing about anyone"
        );
    }

    /// MULTIPLE SPELLINGS, MULTIPLE ROWS, ONE PRINCIPAL.
    ///
    /// Both rows are the principal's *own* authenticated claims — #2520 means no row exists
    /// for a key its holder did not prove possession of on a live connection — so combining
    /// them crosses no trust boundary, and `DURABLE_SIGNING_SEQUENCE` describes the sender's
    /// signing store (#2510: crash-safe, monotonic, never reissued) rather than a QUIC
    /// connection. The join is therefore "any authenticated row for this principal proved
    /// it", which is the only rule that cannot be *suppressed* by adding a row — and adding
    /// rows is exactly what a key holder (Hello under a second spelling) and an attacker
    /// (re-spelling `from`) can do.
    ///
    /// The envelope is spelled canonically and every row is an alias, so no textual lookup
    /// can reach either row: what is under test is the join, not the identity fix alone.
    #[tokio::test]
    async fn one_durable_row_makes_the_principal_durable_whatever_else_it_authenticated_as() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let durable_row = alias_in(multibase::Base::Base64, "base64", &canonical);
        let legacy_row = alias_in(multibase::Base::Base32Lower, "base32-lower", &canonical);
        let captured = create_signed_envelope(&sender, 7);

        for (order, rows) in [
            (
                "durable inserted first",
                vec![
                    (durable_row.clone(), DURABLE, Row::Live),
                    (legacy_row.clone(), NOT_DURABLE, Row::Live),
                ],
            ),
            (
                "durable inserted second",
                vec![
                    (legacy_row.clone(), NOT_DURABLE, Row::Live),
                    (durable_row.clone(), DURABLE, Row::Live),
                ],
            ),
        ] {
            assert_eq!(
                attributed_regime(&rows, &captured).await,
                crate::replay_guard::ObservedSenderRegime::DurableV1,
                "{order}: a proof this principal gave cannot be erased by a row it also gave"
            );
        }
    }

    /// The join must not be *whichever row iteration reached first*.
    ///
    /// Insertion order is not the lever here — `HashMap` iteration order is a function of the
    /// hasher, not of insertion — so this rotates *which* of 22 same-principal rows carries
    /// the capability, rebuilding the map each time. A rule that reads one arbitrary row and
    /// returns its capability has a 1-in-22 chance of surviving each rotation, so it does not
    /// survive the sweep; `any` is order-independent by construction and survives all of it.
    #[tokio::test]
    async fn the_durable_row_is_found_wherever_iteration_happens_to_put_it() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let captured = create_signed_envelope(&sender, 3);
        let aliases: Vec<Did> = ALTERNATE_SPELLINGS
            .iter()
            .map(|(label, base)| alias_in(*base, label, &canonical))
            .collect();

        for (durable_at, (label, _)) in ALTERNATE_SPELLINGS.iter().enumerate() {
            let rows: Vec<(Did, crate::CapabilityFlags, Row)> = aliases
                .iter()
                .enumerate()
                .map(|(i, alias)| {
                    (
                        alias.clone(),
                        if i == durable_at {
                            DURABLE
                        } else {
                            NOT_DURABLE
                        },
                        Row::Live,
                    )
                })
                .collect();
            assert_eq!(
                attributed_regime(&rows, &captured).await,
                crate::replay_guard::ObservedSenderRegime::DurableV1,
                "the capability was proved on the {label} row; which row iteration visits \
                 first is not the protocol's to choose"
            );
        }
    }

    /// EXISTING DURABLE WINDOW — the older row-#57 case, preserved.
    ///
    /// Once a floor exists, the floor is what rejects the re-spelled replay, in either regime
    /// arm. That is the redundancy the N2-A0 inventory described; this pins it, and the
    /// empty-window case above is the half where the redundancy is absent.
    #[tokio::test]
    async fn a_respelled_replay_below_an_established_durable_floor_is_still_rejected() {
        let (ctx, forwarded) = create_test_context(None);
        let sender = KeyPair::generate().unwrap();
        let _live = install_row(&ctx, sender.did(), DURABLE, Row::Live).await;

        // Establish a durable floor directly: the migration hold is not what is under test.
        let accepted = create_signed_envelope(&sender, 9);
        ctx.replay_guard
            .write()
            .await
            .check_replay_only(
                &accepted,
                crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            )
            .expect("an empty window accepts the sender's first sequence");

        let alias = alias_in(multibase::Base::Base36Lower, "base36-lower", sender.did());
        let forged = respell(&accepted, &alias);
        ctx.handle_signed(create_network_message(&forged), &forged)
            .await;

        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            0,
            "a re-spelled replay of an already-accepted sequence must stay rejected"
        );
    }

    /// A stored DID that names no key at all is evidence about nobody.
    ///
    /// `Did::from_anchor_id` bypasses validation, and roughly half of the 32-byte strings it
    /// wraps do not decompress to an Ed25519 point (N2-A0 inventory §10.1) — so a `Did` that
    /// names no principal is representable, and `HashMap<Did, PeerConnectionInfo>` will hold
    /// one. Neither production writer can put one there today (`handle_hello` binds a `from`
    /// that `Did::deserialize` already validated, and the snapshot restore goes through
    /// `Did::from_str`, which decompresses), so this pins the skip as defence in depth and as
    /// the direction that cannot invent a match: a DID outside every equivalence class is not
    /// an alias of the sender, and skipping it costs nothing the textual lookup had, because
    /// `envelope.from` always decodes.
    #[tokio::test]
    async fn a_stored_did_that_decodes_to_no_key_is_skipped_rather_than_matched() {
        let undecodable = (0u8..=255)
            .map(|byte| Did::from_anchor_id(&[byte; 32]))
            .find(|did| did.to_verifying_key().is_err())
            .expect(
                "CONTROL: some anchor id must fail to decompress to an Ed25519 point, or this \
                 case is vacuous",
            );

        let sender = KeyPair::generate().unwrap();
        assert_eq!(
            attributed_regime(
                &[
                    (undecodable, DURABLE, Row::Live),
                    (sender.did().clone(), NOT_DURABLE, Row::Live)
                ],
                &create_signed_envelope(&sender, 1)
            )
            .await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "an undecodable row must neither match nor lend its capability to anyone"
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
        let _live = install_row(
            &ctx,
            &migrating_did,
            crate::CapabilityFlags::E2E_ENCRYPTION
                | crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE,
            Row::Live,
        )
        .await;

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

    /// A store whose `flush` fails, so replay-state initialization fails with it.
    struct FlushFailsStore {
        inner: Arc<icn_store::SledStore>,
        failing: std::sync::atomic::AtomicBool,
    }

    impl icn_store::Store for FlushFailsStore {
        fn get(&self, key: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
            self.inner.get(key)
        }
        fn put(&self, key: &[u8], value: &[u8]) -> anyhow::Result<()> {
            self.inner.put(key, value)
        }
        fn delete(&self, key: &[u8]) -> anyhow::Result<()> {
            self.inner.delete(key)
        }
        fn scan(&self, prefix: &[u8]) -> anyhow::Result<Vec<(Vec<u8>, Vec<u8>)>> {
            self.inner.scan(prefix)
        }
        fn flush(&self) -> anyhow::Result<()> {
            if self.failing.load(Ordering::SeqCst) {
                anyhow::bail!("simulated disk failure during flush");
            }
            self.inner.flush().map(|_| ())
        }
        fn get_replica_metadata(
            &self,
            hash: &icn_store::ContentHash,
        ) -> anyhow::Result<Option<icn_store::ReplicaMetadata>> {
            self.inner.get_replica_metadata(hash)
        }
        fn put_replica_metadata(&self, meta: &icn_store::ReplicaMetadata) -> anyhow::Result<()> {
            self.inner.put_replica_metadata(meta)
        }
        fn list_replica_hashes(&self) -> anyhow::Result<Vec<icn_store::ContentHash>> {
            self.inner.list_replica_hashes()
        }
    }

    /// Seed two spellings of one sender, so replay-state initialization must write, flush and
    /// delete while collapsing them onto one canonical key (#2640).
    fn seed_two_spellings(store: &dyn icn_store::Store, sender: &KeyPair) {
        let canonical = sender.did().clone();
        let hex: String = canonical
            .to_verifying_key()
            .unwrap()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let alias = icn_identity::Did::from_str(&format!("did:icn:f{hex}")).unwrap();
        assert_ne!(
            alias.as_str(),
            canonical.as_str(),
            "CONTROL: the alias must be a different spelling of the same key"
        );
        for (did, seq) in [(&canonical, 10u64), (&alias, 11u64)] {
            let mut key = b"replay_max_seq:".to_vec();
            key.extend_from_slice(did.as_str().as_bytes());
            let value = serde_json::json!({
                "max_seq": seq,
                "updated_at_ms": 0u64,
                "semantic_version": 1u32,
                // The unproven namespace, which is what a context with no `peer_connections`
                // entry resolves the live sender to. Tagging these rows durable instead would
                // make the repaired-store control a `SenderRegimeDowngrade` — a different
                // local fault, and not the one under test.
                "sender_regime": 0u32,
            });
            store
                .put(&key, &serde_json::to_vec(&value).unwrap())
                .unwrap();
        }
    }

    /// #2644 — a local replay-state initialization failure must never reach the peer-ban path.
    ///
    /// The handler boundary, not the guard's unit boundary, because the defect lived in the
    /// join between them: `ReplayGuard::check_replay_only` propagated an untyped storage
    /// error, and this function classifies local faults by downcasting to the replay-state
    /// error types. Nothing matched, so the fall-through ran — `warn!("Replay attack
    /// detected")` and `Violation::ReplayAttack` against a peer that had done nothing.
    ///
    /// The guard deliberately stays uninitialized after a failed load and retries on every
    /// message, so this is not a single stray score: every honest peer is scored on every
    /// message until the store is repaired, which is exactly the automatic-ban input
    /// `MisbehaviorDetector` consumes. The message is looped for that reason.
    #[tokio::test]
    async fn a_local_replay_state_initialization_failure_is_never_scored_as_a_replay_attack() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let sled = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();
        seed_two_spellings(sled.as_ref(), &sender);

        let broken = Arc::new(FlushFailsStore {
            inner: sled.clone(),
            failing: std::sync::atomic::AtomicBool::new(true),
        });
        let guard = ReplayGuard::new_persistent(300, 3600, broken.clone());
        let (ctx, forward_count) = create_test_context_with_guard(Some(detector.clone()), guard);

        for round in 0..5 {
            let envelope = create_signed_envelope(&sender, 100 + round);
            ctx.handle_signed(create_network_message(&envelope), &envelope)
                .await;
        }

        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            0,
            "the guard must still fail closed: nothing may be delivered while replay state \
             cannot be loaded"
        );
        {
            let detector_guard = detector.read().await;
            let violations = detector_guard.get_violations(sender.did());
            assert!(
                violations.is_empty(),
                "our own storage failure must not be scored against the peer; got \
                 {violations:?}"
            );
        }

        // CONTROL: the guard really was uninitialized on every one of those messages, so the
        // assertion above is about the classifier and not about a guard that quietly
        // succeeded.
        assert!(
            !ctx.replay_guard.read().await.is_initialized(),
            "CONTROL: the load must have failed on every message"
        );

        // CONTROL: repairing the store proves the same peer, the same handler and the same
        // detector do deliver traffic — so "no violations" above is not "nothing works".
        broken.failing.store(false, Ordering::SeqCst);
        let fresh = create_signed_envelope(&sender, 200);
        ctx.handle_signed(create_network_message(&fresh), &fresh)
            .await;
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            1,
            "CONTROL: once the store is repaired the retry must initialize and deliver"
        );
    }

    /// The other half of the control: a genuine replay is still scored as one.
    ///
    /// Without it, every assertion in the test above is satisfiable by a handler that stopped
    /// recording `ReplayAttack` altogether.
    #[tokio::test]
    async fn a_genuine_replay_is_still_scored_after_the_local_fault_exemption() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let sled = Arc::new(icn_store::SledStore::temporary().unwrap());
        let sender = KeyPair::generate().unwrap();

        let healthy = Arc::new(FlushFailsStore {
            inner: sled.clone(),
            failing: std::sync::atomic::AtomicBool::new(false),
        });
        let guard = ReplayGuard::new_persistent(300, 3600, healthy);
        let (ctx, forward_count) = create_test_context_with_guard(Some(detector.clone()), guard);

        let envelope = create_signed_envelope(&sender, 1);
        let message = create_network_message(&envelope);
        ctx.handle_signed(message.clone(), &envelope).await;
        assert_eq!(forward_count.load(Ordering::SeqCst), 1);

        ctx.handle_signed(message, &envelope).await;
        assert_eq!(
            forward_count.load(Ordering::SeqCst),
            1,
            "the replay must not be delivered"
        );

        let detector_guard = detector.read().await;
        let violations = detector_guard.get_violations(sender.did());
        assert!(
            violations.iter().any(|v| matches!(
                v.violation,
                icn_security::Violation::ReplayAttack { sequence: 1, .. }
            )),
            "a real replay must still reach the ReplayAttack path; got {violations:?}"
        );
    }

    // ==================================================================================
    // #2644 (second round): current-versus-historical capability evidence
    // ==================================================================================

    /// A monotonic clock the test drives, so the 600s retirement horizon can be crossed
    /// without a 600s test. See `replay_guard`'s own `TestClock` for why the production
    /// horizon rather than a scaled-down one is the right thing to cross.
    struct HarnessClock {
        nanos: std::sync::atomic::AtomicU64,
    }

    impl HarnessClock {
        fn new() -> Arc<Self> {
            Arc::new(HarnessClock {
                nanos: std::sync::atomic::AtomicU64::new(0),
            })
        }

        fn advance(&self, by: std::time::Duration) {
            self.nanos.fetch_add(by.as_nanos() as u64, Ordering::SeqCst);
        }
    }

    impl crate::replay_guard::MonotonicClock for HarnessClock {
        fn elapsed(&self) -> std::time::Duration {
            std::time::Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
        }
    }

    /// `max_clock_skew` is 300s in these contexts, so the retirement horizon is 600s.
    const RETIREMENT_HORIZON: std::time::Duration = std::time::Duration::from_secs(601);

    /// The keyspace agreement the whole pairing rests on.
    ///
    /// `peer_connections` is keyed by `Did` and the session map by `from.to_string()`. If
    /// those two ever stop producing the same bytes, every capability row silently loses its
    /// liveness partner and every durable peer reads as `LegacyOrUnproven` — a fail-closed
    /// break, but a total one. Asserted across the whole accepted spelling class rather than
    /// on one DID, because the failure would be per-encoding.
    #[test]
    fn the_two_maps_key_the_same_peer_the_same_way() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        for (label, base) in ALTERNATE_SPELLINGS {
            let alias = alias_in(base, label, &canonical);
            assert_eq!(
                alias.to_string(),
                alias.as_str(),
                "{label}: the session map's key and the capability map's key must be the same \
                 bytes, or the row and its session can never be paired"
            );
        }
        assert_eq!(canonical.to_string(), canonical.as_str());
    }

    /// Drive one peer to an established `DurableV1` window with a durable floor of 10, using
    /// only the handler's own path.
    ///
    /// There is no shortcut: first durable evidence costs a retirement hold, and the
    /// promotion only lands once the horizon has passed.
    async fn establish_durable_floor_10(
        ctx: &ConnectionContext,
        forwarded: &Arc<AtomicUsize>,
        clock: &HarnessClock,
        sender: &KeyPair,
    ) {
        let first = create_signed_envelope(sender, 10);
        ctx.handle_signed(create_network_message(&first), &first)
            .await;
        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            0,
            "precondition: first durable evidence must cost a retirement hold"
        );
        clock.advance(RETIREMENT_HORIZON);
        ctx.handle_signed(create_network_message(&first), &first)
            .await;
        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            1,
            "precondition: the promotion must land and establish the durable floor at 10"
        );
    }

    /// THE REPORTED BYPASS, SECOND ROUND (#2644).
    ///
    /// A peer proves `DURABLE_SIGNING_SEQUENCE` under spelling A and establishes a durable
    /// window with a floor of 10. It then closes that connection and comes back under
    /// spelling B *without* the capability — an operator rollback, or a signing store it no
    /// longer has. That is exactly the `(DurableV1, LegacyOrUnproven)` state, and the guard
    /// refuses it precisely because a number from the peer's old namespace cannot be compared
    /// with a durable floor.
    ///
    /// Nothing removes A's row when A's connection ends, and B's Hello only replaces B's own
    /// key, so both rows sit in the map at once. Joining on principal alone therefore still
    /// found A's abandoned durable row, and the downgrade was never seen — so a captured
    /// old-namespace envelope numbered *above* the retained floor was compared to that floor
    /// as if it were a durable number, and admitted.
    ///
    /// Sequence 100 against a floor of 10: high enough that every replay check below the
    /// regime match passes, so the regime match is the only thing that can refuse it.
    #[tokio::test]
    async fn a_stale_alias_row_cannot_keep_a_rolled_back_sender_durable() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let clock = HarnessClock::new();
        let (ctx, forwarded) = create_test_context_with_guard(
            Some(detector.clone()),
            ReplayGuard::new(300, 3600).with_clock(clock.clone()),
        );
        let sender = KeyPair::generate().unwrap();
        let a = sender.did().clone();
        let b = alias_in(multibase::Base::Base16Lower, "base16-lower", &a);

        // The peer authenticates as A, advertising the durable capability, and establishes
        // its window.
        let live_a = install_row(&ctx, &a, DURABLE, Row::Live)
            .await
            .expect("A has a live session");
        establish_durable_floor_10(&ctx, &forwarded, &clock, &sender).await;

        // A disconnects normally. Nothing removes its capability row.
        drop(live_a);
        {
            let rows = ctx.peer_connections.read().await;
            let stale = rows.get(&a).expect(
                "peer_connections is a cache, not a live-session registry: the connection \
                 handler returns on both the application-close and the error path without \
                 removing anything",
            );
            assert!(
                stale
                    .peer_capabilities
                    .contains(crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE),
                "the abandoned row still advertises DurableV1, which is what made it usable"
            );
        }

        // The same key comes back under a different accepted spelling, without the capability.
        // A live legacy connection holds no lease: the registry tracks durable claims only,
        // and "connected but not claiming" is exactly what a rolled-back peer looks like.
        let _live_b = install_row(&ctx, &b, NOT_DURABLE, Row::Live).await;
        {
            let rows = ctx.peer_connections.read().await;
            assert!(rows.contains_key(&a) && rows.contains_key(&b));
            assert_eq!(
                crate::replay_guard::SenderPrincipal::from_did(&a).unwrap(),
                crate::replay_guard::SenderPrincipal::from_did(&b).unwrap(),
                "CONTROL: both rows must name one principal, or the case is about two peers"
            );
        }

        // A still-fresh envelope captured from the peer's old numbering, above the floor.
        let captured = create_signed_envelope(&sender, 100);
        let forged = respell(&captured, &b);
        let before = forwarded.load(Ordering::SeqCst);
        ctx.handle_signed(create_network_message(&forged), &forged)
            .await;

        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            before,
            "a captured old-namespace sequence above the durable floor must not reach the \
             application: the peer's *current* connection does not advertise the durable \
             regime, so this is the (DurableV1, LegacyOrUnproven) downgrade, and an abandoned \
             row under another spelling is not evidence about the peer's numbering now"
        );

        // The refusal must be the typed local downgrade, not a replay verdict: an operator
        // rollback is our incompatibility, not the peer's misbehaviour. Checked under both
        // spellings because the detector is keyed by `envelope.from`.
        for did in [&a, &b] {
            assert!(
                detector.read().await.get_violations(did).is_empty(),
                "a sender-regime downgrade is a local migration fault, not peer misbehaviour \
                 ({did})"
            );
        }
    }

    /// CONTROL: the same rollback with no stale row at all.
    ///
    /// Without this, the test above could pass on a build that refused the envelope for some
    /// unrelated reason. The stale row must make *no difference*: same window, same current
    /// Hello, same refusal.
    #[tokio::test]
    async fn the_same_rollback_is_refused_when_no_stale_row_exists() {
        let detector = Arc::new(RwLock::new(MisbehaviorDetector::new(
            MisbehaviorThresholds::default(),
        )));
        let clock = HarnessClock::new();
        let (ctx, forwarded) = create_test_context_with_guard(
            Some(detector.clone()),
            ReplayGuard::new(300, 3600).with_clock(clock.clone()),
        );
        let sender = KeyPair::generate().unwrap();
        let a = sender.did().clone();
        let b = alias_in(multibase::Base::Base16Lower, "base16-lower", &a);

        let live_a = install_row(&ctx, &a, DURABLE, Row::Live)
            .await
            .expect("A has a live session");
        establish_durable_floor_10(&ctx, &forwarded, &clock, &sender).await;
        drop(live_a);

        // The only difference from the case above.
        ctx.peer_connections.write().await.remove(&a);

        let _live_b = install_row(&ctx, &b, NOT_DURABLE, Row::Live).await;
        let captured = create_signed_envelope(&sender, 100);
        let forged = respell(&captured, &b);
        let before = forwarded.load(Ordering::SeqCst);
        ctx.handle_signed(create_network_message(&forged), &forged)
            .await;

        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            before,
            "the downgrade refusal is the baseline this fix restores"
        );
        for did in [&a, &b] {
            assert!(detector.read().await.get_violations(did).is_empty());
        }
    }

    /// OVER-CORRECTION CONTROL. A peer that is *genuinely* still durable must still be served.
    ///
    /// The fix narrows what counts as evidence, and a narrowing that goes too far is invisible
    /// to every test above: they all assert a refusal, and refusing everything satisfies them
    /// all. This is the one that fails if the liveness rule stops recognising a peer that has
    /// done nothing wrong — same window, same floor, same sequence 100, and the peer simply
    /// stays connected under the spelling it proved on.
    #[tokio::test]
    async fn a_peer_that_is_still_durable_is_still_served() {
        let clock = HarnessClock::new();
        let (ctx, forwarded) = create_test_context_with_guard(
            None,
            ReplayGuard::new(300, 3600).with_clock(clock.clone()),
        );
        let sender = KeyPair::generate().unwrap();

        let _live = install_row(&ctx, sender.did(), DURABLE, Row::Live)
            .await
            .expect("a live session");
        establish_durable_floor_10(&ctx, &forwarded, &clock, &sender).await;

        let next = create_signed_envelope(&sender, 100);
        ctx.handle_signed(create_network_message(&next), &next)
            .await;
        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            2,
            "a durable peer that never went away must keep being read as durable; requiring a \
             live session must not become requiring a *new* one"
        );
    }

    /// SNAPSHOT-RESTORED ROW. Historical state from disk is not an observation.
    ///
    /// `NetworkHandle::restore_state` recreates `peer_connections` at startup from a snapshot,
    /// capability bits included, before any Hello has happened in this process — see
    /// `actor::tests::a_snapshot_restored_row_is_a_capability_claim_with_no_session`. A row in
    /// that state names a real principal and records something that was once true, and it has
    /// no bearing on what the sender's numbering is now.
    ///
    /// Strictly stronger than the reported case: reaching it needs no key at all, only a
    /// captured envelope and a receiver that restarted.
    #[tokio::test]
    async fn a_snapshot_restored_row_is_not_current_evidence() {
        let sender = KeyPair::generate().unwrap();
        assert_eq!(
            attributed_regime(
                &[(sender.did().clone(), DURABLE, Row::Snapshot)],
                &create_signed_envelope(&sender, 1)
            )
            .await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "a cache row restored from disk was authenticated once, in some previous process; \
             it is not the current connection proving anything"
        );
    }

    /// CLOSED SESSION HANDLE. Map occupancy is not liveness.
    ///
    /// A peer's disconnect leaves its session entry behind until something replaces it
    /// (#2504), so the entry existing proves only that the peer was once here. This is the
    /// same distinction `connected_peer_endpoints` already draws on the same map.
    #[tokio::test]
    async fn a_closed_session_entry_is_not_current_evidence() {
        let sender = KeyPair::generate().unwrap();
        assert_eq!(
            attributed_regime(
                &[(sender.did().clone(), DURABLE, Row::Closed)],
                &create_signed_envelope(&sender, 1)
            )
            .await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "a session entry whose connection has closed is a record of a peer that left"
        );
    }

    /// EXACT PARTNER COUPLING — the trap in the obvious fix.
    ///
    /// The tempting shortcut is to ask whether *this principal* has any live session and, if
    /// so, treat its rows as current. That rebuilds the reported bug exactly: the abandoned
    /// durable row A and the live legacy connection B decode to one key, so B's liveness would
    /// vouch for A's capability and the downgrade would vanish again.
    ///
    /// A row's evidence has to live and die with its own session. Run for a closed partner and
    /// for no partner at all, since a shortcut could be written either way round.
    #[tokio::test]
    async fn a_live_session_under_one_spelling_does_not_revive_another_spellings_row() {
        let sender = KeyPair::generate().unwrap();
        let a = sender.did().clone();
        let b = alias_in(multibase::Base::Base32Lower, "base32-lower", &a);
        let captured = create_signed_envelope(&sender, 5);

        for stale in [Row::Closed, Row::Snapshot] {
            assert_eq!(
                attributed_regime(
                    &[
                        (a.clone(), DURABLE, stale),
                        (b.clone(), NOT_DURABLE, Row::Live)
                    ],
                    &captured
                )
                .await,
                crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
                "{stale:?}: the live connection is B's and says nothing durable; A's row is \
                 not made current by sharing a key with it"
            );
        }
    }

    /// The mirror: a live durable row is not weakened by a stale legacy one.
    ///
    /// The currency filter removes rows from the disjunction, and a filter that removed the
    /// wrong ones would show up here rather than in any refusal test.
    #[tokio::test]
    async fn a_stale_legacy_row_does_not_erase_a_live_durable_proof() {
        let sender = KeyPair::generate().unwrap();
        let a = sender.did().clone();
        let b = alias_in(multibase::Base::Base64, "base64", &a);
        let captured = create_signed_envelope(&sender, 5);

        for order in 0..2 {
            let rows = if order == 0 {
                vec![
                    (a.clone(), DURABLE, Row::Live),
                    (b.clone(), NOT_DURABLE, Row::Closed),
                ]
            } else {
                vec![
                    (b.clone(), NOT_DURABLE, Row::Closed),
                    (a.clone(), DURABLE, Row::Live),
                ]
            };
            assert_eq!(
                attributed_regime(&rows, &captured).await,
                crate::replay_guard::ObservedSenderRegime::DurableV1,
                "insertion order {order}: a connection the peer has closed cannot retract a \
                 proof another connection is still making"
            );
        }
    }

    /// DIFFERENT-KEY CONTROL, liveness edition. A live durable session for someone else is
    /// still someone else's.
    #[tokio::test]
    async fn a_live_durable_session_for_another_principal_does_not_bleed() {
        let durable = KeyPair::generate().unwrap();
        let other = KeyPair::generate().unwrap();
        assert_eq!(
            attributed_regime(
                &[
                    (durable.did().clone(), DURABLE, Row::Live),
                    (other.did().clone(), NOT_DURABLE, Row::Live)
                ],
                &create_signed_envelope(&other, 1)
            )
            .await,
            crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
            "liveness selects which rows are evidence; it never changes whose evidence they are"
        );
    }

    /// SAME-SPELLING RECONNECT. An unchanged textual key must not preserve an old capability.
    ///
    /// The reported case needed a spelling change only because a same-spelling Hello
    /// *replaces* the row. This pins that: nothing may merge the old capability into the new
    /// claim, and the surviving live session must belong to the connection that made it.
    #[tokio::test]
    async fn a_same_spelling_reconnect_cannot_inherit_the_capability_it_dropped() {
        let sender = KeyPair::generate().unwrap();
        let captured = create_signed_envelope(&sender, 5);
        let (ctx, forwarded) = create_test_context(None);

        // Durable, then gone.
        let first = install_row(&ctx, sender.did(), DURABLE, Row::Live)
            .await
            .expect("a live session");
        drop(first);

        // Back under the *same* spelling, no capability. The row and the session entry are
        // both replaced by this connection's own.
        let _second = install_row(&ctx, sender.did(), NOT_DURABLE, Row::Live).await;

        ctx.handle_signed(create_network_message(&captured), &captured)
            .await;
        assert_eq!(
            forwarded.load(Ordering::SeqCst),
            1,
            "the peer's current claim is the only one it is making; on an empty window that is \
             the legacy steady state, so this envelope is forwarded — if the dropped \
             capability had survived, the retirement hold would have refused it instead"
        );
    }

    /// MULTIPLE CURRENT ROWS, ONE PRINCIPAL. `any` over *current* rows, order-independent.
    ///
    /// A key holder can authenticate under more than one spelling of itself, so two live rows
    /// for one principal is a legitimate state. The join stays `any`: a proof one live
    /// connection is making cannot be cancelled by another connection that simply does not
    /// mention it, or `all`/first-wins/last-wins would let a peer suppress its own proof by
    /// opening a second session. Both insertion orders, because a `HashMap` gives no promise
    /// about which the handler visits first.
    #[tokio::test]
    async fn two_live_rows_for_one_principal_join_the_same_way_in_either_order() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();
        let durable_row = alias_in(multibase::Base::Base36Lower, "base36-lower", &canonical);
        let legacy_row = alias_in(
            multibase::Base::Base32HexLower,
            "base32-hex-lower",
            &canonical,
        );
        let captured = create_signed_envelope(&sender, 9);

        for (order, rows) in [
            (
                "durable first",
                vec![
                    (durable_row.clone(), DURABLE, Row::Live),
                    (legacy_row.clone(), NOT_DURABLE, Row::Live),
                ],
            ),
            (
                "durable second",
                vec![
                    (legacy_row.clone(), NOT_DURABLE, Row::Live),
                    (durable_row.clone(), DURABLE, Row::Live),
                ],
            ),
        ] {
            assert_eq!(
                attributed_regime(&rows, &captured).await,
                crate::replay_guard::ObservedSenderRegime::DurableV1,
                "{order}: one live connection proving the capability is enough, whichever \
                 order the rows were written in"
            );
        }
    }

    /// STALENESS IS SPELLING-INVARIANT TOO.
    ///
    /// `every_accepted_spelling_selects_the_same_durable_capability` proves a *live* proof is
    /// found under all 22 alternates. The currency filter has to be just as encoding-blind, or
    /// some spellings would keep their abandoned rows and others would not — which is the
    /// original #2640 defect wearing a different hat.
    #[tokio::test]
    async fn an_abandoned_row_stops_counting_under_every_spelling() {
        let sender = KeyPair::generate().unwrap();
        let canonical = sender.did().clone();

        for (label, base) in ALTERNATE_SPELLINGS {
            let alias = alias_in(base, label, &canonical);
            let captured = create_signed_envelope(&sender, 42);
            assert_eq!(
                attributed_regime(&[(alias, DURABLE, Row::Closed)], &captured).await,
                crate::replay_guard::ObservedSenderRegime::LegacyOrUnproven,
                "{label}: an abandoned row must stop counting whatever base it was written in"
            );
        }
    }

    /// DUPLICATE PHYSICAL CONNECTIONS FOR ONE KEY.
    ///
    /// `handle_hello` writes `peer_connections[from]` unconditionally and only then calls
    /// `install_incoming_connection`, which *declines* — without closing — a duplicate when a
    /// live entry already owns the key (#2504). Under the old cache-plus-session reading that
    /// was a genuine mismatch: the capability row could be written by one physical connection
    /// while the thing proving it was still up was another.
    ///
    /// Leasing removes the mismatch instead of tolerating it. Each connection claims for
    /// itself, whether or not it won the session map, and gives its claim back when it ends. So
    /// this asserts the property that replaces it: the key stays proved while *any* of its
    /// connections is up, and stops the moment the last one goes — not when the first does.
    ///
    /// Dropping on the first would let a peer cancel its own live proof by closing an unrelated
    /// connection, which is the "adding a row must not suppress a proof" failure mirrored.
    #[tokio::test]
    async fn a_key_stays_proved_until_its_last_connection_goes() {
        let sender = KeyPair::generate().unwrap();
        let alias = alias_in(multibase::Base::Base32Lower, "base32-lower", sender.did());

        // A fresh context per case: the first accepted message would install the retirement
        // hold, and a hold left over from the setup would refuse the message under test for a
        // reason that has nothing to do with the claim.
        {
            let (ctx, forwarded) = create_test_context(None);
            let first = install_row(&ctx, sender.did(), DURABLE, Row::Live)
                .await
                .expect("a live claim");
            let _second = install_row(&ctx, &alias, DURABLE, Row::Live)
                .await
                .expect("a live claim");
            drop(first);

            let captured = create_signed_envelope(&sender, 4);
            ctx.handle_signed(create_network_message(&captured), &captured)
                .await;
            assert_eq!(
                forwarded.load(Ordering::SeqCst),
                0,
                "one connection closing must not retract a claim another connection is still \
                 making: an empty window still owes this sender the retirement hold"
            );
        }

        {
            let (ctx, forwarded) = create_test_context(None);
            let first = install_row(&ctx, sender.did(), DURABLE, Row::Live)
                .await
                .expect("a live claim");
            let second = install_row(&ctx, &alias, DURABLE, Row::Live)
                .await
                .expect("a live claim");
            drop(first);
            drop(second);

            let captured = create_signed_envelope(&sender, 4);
            ctx.handle_signed(create_network_message(&captured), &captured)
                .await;
            assert_eq!(
                forwarded.load(Ordering::SeqCst),
                1,
                "once the last connection is gone the key proves nothing, so this is the legacy \
                 steady state and the envelope is forwarded — the cache rows both connections \
                 wrote are still there and must not speak for them"
            );
        }
    }
}
