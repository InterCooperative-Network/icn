//! Recovery Service
//!
//! Orchestrates end-to-end identity recovery, coordinating with the steward
//! network to verify identity ownership and bind new keys to an existing anchor.
//!
//! # Recovery Flow
//!
//! 1. User generates evidence (ZK proof of VUI ownership)
//! 2. User contacts coordinator steward with recovery request
//! 3. Coordinator broadcasts request to steward network
//! 4. Stewards verify evidence and submit attestations
//! 5. Once threshold attestations received, new keys are bound
//! 6. Old keys are revoked across the network
//! 7. User receives confirmation with updated identity

use std::collections::HashMap;

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ceremony::recovery::{
    RecoveryAttestation, RecoveryCeremony, RecoveryPhase, RecoveryResult,
};
use crate::ceremony::CeremonyId;

/// Configuration for the recovery service
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Minimum attestations required (threshold)
    pub threshold: u32,
    /// Total stewards in the network
    pub total_stewards: u32,
    /// Timeout for collecting attestations (seconds)
    pub attestation_timeout_secs: u64,
    /// Maximum concurrent recovery ceremonies
    pub max_concurrent_recoveries: usize,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            threshold: 3,
            total_stewards: 5,
            attestation_timeout_secs: 3600, // 1 hour
            max_concurrent_recoveries: 100,
        }
    }
}

/// Evidence for identity recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEvidence {
    /// Hash of the VUI (proves knowledge of VUI)
    pub vui_hash: [u8; 32],
    /// ZK proof of VUI ownership (simplified for now)
    pub zk_proof: Vec<u8>,
    /// Anchor commitment being recovered
    pub anchor_commitment: [u8; 32],
    /// Timestamp of evidence generation
    pub generated_at: u64,
}

impl RecoveryEvidence {
    /// Create new recovery evidence
    pub fn new(vui: &[u8; 32], anchor_commitment: [u8; 32]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-vui-hash");
        hasher.update(vui);
        let hash = hasher.finalize();

        let mut vui_hash = [0u8; 32];
        vui_hash.copy_from_slice(&hash);

        // Simplified ZK proof - in production this would be a real STARK proof
        let mut proof_hasher = Sha256::new();
        proof_hasher.update(b"icn-recovery-proof-v1");
        proof_hasher.update(vui);
        proof_hasher.update(anchor_commitment);
        let proof = proof_hasher.finalize().to_vec();

        Self {
            vui_hash,
            zk_proof: proof,
            anchor_commitment,
            generated_at: icn_time::current_timestamp_secs(),
        }
    }

    /// Verify the evidence hash matches the anchor commitment
    pub fn verify(&self, anchor_commitment: &[u8; 32]) -> bool {
        self.anchor_commitment == *anchor_commitment
    }

    /// Get the evidence hash for gossip
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"icn-recovery-evidence");
        hasher.update(self.vui_hash);
        hasher.update(&self.zk_proof);
        let hash = hasher.finalize();

        let mut result = [0u8; 32];
        result.copy_from_slice(&hash);
        result
    }
}

/// Request to initiate recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryRequest {
    /// Old DID being recovered
    pub old_did: Did,
    /// New DID to bind to anchor
    pub new_did: Did,
    /// Evidence proving ownership
    pub evidence: RecoveryEvidence,
    /// Preferred coordinator steward (optional)
    pub preferred_coordinator: Option<Did>,
}

impl RecoveryRequest {
    /// Create a new recovery request
    pub fn new(old_did: Did, new_did: Did, evidence: RecoveryEvidence) -> Self {
        Self {
            old_did,
            new_did,
            evidence,
            preferred_coordinator: None,
        }
    }

    /// Set preferred coordinator
    pub fn with_coordinator(mut self, coordinator: Did) -> Self {
        self.preferred_coordinator = Some(coordinator);
        self
    }
}

/// Response when recovery ceremony is started
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStartResponse {
    /// Ceremony ID
    pub ceremony_id: CeremonyId,
    /// Coordinator steward DID
    pub coordinator: Did,
    /// Participating stewards
    pub stewards: Vec<Did>,
    /// Threshold required
    pub threshold: u32,
}

/// Service for coordinating identity recovery
pub struct RecoveryService {
    /// Configuration
    config: RecoveryConfig,
    /// This steward's DID
    steward_did: Did,
    /// Active recovery ceremonies (coordinator role)
    ceremonies: HashMap<CeremonyId, RecoveryCeremony>,
    /// Revoked DIDs registry
    revoked_dids: HashMap<Did, RevocationRecord>,
}

/// Record of a revoked DID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationRecord {
    /// Old DID that was revoked
    pub old_did: Did,
    /// New DID that replaced it
    pub new_did: Did,
    /// When the revocation occurred
    pub revoked_at: u64,
    /// Recovery ceremony ID
    pub ceremony_id: CeremonyId,
    /// Attesting stewards
    pub attesters: Vec<Did>,
}

/// Statistics about recovery service
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    /// Active ceremonies
    pub active_ceremonies: usize,
    /// Total revoked DIDs
    pub revoked_dids: usize,
    /// Total successful recoveries
    pub successful_recoveries: u64,
    /// Total failed recoveries
    pub failed_recoveries: u64,
}

impl RecoveryService {
    /// Create a new recovery service
    pub fn new(steward_did: Did, config: RecoveryConfig) -> Self {
        Self {
            config,
            steward_did,
            ceremonies: HashMap::new(),
            revoked_dids: HashMap::new(),
        }
    }

    /// Get this steward's DID
    pub fn steward_did(&self) -> &Did {
        &self.steward_did
    }

    /// Start a recovery ceremony (coordinator role)
    pub fn start_ceremony(
        &mut self,
        request: RecoveryRequest,
    ) -> anyhow::Result<RecoveryStartResponse> {
        // Check if old DID is already revoked
        if self.revoked_dids.contains_key(&request.old_did) {
            anyhow::bail!("DID has already been recovered/revoked");
        }

        // Check concurrent limit
        if self.ceremonies.len() >= self.config.max_concurrent_recoveries {
            anyhow::bail!("Maximum concurrent recoveries reached");
        }

        // Generate ceremony ID
        let now = icn_time::current_timestamp_secs();
        let ceremony_id =
            RecoveryCeremony::generate_ceremony_id(&request.old_did, &request.new_did, now);

        // Create ceremony
        let ceremony = RecoveryCeremony::new(
            ceremony_id,
            self.steward_did.clone(),
            request.old_did,
            request.new_did,
            request.evidence.hash(),
            request.evidence.anchor_commitment,
            self.config.threshold,
        );

        self.ceremonies.insert(ceremony_id, ceremony);

        Ok(RecoveryStartResponse {
            ceremony_id,
            coordinator: self.steward_did.clone(),
            stewards: vec![self.steward_did.clone()], // In real impl, would list participating stewards
            threshold: self.config.threshold,
        })
    }

    /// Add attestation to a ceremony
    pub fn add_attestation(
        &mut self,
        ceremony_id: &CeremonyId,
        attestation: RecoveryAttestation,
    ) -> anyhow::Result<()> {
        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        ceremony.add_attestation(attestation)?;
        Ok(())
    }

    /// Check if ceremony has enough attestations and can proceed
    pub fn can_proceed(&self, ceremony_id: &CeremonyId) -> bool {
        self.ceremonies
            .get(ceremony_id)
            .map(|c| c.has_enough_support())
            .unwrap_or(false)
    }

    /// Proceed through verification and key binding
    pub fn verify_and_bind(&mut self, ceremony_id: &CeremonyId) -> anyhow::Result<()> {
        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        // Progress through phases
        ceremony.begin_verification()?;
        ceremony.confirm_evidence()?;
        ceremony.begin_revocation()?;

        Ok(())
    }

    /// Complete the recovery ceremony
    pub fn complete_recovery(
        &mut self,
        ceremony_id: &CeremonyId,
    ) -> anyhow::Result<RecoveryResult> {
        let ceremony = self
            .ceremonies
            .get_mut(ceremony_id)
            .ok_or_else(|| anyhow::anyhow!("Ceremony not found"))?;

        let result = ceremony.complete()?;

        // Record revocation
        let revocation = RevocationRecord {
            old_did: result.old_did.clone(),
            new_did: result.new_did.clone(),
            revoked_at: result.completed_at,
            ceremony_id: *ceremony_id,
            attesters: result.attesters.clone(),
        };
        self.revoked_dids.insert(result.old_did.clone(), revocation);

        Ok(result)
    }

    /// Check if a DID has been revoked
    pub fn is_revoked(&self, did: &Did) -> bool {
        self.revoked_dids.contains_key(did)
    }

    /// Get revocation record for a DID
    pub fn get_revocation(&self, did: &Did) -> Option<&RevocationRecord> {
        self.revoked_dids.get(did)
    }

    /// Get the new DID for a revoked DID
    pub fn get_replacement_did(&self, old_did: &Did) -> Option<&Did> {
        self.revoked_dids.get(old_did).map(|r| &r.new_did)
    }

    /// Get ceremony by ID
    pub fn get_ceremony(&self, ceremony_id: &CeremonyId) -> Option<&RecoveryCeremony> {
        self.ceremonies.get(ceremony_id)
    }

    /// Get service statistics
    pub fn stats(&self) -> RecoveryStats {
        RecoveryStats {
            active_ceremonies: self
                .ceremonies
                .values()
                .filter(|c| c.phase != RecoveryPhase::Completed && c.phase != RecoveryPhase::Failed)
                .count(),
            revoked_dids: self.revoked_dids.len(),
            successful_recoveries: self
                .ceremonies
                .values()
                .filter(|c| c.phase == RecoveryPhase::Completed)
                .count() as u64,
            failed_recoveries: self
                .ceremonies
                .values()
                .filter(|c| c.phase == RecoveryPhase::Failed || c.phase == RecoveryPhase::Rejected)
                .count() as u64,
        }
    }
}

/// Client-side recovery helper
pub struct RecoveryClient {
    /// Old DID being recovered
    old_did: Did,
    /// New DID to bind
    new_did: Did,
    /// Evidence for recovery
    evidence: Option<RecoveryEvidence>,
    /// Ceremony ID (set after starting)
    ceremony_id: Option<CeremonyId>,
    /// Recovery result (set after completion)
    result: Option<RecoveryResult>,
}

impl RecoveryClient {
    /// Create a new recovery client
    pub fn new(old_did: Did, new_did: Did) -> Self {
        Self {
            old_did,
            new_did,
            evidence: None,
            ceremony_id: None,
            result: None,
        }
    }

    /// Generate evidence for recovery
    pub fn generate_evidence(
        &mut self,
        vui: &[u8; 32],
        anchor_commitment: [u8; 32],
    ) -> &RecoveryEvidence {
        self.evidence = Some(RecoveryEvidence::new(vui, anchor_commitment));
        self.evidence.as_ref().unwrap()
    }

    /// Create recovery request
    pub fn create_request(&self) -> anyhow::Result<RecoveryRequest> {
        let evidence = self
            .evidence
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Evidence not generated"))?;

        Ok(RecoveryRequest::new(
            self.old_did.clone(),
            self.new_did.clone(),
            evidence,
        ))
    }

    /// Set ceremony ID after starting
    pub fn set_ceremony_id(&mut self, ceremony_id: CeremonyId) {
        self.ceremony_id = Some(ceremony_id);
    }

    /// Set result after completion
    pub fn set_result(&mut self, result: RecoveryResult) {
        self.result = Some(result);
    }

    /// Get the old DID
    pub fn old_did(&self) -> &Did {
        &self.old_did
    }

    /// Get the new DID
    pub fn new_did(&self) -> &Did {
        &self.new_did
    }

    /// Get recovery result
    pub fn result(&self) -> Option<&RecoveryResult> {
        self.result.as_ref()
    }

    /// Check if recovery is complete
    pub fn is_complete(&self) -> bool {
        self.result.is_some()
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
    fn test_recovery_evidence() {
        let vui = [42u8; 32];
        let anchor_commitment = [1u8; 32];

        let evidence = RecoveryEvidence::new(&vui, anchor_commitment);

        assert!(evidence.verify(&anchor_commitment));
        assert!(!evidence.verify(&[2u8; 32]));
        assert_eq!(evidence.anchor_commitment, anchor_commitment);
    }

    #[test]
    fn test_recovery_service_creation() {
        let steward_did = test_did();
        let service = RecoveryService::new(steward_did.clone(), RecoveryConfig::default());

        assert_eq!(*service.steward_did(), steward_did);
        assert_eq!(service.stats().active_ceremonies, 0);
    }

    #[test]
    fn test_start_recovery_ceremony() {
        let steward_did = test_did();
        let mut service = RecoveryService::new(steward_did, RecoveryConfig::default());

        let old_did = test_did();
        let new_did = test_did();
        let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence);

        let response = service.start_ceremony(request).unwrap();

        assert_eq!(response.threshold, 3);
        assert!(service.get_ceremony(&response.ceremony_id).is_some());
    }

    #[test]
    fn test_recovery_with_attestations() {
        let steward_did = test_did();
        let mut service = RecoveryService::new(
            steward_did,
            RecoveryConfig {
                threshold: 2,
                ..Default::default()
            },
        );

        // Start ceremony
        let old_did = test_did();
        let new_did = test_did();
        let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence);
        let response = service.start_ceremony(request).unwrap();

        // Add attestations
        let attestation1 = RecoveryAttestation::support(test_did(), vec![1, 2, 3]);
        service
            .add_attestation(&response.ceremony_id, attestation1)
            .unwrap();
        assert!(!service.can_proceed(&response.ceremony_id));

        let attestation2 = RecoveryAttestation::support(test_did(), vec![4, 5, 6]);
        service
            .add_attestation(&response.ceremony_id, attestation2)
            .unwrap();
        assert!(service.can_proceed(&response.ceremony_id));
    }

    #[test]
    fn test_complete_recovery() {
        let steward_did = test_did();
        let mut service = RecoveryService::new(
            steward_did,
            RecoveryConfig {
                threshold: 2,
                ..Default::default()
            },
        );

        // Start ceremony
        let old_did = test_did();
        let new_did = test_did();
        let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence);
        let response = service.start_ceremony(request).unwrap();

        // Add attestations
        service
            .add_attestation(
                &response.ceremony_id,
                RecoveryAttestation::support(test_did(), vec![]),
            )
            .unwrap();
        service
            .add_attestation(
                &response.ceremony_id,
                RecoveryAttestation::support(test_did(), vec![]),
            )
            .unwrap();

        // Complete recovery
        service.verify_and_bind(&response.ceremony_id).unwrap();
        let result = service.complete_recovery(&response.ceremony_id).unwrap();

        assert_eq!(result.old_did, old_did);
        assert_eq!(result.new_did, new_did);
        assert_eq!(result.attesters.len(), 2);

        // Old DID should be revoked
        assert!(service.is_revoked(&old_did));
        assert_eq!(service.get_replacement_did(&old_did), Some(&new_did));
    }

    #[test]
    fn test_double_recovery_rejected() {
        let steward_did = test_did();
        let mut service = RecoveryService::new(
            steward_did,
            RecoveryConfig {
                threshold: 2,
                ..Default::default()
            },
        );

        let old_did = test_did();
        let new_did = test_did();

        // First recovery
        let evidence1 = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request1 = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence1);
        let response1 = service.start_ceremony(request1).unwrap();

        service
            .add_attestation(
                &response1.ceremony_id,
                RecoveryAttestation::support(test_did(), vec![]),
            )
            .unwrap();
        service
            .add_attestation(
                &response1.ceremony_id,
                RecoveryAttestation::support(test_did(), vec![]),
            )
            .unwrap();

        service.verify_and_bind(&response1.ceremony_id).unwrap();
        service.complete_recovery(&response1.ceremony_id).unwrap();

        // Second recovery attempt should fail
        let new_did2 = test_did();
        let evidence2 = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request2 = RecoveryRequest::new(old_did, new_did2, evidence2);
        let result = service.start_ceremony(request2);

        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_client() {
        let old_did = test_did();
        let new_did = test_did();
        let mut client = RecoveryClient::new(old_did.clone(), new_did.clone());

        // Generate evidence
        let vui = [42u8; 32];
        let anchor_commitment = [1u8; 32];
        client.generate_evidence(&vui, anchor_commitment);

        // Create request
        let request = client.create_request().unwrap();
        assert_eq!(request.old_did, old_did);
        assert_eq!(request.new_did, new_did);

        // Simulate completion
        let result = RecoveryResult {
            ceremony_id: [0u8; 32],
            old_did: old_did.clone(),
            new_did: new_did.clone(),
            attesters: vec![test_did(), test_did()],
            completed_at: 12345,
        };
        client.set_result(result);

        assert!(client.is_complete());
        assert_eq!(client.result().unwrap().attesters.len(), 2);
    }

    #[test]
    fn test_recovery_stats() {
        let steward_did = test_did();
        let mut service = RecoveryService::new(
            steward_did,
            RecoveryConfig {
                threshold: 1,
                ..Default::default()
            },
        );

        // Start first recovery
        let evidence1 = RecoveryEvidence::new(&[1u8; 32], [1u8; 32]);
        let request1 = RecoveryRequest::new(test_did(), test_did(), evidence1);
        let response1 = service.start_ceremony(request1).unwrap();

        let stats = service.stats();
        assert_eq!(stats.active_ceremonies, 1);

        // Complete it
        service
            .add_attestation(
                &response1.ceremony_id,
                RecoveryAttestation::support(test_did(), vec![]),
            )
            .unwrap();
        service.verify_and_bind(&response1.ceremony_id).unwrap();
        service.complete_recovery(&response1.ceremony_id).unwrap();

        let stats = service.stats();
        assert_eq!(stats.active_ceremonies, 0);
        assert_eq!(stats.successful_recoveries, 1);
        assert_eq!(stats.revoked_dids, 1);
    }

    #[test]
    fn test_revocation_record() {
        let steward_did = test_did();
        let mut service = RecoveryService::new(
            steward_did,
            RecoveryConfig {
                threshold: 1,
                ..Default::default()
            },
        );

        let old_did = test_did();
        let new_did = test_did();
        let evidence = RecoveryEvidence::new(&[42u8; 32], [1u8; 32]);
        let request = RecoveryRequest::new(old_did.clone(), new_did.clone(), evidence);
        let response = service.start_ceremony(request).unwrap();

        service
            .add_attestation(
                &response.ceremony_id,
                RecoveryAttestation::support(test_did(), vec![]),
            )
            .unwrap();
        service.verify_and_bind(&response.ceremony_id).unwrap();
        service.complete_recovery(&response.ceremony_id).unwrap();

        let revocation = service.get_revocation(&old_did).unwrap();
        assert_eq!(revocation.old_did, old_did);
        assert_eq!(revocation.new_did, new_did);
        assert_eq!(revocation.ceremony_id, response.ceremony_id);
    }
}
