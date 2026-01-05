// Allow unused_assignments from Zeroize derive macro generated code
#![allow(unused_assignments)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

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
//! - **Accumulators**: RSA (legacy) and Merkle (PQ-safe) for revocation checking
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
//! // Check proof capability first
//! let capability = icn_zkp::proof_capability();
//! if !capability.can_prove() {
//!     panic!("ZKP proving not available in this build");
//! }
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
//! - `simulated` - Enable simulated proofs for testing (NO cryptographic security!)
//!
//! ## Security Warning
//!
//! **Default builds (no features) will return errors when attempting to generate proofs.**
//!
//! - For production: Enable the `stark` feature for real cryptographic proofs
//! - For testing only: Enable the `simulated` feature (NEVER in production!)
//!
//! Use [`proof_capability()`] to check what this build supports.

pub mod accumulator;
pub mod circuit;
pub mod merkle_accumulator;
pub mod prover;
#[cfg(feature = "stark")]
pub mod stark;
pub mod types;
pub mod verifier;

// Re-exports for convenience
pub use accumulator::{AccumulatorError, MembershipWitness, NonMembershipWitness, RsaAccumulator};
pub use merkle_accumulator::{
    MerkleAccumulator, MerkleAccumulatorError, MerkleMembershipProof, MerkleNonMembershipProof,
};
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

// ============================================================================
// Proof Capability Checking
// ============================================================================

/// Describes the proof capabilities of this build
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofCapability {
    /// Full STARK proofs available (production-ready)
    Stark,
    /// Simulated proofs only (NO cryptographic security - testing only!)
    Simulated,
    /// No proof capability (default build)
    None,
}

impl ProofCapability {
    /// Check if this build can generate proofs
    pub fn can_prove(&self) -> bool {
        matches!(self, ProofCapability::Stark | ProofCapability::Simulated)
    }

    /// Check if proofs from this build are cryptographically secure
    pub fn is_secure(&self) -> bool {
        matches!(self, ProofCapability::Stark)
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            ProofCapability::Stark => "STARK proofs (cryptographically secure)",
            ProofCapability::Simulated => "SIMULATED proofs (NO SECURITY - testing only!)",
            ProofCapability::None => "No proof capability (enable 'stark' or 'simulated' feature)",
        }
    }
}

/// Get the proof capability of this build
///
/// Use this to check what kind of proofs this build can generate before
/// attempting to create proofs.
///
/// # Example
///
/// ```rust,no_run
/// use icn_zkp::{proof_capability, ProofCapability};
///
/// let cap = proof_capability();
/// match cap {
///     ProofCapability::Stark => println!("Production-ready STARK proofs"),
///     ProofCapability::Simulated => eprintln!("WARNING: Using simulated proofs!"),
///     ProofCapability::None => panic!("No proof capability in this build"),
/// }
/// ```
#[must_use]
pub const fn proof_capability() -> ProofCapability {
    #[cfg(feature = "stark")]
    {
        ProofCapability::Stark
    }
    #[cfg(all(not(feature = "stark"), feature = "simulated"))]
    {
        ProofCapability::Simulated
    }
    #[cfg(all(not(feature = "stark"), not(feature = "simulated")))]
    {
        ProofCapability::None
    }
}

/// Check if simulated proofs are being used
///
/// This is a compile-time check that can be used to prevent accidental
/// use of simulated proofs in release builds.
///
/// # Example
///
/// ```rust,ignore
/// #[cfg(not(debug_assertions))]
/// compile_error!("Simulated proofs detected in release build!");
/// ```
#[must_use]
pub const fn is_simulated() -> bool {
    #[cfg(all(not(feature = "stark"), feature = "simulated"))]
    {
        true
    }
    #[cfg(any(feature = "stark", not(feature = "simulated")))]
    {
        false
    }
}

// ============================================================================
// Compound Proof API
// ============================================================================

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
    #[cfg(feature = "simulated")]
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
    #[cfg(feature = "simulated")]
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
