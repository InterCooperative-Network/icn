//! Storage challenge handlers: StorageChallengeMsg, StorageProofMsg, StorageContentNotFoundMsg
//!
//! These handlers implement proof-of-storage challenge protocol for
//! verifying that replica holders actually store the data they claim.

use crate::gossip::GossipActor;
use crate::types::GossipMessage;
use anyhow::Result;
use icn_identity::Did;
use icn_store::{
    ContentChunkTree, MerkleProofData, StorageChallenge, StorageContentNotFound, StorageProof,
    DEFAULT_CHUNK_SIZE,
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
            return Ok(());
        }

        // Check if challenge has expired
        if challenge.is_expired() {
            warn!(
                challenge_id = %hex::encode(challenge.id),
                "Received expired storage challenge"
            );
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
                    return Ok(());
                }
            }
            None => {
                warn!(
                    challenge_id = %hex::encode(challenge.id),
                    "Cannot sign storage proof - no keypair configured"
                );
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
        } else {
            info!(
                challenge_id = %hex::encode(proof.challenge_id),
                prover = %proof.prover,
                "Storage proof received (ChallengeScheduler not configured)"
            );
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

        // Verify signature if present (unsigned responses are suspicious)
        if response.is_signed() {
            if let Err(e) = response.verify_signature() {
                warn!(
                    challenge_id = %hex::encode(response.challenge_id),
                    responder = %response.responder,
                    error = %e,
                    "Content-not-found signature verification failed"
                );
                return Ok(());
            }
        } else {
            warn!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                "Received unsigned content-not-found response"
            );
            // Still forward to scheduler - it may want to track unsigned responses differently
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
        } else {
            info!(
                challenge_id = %hex::encode(response.challenge_id),
                responder = %response.responder,
                "Content-not-found received (ChallengeScheduler not configured)"
            );
        }

        Ok(())
    }
}
