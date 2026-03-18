//! Federation-related RPC handlers

use std::sync::Arc;

use tracing::info;

use icn_identity::Did;

use crate::auth::RpcTokenClaims;
use crate::server::RpcServer;
use crate::types::RpcResponse;

/// Handle federation.coop.list RPC call - list all known cooperatives
pub async fn handle_federation_coop_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    match registry.list() {
        Ok(coops) => {
            let coop_json: Vec<serde_json::Value> = coops.iter().map(coop_info_to_json).collect();
            RpcResponse::success(id, serde_json::json!({ "cooperatives": coop_json }))
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list cooperatives: {e}")),
    }
}

/// Handle federation.coop.get RPC call - get a specific cooperative
pub async fn handle_federation_coop_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    match registry.get(&params.coop_id) {
        Ok(Some(coop)) => RpcResponse::success(id, coop_info_to_json(&coop)),
        Ok(None) => RpcResponse::error(
            id,
            -32000,
            format!("Cooperative not found: {}", params.coop_id),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get cooperative: {e}")),
    }
}

/// Handle federation.coop.register RPC call - register a new cooperative
pub async fn handle_federation_coop_register(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
        name: String,
        public_did: String,
        #[serde(default)]
        gateway_endpoints: Vec<String>,
        #[serde(default)]
        capabilities: Vec<String>,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Parse DID
    let public_did = match Did::from_str(&params.public_did) {
        Ok(d) => d,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}")),
    };

    // Build cooperative info
    let mut coop_info = icn_federation::CooperativeInfo::new(
        params.coop_id.clone(),
        params.name,
        public_did,
        icn_federation::FederationPolicy::Open,
    );

    for endpoint in params.gateway_endpoints {
        coop_info = coop_info.with_gateway(endpoint);
    }

    for capability in params.capabilities {
        coop_info = coop_info.with_capability(&capability);
    }

    match registry.register(coop_info) {
        Ok(()) => {
            info!("Registered cooperative: {}", params.coop_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "coop_id": params.coop_id,
                    "registered": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to register cooperative: {e}")),
    }
}

/// Handle federation.coop.remove RPC call - remove a cooperative
pub async fn handle_federation_coop_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    match registry.remove(&params.coop_id) {
        Ok(()) => {
            info!("Removed cooperative: {}", params.coop_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "coop_id": params.coop_id,
                    "removed": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to remove cooperative: {e}")),
    }
}

/// Handle federation.own.get RPC call - get own cooperative info
pub async fn handle_federation_own_get(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    let own_info = registry.own_coop_info();
    RpcResponse::success(id, coop_info_to_json(&own_info))
}

/// Handle federation.own.update RPC call - update own cooperative info
pub async fn handle_federation_own_update(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        gateway_endpoints: Option<Vec<String>>,
        #[serde(default)]
        capabilities: Option<Vec<String>>,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Get current own info and update fields
    let mut own_info = registry.own_coop_info();

    if let Some(name) = params.name {
        own_info.name = name;
    }
    if let Some(endpoints) = params.gateway_endpoints {
        own_info.gateway_endpoints = endpoints;
    }
    if let Some(caps) = params.capabilities {
        own_info.capabilities = caps;
    }

    match registry.update_own_info(own_info.clone()) {
        Ok(()) => {
            info!("Updated own cooperative info");
            RpcResponse::success(id, coop_info_to_json(&own_info))
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to update own info: {e}")),
    }
}

/// Handle federation.vouch.list RPC call - list vouches for a cooperative
pub async fn handle_federation_vouch_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    match registry.get_vouches(&params.coop_id) {
        Ok(vouches) => RpcResponse::success(
            id,
            serde_json::json!({
                "coop_id": params.coop_id,
                "vouchers": vouches,
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get vouches: {e}")),
    }
}

/// Handle federation.vouch.issue RPC call - issue a vouch for another cooperative
pub async fn handle_federation_vouch_issue(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        target_coop_id: String,
        #[serde(default = "default_trust_score")]
        trust_score: f64,
    }

    fn default_trust_score() -> f64 {
        0.5
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Get own coop info for the vouch
    let own_info = registry.own_coop_info();

    // Create the vouch
    let vouch = icn_federation::Vouch::new(
        own_info.coop_id.clone(),
        own_info.public_did.clone(),
        params.target_coop_id.clone(),
        params.trust_score,
    );

    // Sign if we have a keypair
    let signed_vouch = if let Some(keypair) = state.own_keypair() {
        vouch.sign(keypair)
    } else {
        vouch
    };

    match registry.add_vouch(&signed_vouch) {
        Ok(()) => {
            info!(
                "Issued vouch for {} with trust score {}",
                params.target_coop_id, params.trust_score
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "voucher_coop_id": own_info.coop_id,
                    "target_coop_id": params.target_coop_id,
                    "trust_score": params.trust_score,
                    "issued": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to issue vouch: {e}")),
    }
}

/// Handle federation.vouch.remove RPC call - remove a vouch
pub async fn handle_federation_vouch_remove(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        target_coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_info = registry.own_coop_info();

    match registry.remove_vouch(&own_info.coop_id, &params.target_coop_id) {
        Ok(()) => {
            info!("Removed vouch for {}", params.target_coop_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "voucher_coop_id": own_info.coop_id,
                    "target_coop_id": params.target_coop_id,
                    "removed": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to remove vouch: {e}")),
    }
}

// ============================================================================
// Attestation handlers
// ============================================================================

/// Handle federation.attestation.list RPC call - list attestations for a member
pub async fn handle_federation_attestation_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::AttestationStore;

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        member_did: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let did = match Did::from_str(&params.member_did) {
        Ok(d) => d,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}")),
    };

    let att_store = AttestationStore::new(store);
    match att_store.get_valid_attestations_for(&did) {
        Ok(attestations) => {
            let atts_json: Vec<serde_json::Value> = attestations
                .iter()
                .map(|att| {
                    serde_json::json!({
                        "source_coop_id": att.source_coop_id,
                        "source_coop_did": att.source_coop_did.to_string(),
                        "member_did": att.member_did.to_string(),
                        "trust_score": att.trust_score,
                        "trust_context": format!("{:?}", att.trust_context),
                        "issued_at": att.issued_at,
                        "expires_at": att.expires_at,
                    })
                })
                .collect();

            RpcResponse::success(
                id,
                serde_json::json!({
                    "member_did": params.member_did,
                    "attestations": atts_json,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get attestations: {e}")),
    }
}

/// Handle federation.attestation.from RPC call - list attestations from a cooperative
pub async fn handle_federation_attestation_from(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::AttestationStore;

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        coop_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let att_store = AttestationStore::new(store);
    match att_store.get_attestations_from(&params.coop_id) {
        Ok(attestations) => {
            let atts_json: Vec<serde_json::Value> = attestations
                .iter()
                .map(|att| {
                    serde_json::json!({
                        "source_coop_id": att.source_coop_id,
                        "source_coop_did": att.source_coop_did.to_string(),
                        "member_did": att.member_did.to_string(),
                        "trust_score": att.trust_score,
                        "trust_context": format!("{:?}", att.trust_context),
                        "issued_at": att.issued_at,
                        "expires_at": att.expires_at,
                    })
                })
                .collect();

            RpcResponse::success(
                id,
                serde_json::json!({
                    "coop_id": params.coop_id,
                    "attestations": atts_json,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get attestations: {e}")),
    }
}

/// Handle federation.attestation.issue RPC call - issue a new attestation
pub async fn handle_federation_attestation_issue(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use icn_federation::{AttestationStore, FederatedTrustAttestation, TrustContext};

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        member_did: String,
        trust_score: f64,
        context: String,
        validity_days: u64,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    // Validate trust score
    if !(0.0..=1.0).contains(&params.trust_score) {
        return RpcResponse::error(
            id,
            -32602,
            "Trust score must be between 0.0 and 1.0".to_string(),
        );
    }

    let did = match Did::from_str(&params.member_did) {
        Ok(d) => d,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}")),
    };

    let trust_context = match params.context.to_lowercase().as_str() {
        "economic" => TrustContext::Economic,
        "governance" => TrustContext::Governance,
        "social" => TrustContext::Social,
        "general" => TrustContext::General,
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid context. Use: economic, governance, social, or general".to_string(),
            );
        }
    };

    let own_info = registry.own_coop_info();
    let validity_secs = params.validity_days * 24 * 60 * 60;

    let attestation = FederatedTrustAttestation::new(
        own_info.coop_id.clone(),
        own_info.public_did.clone(),
        did.clone(),
        params.trust_score,
        trust_context,
        validity_secs,
    );

    let att_store = AttestationStore::new(store);
    match att_store.store_attestation(attestation) {
        Ok(()) => {
            info!(
                "Issued attestation for {} with trust score {}",
                params.member_did, params.trust_score
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "source_coop_id": own_info.coop_id,
                    "member_did": params.member_did,
                    "trust_score": params.trust_score,
                    "context": params.context,
                    "validity_days": params.validity_days,
                    "issued": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to issue attestation: {e}")),
    }
}

// ============================================================================
// Clearing handlers
// ============================================================================

/// Handle federation.clearing.list RPC call - list clearing agreements
pub async fn handle_federation_clearing_list(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Failed to create clearing manager: {e}"),
            )
        }
    };

    let agreements = manager.list_agreements();
    let agreements_json: Vec<serde_json::Value> = agreements
        .iter()
        .map(|a| {
            serde_json::json!({
                "agreement_id": a.agreement_id,
                "coop_a": a.coop_a,
                "coop_b": a.coop_b,
                "max_imbalance": a.max_imbalance,
                "settlement_interval": format!("{:?}", a.settlement_interval),
                "signatures": a.signatures.len(),
            })
        })
        .collect();

    RpcResponse::success(
        id,
        serde_json::json!({
            "agreements": agreements_json,
        }),
    )
}

/// Handle federation.clearing.show RPC call - show clearing agreement details
pub async fn handle_federation_clearing_show(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Failed to create clearing manager: {e}"),
            )
        }
    };

    match manager.get_agreement(&params.agreement_id) {
        Ok(Some(agreement)) => RpcResponse::success(
            id,
            serde_json::json!({
                "agreement_id": agreement.agreement_id,
                "coop_a": agreement.coop_a,
                "coop_b": agreement.coop_b,
                "coop_a_did": agreement.coop_a_did.to_string(),
                "coop_b_did": agreement.coop_b_did.to_string(),
                "max_imbalance": agreement.max_imbalance,
                "settlement_interval": format!("{:?}", agreement.settlement_interval),
                "signatures": agreement.signatures.len(),
                "exchange_rates": agreement.exchange_rates,
            }),
        ),
        Ok(None) => RpcResponse::error(
            id,
            -32000,
            format!("Agreement '{}' not found", params.agreement_id),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get agreement: {e}")),
    }
}

/// Handle federation.clearing.create RPC call - create a new clearing agreement
pub async fn handle_federation_clearing_create(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use icn_federation::{BilateralClearingAgreement, ClearingManager, SettlementInterval};

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
        partner_coop: String,
        max_imbalance: i64,
        settlement: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_info = registry.own_coop_info();
    let own_coop_id = own_info.coop_id.clone();

    // Get partner coop info
    let partner = match registry.get(&params.partner_coop) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Partner cooperative '{}' not found", params.partner_coop),
            );
        }
        Err(e) => {
            return RpcResponse::error(id, -32000, format!("Failed to get partner coop: {e}"))
        }
    };

    let settlement_interval = match params.settlement.to_lowercase().as_str() {
        "daily" => SettlementInterval::Daily,
        "weekly" => SettlementInterval::Weekly,
        "monthly" => SettlementInterval::Monthly,
        "manual" => SettlementInterval::Manual,
        _ => {
            return RpcResponse::error(
                id,
                -32602,
                "Invalid settlement. Use: daily, weekly, monthly, or manual".to_string(),
            );
        }
    };

    let mut agreement = BilateralClearingAgreement::new(
        params.agreement_id.clone(),
        own_coop_id.clone(),
        own_info.public_did.clone(),
        params.partner_coop.clone(),
        partner.public_did.clone(),
    );
    agreement.max_imbalance = params.max_imbalance;
    agreement.settlement_interval = settlement_interval;

    let manager = match ClearingManager::new(store, own_coop_id.clone()) {
        Ok(m) => m,
        Err(e) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Failed to create clearing manager: {e}"),
            )
        }
    };

    match manager.create_agreement(agreement) {
        Ok(_agreement_id) => {
            info!(
                "Created clearing agreement {} with {}",
                params.agreement_id, params.partner_coop
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "agreement_id": params.agreement_id,
                    "our_coop": own_coop_id,
                    "partner_coop": params.partner_coop,
                    "max_imbalance": params.max_imbalance,
                    "settlement": params.settlement,
                    "created": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to create agreement: {e}")),
    }
}

/// Handle federation.clearing.position RPC call - get clearing position
pub async fn handle_federation_clearing_position(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Failed to create clearing manager: {e}"),
            )
        }
    };

    match manager.calculate_position(&params.agreement_id) {
        Ok(position) => RpcResponse::success(
            id,
            serde_json::json!({
                "agreement_id": params.agreement_id,
                "coop_a_owes_b": position.coop_a_owes_b,
                "coop_b_owes_a": position.coop_b_owes_a,
                "net_position": position.net_position(),
                "pending_transfers": position.pending_transfers.len(),
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to calculate position: {e}")),
    }
}

/// Handle federation.clearing.settle RPC call - trigger settlement
pub async fn handle_federation_clearing_settle(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    _claims: Option<&RpcTokenClaims>,
) -> RpcResponse {
    use icn_federation::ClearingManager;

    let store = match state.store_handle() {
        Some(s) => s,
        None => {
            return RpcResponse::error(id, -32000, "Store not available".to_string());
        }
    };

    let registry = match state.federation_registry() {
        Some(r) => r,
        None => {
            return RpcResponse::error(id, -32000, "Federation not enabled".to_string());
        }
    };

    #[derive(serde::Deserialize)]
    struct Params {
        agreement_id: String,
    }

    let params: Params = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => return RpcResponse::error(id, -32602, format!("Invalid params: {e}")),
    };

    let own_coop_id = registry.own_coop_info().coop_id.clone();
    let manager = match ClearingManager::new(store, own_coop_id) {
        Ok(m) => m,
        Err(e) => {
            return RpcResponse::error(
                id,
                -32000,
                format!("Failed to create clearing manager: {e}"),
            )
        }
    };

    match manager.trigger_settlement(&params.agreement_id) {
        Ok(report) => {
            info!("Settlement completed for {}", params.agreement_id);
            RpcResponse::success(
                id,
                serde_json::json!({
                    "agreement_id": report.agreement_id,
                    "coop_a_owed": report.coop_a_owed,
                    "coop_b_owed": report.coop_b_owed,
                    "net_settlement": report.net_settlement,
                    "transfers_settled": report.transfers_settled,
                    "settled": true,
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to settle: {e}")),
    }
}

/// Convert a CooperativeInfo to JSON
fn coop_info_to_json(coop: &icn_federation::CooperativeInfo) -> serde_json::Value {
    // Determine if the coop allows federation based on its policy
    let federated = coop.federation_policy.allows_federation();

    // Serialize the federation policy to JSON
    let federation_policy = match &coop.federation_policy {
        icn_federation::FederationPolicy::Open => serde_json::json!({"type": "Open"}),
        icn_federation::FederationPolicy::Vouched { min_vouches } => {
            serde_json::json!({"type": "Vouched", "min_vouches": min_vouches})
        }
        icn_federation::FederationPolicy::Closed => serde_json::json!({"type": "Closed"}),
    };

    serde_json::json!({
        "coop_id": coop.coop_id,
        "name": coop.name,
        "public_did": coop.public_did.to_string(),
        "gateway_endpoints": coop.gateway_endpoints,
        "federation_policy": federation_policy,
        "federated": federated,
        "capabilities": coop.capabilities,
        "currencies": coop.currencies.iter().map(|c| serde_json::json!({
            "symbol": c.symbol,
            "name": c.name,
            "decimals": c.decimals,
        })).collect::<Vec<_>>(),
        "last_seen": coop.last_seen,
    })
}
