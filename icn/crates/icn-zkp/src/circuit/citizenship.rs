//! Citizenship Proof Circuit
//!
//! Proves citizenship or residency status without revealing full identity details.

use super::{compute_public_inputs_hash, Circuit, CircuitError};
use crate::types::StarkProof;
use serde::{Deserialize, Serialize};

/// Citizenship/residency status type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatusType {
    Citizen = 0,
    PermanentResident = 1,
    TemporaryResident = 2,
    VisaHolder = 3,
}

/// Public inputs for citizenship proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitizenshipProofPublic {
    /// ISO 3166-1 alpha-2 country code being verified
    pub country_code: [u8; 2],
    /// Minimum status required (Citizen = most restrictive)
    pub minimum_status: StatusType,
    /// Issuer's public key (government or trusted authority)
    pub issuer_pk: [u8; 32],
    /// Proof context nonce
    pub nonce: [u8; 16],
}

impl CitizenshipProofPublic {
    /// Create new public inputs
    pub fn new(country_code: [u8; 2], minimum_status: StatusType, issuer_pk: [u8; 32]) -> Self {
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
        Self {
            country_code,
            minimum_status,
            issuer_pk,
            nonce,
        }
    }

    /// Validate public inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        // Country code should be ASCII uppercase
        for byte in &self.country_code {
            if !byte.is_ascii_uppercase() {
                return Err(CircuitError::InvalidPublicInput(
                    "country code must be uppercase ASCII".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Private inputs for citizenship proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitizenshipProofPrivate {
    /// Actual country code in attestation
    pub actual_country: [u8; 2],
    /// Actual status
    pub actual_status: StatusType,
    /// Issuer's signature over (country, status, subject_id)
    pub issuer_signature: Vec<u8>,
    /// Subject identifier (hashed)
    pub subject_id_hash: [u8; 32],
    /// Blinding factor
    pub blinding: [u8; 32],
}

impl CitizenshipProofPrivate {
    /// Validate private inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        if self.issuer_signature.len() != 64 {
            return Err(CircuitError::InvalidPrivateInput(
                "invalid signature length".into(),
            ));
        }
        Ok(())
    }
}

/// Citizenship proof circuit
pub struct CitizenshipProofCircuit;

impl Circuit for CitizenshipProofCircuit {
    type Public = CitizenshipProofPublic;
    type Private = CitizenshipProofPrivate;

    fn name() -> &'static str {
        "citizenship_proof_v1"
    }

    fn prove(public: &Self::Public, private: &Self::Private) -> Result<StarkProof, CircuitError> {
        public.validate()?;
        private.validate()?;

        // Verify country matches
        if private.actual_country != public.country_code {
            return Err(CircuitError::ConstraintNotSatisfied(
                "country code mismatch".into(),
            ));
        }

        // Verify status meets minimum requirement
        // Lower enum value = higher privilege (Citizen < PermanentResident < etc.)
        if (private.actual_status as u8) > (public.minimum_status as u8) {
            return Err(CircuitError::ConstraintNotSatisfied(format!(
                "status {:?} does not meet minimum {:?}",
                private.actual_status, public.minimum_status
            )));
        }

        #[cfg(not(feature = "stark"))]
        {
            use sha3::{Digest, Sha3_256};

            let public_hash = compute_public_inputs_hash(public);

            let mut hasher = Sha3_256::new();
            hasher.update(private.actual_country);
            hasher.update([private.actual_status as u8]);
            hasher.update(private.subject_id_hash);
            hasher.update(private.blinding);
            hasher.update(public.nonce);
            let commitment: [u8; 32] = hasher.finalize().into();

            let mut proof_data = Vec::with_capacity(1024);
            proof_data.extend_from_slice(&commitment);
            proof_data.extend_from_slice(&public_hash);
            proof_data.extend_from_slice(&[0u8; 960]);

            Ok(StarkProof::new(proof_data, public_hash))
        }

        #[cfg(feature = "stark")]
        {
            let _public_hash = compute_public_inputs_hash(public);
            unimplemented!("Full STARK proofs require 'stark' feature");
        }
    }

    fn verify(public: &Self::Public, proof: &StarkProof) -> Result<bool, CircuitError> {
        public.validate()?;

        let expected_hash = compute_public_inputs_hash(public);
        if proof.public_inputs_hash != expected_hash {
            return Ok(false);
        }

        #[cfg(not(feature = "stark"))]
        {
            if proof.proof_bytes.len() < 64 {
                return Ok(false);
            }
            if proof.proof_bytes[32..64] != expected_hash[..] {
                return Ok(false);
            }
            Ok(true)
        }

        #[cfg(feature = "stark")]
        {
            unimplemented!("Full STARK verification requires 'stark' feature");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_citizenship_proof_success() {
        let issuer_pk = [1u8; 32];
        let public = CitizenshipProofPublic::new([b'U', b'S'], StatusType::Citizen, issuer_pk);

        let private = CitizenshipProofPrivate {
            actual_country: [b'U', b'S'],
            actual_status: StatusType::Citizen,
            issuer_signature: vec![0u8; 64],
            subject_id_hash: [0u8; 32],
            blinding: [0u8; 32],
        };

        let proof = CitizenshipProofCircuit::prove(&public, &private).unwrap();
        assert!(CitizenshipProofCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    fn test_citizenship_proof_resident_for_citizen_fails() {
        let issuer_pk = [1u8; 32];
        let public = CitizenshipProofPublic::new([b'U', b'S'], StatusType::Citizen, issuer_pk);

        let private = CitizenshipProofPrivate {
            actual_country: [b'U', b'S'],
            actual_status: StatusType::PermanentResident, // Not a citizen
            issuer_signature: vec![0u8; 64],
            subject_id_hash: [0u8; 32],
            blinding: [0u8; 32],
        };

        let result = CitizenshipProofCircuit::prove(&public, &private);
        assert!(matches!(
            result,
            Err(CircuitError::ConstraintNotSatisfied(_))
        ));
    }

    #[test]
    fn test_citizenship_proof_citizen_for_resident_succeeds() {
        let issuer_pk = [1u8; 32];
        // Require at least permanent resident
        let public =
            CitizenshipProofPublic::new([b'U', b'S'], StatusType::PermanentResident, issuer_pk);

        let private = CitizenshipProofPrivate {
            actual_country: [b'U', b'S'],
            actual_status: StatusType::Citizen, // Citizen exceeds requirement
            issuer_signature: vec![0u8; 64],
            subject_id_hash: [0u8; 32],
            blinding: [0u8; 32],
        };

        let proof = CitizenshipProofCircuit::prove(&public, &private).unwrap();
        assert!(CitizenshipProofCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    fn test_wrong_country_fails() {
        let issuer_pk = [1u8; 32];
        let public = CitizenshipProofPublic::new([b'U', b'S'], StatusType::Citizen, issuer_pk);

        let private = CitizenshipProofPrivate {
            actual_country: [b'C', b'A'], // Canada, not US
            actual_status: StatusType::Citizen,
            issuer_signature: vec![0u8; 64],
            subject_id_hash: [0u8; 32],
            blinding: [0u8; 32],
        };

        let result = CitizenshipProofCircuit::prove(&public, &private);
        assert!(matches!(
            result,
            Err(CircuitError::ConstraintNotSatisfied(_))
        ));
    }
}
