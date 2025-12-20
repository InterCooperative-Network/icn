//! ZKP Circuit Definitions
//!
//! This module contains circuit definitions for various attribute proofs.
//! Circuits define the computation to be proven in zero-knowledge.

pub mod age;
pub mod citizenship;
pub mod membership;
pub mod non_revocation;

pub use age::{AgeProofCircuit, AgeProofPrivate, AgeProofPublic};
pub use citizenship::{CitizenshipProofCircuit, CitizenshipProofPrivate, CitizenshipProofPublic};
pub use membership::{MembershipProofCircuit, MembershipProofPrivate, MembershipProofPublic};
pub use non_revocation::{NonRevocationCircuit, NonRevocationPrivate, NonRevocationPublic};

use crate::types::StarkProof;
use thiserror::Error;

/// Errors that can occur during circuit operations
#[derive(Debug, Error)]
pub enum CircuitError {
    #[error("Invalid public input: {0}")]
    InvalidPublicInput(String),

    #[error("Invalid private input: {0}")]
    InvalidPrivateInput(String),

    #[error("Proof generation failed: {0}")]
    ProofGenerationFailed(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("Signature verification failed")]
    SignatureInvalid,

    #[error("Constraint not satisfied: {0}")]
    ConstraintNotSatisfied(String),
}

/// Trait for ZK circuits
pub trait Circuit {
    /// Public inputs (known to verifier)
    type Public;
    /// Private inputs (known only to prover)
    type Private;

    /// Name of the circuit for identification
    fn name() -> &'static str;

    /// Generate a proof
    fn prove(public: &Self::Public, private: &Self::Private) -> Result<StarkProof, CircuitError>;

    /// Verify a proof
    fn verify(public: &Self::Public, proof: &StarkProof) -> Result<bool, CircuitError>;
}

/// Helper to compute SHA3-256 hash
pub(crate) fn sha3_256(data: &[u8]) -> [u8; 32] {
    use sha3::{Digest, Sha3_256};
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Helper to compute public inputs hash from structured data
pub(crate) fn compute_public_inputs_hash<T: serde::Serialize>(public: &T) -> [u8; 32] {
    let bytes =
        bincode::serde::encode_to_vec(public, bincode::config::legacy()).unwrap_or_default();
    sha3_256(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha3_256() {
        let hash = sha3_256(b"hello world");
        assert_eq!(hash.len(), 32);
        // SHA3-256 of "hello world"
        assert!(hash[0] != 0 || hash[1] != 0); // Non-zero result
    }

    #[test]
    fn test_public_inputs_hash_deterministic() {
        #[derive(serde::Serialize)]
        struct TestInput {
            value: u32,
        }

        let input = TestInput { value: 42 };
        let hash1 = compute_public_inputs_hash(&input);
        let hash2 = compute_public_inputs_hash(&input);
        assert_eq!(hash1, hash2);
    }
}
