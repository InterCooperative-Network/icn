//! Distributed snapshot protocol (Chandy-Lamport)
//!
//! This module implements a distributed snapshot protocol for capturing
//! consistent global state across the ICN network. Based on the Chandy-Lamport
//! algorithm adapted for gossip-based systems.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use icn_identity::Did;

/// Unique identifier for a snapshot operation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub [u8; 32]);

impl SnapshotId {
    /// Generate a new random snapshot ID
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).expect("Failed to generate random snapshot ID");
        Self(bytes)
    }

    /// Create from existing bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Snapshot protocol messages exchanged via gossip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotMessage {
    /// Initiator broadcasts snapshot request
    InitiateSnapshot {
        snapshot_id: SnapshotId,
        initiator: Did,
        timestamp: u64,
        /// Peers that should participate
        participants: Vec<Did>,
    },

    /// Node acknowledges participation and records local state
    SnapshotAck {
        snapshot_id: SnapshotId,
        node: Did,
        /// Hash of local state snapshot
        state_hash: [u8; 32],
        /// Size in bytes
        state_size: u64,
    },

    /// Marker message to delineate pre/post-snapshot messages
    /// Sent on all outgoing channels after recording local state
    Marker {
        snapshot_id: SnapshotId,
        sender: Did,
    },

    /// Request state from a peer
    RequestState {
        snapshot_id: SnapshotId,
        requester: Did,
    },

    /// Respond with state chunk
    StateChunk {
        snapshot_id: SnapshotId,
        sender: Did,
        chunk_index: u32,
        total_chunks: u32,
        data: Vec<u8>,
    },

    /// Snapshot complete notification
    SnapshotComplete {
        snapshot_id: SnapshotId,
        coordinator: Did,
        /// Merkle root of combined states
        global_state_root: [u8; 32],
        /// Participants who contributed
        participants: Vec<Did>,
    },

    /// Snapshot verification request
    VerifySnapshot {
        snapshot_id: SnapshotId,
        requester: Did,
        expected_root: [u8; 32],
    },

    /// Snapshot verification response
    VerificationResult {
        snapshot_id: SnapshotId,
        verifier: Did,
        valid: bool,
        computed_root: [u8; 32],
    },
}

/// State of a node during snapshot protocol
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotState {
    /// Not participating in any snapshot
    Idle,
    /// Recording local state
    Recording,
    /// Waiting for markers from all peers
    WaitingForMarkers { received_from: HashSet<Did> },
    /// Uploading state to coordinator
    Uploading { chunks_sent: u32, total_chunks: u32 },
    /// Snapshot complete
    Complete,
}

/// Snapshot coordination metadata
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub snapshot_id: SnapshotId,
    pub initiator: Did,
    pub started_at: u64,
    pub participants: Vec<Did>,
    pub state: SnapshotState,
    /// Channel states: messages received between local snapshot and marker
    pub channel_states: HashMap<Did, Vec<Vec<u8>>>,
    /// State hashes from participants
    pub participant_hashes: HashMap<Did, [u8; 32]>,
}

impl SnapshotMetadata {
    /// Create new snapshot metadata
    pub fn new(snapshot_id: SnapshotId, initiator: Did, participants: Vec<Did>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            snapshot_id,
            initiator,
            started_at: now,
            participants,
            state: SnapshotState::Idle,
            channel_states: HashMap::new(),
            participant_hashes: HashMap::new(),
        }
    }

    /// Check if all markers received
    pub fn all_markers_received(&self, expected_peers: &[Did]) -> bool {
        if let SnapshotState::WaitingForMarkers { received_from } = &self.state {
            expected_peers.iter().all(|peer| received_from.contains(peer))
        } else {
            false
        }
    }

    /// Add marker from peer
    pub fn add_marker(&mut self, peer: Did) {
        if let SnapshotState::WaitingForMarkers { received_from } = &mut self.state {
            received_from.insert(peer);
        }
    }

    /// Record channel message (before marker received)
    pub fn record_channel_message(&mut self, sender: &Did, message: Vec<u8>) {
        self.channel_states
            .entry(sender.clone())
            .or_default()
            .push(message);
    }

    /// Add participant state hash
    pub fn add_participant_hash(&mut self, peer: Did, hash: [u8; 32]) {
        self.participant_hashes.insert(peer, hash);
    }

    /// Check if all participants reported
    pub fn all_participants_reported(&self) -> bool {
        self.participant_hashes.len() == self.participants.len()
    }

    /// Compute global state root from participant hashes
    pub fn compute_global_root(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        
        let mut hasher = Sha256::new();
        
        // Sort participants for deterministic ordering
        let mut participants: Vec<_> = self.participant_hashes.iter().collect();
        participants.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()));
        
        for (did, hash) in participants {
            hasher.update(did.to_string().as_bytes());
            hasher.update(hash);
        }
        
        let result = hasher.finalize();
        let mut root = [0u8; 32];
        root.copy_from_slice(&result);
        root
    }
}

/// Configuration for snapshot coordination
#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Minimum trust score to participate in snapshots
    pub min_trust_for_snapshot: f32,
    /// Maximum snapshot size per node (bytes)
    pub max_snapshot_size: u64,
    /// Chunk size for state transfer (bytes)
    pub chunk_size: usize,
    /// Timeout for snapshot completion (seconds)
    pub snapshot_timeout: u64,
    /// Minimum participants for valid snapshot
    pub min_participants: usize,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            min_trust_for_snapshot: 0.5,
            max_snapshot_size: 100 * 1024 * 1024, // 100 MB
            chunk_size: 1024 * 1024,             // 1 MB chunks
            snapshot_timeout: 300,               // 5 minutes
            min_participants: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_id_generation() {
        let id1 = SnapshotId::generate();
        let id2 = SnapshotId::generate();
        assert_ne!(id1, id2, "Snapshot IDs should be unique");
        assert_eq!(id1.as_bytes().len(), 32);
    }

    #[test]
    fn test_snapshot_id_hex() {
        let bytes = [0x42u8; 32];
        let id = SnapshotId::from_bytes(bytes);
        let hex = id.to_hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_snapshot_metadata_markers() {
        use icn_identity::IdentityBundle;

        let snapshot_id = SnapshotId::generate();
        let initiator = IdentityBundle::generate().unwrap().did().clone();
        let peer1 = IdentityBundle::generate().unwrap().did().clone();
        let peer2 = IdentityBundle::generate().unwrap().did().clone();

        let mut metadata = SnapshotMetadata::new(
            snapshot_id,
            initiator,
            vec![peer1.clone(), peer2.clone()],
        );

        metadata.state = SnapshotState::WaitingForMarkers {
            received_from: HashSet::new(),
        };

        assert!(!metadata.all_markers_received(&[peer1.clone(), peer2.clone()]));

        metadata.add_marker(peer1.clone());
        assert!(!metadata.all_markers_received(&[peer1.clone(), peer2.clone()]));

        metadata.add_marker(peer2.clone());
        assert!(metadata.all_markers_received(&[peer1, peer2]));
    }

    #[test]
    fn test_global_root_deterministic() {
        use icn_identity::IdentityBundle;

        let snapshot_id = SnapshotId::generate();
        let initiator = IdentityBundle::generate().unwrap().did().clone();
        let peer1 = IdentityBundle::generate().unwrap().did().clone();
        let peer2 = IdentityBundle::generate().unwrap().did().clone();

        let mut metadata = SnapshotMetadata::new(
            snapshot_id,
            initiator,
            vec![peer1.clone(), peer2.clone()],
        );

        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];

        metadata.add_participant_hash(peer1.clone(), hash1);
        metadata.add_participant_hash(peer2.clone(), hash2);

        let root1 = metadata.compute_global_root();

        // Recompute with same data
        let metadata2 = metadata.clone();
        let root2 = metadata2.compute_global_root();

        assert_eq!(root1, root2, "Global root should be deterministic");
    }

    #[test]
    fn test_channel_state_recording() {
        use icn_identity::IdentityBundle;

        let snapshot_id = SnapshotId::generate();
        let initiator = IdentityBundle::generate().unwrap().did().clone();
        let sender = IdentityBundle::generate().unwrap().did().clone();

        let mut metadata = SnapshotMetadata::new(snapshot_id, initiator, vec![]);

        metadata.record_channel_message(&sender, vec![1, 2, 3]);
        metadata.record_channel_message(&sender, vec![4, 5, 6]);

        assert_eq!(metadata.channel_states.get(&sender).unwrap().len(), 2);
        assert_eq!(metadata.channel_states.get(&sender).unwrap()[0], vec![1, 2, 3]);
        assert_eq!(metadata.channel_states.get(&sender).unwrap()[1], vec![4, 5, 6]);
    }
}
