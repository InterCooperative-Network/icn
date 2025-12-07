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
    pub fn new_with_storage(data_dir: std::path::PathBuf) -> Self {
        let store_path = data_dir.join("federation_store");
        let store =
            Arc::new(SledStore::open(&store_path).expect("Failed to open federation store"));
        let attestation_store = AttestationStore::new(store.clone());

        Self {
            store,
            registry: RwLock::new(None),
            attestation_store,
            clearing_manager: RwLock::new(None),
        }
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
}
