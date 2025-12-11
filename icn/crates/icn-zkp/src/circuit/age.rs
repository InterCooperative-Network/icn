//! Age Proof Circuit
//!
//! Proves that a person is at least a certain age without revealing their exact birthdate.
//!
//! The circuit proves:
//! 1. The issuer's signature over the birthdate is valid
//! 2. current_date - birthdate >= threshold * 365 days

use super::{compute_public_inputs_hash, Circuit, CircuitError};
use crate::types::StarkProof;
use serde::{Deserialize, Serialize};

/// Days in a year (approximate, for age calculation)
const DAYS_PER_YEAR: u32 = 365;

/// Public inputs for age proof (visible to verifier)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeProofPublic {
    /// Minimum age threshold in years
    pub threshold: u8,
    /// Current date as days since Unix epoch
    pub current_date: u32,
    /// Issuer's public key
    pub issuer_pk: [u8; 32],
    /// Proof context nonce
    pub nonce: [u8; 16],
}

impl AgeProofPublic {
    /// Create with current date
    pub fn new(threshold: u8, issuer_pk: [u8; 32]) -> Self {
        use rand::RngCore;
        let mut nonce = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce);

        // Calculate current date as days since epoch
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let current_date = (now.as_secs() / 86400) as u32;

        Self {
            threshold,
            current_date,
            issuer_pk,
            nonce,
        }
    }

    /// Validate public inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        if self.threshold == 0 {
            return Err(CircuitError::InvalidPublicInput(
                "threshold must be positive".into(),
            ));
        }
        if self.threshold > 150 {
            return Err(CircuitError::InvalidPublicInput(
                "threshold unreasonably high".into(),
            ));
        }
        Ok(())
    }
}

/// Private inputs for age proof (known only to prover)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeProofPrivate {
    /// Birth date as days since Unix epoch
    pub birthdate_days: u32,
    /// Issuer's signature over the birthdate
    pub issuer_signature: Vec<u8>,
    /// Randomness for hiding
    pub blinding: [u8; 32],
}

impl AgeProofPrivate {
    /// Create new private inputs
    pub fn new(birthdate_days: u32, issuer_signature: Vec<u8>) -> Self {
        let mut blinding = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut blinding);
        Self {
            birthdate_days,
            issuer_signature,
            blinding,
        }
    }

    /// Validate private inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        if self.issuer_signature.is_empty() {
            return Err(CircuitError::InvalidPrivateInput(
                "signature cannot be empty".into(),
            ));
        }
        // Birthdate should be in a reasonable range (1900-present)
        let max_date = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86400) as u32;

        if self.birthdate_days > max_date {
            return Err(CircuitError::InvalidPrivateInput(
                "birthdate in the future".into(),
            ));
        }

        Ok(())
    }
}

/// Age proof circuit implementation
pub struct AgeProofCircuit;

impl Circuit for AgeProofCircuit {
    type Public = AgeProofPublic;
    type Private = AgeProofPrivate;

    fn name() -> &'static str {
        "age_proof_v1"
    }

    /// Generate a STARK proof that age >= threshold
    ///
    /// The circuit verifies:
    /// 1. issuer_signature is valid for (issuer_pk, birthdate_days)
    /// 2. (current_date - birthdate_days) >= threshold * 365
    fn prove(public: &Self::Public, private: &Self::Private) -> Result<StarkProof, CircuitError> {
        // Validate inputs
        public.validate()?;
        private.validate()?;

        // Check the age constraint
        let age_in_days = public
            .current_date
            .checked_sub(private.birthdate_days)
            .ok_or_else(|| CircuitError::ConstraintNotSatisfied("negative age".into()))?;

        let required_days = (public.threshold as u32) * DAYS_PER_YEAR;

        if age_in_days < required_days {
            return Err(CircuitError::ConstraintNotSatisfied(format!(
                "age {age_in_days} days < required {required_days} days"
            )));
        }

        // Verify the issuer's signature over the birthdate
        // In a real implementation, this would verify Ed25519 signature
        // For now, we check that signature exists and is properly sized
        if private.issuer_signature.len() != 64 {
            return Err(CircuitError::SignatureInvalid);
        }

        // Build the proof
        // In production, this would invoke winterfell STARK prover
        // For now, we create a simulated proof structure

        #[cfg(feature = "stark")]
        {
            // TODO: Actual STARK proof generation with winterfell
            let _public_hash = compute_public_inputs_hash(public);
            unimplemented!("Full STARK proofs require 'stark' feature");
        }

        #[cfg(not(feature = "stark"))]
        {
            // Simulated proof for testing
            // Contains: commitment to private data, public hash, simulated proof data
            use sha3::{Digest, Sha3_256};

            let public_hash = compute_public_inputs_hash(public);

            let mut hasher = Sha3_256::new();
            hasher.update(private.birthdate_days.to_le_bytes());
            hasher.update(private.blinding);
            hasher.update(public.nonce);
            let commitment: [u8; 32] = hasher.finalize().into();

            // Create a deterministic "proof" for testing
            let mut proof_data = Vec::with_capacity(1024);
            proof_data.extend_from_slice(&commitment);
            proof_data.extend_from_slice(&public_hash);
            proof_data.extend_from_slice(&[0u8; 960]); // Padding to realistic size

            Ok(StarkProof::new(proof_data, public_hash))
        }
    }

    /// Verify an age proof
    fn verify(public: &Self::Public, proof: &StarkProof) -> Result<bool, CircuitError> {
        // Validate public inputs
        public.validate()?;

        // Check public inputs hash matches
        let expected_hash = compute_public_inputs_hash(public);
        if proof.public_inputs_hash != expected_hash {
            return Ok(false);
        }

        #[cfg(feature = "stark")]
        {
            // TODO: Actual STARK verification with winterfell
            unimplemented!("Full STARK verification requires 'stark' feature");
        }

        #[cfg(not(feature = "stark"))]
        {
            // Simulated verification for testing
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
    }
}

impl AgeProofCircuit {
    /// Compute age in years from days
    pub fn days_to_years(days: u32) -> u32 {
        days / DAYS_PER_YEAR
    }

    /// Check if someone would pass an age check
    pub fn would_pass(birthdate_days: u32, threshold: u8, current_date: u32) -> bool {
        let age_in_days = current_date.saturating_sub(birthdate_days);
        let required_days = (threshold as u32) * DAYS_PER_YEAR;
        age_in_days >= required_days
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days_since_epoch() -> u32 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        (now.as_secs() / 86400) as u32
    }

    #[test]
    fn test_age_proof_success() {
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(18, issuer_pk);

        // Someone born 20 years ago
        let birthdate = public.current_date - (20 * 365);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 64], // Simulated signature
            blinding: [0u8; 32],
        };

        let proof = AgeProofCircuit::prove(&public, &private).expect("proof should succeed");
        assert!(proof.proof_bytes.len() >= 1000);

        let valid = AgeProofCircuit::verify(&public, &proof).expect("verify should not error");
        assert!(valid);
    }

    #[test]
    fn test_age_proof_too_young() {
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(21, issuer_pk);

        // Someone born 18 years ago (too young for 21)
        let birthdate = public.current_date - (18 * 365);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let result = AgeProofCircuit::prove(&public, &private);
        assert!(matches!(
            result,
            Err(CircuitError::ConstraintNotSatisfied(_))
        ));
    }

    #[test]
    fn test_age_proof_invalid_signature() {
        let issuer_pk = [1u8; 32];
        let public = AgeProofPublic::new(18, issuer_pk);

        let birthdate = public.current_date - (20 * 365);
        let private = AgeProofPrivate {
            birthdate_days: birthdate,
            issuer_signature: vec![0u8; 32], // Wrong size
            blinding: [0u8; 32],
        };

        let result = AgeProofCircuit::prove(&public, &private);
        assert!(matches!(result, Err(CircuitError::SignatureInvalid)));
    }

    #[test]
    fn test_would_pass() {
        let current = days_since_epoch();
        let birthdate_20_years_ago = current - (20 * 365);
        let birthdate_15_years_ago = current - (15 * 365);

        assert!(AgeProofCircuit::would_pass(
            birthdate_20_years_ago,
            18,
            current
        ));
        assert!(!AgeProofCircuit::would_pass(
            birthdate_15_years_ago,
            18,
            current
        ));
        assert!(AgeProofCircuit::would_pass(
            birthdate_15_years_ago,
            13,
            current
        ));
    }

    #[test]
    fn test_days_to_years() {
        assert_eq!(AgeProofCircuit::days_to_years(365), 1);
        assert_eq!(AgeProofCircuit::days_to_years(730), 2);
        assert_eq!(AgeProofCircuit::days_to_years(7300), 20);
    }

    #[test]
    fn test_public_validation() {
        let issuer_pk = [1u8; 32];

        let valid = AgeProofPublic::new(18, issuer_pk);
        assert!(valid.validate().is_ok());

        let zero_threshold = AgeProofPublic {
            threshold: 0,
            ..valid.clone()
        };
        assert!(zero_threshold.validate().is_err());

        let huge_threshold = AgeProofPublic {
            threshold: 200,
            ..valid.clone()
        };
        assert!(huge_threshold.validate().is_err());
    }
}
