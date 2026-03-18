//! Non-Revocation Proof Circuit
//!
//! Proves that a credential has not been revoked using accumulator-based proofs.
//!
//! # Accumulator Support
//!
//! This circuit supports two accumulator types:
//!
//! - **RSA Accumulator** (legacy): Uses RSA modular exponentiation. NOT post-quantum safe.
//! - **Merkle Accumulator** (recommended): Uses sorted Merkle trees with Blake3. Post-quantum safe.
//!
//! The Merkle accumulator is recommended for new deployments due to its quantum resistance.

use super::{compute_public_inputs_hash, Circuit, CircuitError};
use crate::merkle_accumulator::MerkleNonMembershipProof;
use crate::types::StarkProof;
use serde::{Deserialize, Serialize};

/// Type of accumulator being used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccumulatorType {
    /// RSA-based accumulator (legacy, NOT post-quantum safe)
    Rsa,
    /// Merkle tree accumulator (post-quantum safe, RECOMMENDED)
    #[default]
    Merkle,
}

impl AccumulatorType {
    /// Check if this accumulator type is post-quantum safe
    pub fn is_pq_safe(&self) -> bool {
        matches!(self, AccumulatorType::Merkle)
    }

    /// Get a description of the accumulator type
    pub fn description(&self) -> &'static str {
        match self {
            AccumulatorType::Rsa => "RSA accumulator (legacy, NOT post-quantum safe)",
            AccumulatorType::Merkle => "Merkle accumulator (post-quantum safe)",
        }
    }
}

/// Public inputs for non-revocation proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonRevocationPublic {
    /// Current accumulator value (root hash)
    pub accumulator_value: [u8; 32],
    /// Accumulator epoch/version
    pub accumulator_epoch: u64,
    /// Issuer's public key
    pub issuer_pk: [u8; 32],
    /// Proof context nonce
    pub nonce: [u8; 16],
    /// Type of accumulator (RSA or Merkle)
    #[serde(default)]
    pub accumulator_type: AccumulatorType,
}

impl NonRevocationPublic {
    /// Create new public inputs (defaults to RSA for backward compatibility)
    ///
    /// For post-quantum safe proofs, use [`new_merkle()`] instead.
    pub fn new(accumulator_value: [u8; 32], accumulator_epoch: u64, issuer_pk: [u8; 32]) -> Self {
        Self::with_type(
            accumulator_value,
            accumulator_epoch,
            issuer_pk,
            AccumulatorType::Rsa,
        )
    }

    /// Create new public inputs for Merkle accumulator (post-quantum safe, RECOMMENDED)
    pub fn new_merkle(
        accumulator_value: [u8; 32],
        accumulator_epoch: u64,
        issuer_pk: [u8; 32],
    ) -> Self {
        Self::with_type(
            accumulator_value,
            accumulator_epoch,
            issuer_pk,
            AccumulatorType::Merkle,
        )
    }

    /// Create new public inputs with specified accumulator type
    pub fn with_type(
        accumulator_value: [u8; 32],
        accumulator_epoch: u64,
        issuer_pk: [u8; 32],
        accumulator_type: AccumulatorType,
    ) -> Self {
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        Self {
            accumulator_value,
            accumulator_epoch,
            issuer_pk,
            nonce,
            accumulator_type,
        }
    }

    /// Validate public inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        // RSA accumulator value should not be all zeros (indicates uninitialized)
        // Merkle accumulator CAN have zero root (empty revocation list is valid)
        if self.accumulator_type == AccumulatorType::Rsa && self.accumulator_value == [0u8; 32] {
            return Err(CircuitError::InvalidPublicInput(
                "RSA accumulator value cannot be zero".into(),
            ));
        }
        Ok(())
    }
}

/// Private inputs for non-revocation proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonRevocationPrivate {
    /// Credential identifier (what would be in revocation list if revoked)
    pub credential_id: [u8; 32],
    /// Non-membership witness for accumulator
    pub witness: NonMembershipWitness,
    /// Blinding factor
    pub blinding: [u8; 32],
}

/// Witness for proving non-membership in accumulator
///
/// This struct supports both RSA and Merkle accumulator witnesses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonMembershipWitness {
    /// Bezout coefficient a (for RSA: ax + by = 1)
    #[serde(default)]
    pub a: Vec<u8>,
    /// Bezout coefficient b (for RSA)
    #[serde(default)]
    pub b: Vec<u8>,
    /// Auxiliary data (RSA: accumulator value, Merkle: serialized proof)
    pub aux: [u8; 32],
    /// Additional proof data for Merkle accumulator
    #[serde(default)]
    pub merkle_proof: Option<MerkleNonMembershipProofData>,
}

/// Serializable Merkle non-membership proof data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNonMembershipProofData {
    /// Left neighbor hash (None if element would be first)
    pub left_neighbor: Option<[u8; 32]>,
    /// Right neighbor hash (None if element would be last)
    pub right_neighbor: Option<[u8; 32]>,
    /// Index of left neighbor (for O(log n) verification)
    #[serde(default)]
    pub left_index: Option<usize>,
    /// Index of right neighbor (for O(log n) verification)
    #[serde(default)]
    pub right_index: Option<usize>,
    /// Merkle proof for left neighbor (sibling hashes)
    pub left_proof: Option<Vec<[u8; 32]>>,
    /// Merkle proof for right neighbor
    pub right_proof: Option<Vec<[u8; 32]>>,
}

impl From<MerkleNonMembershipProof> for MerkleNonMembershipProofData {
    fn from(proof: MerkleNonMembershipProof) -> Self {
        Self {
            left_neighbor: proof.left_neighbor,
            right_neighbor: proof.right_neighbor,
            left_index: proof.left_index,
            right_index: proof.right_index,
            left_proof: proof.left_proof,
            right_proof: proof.right_proof,
        }
    }
}

impl NonMembershipWitness {
    /// Create a simple witness (for testing with RSA accumulator)
    pub fn simple(credential_id: &[u8; 32]) -> Self {
        Self {
            a: vec![1u8; 32],
            b: vec![1u8; 32],
            aux: *credential_id,
            merkle_proof: None,
        }
    }

    /// Create a witness from Merkle non-membership proof
    pub fn from_merkle(proof: MerkleNonMembershipProof) -> Self {
        Self {
            a: Vec::new(),
            b: Vec::new(),
            aux: proof.element_hash,
            merkle_proof: Some(MerkleNonMembershipProofData::from(proof)),
        }
    }

    /// Check if this is a Merkle witness
    pub fn is_merkle(&self) -> bool {
        self.merkle_proof.is_some()
    }
}

impl NonRevocationPrivate {
    /// Validate private inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        // For Merkle witnesses, the merkle_proof must be present
        // For RSA witnesses, the Bezout coefficients must be non-empty
        if self.witness.is_merkle() {
            // Merkle witness validation
            if self.witness.merkle_proof.is_none() {
                return Err(CircuitError::InvalidPrivateInput(
                    "Merkle witness missing proof data".into(),
                ));
            }
        } else {
            // RSA witness validation
            if self.witness.a.is_empty() || self.witness.b.is_empty() {
                return Err(CircuitError::InvalidPrivateInput(
                    "RSA witness coefficients cannot be empty".into(),
                ));
            }
        }
        Ok(())
    }

    /// Create private inputs from a Merkle accumulator
    pub fn from_merkle(
        credential_id: [u8; 32],
        proof: MerkleNonMembershipProof,
        blinding: [u8; 32],
    ) -> Self {
        Self {
            credential_id,
            witness: NonMembershipWitness::from_merkle(proof),
            blinding,
        }
    }
}

/// Non-revocation proof circuit
pub struct NonRevocationCircuit;

impl Circuit for NonRevocationCircuit {
    type Public = NonRevocationPublic;
    type Private = NonRevocationPrivate;

    fn name() -> &'static str {
        "non_revocation_proof_v1"
    }

    #[allow(clippy::needless_return)]
    fn prove(public: &Self::Public, private: &Self::Private) -> Result<StarkProof, CircuitError> {
        public.validate()?;
        private.validate()?;

        // Security: Ensure witness type matches accumulator type
        // This prevents downgrade attacks where Merkle type is declared but RSA witness is used
        let witness_is_merkle = private.witness.is_merkle();
        let acc_is_merkle = public.accumulator_type == AccumulatorType::Merkle;
        if witness_is_merkle != acc_is_merkle {
            return Err(CircuitError::InvalidPrivateInput(format!(
                "Witness type mismatch: witness is {}, but accumulator type is {}",
                if witness_is_merkle { "Merkle" } else { "RSA" },
                public.accumulator_type.description()
            )));
        }

        // In a real implementation, we would verify:
        // 1. The Bezout coefficients prove non-membership
        // 2. The witness is valid for the current accumulator value
        //
        // For now, we simulate the proof generation

        // Build the proof based on enabled features
        let public_hash = compute_public_inputs_hash(public);

        #[cfg(feature = "stark")]
        {
            // Use the winterfell STARK prover
            let _ = public_hash;
            return crate::stark::non_revocation::prove_non_revocation(public, private);
        }

        #[cfg(all(not(feature = "stark"), feature = "simulated"))]
        {
            // SIMULATED proof for testing ONLY
            // WARNING: This provides NO cryptographic security!
            use sha3::{Digest, Sha3_256};

            tracing::warn!("Creating SIMULATED non-revocation proof - NO CRYPTOGRAPHIC SECURITY");

            let mut hasher = Sha3_256::new();
            hasher.update(private.credential_id);
            hasher.update(private.witness.aux);
            hasher.update(private.blinding);
            hasher.update(public.nonce);
            let commitment: [u8; 32] = hasher.finalize().into();

            let mut proof_data = Vec::with_capacity(1024);
            proof_data.extend_from_slice(&commitment);
            proof_data.extend_from_slice(&public_hash);
            proof_data.extend_from_slice(&[0u8; 960]);

            Ok(StarkProof::new_simulated(proof_data, public_hash))
        }

        #[cfg(all(not(feature = "stark"), not(feature = "simulated")))]
        {
            // No proof capability - return error
            let _ = public_hash;
            Err(CircuitError::ProofGenerationFailed(
                "ZKP proving not available. Enable 'stark' feature for production, \
                 or 'simulated' feature for testing only."
                    .into(),
            ))
        }
    }

    #[allow(clippy::needless_return)]
    fn verify(public: &Self::Public, proof: &StarkProof) -> Result<bool, CircuitError> {
        public.validate()?;

        let expected_hash = compute_public_inputs_hash(public);
        if proof.public_inputs_hash != expected_hash {
            return Ok(false);
        }

        #[cfg(feature = "stark")]
        {
            // Use the winterfell STARK verifier
            return crate::stark::non_revocation::verify_non_revocation(public, proof);
        }

        #[cfg(all(not(feature = "stark"), feature = "simulated"))]
        {
            // SIMULATED verification for testing ONLY
            if proof.is_simulated() {
                tracing::warn!("Verifying SIMULATED proof - NO CRYPTOGRAPHIC SECURITY");
            }

            // Check proof structure
            if proof.proof_bytes.len() < 64 {
                return Ok(false);
            }

            // Verify the proof contains the expected public hash
            if proof.proof_bytes[32..64] != expected_hash[..] {
                return Ok(false);
            }

            Ok(true)
        }

        #[cfg(all(not(feature = "stark"), not(feature = "simulated")))]
        {
            let _ = expected_hash;
            Err(CircuitError::VerificationFailed(
                "ZKP verification not available. Enable 'stark' feature for production, \
                 or 'simulated' feature for testing only."
                    .into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "simulated")]
    fn test_non_revocation_proof() {
        let issuer_pk = [1u8; 32];
        let accumulator = [2u8; 32];
        let credential_id = [3u8; 32];

        let public = NonRevocationPublic::new(accumulator, 1, issuer_pk);

        let private = NonRevocationPrivate {
            credential_id,
            witness: NonMembershipWitness::simple(&credential_id),
            blinding: [0u8; 32],
        };

        let proof = NonRevocationCircuit::prove(&public, &private).unwrap();
        assert!(NonRevocationCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    fn test_zero_rsa_accumulator_invalid() {
        let issuer_pk = [1u8; 32];
        let accumulator = [0u8; 32]; // Invalid for RSA

        // RSA accumulator with zero value should be invalid
        let public =
            NonRevocationPublic::with_type(accumulator, 1, issuer_pk, AccumulatorType::Rsa);
        assert!(public.validate().is_err());
    }

    #[test]
    fn test_zero_merkle_accumulator_valid() {
        let issuer_pk = [1u8; 32];
        let accumulator = [0u8; 32]; // Valid for empty Merkle tree

        // Merkle accumulator with zero root (empty) should be valid
        let public =
            NonRevocationPublic::with_type(accumulator, 0, issuer_pk, AccumulatorType::Merkle);
        assert!(public.validate().is_ok());
    }

    #[test]
    #[cfg(feature = "simulated")]
    fn test_empty_merkle_accumulator_proof() {
        use crate::merkle_accumulator::MerkleAccumulator;

        let issuer_pk = [1u8; 32];
        let credential_id = [5u8; 32];

        // Empty Merkle accumulator (no revoked credentials)
        let mut acc = MerkleAccumulator::new();
        let merkle_proof = acc.non_membership_proof(&credential_id).unwrap();
        let root = acc.root();

        // Root should be zero for empty accumulator
        assert_eq!(root, [0u8; 32]);

        // Create public inputs
        let public =
            NonRevocationPublic::with_type(root, acc.epoch(), issuer_pk, AccumulatorType::Merkle);

        // Validation should pass
        assert!(public.validate().is_ok());

        // Create private inputs
        let private = NonRevocationPrivate::from_merkle(credential_id, merkle_proof, [0u8; 32]);

        // Generate and verify proof
        let proof = NonRevocationCircuit::prove(&public, &private).unwrap();
        assert!(NonRevocationCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    fn test_accumulator_type_pq_safe() {
        assert!(!AccumulatorType::Rsa.is_pq_safe());
        assert!(AccumulatorType::Merkle.is_pq_safe());
    }

    #[test]
    fn test_merkle_witness_creation() {
        use crate::merkle_accumulator::MerkleAccumulator;

        let mut acc = MerkleAccumulator::new();
        let elem1 = [1u8; 32];
        let elem2 = [3u8; 32];

        acc.add(&elem1).unwrap();
        acc.add(&elem2).unwrap();

        // Credential not in revocation list
        let credential_id = [2u8; 32];
        let proof = acc.non_membership_proof(&credential_id).unwrap();
        let root = acc.root();

        // Verify the raw Merkle proof
        assert!(MerkleAccumulator::verify_non_membership(&root, &proof));

        // Create witness from Merkle proof
        let witness = NonMembershipWitness::from_merkle(proof);
        assert!(witness.is_merkle());
        assert!(witness.merkle_proof.is_some());
    }

    #[test]
    #[cfg(feature = "simulated")]
    fn test_merkle_non_revocation_proof() {
        use crate::merkle_accumulator::MerkleAccumulator;

        let issuer_pk = [1u8; 32];
        let credential_id = [5u8; 32];

        // Create Merkle accumulator with some revoked credentials
        let mut acc = MerkleAccumulator::new();
        acc.add(&[1u8; 32]).unwrap(); // revoked
        acc.add(&[2u8; 32]).unwrap(); // revoked
        acc.add(&[3u8; 32]).unwrap(); // revoked

        // Get proof that credential_id is NOT in revocation list
        let merkle_proof = acc.non_membership_proof(&credential_id).unwrap();
        let root = acc.root();

        // Create public inputs with Merkle accumulator type
        let public =
            NonRevocationPublic::with_type(root, acc.epoch(), issuer_pk, AccumulatorType::Merkle);

        // Create private inputs from Merkle proof
        let private = NonRevocationPrivate::from_merkle(credential_id, merkle_proof, [0u8; 32]);

        // Generate and verify proof
        let proof = NonRevocationCircuit::prove(&public, &private).unwrap();
        assert!(NonRevocationCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    fn test_default_accumulator_type_is_merkle() {
        assert_eq!(AccumulatorType::default(), AccumulatorType::Merkle);
    }
}
