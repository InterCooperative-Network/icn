//! Federated Trust Attestations (Phase F2)
//!
//! Trust attestations allow cooperatives to vouch for the trustworthiness
//! of their members to other cooperatives in the federation.

use crate::error::{FederationError, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use icn_identity::Did;
use serde::{Deserialize, Serialize};

use crate::types::current_timestamp;

/// Federated trust attestation from one cooperative about a member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedTrustAttestation {
    /// ID of the cooperative issuing the attestation
    pub source_coop_id: String,

    /// DID of the cooperative (for signature verification)
    pub source_coop_did: Did,

    /// DID of the member being attested
    pub member_did: Did,

    /// Trust score (0.0 to 1.0)
    pub trust_score: f64,

    /// Context for the trust score
    pub trust_context: TrustContext,

    /// Summary of evidence supporting this attestation
    pub evidence_summary: Vec<EvidenceSummary>,

    /// Unix timestamp when issued
    pub issued_at: u64,

    /// Unix timestamp when this attestation expires
    pub expires_at: u64,

    /// Ed25519 signature over the attestation content
    pub signature: Vec<u8>,
}

/// Context for a trust score
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustContext {
    /// Economic behavior (payment history, credit behavior)
    Economic,
    /// Social behavior (community participation)
    Social,
    /// Governance behavior (voting, proposals)
    Governance,
    /// General overall standing
    General,
}

impl TrustContext {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustContext::Economic => "economic",
            TrustContext::Social => "social",
            TrustContext::Governance => "governance",
            TrustContext::General => "general",
        }
    }
}

/// Summary of evidence supporting an attestation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSummary {
    /// Type of evidence (e.g., "transaction_count", "dispute_count")
    pub kind: String,
    /// Human-readable description
    pub description: String,
    /// Optional count or value
    pub count: Option<u64>,
}

impl EvidenceSummary {
    pub fn new(kind: &str, description: &str) -> Self {
        Self {
            kind: kind.to_string(),
            description: description.to_string(),
            count: None,
        }
    }

    pub fn with_count(mut self, count: u64) -> Self {
        self.count = Some(count);
        self
    }
}

impl FederatedTrustAttestation {
    /// Create a new unsigned attestation
    pub fn new(
        source_coop_id: String,
        source_coop_did: Did,
        member_did: Did,
        trust_score: f64,
        trust_context: TrustContext,
        expiry_secs: u64,
    ) -> Self {
        let now = current_timestamp();
        Self {
            source_coop_id,
            source_coop_did,
            member_did,
            trust_score: trust_score.clamp(0.0, 1.0),
            trust_context,
            evidence_summary: Vec::new(),
            issued_at: now,
            expires_at: now + expiry_secs,
            signature: Vec::new(),
        }
    }

    /// Add evidence summary
    pub fn with_evidence(mut self, evidence: EvidenceSummary) -> Self {
        self.evidence_summary.push(evidence);
        self
    }

    /// Get the bytes to sign
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut att = self.clone();
        att.signature = Vec::new();
        serde_json::to_vec(&att).unwrap_or_default()
    }

    /// Sign the attestation
    pub fn sign(&mut self, signing_key: &SigningKey) -> Result<()> {
        let bytes = self.signing_bytes();
        let signature: Signature = signing_key.sign(&bytes);
        self.signature = signature.to_bytes().to_vec();
        Ok(())
    }

    /// Verify the attestation signature
    pub fn verify(&self, verifying_key: &VerifyingKey) -> Result<bool> {
        if self.signature.len() != 64 {
            return Err(FederationError::InvalidSignature(
                "Invalid signature length".to_string(),
            ));
        }

        let bytes = self.signing_bytes();
        let sig_bytes: [u8; 64] = self.signature[..64].try_into().map_err(|_| {
            FederationError::InvalidSignature("Invalid signature format".to_string())
        })?;

        let signature = Signature::from_bytes(&sig_bytes);
        match verifying_key.verify(&bytes, &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Check if the attestation has expired
    pub fn is_expired(&self) -> bool {
        current_timestamp() > self.expires_at
    }

    /// Check if the attestation is valid (not expired and has signature)
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && !self.signature.is_empty()
    }

    /// Get the remaining validity time in seconds
    pub fn remaining_validity(&self) -> u64 {
        let now = current_timestamp();
        self.expires_at.saturating_sub(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use icn_identity::KeyPair;
    use rand::rngs::OsRng;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_attestation_creation() {
        let att = FederatedTrustAttestation::new(
            "food-coop".to_string(),
            test_did(),
            test_did(),
            0.85,
            TrustContext::Economic,
            30 * 24 * 60 * 60, // 30 days
        );

        assert_eq!(att.source_coop_id, "food-coop");
        assert_eq!(att.trust_score, 0.85);
        assert!(!att.is_expired());
    }

    #[test]
    fn test_attestation_signing() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let mut att = FederatedTrustAttestation::new(
            "food-coop".to_string(),
            test_did(),
            test_did(),
            0.85,
            TrustContext::Economic,
            30 * 24 * 60 * 60,
        );

        att.sign(&signing_key).unwrap();
        assert!(!att.signature.is_empty());

        let verified = att.verify(&verifying_key).unwrap();
        assert!(verified);
    }

    #[test]
    fn test_trust_score_clamping() {
        let att = FederatedTrustAttestation::new(
            "food-coop".to_string(),
            test_did(),
            test_did(),
            1.5, // Over 1.0
            TrustContext::Economic,
            30 * 24 * 60 * 60,
        );

        assert_eq!(att.trust_score, 1.0); // Should be clamped
    }

    #[test]
    fn test_evidence_summary() {
        let att = FederatedTrustAttestation::new(
            "food-coop".to_string(),
            test_did(),
            test_did(),
            0.85,
            TrustContext::Economic,
            30 * 24 * 60 * 60,
        )
        .with_evidence(EvidenceSummary::new("transactions", "Total transactions").with_count(150))
        .with_evidence(EvidenceSummary::new("disputes", "Open disputes").with_count(0));

        assert_eq!(att.evidence_summary.len(), 2);
    }
}
