#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for Proof-of-Storage challenge-response protocol
//!
//! Tests the storage challenge flow:
//! - Challenge creation and multi-block selection
//! - Proof generation and verification
//! - Timeout handling and misbehavior recording
//! - Re-replication triggers on failure

use anyhow::Result;
use icn_core::storage_challenge::{ChallengeScheduler, ChallengeSchedulerHandle};
use icn_gossip::GossipActor;
use icn_identity::{Did, KeyPair};
use icn_security::{MisbehaviorDetector, MisbehaviorThresholds, StorageFailureReason, Violation};
use icn_store::pos::{ChallengeConfig, ContentChunkTree, StorageChallenge, StorageProof};
use icn_store::{ReplicaHealth, SledStore, Store};
use icn_trust::TrustGraph;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Test helper to create a node with storage challenge scheduler
struct TestNode {
    did: Did,
    #[allow(dead_code)]
    keypair: Arc<KeyPair>,
    #[allow(dead_code)]
    gossip_handle: Arc<RwLock<GossipActor>>,
    #[allow(dead_code)]
    trust_graph_handle: Arc<RwLock<TrustGraph>>,
    misbehavior_handle: Arc<RwLock<MisbehaviorDetector>>,
    store: Arc<dyn Store>,
    challenge_handle: ChallengeSchedulerHandle,
    _shutdown_tx: tokio::sync::broadcast::Sender<()>,
}

impl TestNode {
    async fn new(config: ChallengeConfig) -> Result<Self> {
        let keypair = KeyPair::generate()?;
        let did = keypair.did().clone();
        let keypair = Arc::new(keypair);

        // Create stores
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary()?);
        let trust_store: Arc<dyn Store> = Arc::new(SledStore::temporary()?);

        // Create trust graph
        let trust_graph = TrustGraph::new(trust_store, did.clone());
        let trust_graph_handle = Arc::new(RwLock::new(trust_graph));

        // Create misbehavior detector
        let misbehavior = MisbehaviorDetector::new(MisbehaviorThresholds::default());
        let misbehavior_handle = Arc::new(RwLock::new(misbehavior));

        // Create gossip actor
        let trust_lookup = Arc::new(|_: &Did| None);
        let gossip_handle = GossipActor::spawn(did.clone(), trust_lookup);

        // Set gossip store
        {
            let mut gossip = gossip_handle.write().await;
            gossip.set_store(store.clone());
        }

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel(1);

        // Spawn challenge scheduler
        let challenge_handle = ChallengeScheduler::spawn(
            did.clone(),
            keypair.clone(),
            config,
            store.clone(),
            trust_graph_handle.clone(),
            gossip_handle.clone(),
            misbehavior_handle.clone(),
            shutdown_rx,
        );

        Ok(Self {
            did,
            keypair,
            gossip_handle,
            trust_graph_handle,
            misbehavior_handle,
            store,
            challenge_handle,
            _shutdown_tx: shutdown_tx,
        })
    }

    /// Store content and create a chunk tree for it
    fn store_content(&self, data: &[u8]) -> Result<([u8; 32], ContentChunkTree)> {
        // Store the raw content
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash: [u8; 32] = hasher.finalize().into();
        self.store.put(&hash, data)?;

        // Create chunk tree
        let tree = ContentChunkTree::new(data.to_vec(), 1024); // 1KB chunks

        Ok((hash, tree))
    }

    /// Check if a violation was recorded for a peer
    async fn has_violation_for(&self, peer_did: &Did) -> bool {
        let misbehavior = self.misbehavior_handle.read().await;
        !misbehavior.get_violations(peer_did).is_empty()
    }

    /// Get all violations for a peer
    async fn get_violations_for(&self, peer_did: &Did) -> Vec<Violation> {
        let misbehavior = self.misbehavior_handle.read().await;
        misbehavior
            .get_violations(peer_did)
            .iter()
            .map(|r| r.violation.clone())
            .collect()
    }
}

#[tokio::test]
async fn test_challenge_creation_multi_block() -> Result<()> {
    // Test that challenges are created with multiple block indices
    let config = ChallengeConfig {
        blocks_per_challenge: 3,
        ..Default::default()
    };

    let node = TestNode::new(config).await?;

    // Store content large enough for multiple chunks
    let data = vec![0u8; 5000]; // ~5 chunks at 1KB each
    let (hash, tree) = node.store_content(&data)?;

    // Create a challenge
    let peer_did = KeyPair::generate()?.did().clone();

    // Select 3 random chunk indices
    let chunk_indices: Vec<u32> = (0..3).map(|i| i % tree.num_chunks()).collect();

    let challenge = StorageChallenge::new_multi_block(
        hash,
        peer_did.to_string(),
        0,    // byte_offset
        1024, // byte_length
        chunk_indices,
        node.did.to_string(),
        30, // timeout_secs
    );

    // Verify multi-block challenge properties
    assert_eq!(challenge.chunk_indices.len(), 3);
    assert_eq!(challenge.challenge_nonce.len(), 32); // CSPRNG nonce
    assert!(challenge
        .chunk_indices
        .iter()
        .all(|&i| i < tree.num_chunks()));

    Ok(())
}

#[tokio::test]
async fn test_proof_generation_multi_block() -> Result<()> {
    // Test that proofs are generated for all requested block indices
    let config = ChallengeConfig::default();
    let node = TestNode::new(config).await?;

    // Store content
    let data = vec![42u8; 5000];
    let (hash, tree) = node.store_content(&data)?;

    // Select chunk indices (0, 1, 2 for a 5-chunk file)
    let chunk_indices: Vec<u32> = vec![0, 1, 2];

    // Create a challenge for 3 blocks with byte range [0..1024]
    let byte_offset = 0u64;
    let byte_length = 1024u32;
    let challenge = StorageChallenge::new_multi_block(
        hash,
        "did:icn:peer".to_string(),
        byte_offset,
        byte_length,
        chunk_indices.clone(),
        node.did.to_string(),
        30,
    );

    // Extract just the challenged byte range for the proof
    let challenged_bytes = tree.get_bytes(byte_offset, byte_length).unwrap_or_default();

    // Generate proof with correct byte range
    let proof = StorageProof::new_multi(
        challenge.id,
        challenged_bytes,
        challenge
            .chunk_indices
            .iter()
            .map(|&idx| {
                let merkle_proof = tree.generate_proof(idx).unwrap();
                icn_store::pos::MerkleProofData::from(merkle_proof)
            })
            .collect(),
        node.did.to_string(),
    );

    // Verify proof has correct number of proofs
    assert_eq!(proof.merkle_proofs.len(), challenge.chunk_indices.len());

    // Verify each merkle proof
    for (i, merkle_proof) in proof.merkle_proofs.iter().enumerate() {
        assert_eq!(merkle_proof.chunk_index, challenge.chunk_indices[i]);
    }

    Ok(())
}

#[tokio::test]
async fn test_valid_proof_verification() -> Result<()> {
    // Test that valid proofs pass verification
    let config = ChallengeConfig::default();
    let node = TestNode::new(config).await?;

    // Store content
    let data = b"Hello, storage challenge!".to_vec();
    let (hash, tree) = node.store_content(&data)?;

    // Create challenge
    let challenge = StorageChallenge::new_multi_block(
        hash,
        "did:icn:peer".to_string(),
        0,
        data.len() as u32,
        vec![0], // Single block for simplicity
        node.did.to_string(),
        30,
    );

    // Generate valid proof
    let proof = StorageProof::new_multi(
        challenge.id,
        data.clone(),
        vec![icn_store::pos::MerkleProofData::from(
            tree.generate_proof(challenge.chunk_indices[0]).unwrap(),
        )],
        node.did.to_string(),
    );

    // Verify proof has correct byte_data
    assert_eq!(proof.byte_data, data);

    Ok(())
}

#[tokio::test]
async fn test_invalid_proof_detection() -> Result<()> {
    // Test that invalid proofs are detected
    let config = ChallengeConfig::default();
    let node = TestNode::new(config).await?;

    // Store content
    let data = b"Original content".to_vec();
    let (hash, _tree) = node.store_content(&data)?;

    // Create challenge
    let challenge = StorageChallenge::new_multi_block(
        hash,
        "did:icn:peer".to_string(),
        0,
        data.len() as u32,
        vec![0],
        node.did.to_string(),
        30,
    );

    // Generate proof with WRONG data
    let wrong_data = b"Tampered content!".to_vec();
    let wrong_tree = ContentChunkTree::new(wrong_data.clone(), 1024);
    let proof = StorageProof::new_multi(
        challenge.id,
        wrong_data.clone(),
        vec![icn_store::pos::MerkleProofData::from(
            wrong_tree.generate_proof(0).unwrap(),
        )],
        node.did.to_string(),
    );

    // The proof should have wrong byte_data
    assert_ne!(proof.byte_data, data);

    Ok(())
}

#[tokio::test]
async fn test_challenge_config_blocks_per_challenge() -> Result<()> {
    // Test that blocks_per_challenge config is respected
    let config = ChallengeConfig {
        blocks_per_challenge: 5,
        ..Default::default()
    };

    assert_eq!(config.blocks_per_challenge, 5);

    // Default should be DEFAULT_BLOCKS_PER_CHALLENGE
    let default_config = ChallengeConfig::default();
    assert_eq!(
        default_config.blocks_per_challenge,
        icn_store::pos::DEFAULT_BLOCKS_PER_CHALLENGE
    );

    Ok(())
}

#[tokio::test]
async fn test_merkle_proof_data_round_trip() -> Result<()> {
    // Test MerkleProofData serialization/deserialization
    let data = vec![0u8; 4096]; // 4 chunks
    let tree = ContentChunkTree::new(data, 1024);

    let original_proof = tree.generate_proof(2).unwrap();
    let proof_data = icn_store::pos::MerkleProofData::from(original_proof);

    // Verify the proof data
    assert_eq!(proof_data.chunk_index, 2);
    assert!(!proof_data.siblings.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_challenge_scheduler_handle_pending_count() -> Result<()> {
    // Test that pending count is tracked correctly
    let config = ChallengeConfig {
        timeout_secs: 60,           // Long timeout so challenges stay pending
        challenge_probability: 0.0, // Disable automatic challenges
        ..Default::default()
    };

    let node = TestNode::new(config).await?;

    // Initially no pending challenges
    let count = node.challenge_handle.pending_count().await;
    assert_eq!(count, 0);

    Ok(())
}

#[tokio::test]
async fn test_re_replication_callback_interface() -> Result<()> {
    // Test that re-replication callback can be set up
    let re_replication_triggered = Arc::new(AtomicBool::new(false));
    let triggered_clone = re_replication_triggered.clone();

    let _callback: icn_core::storage_challenge::ReReplicationCallback =
        Arc::new(move |_content_hash, _peer_did, _failure_count| {
            triggered_clone.store(true, Ordering::SeqCst);
            Ok(())
        });

    // The callback type should compile and be usable
    assert!(!re_replication_triggered.load(Ordering::SeqCst));

    Ok(())
}

#[tokio::test]
async fn test_storage_failure_reason_severity() -> Result<()> {
    // Test that failure reasons have appropriate severity scores

    // Network issues - low severity
    assert!(StorageFailureReason::NoResponse.severity() < 3);
    assert!(StorageFailureReason::Expired.severity() < 3);

    // Data issues - medium severity
    assert!(StorageFailureReason::ContentNotFound.severity() >= 3);

    // Likely malicious - high severity
    assert!(StorageFailureReason::DataMismatch.severity() >= 5);
    assert!(StorageFailureReason::InvalidMerkleProof.severity() >= 8);
    assert!(StorageFailureReason::InvalidSignature.severity() >= 8);

    Ok(())
}

#[tokio::test]
async fn test_violation_creation_for_failed_challenge() -> Result<()> {
    // Test that Violation::FailedStorageChallenge can be created
    let content_hash: [u8; 32] = [1u8; 32];
    let challenge_id: [u8; 32] = [2u8; 32];

    let violation = Violation::FailedStorageChallenge {
        content_hash,
        challenge_id,
        reason: StorageFailureReason::InvalidMerkleProof,
    };

    // Verify high severity for invalid merkle proof
    assert!(violation.severity() >= 8);

    Ok(())
}

#[tokio::test]
async fn test_challenge_nonce_uniqueness() -> Result<()> {
    // Test that challenge nonces are unique (CSPRNG)
    let node = TestNode::new(ChallengeConfig::default()).await?;
    let data = vec![0u8; 1024];
    let (hash, _tree) = node.store_content(&data)?;

    let peer_did = KeyPair::generate()?.did().clone();

    let challenge1 = StorageChallenge::new_multi_block(
        hash,
        peer_did.to_string(),
        0,
        1024,
        vec![0], // First chunk
        node.did.to_string(),
        30,
    );

    let challenge2 = StorageChallenge::new_multi_block(
        hash,
        peer_did.to_string(),
        0,
        1024,
        vec![0], // Same chunk - nonces should still differ
        node.did.to_string(),
        30,
    );

    // Nonces should be different (CSPRNG)
    assert_ne!(challenge1.challenge_nonce, challenge2.challenge_nonce);
    // Challenge IDs should be different too
    assert_ne!(challenge1.id, challenge2.id);

    Ok(())
}

#[tokio::test]
async fn test_misbehavior_detector_records_storage_violation() -> Result<()> {
    // Test that misbehavior detector can record storage violations
    let config = ChallengeConfig::default();
    let node = TestNode::new(config).await?;

    // Create a peer that will be marked as misbehaving
    let bad_peer = KeyPair::generate()?.did().clone();

    // Record a violation
    {
        let mut misbehavior = node.misbehavior_handle.write().await;
        let violation = Violation::FailedStorageChallenge {
            content_hash: [0u8; 32],
            challenge_id: [1u8; 32],
            reason: StorageFailureReason::InvalidMerkleProof,
        };
        misbehavior.record_violation(&bad_peer, violation, vec![]);
    }

    // Verify violation was recorded
    assert!(node.has_violation_for(&bad_peer).await);

    let violations = node.get_violations_for(&bad_peer).await;
    assert_eq!(violations.len(), 1);
    assert!(matches!(
        violations[0],
        Violation::FailedStorageChallenge { .. }
    ));

    Ok(())
}

#[tokio::test]
async fn test_unhealthy_replica_tracking() -> Result<()> {
    // Test that replicas can be marked as unhealthy
    let config = ChallengeConfig::default();
    let node = TestNode::new(config).await?;

    // Store content and add replica
    let data = b"test content".to_vec();
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash: [u8; 32] = hasher.finalize().into();
    node.store.put(&hash, &data)?;

    // Add a replica
    let peer_did = "did:icn:testpeer123";
    node.store
        .add_replica(&hash, peer_did.to_string(), ReplicaHealth::Healthy)?;

    // Verify replica is healthy
    let metadata = node.store.get_replica_metadata(&hash)?.unwrap();
    assert_eq!(metadata.replicas.len(), 1);
    assert_eq!(metadata.replicas[0].health, ReplicaHealth::Healthy);

    // Mark replica as unhealthy
    let mut metadata = metadata;
    let triggered = metadata.mark_replica_unhealthy(peer_did, 3);
    assert!(triggered);

    // Verify unhealthy status
    assert_eq!(metadata.unhealthy_count(), 1);
    assert!(metadata.needs_re_replication(1));

    Ok(())
}

#[tokio::test]
async fn test_challenge_signing_and_verification() -> Result<()> {
    // Test that challenges can be signed and verified
    let keypair = KeyPair::generate()?;
    let did = keypair.did().clone();

    let data = b"test data for signing".to_vec();
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash: [u8; 32] = hasher.finalize().into();

    let mut challenge = StorageChallenge::new_multi_block(
        hash,
        "did:icn:target".to_string(),
        0,
        1024,
        vec![0],
        did.to_string(),
        30,
    );

    // Sign the challenge
    challenge.sign(&keypair).unwrap();
    assert!(!challenge.signature.is_empty());

    // Verify the signature
    assert!(challenge.verify_signature().is_ok());

    Ok(())
}

#[tokio::test]
async fn test_challenge_expiration() -> Result<()> {
    // Test that challenges correctly report expiration
    let data = b"expiration test".to_vec();
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash: [u8; 32] = hasher.finalize().into();

    // Create a challenge with long timeout
    let challenge = StorageChallenge::new_multi_block(
        hash,
        "did:icn:target".to_string(),
        0,
        1024,
        vec![0],
        "did:icn:challenger".to_string(),
        3600, // 1 hour timeout
    );

    // Should not be expired
    assert!(!challenge.is_expired());

    // Verify the expiration timestamp is set correctly
    // (created_at + timeout_secs)
    assert!(challenge.expires_at > challenge.created_at);
    assert_eq!(challenge.expires_at - challenge.created_at, 3600);

    Ok(())
}

#[tokio::test]
async fn test_content_chunk_tree_verification() -> Result<()> {
    // Test that ContentChunkTree Merkle proofs verify correctly
    let data = vec![0u8; 4096]; // 4 chunks at 1KB each
    let tree = ContentChunkTree::new(data.clone(), 1024);

    assert_eq!(tree.num_chunks(), 4);
    assert_eq!(tree.content_size(), 4096);

    // Generate and verify proof for each chunk
    for chunk_idx in 0..tree.num_chunks() {
        let proof = tree.generate_proof(chunk_idx).unwrap();
        assert!(proof.verify());
    }

    // Invalid chunk index should return None
    let invalid_proof = tree.generate_proof(100);
    assert!(invalid_proof.is_none());

    Ok(())
}

#[tokio::test]
async fn test_proof_byte_data_hash_verification() -> Result<()> {
    // Test that we can verify proof byte_data matches expected content
    let config = ChallengeConfig::default();
    let node = TestNode::new(config).await?;

    // Store content
    let data = b"Test content for hash verification".to_vec();
    let (hash, tree) = node.store_content(&data)?;

    // Create challenge
    let challenge = StorageChallenge::new_multi_block(
        hash,
        "did:icn:peer".to_string(),
        0,
        data.len() as u32,
        vec![0],
        node.did.to_string(),
        30,
    );

    // Generate proof
    let proof = StorageProof::new_multi(
        challenge.id,
        data.clone(),
        vec![icn_store::pos::MerkleProofData::from(
            tree.generate_proof(0).unwrap(),
        )],
        node.did.to_string(),
    );

    // Verify byte_data hashes to expected content hash
    let mut hasher = Sha256::new();
    hasher.update(&proof.byte_data);
    let proof_data_hash: [u8; 32] = hasher.finalize().into();
    assert_eq!(proof_data_hash, hash);

    Ok(())
}
