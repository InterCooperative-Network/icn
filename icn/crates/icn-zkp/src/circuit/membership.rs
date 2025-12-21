//! Membership Proof Circuit
//!
//! Proves membership in an organization without revealing member identity.

use super::{compute_public_inputs_hash, Circuit, CircuitError};
use crate::types::StarkProof;
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Public inputs for membership proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipProofPublic {
    /// Organization DID
    pub org_did: Did,
    /// Required role (if any)
    pub required_role: Option<String>,
    /// Current timestamp (for validity check)
    pub current_time: u64,
    /// Proof context nonce
    pub nonce: [u8; 16],
}

impl MembershipProofPublic {
    /// Create new public inputs for any membership
    pub fn new(org_did: Did) -> Self {
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);

        Self {
            org_did,
            required_role: None,
            current_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            nonce,
        }
    }

    /// Require a specific role
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.required_role = Some(role.into());
        self
    }

    /// Validate public inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        // Role should not be empty string if specified
        if let Some(ref role) = self.required_role {
            if role.is_empty() {
                return Err(CircuitError::InvalidPublicInput(
                    "role cannot be empty".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Private inputs for membership proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipProofPrivate {
    /// Member's DID (hidden)
    pub member_did: Did,
    /// Member's role in the organization
    pub actual_role: Option<String>,
    /// Membership start timestamp
    pub member_since: u64,
    /// Membership expiry (if any)
    pub expires_at: Option<u64>,
    /// Organization's signature over membership
    pub org_signature: Vec<u8>,
    /// Blinding factor
    pub blinding: [u8; 32],
}

impl MembershipProofPrivate {
    /// Validate private inputs
    pub fn validate(&self) -> Result<(), CircuitError> {
        if self.org_signature.len() != 64 {
            return Err(CircuitError::InvalidPrivateInput(
                "invalid signature length".into(),
            ));
        }
        Ok(())
    }

    /// Check if membership is currently valid
    pub fn is_valid_at(&self, time: u64) -> bool {
        // Must have started
        if time < self.member_since {
            return false;
        }
        // Must not have expired
        if let Some(expires) = self.expires_at {
            if time > expires {
                return false;
            }
        }
        true
    }
}

/// Membership proof circuit
pub struct MembershipProofCircuit;

impl Circuit for MembershipProofCircuit {
    type Public = MembershipProofPublic;
    type Private = MembershipProofPrivate;

    fn name() -> &'static str {
        "membership_proof_v1"
    }

    fn prove(public: &Self::Public, private: &Self::Private) -> Result<StarkProof, CircuitError> {
        public.validate()?;
        private.validate()?;

        // Check membership is valid at current time
        if !private.is_valid_at(public.current_time) {
            return Err(CircuitError::ConstraintNotSatisfied(
                "membership not valid at current time".into(),
            ));
        }

        // Check role requirement if specified
        if let Some(ref required) = public.required_role {
            match &private.actual_role {
                Some(actual) if actual == required => {}
                Some(actual) => {
                    return Err(CircuitError::ConstraintNotSatisfied(format!(
                        "role '{actual}' does not match required '{required}'"
                    )));
                }
                None => {
                    return Err(CircuitError::ConstraintNotSatisfied(
                        "no role but role required".into(),
                    ));
                }
            }
        }

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
                "Creating SIMULATED membership proof - NO CRYPTOGRAPHIC SECURITY"
            );

            let mut hasher = Sha3_256::new();
            hasher.update(private.member_did.as_str().as_bytes());
            hasher.update(private.member_since.to_le_bytes());
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

    fn test_org_did() -> Did {
        Did::from_anchor_id(&[1u8; 32])
    }

    fn test_member_did() -> Did {
        Did::from_anchor_id(&[2u8; 32])
    }

    #[test]
    #[cfg(feature = "simulated")]
    fn test_membership_proof_success() {
        let public = MembershipProofPublic::new(test_org_did());

        let private = MembershipProofPrivate {
            member_did: test_member_did(),
            actual_role: None,
            member_since: 0,
            expires_at: None,
            org_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let proof = MembershipProofCircuit::prove(&public, &private).unwrap();
        assert!(MembershipProofCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    #[cfg(feature = "simulated")]
    fn test_membership_with_role() {
        let public = MembershipProofPublic::new(test_org_did()).with_role("admin");

        let private = MembershipProofPrivate {
            member_did: test_member_did(),
            actual_role: Some("admin".to_string()),
            member_since: 0,
            expires_at: None,
            org_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let proof = MembershipProofCircuit::prove(&public, &private).unwrap();
        assert!(MembershipProofCircuit::verify(&public, &proof).unwrap());
    }

    #[test]
    fn test_membership_wrong_role() {
        let public = MembershipProofPublic::new(test_org_did()).with_role("admin");

        let private = MembershipProofPrivate {
            member_did: test_member_did(),
            actual_role: Some("member".to_string()), // Not admin
            member_since: 0,
            expires_at: None,
            org_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let result = MembershipProofCircuit::prove(&public, &private);
        assert!(matches!(
            result,
            Err(CircuitError::ConstraintNotSatisfied(_))
        ));
    }

    #[test]
    fn test_membership_expired() {
        let public = MembershipProofPublic::new(test_org_did());

        let private = MembershipProofPrivate {
            member_did: test_member_did(),
            actual_role: None,
            member_since: 0,
            expires_at: Some(1), // Expired in 1970
            org_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        let result = MembershipProofCircuit::prove(&public, &private);
        assert!(matches!(
            result,
            Err(CircuitError::ConstraintNotSatisfied(_))
        ));
    }

    #[test]
    fn test_membership_validity() {
        let private = MembershipProofPrivate {
            member_did: test_member_did(),
            actual_role: None,
            member_since: 100,
            expires_at: Some(200),
            org_signature: vec![0u8; 64],
            blinding: [0u8; 32],
        };

        assert!(!private.is_valid_at(50)); // Before start
        assert!(private.is_valid_at(100)); // At start
        assert!(private.is_valid_at(150)); // During
        assert!(private.is_valid_at(200)); // At end
        assert!(!private.is_valid_at(201)); // After end
    }
}
