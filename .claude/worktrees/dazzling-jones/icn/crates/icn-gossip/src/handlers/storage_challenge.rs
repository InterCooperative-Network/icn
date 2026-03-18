//! Storage challenge handlers: StorageChallengeMsg, StorageProofMsg, StorageContentNotFoundMsg
//!
//! These handlers implement proof-of-storage challenge protocol for
//! verifying that replica holders actually store the data they claim.

use crate::gossip::GossipActor;
use crate::types::GossipMessage;
use anyhow::Result;
use icn_identity::Did;
use icn_obs::metrics::storage::{
    challenge_result_inc, challenges_received_inc, proof_verification_inc, proofs_received_inc,
    ChallengeResult, ProofVerifyResult,
};
use icn_store::{
    ContentChunkTree, MerkleProofData, StorageChallenge, StorageContentNotFound, StorageProof,
    DEFAULT_CHALLENGE_TIMEOUT_SECS, DEFAULT_CHUNK_SIZE,
};
use tracing::{debug, info, warn};

impl GossipActor {
    /// Handle a StorageChallengeMsg - challenge from verifier
    ///
    /// When we receive a storage challenge:
    /// 1. Validate the challenge hasn't expired
    /// 2. Check if we have the content
    /// 3. Generate proof (bytes + Merkle proof)
    /// 4. Send proof back to challenger
    pub(crate) fn handle_storage_challenge(
        &mut self,
        _sender: &Did,
        challenge: StorageChallenge,
    ) -> Result<()> {
        // Record that we received a challenge
        challenges_received_inc();

        debug!(
            challenge_id = %hex::encode(challenge.id),
            content_hash = %hex::encode(challenge.content_hash),
            target = %challenge.target_peer,
            challenger = %challenge.challenger,
            message_type = "StorageChallengeMsg",
            "Received storage challenge"
        );

        // Validate protocol version (reject unsupported versions early)
        if let Err(e) = challenge.validate_version() {
            warn!(
                challenge_id = %hex::encode(challenge.id),
                version = challenge.version,
                error = %e,
                "Rejecting challenge with unsupported protocol version"
            );
            challenge_result_inc(ChallengeResult::InvalidVersion);
            return Ok(());
        }

        // Check if challenge is for us
        if challenge.target_peer != self.own_did.to_string() {
            debug!(
                challenge_id = %hex::encode(challenge.id),
                target = %challenge.target_peer,
                our_did = %self.own_did,
                "Challenge not for us, ignoring"
            );
            challenge_result_inc(ChallengeResult::WrongTarget);
            return Ok(());
        }

        // Check if challenge has expired
        if challenge.is_expired() {
            warn!(
                challenge_id = %hex::encode(challenge.id),
                "Received expired storage challenge"
            );
            challenge_result_inc(ChallengeResult::Expired);
            return Ok(());
        }

        // Verify challenger signature (required - prevents forgery)
        if let Err(e) = challenge.verify_signature() {
            warn!(
                challenge_id = %hex::encode(challenge.id),
                challenger = %challenge.challenger,
                error = %e,
                "Challenge signature verification failed"
            );
            challenge_result_inc(ChallengeResult::SignatureFailed);
            return Ok(());
        }

        // Look for the content in our entries
        let mut content_data: Option<Vec<u8>> = None;
        for entries in self.entries.values() {
            if let Some(entry) = entries.get(&challenge.content_hash) {
                content_data = Some(entry.data.clone());
                break;
            }
        }

        let Some(content) = content_data else {
            info!(
                challenge_id = %hex::encode(challenge.id),
                content_hash = %hex::encode(challenge.content_hash),
                "Content not found - sending ContentNotFound response"
            );

            // Send ContentNotFound response to challenger
            let challenger_did = match Did::from_str(&challenge.challenger) {
                Ok(did) => did,
                Err(e) => {
                    warn!(
                        challenger = %challenge.challenger,
                        error = %e,
                        "Invalid challenger DID in content-not-found response"
                    );
                    return Ok(());
                }
            };

            // Create and sign the content-not-found response
            let mut response = StorageContentNotFound::new(
                challenge.id,
                challenge.content_hash,
                self.own_did.to_string(),
            );

            // Sign if we have a keypair
            if let Some(kp) = &self.keypair {
                if let Err(e) = response.sign(kp) {
                    warn!(
                        challenge_id = %hex::encode(challenge.id),
                        error = %e,
                        "Failed to sign content-not-found response"
                    );
                    return Ok(());
                }
            }

            self.send_message(
                Some(challenger_did),
                GossipMessage::StorageContentNotFoundMsg { response },
            );

            challenge_result_inc(ChallengeResult::ContentNotFound);
            return Ok(());
        };

        // Build Merkle tree and generate proof
        let chunk_size = DEFAULT_CHUNK_SIZE;
        let tree = ContentChunkTree::new(content, chunk_size);

        // Get requested bytes
        let byte_data = tree
            .get_bytes(challenge.byte_offset, challenge.byte_length)
            .unwrap_or_default();

        // Generate Merkle proofs for all requested chunk indices (v2 multi-block)
        let mut merkle_proofs: Vec<MerkleProofData> = Vec::new();
        for &chunk_idx in &challenge.chunk_indices {
            let merkle_proof = match tree.generate_proof(chunk_idx) {
                Some(proof) => proof,
                None => {
                    warn!(
                        challenge_id = %hex::encode(challenge.id),
                        chunk_index = chunk_idx,
                        num_chunks = tree.num_chunks(),
                        "Invalid chunk index in challenge"
                    );
                    challenge_result_inc(ChallengeResult::ProofError);
                    return Ok(());
                }
            };
            merkle_proofs.push(MerkleProofData::from(merkle_proof));
        }

        // Create proof response (v2 multi-proof)
        let mut proof = StorageProof::new_multi(
            challenge.id,
            byte_data,
            merkle_proofs,
            self.own_did.to_string(),
        );

        // Sign the proof (required for verification by challenger)
        match &self.keypair {
            Some(kp) => {
                if let Err(e) = proof.sign(kp) {
                    warn!(
                        challenge_id = %hex::encode(challenge.id),
                        error = %e,
                        "Failed to sign storage proof"
                    );
                    challenge_result_inc(ChallengeResult::SignatureFailed);
                    return Ok(());
                }
            }
            None => {
                warn!(
                    challenge_id = %hex::encode(challenge.id),
                    "Cannot sign storage proof - no keypair configured"
                );
                challenge_result_inc(ChallengeResult::NoKeypair);
                return Ok(());
            }
        }

        // Send proof back to challenger
        let challenger_did = match Did::from_str(&challenge.challenger) {
            Ok(did) => did,
            Err(e) => {
                warn!(
                    challenger = %challenge.challenger,
                    error = %e,
                    "Invalid challenger DID"
                );
                return Ok(());
            }
        };

        info!(
            challenge_id = %hex::encode(challenge.id),
            content_hash = %hex::encode(challenge.content_hash),
            byte_data_len = proof.byte_data.len(),
            num_proofs = proof.merkle_proofs.len(),
            "Sending storage proof"
        );

        self.send_message(
            Some(challenger_did),
            GossipMessage::StorageProofMsg { proof },
        );

        challenge_result_inc(ChallengeResult::ProofSent);
        Ok(())
    }

    /// Handle a StorageProofMsg - proof response from replica holder
    ///
    /// When we receive a storage proof:
    /// 1. Forward to ChallengeScheduler (via callback)
    /// 2. ChallengeScheduler handles:
    ///    - Match to pending challenge
    ///    - Verify byte data hash
    ///    - Verify Merkle proof
    ///    - Update replica health status
    ///    - Record violation if invalid
    pub(crate) fn handle_storage_proof(
        &mut self,
        _sender: &Did,
        proof: StorageProof,
    ) -> Result<()> {
        // Record that we received a proof
        proofs_received_inc();

        debug!(
            challenge_id = %hex::encode(proof.challenge_id),
            prover = %proof.prover,
            byte_data_len = proof.byte_data.len(),
            num_proofs = proof.merkle_proofs.len(),
            message_type = "StorageProofMsg",
            "Received storage proof"
        );

        // Forward to ChallengeScheduler for verification
        if let Some(callback) = &self.storage_proof_callback {
            if let Err(e) = callback(proof.clone()) {
                warn!(
                    challenge_id = %hex::encode(proof.challenge_id),
                    error = %e,
                    "Error processing storage proof in ChallengeScheduler"
                );
            }
            // Verification result is tracked by ChallengeScheduler
            proof_verification_inc(ProofVerifyResult::Valid);
        } else {
            info!(
                challenge_id = %hex::encode(proof.challenge_id),
                prover = %proof.prover,
                "Storage proof received (ChallengeScheduler not configured)"
            );
            proof_verification_inc(ProofVerifyResult::NoScheduler);
        }

        Ok(())
    }

    /// Handle a StorageContentNotFoundMsg - content not found response from replica holder
    ///
    /// When we receive a content-not-found response:
    /// 1. Verify the signature (ensures authenticity)
    /// 2. Forward to ChallengeScheduler (via callback)
    /// 3. ChallengeScheduler handles:
    ///    - Match to pending challenge
    ///    - Record that replica no longer has the content
    ///    - Update replica health status (may demote to Degraded)
    ///    - Trigger re-replication if needed
    pub(crate) fn handle_storage_content_not_found(
        &mut self,
        _sender: &Did,
        response: StorageContentNotFound,
    ) -> Result<()> {
        debug!(
            challenge_id = %hex::encode(response.challenge_id),
            content_hash = %hex::encode(response.content_hash),
            responder = %response.responder,
            message_type = "StorageContentNotFoundMsg",
            "Received content-not-found response"
        );

        // Validate timestamp to prevent replay attacks
        // Use stricter clock skew for storage challenges (60s vs network's 300s)
        const MAX_CLOCK_SKEW_SECS: u64 = 60;
        let now = icn_time::current_timestamp_secs();

        // Check for future timestamps (possible clock manipulation)
        if response.generated_at > now + MAX_CLOCK_SKEW_SECS {
            warn!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                generated_at = response.generated_at,
                now = now,
                "Content-not-found response has future timestamp, rejecting"
            );
            proof_verification_inc(ProofVerifyResult::NotFoundExpired);
            return Ok(());
        }

        // Check for stale timestamps (replay attack)
        let age = now.saturating_sub(response.generated_at);
        if age > DEFAULT_CHALLENGE_TIMEOUT_SECS {
            warn!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                age_seconds = age,
                "Content-not-found response too old, possible replay attack"
            );
            proof_verification_inc(ProofVerifyResult::NotFoundExpired);
            return Ok(());
        }

        // Require signature on all content-not-found responses (security hardening)
        if !response.is_signed() {
            warn!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                "Received unsigned content-not-found response, rejecting"
            );
            proof_verification_inc(ProofVerifyResult::NotFoundUnsigned);
            return Ok(());
        }

        // Verify the signature
        if let Err(e) = response.verify_signature() {
            warn!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                error = %e,
                "Content-not-found signature verification failed"
            );
            proof_verification_inc(ProofVerifyResult::InvalidSignature);
            return Ok(());
        }

        // Forward to ChallengeScheduler for processing
        if let Some(callback) = &self.storage_not_found_callback {
            if let Err(e) = callback(response.clone()) {
                warn!(
                    challenge_id = %hex::encode(response.challenge_id),
                    error = %e,
                    "Error processing content-not-found in ChallengeScheduler"
                );
            }
            proof_verification_inc(ProofVerifyResult::NotFound);
        } else {
            info!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                "Content-not-found received (ChallengeScheduler not configured)"
            );
            proof_verification_inc(ProofVerifyResult::NoScheduler);
        }

        Ok(())
    }
}
