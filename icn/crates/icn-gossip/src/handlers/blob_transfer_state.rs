//! Blob transfer state machine (PR2c — #1070)
//!
//! Manages the lifecycle of blob transfer sessions from request through
//! chunk reception, reassembly, verification, and storage. Each session
//! is keyed by `RequestId` and follows monotonic state transitions:
//!
//! ```text
//! Requested → Receiving { bitmap } → Assembling → Verified → Stored
//!                                         ↓
//!                                   Failed(ErrCode) [terminal]
//! ```
//!
//! # Resource Bounds
//!
//! - Max in-flight requests per peer: 8
//! - Max concurrent reassemblies (global): 16
//! - Chunk size: 64 KB (enforced by handler, not here)
//! - Blob max size: 10 MB
//!
//! # Disk Layout
//!
//! Chunk bytes are stored on disk during transfer:
//! `<partial_dir>/<hex(request_id)>/chunk_<NNNN>.bin`
//!
//! Temp dir is cleaned up on session completion or expiry.

use icn_identity::Did;
use icn_kernel_api::error::ErrCode;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::types::ContentHash;

/// Type alias for request identifiers (32-byte nonce)
pub type RequestId = [u8; 32];

/// Maximum in-flight blob requests per peer
pub const MAX_REQUESTS_PER_PEER: usize = 8;

/// Maximum concurrent reassemblies (global)
pub const MAX_CONCURRENT_REASSEMBLIES: usize = 16;

/// Maximum blob size in bytes (10 MB)
const MAX_BLOB_SIZE: u64 = 10 * 1024 * 1024;

/// State of a blob transfer session.
///
/// Transitions are monotonic — only one transition per event, no jumping.
#[derive(Debug, Clone)]
pub enum TransferState {
    /// Request initiated, waiting for first chunk
    Requested,
    /// Receiving chunks; bitmap tracks which chunks have arrived
    Receiving {
        chunks_received: Vec<bool>,
        received_bytes: u64,
    },
    /// All chunks received, reassembly in progress
    Assembling,
    /// Reassembly complete, hash verified
    Verified,
    /// Blob committed to BlobStore
    Stored,
    /// Terminal failure state
    Failed(ErrCode),
}

/// A single blob transfer session.
#[derive(Debug)]
pub struct TransferSession {
    pub request_id: RequestId,
    pub blob_hash: ContentHash,
    pub provider_did: Did,
    pub requester_did: Did,
    pub scope_id: String,
    pub expires_at: u64,
    pub total_chunks: u32,
    pub total_size: u64,
    pub state: TransferState,
    pub created_at: u64,
}

/// Result of receiving a chunk.
#[derive(Debug, PartialEq, Eq)]
pub enum ChunkResult {
    /// Chunk accepted, more chunks needed
    Accepted,
    /// All chunks received, ready for assembly
    AllChunksReceived,
}

/// Job context for two-phase assembly (phase 2 runs without lock held).
#[derive(Debug)]
pub struct AssemblyJob {
    /// Directory containing chunk files
    pub partial_dir: PathBuf,
    /// Number of chunks to reassemble
    pub total_chunks: u32,
    /// Expected blake3 hash of the complete blob
    pub blob_hash: ContentHash,
}

impl AssemblyJob {
    /// Execute the assembly: read chunks, concatenate, verify hash.
    ///
    /// This runs WITHOUT the manager lock held, so it won't stall chunk handling.
    pub fn execute(&self) -> Result<Vec<u8>, ErrCode> {
        let mut assembled = Vec::new();
        for i in 0..self.total_chunks {
            let chunk_path = self.partial_dir.join(format!("chunk_{:04}.bin", i));
            let chunk_data = std::fs::read(&chunk_path).map_err(|e| {
                warn!(
                    chunk_index = i,
                    path = %chunk_path.display(),
                    error = %e,
                    "Failed to read chunk file during assembly"
                );
                ErrCode::InvalidRequest
            })?;
            assembled.extend_from_slice(&chunk_data);
        }

        let actual_hash = *blake3::hash(&assembled).as_bytes();
        if actual_hash != self.blob_hash {
            warn!(
                expected = %hex::encode(self.blob_hash),
                actual = %hex::encode(actual_hash),
                "Blob hash mismatch after reassembly"
            );
            return Err(ErrCode::InvalidRequest);
        }

        Ok(assembled)
    }
}

/// Write a chunk to disk atomically (write temp → rename).
fn write_chunk(session_dir: &Path, chunk_index: u32, data: &[u8]) -> Result<(), ErrCode> {
    let chunk_path = session_dir.join(format!("chunk_{:04}.bin", chunk_index));
    let tmp_path = session_dir.join(format!("chunk_{:04}.tmp", chunk_index));

    std::fs::write(&tmp_path, data).map_err(|e| {
        warn!(
            chunk_index = chunk_index,
            path = %tmp_path.display(),
            error = %e,
            "Failed to write chunk temp file"
        );
        ErrCode::Busy
    })?;

    std::fs::rename(&tmp_path, &chunk_path).map_err(|e| {
        warn!(
            chunk_index = chunk_index,
            error = %e,
            "Failed to rename chunk temp to final"
        );
        ErrCode::Busy
    })?;

    Ok(())
}

/// Manages all active blob transfer sessions.
pub struct TransferSessionManager {
    /// Active sessions keyed by RequestId
    sessions: HashMap<RequestId, TransferSession>,
    /// Number of active requests per peer (for per-peer limits)
    per_peer_counts: HashMap<Did, usize>,
    /// Number of sessions currently in Assembling state
    active_reassemblies: usize,
    /// Base directory for partial chunk storage
    partial_dir: PathBuf,
}

impl TransferSessionManager {
    /// Create a new manager with the given partial directory.
    pub fn new(partial_dir: PathBuf) -> Self {
        Self {
            sessions: HashMap::new(),
            per_peer_counts: HashMap::new(),
            active_reassemblies: 0,
            partial_dir,
        }
    }

    /// Start a new transfer request.
    ///
    /// Generates a request_id internally, picks the first provider from the list
    /// (PR2d will control ordering/ranking).
    ///
    /// Returns the generated RequestId on success.
    pub fn start_request(
        &mut self,
        blob_hash: ContentHash,
        providers: Vec<Did>,
        requester_did: Did,
        scope_id: String,
        expires_at: u64,
    ) -> Result<RequestId, ErrCode> {
        if providers.is_empty() {
            return Err(ErrCode::InvalidRequest);
        }

        // Check per-peer limit for the requester
        let count = self
            .per_peer_counts
            .get(&requester_did)
            .copied()
            .unwrap_or(0);
        if count >= MAX_REQUESTS_PER_PEER {
            return Err(ErrCode::Busy);
        }

        // Generate request_id from blake3(blob_hash || requester_did || timestamp)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut hasher = blake3::Hasher::new();
        hasher.update(&blob_hash);
        hasher.update(requester_did.as_str().as_bytes());
        hasher.update(&now.to_le_bytes());
        let request_id: RequestId = *hasher.finalize().as_bytes();

        // Pick first provider (PR2d will add ranking)
        let provider_did = providers
            .into_iter()
            .next()
            .ok_or(ErrCode::InvalidRequest)?;

        let session = TransferSession {
            request_id,
            blob_hash,
            provider_did,
            requester_did: requester_did.clone(),
            scope_id,
            expires_at,
            total_chunks: 0, // Set on first chunk
            total_size: 0,   // Set on first chunk
            state: TransferState::Requested,
            created_at: now / 1_000_000_000, // Convert nanos to secs
        };

        self.sessions.insert(request_id, session);
        *self.per_peer_counts.entry(requester_did).or_insert(0) += 1;

        // Create partial directory for this request
        let session_dir = self.session_dir(&request_id);
        if let Err(e) = std::fs::create_dir_all(&session_dir) {
            warn!(
                request_id = %hex::encode(request_id),
                error = %e,
                "Failed to create partial dir for transfer session"
            );
            // Clean up session on failure
            self.sessions.remove(&request_id);
            return Err(ErrCode::Busy);
        }

        Ok(request_id)
    }

    /// Receive a chunk for an existing session.
    ///
    /// Called by the handler AFTER chunk_hash + replay checks pass.
    pub fn receive_chunk(
        &mut self,
        request_id: RequestId,
        sender: &Did,
        chunk_index: u32,
        total_chunks: u32,
        total_size: u64,
        data: &[u8],
    ) -> Result<ChunkResult, ErrCode> {
        // Compute session dir before borrowing sessions mutably
        let session_dir = self.session_dir(&request_id);

        let session = self
            .sessions
            .get_mut(&request_id)
            .ok_or(ErrCode::InvalidRequest)?;

        // Check expiry
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if session.expires_at > 0 && now > session.expires_at {
            session.state = TransferState::Failed(ErrCode::Expired);
            return Err(ErrCode::Expired);
        }

        // Check provider
        if *sender != session.provider_did {
            return Err(ErrCode::Forbidden);
        }

        // Validate total_size
        if total_size > MAX_BLOB_SIZE {
            session.state = TransferState::Failed(ErrCode::Oversize);
            return Err(ErrCode::Oversize);
        }

        match &mut session.state {
            TransferState::Requested => {
                // First chunk: initialize receiving state
                if total_chunks == 0 {
                    session.state = TransferState::Failed(ErrCode::InvalidRequest);
                    return Err(ErrCode::InvalidRequest);
                }

                session.total_chunks = total_chunks;
                session.total_size = total_size;

                let mut bitmap = vec![false; total_chunks as usize];
                bitmap[chunk_index as usize] = true;

                // Write chunk to disk
                write_chunk(&session_dir, chunk_index, data)?;

                session.state = TransferState::Receiving {
                    chunks_received: bitmap,
                    received_bytes: data.len() as u64,
                };

                if total_chunks == 1 {
                    Ok(ChunkResult::AllChunksReceived)
                } else {
                    Ok(ChunkResult::Accepted)
                }
            }
            TransferState::Receiving {
                chunks_received,
                received_bytes,
            } => {
                // Subsequent chunks: validate consistency
                if total_chunks != session.total_chunks {
                    session.state = TransferState::Failed(ErrCode::InvalidRequest);
                    return Err(ErrCode::InvalidRequest);
                }
                if total_size != session.total_size {
                    session.state = TransferState::Failed(ErrCode::InvalidRequest);
                    return Err(ErrCode::InvalidRequest);
                }

                if chunk_index >= total_chunks {
                    return Err(ErrCode::InvalidRequest);
                }

                // Duplicate chunk (already received)
                if chunks_received[chunk_index as usize] {
                    return Ok(ChunkResult::Accepted);
                }

                // Write chunk to disk
                write_chunk(&session_dir, chunk_index, data)?;

                chunks_received[chunk_index as usize] = true;
                *received_bytes += data.len() as u64;

                if chunks_received.iter().all(|&b| b) {
                    Ok(ChunkResult::AllChunksReceived)
                } else {
                    Ok(ChunkResult::Accepted)
                }
            }
            TransferState::Assembling | TransferState::Verified | TransferState::Stored => {
                // Session already past receiving phase
                Err(ErrCode::InvalidRequest)
            }
            TransferState::Failed(_) => Err(ErrCode::InvalidRequest),
        }
    }

    /// Phase 1 of two-phase assembly: mark as Assembling and return job.
    ///
    /// Must be called WITH the lock held. Returns the AssemblyJob for
    /// phase 2 (which runs WITHOUT the lock).
    pub fn begin_assembly(&mut self, request_id: RequestId) -> Result<AssemblyJob, ErrCode> {
        if self.active_reassemblies >= MAX_CONCURRENT_REASSEMBLIES {
            return Err(ErrCode::Busy);
        }

        // Compute dir before mutable borrow
        let partial_dir = self.session_dir(&request_id);

        let session = self
            .sessions
            .get_mut(&request_id)
            .ok_or(ErrCode::InvalidRequest)?;

        // Only transition from Receiving with all chunks
        match &session.state {
            TransferState::Receiving {
                chunks_received, ..
            } => {
                if !chunks_received.iter().all(|&b| b) {
                    return Err(ErrCode::InvalidRequest);
                }
            }
            _ => return Err(ErrCode::InvalidRequest),
        }

        let total_chunks = session.total_chunks;
        let blob_hash = session.blob_hash;
        session.state = TransferState::Assembling;
        self.active_reassemblies += 1;

        Ok(AssemblyJob {
            partial_dir,
            total_chunks,
            blob_hash,
        })
    }

    /// Phase 3 of two-phase assembly: finalize state after assembly.
    ///
    /// Must be called WITH the lock held after phase 2 completes.
    pub fn complete_assembly(
        &mut self,
        request_id: RequestId,
        success: bool,
    ) -> Result<(), ErrCode> {
        let session = self
            .sessions
            .get_mut(&request_id)
            .ok_or(ErrCode::InvalidRequest)?;

        match session.state {
            TransferState::Assembling => {
                if success {
                    session.state = TransferState::Verified;
                } else {
                    session.state = TransferState::Failed(ErrCode::InvalidRequest);
                }
                self.active_reassemblies = self.active_reassemblies.saturating_sub(1);
                Ok(())
            }
            _ => Err(ErrCode::InvalidRequest),
        }
    }

    /// Mark a session as Stored after blob has been committed.
    pub fn mark_stored(&mut self, request_id: RequestId) {
        if let Some(session) = self.sessions.get_mut(&request_id) {
            if matches!(session.state, TransferState::Verified) {
                session.state = TransferState::Stored;
            }
        }
    }

    /// Clean up a session: remove from registry, delete temp dir, update counters.
    pub fn cleanup_session(&mut self, request_id: RequestId) {
        if let Some(session) = self.sessions.remove(&request_id) {
            // Decrement per-peer counter
            if let Some(count) = self.per_peer_counts.get_mut(&session.requester_did) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.per_peer_counts.remove(&session.requester_did);
                }
            }

            // Clean up temp directory
            let session_dir = self.session_dir(&request_id);
            if session_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&session_dir) {
                    warn!(
                        request_id = %hex::encode(request_id),
                        error = %e,
                        "Failed to clean up partial dir"
                    );
                }
            }
        }
    }

    /// Clean up all expired sessions. Returns count of cleaned sessions.
    pub fn cleanup_expired(&mut self, now: u64) -> usize {
        let expired: Vec<RequestId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.expires_at > 0 && now > s.expires_at)
            .map(|(id, _)| *id)
            .collect();

        let count = expired.len();
        for id in expired {
            // If assembling, decrement counter
            if let Some(s) = self.sessions.get(&id) {
                if matches!(s.state, TransferState::Assembling) {
                    self.active_reassemblies = self.active_reassemblies.saturating_sub(1);
                }
            }
            self.cleanup_session(id);
        }
        count
    }

    /// Get session metadata (for creating receipts etc.)
    pub fn get_session(&self, request_id: &RequestId) -> Option<&TransferSession> {
        self.sessions.get(request_id)
    }

    /// Number of active sessions
    #[cfg(test)]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Number of active reassemblies
    #[cfg(test)]
    pub fn active_reassemblies(&self) -> usize {
        self.active_reassemblies
    }

    /// Number of active sessions for a specific peer
    #[cfg(test)]
    pub fn peer_request_count(&self, peer: &Did) -> usize {
        self.per_peer_counts.get(peer).copied().unwrap_or(0)
    }

    // --- Internal helpers ---

    fn session_dir(&self, request_id: &RequestId) -> PathBuf {
        self.partial_dir.join(hex::encode(request_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn make_manager(dir: &Path) -> TransferSessionManager {
        TransferSessionManager::new(dir.to_path_buf())
    }

    fn start_session(
        manager: &mut TransferSessionManager,
        blob_hash: ContentHash,
        provider: &Did,
        requester: &Did,
    ) -> RequestId {
        manager
            .start_request(
                blob_hash,
                vec![provider.clone()],
                requester.clone(),
                "test-scope".to_string(),
                u64::MAX, // far future
            )
            .expect("start_request should succeed")
    }

    fn make_chunk_data(index: u32) -> Vec<u8> {
        let mut data = vec![0u8; 1024];
        data[0..4].copy_from_slice(&index.to_be_bytes());
        data
    }

    fn blob_hash_for_chunks(total_chunks: u32) -> ContentHash {
        let mut assembled = Vec::new();
        for i in 0..total_chunks {
            assembled.extend_from_slice(&make_chunk_data(i));
        }
        *blake3::hash(&assembled).as_bytes()
    }

    #[test]
    fn state_machine_transitions_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let provider = make_did();
        let requester = make_did();

        let total_chunks = 3u32;
        let blob_hash = blob_hash_for_chunks(total_chunks);
        let rid = start_session(&mut mgr, blob_hash, &provider, &requester);

        // Initially Requested
        assert!(matches!(
            mgr.get_session(&rid).unwrap().state,
            TransferState::Requested
        ));

        // First chunk → Receiving
        let data0 = make_chunk_data(0);
        let r = mgr.receive_chunk(
            rid,
            &provider,
            0,
            total_chunks,
            total_chunks as u64 * 1024,
            &data0,
        );
        assert_eq!(r.unwrap(), ChunkResult::Accepted);
        assert!(matches!(
            mgr.get_session(&rid).unwrap().state,
            TransferState::Receiving { .. }
        ));

        // Second chunk → still Receiving
        let data1 = make_chunk_data(1);
        let r = mgr.receive_chunk(
            rid,
            &provider,
            1,
            total_chunks,
            total_chunks as u64 * 1024,
            &data1,
        );
        assert_eq!(r.unwrap(), ChunkResult::Accepted);

        // Third chunk → AllChunksReceived
        let data2 = make_chunk_data(2);
        let r = mgr.receive_chunk(
            rid,
            &provider,
            2,
            total_chunks,
            total_chunks as u64 * 1024,
            &data2,
        );
        assert_eq!(r.unwrap(), ChunkResult::AllChunksReceived);

        // Begin assembly → Assembling
        let job = mgr.begin_assembly(rid).unwrap();
        assert!(matches!(
            mgr.get_session(&rid).unwrap().state,
            TransferState::Assembling
        ));
        assert_eq!(mgr.active_reassemblies(), 1);

        // Execute assembly (reads files, verifies hash)
        let assembled = job.execute().unwrap();
        assert_eq!(assembled.len(), total_chunks as usize * 1024);

        // Complete assembly → Verified
        mgr.complete_assembly(rid, true).unwrap();
        assert!(matches!(
            mgr.get_session(&rid).unwrap().state,
            TransferState::Verified
        ));
        assert_eq!(mgr.active_reassemblies(), 0);

        // Mark stored → Stored
        mgr.mark_stored(rid);
        assert!(matches!(
            mgr.get_session(&rid).unwrap().state,
            TransferState::Stored
        ));

        // Cleanup
        mgr.cleanup_session(rid);
        assert_eq!(mgr.session_count(), 0);
    }

    #[test]
    fn reject_chunk_before_request() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let sender = make_did();

        let result = mgr.receive_chunk([0xFF; 32], &sender, 0, 5, 5000, &[0u8; 100]);
        assert_eq!(result.unwrap_err(), ErrCode::InvalidRequest);
    }

    #[test]
    fn reject_wrong_provider() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let provider = make_did();
        let requester = make_did();
        let wrong_sender = make_did();

        let rid = start_session(&mut mgr, [0xAA; 32], &provider, &requester);

        let result = mgr.receive_chunk(rid, &wrong_sender, 0, 1, 1024, &[0u8; 100]);
        assert_eq!(result.unwrap_err(), ErrCode::Forbidden);
    }

    #[test]
    fn reject_expired() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let provider = make_did();
        let requester = make_did();

        // Start with expires_at in the past
        let rid = mgr
            .start_request(
                [0xAA; 32],
                vec![provider.clone()],
                requester.clone(),
                "test".to_string(),
                1, // 1 second after epoch = expired
            )
            .unwrap();

        let result = mgr.receive_chunk(rid, &provider, 0, 1, 1024, &[0u8; 100]);
        assert_eq!(result.unwrap_err(), ErrCode::Expired);
    }

    #[test]
    fn reject_total_size_oversize() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let provider = make_did();
        let requester = make_did();

        let rid = start_session(&mut mgr, [0xAA; 32], &provider, &requester);

        let result = mgr.receive_chunk(
            rid,
            &provider,
            0,
            200,
            MAX_BLOB_SIZE + 1, // Over 10 MB
            &[0u8; 100],
        );
        assert_eq!(result.unwrap_err(), ErrCode::Oversize);
    }

    #[test]
    fn enforce_max_concurrent_reassemblies() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());

        // Fill up reassembly slots. Use multiple requesters to avoid
        // hitting the per-peer limit (MAX_REQUESTS_PER_PEER = 8).
        for i in 0..MAX_CONCURRENT_REASSEMBLIES {
            let provider = make_did();
            let requester = make_did(); // fresh requester each time
            let _ = i;
            let data = make_chunk_data(0);
            let expected_hash = *blake3::hash(&data).as_bytes();
            let rid = start_session(&mut mgr, expected_hash, &provider, &requester);

            mgr.receive_chunk(rid, &provider, 0, 1, data.len() as u64, &data)
                .unwrap();
            mgr.begin_assembly(rid).unwrap();
        }

        assert_eq!(mgr.active_reassemblies(), MAX_CONCURRENT_REASSEMBLIES);

        // One more should fail with Busy at begin_assembly
        let provider = make_did();
        let requester = make_did();
        let data = make_chunk_data(99);
        let expected_hash = *blake3::hash(&data).as_bytes();
        let rid = start_session(&mut mgr, expected_hash, &provider, &requester);
        mgr.receive_chunk(rid, &provider, 0, 1, data.len() as u64, &data)
            .unwrap();

        let result = mgr.begin_assembly(rid);
        assert_eq!(result.unwrap_err(), ErrCode::Busy);
    }

    #[test]
    fn cleanup_on_expiry_deletes_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let provider = make_did();
        let requester = make_did();

        // Start with near-future expiry
        let rid = mgr
            .start_request(
                [0xAA; 32],
                vec![provider.clone()],
                requester.clone(),
                "test".to_string(),
                100, // expires at 100 seconds
            )
            .unwrap();

        let session_dir = tmp.path().join(hex::encode(rid));
        assert!(session_dir.exists());

        // Cleanup with now=200 (past expiry)
        let cleaned = mgr.cleanup_expired(200);
        assert_eq!(cleaned, 1);
        assert_eq!(mgr.session_count(), 0);
        assert!(!session_dir.exists());
    }

    #[test]
    fn receipt_hash_stable_across_runs() {
        use icn_kernel_api::proofs::ArtifactReceipt;

        let r1 = ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:provider".to_string(),
            "did:icn:requester".to_string(),
            [0xBB; 32],
            "coop:scope".to_string(),
            1700000000,
        );
        let r2 = ArtifactReceipt::new(
            [0xAA; 32],
            "did:icn:provider".to_string(),
            "did:icn:requester".to_string(),
            [0xBB; 32],
            "coop:scope".to_string(),
            1700000000,
        );

        assert_eq!(r1.receipt_hash, r2.receipt_hash);
        assert!(r1.verify_binding());
        assert!(r2.verify_binding());
    }

    #[test]
    fn per_peer_request_limit_enforced() {
        let tmp = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(tmp.path());
        let requester = make_did();

        // Fill up per-peer slots
        for i in 0..MAX_REQUESTS_PER_PEER {
            let provider = make_did();
            let mut blob_hash = [0u8; 32];
            blob_hash[0] = i as u8;
            mgr.start_request(
                blob_hash,
                vec![provider],
                requester.clone(),
                "test".to_string(),
                u64::MAX,
            )
            .unwrap();
        }

        assert_eq!(mgr.peer_request_count(&requester), MAX_REQUESTS_PER_PEER);

        // One more should fail with Busy
        let result = mgr.start_request(
            [0xFF; 32],
            vec![make_did()],
            requester.clone(),
            "test".to_string(),
            u64::MAX,
        );
        assert_eq!(result.unwrap_err(), ErrCode::Busy);
    }

    #[test]
    fn assemble_verifies_blob_hash_match_and_mismatch() {
        let tmp = tempfile::tempdir().unwrap();

        // Test match: correct hash
        {
            let mut mgr = make_manager(tmp.path());
            let provider = make_did();
            let requester = make_did();

            let data = make_chunk_data(0);
            let correct_hash = *blake3::hash(&data).as_bytes();
            let rid = start_session(&mut mgr, correct_hash, &provider, &requester);
            mgr.receive_chunk(rid, &provider, 0, 1, data.len() as u64, &data)
                .unwrap();

            let job = mgr.begin_assembly(rid).unwrap();
            let result = job.execute();
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), data);
        }

        // Test mismatch: wrong hash
        {
            let mut mgr = make_manager(tmp.path());
            let provider = make_did();
            let requester = make_did();

            let data = make_chunk_data(0);
            let wrong_hash = [0xFF; 32]; // doesn't match data
            let rid = start_session(&mut mgr, wrong_hash, &provider, &requester);
            mgr.receive_chunk(rid, &provider, 0, 1, data.len() as u64, &data)
                .unwrap();

            let job = mgr.begin_assembly(rid).unwrap();
            let result = job.execute();
            assert_eq!(result.unwrap_err(), ErrCode::InvalidRequest);
        }
    }
}
