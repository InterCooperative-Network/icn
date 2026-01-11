//! Snapshot coordinator for distributed snapshots
//!
//! Implements the Chandy-Lamport distributed snapshot algorithm adapted
//! for ICN's gossip-based architecture.

use crate::protocol::{
    SnapshotConfig, SnapshotId, SnapshotMessage, SnapshotMetadata, SnapshotState,
};
use crate::StateSnapshot;
use anyhow::{anyhow, Context, Result};
use icn_identity::Did;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Snapshot coordinator manages distributed snapshot operations
pub struct SnapshotCoordinator {
    /// This node's DID
    own_did: Did,
    /// Configuration
    config: SnapshotConfig,
    /// Active snapshots (snapshot_id -> metadata)
    active_snapshots: Arc<RwLock<HashMap<SnapshotId, SnapshotMetadata>>>,
    /// Completed snapshots
    completed_snapshots: Arc<RwLock<HashMap<SnapshotId, CompletedSnapshot>>>,
    /// Chunk reassembly buffer (snapshot_id -> ChunkBuffer)
    chunk_buffers: Arc<RwLock<HashMap<SnapshotId, ChunkBuffer>>>,
}

/// Buffer for reassembling chunked state data
#[derive(Debug)]
struct ChunkBuffer {
    sender: Did,
    received_chunks: HashMap<u32, Vec<u8>>,
    data_size: usize,
}

/// A completed snapshot with global state
#[derive(Debug, Clone)]
pub struct CompletedSnapshot {
    pub snapshot_id: SnapshotId,
    pub coordinator: Did,
    pub completed_at: u64,
    pub global_state_root: [u8; 32],
    pub participants: Vec<Did>,
    pub participant_states: HashMap<Did, Vec<u8>>,
}

impl SnapshotCoordinator {
    /// Create a new snapshot coordinator
    pub fn new(own_did: Did, config: SnapshotConfig) -> Self {
        Self {
            own_did,
            config,
            active_snapshots: Arc::new(RwLock::new(HashMap::new())),
            completed_snapshots: Arc::new(RwLock::new(HashMap::new())),
            chunk_buffers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initiate a new distributed snapshot
    ///
    /// Returns the snapshot ID and InitiateSnapshot message to broadcast
    pub async fn initiate_snapshot(
        &self,
        participants: Vec<Did>,
    ) -> Result<(SnapshotId, SnapshotMessage)> {
        if participants.len() < self.config.min_participants {
            return Err(anyhow!(
                "Insufficient participants: {} < {}",
                participants.len(),
                self.config.min_participants
            ));
        }

        let snapshot_id = SnapshotId::generate();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let metadata = SnapshotMetadata::new(
            snapshot_id.clone(),
            self.own_did.clone(),
            participants.clone(),
        );

        self.active_snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), metadata);

        let msg = SnapshotMessage::InitiateSnapshot {
            snapshot_id: snapshot_id.clone(),
            initiator: self.own_did.clone(),
            timestamp: now,
            participants,
        };

        info!("Initiated distributed snapshot: {}", snapshot_id.to_hex());

        Ok((snapshot_id, msg))
    }

    /// Handle incoming snapshot message
    pub async fn handle_message(
        &self,
        _sender: &Did,
        message: SnapshotMessage,
    ) -> Result<Vec<SnapshotMessage>> {
        match message {
            SnapshotMessage::InitiateSnapshot {
                snapshot_id,
                initiator,
                timestamp,
                participants,
            } => {
                self.handle_initiate(snapshot_id, initiator, timestamp, participants)
                    .await
            }
            SnapshotMessage::SnapshotAck {
                snapshot_id,
                node,
                state_hash,
                state_size,
            } => {
                self.handle_ack(snapshot_id, node, state_hash, state_size)
                    .await
            }
            SnapshotMessage::Marker {
                snapshot_id,
                sender,
            } => self.handle_marker(snapshot_id, sender.clone()).await,
            SnapshotMessage::RequestState {
                snapshot_id,
                requester,
            } => self.handle_state_request(snapshot_id, requester).await,
            SnapshotMessage::StateChunk {
                snapshot_id,
                sender: _,
                chunk_index,
                total_chunks,
                data,
            } => {
                self.handle_state_chunk(snapshot_id, chunk_index, total_chunks, data)
                    .await
            }
            SnapshotMessage::SnapshotComplete {
                snapshot_id,
                coordinator,
                global_state_root,
                participants,
            } => {
                self.handle_complete(snapshot_id, coordinator, global_state_root, participants)
                    .await
            }
            SnapshotMessage::VerifySnapshot {
                snapshot_id,
                requester,
                expected_root,
            } => {
                self.handle_verify_request(snapshot_id, requester, expected_root)
                    .await
            }
            SnapshotMessage::VerificationResult {
                snapshot_id,
                verifier,
                valid,
                computed_root,
            } => {
                self.handle_verification_result(snapshot_id, verifier, valid, computed_root)
                    .await
            }
        }
    }

    /// Handle InitiateSnapshot message
    async fn handle_initiate(
        &self,
        snapshot_id: SnapshotId,
        initiator: Did,
        _timestamp: u64,
        participants: Vec<Did>,
    ) -> Result<Vec<SnapshotMessage>> {
        // Check if we're a participant
        if !participants.contains(&self.own_did) {
            debug!("Not a participant in snapshot {}", snapshot_id.to_hex());
            return Ok(vec![]);
        }

        info!(
            "Participating in snapshot {} initiated by {}",
            snapshot_id.to_hex(),
            initiator
        );

        // Record local state immediately
        let local_state = self.capture_local_state().await?;
        let state_hash = self.hash_state(&local_state);

        // Create metadata
        let mut metadata = SnapshotMetadata::new(snapshot_id.clone(), initiator, participants);
        metadata.state = SnapshotState::WaitingForMarkers {
            received_from: HashSet::new(),
        };

        self.active_snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), metadata);

        // Send ACK to initiator
        let ack = SnapshotMessage::SnapshotAck {
            snapshot_id: snapshot_id.clone(),
            node: self.own_did.clone(),
            state_hash,
            state_size: local_state.len() as u64,
        };

        // Send markers to all peers
        let marker = SnapshotMessage::Marker {
            snapshot_id,
            sender: self.own_did.clone(),
        };

        Ok(vec![ack, marker])
    }

    /// Handle SnapshotAck message
    async fn handle_ack(
        &self,
        snapshot_id: SnapshotId,
        node: Did,
        state_hash: [u8; 32],
        _state_size: u64,
    ) -> Result<Vec<SnapshotMessage>> {
        let mut snapshots = self.active_snapshots.write().await;

        if let Some(metadata) = snapshots.get_mut(&snapshot_id) {
            metadata.add_participant_hash(node.clone(), state_hash);

            info!(
                "Received snapshot ACK from {} for {}",
                node,
                snapshot_id.to_hex()
            );

            // Check if all participants reported
            if metadata.all_participants_reported() {
                let global_root = metadata.compute_global_root();
                let complete_msg = SnapshotMessage::SnapshotComplete {
                    snapshot_id: snapshot_id.clone(),
                    coordinator: self.own_did.clone(),
                    global_state_root: global_root,
                    participants: metadata.participants.clone(),
                };

                info!(
                    "Snapshot {} complete with root: {}",
                    snapshot_id.to_hex(),
                    hex::encode(global_root)
                );

                return Ok(vec![complete_msg]);
            }
        }

        Ok(vec![])
    }

    /// Handle Marker message
    async fn handle_marker(
        &self,
        snapshot_id: SnapshotId,
        sender: Did,
    ) -> Result<Vec<SnapshotMessage>> {
        let mut snapshots = self.active_snapshots.write().await;

        if let Some(metadata) = snapshots.get_mut(&snapshot_id) {
            metadata.add_marker(sender.clone());

            debug!(
                "Received marker from {} for snapshot {}",
                sender,
                snapshot_id.to_hex()
            );

            // Check if all markers received
            if metadata.all_markers_received(&metadata.participants.clone()) {
                info!("All markers received for snapshot {}", snapshot_id.to_hex());
                metadata.state = SnapshotState::Complete;
            }
        }

        Ok(vec![])
    }

    /// Handle RequestState message
    async fn handle_state_request(
        &self,
        snapshot_id: SnapshotId,
        requester: Did,
    ) -> Result<Vec<SnapshotMessage>> {
        // Check if snapshot exists
        let snapshots = self.active_snapshots.read().await;
        if !snapshots.contains_key(&snapshot_id) {
            warn!(
                "State requested for unknown snapshot: {}",
                snapshot_id.to_hex()
            );
            return Ok(vec![]);
        }
        drop(snapshots);

        // Capture local state
        let local_state = self.capture_local_state().await?;

        // Split into chunks
        let chunks = self.split_into_chunks(&local_state);
        let total_chunks = chunks.len() as u32;

        let mut messages = Vec::new();
        for (index, chunk) in chunks.into_iter().enumerate() {
            messages.push(SnapshotMessage::StateChunk {
                snapshot_id: snapshot_id.clone(),
                sender: self.own_did.clone(),
                chunk_index: index as u32,
                total_chunks,
                data: chunk,
            });
        }

        info!(
            "Sending {} state chunks to {} for snapshot {}",
            messages.len(),
            requester,
            snapshot_id.to_hex()
        );

        Ok(messages)
    }

    /// Handle StateChunk message
    async fn handle_state_chunk(
        &self,
        snapshot_id: SnapshotId,
        chunk_index: u32,
        total_chunks: u32,
        data: Vec<u8>,
    ) -> Result<Vec<SnapshotMessage>> {
        let mut buffers = self.chunk_buffers.write().await;

        let buffer = buffers
            .entry(snapshot_id.clone())
            .or_insert_with(|| ChunkBuffer {
                sender: self.own_did.clone(),
                received_chunks: HashMap::new(),
                data_size: 0,
            });

        buffer.received_chunks.insert(chunk_index, data.clone());
        buffer.data_size += data.len();

        debug!(
            "Received chunk {}/{} for snapshot {:?}, total size: {} bytes",
            chunk_index + 1,
            total_chunks,
            snapshot_id,
            buffer.data_size
        );

        // Check if we have all chunks
        if buffer.received_chunks.len() == total_chunks as usize {
            // Reassemble in order
            let mut reassembled = Vec::with_capacity(buffer.data_size);
            for i in 0..total_chunks {
                if let Some(chunk) = buffer.received_chunks.get(&i) {
                    reassembled.extend_from_slice(chunk);
                } else {
                    return Err(anyhow!("Missing chunk {i} during reassembly"));
                }
            }

            info!(
                "Reassembled complete state for snapshot {:?}: {} bytes from {} chunks",
                snapshot_id,
                reassembled.len(),
                total_chunks
            );

            // Store in completed snapshots
            let mut completed = self.completed_snapshots.write().await;
            if let Some(snapshot) = completed.get_mut(&snapshot_id) {
                snapshot
                    .participant_states
                    .insert(buffer.sender.clone(), reassembled);
            }

            // Clean up buffer
            buffers.remove(&snapshot_id);
        }

        Ok(vec![])
    }

    /// Handle SnapshotComplete message
    async fn handle_complete(
        &self,
        snapshot_id: SnapshotId,
        coordinator: Did,
        global_state_root: [u8; 32],
        participants: Vec<Did>,
    ) -> Result<Vec<SnapshotMessage>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let completed = CompletedSnapshot {
            snapshot_id: snapshot_id.clone(),
            coordinator,
            completed_at: now,
            global_state_root,
            participants,
            participant_states: HashMap::new(),
        };

        self.completed_snapshots
            .write()
            .await
            .insert(snapshot_id.clone(), completed);

        // Remove from active
        self.active_snapshots.write().await.remove(&snapshot_id);

        info!(
            "Snapshot {} marked complete with root: {}",
            snapshot_id.to_hex(),
            hex::encode(global_state_root)
        );

        Ok(vec![])
    }

    /// Handle VerifySnapshot message
    async fn handle_verify_request(
        &self,
        snapshot_id: SnapshotId,
        _requester: Did,
        expected_root: [u8; 32],
    ) -> Result<Vec<SnapshotMessage>> {
        let completed = self.completed_snapshots.read().await;

        if let Some(snapshot) = completed.get(&snapshot_id) {
            let valid = snapshot.global_state_root == expected_root;
            let result = SnapshotMessage::VerificationResult {
                snapshot_id,
                verifier: self.own_did.clone(),
                valid,
                computed_root: snapshot.global_state_root,
            };

            return Ok(vec![result]);
        }

        warn!(
            "Verification requested for unknown snapshot: {}",
            snapshot_id.to_hex()
        );
        Ok(vec![])
    }

    /// Handle VerificationResult message
    async fn handle_verification_result(
        &self,
        snapshot_id: SnapshotId,
        verifier: Did,
        valid: bool,
        computed_root: [u8; 32],
    ) -> Result<Vec<SnapshotMessage>> {
        if valid {
            info!("Snapshot {} verified by {}", snapshot_id.to_hex(), verifier);
        } else {
            warn!(
                "Snapshot {} verification FAILED by {} (expected root mismatch: {})",
                snapshot_id.to_hex(),
                verifier,
                hex::encode(computed_root)
            );
        }

        Ok(vec![])
    }

    /// Capture local state
    async fn capture_local_state(&self) -> Result<Vec<u8>> {
        // Create a snapshot of local state
        let snapshot = StateSnapshot::new();

        // Serialize to bytes
        icn_encoding::encode_bincode_legacy(&snapshot).context("Failed to serialize state snapshot")
    }

    /// Hash state bytes
    fn hash_state(&self, state: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(state);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Split state into chunks
    fn split_into_chunks(&self, state: &[u8]) -> Vec<Vec<u8>> {
        state
            .chunks(self.config.chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    /// Get active snapshot count
    pub async fn active_count(&self) -> usize {
        self.active_snapshots.read().await.len()
    }

    /// Get completed snapshot count
    pub async fn completed_count(&self) -> usize {
        self.completed_snapshots.read().await.len()
    }

    /// Check if snapshot is complete
    pub async fn is_complete(&self, snapshot_id: &SnapshotId) -> bool {
        self.completed_snapshots
            .read()
            .await
            .contains_key(snapshot_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::IdentityBundle;

    #[tokio::test]
    async fn test_initiate_snapshot() {
        let own = IdentityBundle::generate().unwrap().did().clone();
        let peer1 = IdentityBundle::generate().unwrap().did().clone();
        let peer2 = IdentityBundle::generate().unwrap().did().clone();
        let peer3 = IdentityBundle::generate().unwrap().did().clone();

        let coordinator = SnapshotCoordinator::new(own, SnapshotConfig::default());

        let participants = vec![peer1, peer2, peer3];
        let result = coordinator.initiate_snapshot(participants.clone()).await;

        assert!(result.is_ok());
        let (snapshot_id, msg) = result.unwrap();

        match msg {
            SnapshotMessage::InitiateSnapshot {
                snapshot_id: msg_id,
                initiator,
                participants: msg_participants,
                ..
            } => {
                assert_eq!(msg_id, snapshot_id);
                assert_eq!(initiator, coordinator.own_did);
                assert_eq!(msg_participants, participants);
            }
            _ => panic!("Expected InitiateSnapshot message"),
        }

        assert_eq!(coordinator.active_count().await, 1);
    }

    #[tokio::test]
    async fn test_insufficient_participants() {
        let own = IdentityBundle::generate().unwrap().did().clone();
        let peer1 = IdentityBundle::generate().unwrap().did().clone();

        let coordinator = SnapshotCoordinator::new(own, SnapshotConfig::default());

        let participants = vec![peer1]; // Only 1 participant, need 3
        let result = coordinator.initiate_snapshot(participants).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Insufficient participants"));
    }

    #[tokio::test]
    async fn test_handle_initiate_as_participant() {
        let own = IdentityBundle::generate().unwrap().did().clone();
        let initiator = IdentityBundle::generate().unwrap().did().clone();
        let peer2 = IdentityBundle::generate().unwrap().did().clone();

        let coordinator = SnapshotCoordinator::new(own.clone(), SnapshotConfig::default());

        let snapshot_id = SnapshotId::generate();
        let participants = vec![own.clone(), peer2];

        let result = coordinator
            .handle_initiate(snapshot_id.clone(), initiator, 12345, participants)
            .await;

        assert!(result.is_ok());
        let messages = result.unwrap();

        // Should get ACK + Marker
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0], SnapshotMessage::SnapshotAck { .. }));
        assert!(matches!(messages[1], SnapshotMessage::Marker { .. }));

        assert_eq!(coordinator.active_count().await, 1);
    }

    #[tokio::test]
    async fn test_complete_snapshot_when_all_acks_received() {
        let own = IdentityBundle::generate().unwrap().did().clone();
        let peer1 = IdentityBundle::generate().unwrap().did().clone();
        let peer2 = IdentityBundle::generate().unwrap().did().clone();

        let coordinator = SnapshotCoordinator::new(own.clone(), SnapshotConfig::default());

        // Initiate snapshot
        let participants = vec![own.clone(), peer1.clone(), peer2.clone()];
        let (snapshot_id, _) = coordinator
            .initiate_snapshot(participants.clone())
            .await
            .unwrap();

        // Receive ACKs from all participants
        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];
        let hash3 = [3u8; 32];

        coordinator
            .handle_ack(snapshot_id.clone(), own.clone(), hash1, 1000)
            .await
            .unwrap();
        coordinator
            .handle_ack(snapshot_id.clone(), peer1.clone(), hash2, 1000)
            .await
            .unwrap();

        let result = coordinator
            .handle_ack(snapshot_id.clone(), peer2.clone(), hash3, 1000)
            .await
            .unwrap();

        // Should get SnapshotComplete message
        assert_eq!(result.len(), 1);
        assert!(matches!(
            result[0],
            SnapshotMessage::SnapshotComplete { .. }
        ));
    }
}
