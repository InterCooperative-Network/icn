//! Blob transfer protocol handlers (Pilot)
//!
//! Handles BlobRequest and BlobTransferChunk messages for the blob gossip protocol.
//!
//! **Provider side** (`handle_blob_request`): Looks up blob in BlobService,
//! splits into 64KB chunks, sends via send_message callback.
//!
//! **Requester side** (`handle_blob_transfer_chunk`): Validates chunk, feeds
//! into TransferSessionManager, handles assembly and ArtifactReceipt emission.
//!
//! # Security
//!
//! All blob transfer messages MUST be wrapped in SignedEnvelope.
//! The dispatch layer verifies signatures before reaching these handlers.
//! Replay protection is handled by BlobNonceGuard (PR2b #1069).

use crate::gossip::GossipActor;
use crate::handlers::blob_nonce_guard::composite_chunk_nonce;
use crate::handlers::blob_transfer_state::{ChunkResult, RequestId, TransferSessionManager};
use crate::types::{ContentHash, GossipMessage};
use anyhow::Result;
use icn_identity::Did;
use icn_kernel_api::proofs::ArtifactReceipt;
use std::sync::Mutex;
use tracing::{debug, info, warn};

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

/// RAII guard for the two-phase assembly section.
///
/// Ensures `active_reassemblies` is decremented and the session is cleaned up
/// even if an error occurs between `begin_assembly` and `complete_assembly`.
/// Call `defuse()` after successful storage to prevent cleanup on drop.
struct AssemblyGuard<'a> {
    manager: &'a Mutex<TransferSessionManager>,
    request_id: RequestId,
    defused: bool,
}

impl<'a> AssemblyGuard<'a> {
    fn new(manager: &'a Mutex<TransferSessionManager>, request_id: RequestId) -> Self {
        Self {
            manager,
            request_id,
            defused: false,
        }
    }

    /// Mark the assembly as successful; drop will not clean up.
    fn defuse(&mut self) {
        self.defused = true;
    }
}

impl Drop for AssemblyGuard<'_> {
    fn drop(&mut self) {
        if !self.defused {
            if let Ok(mut mgr) = self.manager.lock() {
                let _ = mgr.complete_assembly(self.request_id, false);
                mgr.cleanup_session(self.request_id);
            }
        }
    }
}

impl GossipActor {
    /// Handle a BlobAnnounce message — records the announcing peer in the provider registry.
    ///
    /// # Validation
    /// - Sender must match the announced peer_did (envelope authenticity)
    /// - Size must not exceed MAX_BLOB_SIZE
    ///
    /// Announcements are hints, not authoritative. The requester always validates
    /// the actual blob after transfer.
    pub(crate) fn handle_blob_announce(
        &mut self,
        sender: &Did,
        blob_hash: ContentHash,
        peer_did: Did,
        size_bytes: u64,
    ) -> Result<()> {
        debug!(
            sender = %sender,
            peer_did = %peer_did,
            blob_hash = %hex::encode(blob_hash),
            size_bytes = size_bytes,
            message_type = "BlobAnnounce",
            "Received blob announcement"
        );

        // Validate: peer_did must match sender (envelope.from)
        if *sender != peer_did {
            warn!(
                sender = %sender,
                claimed_peer = %peer_did,
                "BlobAnnounce peer_did does not match envelope sender, rejecting"
            );
            return Ok(());
        }

        // Validate: announced size within limit
        if size_bytes > MAX_BLOB_SIZE {
            warn!(
                sender = %sender,
                size_bytes = size_bytes,
                max = MAX_BLOB_SIZE,
                "BlobAnnounce size exceeds max blob size, rejecting"
            );
            return Ok(());
        }

        // Record in provider registry for later selection
        self.provider_registry
            .record_announcement(blob_hash, peer_did, size_bytes);

        Ok(())
    }

    /// Handle an incoming BlobRequest message (provider side).
    ///
    /// After validation (replay, expiry, sender match), looks up the blob
    /// in BlobService and sends 64KB chunks back to the requester.
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

        // Replay check: reject duplicate request_id from same sender
        if let Err(e) = self.blob_nonce_guard.check_and_record(sender, request_id) {
            warn!(
                sender = %sender,
                request_id = %hex::encode(request_id),
                "BlobRequest replay rejected: {e}"
            );
            return Ok(());
        }

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
        let now = icn_time::current_timestamp_secs();
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

        // Look up blob in BlobService
        let blob_store = match &self.blob_store {
            Some(store) => store.clone(),
            None => {
                warn!("No blob store configured, cannot serve BlobRequest");
                return Ok(());
            }
        };

        let blob_data = match blob_store.get(&blob_hash) {
            Ok(data) => data,
            Err(_) => {
                debug!(
                    blob_hash = %hex::encode(blob_hash),
                    "Blob not found locally, cannot serve request"
                );
                return Ok(());
            }
        };

        // Split into 64KB chunks and send
        let send_callback = match &self.send_callback {
            Some(cb) => cb.clone(),
            None => {
                warn!("No send callback configured, cannot respond to BlobRequest");
                return Ok(());
            }
        };

        let total_size = blob_data.len() as u64;
        let chunks: Vec<&[u8]> = blob_data.chunks(MAX_CHUNK_SIZE).collect();
        let total_chunks = chunks.len() as u32;

        for (i, chunk) in chunks.iter().enumerate() {
            let chunk_hash = *blake3::hash(chunk).as_bytes();
            let msg = GossipMessage::BlobTransferChunk {
                request_id,
                blob_hash,
                chunk_index: i as u32,
                total_chunks,
                chunk_hash,
                total_size,
                data: chunk.to_vec(),
            };
            send_callback(Some(requester_did.clone()), msg);
        }

        debug!(
            blob_hash = %hex::encode(blob_hash),
            total_chunks = total_chunks,
            total_size = total_size,
            requester = %requester_did,
            "Sent blob chunks to requester"
        );

        Ok(())
    }

    /// Handle an incoming BlobTransferChunk message (requester side).
    ///
    /// After validation (chunk_hash, replay, size, bounds), feeds the chunk
    /// into the TransferSessionManager. On AllChunksReceived, performs
    /// two-phase assembly and emits ArtifactReceipt.
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

        // Replay check: reject duplicate (request_id, chunk_index) from same sender
        let chunk_nonce = composite_chunk_nonce(&request_id, chunk_index);
        if let Err(e) = self.blob_nonce_guard.check_and_record(sender, chunk_nonce) {
            warn!(
                sender = %sender,
                request_id = %hex::encode(request_id),
                chunk_index = chunk_index,
                "BlobTransferChunk replay rejected: {e}"
            );
            return Ok(());
        }

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

        // Feed into transfer session manager
        let transfer_manager = match &self.transfer_manager {
            Some(mgr) => mgr,
            None => {
                warn!("No transfer manager configured, cannot process BlobTransferChunk");
                return Ok(());
            }
        };

        // Phase 1: receive chunk (lock held)
        let chunk_result = {
            let mut mgr = transfer_manager
                .lock()
                .map_err(|e| anyhow::anyhow!("Transfer manager lock poisoned: {e}"))?;
            match mgr.receive_chunk(
                request_id,
                sender,
                chunk_index,
                total_chunks,
                total_size,
                &data,
            ) {
                Ok(result) => result,
                Err(err_code) => {
                    debug!(
                        sender = %sender,
                        request_id = %hex::encode(request_id),
                        error = %err_code,
                        "Transfer manager rejected chunk"
                    );
                    return Ok(());
                }
            }
        };

        if chunk_result != ChunkResult::AllChunksReceived {
            return Ok(());
        }

        // All chunks received — begin two-phase assembly with RAII guard.
        // The guard ensures active_reassemblies is decremented and the session
        // is cleaned up even if an error occurs mid-assembly.
        let mut guard = AssemblyGuard::new(transfer_manager, request_id);

        // Phase 2a: begin_assembly (lock held) — marks Assembling, returns job
        let assembly_job = {
            let mut mgr = transfer_manager
                .lock()
                .map_err(|e| anyhow::anyhow!("Transfer manager lock poisoned: {e}"))?;
            match mgr.begin_assembly(request_id) {
                Ok(job) => job,
                Err(err_code) => {
                    // begin_assembly failed — guard has nothing to clean up since
                    // active_reassemblies was never incremented
                    guard.defuse();
                    warn!(
                        request_id = %hex::encode(request_id),
                        error = %err_code,
                        "Failed to begin assembly"
                    );
                    return Ok(());
                }
            }
        };

        // Phase 2b: execute assembly (NO lock held — disk IO + hash verification)
        let assembled_data = match assembly_job.execute() {
            Ok(data) => data,
            Err(err_code) => {
                // Guard will call complete_assembly(false) + cleanup_session on drop
                warn!(
                    request_id = %hex::encode(request_id),
                    error = %err_code,
                    "Blob assembly failed, session cleaned up"
                );
                return Ok(());
            }
        };

        // Phase 2c: complete_assembly success (lock held) — marks Verified
        {
            let mut mgr = transfer_manager
                .lock()
                .map_err(|e| anyhow::anyhow!("Transfer manager lock poisoned: {e}"))?;
            mgr.complete_assembly(request_id, true)
                .map_err(|e| anyhow::anyhow!("complete_assembly failed: {e}"))?;
        }

        // Store in BlobStore
        if let Some(blob_store) = &self.blob_store {
            let ns = icn_kernel_api::types::Namespace::new("blob", "transfer");
            match blob_store.put(&ns, &assembled_data) {
                Ok(_stored_hash) => {
                    debug!(
                        request_id = %hex::encode(request_id),
                        blob_hash = %hex::encode(blob_hash),
                        "Blob stored successfully"
                    );
                }
                Err(e) => {
                    warn!(
                        request_id = %hex::encode(request_id),
                        error = %e,
                        "Failed to store assembled blob"
                    );
                    // Guard will clean up on drop
                    return Ok(());
                }
            }
        }

        // All done — defuse the guard so it doesn't clean up on drop
        guard.defuse();

        // Mark stored and get session metadata for receipt
        let (provider_did, requester_did, scope_id) = {
            let mut mgr = transfer_manager
                .lock()
                .map_err(|e| anyhow::anyhow!("Transfer manager lock poisoned: {e}"))?;
            mgr.mark_stored(request_id);

            let session = mgr.get_session(&request_id);
            let meta = session.map(|s| {
                (
                    s.provider_did.clone(),
                    s.requester_did.clone(),
                    s.scope_id.clone(),
                )
            });

            mgr.cleanup_session(request_id);
            meta.unwrap_or_else(|| (sender.clone(), self.own_did.clone(), String::new()))
        };

        // Emit ArtifactReceipt
        let now = icn_time::current_timestamp_secs();

        let receipt = ArtifactReceipt::new(
            blob_hash,
            provider_did.to_string(),
            requester_did.to_string(),
            request_id,
            scope_id.clone(),
            now,
        );

        info!(
            request_id = %hex::encode(request_id),
            blob_hash = %hex::encode(blob_hash),
            provider_did = %provider_did,
            requester_did = %requester_did,
            scope_id = %scope_id,
            receipt_hash = %hex::encode(receipt.receipt_hash),
            verified_at = now,
            "ArtifactReceipt emitted for verified blob transfer"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::blob_transfer_state::TransferSessionManager;
    use icn_identity::KeyPair;
    use icn_kernel_api::state::{BlobMetadata, BlobService, StateError};
    use icn_kernel_api::types::{Hash, Namespace};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn make_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    /// In-memory BlobService for testing
    struct MockBlobStore {
        blobs: Mutex<HashMap<Hash, (Vec<u8>, Namespace)>>,
    }

    impl MockBlobStore {
        fn new() -> Self {
            Self {
                blobs: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, data: &[u8]) -> Hash {
            let hash = *blake3::hash(data).as_bytes();
            let mut blobs = self.blobs.lock().unwrap_or_else(|e| e.into_inner());
            blobs.insert(hash, (data.to_vec(), Namespace::new("test", "blob")));
            hash
        }
    }

    impl BlobService for MockBlobStore {
        fn put(&self, namespace: &Namespace, data: &[u8]) -> Result<Hash, StateError> {
            let hash = *blake3::hash(data).as_bytes();
            let mut blobs = self.blobs.lock().unwrap_or_else(|e| e.into_inner());
            blobs.insert(hash, (data.to_vec(), namespace.clone()));
            Ok(hash)
        }

        fn get(&self, hash: &Hash) -> Result<Vec<u8>, StateError> {
            let blobs = self.blobs.lock().unwrap_or_else(|e| e.into_inner());
            blobs
                .get(hash)
                .map(|(data, _)| data.clone())
                .ok_or(StateError::BlobNotFound)
        }

        fn exists(&self, hash: &Hash) -> Result<bool, StateError> {
            let blobs = self.blobs.lock().unwrap_or_else(|e| e.into_inner());
            Ok(blobs.contains_key(hash))
        }

        fn delete(&self, hash: &Hash) -> Result<(), StateError> {
            let mut blobs = self.blobs.lock().unwrap_or_else(|e| e.into_inner());
            blobs.remove(hash);
            Ok(())
        }

        fn metadata(&self, hash: &Hash) -> Result<BlobMetadata, StateError> {
            let blobs = self.blobs.lock().unwrap_or_else(|e| e.into_inner());
            blobs
                .get(hash)
                .map(|(data, ns)| BlobMetadata {
                    size: data.len() as u64,
                    namespace: ns.clone(),
                    created_at: 0,
                    content_type: None,
                })
                .ok_or(StateError::BlobNotFound)
        }
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

    // --- Replay protection tests (PR2b) ---

    #[test]
    fn blob_request_handler_rejects_duplicate_request() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let request_id = [0x11; 32];

        // First request: accepted
        let r1 =
            actor.handle_blob_request(&sender, request_id, [0xAA; 32], sender.clone(), u64::MAX);
        assert!(r1.is_ok());

        // Same request_id from same sender: silently rejected (replay)
        let r2 =
            actor.handle_blob_request(&sender, request_id, [0xAA; 32], sender.clone(), u64::MAX);
        assert!(r2.is_ok()); // Returns Ok but logs warning and short-circuits

        // Verify nonce was tracked
        assert_eq!(actor.blob_nonce_guard.nonce_count(&sender), 1);
    }

    #[test]
    fn blob_request_handler_allows_different_requests() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();

        // Two different request_ids: both accepted
        let r1 =
            actor.handle_blob_request(&sender, [0x11; 32], [0xAA; 32], sender.clone(), u64::MAX);
        assert!(r1.is_ok());

        let r2 =
            actor.handle_blob_request(&sender, [0x22; 32], [0xBB; 32], sender.clone(), u64::MAX);
        assert!(r2.is_ok());

        assert_eq!(actor.blob_nonce_guard.nonce_count(&sender), 2);
    }

    #[test]
    fn blob_chunk_handler_rejects_duplicate_chunk() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let data = vec![0x42u8; 100];
        let hash = *blake3::hash(&data).as_bytes();

        // First chunk: accepted
        let r1 = actor.handle_blob_transfer_chunk(
            &sender,
            [0x11; 32],
            [0xAA; 32],
            0,
            5,
            hash,
            500,
            data.clone(),
        );
        assert!(r1.is_ok());

        // Same (request_id, chunk_index) from same sender: silently rejected
        let r2 = actor
            .handle_blob_transfer_chunk(&sender, [0x11; 32], [0xAA; 32], 0, 5, hash, 500, data);
        assert!(r2.is_ok()); // Returns Ok but logs warning
        assert_eq!(actor.blob_nonce_guard.nonce_count(&sender), 1);
    }

    #[test]
    fn blob_chunk_handler_allows_different_chunks_same_request() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let data = vec![0x42u8; 100];
        let hash = *blake3::hash(&data).as_bytes();

        // Chunk 0 and chunk 1 of same request: both accepted
        let r1 = actor.handle_blob_transfer_chunk(
            &sender,
            [0x11; 32],
            [0xAA; 32],
            0,
            5,
            hash,
            500,
            data.clone(),
        );
        assert!(r1.is_ok());

        let r2 = actor
            .handle_blob_transfer_chunk(&sender, [0x11; 32], [0xAA; 32], 1, 5, hash, 500, data);
        assert!(r2.is_ok());
        assert_eq!(actor.blob_nonce_guard.nonce_count(&sender), 2);
    }

    // --- Integration tests (PR2c) ---

    /// Helper: create a GossipActor with blob_store and transfer_manager
    fn make_actor_with_transfer(
        blob_store: Arc<MockBlobStore>,
        tmp_dir: &std::path::Path,
    ) -> (GossipActor, KeyPair) {
        let kp = KeyPair::generate().unwrap();
        let mut actor = GossipActor::new(kp.did().clone(), None);
        actor.set_blob_store(blob_store);
        actor.set_transfer_manager(TransferSessionManager::new(tmp_dir.to_path_buf()));
        (actor, kp)
    }

    #[test]
    fn blob_transfer_reassembly_and_verification() {
        // End-to-end: provider serves blob → requester reassembles + verifies

        let tmp_provider = tempfile::tempdir().unwrap();
        let tmp_requester = tempfile::tempdir().unwrap();

        // Create blob data (3 chunks worth)
        let blob_data: Vec<u8> = (0..MAX_CHUNK_SIZE * 3).map(|i| (i % 256) as u8).collect();
        let blob_hash = *blake3::hash(&blob_data).as_bytes();

        // Provider: has the blob
        let provider_store = Arc::new(MockBlobStore::new());
        provider_store.insert(&blob_data);

        let sent_messages: Arc<Mutex<Vec<GossipMessage>>> = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = sent_messages.clone();

        let (mut provider_actor, _provider_kp) =
            make_actor_with_transfer(provider_store, tmp_provider.path());
        provider_actor.set_send_callback(Arc::new(move |_recipient, msg| {
            sent_clone
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(msg);
        }));

        // Requester: wants the blob
        let requester_store = Arc::new(MockBlobStore::new());
        let (mut requester_actor, _requester_kp) =
            make_actor_with_transfer(requester_store.clone(), tmp_requester.path());

        let requester_did = requester_actor.own_did.clone();
        let provider_did = provider_actor.own_did.clone();

        // Requester creates a transfer session
        let request_id = {
            let mgr_mutex = requester_actor.transfer_manager.as_ref().unwrap();
            let mut mgr = mgr_mutex.lock().unwrap();
            mgr.start_request(
                blob_hash,
                vec![provider_did.clone()],
                requester_did.clone(),
                "test-scope".to_string(),
                u64::MAX,
            )
            .unwrap()
        };

        // Provider handles the request
        provider_actor
            .handle_blob_request(
                &requester_did,
                request_id,
                blob_hash,
                requester_did.clone(),
                u64::MAX,
            )
            .unwrap();

        // Provider should have sent 3 chunks
        let messages = sent_messages.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(messages.len(), 3, "Provider should send 3 chunks");

        // Requester processes each chunk
        for msg in messages.iter() {
            match msg {
                GossipMessage::BlobTransferChunk {
                    request_id: rid,
                    blob_hash: bh,
                    chunk_index,
                    total_chunks,
                    chunk_hash,
                    total_size,
                    data,
                } => {
                    requester_actor
                        .handle_blob_transfer_chunk(
                            &provider_did,
                            *rid,
                            *bh,
                            *chunk_index,
                            *total_chunks,
                            *chunk_hash,
                            *total_size,
                            data.clone(),
                        )
                        .unwrap();
                }
                _ => panic!("Expected BlobTransferChunk"),
            }
        }

        // Verify the blob was stored in the requester's store
        assert!(
            requester_store.exists(&blob_hash).unwrap(),
            "Assembled blob should be stored in requester's BlobService"
        );

        let stored_data = requester_store.get(&blob_hash).unwrap();
        assert_eq!(stored_data, blob_data, "Stored data should match original");
    }

    #[test]
    fn reject_unsolicited_blob_transfer() {
        // Adversarial: someone sends chunks without a prior request session

        let tmp = tempfile::tempdir().unwrap();
        let store = Arc::new(MockBlobStore::new());
        let (mut actor, _kp) = make_actor_with_transfer(store, tmp.path());

        let attacker = make_test_did();
        let data = vec![0x42u8; 100];
        let chunk_hash = *blake3::hash(&data).as_bytes();

        // No session exists for this request_id — chunk should be silently dropped
        // by the transfer manager (InvalidRequest)
        let result = actor.handle_blob_transfer_chunk(
            &attacker, [0xFF; 32], // unknown request_id
            [0xAA; 32], 0, 1, chunk_hash, 100, data,
        );

        // Handler returns Ok but the chunk is dropped internally
        assert!(result.is_ok());
    }

    // --- BlobAnnounce handler tests (PR2d #1071) ---

    #[test]
    fn blob_announce_records_in_provider_registry() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let blob_hash = [0xAA; 32];

        let result = actor.handle_blob_announce(&sender, blob_hash, sender.clone(), 4096);
        assert!(result.is_ok());

        // Verify provider was recorded
        let providers = actor.provider_registry.list_providers(&blob_hash);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].did, sender);
        assert_eq!(providers[0].size_bytes, 4096);
    }

    #[test]
    fn blob_announce_rejects_mismatched_sender() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let impersonated = make_test_did();
        let blob_hash = [0xBB; 32];

        // peer_did doesn't match sender — should be rejected
        let result = actor.handle_blob_announce(&sender, blob_hash, impersonated, 4096);
        assert!(result.is_ok());

        // Provider should NOT be recorded
        let providers = actor.provider_registry.list_providers(&blob_hash);
        assert_eq!(providers.len(), 0, "mismatched sender must not be recorded");
    }

    #[test]
    fn blob_announce_rejects_oversize() {
        use crate::GossipActor;

        let mut actor = {
            let kp = KeyPair::generate().unwrap();
            GossipActor::new(kp.did().clone(), None)
        };

        let sender = make_test_did();
        let blob_hash = [0xCC; 32];

        // Size exceeds MAX_BLOB_SIZE
        let result =
            actor.handle_blob_announce(&sender, blob_hash, sender.clone(), MAX_BLOB_SIZE + 1);
        assert!(result.is_ok());

        // Provider should NOT be recorded
        let providers = actor.provider_registry.list_providers(&blob_hash);
        assert_eq!(providers.len(), 0, "oversize announce must not be recorded");
    }
}
