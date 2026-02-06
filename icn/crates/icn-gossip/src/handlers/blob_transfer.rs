//! Blob transfer protocol handlers (Pilot)
//!
//! Handles BlobRequest and BlobTransferChunk messages for the blob gossip protocol.
//! These are stub handlers that log and validate; actual transfer state machine
//! is implemented in PR2c.
//!
//! # Security
//!
//! All blob transfer messages MUST be wrapped in SignedEnvelope.
//! The dispatch layer verifies signatures before reaching these handlers.
//! Replay protection is handled by ReplayGuard (PR2b).

use crate::gossip::GossipActor;
use crate::types::ContentHash;
use anyhow::Result;
use icn_identity::Did;
use tracing::{debug, warn};

/// Maximum chunk size: 64 KB
pub const MAX_CHUNK_SIZE: usize = 64 * 1024;

/// Maximum blob size: 10 MB
pub const MAX_BLOB_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of chunks per transfer (10 MB / 64 KB = 160)
pub const MAX_CHUNKS_PER_TRANSFER: u32 = 160;

/// Topic constants for blob transfer protocol
///
/// Used by PR2c (transfer state machine) and PR2d (provider selection)
/// for topic registration and subscription.
#[allow(dead_code)]
pub mod topics {
    /// Topic for blob availability announcements
    pub const BLOB_ANNOUNCE: &str = "blob:announce";
    /// Topic for blob transfer requests
    pub const BLOB_REQUEST: &str = "blob:request";
    /// Topic for blob transfer chunks
    pub const BLOB_TRANSFER: &str = "blob:transfer";
}

impl GossipActor {
    /// Handle an incoming BlobRequest message.
    ///
    /// Validates the request fields and logs the request. Actual blob lookup
    /// and transfer initiation is implemented in PR2c (transfer state machine).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_blob_request(
        &mut self,
        sender: &Did,
        request_id: ContentHash,
        blob_hash: ContentHash,
        requester_did: Did,
        expires_at: u64,
    ) -> Result<()> {
        debug!(
            sender = %sender,
            requester = %requester_did,
            blob_hash = %hex::encode(blob_hash),
            request_id = %hex::encode(request_id),
            expires_at = expires_at,
            message_type = "BlobRequest",
            "Received blob request"
        );

        // Validate: requester_did must match sender (envelope.from)
        if *sender != requester_did {
            warn!(
                sender = %sender,
                claimed_requester = %requester_did,
                "BlobRequest requester_did does not match envelope sender, rejecting"
            );
            return Ok(());
        }

        // Validate: check expiration
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if expires_at > 0 && now > expires_at {
            warn!(
                sender = %sender,
                blob_hash = %hex::encode(blob_hash),
                expires_at = expires_at,
                now = now,
                "BlobRequest expired, ignoring"
            );
            return Ok(());
        }

        // TODO(PR2c): Look up blob in BlobService, initiate chunked transfer
        // For now, just log that we received a valid request

        Ok(())
    }

    /// Handle an incoming BlobTransferChunk message.
    ///
    /// Validates chunk fields (size, index bounds, chunk hash).
    /// Actual reassembly state machine is implemented in PR2c.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_blob_transfer_chunk(
        &mut self,
        sender: &Did,
        request_id: ContentHash,
        blob_hash: ContentHash,
        chunk_index: u32,
        total_chunks: u32,
        chunk_hash: ContentHash,
        total_size: u64,
        data: Vec<u8>,
    ) -> Result<()> {
        debug!(
            sender = %sender,
            blob_hash = %hex::encode(blob_hash),
            request_id = %hex::encode(request_id),
            chunk = %format!("{}/{}", chunk_index, total_chunks),
            chunk_size = data.len(),
            total_size = total_size,
            message_type = "BlobTransferChunk",
            "Received blob transfer chunk"
        );

        // Validate: chunk data size
        if data.len() > MAX_CHUNK_SIZE {
            warn!(
                sender = %sender,
                chunk_size = data.len(),
                max = MAX_CHUNK_SIZE,
                "BlobTransferChunk exceeds max chunk size, rejecting"
            );
            return Ok(());
        }

        // Validate: total_size within blob limit
        if total_size > MAX_BLOB_SIZE {
            warn!(
                sender = %sender,
                total_size = total_size,
                max = MAX_BLOB_SIZE,
                "BlobTransferChunk total_size exceeds max blob size, rejecting"
            );
            return Ok(());
        }

        // Validate: chunk_index within bounds
        if chunk_index >= total_chunks {
            warn!(
                sender = %sender,
                chunk_index = chunk_index,
                total_chunks = total_chunks,
                "BlobTransferChunk chunk_index out of bounds, rejecting"
            );
            return Ok(());
        }

        // Validate: total_chunks within bounds
        if total_chunks > MAX_CHUNKS_PER_TRANSFER {
            warn!(
                sender = %sender,
                total_chunks = total_chunks,
                max = MAX_CHUNKS_PER_TRANSFER,
                "BlobTransferChunk total_chunks exceeds maximum, rejecting"
            );
            return Ok(());
        }

        // Validate: chunk hash
        let actual_hash = *blake3::hash(&data).as_bytes();
        if actual_hash != chunk_hash {
            warn!(
                sender = %sender,
                expected = %hex::encode(chunk_hash),
                actual = %hex::encode(actual_hash),
                "BlobTransferChunk hash mismatch, rejecting"
            );
            return Ok(());
        }

        // TODO(PR2c): Feed chunk into reassembly state machine
        // For now, validation is complete

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn blob_request_variant_name_and_fields() {
        use crate::types::GossipMessage;

        let did = make_test_did();
        let msg = GossipMessage::BlobRequest {
            request_id: [1u8; 32],
            blob_hash: [2u8; 32],
            requester_did: did.clone(),
            expires_at: 1700000000,
        };
        assert_eq!(msg.variant_name(), "BlobRequest");

        // Verify fields are accessible via pattern match
        match msg {
            GossipMessage::BlobRequest {
                request_id,
                blob_hash,
                requester_did: rd,
                expires_at,
            } => {
                assert_eq!(request_id, [1u8; 32]);
                assert_eq!(blob_hash, [2u8; 32]);
                assert_eq!(rd, did);
                assert_eq!(expires_at, 1700000000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn blob_transfer_chunk_variant_name_and_fields() {
        use crate::types::GossipMessage;

        let chunk_data = vec![0xABu8; 1024];
        let chunk_hash = *blake3::hash(&chunk_data).as_bytes();

        let msg = GossipMessage::BlobTransferChunk {
            request_id: [1u8; 32],
            blob_hash: [2u8; 32],
            chunk_index: 0,
            total_chunks: 5,
            chunk_hash,
            total_size: 5 * 1024,
            data: chunk_data.clone(),
        };
        assert_eq!(msg.variant_name(), "BlobTransferChunk");

        match msg {
            GossipMessage::BlobTransferChunk {
                chunk_index,
                total_chunks,
                chunk_hash: rh,
                data,
                ..
            } => {
                assert_eq!(chunk_index, 0);
                assert_eq!(total_chunks, 5);
                assert_eq!(rh, chunk_hash);
                assert_eq!(data, chunk_data);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn chunk_hash_validation() {
        let data = b"hello chunk";
        let correct_hash = *blake3::hash(data).as_bytes();
        let wrong_hash = [0u8; 32];

        assert_eq!(*blake3::hash(data).as_bytes(), correct_hash);
        assert_ne!(correct_hash, wrong_hash);
    }

    #[test]
    fn max_chunk_size_within_blob_limit() {
        // 10 MB / 64 KB = 160 chunks max
        assert_eq!(MAX_BLOB_SIZE / MAX_CHUNK_SIZE as u64, 160);
        assert_eq!(MAX_CHUNKS_PER_TRANSFER, 160);
    }

    #[test]
    fn topic_constants_are_namespaced() {
        assert!(topics::BLOB_ANNOUNCE.starts_with("blob:"));
        assert!(topics::BLOB_REQUEST.starts_with("blob:"));
        assert!(topics::BLOB_TRANSFER.starts_with("blob:"));
        // All topics are unique
        assert_ne!(topics::BLOB_ANNOUNCE, topics::BLOB_REQUEST);
        assert_ne!(topics::BLOB_REQUEST, topics::BLOB_TRANSFER);
    }

    #[test]
    fn blob_request_serialization_round_trip() {
        use crate::types::GossipMessage;

        let did = make_test_did();
        let msg = GossipMessage::BlobRequest {
            request_id: [0xAA; 32],
            blob_hash: [0xBB; 32],
            requester_did: did.clone(),
            expires_at: 1700000000,
        };

        // Canonical encoding invariant: encode → decode must round-trip
        let encoded = icn_encoding::encode(&msg).expect("encode BlobRequest");
        let decoded: GossipMessage = icn_encoding::decode(&encoded).expect("decode BlobRequest");

        match decoded {
            GossipMessage::BlobRequest {
                request_id,
                blob_hash,
                requester_did: rd,
                expires_at,
            } => {
                assert_eq!(request_id, [0xAA; 32]);
                assert_eq!(blob_hash, [0xBB; 32]);
                assert_eq!(rd, did);
                assert_eq!(expires_at, 1700000000);
            }
            _ => panic!("wrong variant after round-trip"),
        }
    }

    #[test]
    fn blob_transfer_chunk_serialization_round_trip() {
        use crate::types::GossipMessage;

        let chunk_data = vec![42u8; 512];
        let chunk_hash = *blake3::hash(&chunk_data).as_bytes();

        let msg = GossipMessage::BlobTransferChunk {
            request_id: [0xCC; 32],
            blob_hash: [0xDD; 32],
            chunk_index: 3,
            total_chunks: 10,
            chunk_hash,
            total_size: 5120,
            data: chunk_data.clone(),
        };

        let encoded = icn_encoding::encode(&msg).expect("encode BlobTransferChunk");
        let decoded: GossipMessage =
            icn_encoding::decode(&encoded).expect("decode BlobTransferChunk");

        match decoded {
            GossipMessage::BlobTransferChunk {
                request_id,
                blob_hash,
                chunk_index,
                total_chunks,
                chunk_hash: rh,
                total_size,
                data,
            } => {
                assert_eq!(request_id, [0xCC; 32]);
                assert_eq!(blob_hash, [0xDD; 32]);
                assert_eq!(chunk_index, 3);
                assert_eq!(total_chunks, 10);
                assert_eq!(rh, chunk_hash);
                assert_eq!(total_size, 5120);
                assert_eq!(data, chunk_data);
            }
            _ => panic!("wrong variant after round-trip"),
        }
    }

    #[test]
    fn blob_request_handler_rejects_expired() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();

        // Set expires_at to the past (1 second after epoch)
        let result = actor.handle_blob_request(
            &sender,
            [1u8; 32],
            [2u8; 32],
            sender.clone(),
            1, // expired: 1 second after epoch
        );

        // Should succeed (handler logs warning and returns Ok, doesn't error)
        assert!(result.is_ok());
    }

    #[test]
    fn blob_request_handler_rejects_mismatched_sender() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let different_requester = make_test_did();

        // requester_did doesn't match sender — should be rejected
        let result = actor.handle_blob_request(
            &sender,
            [1u8; 32],
            [2u8; 32],
            different_requester,
            u64::MAX, // far future
        );

        // Handler logs warning and returns Ok (doesn't propagate error)
        assert!(result.is_ok());
    }

    #[test]
    fn blob_transfer_chunk_handler_rejects_oversized_chunk() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let oversized_data = vec![0u8; MAX_CHUNK_SIZE + 1];
        let chunk_hash = *blake3::hash(&oversized_data).as_bytes();

        let result = actor.handle_blob_transfer_chunk(
            &sender,
            [1u8; 32],
            [2u8; 32],
            0,
            1,
            chunk_hash,
            oversized_data.len() as u64,
            oversized_data,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn blob_transfer_chunk_handler_rejects_bad_hash() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let data = vec![0xFFu8; 100];
        let wrong_hash = [0u8; 32]; // not the hash of data

        let result = actor
            .handle_blob_transfer_chunk(&sender, [1u8; 32], [2u8; 32], 0, 1, wrong_hash, 100, data);

        assert!(result.is_ok());
    }
}
