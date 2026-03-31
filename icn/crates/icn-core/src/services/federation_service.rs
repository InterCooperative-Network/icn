//! Federation service adapter - implements kernel-api FederationService.
//!
//! This module bridges the kernel-safe API to the actual icn-federation
//! CooperativeRegistry. It translates kernel DTO types to federation types
//! and maintains provenance tracking for governance decisions.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use icn_federation::types::{CooperativeInfo, FederationPolicy, Vouch};
use icn_federation::{BilateralClearingAgreement, ClearingManager, CooperativeRegistry};
use icn_identity::Did;
use icn_kernel_api::{
    FederationClearingRequest, FederationClearingResult, FederationJoinRequest,
    FederationJoinResult, FederationService, FederationVouchRequest, FederationVouchResult,
};
use sha2::{Digest, Sha256};
use tracing::info;

/// Provenance record for a federation operation.
#[derive(Debug, Clone)]
struct FederationProvenance {
    decision_receipt_id: String,
    decision_hash: String,
}

/// Adapter implementing `FederationService` using `CooperativeRegistry` and `ClearingManager`.
pub struct FederationServiceImpl {
    /// The underlying federation registry
    registry: Arc<CooperativeRegistry>,

    /// Clearing manager for bilateral clearing agreements (optional — only present when
    /// federation clearing is configured in the supervisor)
    clearing: Option<Arc<ClearingManager>>,

    /// Provenance tracking for registered cooperatives
    /// Maps coop_did -> (decision_receipt_id, decision_hash)
    provenance: RwLock<HashMap<String, FederationProvenance>>,
}

impl FederationServiceImpl {
    /// Create a new FederationServiceImpl wrapping the given registry.
    pub fn new(registry: Arc<CooperativeRegistry>) -> Self {
        Self {
            registry,
            clearing: None,
            provenance: RwLock::new(HashMap::new()),
        }
    }

    /// Attach a clearing manager, enabling governance-driven clearing establishment.
    pub fn with_clearing_manager(mut self, clearing: Arc<ClearingManager>) -> Self {
        self.clearing = Some(clearing);
        self
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

    /// Compute a state change hash for a clearing establishment.
    fn compute_clearing_hash(request: &FederationClearingRequest) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"federation:clearing:");
        hasher.update(request.coop_a_did.as_bytes());
        hasher.update(b"<->");
        hasher.update(request.coop_b_did.as_bytes());
        hasher.update(b":");
        hasher.update(request.agreement_id.as_bytes());
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
                #[allow(clippy::unwrap_used)]
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

    fn vouch_for_cooperative(
        &self,
        request: FederationVouchRequest,
    ) -> Result<FederationVouchResult> {
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

    fn establish_clearing(
        &self,
        request: FederationClearingRequest,
    ) -> Result<FederationClearingResult> {
        let clearing = match &self.clearing {
            Some(c) => c,
            None => {
                return Ok(FederationClearingResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(
                        "Clearing manager not configured — wire FederationServiceImpl::with_clearing_manager in supervisor"
                            .to_string(),
                    ),
                });
            }
        };

        info!(
            coop_a_did = %request.coop_a_did,
            coop_b_did = %request.coop_b_did,
            agreement_id = %request.agreement_id,
            decision_receipt_id = %request.decision_receipt_id,
            "Establishing bilateral clearing agreement"
        );

        let coop_a_did_parsed = Did::from_str(&request.coop_a_did).unwrap_or_else(|_| {
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
            Did::from_public_key(&signing_key.verifying_key())
        });
        let coop_b_did_parsed = Did::from_str(&request.coop_b_did).unwrap_or_else(|_| {
            let signing_key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
            Did::from_public_key(&signing_key.verifying_key())
        });

        let agreement = BilateralClearingAgreement::new(
            request.agreement_id.clone(),
            request.coop_a_did.clone(),
            coop_a_did_parsed,
            request.coop_b_did.clone(),
            coop_b_did_parsed,
        );

        match clearing.create_agreement(agreement) {
            Ok(agreement_id) => {
                let state_change_hash = Self::compute_clearing_hash(&request);
                info!(
                    agreement_id = %agreement_id,
                    state_change_hash = %state_change_hash,
                    decision_receipt_id = %request.decision_receipt_id,
                    "Bilateral clearing agreement established"
                );
                Ok(FederationClearingResult {
                    success: true,
                    state_change_hash,
                    error: None,
                })
            }
            Err(e) => {
                tracing::warn!(
                    coop_a_did = %request.coop_a_did,
                    coop_b_did = %request.coop_b_did,
                    error = %e,
                    "Failed to establish clearing agreement"
                );
                Ok(FederationClearingResult {
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
        prov.get(coop_did)
            .map(|p| (p.decision_receipt_id.clone(), p.decision_hash.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use icn_store::SledStore;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_test_did() -> Did {
        // Generate a valid DID from a keypair
        let signing_key = SigningKey::from_bytes(&[0u8; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key)
    }

    fn make_own_info() -> CooperativeInfo {
        let own_did = make_test_did();
        CooperativeInfo::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            own_did,
            FederationPolicy::Open,
        )
    }

    /// Create a registry at a given path (for reload testing)
    fn make_registry_at_path(path: &Path) -> Arc<CooperativeRegistry> {
        let store = Arc::new(SledStore::open(path).expect("Failed to open store"));
        Arc::new(CooperativeRegistry::new(store, make_own_info()).unwrap())
    }

    fn make_test_registry() -> (Arc<CooperativeRegistry>, TempDir) {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let registry = make_registry_at_path(temp_dir.path());
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
        assert!(
            !result.state_change_hash.is_empty(),
            "Should have state change hash"
        );
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

    /// Two-phase durability test: write, close, reopen, verify record survives
    #[test]
    fn test_join_federation_survives_reload() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store_path = temp_dir.path().to_path_buf();

        let state_change_hash;
        let federation_id = "reload-test-coop".to_string();
        let decision_receipt_id = "gov:proposal:fed-join:receipt:reload-123".to_string();
        let decision_hash = "sha256:reload-test-hash".to_string();

        // === PHASE A: Write ===
        {
            let registry = make_registry_at_path(&store_path);
            let service = FederationServiceImpl::new(registry.clone());

            let request = FederationJoinRequest {
                coop_did: "did:icn:zReloadTest1234567890123456789".to_string(),
                coop_name: "Reload Test Coop".to_string(),
                federation_id: federation_id.clone(),
                gateway_endpoints: vec!["https://reload.test".to_string()],
                decision_receipt_id: decision_receipt_id.clone(),
                decision_hash: decision_hash.clone(),
            };

            let result = service.join_federation(request).unwrap();
            assert!(result.success, "Join should succeed");
            state_change_hash = result.state_change_hash;
            assert!(
                !state_change_hash.is_empty(),
                "Should have state change hash"
            );

            // Registry and service drop here, simulating process boundary
        }

        // === PHASE B: Reload and verify ===
        {
            let registry_b = make_registry_at_path(&store_path);

            // Verify record exists after reload
            let stored = registry_b.get(&federation_id).unwrap();
            assert!(stored.is_some(), "Cooperative should survive reload");

            let coop_info = stored.unwrap();
            assert_eq!(coop_info.name, "Reload Test Coop", "Name should match");
            assert_eq!(
                coop_info.coop_id, federation_id,
                "Federation ID should match"
            );
            assert!(
                coop_info
                    .gateway_endpoints
                    .contains(&"https://reload.test".to_string()),
                "Gateway endpoints should survive"
            );

            println!();
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║          FEDERATION RELOAD DURABILITY VERIFIED                   ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!(
                "║ state_change_hash: {:<43} ║",
                &state_change_hash[..43.min(state_change_hash.len())]
            );
            println!("║ federation_id:     {:<43} ║", &federation_id);
            println!("║ coop_name:         {:<43} ║", coop_info.name);
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!();
        }
    }

    /// Two-phase durability test for vouches
    #[test]
    fn test_vouch_survives_reload() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store_path = temp_dir.path().to_path_buf();

        let voucher_id = "voucher-reload".to_string();
        let vouchee_id = "vouchee-reload".to_string();

        // === PHASE A: Write voucher, vouchee, and vouch ===
        {
            let registry = make_registry_at_path(&store_path);
            let service = FederationServiceImpl::new(registry.clone());

            // Register voucher
            service
                .join_federation(FederationJoinRequest {
                    coop_did: "did:icn:zVoucherReload123456789012345".to_string(),
                    coop_name: "Voucher Reload".to_string(),
                    federation_id: voucher_id.clone(),
                    gateway_endpoints: vec![],
                    decision_receipt_id: "gov:join:voucher".to_string(),
                    decision_hash: "sha256:v1".to_string(),
                })
                .unwrap();

            // Register vouchee
            service
                .join_federation(FederationJoinRequest {
                    coop_did: "did:icn:zVoucheeReload123456789012345".to_string(),
                    coop_name: "Vouchee Reload".to_string(),
                    federation_id: vouchee_id.clone(),
                    gateway_endpoints: vec![],
                    decision_receipt_id: "gov:join:vouchee".to_string(),
                    decision_hash: "sha256:v2".to_string(),
                })
                .unwrap();

            // Create vouch
            let result = service
                .vouch_for_cooperative(FederationVouchRequest {
                    voucher_did: voucher_id.clone(),
                    vouchee_did: vouchee_id.clone(),
                    trust_score: 0.75,
                    decision_receipt_id: "gov:vouch:reload-test".to_string(),
                    decision_hash: "sha256:vouch-reload".to_string(),
                })
                .unwrap();

            assert!(result.success, "Vouch should succeed: {:?}", result.error);
        }

        // === PHASE B: Reload and verify vouch survives ===
        {
            let registry_b = make_registry_at_path(&store_path);

            // Verify vouch exists after reload
            let vouches = registry_b.get_vouches(&vouchee_id).unwrap();
            assert!(!vouches.is_empty(), "Vouch should survive reload");
            assert!(
                vouches.contains(&voucher_id),
                "Voucher should be in vouch list: {:?}",
                vouches
            );

            // Verify cooperatives also survived
            assert!(
                registry_b.get(&voucher_id).unwrap().is_some(),
                "Voucher coop should survive"
            );
            assert!(
                registry_b.get(&vouchee_id).unwrap().is_some(),
                "Vouchee coop should survive"
            );

            println!();
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║            VOUCH RELOAD DURABILITY VERIFIED                      ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!("║ voucher: {:<54} ║", &voucher_id);
            println!("║ vouchee: {:<54} ║", &vouchee_id);
            println!("║ vouches_count: {:<48} ║", vouches.len());
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!();
        }
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
        assert!(
            !result.state_change_hash.is_empty(),
            "Should have state change hash"
        );

        // Verify vouch is stored
        let vouches = registry.get_vouches("vouchee-coop").unwrap();
        assert!(!vouches.is_empty(), "Vouch should be stored");
    }

    #[test]
    fn test_establish_clearing_creates_durable_agreement() {
        let (registry, _reg_temp) = make_test_registry();

        // Create a separate store for the clearing manager
        let clearing_temp = tempfile::tempdir().expect("Failed to create clearing temp dir");
        let clearing_store =
            Arc::new(SledStore::open(clearing_temp.path()).expect("Failed to open clearing store"));
        let clearing = Arc::new(
            ClearingManager::new(clearing_store, "coop-alpha".to_string())
                .expect("Failed to create clearing manager"),
        );

        let service = FederationServiceImpl::new(registry).with_clearing_manager(clearing.clone());

        let request = FederationClearingRequest {
            coop_a_did: "did:icn:zCoopAlpha12345678901234567890123".to_string(),
            coop_b_did: "did:icn:zCoopBeta1234567890123456789012345".to_string(),
            agreement_id: "sha256:clearing-agreement-governance-hash".to_string(),
            decision_receipt_id: "gov:proposal:clearing:receipt:test-001".to_string(),
            decision_hash: "sha256:clearing-decision-hash".to_string(),
        };

        let result = service.establish_clearing(request.clone()).unwrap();

        assert!(result.success, "Clearing establishment should succeed");
        assert!(
            !result.state_change_hash.is_empty(),
            "Should have state change hash"
        );
        assert!(result.error.is_none(), "Should have no error");

        // Verify the agreement is persisted in the clearing manager
        let stored = clearing
            .get_agreement(&request.agreement_id)
            .expect("get_agreement failed");
        assert!(
            stored.is_some(),
            "Agreement should be stored in clearing manager"
        );
        let agreement = stored.unwrap();
        assert_eq!(agreement.agreement_id, request.agreement_id);
        assert_eq!(agreement.coop_a, request.coop_a_did);
        assert_eq!(agreement.coop_b, request.coop_b_did);
    }

    #[test]
    fn test_establish_clearing_without_manager_returns_failure() {
        let (registry, _temp) = make_test_registry();
        // Deliberately do NOT attach a clearing manager
        let service = FederationServiceImpl::new(registry);

        let request = FederationClearingRequest {
            coop_a_did: "did:icn:zCoopA12345678901234567890".to_string(),
            coop_b_did: "did:icn:zCoopB12345678901234567890".to_string(),
            agreement_id: "sha256:no-manager-test".to_string(),
            decision_receipt_id: "gov:proposal:clearing:no-manager".to_string(),
            decision_hash: "sha256:no-manager".to_string(),
        };

        let result = service.establish_clearing(request).unwrap();

        assert!(
            !result.success,
            "Should fail without a clearing manager configured"
        );
        assert!(
            result.error.is_some(),
            "Should report an error explaining why"
        );
    }
}
