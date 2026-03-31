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
use icn_kernel_api::services::TreasuryOperationType;
use icn_kernel_api::{
    FederationClearingRequest, FederationClearingResult, FederationClearingSettleRequest,
    FederationClearingSettleResult, FederationJoinRequest, FederationJoinResult, FederationService,
    FederationVouchRequest, FederationVouchResult, LedgerService, TreasuryEntryRequest,
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

    /// Ledger service for emitting settlement transfers (optional — required for non-zero
    /// net settlement to produce ledger entries)
    ledger: Option<Arc<dyn LedgerService>>,

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
            ledger: None,
            provenance: RwLock::new(HashMap::new()),
        }
    }

    /// Attach a clearing manager, enabling governance-driven clearing establishment.
    pub fn with_clearing_manager(mut self, clearing: Arc<ClearingManager>) -> Self {
        self.clearing = Some(clearing);
        self
    }

    /// Attach a ledger service so that non-zero clearing settlements produce ledger entries.
    ///
    /// Required for the operational clearing scheduler and governance force-settle paths
    /// to write net obligations into the ledger.
    pub fn with_ledger(mut self, ledger: Arc<dyn LedgerService>) -> Self {
        self.ledger = Some(ledger);
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

        let coop_a_did_parsed = match Did::from_str(&request.coop_a_did) {
            Ok(did) => did,
            Err(e) => {
                return Ok(FederationClearingResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(format!("Invalid coop_a_did '{}': {e}", request.coop_a_did)),
                });
            }
        };
        let coop_b_did_parsed = match Did::from_str(&request.coop_b_did) {
            Ok(did) => did,
            Err(e) => {
                return Ok(FederationClearingResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(format!("Invalid coop_b_did '{}': {e}", request.coop_b_did)),
                });
            }
        };

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

    fn settle_clearing(
        &self,
        request: FederationClearingSettleRequest,
    ) -> Result<FederationClearingSettleResult> {
        let clearing = match &self.clearing {
            Some(c) => c,
            None => {
                return Ok(FederationClearingSettleResult {
                    success: false,
                    net_settlement: 0,
                    coop_a: String::new(),
                    coop_b: String::new(),
                    transfers_settled: 0,
                    state_change_hash: String::new(),
                    ledger_entry_hash: None,
                    error: Some(
                        "Clearing manager not configured — wire FederationServiceImpl::with_clearing_manager in supervisor"
                            .to_string(),
                    ),
                });
            }
        };

        // Look up the agreement to get coop identifiers for the ledger transfer
        let agreement = match clearing.get_agreement(&request.agreement_id)? {
            Some(a) => a,
            None => {
                return Ok(FederationClearingSettleResult {
                    success: false,
                    net_settlement: 0,
                    coop_a: String::new(),
                    coop_b: String::new(),
                    transfers_settled: 0,
                    state_change_hash: String::new(),
                    ledger_entry_hash: None,
                    error: Some(format!(
                        "Clearing agreement '{}' not found",
                        request.agreement_id
                    )),
                });
            }
        };

        let coop_a = agreement.coop_a.clone();
        let coop_b = agreement.coop_b.clone();
        // Store DIDs as strings for the ledger transfer — the ledger's Transfer path
        // parses recipient via Did::from_str(), so we must pass the DID, not the coop ID.
        let coop_a_did_str = agreement.coop_a_did.to_string();
        let coop_b_did_str = agreement.coop_b_did.to_string();

        // Idempotency guard: if net is non-zero and no ledger is configured, fail BEFORE
        // mutating clearing positions.  Otherwise trigger_settlement() would clear positions
        // leaving no accounting trace (partial-settlement state).
        if self.ledger.is_none() {
            let preview_net = clearing
                .calculate_position(&request.agreement_id)
                .map(|p| p.net_position())
                .unwrap_or(0);
            if preview_net != 0 {
                return Ok(FederationClearingSettleResult {
                    success: false,
                    net_settlement: 0,
                    coop_a,
                    coop_b,
                    transfers_settled: 0,
                    state_change_hash: String::new(),
                    ledger_entry_hash: None,
                    error: Some(format!(
                        "Cannot settle agreement '{}': net position is {} but no ledger configured — wire with_ledger()",
                        request.agreement_id,
                        preview_net
                    )),
                });
            }
        }

        info!(
            agreement_id = %request.agreement_id,
            currency = %request.currency,
            coop_a = %coop_a,
            coop_b = %coop_b,
            decision_receipt_id = %request.decision_receipt_id,
            "Triggering clearing settlement"
        );

        match clearing.trigger_settlement(&request.agreement_id) {
            Ok(report) => {
                let mut hasher = Sha256::new();
                hasher.update(b"federation:settle:");
                hasher.update(request.agreement_id.as_bytes());
                hasher.update(b":");
                hasher.update(request.decision_receipt_id.as_bytes());
                hasher.update(b":");
                hasher.update(report.net_settlement.to_le_bytes());
                let state_change_hash = format!("{:x}", hasher.finalize());

                info!(
                    agreement_id = %report.agreement_id,
                    net_settlement = %report.net_settlement,
                    transfers_settled = %report.transfers_settled,
                    state_change_hash = %state_change_hash,
                    "Clearing settlement completed"
                );

                // Emit ledger transfer for non-zero net positions.
                // net_settlement > 0: coop_a owes coop_b
                // net_settlement < 0: coop_b owes coop_a
                // We use DID strings (not coop ID strings) because the ledger's Transfer
                // path parses treasury_id and recipient via Did::from_str().
                let ledger_entry_hash = if report.net_settlement != 0 {
                    if let Some(ledger) = &self.ledger {
                        let (debtor_did, creditor_did, debtor_label, creditor_label, amount) =
                            if report.net_settlement > 0 {
                                (
                                    coop_a_did_str.clone(),
                                    coop_b_did_str.clone(),
                                    coop_a.clone(),
                                    coop_b.clone(),
                                    report.net_settlement,
                                )
                            } else {
                                (
                                    coop_b_did_str.clone(),
                                    coop_a_did_str.clone(),
                                    coop_b.clone(),
                                    coop_a.clone(),
                                    -report.net_settlement,
                                )
                            };
                        let entry_req = TreasuryEntryRequest {
                            treasury_id: debtor_did.clone(),
                            operation_type: TreasuryOperationType::Transfer,
                            amount,
                            currency: request.currency.clone(),
                            recipient: Some(creditor_did.clone()),
                            memo: format!(
                                "Clearing settlement for {}: {} -> {}",
                                request.agreement_id, debtor_label, creditor_label
                            ),
                            expected_nonce: None,
                            decision_receipt_id: request.decision_receipt_id.clone(),
                            decision_hash: request.decision_hash.clone(),
                        };
                        match ledger.submit_treasury_entry(entry_req) {
                            Ok(entry_result) => {
                                info!(
                                    agreement_id = %request.agreement_id,
                                    net_settlement = report.net_settlement,
                                    entry_hash = %entry_result.entry_hash,
                                    "Clearing settlement produced ledger transfer entry"
                                );
                                Some(entry_result.entry_hash)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    agreement_id = %request.agreement_id,
                                    error = %e,
                                    "Clearing settlement succeeded but ledger transfer failed"
                                );
                                return Ok(FederationClearingSettleResult {
                                    success: false,
                                    net_settlement: report.net_settlement,
                                    coop_a,
                                    coop_b,
                                    transfers_settled: report.transfers_settled,
                                    state_change_hash,
                                    ledger_entry_hash: None,
                                    error: Some(format!(
                                        "Settlement positions cleared but ledger transfer failed: {e}"
                                    )),
                                });
                            }
                        }
                    } else {
                        tracing::warn!(
                            agreement_id = %request.agreement_id,
                            net_settlement = report.net_settlement,
                            "Non-zero net settlement but no ledger configured — wire with_ledger()"
                        );
                        None
                    }
                } else {
                    None
                };

                Ok(FederationClearingSettleResult {
                    success: true,
                    net_settlement: report.net_settlement,
                    coop_a,
                    coop_b,
                    transfers_settled: report.transfers_settled,
                    state_change_hash,
                    ledger_entry_hash,
                    error: None,
                })
            }
            Err(e) => {
                tracing::warn!(
                    agreement_id = %request.agreement_id,
                    error = %e,
                    "Clearing settlement failed"
                );
                Ok(FederationClearingSettleResult {
                    success: false,
                    net_settlement: 0,
                    coop_a,
                    coop_b,
                    transfers_settled: 0,
                    state_change_hash: String::new(),
                    ledger_entry_hash: None,
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

    /// Generate a valid DID string from a seed byte.
    fn make_test_did_str(seed: u8) -> String {
        let signing_key = SigningKey::from_bytes(&[seed; 32]);
        let verifying_key = signing_key.verifying_key();
        Did::from_public_key(&verifying_key).to_string()
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
            coop_a_did: make_test_did_str(2),
            coop_b_did: make_test_did_str(3),
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

    #[test]
    fn test_establish_clearing_fails_on_invalid_did() {
        let (registry, _reg_temp) = make_test_registry();
        let clearing_temp = tempfile::tempdir().expect("Failed to create clearing temp dir");
        let clearing_store =
            Arc::new(SledStore::open(clearing_temp.path()).expect("Failed to open clearing store"));
        let clearing = Arc::new(
            ClearingManager::new(clearing_store, "coop-alpha".to_string())
                .expect("Failed to create clearing manager"),
        );
        let service = FederationServiceImpl::new(registry).with_clearing_manager(clearing.clone());

        // Invalid coop_a_did
        let result_a = service
            .establish_clearing(FederationClearingRequest {
                coop_a_did: "not-a-did".to_string(),
                coop_b_did: make_test_did_str(3),
                agreement_id: "sha256:invalid-a".to_string(),
                decision_receipt_id: "gov:test".to_string(),
                decision_hash: "sha256:test".to_string(),
            })
            .unwrap();
        assert!(!result_a.success, "Invalid coop_a_did should fail");
        assert!(
            result_a
                .error
                .as_deref()
                .unwrap_or("")
                .contains("coop_a_did"),
            "Error should mention coop_a_did: {:?}",
            result_a.error
        );

        // Invalid coop_b_did
        let result_b = service
            .establish_clearing(FederationClearingRequest {
                coop_a_did: make_test_did_str(2),
                coop_b_did: "also-not-a-did".to_string(),
                agreement_id: "sha256:invalid-b".to_string(),
                decision_receipt_id: "gov:test".to_string(),
                decision_hash: "sha256:test".to_string(),
            })
            .unwrap();
        assert!(!result_b.success, "Invalid coop_b_did should fail");
        assert!(
            result_b
                .error
                .as_deref()
                .unwrap_or("")
                .contains("coop_b_did"),
            "Error should mention coop_b_did: {:?}",
            result_b.error
        );
    }

    #[test]
    fn test_settle_clearing_zero_net_position() {
        let (registry, _reg_temp) = make_test_registry();
        let clearing_temp = tempfile::tempdir().unwrap();
        let clearing_store = Arc::new(SledStore::open(clearing_temp.path()).unwrap());
        let clearing =
            Arc::new(ClearingManager::new(clearing_store, "coop-a".to_string()).unwrap());

        // First establish the agreement
        let service = FederationServiceImpl::new(registry).with_clearing_manager(clearing.clone());
        let establish = service
            .establish_clearing(FederationClearingRequest {
                coop_a_did: make_test_did_str(2),
                coop_b_did: make_test_did_str(3),
                agreement_id: "agreement-settle-zero".to_string(),
                decision_receipt_id: "gov:establish-clearing".to_string(),
                decision_hash: "sha256:establish".to_string(),
            })
            .unwrap();
        assert!(establish.success, "EstablishClearing should succeed");

        // Settle with no transfers — net position is zero
        let result = service
            .settle_clearing(FederationClearingSettleRequest {
                agreement_id: "agreement-settle-zero".to_string(),
                currency: "HOURS".to_string(),
                decision_receipt_id: "gov:settle-clearing".to_string(),
                decision_hash: "sha256:settle".to_string(),
            })
            .unwrap();

        assert!(
            result.success,
            "Settlement should succeed: {:?}",
            result.error
        );
        assert_eq!(result.net_settlement, 0, "No transfers → zero net position");
        assert_eq!(result.transfers_settled, 0);
        assert!(!result.state_change_hash.is_empty());
    }

    /// Non-zero net settlement without a ledger service must fail BEFORE mutating
    /// clearing positions (idempotency guard).  The clearing positions should be
    /// unchanged after the call so a retry with a ledger wired will work correctly.
    #[test]
    fn test_settle_clearing_nonzero_net_requires_ledger() {
        use icn_federation::clearing::CrossCoopTransfer;

        let (registry, _reg_temp) = make_test_registry();
        let clearing_temp = tempfile::tempdir().unwrap();
        let clearing_store = Arc::new(SledStore::open(clearing_temp.path()).unwrap());
        let clearing =
            Arc::new(ClearingManager::new(clearing_store, "coop-a".to_string()).unwrap());

        let coop_a_did = Did::from_public_key(
            &ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]).verifying_key(),
        );
        let coop_b_did = Did::from_public_key(
            &ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]).verifying_key(),
        );
        let agreement_id = "agreement-settle-nonzero".to_string();

        // Establish the agreement with an HOURS→HOURS exchange rate
        clearing
            .create_agreement(
                BilateralClearingAgreement::new(
                    agreement_id.clone(),
                    "coop-a".to_string(),
                    coop_a_did.clone(),
                    "coop-b".to_string(),
                    coop_b_did.clone(),
                )
                .with_rate("HOURS", "HOURS", 1.0),
            )
            .unwrap();

        // Propose and confirm a transfer: coop-a sends 100 HOURS to coop-b
        let transfer = CrossCoopTransfer::new(
            "transfer-001".to_string(),
            "coop-a".to_string(),
            coop_a_did.clone(),
            "coop-b".to_string(),
            coop_b_did.clone(),
            100,
            "HOURS".to_string(),
            100,
            "HOURS".to_string(),
        );
        let transfer_id = clearing.propose_transfer(transfer).unwrap();
        clearing.confirm_transfer(&transfer_id).unwrap();

        // Service without ledger — must fail on non-zero net to prevent partial settlement
        let service = FederationServiceImpl::new(registry).with_clearing_manager(clearing.clone());

        let result = service
            .settle_clearing(FederationClearingSettleRequest {
                agreement_id: agreement_id.clone(),
                currency: "HOURS".to_string(),
                decision_receipt_id: "gov:settle-nonzero".to_string(),
                decision_hash: "sha256:settle-nonzero".to_string(),
            })
            .unwrap();

        assert!(!result.success, "Non-zero net without ledger must fail");
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("no ledger configured"),
            "Error should mention ledger requirement: {:?}",
            result.error
        );

        // Verify positions were NOT mutated (idempotency guard fired before trigger_settlement)
        let position = clearing.calculate_position(&agreement_id).unwrap();
        assert_eq!(
            position.net_position(),
            100,
            "Positions must be unchanged after guard fires"
        );
    }

    #[test]
    fn test_settle_clearing_agreement_not_found() {
        let (registry, _reg_temp) = make_test_registry();
        let clearing_temp = tempfile::tempdir().unwrap();
        let clearing_store = Arc::new(SledStore::open(clearing_temp.path()).unwrap());
        let clearing =
            Arc::new(ClearingManager::new(clearing_store, "coop-a".to_string()).unwrap());
        let service = FederationServiceImpl::new(registry).with_clearing_manager(clearing);

        let result = service
            .settle_clearing(FederationClearingSettleRequest {
                agreement_id: "no-such-agreement".to_string(),
                currency: "HOURS".to_string(),
                decision_receipt_id: "gov:settle-missing".to_string(),
                decision_hash: "sha256:missing".to_string(),
            })
            .unwrap();

        assert!(!result.success, "Should fail for unknown agreement");
        assert!(
            result.error.as_deref().unwrap_or("").contains("not found"),
            "Error should mention not found: {:?}",
            result.error
        );
    }
}
