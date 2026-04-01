//! Federation Manager
//!
//! Wraps federation-related operations for the gateway API.

#[cfg(test)]
use icn_federation::FederationPolicy;
use icn_federation::{
    AttestationStore, BilateralClearingAgreement, ClearingManager, ClearingPosition,
    CooperativeInfo, CooperativeRegistry, FederatedTrustAttestation, SettlementReport, Vouch,
};
use icn_identity::Did;
use icn_store::SledStore;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::error::{GatewayError, Result};

/// Manager for federation operations
pub struct FederationManager {
    store: Arc<dyn icn_store::Store>,
    registry: RwLock<Option<CooperativeRegistry>>,
    attestation_store: AttestationStore,
    clearing_manager: RwLock<Option<ClearingManager>>,
}

impl FederationManager {
    /// Create a new federation manager with temporary storage (for testing)
    pub fn new() -> Self {
        // SAFETY: SledStore::temporary() only fails on I/O errors which are unrecoverable
        #[allow(clippy::expect_used)]
        let store = Arc::new(SledStore::temporary().expect("Failed to create temp store"));
        let attestation_store = AttestationStore::new(store.clone());

        Self {
            store,
            registry: RwLock::new(None),
            attestation_store,
            clearing_manager: RwLock::new(None),
        }
    }

    /// Create a new federation manager with persistent storage
    pub fn new_with_storage(data_dir: std::path::PathBuf) -> Result<Self> {
        let store_path = data_dir.join("federation_store");
        let store = Arc::new(SledStore::open(&store_path).map_err(|e| {
            GatewayError::InternalError(format!("Failed to open federation store: {e}"))
        })?);
        let attestation_store = AttestationStore::new(store.clone());

        Ok(Self {
            store,
            registry: RwLock::new(None),
            attestation_store,
            clearing_manager: RwLock::new(None),
        })
    }

    /// Initialize registry with cooperative info
    pub async fn init_registry(&self, own_info: CooperativeInfo) -> Result<()> {
        let coop_id = own_info.coop_id.clone();
        let registry = CooperativeRegistry::new(self.store.clone(), own_info.clone())
            .map_err(|e| GatewayError::InternalError(format!("Failed to init registry: {e}")))?;

        let clearing = ClearingManager::new(self.store.clone(), coop_id.clone())
            .map_err(|e| GatewayError::InternalError(format!("Failed to init clearing: {e}")))?;

        *self.registry.write().await = Some(registry);
        *self.clearing_manager.write().await = Some(clearing);

        info!(
            "Federation manager initialized for cooperative: {}",
            coop_id
        );
        Ok(())
    }

    // ========================================================================
    // Cooperative Registry Operations
    // ========================================================================

    /// List all known cooperatives
    pub async fn list_cooperatives(&self) -> Result<Vec<CooperativeInfo>> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        registry
            .list()
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Get a specific cooperative
    pub async fn get_cooperative(&self, coop_id: &str) -> Result<Option<CooperativeInfo>> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        registry
            .get(coop_id)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Register a new cooperative
    pub async fn register_cooperative(&self, info: CooperativeInfo) -> Result<()> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        registry
            .register(info)
            .map_err(|e| GatewayError::BadRequest(e.to_string()))
    }

    /// Get our own cooperative info
    pub async fn get_own_info(&self) -> Result<CooperativeInfo> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        Ok(registry.own_coop_info())
    }

    /// Check if a cooperative is federated
    pub async fn is_federated(&self, coop_id: &str) -> Result<bool> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        Ok(registry.is_federated(coop_id))
    }

    // ========================================================================
    // Vouch Operations
    // ========================================================================

    /// Add a vouch for a cooperative
    pub async fn add_vouch(&self, vouch: &Vouch) -> Result<()> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        registry
            .add_vouch(vouch)
            .map_err(|e| GatewayError::BadRequest(e.to_string()))
    }

    /// Get vouches for a cooperative
    pub async fn get_vouches(&self, coop_id: &str) -> Result<Vec<String>> {
        let registry = self.registry.read().await;
        let registry = registry
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        registry
            .get_vouches(coop_id)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    // ========================================================================
    // Attestation Operations
    // ========================================================================

    /// Store an attestation
    pub async fn store_attestation(&self, attestation: FederatedTrustAttestation) -> Result<()> {
        self.attestation_store
            .store_attestation(attestation)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Get attestations for a member
    pub async fn get_attestations_for(
        &self,
        member: &Did,
    ) -> Result<Vec<FederatedTrustAttestation>> {
        self.attestation_store
            .get_valid_attestations_for(member)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Get attestations from a cooperative
    pub async fn get_attestations_from(
        &self,
        coop_id: &str,
    ) -> Result<Vec<FederatedTrustAttestation>> {
        self.attestation_store
            .get_attestations_from(coop_id)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    // ========================================================================
    // Clearing Operations
    // ========================================================================

    /// List clearing agreements
    pub async fn list_agreements(&self) -> Result<Vec<BilateralClearingAgreement>> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        Ok(clearing.list_agreements())
    }

    /// Get a clearing agreement
    pub async fn get_agreement(
        &self,
        agreement_id: &str,
    ) -> Result<Option<BilateralClearingAgreement>> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        clearing
            .get_agreement(agreement_id)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Create a clearing agreement
    pub async fn create_agreement(&self, agreement: BilateralClearingAgreement) -> Result<String> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        clearing
            .create_agreement(agreement)
            .map_err(|e| GatewayError::BadRequest(e.to_string()))
    }

    /// Get clearing position
    pub async fn get_position(&self, agreement_id: &str) -> Result<ClearingPosition> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        clearing
            .calculate_position(agreement_id)
            .map_err(|e| GatewayError::NotFound(e.to_string()))
    }

    /// Trigger settlement
    pub async fn settle(&self, agreement_id: &str) -> Result<SettlementReport> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        clearing
            .trigger_settlement(agreement_id)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Process all scheduled settlements
    pub async fn process_scheduled_settlements(&self) -> Result<Vec<SettlementReport>> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        Ok(clearing.process_scheduled_settlements())
    }

    /// Perform multilateral netting for a currency (read-only analysis)
    pub async fn perform_multilateral_netting(
        &self,
        currency: &str,
    ) -> Result<icn_federation::netting::NettingResult> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        clearing
            .perform_multilateral_netting(currency)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }

    /// Apply multilateral netting results to positions
    pub async fn apply_multilateral_netting(
        &self,
        result: &icn_federation::netting::NettingResult,
    ) -> Result<()> {
        let clearing = self.clearing_manager.read().await;
        let clearing = clearing
            .as_ref()
            .ok_or_else(|| GatewayError::BadRequest("Federation not initialized".to_string()))?;

        clearing
            .apply_multilateral_netting(result)
            .map_err(|e| GatewayError::InternalError(e.to_string()))
    }
}

impl Default for FederationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[tokio::test]
    async fn test_federation_manager_init() {
        let manager = FederationManager::new();

        let own_info = CooperativeInfo::new(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            test_did(),
            FederationPolicy::default(),
        );

        manager.init_registry(own_info).await.unwrap();

        let info = manager.get_own_info().await.unwrap();
        assert_eq!(info.coop_id, "test-coop");
    }

    #[tokio::test]
    async fn test_list_cooperatives_before_init() {
        let manager = FederationManager::new();

        let result = manager.list_cooperatives().await;
        assert!(result.is_err());
    }

    /// Gateway API federation state must survive across restarts when using persistent storage.
    ///
    /// Architectural invariant (ADR 0011 / Option B): the gateway's own FederationManager
    /// serves as the truth for API-originated federation state (agreements created via direct
    /// API, not governance effects). This state must be durable — ephemeral temp store was the
    /// root cause of the state-loss gap identified in the audit.
    ///
    /// This test proves that `new_with_storage()` gives independent manager instances access
    /// to the same persisted cooperative registry data — simulating a restart scenario.
    #[tokio::test]
    async fn test_persistent_storage_survives_manager_reconstruction() {
        let data_dir = tempfile::tempdir().expect("tempdir");
        let path = data_dir.path().to_path_buf();

        // First instance: create and initialize
        {
            let mgr = FederationManager::new_with_storage(path.clone()).expect("new_with_storage");
            let own_info = CooperativeInfo::new(
                "alpha-coop".to_string(),
                "Alpha Cooperative".to_string(),
                test_did(),
                FederationPolicy::default(),
            );
            mgr.init_registry(own_info).await.unwrap();
            let peer = CooperativeInfo::new(
                "beta-coop".to_string(),
                "Beta Cooperative".to_string(),
                test_did(),
                FederationPolicy::default(),
            );
            mgr.register_cooperative(peer).await.unwrap();
        }

        // Second instance at the same path — simulates daemon restart
        {
            let mgr2 = FederationManager::new_with_storage(path.clone())
                .expect("new_with_storage after restart");
            // The registry is lazily loaded — re-init from the same store
            let own_info = CooperativeInfo::new(
                "alpha-coop".to_string(),
                "Alpha Cooperative".to_string(),
                test_did(),
                FederationPolicy::default(),
            );
            mgr2.init_registry(own_info).await.unwrap();

            let coops = mgr2.list_cooperatives().await.unwrap();
            let found_beta = coops.iter().any(|c| c.coop_id == "beta-coop");
            assert!(
                found_beta,
                "beta-coop registered in first instance must be visible after restart"
            );
        }
    }

    /// Gateway coop list reflects ONLY gateway-registered cooperatives — not governance state.
    ///
    /// Architectural invariant (ADR 0012 / Model C): the two federation origin paths are
    /// intentionally isolated. `FederationManager::list_cooperatives()` returns only state
    /// written via the gateway API path. Governance-registered cooperatives (written by
    /// `FederationServiceImpl`) are invisible here — this is correct, not a bug.
    ///
    /// If this test starts failing because `list_cooperatives` returns cooperatives registered
    /// through other means, the parallel model has been violated and ADR 0012 requires review.
    #[tokio::test]
    async fn test_gateway_coop_list_reflects_only_gateway_registered_coops() {
        let mgr = FederationManager::new();

        let own_info = CooperativeInfo::new(
            "gateway-coop".to_string(),
            "Gateway Cooperative".to_string(),
            test_did(),
            FederationPolicy::default(),
        );
        mgr.init_registry(own_info).await.unwrap();

        // Register one peer via the gateway path
        let gateway_peer = CooperativeInfo::new(
            "gateway-peer".to_string(),
            "Gateway Peer".to_string(),
            test_did(),
            FederationPolicy::default(),
        );
        mgr.register_cooperative(gateway_peer).await.unwrap();

        let coops = mgr.list_cooperatives().await.unwrap();

        // Exactly gateway-registered coops visible — no governance-originated coops
        let coop_ids: Vec<&str> = coops.iter().map(|c| c.coop_id.as_str()).collect();
        assert!(
            coop_ids.contains(&"gateway-coop") || coop_ids.contains(&"gateway-peer"),
            "Expected gateway-registered coops to be visible"
        );
        // The list should only contain what was registered via this manager instance
        assert!(
            coops.len() <= 2,
            "Expected at most 2 coops (own + 1 peer); got {} — governance state may have leaked in",
            coops.len()
        );
    }

    /// Gateway clearing agreements are stored in a separate store from governance-originated ones.
    ///
    /// Architectural invariant (ADR 0012 / Model C): agreements created via `POST /federation/clearing`
    /// live in the gateway's FederationManager store. Governance-established clearing agreements
    /// live in the supervisor's ClearingManager. These stores are intentionally isolated.
    ///
    /// This test verifies that the gateway's ClearingManager only surfaces gateway-created agreements.
    /// If it starts returning supervisor-state agreements, the parallel model has been violated.
    #[tokio::test]
    async fn test_gateway_clearing_list_does_not_surface_supervisor_agreements() {
        let mgr = FederationManager::new();

        let own_info = CooperativeInfo::new(
            "alpha-coop".to_string(),
            "Alpha Cooperative".to_string(),
            test_did(),
            FederationPolicy::default(),
        );
        mgr.init_registry(own_info).await.unwrap();

        // No agreements created — list should be empty
        let agreements = mgr.list_agreements().await.unwrap();
        assert!(
            agreements.is_empty(),
            "Expected no gateway-created agreements in a fresh manager; supervisor state must not leak"
        );

        // Create one gateway-path agreement
        let partner_did = test_did();
        use icn_federation::{BilateralClearingAgreement, SettlementInterval};
        let mut agr = BilateralClearingAgreement::new(
            "agr-gw-001".to_string(),
            "alpha-coop".to_string(),
            test_did(),
            "beta-coop".to_string(),
            partner_did,
        );
        agr.settlement_interval = SettlementInterval::Manual;
        let id = mgr.create_agreement(agr).await.unwrap();
        assert_eq!(id, "agr-gw-001");

        let agreements_after = mgr.list_agreements().await.unwrap();
        assert_eq!(
            agreements_after.len(),
            1,
            "Expected exactly 1 agreement (gateway-created); got {}",
            agreements_after.len()
        );
        assert_eq!(agreements_after[0].agreement_id, "agr-gw-001");
    }
}
