//! Non-Revocation Proof Circuit
//!
//! Proves that a credential has not been revoked using accumulator-based proofs.

use super::{compute_public_inputs_hash, Circuit, CircuitError};
use crate::types::StarkProof;
use serde::{Deserialize, Serialize};

/// Public inputs for non-revocation proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonRevocationPublic {
    /// Current accumulator value
    pub accumulator_value: [u8; 32],
    /// Accumulator epoch/version
    pub accumulator_epoch: u64,
    /// Issuer's public key
    pub issuer_pk: [u8; 32],
    /// Proof context nonce
    pub nonce: [u8; 16],
}

impl NonRevocationPublic {
    /// Create new public inputs
    pub fn new(accumulator_value: [u8; 32], accumulator_epoch: u64, issuer_pk: [u8; 32]) -> Self {
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        Self {
            accumulator_value,
            accumulator_epoch,
            issuer_pk,
            nonce,
        }
    }

    /// Validate public inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        // Accumulator value should not be all zeros
        if self.accumulator_value == [0u8; 32] {
            return Err(CircuitError::InvalidPublicInput(
                "accumulator value cannot be zero".into(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NonMembershipWitness {
    /// Bezout coefficient a (for ax + by = 1)
    pub a: Vec<u8>,
    /// Bezout coefficient b
    pub b: Vec<u8>,
    /// Auxiliary accumulator value
    pub aux: [u8; 32],
}

impl NonMembershipWitness {
    /// Create a simple witness (for testing)
    pub fn simple(credential_id: &[u8; 32]) -> Self {
        Self {
            a: vec![1u8; 32],
            b: vec![1u8; 32],
            aux: *credential_id,
        }
    }
}

impl NonRevocationPrivate {
    /// Validate private inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        if self.witness.a.is_empty() || self.witness.b.is_empty() {
            return Err(CircuitError::InvalidPrivateInput(
                "witness coefficients cannot be empty".into(),
            ));
        }
        Ok(())
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

    fn prove(public: &Self::Public, private: &Self::Private) -> Result<StarkProof, CircuitError> {
        public.validate()?;
        private.validate()?;

        // In a real implementation, we would verify:
        // 1. The Bezout coefficients prove non-membership
        // 2. The witness is valid for the current accumulator value
        //
        // For now, we simulate the proof generation

        // Build the proof based on enabled features
        let public_hash = compute_public_inputs_hash(public);

        #[cfg(feature = "stark")]
        {
            // TODO: Actual STARK proof generation with winterfell
            let _ = public_hash;
            unimplemented!("Full STARK proofs require 'stark' feature implementation");
        }

        #[cfg(all(not(feature = "stark"), feature = "simulated"))]
        {
            // SIMULATED proof for testing ONLY
            // WARNING: This provides NO cryptographic security!
            use sha3::{Digest, Sha3_256};

            tracing::warn!(
                "Creating SIMULATED non-revocation proof - NO CRYPTOGRAPHIC SECURITY"
            );

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
                 or 'simulated' feature for testing only.".into()
            ))
        }
    }

    fn verify(public: &Self::Public, proof: &StarkProof) -> Result<bool, CircuitError> {
        public.validate()?;

        let expected_hash = compute_public_inputs_hash(public);
        if proof.public_inputs_hash != expected_hash {
            return Ok(false);
        }

        #[cfg(feature = "stark")]
        {
            // TODO: Actual STARK verification with winterfell
            unimplemented!("Full STARK verification requires 'stark' feature implementation");
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
                 or 'simulated' feature for testing only.".into()
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
    fn test_zero_accumulator_invalid() {
        let issuer_pk = [1u8; 32];
        let accumulator = [0u8; 32]; // Invalid

        let public = NonRevocationPublic::new(accumulator, 1, issuer_pk);
        assert!(public.validate().is_err());
    }
}
