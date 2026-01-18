//! Proof-of-Storage Challenge System
//!
//! Cryptographic verification that replica holders actually store the data they claim.
//! Uses a combination of random byte range challenges and Merkle proofs.

use crate::ContentHash;
use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier};
use icn_identity::{Did, KeyPair};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Default chunk size for Merkle tree (4KB)
pub const DEFAULT_CHUNK_SIZE: u32 = 4096;

/// Maximum byte sample size for challenges (4KB)
pub const MAX_BYTE_SAMPLE_SIZE: u32 = 4096;

/// Default challenge timeout in seconds
pub const DEFAULT_CHALLENGE_TIMEOUT_SECS: u64 = 30;

// ============================================================================
// Challenge Types
// ============================================================================

/// Protocol version for storage challenges
pub const CHALLENGE_PROTOCOL_VERSION: u8 = 2;

/// Minimum supported protocol version
pub const MIN_SUPPORTED_VERSION: u8 = 1;

/// Maximum supported protocol version
pub const MAX_SUPPORTED_VERSION: u8 = 2;

/// Storage challenge combining byte range + Merkle proof verification
///
/// # Protocol Versions
/// - v1: Single chunk index, u64 nonce (legacy)
/// - v2: Multiple chunk indices, 32-byte CSPRNG nonce
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageChallenge {
    /// Protocol version (default: 2)
    #[serde(default = "default_challenge_version")]
    pub version: u8,

    /// Unique challenge ID (SHA-256 of challenge parameters)
    pub id: [u8; 32],

    /// Content hash being challenged
    pub content_hash: ContentHash,

    /// DID of the replica holder being challenged
    pub target_peer: String,

    /// Random byte range start offset
    pub byte_offset: u64,

    /// Number of bytes to return (max 4KB)
    pub byte_length: u32,

    /// Merkle chunk indices to prove (v2: multiple blocks)
    /// For v1 compatibility, a single-element vec is used
    pub chunk_indices: Vec<u32>,

    /// CSPRNG nonce for replay protection (v2: 32 bytes)
    /// For v1 compatibility, first 8 bytes contain the u64 nonce
    pub challenge_nonce: [u8; 32],

    /// DID of the challenger
    pub challenger: String,

    /// Challenge creation timestamp (Unix seconds)
    pub created_at: u64,

    /// Challenge expiration timestamp (Unix seconds)
    pub expires_at: u64,

    /// Ed25519 signature by challenger over challenge parameters
    pub signature: Vec<u8>,
}

fn default_challenge_version() -> u8 {
    CHALLENGE_PROTOCOL_VERSION
}

impl StorageChallenge {
    /// Create a new storage challenge for a single chunk (v2 protocol, unsigned)
    ///
    /// For multi-block challenges, use `new_multi_block` instead.
    pub fn new(
        content_hash: ContentHash,
        target_peer: String,
        byte_offset: u64,
        byte_length: u32,
        chunk_index: u32,
        challenger: String,
        timeout_secs: u64,
    ) -> Self {
        Self::new_multi_block(
            content_hash,
            target_peer,
            byte_offset,
            byte_length,
            vec![chunk_index],
            challenger,
            timeout_secs,
        )
    }

    /// Create a new multi-block storage challenge (v2 protocol, unsigned)
    ///
    /// Requests proofs for multiple chunk indices in a single challenge,
    /// making it harder for malicious nodes to pass challenges while storing
    /// only portions of the data.
    pub fn new_multi_block(
        content_hash: ContentHash,
        target_peer: String,
        byte_offset: u64,
        byte_length: u32,
        chunk_indices: Vec<u32>,
        challenger: String,
        timeout_secs: u64,
    ) -> Self {
        // Get current Unix timestamp. If system clock is before Unix epoch, use 0.
        // This is extremely rare but could occur with misconfigured system clocks.
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        };

        // Generate 32-byte CSPRNG nonce for unpredictability
        let challenge_nonce: [u8; 32] = rand::random();

        let mut challenge = Self {
            version: CHALLENGE_PROTOCOL_VERSION,
            id: [0u8; 32],
            content_hash,
            target_peer,
            byte_offset,
            byte_length: byte_length.min(MAX_BYTE_SAMPLE_SIZE),
            chunk_indices,
            challenge_nonce,
            challenger,
            created_at: now,
            expires_at: now + timeout_secs,
            signature: Vec::new(),
        };

        // Compute challenge ID from parameters
        challenge.id = challenge.compute_id();
        challenge
    }

    /// Get the primary chunk index (for single-block compatibility)
    pub fn chunk_index(&self) -> Option<u32> {
        self.chunk_indices.first().copied()
    }

    /// Get legacy u64 nonce from first 8 bytes (for v1 compatibility)
    pub fn nonce(&self) -> u64 {
        // Explicit construction avoids try_into() and is infallible for fixed-size arrays
        let bytes: [u8; 8] = [
            self.challenge_nonce[0],
            self.challenge_nonce[1],
            self.challenge_nonce[2],
            self.challenge_nonce[3],
            self.challenge_nonce[4],
            self.challenge_nonce[5],
            self.challenge_nonce[6],
            self.challenge_nonce[7],
        ];
        u64::from_le_bytes(bytes)
    }

    /// Compute challenge ID from parameters (excluding signature)
    fn compute_id(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update([self.version]);
        hasher.update(self.content_hash);
        hasher.update(self.target_peer.as_bytes());
        hasher.update(self.byte_offset.to_le_bytes());
        hasher.update(self.byte_length.to_le_bytes());
        // Include count and all chunk indices
        hasher.update((self.chunk_indices.len() as u32).to_le_bytes());
        for idx in &self.chunk_indices {
            hasher.update(idx.to_le_bytes());
        }
        hasher.update(self.challenge_nonce);
        hasher.update(self.challenger.as_bytes());
        hasher.update(self.created_at.to_le_bytes());
        hasher.update(self.expires_at.to_le_bytes());
        hasher.finalize().into()
    }

    /// Get the signing payload (used for signature creation/verification)
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.push(self.version);
        payload.extend_from_slice(&self.id);
        payload.extend_from_slice(&self.content_hash);
        payload.extend_from_slice(self.target_peer.as_bytes());
        payload.extend_from_slice(&self.byte_offset.to_le_bytes());
        payload.extend_from_slice(&self.byte_length.to_le_bytes());
        // Include count and all chunk indices
        payload.extend_from_slice(&(self.chunk_indices.len() as u32).to_le_bytes());
        for idx in &self.chunk_indices {
            payload.extend_from_slice(&idx.to_le_bytes());
        }
        payload.extend_from_slice(&self.challenge_nonce[..]);
        payload.extend_from_slice(self.challenger.as_bytes());
        payload.extend_from_slice(&self.created_at.to_le_bytes());
        payload.extend_from_slice(&self.expires_at.to_le_bytes());
        payload
    }

    /// Check if the challenge has expired
    pub fn is_expired(&self) -> bool {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        };
        now > self.expires_at
    }

    /// Validate challenge ID matches computed value
    pub fn validate_id(&self) -> bool {
        self.id == self.compute_id()
    }

    /// Validate that the protocol version is supported
    ///
    /// Returns an error if the version is outside the supported range.
    /// This prevents silent mishandling of future protocol versions.
    pub fn validate_version(&self) -> Result<()> {
        if self.version < MIN_SUPPORTED_VERSION || self.version > MAX_SUPPORTED_VERSION {
            anyhow::bail!(
                "Unsupported challenge protocol version {}: supported range is {}-{}",
                self.version,
                MIN_SUPPORTED_VERSION,
                MAX_SUPPORTED_VERSION
            );
        }
        Ok(())
    }

    /// Sign this challenge with the challenger's keypair
    ///
    /// The keypair's DID must match the `challenger` field.
    pub fn sign(&mut self, keypair: &KeyPair) -> Result<()> {
        // Verify the keypair matches the challenger
        if keypair.did().as_str() != self.challenger {
            anyhow::bail!(
                "Keypair DID {} does not match challenger {}",
                keypair.did(),
                self.challenger
            );
        }

        let payload = self.signing_payload();
        let signature = keypair.sign(&payload);
        self.signature = signature.to_bytes().to_vec();
        Ok(())
    }

    /// Verify the signature on this challenge
    ///
    /// Extracts the verifying key from the challenger's DID and verifies the signature.
    pub fn verify_signature(&self) -> Result<()> {
        if self.signature.is_empty() {
            anyhow::bail!("Challenge has no signature");
        }

        let did = Did::from_str(&self.challenger).context("Invalid challenger DID format")?;
        let verifying_key = did
            .to_verifying_key()
            .context("Failed to extract verifying key from challenger DID")?;

        let signature =
            Signature::from_slice(&self.signature).context("Invalid signature format")?;

        let payload = self.signing_payload();
        verifying_key
            .verify(&payload, &signature)
            .context("Challenge signature verification failed")?;

        Ok(())
    }

    /// Check if this challenge has a valid signature
    pub fn is_signed(&self) -> bool {
        !self.signature.is_empty()
    }
}

// ============================================================================
// Proof Types
// ============================================================================

/// Individual Merkle proof data for a single chunk
///
/// Used within `StorageProof` to provide proofs for multiple chunks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MerkleProofData {
    /// Index of the chunk being proven
    pub chunk_index: u32,

    /// Hash of the chunk
    pub chunk_hash: [u8; 32],

    /// Sibling hashes from leaf to root
    pub siblings: Vec<[u8; 32]>,

    /// Position bits: false = left child, true = right child
    pub path_bits: Vec<bool>,
}

impl MerkleProofData {
    /// Verify this proof against an expected root
    pub fn verify_against_root(&self, expected_root: &[u8; 32]) -> bool {
        if self.siblings.len() != self.path_bits.len() {
            return false;
        }

        let mut current = self.chunk_hash;

        for (sibling, is_right) in self.siblings.iter().zip(self.path_bits.iter()) {
            current = if *is_right {
                ContentChunkTree::hash_pair(sibling, &current)
            } else {
                ContentChunkTree::hash_pair(&current, sibling)
            };
        }

        current == *expected_root
    }
}

impl From<MerkleProof> for MerkleProofData {
    fn from(proof: MerkleProof) -> Self {
        Self {
            chunk_index: proof.chunk_index,
            chunk_hash: proof.chunk_hash,
            siblings: proof.siblings,
            path_bits: proof.path_bits,
        }
    }
}

/// Storage proof response from replica holder
///
/// # Protocol Versions
/// - v1: Single proof (chunk_hash, merkle_siblings, merkle_path_bits)
/// - v2: Multiple proofs via `merkle_proofs` vec
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageProof {
    /// Protocol version (default: 2)
    #[serde(default = "default_challenge_version")]
    pub version: u8,

    /// Challenge ID being answered
    pub challenge_id: [u8; 32],

    /// The requested bytes from byte range challenge
    pub byte_data: Vec<u8>,

    /// Multiple Merkle proofs (v2: one per challenged chunk)
    /// For v1 compatibility, single-element vec works
    pub merkle_proofs: Vec<MerkleProofData>,

    /// DID of the prover (replica holder)
    pub prover: String,

    /// Proof generation timestamp (Unix seconds)
    pub generated_at: u64,

    /// Ed25519 signature by prover over proof parameters
    pub signature: Vec<u8>,
}

impl StorageProof {
    /// Create a new storage proof for a single chunk (v2 protocol, unsigned)
    ///
    /// For multi-chunk proofs, use `new_multi` instead.
    pub fn new(
        challenge_id: [u8; 32],
        byte_data: Vec<u8>,
        chunk_hash: [u8; 32],
        merkle_siblings: Vec<[u8; 32]>,
        merkle_path_bits: Vec<bool>,
        prover: String,
    ) -> Self {
        Self::new_multi(
            challenge_id,
            byte_data,
            vec![MerkleProofData {
                chunk_index: 0, // Default index for single-proof compatibility
                chunk_hash,
                siblings: merkle_siblings,
                path_bits: merkle_path_bits,
            }],
            prover,
        )
    }

    /// Create a new multi-chunk storage proof (v2 protocol, unsigned)
    pub fn new_multi(
        challenge_id: [u8; 32],
        byte_data: Vec<u8>,
        merkle_proofs: Vec<MerkleProofData>,
        prover: String,
    ) -> Self {
        // Get current Unix timestamp. If system clock is before Unix epoch, use 0.
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => 0,
        };

        Self {
            version: CHALLENGE_PROTOCOL_VERSION,
            challenge_id,
            byte_data,
            merkle_proofs,
            prover,
            generated_at: now,
            signature: Vec::new(),
        }
    }

    /// Get the first chunk hash (for single-proof compatibility)
    pub fn chunk_hash(&self) -> Option<[u8; 32]> {
        self.merkle_proofs.first().map(|p| p.chunk_hash)
    }

    /// Get the first merkle siblings (for single-proof compatibility)
    pub fn merkle_siblings(&self) -> Option<&Vec<[u8; 32]>> {
        self.merkle_proofs.first().map(|p| &p.siblings)
    }

    /// Get the first merkle path bits (for single-proof compatibility)
    pub fn merkle_path_bits(&self) -> Option<&Vec<bool>> {
        self.merkle_proofs.first().map(|p| &p.path_bits)
    }

    /// Validate that the protocol version is supported
    ///
    /// Returns an error if the version is outside the supported range.
    /// This prevents silent mishandling of future protocol versions.
    pub fn validate_version(&self) -> Result<()> {
        if self.version < MIN_SUPPORTED_VERSION || self.version > MAX_SUPPORTED_VERSION {
            anyhow::bail!(
                "Unsupported proof protocol version {}: supported range is {}-{}",
                self.version,
                MIN_SUPPORTED_VERSION,
                MAX_SUPPORTED_VERSION
            );
        }
        Ok(())
    }

    /// Get the signing payload (used for signature creation/verification)
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.push(self.version);
        payload.extend_from_slice(&self.challenge_id);
        payload.extend_from_slice(&(self.byte_data.len() as u32).to_le_bytes());
        payload.extend_from_slice(&self.byte_data);
        // Include count and all merkle proofs
        payload.extend_from_slice(&(self.merkle_proofs.len() as u32).to_le_bytes());
        for proof in &self.merkle_proofs {
            payload.extend_from_slice(&proof.chunk_index.to_le_bytes());
            payload.extend_from_slice(&proof.chunk_hash);
            payload.extend_from_slice(&(proof.siblings.len() as u32).to_le_bytes());
            for sibling in &proof.siblings {
                payload.extend_from_slice(sibling);
            }
            for bit in &proof.path_bits {
                payload.push(if *bit { 1 } else { 0 });
            }
        }
        payload.extend_from_slice(self.prover.as_bytes());
        payload.extend_from_slice(&self.generated_at.to_le_bytes());
        payload
    }

    /// Sign this proof with the prover's keypair
    ///
    /// The keypair's DID must match the `prover` field.
    pub fn sign(&mut self, keypair: &KeyPair) -> Result<()> {
        // Verify the keypair matches the prover
        if keypair.did().as_str() != self.prover {
            anyhow::bail!(
                "Keypair DID {} does not match prover {}",
                keypair.did(),
                self.prover
            );
        }

        let payload = self.signing_payload();
        let signature = keypair.sign(&payload);
        self.signature = signature.to_bytes().to_vec();
        Ok(())
    }

    /// Verify the signature on this proof
    ///
    /// Extracts the verifying key from the prover's DID and verifies the signature.
    pub fn verify_signature(&self) -> Result<()> {
        if self.signature.is_empty() {
            anyhow::bail!("Proof has no signature");
        }

        let did = Did::from_str(&self.prover).context("Invalid prover DID format")?;
        let verifying_key = did
            .to_verifying_key()
            .context("Failed to extract verifying key from prover DID")?;

        let signature =
            Signature::from_slice(&self.signature).context("Invalid signature format")?;

        let payload = self.signing_payload();
        verifying_key
            .verify(&payload, &signature)
            .context("Proof signature verification failed")?;

        Ok(())
    }

    /// Check if this proof has a valid signature
    pub fn is_signed(&self) -> bool {
        !self.signature.is_empty()
    }
}

// ============================================================================
// Failure Reasons
// ============================================================================

/// Reason for storage challenge failure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageFailureReason {
    /// No response received within timeout
    NoResponse,

    /// Returned bytes don't match expected hash
    DataMismatch,

    /// Merkle proof doesn't verify against expected root
    InvalidMerkleProof,

    /// Prover reports content not found
    ContentNotFound,

    /// Signature verification failed
    InvalidSignature,

    /// Challenge ID mismatch
    ChallengeMismatch,

    /// Proof expired or challenge was already expired
    Expired,
}

impl StorageFailureReason {
    /// Get severity score for this failure type
    /// Higher score = more severe, likely intentional misbehavior
    pub fn severity(&self) -> u32 {
        match self {
            // Network/timing issues - might be temporary
            StorageFailureReason::NoResponse => 1,
            StorageFailureReason::Expired => 1,

            // Data issues - concerning
            StorageFailureReason::ContentNotFound => 3,

            // Likely intentional misbehavior
            StorageFailureReason::DataMismatch => 5,
            StorageFailureReason::ChallengeMismatch => 5,

            // Almost certainly malicious
            StorageFailureReason::InvalidMerkleProof => 8,
            StorageFailureReason::InvalidSignature => 8,
        }
    }

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            StorageFailureReason::NoResponse => "No response received within timeout",
            StorageFailureReason::DataMismatch => "Returned bytes don't match expected hash",
            StorageFailureReason::InvalidMerkleProof => "Merkle proof verification failed",
            StorageFailureReason::ContentNotFound => "Prover reports content not found",
            StorageFailureReason::InvalidSignature => "Proof signature verification failed",
            StorageFailureReason::ChallengeMismatch => "Proof doesn't match challenge",
            StorageFailureReason::Expired => "Challenge or proof expired",
        }
    }
}

// ============================================================================
// Merkle Tree for Content Chunks
// ============================================================================

/// Merkle tree over content chunks for proof-of-storage
#[derive(Debug, Clone)]
pub struct ContentChunkTree {
    /// Content being chunked
    content: Vec<u8>,

    /// Chunk size in bytes
    chunk_size: u32,

    /// Leaf hashes (one per chunk)
    leaf_hashes: Vec<[u8; 32]>,

    /// All tree nodes (leaves + internal nodes)
    /// Stored in level order: leaves first, then internal nodes bottom-up
    nodes: Vec<[u8; 32]>,

    /// Merkle root hash
    root: [u8; 32],
}

impl ContentChunkTree {
    /// Build a Merkle tree from content
    pub fn new(content: Vec<u8>, chunk_size: u32) -> Self {
        let chunk_size = chunk_size.max(1);
        let chunks: Vec<&[u8]> = content.chunks(chunk_size as usize).collect();
        let num_chunks = chunks.len().max(1);

        // Compute leaf hashes
        let leaf_hashes: Vec<[u8; 32]> = chunks
            .iter()
            .map(|chunk| {
                let mut hasher = Sha256::new();
                hasher.update(chunk);
                hasher.finalize().into()
            })
            .collect();

        // Pad to power of 2 with zero hashes
        let tree_size = num_chunks.next_power_of_two();
        let mut nodes: Vec<[u8; 32]> = leaf_hashes.clone();
        nodes.resize(tree_size, [0u8; 32]);

        // Build internal nodes
        let mut level_start = 0;
        let mut level_size = tree_size;

        while level_size > 1 {
            let next_level_size = level_size / 2;
            for i in 0..next_level_size {
                let left = nodes[level_start + 2 * i];
                let right = nodes[level_start + 2 * i + 1];
                let parent = Self::hash_pair(&left, &right);
                nodes.push(parent);
            }
            level_start += level_size;
            level_size = next_level_size;
        }

        let root = if nodes.is_empty() {
            [0u8; 32]
        } else {
            *nodes.last().unwrap_or(&[0u8; 32])
        };

        Self {
            content,
            chunk_size,
            leaf_hashes,
            nodes,
            root,
        }
    }

    /// Hash two sibling nodes together
    fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(left);
        hasher.update(right);
        hasher.finalize().into()
    }

    /// Get the Merkle root
    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    /// Get number of chunks
    pub fn num_chunks(&self) -> u32 {
        self.leaf_hashes.len() as u32
    }

    /// Get content size
    pub fn content_size(&self) -> u64 {
        self.content.len() as u64
    }

    /// Get chunk size
    pub fn chunk_size(&self) -> u32 {
        self.chunk_size
    }

    /// Get bytes at specified offset and length
    pub fn get_bytes(&self, offset: u64, length: u32) -> Option<Vec<u8>> {
        let start = offset as usize;
        let end = (offset + length as u64) as usize;

        if start >= self.content.len() {
            return None;
        }

        let actual_end = end.min(self.content.len());
        Some(self.content[start..actual_end].to_vec())
    }

    /// Get hash of a specific chunk
    pub fn chunk_hash(&self, chunk_index: u32) -> Option<[u8; 32]> {
        self.leaf_hashes.get(chunk_index as usize).copied()
    }

    /// Generate Merkle proof for a chunk
    pub fn generate_proof(&self, chunk_index: u32) -> Option<MerkleProof> {
        let tree_size = self.leaf_hashes.len().next_power_of_two();
        if chunk_index as usize >= tree_size {
            return None;
        }

        let chunk_hash = self.chunk_hash(chunk_index)?;
        let mut siblings = Vec::new();
        let mut path_bits = Vec::new();

        let mut idx = chunk_index as usize;
        let mut level_start = 0;
        let mut level_size = tree_size;

        while level_size > 1 {
            let sibling_idx = if idx.is_multiple_of(2) {
                idx + 1
            } else {
                idx - 1
            };
            let is_right = !idx.is_multiple_of(2);

            if level_start + sibling_idx < self.nodes.len() {
                siblings.push(self.nodes[level_start + sibling_idx]);
            } else {
                siblings.push([0u8; 32]);
            }
            path_bits.push(is_right);

            idx /= 2;
            level_start += level_size;
            level_size /= 2;
        }

        Some(MerkleProof {
            chunk_index,
            chunk_hash,
            siblings,
            path_bits,
            root: self.root,
        })
    }
}

/// Merkle proof for a single chunk
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MerkleProof {
    /// Index of the chunk being proven
    pub chunk_index: u32,

    /// Hash of the chunk
    pub chunk_hash: [u8; 32],

    /// Sibling hashes from leaf to root
    pub siblings: Vec<[u8; 32]>,

    /// Position bits: false = left child, true = right child
    pub path_bits: Vec<bool>,

    /// Expected Merkle root
    pub root: [u8; 32],
}

impl MerkleProof {
    /// Verify the proof against expected root
    pub fn verify(&self) -> bool {
        self.verify_against_root(&self.root)
    }

    /// Verify the proof against a specific root
    pub fn verify_against_root(&self, expected_root: &[u8; 32]) -> bool {
        if self.siblings.len() != self.path_bits.len() {
            return false;
        }

        let mut current = self.chunk_hash;

        for (sibling, is_right) in self.siblings.iter().zip(self.path_bits.iter()) {
            current = if *is_right {
                // Current node is right child
                ContentChunkTree::hash_pair(sibling, &current)
            } else {
                // Current node is left child
                ContentChunkTree::hash_pair(&current, sibling)
            };
        }

        current == *expected_root
    }
}

// ============================================================================
// Challenge State Tracking
// ============================================================================

/// State of a pending challenge
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChallengeState {
    /// Challenge sent, awaiting response
    Pending { sent_at: u64, attempt: u32 },

    /// Valid proof received
    Verified {
        verified_at: u64,
        proof_hash: [u8; 32],
    },

    /// Challenge failed
    Failed {
        failed_at: u64,
        reason: StorageFailureReason,
    },

    /// Challenge expired (grace period)
    Expired { expired_at: u64 },
}

/// Tracked pending challenge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChallenge {
    /// The challenge itself
    pub challenge: StorageChallenge,

    /// Current state
    pub state: ChallengeState,

    /// Expected Merkle root for verification
    pub expected_merkle_root: [u8; 32],
}

// ============================================================================
// Configuration
// ============================================================================

/// Default number of blocks per challenge
pub const DEFAULT_BLOCKS_PER_CHALLENGE: u32 = 3;

/// Configuration for the challenge scheduler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeConfig {
    /// Base interval between challenge rounds
    pub base_interval_secs: u64,

    /// Jitter range in seconds (0 to jitter added randomly)
    pub jitter_secs: u64,

    /// Challenge timeout in seconds
    pub timeout_secs: u64,

    /// Grace period: number of failures before escalation
    pub grace_attempts: u32,

    /// Probability of challenging a replica per round (0.0 to 1.0)
    pub challenge_probability: f64,

    /// Max concurrent pending challenges per node
    pub max_pending_per_node: usize,

    /// Max total pending challenges (prevents unbounded memory growth)
    pub max_pending_total: usize,

    /// Chunk size for Merkle tree
    pub chunk_size: u32,

    /// Byte sample size for challenges
    pub byte_sample_size: u32,

    /// Number of random chunk indices to challenge per request (v2 protocol)
    /// Higher values increase security but also increase proof size
    pub blocks_per_challenge: u32,
}

impl Default for ChallengeConfig {
    fn default() -> Self {
        Self {
            base_interval_secs: 3600, // 1 hour
            jitter_secs: 600,         // 0-10 minutes
            timeout_secs: DEFAULT_CHALLENGE_TIMEOUT_SECS,
            grace_attempts: 3,
            challenge_probability: 0.1, // 10% per round
            max_pending_per_node: 10,
            max_pending_total: 1000,
            chunk_size: DEFAULT_CHUNK_SIZE,
            byte_sample_size: MAX_BYTE_SAMPLE_SIZE,
            blocks_per_challenge: DEFAULT_BLOCKS_PER_CHALLENGE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_challenge_creation() {
        let content_hash = [1u8; 32];
        let challenge = StorageChallenge::new(
            content_hash,
            "did:icn:target".to_string(),
            100,
            1024,
            5,
            "did:icn:challenger".to_string(),
            30,
        );

        assert_eq!(challenge.content_hash, content_hash);
        assert_eq!(challenge.byte_offset, 100);
        assert_eq!(challenge.byte_length, 1024);
        assert_eq!(challenge.chunk_index(), Some(5));
        assert_eq!(challenge.chunk_indices, vec![5]);
        assert_eq!(challenge.version, CHALLENGE_PROTOCOL_VERSION);
        assert!(!challenge.is_expired());
        assert!(challenge.validate_id());
    }

    #[test]
    fn test_multi_block_challenge_creation() {
        let content_hash = [1u8; 32];
        let challenge = StorageChallenge::new_multi_block(
            content_hash,
            "did:icn:target".to_string(),
            100,
            1024,
            vec![2, 5, 8],
            "did:icn:challenger".to_string(),
            30,
        );

        assert_eq!(challenge.chunk_indices, vec![2, 5, 8]);
        assert_eq!(challenge.chunk_index(), Some(2));
        assert_eq!(challenge.version, 2);
        assert_eq!(challenge.challenge_nonce.len(), 32);
        assert!(challenge.validate_id());
    }

    #[test]
    fn test_challenge_id_deterministic() {
        let content_hash = [1u8; 32];
        let mut c1 = StorageChallenge::new(
            content_hash,
            "did:icn:target".to_string(),
            100,
            1024,
            5,
            "did:icn:challenger".to_string(),
            30,
        );

        // Same parameters should produce same ID
        let mut c2 = c1.clone();
        c2.id = [0u8; 32];
        c2.id = c2.compute_id();

        assert_eq!(c1.id, c2.id);

        // Different nonce should produce different ID
        c1.challenge_nonce = [0x42u8; 32];
        c1.id = c1.compute_id();
        assert_ne!(c1.id, c2.id);
    }

    #[test]
    fn test_merkle_tree_small_content() {
        let content = b"hello world".to_vec();
        let tree = ContentChunkTree::new(content.clone(), 4);

        assert!(tree.num_chunks() >= 3); // "hell", "o wo", "rld"
        assert_eq!(tree.content_size(), 11);

        // Root should be deterministic
        let tree2 = ContentChunkTree::new(content, 4);
        assert_eq!(tree.root(), tree2.root());
    }

    #[test]
    fn test_merkle_tree_single_chunk() {
        let content = b"hi".to_vec();
        let tree = ContentChunkTree::new(content, 1024);

        assert_eq!(tree.num_chunks(), 1);
        assert_eq!(tree.root(), tree.chunk_hash(0).unwrap());
    }

    #[test]
    fn test_merkle_proof_verification() {
        let content = b"0123456789abcdef".to_vec();
        let tree = ContentChunkTree::new(content, 4);

        for i in 0..tree.num_chunks() {
            let proof = tree.generate_proof(i).expect("should generate proof");
            assert!(proof.verify(), "proof for chunk {i} should verify");
        }
    }

    #[test]
    fn test_merkle_proof_invalid_root() {
        let content = b"0123456789abcdef".to_vec();
        let tree = ContentChunkTree::new(content, 4);

        let proof = tree.generate_proof(0).expect("should generate proof");
        let wrong_root = [0xFFu8; 32];
        assert!(!proof.verify_against_root(&wrong_root));
    }

    #[test]
    fn test_get_bytes() {
        let content = b"hello world".to_vec();
        let tree = ContentChunkTree::new(content, 4);

        assert_eq!(tree.get_bytes(0, 5), Some(b"hello".to_vec()));
        assert_eq!(tree.get_bytes(6, 5), Some(b"world".to_vec()));
        assert_eq!(tree.get_bytes(6, 100), Some(b"world".to_vec())); // Clamped
        assert_eq!(tree.get_bytes(100, 5), None); // Out of bounds
    }

    #[test]
    fn test_storage_proof_creation() {
        let proof = StorageProof::new(
            [1u8; 32],
            b"test data".to_vec(),
            [2u8; 32],
            vec![[3u8; 32], [4u8; 32]],
            vec![false, true],
            "did:icn:prover".to_string(),
        );

        assert_eq!(proof.challenge_id, [1u8; 32]);
        assert_eq!(proof.byte_data, b"test data");
        assert_eq!(proof.version, CHALLENGE_PROTOCOL_VERSION);
        assert_eq!(proof.merkle_proofs.len(), 1);
        assert_eq!(proof.chunk_hash(), Some([2u8; 32]));
        assert!(!proof.signing_payload().is_empty());
    }

    #[test]
    fn test_multi_proof_creation() {
        let proofs = vec![
            MerkleProofData {
                chunk_index: 2,
                chunk_hash: [2u8; 32],
                siblings: vec![[10u8; 32], [11u8; 32]],
                path_bits: vec![false, true],
            },
            MerkleProofData {
                chunk_index: 5,
                chunk_hash: [5u8; 32],
                siblings: vec![[12u8; 32], [13u8; 32]],
                path_bits: vec![true, false],
            },
        ];

        let proof = StorageProof::new_multi(
            [1u8; 32],
            b"test data".to_vec(),
            proofs,
            "did:icn:prover".to_string(),
        );

        assert_eq!(proof.merkle_proofs.len(), 2);
        assert_eq!(proof.merkle_proofs[0].chunk_index, 2);
        assert_eq!(proof.merkle_proofs[1].chunk_index, 5);
        assert_eq!(proof.chunk_hash(), Some([2u8; 32]));
    }

    #[test]
    fn test_merkle_proof_data_verification() {
        let content = b"0123456789abcdef".to_vec();
        let tree = ContentChunkTree::new(content, 4);

        let proof = tree.generate_proof(0).expect("should generate proof");
        let proof_data = MerkleProofData::from(proof);

        assert!(proof_data.verify_against_root(&tree.root()));
    }

    #[test]
    fn test_failure_severity() {
        assert_eq!(StorageFailureReason::NoResponse.severity(), 1);
        assert_eq!(StorageFailureReason::ContentNotFound.severity(), 3);
        assert_eq!(StorageFailureReason::DataMismatch.severity(), 5);
        assert_eq!(StorageFailureReason::InvalidMerkleProof.severity(), 8);
    }

    #[test]
    fn test_config_default() {
        let config = ChallengeConfig::default();
        assert_eq!(config.base_interval_secs, 3600);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.grace_attempts, 3);
        assert_eq!(config.blocks_per_challenge, DEFAULT_BLOCKS_PER_CHALLENGE);
    }
}
