//! ZKP type definitions
//!
//! Core types for zero-knowledge proofs in SDIS.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use zeroize::ZeroizeOnDrop;

/// STARK proof (~50-100 KB typical)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarkProof {
    /// Raw proof bytes
    pub proof_bytes: Vec<u8>,
    /// Hash of public inputs for verification
    pub public_inputs_hash: [u8; 32],
    /// Proof system version
    pub version: u8,
}

impl StarkProof {
    /// Create a new STARK proof
    pub fn new(proof_bytes: Vec<u8>, public_inputs_hash: [u8; 32]) -> Self {
        Self {
            proof_bytes,
            public_inputs_hash,
            version: 1,
        }
    }

    /// Get proof size in bytes
    pub fn size(&self) -> usize {
        self.proof_bytes.len()
    }

    /// Check if proof is within expected size budget
    pub fn is_valid_size(&self) -> bool {
        // STARK proofs should be 50-100KB
        self.proof_bytes.len() >= 1000 && self.proof_bytes.len() <= 200_000
    }
}

/// Type of proof being requested or generated
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProofType {
    /// Prove age is at least threshold years
    AgeAtLeast { threshold: u8 },

    /// Prove citizenship/residence in a country
    Citizenship { country_code: [u8; 2] },

    /// Prove membership in an organization
    Membership { org_did: Did },

    /// Prove credential has not been revoked
    NonRevocation,

    /// Custom circuit proof
    Custom { circuit_id: [u8; 32] },
}

impl ProofType {
    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            ProofType::AgeAtLeast { threshold } => format!("Age >= {threshold} years"),
            ProofType::Citizenship { country_code } => {
                let code = String::from_utf8_lossy(country_code);
                format!("Citizen of {code}")
            }
            ProofType::Membership { org_did } => format!("Member of {org_did}"),
            ProofType::NonRevocation => "Not revoked".to_string(),
            ProofType::Custom { circuit_id } => {
                format!("Custom: {}", hex::encode(&circuit_id[..8]))
            }
        }
    }
}

/// Context for proof generation/verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofContext {
    /// Verifier's DID (if selective disclosure)
    pub verifier_did: Option<Did>,
    /// When the proof was generated
    pub timestamp: u64,
    /// Random nonce to prevent replay
    pub nonce: [u8; 16],
    /// Domain separator for the proof
    pub domain: Option<String>,
}

impl ProofContext {
    /// Create a new context with current timestamp
    pub fn new(verifier_did: Option<Did>) -> Self {
        use rand::RngCore;
        let mut nonce = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce);

        Self {
            verifier_did,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            nonce,
            domain: None,
        }
    }

    /// Set domain separator
    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Check if context is still valid (not too old)
    pub fn is_valid(&self, max_age_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now.saturating_sub(self.timestamp) <= max_age_secs
    }
}

/// Request for an attribute proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeProofRequest {
    /// What to prove
    pub proof_type: ProofType,
    /// Contextual information
    pub context: ProofContext,
    /// Whether to include non-revocation proof
    pub include_non_revocation: bool,
}

impl AttributeProofRequest {
    /// Create a new proof request
    pub fn new(proof_type: ProofType) -> Self {
        Self {
            proof_type,
            context: ProofContext::new(None),
            include_non_revocation: true,
        }
    }

    /// Set the verifier
    pub fn for_verifier(mut self, verifier: Did) -> Self {
        self.context.verifier_did = Some(verifier);
        self
    }

    /// Skip non-revocation check
    pub fn without_revocation_check(mut self) -> Self {
        self.include_non_revocation = false;
        self
    }
}

/// Combined attribute + non-revocation proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompoundProof {
    /// Proof of the attribute
    pub attribute_proof: StarkProof,
    /// Proof of non-revocation (optional)
    pub non_revocation_proof: Option<StarkProof>,
    /// Context used for generation
    pub context: ProofContext,
    /// What was proved
    pub proof_type: ProofType,
}

impl CompoundProof {
    /// Total size of all proofs
    pub fn total_size(&self) -> usize {
        self.attribute_proof.size()
            + self
                .non_revocation_proof
                .as_ref()
                .map(|p| p.size())
                .unwrap_or(0)
    }
}

/// Private attestation from an issuer (kept secret by holder)
#[derive(Debug, Clone, Serialize, Deserialize, ZeroizeOnDrop)]
pub struct Attestation {
    /// Issuer's DID
    #[zeroize(skip)]
    pub issuer_did: Did,
    /// Subject's anchor hash (truncated)
    #[zeroize(skip)]
    pub subject_anchor: [u8; 16],
    /// Attestation payload (encrypted or structured)
    pub payload: Vec<u8>,
    /// Issuer's signature
    #[zeroize(skip)]
    pub signature: Vec<u8>,
    /// When issued
    #[zeroize(skip)]
    pub issued_at: u64,
    /// Expiration (if any)
    #[zeroize(skip)]
    pub expires_at: Option<u64>,
}

impl Attestation {
    /// Check if attestation has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now > expires
        } else {
            false
        }
    }
}

/// Age attestation payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeAttestation {
    /// Birth date as days since Unix epoch
    pub birthdate_days: u32,
    /// Issuer's signature over birthdate
    pub signature: Vec<u8>,
}

/// Citizenship attestation payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitizenshipAttestation {
    /// ISO 3166-1 alpha-2 country code
    pub country_code: [u8; 2],
    /// Type: citizen, resident, visa holder
    pub status_type: CitizenshipStatus,
    /// Issuer's signature
    pub signature: Vec<u8>,
}

/// Citizenship/residency status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CitizenshipStatus {
    Citizen,
    PermanentResident,
    TemporaryResident,
    VisaHolder,
}

/// Membership attestation payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipAttestation {
    /// Organization DID
    pub org_did: Did,
    /// Member role (if any)
    pub role: Option<String>,
    /// Membership start date
    pub member_since: u64,
    /// Issuer's signature
    pub signature: Vec<u8>,
}

/// Verification result
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification succeeded
    pub valid: bool,
    /// Proof type that was verified
    pub proof_type: ProofType,
    /// Any warnings or notes
    pub warnings: Vec<String>,
    /// Verification timestamp
    pub verified_at: u64,
}

impl VerificationResult {
    /// Create a successful result
    pub fn success(proof_type: ProofType) -> Self {
        Self {
            valid: true,
            proof_type,
            warnings: Vec::new(),
            verified_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Create a failed result
    pub fn failure(proof_type: ProofType) -> Self {
        Self {
            valid: false,
            proof_type,
            warnings: Vec::new(),
            verified_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Add a warning
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_type_description() {
        let age = ProofType::AgeAtLeast { threshold: 18 };
        assert_eq!(age.description(), "Age >= 18 years");

        let citizenship = ProofType::Citizenship {
            country_code: [b'U', b'S'],
        };
        assert_eq!(citizenship.description(), "Citizen of US");

        let revocation = ProofType::NonRevocation;
        assert_eq!(revocation.description(), "Not revoked");
    }

    #[test]
    fn test_proof_context_validity() {
        let ctx = ProofContext::new(None);
        assert!(ctx.is_valid(60)); // Valid within 60 seconds
        assert!(ctx.is_valid(3600)); // Valid within 1 hour
    }

    #[test]
    fn test_stark_proof_size() {
        // Small proof (invalid)
        let small = StarkProof::new(vec![0u8; 100], [0u8; 32]);
        assert!(!small.is_valid_size());

        // Normal proof (valid)
        let normal = StarkProof::new(vec![0u8; 50_000], [0u8; 32]);
        assert!(normal.is_valid_size());

        // Large proof (invalid)
        let large = StarkProof::new(vec![0u8; 300_000], [0u8; 32]);
        assert!(!large.is_valid_size());
    }

    #[test]
    fn test_attestation_expiry() {
        let mut att = Attestation {
            issuer_did: Did::from_anchor_id(&[0u8; 32]),
            subject_anchor: [0u8; 16],
            payload: vec![],
            signature: vec![],
            issued_at: 0,
            expires_at: None,
        };

        // No expiry - never expires
        assert!(!att.is_expired());

        // Future expiry - not expired
        att.expires_at = Some(u64::MAX);
        assert!(!att.is_expired());

        // Past expiry - expired
        att.expires_at = Some(1);
        assert!(att.is_expired());
    }
}
