//! ICN Zero-Knowledge Proofs
//!
//! Zero-knowledge proof system for SDIS (Sovereign Digital Identity System).
//!
//! # Overview
//!
//! This crate provides:
//! - **Attribute proofs**: Prove properties without revealing data (age, citizenship, membership)
//! - **Non-revocation proofs**: Prove credential has not been revoked
//! - **Compound proofs**: Combine attribute + non-revocation
//! - **RSA accumulator**: For efficient revocation checking
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     ZKP System                                   │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌──────────────────────────────────────────────────────────┐   │
//! │  │                    High-Level API                         │   │
//! │  │  ┌───────────────┐              ┌────────────────────┐   │   │
//! │  │  │   ZkProver    │              │    ZkVerifier      │   │   │
//! │  │  │ prove_age()   │              │   verify_age()     │   │   │
//! │  │  │ prove_cit()   │              │   verify_cit()     │   │   │
//! │  │  │ prove_mem()   │              │   verify_mem()     │   │   │
//! │  │  │ prove_nr()    │              │   verify_nr()      │   │   │
//! │  │  └───────┬───────┘              └─────────┬──────────┘   │   │
//! │  └──────────┼────────────────────────────────┼──────────────┘   │
//! │             │                                │                   │
//! │  ┌──────────▼────────────────────────────────▼──────────────┐   │
//! │  │                     Circuits                              │   │
//! │  │  ┌─────────┐ ┌─────────────┐ ┌──────────┐ ┌───────────┐  │   │
//! │  │  │  Age    │ │ Citizenship │ │Membership│ │NonRevoke  │  │   │
//! │  │  │ Circuit │ │   Circuit   │ │ Circuit  │ │ Circuit   │  │   │
//! │  │  └─────────┘ └─────────────┘ └──────────┘ └───────────┘  │   │
//! │  └──────────────────────────────────────────────────────────┘   │
//! │                                                                  │
//! │  ┌──────────────────────────────────────────────────────────┐   │
//! │  │                   Accumulator                             │   │
//! │  │                 (Revocation List)                         │   │
//! │  └──────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use icn_zkp::{ZkProver, ZkVerifier, ProofContext, AgeAttestation};
//!
//! // Prover side
//! let prover = ZkProver::new();
//! let context = ProofContext::new(None);
//! let attestation = AgeAttestation {
//!     birthdate_days: 7300, // ~20 years ago
//!     signature: vec![0u8; 64],
//! };
//!
//! let proof = prover.prove_age(18, &attestation, issuer_pk, &context)?;
//!
//! // Verifier side
//! let mut verifier = ZkVerifier::new();
//! let result = verifier.verify_age(18, &proof, issuer_pk, &context)?;
//! assert!(result.valid);
//! ```
//!
//! # Features
//!
//! - `stark` - Enable full STARK proving with winterfell (adds ~10MB binary size)
//!
//! Without the `stark` feature, proofs are simulated for testing purposes.

pub mod accumulator;
pub mod circuit;
pub mod prover;
pub mod types;
pub mod verifier;

// Re-exports for convenience
pub use accumulator::{AccumulatorError, MembershipWitness, NonMembershipWitness, RsaAccumulator};
pub use circuit::{
    AgeProofCircuit, AgeProofPrivate, AgeProofPublic, Circuit, CircuitError,
    CitizenshipProofCircuit, CitizenshipProofPrivate, CitizenshipProofPublic,
    MembershipProofCircuit, MembershipProofPrivate, MembershipProofPublic, NonRevocationCircuit,
    NonRevocationPrivate, NonRevocationPublic,
};
pub use prover::{ProverError, ZkProver};
pub use types::{
    AgeAttestation, Attestation, AttributeProofRequest, CitizenshipAttestation, CitizenshipStatus,
    CompoundProof, MembershipAttestation, ProofContext, ProofType, StarkProof, VerificationResult,
};
pub use verifier::{VerifierError, ZkVerifier};

/// Generate a compound proof combining attribute proof with non-revocation
///
/// This is the main entry point for generating proofs that can be verified
/// to prove both an attribute and that the credential is not revoked.
pub fn generate_compound_proof(
    proof_type: ProofType,
    attestation: &Attestation,
    credential_id: [u8; 32],
    accumulator: &RsaAccumulator,
    revoked_credentials: &[[u8; 32]],
) -> Result<CompoundProof, ProverError> {
    let prover = ZkProver::new();
    prover.prove_compound(
        proof_type,
        attestation,
        credential_id,
        accumulator,
        revoked_credentials,
    )
}

/// Verify a compound proof
pub fn verify_compound_proof(
    proof: &CompoundProof,
    accumulator: &RsaAccumulator,
    issuer_pk: [u8; 32],
) -> Result<VerificationResult, VerifierError> {
    let mut verifier = ZkVerifier::new();
    verifier.verify_compound(proof, accumulator, issuer_pk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_age_proof_flow() {
        // Setup
        let prover = ZkProver::new();
        let mut verifier = ZkVerifier::new();

        let issuer_pk = [1u8; 32];
        let context = ProofContext::new(None);

        // Create attestation for someone 25 years old
        let current_days = (context.timestamp / 86400) as u32;
        let birthdate_days = current_days - (25 * 365);

        let attestation = AgeAttestation {
            birthdate_days,
            signature: vec![0u8; 64],
        };

        // Generate proof for 21+ age check
        let proof = prover
            .prove_age(21, &attestation, issuer_pk, &context)
            .expect("proof generation should succeed");

        // Verify proof
        let result = verifier
            .verify_age(21, &proof, issuer_pk, &context)
            .expect("verification should not error");

        assert!(result.valid, "proof should be valid");
    }

    #[test]
    fn test_age_proof_fails_for_underage() {
        let prover = ZkProver::new();

        let issuer_pk = [1u8; 32];
        let context = ProofContext::new(None);

        // Create attestation for someone 17 years old
        let current_days = (context.timestamp / 86400) as u32;
        let birthdate_days = current_days - (17 * 365);

        let attestation = AgeAttestation {
            birthdate_days,
            signature: vec![0u8; 64],
        };

        // Try to generate proof for 18+ age check
        let result = prover.prove_age(18, &attestation, issuer_pk, &context);

        assert!(result.is_err(), "proof should fail for underage person");
    }

    #[test]
    fn test_non_revocation_flow() {
        let prover = ZkProver::new();
        let mut verifier = ZkVerifier::new();

        let issuer_pk = [1u8; 32];
        let context = ProofContext::new(None);

        // Create accumulator with some revoked credentials
        let mut accumulator = RsaAccumulator::new_test();
        let revoked1 = [1u8; 32];
        let revoked2 = [2u8; 32];
        accumulator.add(&revoked1);
        accumulator.add(&revoked2);

        let revoked_list = vec![revoked1, revoked2];

        // Test credential that is NOT revoked
        let valid_credential = [3u8; 32];
        let proof = prover
            .prove_non_revocation(
                valid_credential,
                &accumulator,
                &revoked_list,
                issuer_pk,
                &context,
            )
            .expect("non-revocation proof should succeed");

        let result = verifier
            .verify_non_revocation(&proof, &accumulator, issuer_pk, &context)
            .expect("verification should not error");

        assert!(result.valid, "non-revoked credential should verify");
    }

    #[test]
    fn test_accumulator_membership() {
        let mut acc = RsaAccumulator::new_test();

        let elem1 = [1u8; 32];
        let elem2 = [2u8; 32];
        let not_member = [3u8; 32];

        acc.add(&elem1);
        acc.add(&elem2);

        let elements = vec![elem1, elem2];

        // Membership witness for elem1
        let witness = acc
            .membership_witness(&elem1, &elements)
            .expect("should create witness");
        assert!(acc.verify_membership(&elem1, &witness));

        // Non-membership witness for elem3
        let nm_witness = acc
            .non_membership_witness(&not_member, &elements)
            .expect("should create witness");
        assert!(acc.verify_non_membership(&not_member, &nm_witness));
    }

    #[test]
    fn test_proof_type_descriptions() {
        assert!(ProofType::AgeAtLeast { threshold: 18 }
            .description()
            .contains("18"));
        assert!(ProofType::Citizenship {
            country_code: [b'U', b'S']
        }
        .description()
        .contains("US"));
        assert!(ProofType::NonRevocation.description().contains("revoked"));
    }
}
