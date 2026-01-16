//! Storage challenge handlers: StorageChallengeMsg, StorageProofMsg
//!
//! These handlers implement proof-of-storage challenge protocol for
//! verifying that replica holders actually store the data they claim.

use crate::gossip::GossipActor;
use crate::types::GossipMessage;
use anyhow::Result;
use icn_identity::Did;
use icn_store::{ContentChunkTree, StorageChallenge, StorageProof, DEFAULT_CHUNK_SIZE};
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

        // TODO: Validate challenger signature

        // Look for the content in our entries
        let mut content_data: Option<Vec<u8>> = None;
        for entries in self.entries.values() {
            if let Some(entry) = entries.get(&challenge.content_hash) {
                content_data = Some(entry.data.clone());
                break;
            }
        }

        let Some(content) = content_data else {
            warn!(
                challenge_id = %hex::encode(challenge.id),
                content_hash = %hex::encode(challenge.content_hash),
                "Cannot respond to challenge - content not found"
            );
            // TODO: Could send a "ContentNotFound" response
            return Ok(());
        };

        // Build Merkle tree and generate proof
        let chunk_size = DEFAULT_CHUNK_SIZE;
        let tree = ContentChunkTree::new(content, chunk_size);

        // Get requested bytes
        let byte_data = tree
            .get_bytes(challenge.byte_offset, challenge.byte_length)
            .unwrap_or_default();

        // Generate Merkle proof for requested chunk
        let merkle_proof = match tree.generate_proof(challenge.chunk_index) {
            Some(proof) => proof,
            None => {
                warn!(
                    challenge_id = %hex::encode(challenge.id),
                    chunk_index = challenge.chunk_index,
                    num_chunks = tree.num_chunks(),
                    "Invalid chunk index in challenge"
                );
                return Ok(());
            }
        };

        // Create proof response
        let proof = StorageProof::new(
            challenge.id,
            byte_data,
            merkle_proof.chunk_hash,
            merkle_proof.siblings,
            merkle_proof.path_bits,
            self.own_did.to_string(),
        );

        // TODO: Sign the proof

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
            merkle_depth = proof.merkle_siblings.len(),
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
    /// 1. Match to pending challenge
    /// 2. Verify byte data hash
    /// 3. Verify Merkle proof
    /// 4. Update replica health status
    /// 5. Record violation if invalid
    pub(crate) fn handle_storage_proof(
        &mut self,
        _sender: &Did,
        proof: StorageProof,
    ) -> Result<()> {
        debug!(
            challenge_id = %hex::encode(proof.challenge_id),
            prover = %proof.prover,
            byte_data_len = proof.byte_data.len(),
            merkle_depth = proof.merkle_siblings.len(),
            message_type = "StorageProofMsg",
            "Received storage proof"
        );

        // TODO: Match to pending challenge
        // TODO: Verify byte data matches expected hash
        // TODO: Verify Merkle proof against expected root
        // TODO: Update replica health status
        // TODO: Record violation if invalid

        info!(
            challenge_id = %hex::encode(proof.challenge_id),
            prover = %proof.prover,
            "Storage proof received (verification not yet implemented)"
        );

        Ok(())
    }
}
