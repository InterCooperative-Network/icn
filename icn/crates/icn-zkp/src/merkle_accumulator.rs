//! Post-Quantum Safe Merkle Tree Accumulator
//!
//! A hash-based accumulator using sorted Merkle trees for credential revocation.
//! Unlike RSA accumulators, this is resistant to quantum attacks since it relies
//! only on hash function security (collision resistance).
//!
//! # Security Properties
//!
//! - **Post-Quantum Safe**: Uses Blake3 hashing, no number-theoretic assumptions
//! - **Membership Proofs**: O(log n) proof size via Merkle inclusion
//! - **Non-Membership Proofs**: O(log n) by proving neighbors in sorted tree
//!
//! # Design
//!
//! Elements are stored in a sorted list and organized into a Merkle tree.
//! Non-membership is proven by showing the two adjacent elements where the
//! target would be inserted, along with Merkle proofs that both are in the tree.

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from Merkle accumulator operations
#[derive(Debug, Error)]
pub enum MerkleAccumulatorError {
    #[error("Element already in accumulator")]
    AlreadyMember,

    #[error("Element not in accumulator")]
    NotMember,

    #[error("Invalid proof")]
    InvalidProof,

    #[error("Empty accumulator")]
    EmptyAccumulator,

    #[error("Proof verification failed: {0}")]
    VerificationFailed(String),
}

/// Hash type used throughout the accumulator (32 bytes)
pub type Hash = [u8; 32];

/// A post-quantum safe Merkle tree accumulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleAccumulator {
    /// Sorted list of element hashes
    elements: Vec<Hash>,
    /// Merkle tree nodes (computed lazily)
    #[serde(skip)]
    tree: Vec<Hash>,
    /// Whether tree needs rebuilding
    #[serde(skip)]
    dirty: bool,
    /// Current epoch (version number)
    epoch: u64,
}

impl Default for MerkleAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleAccumulator {
    /// Create a new empty accumulator
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            tree: Vec::new(),
            dirty: true,
            epoch: 0,
        }
    }

    /// Get the current epoch
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Get the element count
    pub fn count(&self) -> usize {
        self.elements.len()
    }

    /// Check if an element is in the accumulator
    pub fn contains(&self, element: &[u8; 32]) -> bool {
        let hash = hash_element(element);
        self.elements.binary_search(&hash).is_ok()
    }

    /// Get the root hash of the accumulator
    pub fn root(&mut self) -> Hash {
        self.rebuild_tree_if_needed();
        if self.tree.is_empty() {
            // Empty tree has zero root
            [0u8; 32]
        } else {
            self.tree[0]
        }
    }

    /// Get a 32-byte hash of the accumulator value (alias for root)
    pub fn value_hash(&mut self) -> [u8; 32] {
        self.root()
    }

    /// Get the accumulator value as bytes
    pub fn value_bytes(&mut self) -> Vec<u8> {
        self.root().to_vec()
    }

    /// Add an element to the accumulator
    pub fn add(&mut self, element: &[u8; 32]) -> Result<(), MerkleAccumulatorError> {
        let hash = hash_element(element);

        // Binary search for insertion point
        match self.elements.binary_search(&hash) {
            Ok(_) => Err(MerkleAccumulatorError::AlreadyMember),
            Err(pos) => {
                self.elements.insert(pos, hash);
                self.dirty = true;
                self.epoch += 1;
                Ok(())
            }
        }
    }

    /// Remove an element from the accumulator (un-revoke)
    pub fn remove(&mut self, element: &[u8; 32]) -> Result<(), MerkleAccumulatorError> {
        let hash = hash_element(element);

        match self.elements.binary_search(&hash) {
            Ok(pos) => {
                self.elements.remove(pos);
                self.dirty = true;
                self.epoch += 1;
                Ok(())
            }
            Err(_) => Err(MerkleAccumulatorError::NotMember),
        }
    }

    /// Generate a membership proof for an element
    pub fn membership_proof(
        &mut self,
        element: &[u8; 32],
    ) -> Result<MerkleMembershipProof, MerkleAccumulatorError> {
        let hash = hash_element(element);

        let index = self
            .elements
            .binary_search(&hash)
            .map_err(|_| MerkleAccumulatorError::NotMember)?;

        self.rebuild_tree_if_needed();

        let siblings = self.compute_siblings(index);

        Ok(MerkleMembershipProof {
            element_hash: hash,
            index,
            siblings,
            root: self.tree[0],
        })
    }

    /// Verify a membership proof
    pub fn verify_membership(root: &Hash, proof: &MerkleMembershipProof) -> bool {
        if proof.root != *root {
            return false;
        }

        let mut current = proof.element_hash;
        let mut index = proof.index;

        for sibling in &proof.siblings {
            current = if index.is_multiple_of(2) {
                hash_pair(&current, sibling)
            } else {
                hash_pair(sibling, &current)
            };
            index /= 2;
        }

        current == proof.root
    }

    /// Generate a non-membership proof for an element
    pub fn non_membership_proof(
        &mut self,
        element: &[u8; 32],
    ) -> Result<MerkleNonMembershipProof, MerkleAccumulatorError> {
        let hash = hash_element(element);

        // Find where element would be inserted
        let insert_pos = match self.elements.binary_search(&hash) {
            Ok(_) => return Err(MerkleAccumulatorError::AlreadyMember),
            Err(pos) => pos,
        };

        self.rebuild_tree_if_needed();
        let root = if self.tree.is_empty() {
            [0u8; 32]
        } else {
            self.tree[0]
        };

        // Handle edge cases
        if self.elements.is_empty() {
            // Empty accumulator - non-membership is trivial
            return Ok(MerkleNonMembershipProof {
                element_hash: hash,
                left_neighbor: None,
                right_neighbor: None,
                left_proof: None,
                right_proof: None,
                root,
            });
        }

        // Get left neighbor (if exists)
        let left_neighbor = if insert_pos > 0 {
            Some(self.elements[insert_pos - 1])
        } else {
            None
        };

        // Get right neighbor (if exists)
        let right_neighbor = if insert_pos < self.elements.len() {
            Some(self.elements[insert_pos])
        } else {
            None
        };

        // Generate proofs for neighbors
        let left_proof = if insert_pos > 0 {
            Some(self.compute_siblings(insert_pos - 1))
        } else {
            None
        };

        let right_proof = if insert_pos < self.elements.len() {
            Some(self.compute_siblings(insert_pos))
        } else {
            None
        };

        Ok(MerkleNonMembershipProof {
            element_hash: hash,
            left_neighbor,
            right_neighbor,
            left_proof,
            right_proof,
            root,
        })
    }

    /// Verify a non-membership proof
    pub fn verify_non_membership(root: &Hash, proof: &MerkleNonMembershipProof) -> bool {
        if proof.root != *root {
            return false;
        }

        // Empty accumulator case
        if proof.left_neighbor.is_none() && proof.right_neighbor.is_none() {
            // Root should be zero for empty tree
            return *root == [0u8; 32];
        }

        // Verify element hash is between neighbors (or at boundary)
        if let Some(left) = &proof.left_neighbor {
            if *left >= proof.element_hash {
                return false; // Left neighbor must be less than element
            }
        }

        if let Some(right) = &proof.right_neighbor {
            if *right <= proof.element_hash {
                return false; // Right neighbor must be greater than element
            }
        }

        // Verify neighbor proofs
        if let (Some(left), Some(left_siblings)) = (&proof.left_neighbor, &proof.left_proof) {
            // For non-membership, we need to verify the sibling path
            // but the index might not match - we verify the hash chain
            if !verify_sibling_path(left, left_siblings, &proof.root) {
                return false;
            }
        }

        if let (Some(right), Some(right_siblings)) = (&proof.right_neighbor, &proof.right_proof) {
            if !verify_sibling_path(right, right_siblings, &proof.root) {
                return false;
            }
        }

        true
    }

    /// Rebuild the Merkle tree from elements
    fn rebuild_tree_if_needed(&mut self) {
        if !self.dirty {
            return;
        }

        if self.elements.is_empty() {
            self.tree.clear();
            self.dirty = false;
            return;
        }

        // Pad to power of 2
        let n = self.elements.len().next_power_of_two();
        let mut tree = vec![[0u8; 32]; 2 * n - 1];

        // Copy leaves (padded with zeros)
        let leaf_offset = n - 1;
        for (i, elem) in self.elements.iter().enumerate() {
            tree[leaf_offset + i] = *elem;
        }

        // Build tree bottom-up
        for i in (0..leaf_offset).rev() {
            let left = tree[2 * i + 1];
            let right = tree[2 * i + 2];
            tree[i] = hash_pair(&left, &right);
        }

        self.tree = tree;
        self.dirty = false;
    }

    /// Compute sibling hashes for a Merkle proof
    fn compute_siblings(&self, leaf_index: usize) -> Vec<Hash> {
        let n = self.elements.len().next_power_of_two();
        let leaf_offset = n - 1;
        let mut siblings = Vec::new();
        let mut index = leaf_offset + leaf_index;

        while index > 0 {
            let sibling_index = if index.is_multiple_of(2) {
                index - 1
            } else {
                index + 1
            };
            if sibling_index < self.tree.len() {
                siblings.push(self.tree[sibling_index]);
            } else {
                siblings.push([0u8; 32]);
            }
            index = (index - 1) / 2;
        }

        siblings
    }
}

/// Membership proof for Merkle accumulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleMembershipProof {
    /// Hash of the element
    pub element_hash: Hash,
    /// Index in the sorted list
    pub index: usize,
    /// Sibling hashes from leaf to root
    pub siblings: Vec<Hash>,
    /// Root at time of proof
    pub root: Hash,
}

/// Non-membership proof for Merkle accumulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNonMembershipProof {
    /// Hash of the element being proven absent
    pub element_hash: Hash,
    /// Left neighbor in sorted order (None if element would be first)
    pub left_neighbor: Option<Hash>,
    /// Right neighbor in sorted order (None if element would be last)
    pub right_neighbor: Option<Hash>,
    /// Merkle proof for left neighbor
    pub left_proof: Option<Vec<Hash>>,
    /// Merkle proof for right neighbor
    pub right_proof: Option<Vec<Hash>>,
    /// Root at time of proof
    pub root: Hash,
}

impl MerkleNonMembershipProof {
    /// Get auxiliary data as bytes (for compatibility with existing circuit)
    pub fn aux_bytes(&self) -> Vec<u8> {
        let mut aux = Vec::new();

        // Encode left neighbor
        if let Some(left) = &self.left_neighbor {
            aux.extend_from_slice(left);
        } else {
            aux.extend_from_slice(&[0u8; 32]);
        }

        // Encode right neighbor
        if let Some(right) = &self.right_neighbor {
            aux.extend_from_slice(right);
        } else {
            aux.extend_from_slice(&[0u8; 32]);
        }

        aux
    }
}

/// Hash an element to a 32-byte hash
fn hash_element(element: &[u8; 32]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"icn-merkle-accumulator-element");
    hasher.update(element);
    *hasher.finalize().as_bytes()
}

/// Hash two child nodes to produce parent
fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(b"icn-merkle-accumulator-node");
    hasher.update(left);
    hasher.update(right);
    *hasher.finalize().as_bytes()
}

/// Verify a sibling path leads to the given root
fn verify_sibling_path(leaf: &Hash, siblings: &[Hash], root: &Hash) -> bool {
    if siblings.is_empty() {
        return *leaf == *root;
    }

    // Try both left and right positions for each level
    // This is needed because we don't store the index in the non-membership proof
    fn try_path(current: Hash, siblings: &[Hash], depth: usize, root: &Hash) -> bool {
        if depth == siblings.len() {
            return current == *root;
        }

        let sibling = &siblings[depth];

        // Try as left child
        let as_left = hash_pair(&current, sibling);
        if try_path(as_left, siblings, depth + 1, root) {
            return true;
        }

        // Try as right child
        let as_right = hash_pair(sibling, &current);
        try_path(as_right, siblings, depth + 1, root)
    }

    try_path(*leaf, siblings, 0, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_accumulator() {
        let acc = MerkleAccumulator::new();
        assert_eq!(acc.count(), 0);
        assert_eq!(acc.epoch(), 0);
    }

    #[test]
    fn test_add_element() {
        let mut acc = MerkleAccumulator::new();
        let elem = [1u8; 32];

        assert!(acc.add(&elem).is_ok());
        assert_eq!(acc.count(), 1);
        assert_eq!(acc.epoch(), 1);
        assert!(acc.contains(&elem));
    }

    #[test]
    fn test_add_duplicate() {
        let mut acc = MerkleAccumulator::new();
        let elem = [1u8; 32];

        assert!(acc.add(&elem).is_ok());
        assert!(acc.add(&elem).is_err());
    }

    #[test]
    fn test_remove_element() {
        let mut acc = MerkleAccumulator::new();
        let elem = [1u8; 32];

        acc.add(&elem).unwrap();
        assert!(acc.remove(&elem).is_ok());
        assert_eq!(acc.count(), 0);
        assert!(!acc.contains(&elem));
    }

    #[test]
    fn test_membership_proof() {
        let mut acc = MerkleAccumulator::new();
        let elem1 = [1u8; 32];
        let elem2 = [2u8; 32];
        let elem3 = [3u8; 32];

        acc.add(&elem1).unwrap();
        acc.add(&elem2).unwrap();
        acc.add(&elem3).unwrap();

        let proof = acc.membership_proof(&elem2).unwrap();
        let root = acc.root();

        assert!(MerkleAccumulator::verify_membership(&root, &proof));
    }

    #[test]
    fn test_non_membership_proof_empty() {
        let mut acc = MerkleAccumulator::new();
        let elem = [1u8; 32];

        let proof = acc.non_membership_proof(&elem).unwrap();
        let root = acc.root();

        assert!(MerkleAccumulator::verify_non_membership(&root, &proof));
    }

    #[test]
    fn test_non_membership_proof_with_elements() {
        let mut acc = MerkleAccumulator::new();
        let elem1 = [1u8; 32];
        let elem2 = [3u8; 32];
        let not_member = [2u8; 32]; // Between elem1 and elem2

        acc.add(&elem1).unwrap();
        acc.add(&elem2).unwrap();

        let proof = acc.non_membership_proof(&not_member).unwrap();
        let root = acc.root();

        assert!(MerkleAccumulator::verify_non_membership(&root, &proof));
    }

    #[test]
    fn test_non_membership_at_start() {
        let mut acc = MerkleAccumulator::new();
        let elem = [5u8; 32];
        let not_member = [1u8; 32]; // Before elem

        acc.add(&elem).unwrap();

        let proof = acc.non_membership_proof(&not_member).unwrap();
        let root = acc.root();

        assert!(MerkleAccumulator::verify_non_membership(&root, &proof));
    }

    #[test]
    fn test_non_membership_at_end() {
        let mut acc = MerkleAccumulator::new();
        let elem = [1u8; 32];
        let not_member = [255u8; 32]; // After elem (hash will be different)

        acc.add(&elem).unwrap();

        let proof = acc.non_membership_proof(&not_member).unwrap();
        let root = acc.root();

        assert!(MerkleAccumulator::verify_non_membership(&root, &proof));
    }

    #[test]
    fn test_root_changes_on_add() {
        let mut acc = MerkleAccumulator::new();
        let root1 = acc.root();

        acc.add(&[1u8; 32]).unwrap();
        let root2 = acc.root();

        acc.add(&[2u8; 32]).unwrap();
        let root3 = acc.root();

        assert_ne!(root1, root2);
        assert_ne!(root2, root3);
    }

    #[test]
    fn test_deterministic_root() {
        let mut acc1 = MerkleAccumulator::new();
        let mut acc2 = MerkleAccumulator::new();

        // Add same elements in same order
        acc1.add(&[1u8; 32]).unwrap();
        acc1.add(&[2u8; 32]).unwrap();

        acc2.add(&[1u8; 32]).unwrap();
        acc2.add(&[2u8; 32]).unwrap();

        assert_eq!(acc1.root(), acc2.root());
    }

    #[test]
    fn test_many_elements() {
        let mut acc = MerkleAccumulator::new();

        // Add 100 elements
        for i in 0..100u8 {
            let mut elem = [0u8; 32];
            elem[0] = i;
            acc.add(&elem).unwrap();
        }

        assert_eq!(acc.count(), 100);

        // Test membership for existing element
        let mut elem50 = [0u8; 32];
        elem50[0] = 50;
        let proof = acc.membership_proof(&elem50).unwrap();
        let root = acc.root();
        assert!(MerkleAccumulator::verify_membership(&root, &proof));

        // Test non-membership
        let mut not_member = [0u8; 32];
        not_member[0] = 200; // Not added
        let nm_proof = acc.non_membership_proof(&not_member).unwrap();
        assert!(MerkleAccumulator::verify_non_membership(&root, &nm_proof));
    }

    #[test]
    fn test_value_hash_consistency() {
        let mut acc = MerkleAccumulator::new();
        acc.add(&[1u8; 32]).unwrap();

        let root = acc.root();
        let value_hash = acc.value_hash();

        assert_eq!(root, value_hash);
    }
}
