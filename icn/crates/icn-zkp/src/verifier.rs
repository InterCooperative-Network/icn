//! ZK Verifier
//!
//! High-level verifier interface for validating zero-knowledge proofs.

use crate::accumulator::RsaAccumulator;
use crate::circuit::{
    AgeProofCircuit, AgeProofPublic, Circuit, CircuitError, CitizenshipProofCircuit,
    CitizenshipProofPublic, MembershipProofCircuit, MembershipProofPublic, NonRevocationCircuit,
    NonRevocationPublic,
};
use crate::types::{CompoundProof, ProofContext, ProofType, StarkProof, VerificationResult};
use icn_identity::Did;
use thiserror::Error;

/// Errors from proof verification
#[derive(Debug, Error)]
pub enum VerifierError {
    #[error("Circuit error: {0}")]
    Circuit(#[from] CircuitError),

    #[error("Proof expired")]
    ProofExpired,

    #[error("Invalid nonce")]
    InvalidNonce,

    #[error("Untrusted issuer")]
    UntrustedIssuer,

    #[error("Invalid proof structure")]
    InvalidStructure,

    #[error("Context mismatch")]
    ContextMismatch,
}

/// ZK verifier for validating attribute proofs
pub struct ZkVerifier {
    /// Trusted issuer public keys
    trusted_issuers: Vec<[u8; 32]>,
    /// Maximum proof age in seconds
    max_proof_age: u64,
    /// Used nonces (for replay protection)
    used_nonces: Vec<[u8; 16]>,
}

impl Default for ZkVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkVerifier {
    /// Create a new verifier
    pub fn new() -> Self {
        Self {
            trusted_issuers: Vec::new(),
            max_proof_age: 3600, // 1 hour default
            used_nonces: Vec::new(),
        }
    }

    /// Set maximum proof age
    pub fn with_max_age(mut self, seconds: u64) -> Self {
        self.max_proof_age = seconds;
        self
    }

    /// Add a trusted issuer
    pub fn add_trusted_issuer(&mut self, pk: [u8; 32]) {
        if !self.trusted_issuers.contains(&pk) {
            self.trusted_issuers.push(pk);
        }
    }

    /// Check if issuer is trusted
    pub fn is_trusted_issuer(&self, pk: &[u8; 32]) -> bool {
        self.trusted_issuers.is_empty() || self.trusted_issuers.contains(pk)
    }

    /// Check and record nonce (replay protection)
    fn check_nonce(&mut self, nonce: &[u8; 16]) -> Result<(), VerifierError> {
        if self.used_nonces.contains(nonce) {
            return Err(VerifierError::InvalidNonce);
        }
        self.used_nonces.push(*nonce);

        // Keep nonce list bounded (last 10000)
        if self.used_nonces.len() > 10000 {
            self.used_nonces.remove(0);
        }

        Ok(())
    }

    /// Verify an age proof
    pub fn verify_age(
        &mut self,
        threshold: u8,
        proof: &StarkProof,
        issuer_pk: [u8; 32],
        context: &ProofContext,
    ) -> Result<VerificationResult, VerifierError> {
        // Check context validity
        if !context.is_valid(self.max_proof_age) {
            return Err(VerifierError::ProofExpired);
        }

        // Check issuer trust
        if !self.is_trusted_issuer(&issuer_pk) {
            return Err(VerifierError::UntrustedIssuer);
        }

        // Check nonce
        self.check_nonce(&context.nonce)?;

        let public = AgeProofPublic {
            threshold,
            current_date: (context.timestamp / 86400) as u32,
            issuer_pk,
            nonce: context.nonce,
        };

        let valid = AgeProofCircuit::verify(&public, proof)?;

        Ok(if valid {
            VerificationResult::success(ProofType::AgeAtLeast { threshold })
        } else {
            VerificationResult::failure(ProofType::AgeAtLeast { threshold })
        })
    }

    /// Verify a citizenship proof
    pub fn verify_citizenship(
        &mut self,
        country_code: [u8; 2],
        proof: &StarkProof,
        issuer_pk: [u8; 32],
        context: &ProofContext,
    ) -> Result<VerificationResult, VerifierError> {
        use crate::circuit::citizenship::StatusType;

        if !context.is_valid(self.max_proof_age) {
            return Err(VerifierError::ProofExpired);
        }

        if !self.is_trusted_issuer(&issuer_pk) {
            return Err(VerifierError::UntrustedIssuer);
        }

        self.check_nonce(&context.nonce)?;

        let public = CitizenshipProofPublic {
            country_code,
            minimum_status: StatusType::Citizen,
            issuer_pk,
            nonce: context.nonce,
        };

        let valid = CitizenshipProofCircuit::verify(&public, proof)?;

        Ok(if valid {
            VerificationResult::success(ProofType::Citizenship { country_code })
        } else {
            VerificationResult::failure(ProofType::Citizenship { country_code })
        })
    }

    /// Verify a membership proof
    pub fn verify_membership(
        &mut self,
        org_did: Did,
        proof: &StarkProof,
        context: &ProofContext,
    ) -> Result<VerificationResult, VerifierError> {
        if !context.is_valid(self.max_proof_age) {
            return Err(VerifierError::ProofExpired);
        }

        self.check_nonce(&context.nonce)?;

        let public = MembershipProofPublic {
            org_did: org_did.clone(),
            required_role: None,
            current_time: context.timestamp,
            nonce: context.nonce,
        };

        let valid = MembershipProofCircuit::verify(&public, proof)?;

        Ok(if valid {
            VerificationResult::success(ProofType::Membership { org_did })
        } else {
            VerificationResult::failure(ProofType::Membership { org_did })
        })
    }

    /// Verify a non-revocation proof
    pub fn verify_non_revocation(
        &mut self,
        proof: &StarkProof,
        accumulator: &RsaAccumulator,
        issuer_pk: [u8; 32],
        context: &ProofContext,
    ) -> Result<VerificationResult, VerifierError> {
        if !context.is_valid(self.max_proof_age) {
            return Err(VerifierError::ProofExpired);
        }

        if !self.is_trusted_issuer(&issuer_pk) {
            return Err(VerifierError::UntrustedIssuer);
        }

        self.check_nonce(&context.nonce)?;

        let public = NonRevocationPublic {
            accumulator_value: accumulator.value_hash(),
            accumulator_epoch: accumulator.epoch(),
            issuer_pk,
            nonce: context.nonce,
        };

        let valid = NonRevocationCircuit::verify(&public, proof)?;

        Ok(if valid {
            VerificationResult::success(ProofType::NonRevocation)
        } else {
            VerificationResult::failure(ProofType::NonRevocation)
        })
    }

    /// Verify a compound proof
    pub fn verify_compound(
        &mut self,
        proof: &CompoundProof,
        accumulator: &RsaAccumulator,
        issuer_pk: [u8; 32],
    ) -> Result<VerificationResult, VerifierError> {
        if !proof.context.is_valid(self.max_proof_age) {
            return Err(VerifierError::ProofExpired);
        }

        if !self.is_trusted_issuer(&issuer_pk) {
            return Err(VerifierError::UntrustedIssuer);
        }

        // Verify attribute proof
        let attr_valid = match &proof.proof_type {
            ProofType::AgeAtLeast { threshold } => {
                let public = AgeProofPublic {
                    threshold: *threshold,
                    current_date: (proof.context.timestamp / 86400) as u32,
                    issuer_pk,
                    nonce: proof.context.nonce,
                };
                AgeProofCircuit::verify(&public, &proof.attribute_proof)?
            }
            ProofType::Citizenship { country_code } => {
                use crate::circuit::citizenship::StatusType;
                let public = CitizenshipProofPublic {
                    country_code: *country_code,
                    minimum_status: StatusType::Citizen,
                    issuer_pk,
                    nonce: proof.context.nonce,
                };
                CitizenshipProofCircuit::verify(&public, &proof.attribute_proof)?
            }
            ProofType::Membership { org_did } => {
                let public = MembershipProofPublic {
                    org_did: org_did.clone(),
                    required_role: None,
                    current_time: proof.context.timestamp,
                    nonce: proof.context.nonce,
                };
                MembershipProofCircuit::verify(&public, &proof.attribute_proof)?
            }
            ProofType::NonRevocation => true, // Handled below
            ProofType::Custom { .. } => {
                return Err(VerifierError::InvalidStructure);
            }
        };

        if !attr_valid {
            return Ok(VerificationResult::failure(proof.proof_type.clone()));
        }

        // Verify non-revocation proof if present
        if let Some(ref nr_proof) = proof.non_revocation_proof {
            let public = NonRevocationPublic {
                accumulator_value: accumulator.value_hash(),
                accumulator_epoch: accumulator.epoch(),
                issuer_pk,
                nonce: proof.context.nonce,
            };

            let nr_valid = NonRevocationCircuit::verify(&public, nr_proof)?;
            if !nr_valid {
                return Ok(VerificationResult::failure(proof.proof_type.clone())
                    .with_warning("non-revocation proof invalid"));
            }
        }

        Ok(VerificationResult::success(proof.proof_type.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prover::ZkProver;
    use crate::types::AgeAttestation;

    #[test]
    fn test_verifier_creation() {
        let verifier = ZkVerifier::new();
        assert!(verifier.trusted_issuers.is_empty());
        assert_eq!(verifier.max_proof_age, 3600);
    }

    #[test]
    #[cfg(not(feature = "stark"))]
    fn test_age_proof_verification() {
        let prover = ZkProver::new();
        let mut verifier = ZkVerifier::new();

        let context = ProofContext::new(None);
        let issuer_pk = [1u8; 32];

        // Create age attestation for someone 20 years old
        let birthdate_days = ((context.timestamp / 86400) as u32) - (20 * 365);
        let attestation = AgeAttestation {
            birthdate_days,
            signature: vec![0u8; 64],
        };

        // Generate proof
        let proof = prover
            .prove_age(18, &attestation, issuer_pk, &context)
            .unwrap();

        // Verify proof
        let result = verifier
            .verify_age(18, &proof, issuer_pk, &context)
            .unwrap();
        assert!(result.valid);
    }

    #[test]
    #[cfg(not(feature = "stark"))]
    fn test_nonce_replay_protection() {
        let prover = ZkProver::new();
        let mut verifier = ZkVerifier::new();

        let context = ProofContext::new(None);
        let issuer_pk = [1u8; 32];

        let birthdate_days = ((context.timestamp / 86400) as u32) - (20 * 365);
        let attestation = AgeAttestation {
            birthdate_days,
            signature: vec![0u8; 64],
        };

        let proof = prover
            .prove_age(18, &attestation, issuer_pk, &context)
            .unwrap();

        // First verification succeeds
        let result = verifier
            .verify_age(18, &proof, issuer_pk, &context)
            .unwrap();
        assert!(result.valid);

        // Second verification with same nonce fails
        let result2 = verifier.verify_age(18, &proof, issuer_pk, &context);
        assert!(matches!(result2, Err(VerifierError::InvalidNonce)));
    }

    #[test]
    #[cfg(not(feature = "stark"))]
    fn test_untrusted_issuer() {
        let prover = ZkProver::new();
        let mut verifier = ZkVerifier::new();

        // Add a different trusted issuer
        verifier.add_trusted_issuer([99u8; 32]);

        let context = ProofContext::new(None);
        let issuer_pk = [1u8; 32]; // Not trusted

        let birthdate_days = ((context.timestamp / 86400) as u32) - (20 * 365);
        let attestation = AgeAttestation {
            birthdate_days,
            signature: vec![0u8; 64],
        };

        let proof = prover
            .prove_age(18, &attestation, issuer_pk, &context)
            .unwrap();

        let result = verifier.verify_age(18, &proof, issuer_pk, &context);
        assert!(matches!(result, Err(VerifierError::UntrustedIssuer)));
    }
}
