//! Enrollment Service - End-to-end enrollment orchestration
//!
//! This module provides the high-level enrollment service that coordinates:
//! 1. Proof-of-personhood verification
//! 2. Multi-steward ceremony coordination
//! 3. VUI computation via threshold PRF
//! 4. Uniqueness verification against registry
//! 5. Token issuance via blind signatures
//!
//! # Protocol Overview
//!
//! ```text
//! User                     Steward Network
//! ────                     ───────────────
//!   │                            │
//!   │─── EnrollmentRequest ─────>│  (VUI commitment, pathway proof)
//!   │                            │
//!   │<── CeremonyStarted ────────│  (ceremony_id, participating stewards)
//!   │                            │
//!   │                      ┌─────┴─────┐
//!   │                      │  Stewards │
//!   │                      │  compute  │
//!   │                      │  PRF      │
//!   │                      │  partials │
//!   │                      └─────┬─────┘
//!   │                            │
//!   │<── PrfPartials ────────────│  (encrypted partials from t stewards)
//!   │                            │
//!   │─── BlindedTokenRequest ───>│  (after computing VUI locally)
//!   │                            │
//!   │                      ┌─────┴─────┐
//!   │                      │  Check    │
//!   │                      │  VUI      │
//!   │                      │  unique   │
//!   │                      └─────┬─────┘
//!   │                            │
//!   │<── BlindedSignature ───────│  (if VUI is unique)
//!   │                            │
//!   │  (unblind locally)         │
//!   │                            │
//!   └────────────────────────────┘
//! ```

use anyhow::{bail, Result};
use icn_crypto_pq::threshold::{combine_prf_partials, PepperShare, PrfPartial, ShareCommitment};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ceremony::enrollment::{EnrollmentCeremony, EnrollmentResult, ShareContribution};
use crate::ceremony::CeremonyId;
use crate::token::EnrollmentToken;
use crate::vui_registry::{LocalCheckResult, VuiRegistry};

/// Configuration for the enrollment service
#[derive(Debug, Clone)]
pub struct EnrollmentConfig {
    /// Threshold of stewards required (t in t-of-n)
    pub threshold: u32,
    /// Total number of stewards (n in t-of-n)
    pub total_stewards: u32,
    /// Maximum time for ceremony completion
    pub ceremony_timeout: Duration,
    /// Token validity period
    pub token_validity: Duration,
}

impl Default for EnrollmentConfig {
    fn default() -> Self {
        Self {
            threshold: 3,
            total_stewards: 5,
            ceremony_timeout: Duration::from_secs(3600), // 1 hour
            token_validity: Duration::from_secs(7 * 24 * 3600), // 7 days
        }
    }
}

/// Request to start an enrollment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentRequest {
    /// VUI commitment: H(id_data_hash || salt)
    pub vui_commitment: [u8; 32],
    /// Pathway proof hash (proof-of-personhood type)
    pub pathway_hash: [u8; 8],
    /// Salt for VUI commitment (revealed after ceremony)
    pub commitment_salt: [u8; 16],
    /// Optional: specific stewards to request
    pub preferred_stewards: Vec<Did>,
}

impl EnrollmentRequest {
    /// Create a new enrollment request
    pub fn new(id_data_hash: &[u8; 32], pathway_hash: [u8; 8]) -> Self {
        let mut commitment_salt = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut commitment_salt);

        let vui_commitment = Self::compute_commitment(id_data_hash, &commitment_salt);

        Self {
            vui_commitment,
            pathway_hash,
            commitment_salt,
            preferred_stewards: Vec::new(),
        }
    }

    /// Compute VUI commitment from id_data_hash and salt
    fn compute_commitment(id_data_hash: &[u8; 32], salt: &[u8; 16]) -> [u8; 32] {
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-vui-commitment-v1");
        hasher.update(id_data_hash);
        hasher.update(salt);
        let result = hasher.finalize();

        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&result);
        commitment
    }

    /// Verify commitment matches revealed data
    pub fn verify_commitment(&self, id_data_hash: &[u8; 32]) -> bool {
        let expected = Self::compute_commitment(id_data_hash, &self.commitment_salt);
        expected == self.vui_commitment
    }
}

/// Response when ceremony is started
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyStartResponse {
    /// Ceremony identifier
    pub ceremony_id: CeremonyId,
    /// Participating stewards
    pub stewards: Vec<Did>,
    /// Expected completion time
    pub expected_completion: u64,
}

/// PRF partial from a steward (encrypted to user)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPrfPartial {
    /// Steward who computed this partial
    pub steward_did: Did,
    /// Share index for combination
    pub share_index: u8,
    /// Encrypted partial value (encrypted to user's public key)
    pub encrypted_value: Vec<u8>,
    /// Commitment for verification
    pub commitment: [u8; 32],
    /// Signature over the partial
    pub signature: Vec<u8>,
}

/// Request to complete enrollment with blinded token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindedTokenRequest {
    /// Ceremony this request is for
    pub ceremony_id: CeremonyId,
    /// Blinded message for signature
    pub blinded_message: Vec<u8>,
    /// VUI hash for uniqueness check
    pub vui_hash: [u8; 32],
    /// Proof that VUI was correctly computed (ZKP placeholder)
    pub vui_proof: Vec<u8>,
}

/// Enrollment service state
pub struct EnrollmentService {
    /// Configuration
    config: EnrollmentConfig,
    /// This steward's DID
    steward_did: Did,
    /// This steward's pepper share
    pepper_share: Option<PepperShare>,
    /// Share commitment (public)
    share_commitment: Option<ShareCommitment>,
    /// Active ceremonies
    ceremonies: HashMap<CeremonyId, EnrollmentCeremony>,
    /// VUI registry for uniqueness checking
    vui_registry: VuiRegistry,
    /// Issued tokens (for revocation tracking)
    issued_tokens: HashMap<[u8; 32], EnrollmentToken>,
}

impl EnrollmentService {
    /// Create a new enrollment service
    pub fn new(steward_did: Did, config: EnrollmentConfig) -> Self {
        Self {
            config,
            steward_did,
            pepper_share: None,
            share_commitment: None,
            ceremonies: HashMap::new(),
            vui_registry: VuiRegistry::with_defaults(),
            issued_tokens: HashMap::new(),
        }
    }

    /// Initialize with a pepper share
    pub fn set_pepper_share(&mut self, share: PepperShare) {
        self.share_commitment = Some(ShareCommitment::commit(&share));
        self.pepper_share = Some(share);
    }

    /// Get this steward's share commitment (public)
    pub fn share_commitment(&self) -> Option<&ShareCommitment> {
        self.share_commitment.as_ref()
    }

    /// Start a new enrollment ceremony (coordinator role)
    pub fn start_ceremony(&mut self, request: &EnrollmentRequest) -> Result<CeremonyStartResponse> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate ceremony ID
        let ceremony_id =
            EnrollmentCeremony::generate_ceremony_id(&request.vui_commitment, now);

        // Check for duplicate
        if self.ceremonies.contains_key(&ceremony_id) {
            bail!("Ceremony already exists for this commitment");
        }

        // Create ceremony
        let ceremony = EnrollmentCeremony::new(
            ceremony_id,
            self.steward_did.clone(),
            request.vui_commitment,
            request.pathway_hash,
            self.config.threshold,
        );

        let expected_completion = now + self.config.ceremony_timeout.as_secs();

        self.ceremonies.insert(ceremony_id, ceremony);

        Ok(CeremonyStartResponse {
            ceremony_id,
            stewards: vec![self.steward_did.clone()], // Will be populated via gossip
            expected_completion,
        })
    }

    /// Compute PRF partial for a ceremony (participant role)
    pub fn compute_prf_partial(
        &self,
        ceremony_id: &CeremonyId,
        id_data_hash: &[u8; 32],
    ) -> Result<PrfPartial> {
        let pepper_share = self
            .pepper_share
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Pepper share not initialized"))?;

        // Verify ceremony exists and is in correct phase
        let ceremony = self
            .ceremonies
            .get(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        if ceremony.state.is_expired() {
            bail!("Ceremony has expired");
        }

        // Compute PRF partial
        Ok(PrfPartial::compute(pepper_share, id_data_hash))
    }

    /// Add a share contribution to a ceremony
    pub fn add_contribution(
        &mut self,
        ceremony_id: &CeremonyId,
        contribution: ShareContribution,
    ) -> Result<()> {
        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        ceremony.add_share_contribution(contribution)?;
        Ok(())
    }

    /// Complete VUI computation phase
    pub fn complete_vui_computation(
        &mut self,
        ceremony_id: &CeremonyId,
        partials: &[PrfPartial],
    ) -> Result<[u8; 32]> {
        // Verify we have enough partials
        if (partials.len() as u32) < self.config.threshold {
            bail!(
                "Insufficient partials: {} < {}",
                partials.len(),
                self.config.threshold
            );
        }

        // Combine partials to compute VUI
        let vui = combine_prf_partials(partials)?;

        // Update ceremony state
        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        ceremony.begin_vui_computation()?;
        ceremony.set_vui_hash(vui)?;

        Ok(vui)
    }

    /// Check if VUI is unique and proceed to token issuance
    pub fn verify_uniqueness(&mut self, ceremony_id: &CeremonyId) -> Result<bool> {
        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        let vui_hash = ceremony
            .vui_hash
            .ok_or_else(|| anyhow::anyhow!("VUI not computed"))?;

        // Check against registry
        match self.vui_registry.check_local(&vui_hash) {
            LocalCheckResult::Registered(_) => {
                // VUI exists - enrollment rejected
                ceremony.fail("VUI already exists - duplicate enrollment attempt".to_string());
                return Ok(false);
            }
            LocalCheckResult::PossiblyRegistered => {
                // Bloom filter positive but not in exact set - likely unique
                // In production, would do distributed check
            }
            LocalCheckResult::NotRegistered => {
                // Definitely unique
            }
        }

        // VUI is unique - proceed
        ceremony.confirm_uniqueness()?;
        Ok(true)
    }

    /// Issue blinded signature for enrollment token
    pub fn issue_blinded_signature(
        &mut self,
        ceremony_id: &CeremonyId,
        blinded_message: &[u8],
    ) -> Result<EnrollmentResult> {
        // Compute blind signature first (before mutable borrow of ceremony)
        // In production, this would use actual blind signature scheme
        let blinded_signature = self.compute_blind_signature(blinded_message)?;

        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        // Set blinded request
        ceremony.set_blinded_request(blinded_message.to_vec());

        // Complete ceremony
        let result = ceremony.complete(blinded_signature)?;

        // Register VUI
        let _ = self.vui_registry.register(
            result.vui_hash,
            self.steward_did.clone(), // registered_by (coordinator in this case)
            self.steward_did.clone(), // witnessing_steward
        );

        Ok(result)
    }

    /// Compute blind signature (placeholder - uses HMAC for now)
    fn compute_blind_signature(&self, blinded_message: &[u8]) -> Result<Vec<u8>> {
        use hmac::{Hmac, Mac};
        use sha3::Sha3_256;

        // In production, this would use RSA blind signatures or similar
        // For now, we use HMAC as a placeholder
        let pepper_share = self
            .pepper_share
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Pepper share not initialized"))?;

        let mut mac = Hmac::<Sha3_256>::new_from_slice(&pepper_share.value)
            .expect("HMAC key length should be valid");
        mac.update(b"icn-blind-sig-v1");
        mac.update(blinded_message);

        Ok(mac.finalize().into_bytes().to_vec())
    }

    /// Get ceremony status
    pub fn get_ceremony(&self, ceremony_id: &CeremonyId) -> Option<&EnrollmentCeremony> {
        self.ceremonies.get(ceremony_id)
    }

    /// Get enrollment statistics
    pub fn stats(&self) -> EnrollmentStats {
        EnrollmentStats {
            active_ceremonies: self.ceremonies.len(),
            registered_vuis: self.vui_registry.len(),
            issued_tokens: self.issued_tokens.len(),
        }
    }

    /// Cleanup expired ceremonies
    pub fn cleanup_expired(&mut self) -> usize {
        let before = self.ceremonies.len();
        self.ceremonies.retain(|_, c| !c.state.is_expired());
        before - self.ceremonies.len()
    }
}

/// Enrollment service statistics
#[derive(Debug, Clone)]
pub struct EnrollmentStats {
    /// Number of active ceremonies
    pub active_ceremonies: usize,
    /// Number of registered VUIs
    pub registered_vuis: usize,
    /// Number of issued tokens
    pub issued_tokens: usize,
}

/// Client-side enrollment helper
pub struct EnrollmentClient {
    /// User's identity data hash
    id_data_hash: [u8; 32],
    /// Active enrollment request
    request: Option<EnrollmentRequest>,
    /// Collected PRF partials
    collected_partials: Vec<PrfPartial>,
    /// Computed VUI
    vui: Option<[u8; 32]>,
}

impl EnrollmentClient {
    /// Create a new enrollment client
    pub fn new(id_data_hash: [u8; 32]) -> Self {
        Self {
            id_data_hash,
            request: None,
            collected_partials: Vec::new(),
            vui: None,
        }
    }

    /// Create enrollment request
    pub fn create_request(&mut self, pathway_hash: [u8; 8]) -> &EnrollmentRequest {
        let request = EnrollmentRequest::new(&self.id_data_hash, pathway_hash);
        self.request = Some(request);
        self.request.as_ref().unwrap()
    }

    /// Add a PRF partial from a steward
    pub fn add_partial(&mut self, partial: PrfPartial) {
        self.collected_partials.push(partial);
    }

    /// Check if we have enough partials
    pub fn has_enough_partials(&self, threshold: usize) -> bool {
        self.collected_partials.len() >= threshold
    }

    /// Compute VUI from collected partials
    pub fn compute_vui(&mut self) -> Result<[u8; 32]> {
        if self.collected_partials.is_empty() {
            bail!("No partials collected");
        }

        let vui = combine_prf_partials(&self.collected_partials)?;
        self.vui = Some(vui);
        Ok(vui)
    }

    /// Get computed VUI
    pub fn vui(&self) -> Option<[u8; 32]> {
        self.vui
    }

    /// Create blinded token request
    pub fn create_blinded_request(&self, ceremony_id: CeremonyId) -> Result<BlindedTokenRequest> {
        let vui = self
            .vui
            .ok_or_else(|| anyhow::anyhow!("VUI not computed"))?;

        // In production, this would blind the message
        // For now, we use a simple hash
        let mut hasher = Sha3_256::new();
        hasher.update(b"icn-token-request-v1");
        hasher.update(vui);
        hasher.update(ceremony_id);
        let blinded_message = hasher.finalize().to_vec();

        Ok(BlindedTokenRequest {
            ceremony_id,
            blinded_message,
            vui_hash: vui,
            vui_proof: Vec::new(), // Placeholder for ZKP
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        keypair.did().clone()
    }

    #[test]
    fn test_enrollment_request_creation() {
        let id_data_hash = [42u8; 32];
        let pathway_hash = [1u8; 8];

        let request = EnrollmentRequest::new(&id_data_hash, pathway_hash);

        assert!(request.verify_commitment(&id_data_hash));
        assert!(!request.verify_commitment(&[0u8; 32])); // Wrong data fails
    }

    #[test]
    fn test_enrollment_service_creation() {
        let did = test_did();
        let service = EnrollmentService::new(did, EnrollmentConfig::default());

        let stats = service.stats();
        assert_eq!(stats.active_ceremonies, 0);
        assert_eq!(stats.registered_vuis, 0);
    }

    #[test]
    fn test_enrollment_service_with_pepper() {
        let did = test_did();
        let mut service = EnrollmentService::new(did, EnrollmentConfig::default());

        let share = PepperShare::new(1, [99u8; 32]);
        service.set_pepper_share(share);

        assert!(service.share_commitment().is_some());
    }

    #[test]
    fn test_start_ceremony() {
        let did = test_did();
        let mut service = EnrollmentService::new(did, EnrollmentConfig::default());

        let id_data_hash = [42u8; 32];
        let pathway_hash = [1u8; 8];
        let request = EnrollmentRequest::new(&id_data_hash, pathway_hash);

        let response = service.start_ceremony(&request).unwrap();
        assert!(!response.stewards.is_empty());

        let stats = service.stats();
        assert_eq!(stats.active_ceremonies, 1);
    }

    #[test]
    fn test_compute_prf_partial() {
        let did = test_did();
        let mut service = EnrollmentService::new(did, EnrollmentConfig::default());

        let share = PepperShare::new(1, [99u8; 32]);
        service.set_pepper_share(share);

        let id_data_hash = [42u8; 32];
        let pathway_hash = [1u8; 8];
        let request = EnrollmentRequest::new(&id_data_hash, pathway_hash);

        let response = service.start_ceremony(&request).unwrap();

        let partial = service
            .compute_prf_partial(&response.ceremony_id, &id_data_hash)
            .unwrap();
        assert_eq!(partial.index, 1);
    }

    #[test]
    fn test_enrollment_client() {
        let id_data_hash = [42u8; 32];
        let mut client = EnrollmentClient::new(id_data_hash);

        let request = client.create_request([1u8; 8]);
        assert!(request.verify_commitment(&id_data_hash));

        // Simulate receiving partials
        let share1 = PepperShare::new(1, [1u8; 32]);
        let share2 = PepperShare::new(2, [2u8; 32]);
        let share3 = PepperShare::new(3, [3u8; 32]);

        client.add_partial(PrfPartial::compute(&share1, &id_data_hash));
        client.add_partial(PrfPartial::compute(&share2, &id_data_hash));
        client.add_partial(PrfPartial::compute(&share3, &id_data_hash));

        assert!(client.has_enough_partials(3));

        let vui = client.compute_vui().unwrap();
        assert_eq!(client.vui(), Some(vui));
    }

    #[test]
    fn test_full_enrollment_flow() {
        // Setup steward
        let steward_did = test_did();
        let mut service = EnrollmentService::new(
            steward_did,
            EnrollmentConfig {
                threshold: 1, // Single steward for test
                ..Default::default()
            },
        );

        let share = PepperShare::new(1, [99u8; 32]);
        service.set_pepper_share(share.clone());

        // Setup client
        let id_data_hash = [42u8; 32];
        let mut client = EnrollmentClient::new(id_data_hash);
        let request = client.create_request([1u8; 8]);

        // 1. Start ceremony
        let response = service.start_ceremony(request).unwrap();

        // 2. Compute and collect partial
        let partial = service
            .compute_prf_partial(&response.ceremony_id, &id_data_hash)
            .unwrap();
        client.add_partial(partial.clone());

        // 3. Add contribution to ceremony
        let contribution = ShareContribution {
            steward_did: service.steward_did.clone(),
            encrypted_share: vec![1, 2, 3],
            share_commitment: service.share_commitment().unwrap().commitment,
            signature: vec![4, 5, 6],
        };
        service
            .add_contribution(&response.ceremony_id, contribution)
            .unwrap();

        // 4. Complete VUI computation (server side)
        let vui = service
            .complete_vui_computation(&response.ceremony_id, &[partial])
            .unwrap();

        // 5. Client computes VUI too
        let client_vui = client.compute_vui().unwrap();
        assert_eq!(vui, client_vui);

        // 6. Verify uniqueness
        let is_unique = service.verify_uniqueness(&response.ceremony_id).unwrap();
        assert!(is_unique);

        // 7. Issue token
        let token_request = client.create_blinded_request(response.ceremony_id).unwrap();
        let result = service
            .issue_blinded_signature(&response.ceremony_id, &token_request.blinded_message)
            .unwrap();

        assert_eq!(result.vui_hash, vui);

        // 8. Verify VUI is now registered
        let stats = service.stats();
        assert_eq!(stats.registered_vuis, 1);

        // 9. Second enrollment with same VUI should fail
        let request2 = EnrollmentRequest::new(&id_data_hash, [2u8; 8]);
        let response2 = service.start_ceremony(&request2).unwrap();

        let partial2 = service
            .compute_prf_partial(&response2.ceremony_id, &id_data_hash)
            .unwrap();
        let contribution2 = ShareContribution {
            steward_did: service.steward_did.clone(),
            encrypted_share: vec![1, 2, 3],
            share_commitment: service.share_commitment().unwrap().commitment,
            signature: vec![4, 5, 6],
        };
        service
            .add_contribution(&response2.ceremony_id, contribution2)
            .unwrap();
        service
            .complete_vui_computation(&response2.ceremony_id, &[partial2])
            .unwrap();

        let is_unique2 = service.verify_uniqueness(&response2.ceremony_id).unwrap();
        assert!(!is_unique2); // Should fail - VUI already exists
    }
}
