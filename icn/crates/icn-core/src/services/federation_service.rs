//! Federation service adapter - implements kernel-api FederationService.
//!
//! This module bridges the kernel-safe API to the actual icn-federation
//! CooperativeRegistry. It translates kernel DTO types to federation types
//! and maintains provenance tracking for governance decisions.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use icn_federation::types::{CooperativeInfo, FederationPolicy, Vouch};
use icn_federation::CooperativeRegistry;
use icn_identity::Did;
use icn_kernel_api::{
    FederationJoinRequest, FederationJoinResult, FederationService, FederationVouchRequest,
    FederationVouchResult,
};
use sha2::{Digest, Sha256};
use tracing::info;

/// Provenance record for a federation operation.
#[derive(Debug, Clone)]
struct FederationProvenance {
    decision_receipt_id: String,
    decision_hash: String,
}

/// Adapter implementing `FederationService` using `CooperativeRegistry`.
pub struct FederationServiceImpl {
    /// The underlying federation registry
    registry: Arc<CooperativeRegistry>,

    /// Provenance tracking for registered cooperatives
    /// Maps coop_did -> (decision_receipt_id, decision_hash)
    provenance: RwLock<HashMap<String, FederationProvenance>>,
}

impl FederationServiceImpl {
    /// Create a new FederationServiceImpl wrapping the given registry.
    pub fn new(registry: Arc<CooperativeRegistry>) -> Self {
        Self {
            registry,
            provenance: RwLock::new(HashMap::new()),
        }
    }

    /// Compute a state change hash for a join operation.
    fn compute_join_hash(request: &FederationJoinRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"federation:join:");
        hasher.update(request.coop_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.federation_id.as_bytes());
        hasher.update(b":");
        hasher.update(request.decision_receipt_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Compute a state change hash for a vouch operation.
    fn compute_vouch_hash(request: &FederationVouchRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"federation:vouch:");
        hasher.update(request.voucher_did.as_bytes());
        hasher.update(b"->");
        hasher.update(request.vouchee_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.decision_receipt_id.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl FederationService for FederationServiceImpl {
    fn join_federation(&self, request: FederationJoinRequest) -> Result<FederationJoinResult> {
        info!(
            coop_did = %request.coop_did,
            federation_id = %request.federation_id,
            decision_receipt_id = %request.decision_receipt_id,
            decision_hash = %request.decision_hash,
            "Processing federation join request"
        );

        // Parse the DID (or generate a placeholder if parsing fails)
        let public_did = Did::from_str(&request.coop_did).unwrap_or_else(|_| {
            // Generate a placeholder DID from a fixed key
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
            Did::from_public_key(&signing_key.verifying_key())
        });

        // Create CooperativeInfo for registration
        let mut coop_info = CooperativeInfo::new(
            request.federation_id.clone(),
            request.coop_name.clone(),
            public_did,
            FederationPolicy::Open,
        );

        // Add gateway endpoints
        for endpoint in &request.gateway_endpoints {
            coop_info = coop_info.with_gateway(endpoint.clone());
        }

        // Register with the registry
        match self.registry.register(coop_info) {
            Ok(()) => {
                let state_change_hash = Self::compute_join_hash(&request);

                // Store provenance
                {
                    let mut prov = self.provenance.write().unwrap();
                    prov.insert(
                        request.coop_did.clone(),
                        FederationProvenance {
                            decision_receipt_id: request.decision_receipt_id.clone(),
                            decision_hash: request.decision_hash.clone(),
                        },
                    );
                }

                info!(
                    coop_did = %request.coop_did,
                    state_change_hash = %state_change_hash,
                    decision_receipt_id = %request.decision_receipt_id,
                    "Cooperative registered in federation"
                );

                Ok(FederationJoinResult {
                    success: true,
                    state_change_hash,
                    error: None,
                })
            }
            Err(e) => {
                tracing::warn!(
                    coop_did = %request.coop_did,
                    error = %e,
                    "Failed to register cooperative"
                );
                Ok(FederationJoinResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                })
            }
        }
    }

    fn vouch_for_cooperative(&self, request: FederationVouchRequest) -> Result<FederationVouchResult> {
        info!(
            voucher_did = %request.voucher_did,
            vouchee_did = %request.vouchee_did,
            trust_score = %request.trust_score,
            decision_receipt_id = %request.decision_receipt_id,
            "Processing federation vouch request"
        );

        // Parse DIDs (or generate a placeholder if parsing fails)
        let voucher_did_parsed = Did::from_str(&request.voucher_did).unwrap_or_else(|_| {
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
            Did::from_public_key(&signing_key.verifying_key())
        });

        // Create Vouch record
        // Note: Vouch expects coop_id (string), not DID directly for target
        let vouch = Vouch::new(
            request.voucher_did.clone(),
            voucher_did_parsed,
            request.vouchee_did.clone(),
            request.trust_score,
        );

        // Add vouch to registry
        match self.registry.add_vouch(&vouch) {
            Ok(()) => {
                let state_change_hash = Self::compute_vouch_hash(&request);

                info!(
                    voucher_did = %request.voucher_did,
                    vouchee_did = %request.vouchee_did,
                    state_change_hash = %state_change_hash,
                    "Vouch recorded in federation registry"
                );

                Ok(FederationVouchResult {
                    success: true,
                    state_change_hash,
                    error: None,
                })
            }
            Err(e) => {
                tracing::warn!(
                    voucher_did = %request.voucher_did,
                    vouchee_did = %request.vouchee_did,
                    error = %e,
                    "Failed to record vouch"
                );
                Ok(FederationVouchResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                })
            }
        }
    }

    fn is_registered(&self, coop_did: &str) -> bool {
        // Check if the DID (as coop_id) is in the registry
        self.registry.get(coop_did).ok().flatten().is_some()
    }

    fn get_registration_provenance(&self, coop_did: &str) -> Option<(String, String)> {
        let prov = self.provenance.read().ok()?;
        prov.get(coop_did).map(|p| {
            (p.decision_receipt_id.clone(), p.decision_hash.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use icn_store::SledStore;
    use tempfile::TempDir;

    fn make_test_did() -> Did {
        // Generate a valid DID from a keypair
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key)
    }

    fn make_test_registry() -> (Arc<CooperativeRegistry>, TempDir) {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store = Arc::new(SledStore::open(temp_dir.path()).expect("Failed to open store"));
        let own_did = make_test_did();
        let own_info = CooperativeInfo::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            own_did,
            FederationPolicy::Open,
        );
        let registry = Arc::new(CooperativeRegistry::new(store, own_info).unwrap());
        (registry, temp_dir)
    }

    #[test]
    fn test_join_federation_creates_durable_record() {
        let (registry, _temp) = make_test_registry();
        let service = FederationServiceImpl::new(registry.clone());

        let request = FederationJoinRequest {
            coop_did: "did:icn:zNewCoop12345678901234567890123456".to_string(),
            coop_name: "New Cooperative".to_string(),
            federation_id: "new-coop".to_string(),
            gateway_endpoints: vec!["https://gateway.newcoop.example".to_string()],
            decision_receipt_id: "gov:proposal:fed-join:receipt:test-123".to_string(),
            decision_hash: "sha256:fedtest12345".to_string(),
        };

        let result = service.join_federation(request.clone()).unwrap();

        // Verify success
        assert!(result.success, "Join should succeed");
        assert!(!result.state_change_hash.is_empty(), "Should have state change hash");
        assert!(result.error.is_none(), "Should have no error");

        // Verify durable record exists
        let stored = registry.get("new-coop").unwrap();
        assert!(stored.is_some(), "Cooperative should be stored in registry");
        assert_eq!(stored.unwrap().name, "New Cooperative");

        // Verify provenance tracking
        let prov = service.get_registration_provenance(&request.coop_did);
        assert!(prov.is_some(), "Provenance should be tracked");
        let (receipt_id, hash) = prov.unwrap();
        assert_eq!(receipt_id, request.decision_receipt_id);
        assert_eq!(hash, request.decision_hash);
    }

    #[test]
    fn test_vouch_creates_durable_record() {
        let (registry, _temp) = make_test_registry();
        let service = FederationServiceImpl::new(registry.clone());

        // First register the voucher (must be a federation partner)
        let voucher_join = FederationJoinRequest {
            coop_did: "did:icn:zVoucher123456789012345678901234".to_string(),
            coop_name: "Voucher Coop".to_string(),
            federation_id: "voucher-coop".to_string(),
            gateway_endpoints: vec![],
            decision_receipt_id: "gov:proposal:join:receipt:v0".to_string(),
            decision_hash: "sha256:join0".to_string(),
        };
        service.join_federation(voucher_join).unwrap();

        // Then register the vouchee
        let vouchee_join = FederationJoinRequest {
            coop_did: "did:icn:zVouchee1234567890123456789012345".to_string(),
            coop_name: "Vouchee Coop".to_string(),
            federation_id: "vouchee-coop".to_string(),
            gateway_endpoints: vec![],
            decision_receipt_id: "gov:proposal:join:receipt:v1".to_string(),
            decision_hash: "sha256:join1".to_string(),
        };
        service.join_federation(vouchee_join).unwrap();

        // Now vouch (voucher must use their coop_id, not DID)
        let vouch_req = FederationVouchRequest {
            voucher_did: "voucher-coop".to_string(), // Use coop_id, not DID
            vouchee_did: "vouchee-coop".to_string(),
            trust_score: 0.8,
            decision_receipt_id: "gov:proposal:vouch:receipt:v2".to_string(),
            decision_hash: "sha256:vouch1".to_string(),
        };

        let result = service.vouch_for_cooperative(vouch_req).unwrap();

        assert!(result.success, "Vouch should succeed: {:?}", result.error);
        assert!(!result.state_change_hash.is_empty(), "Should have state change hash");

        // Verify vouch is stored
        let vouches = registry.get_vouches("vouchee-coop").unwrap();
        assert!(!vouches.is_empty(), "Vouch should be stored");
    }
}
