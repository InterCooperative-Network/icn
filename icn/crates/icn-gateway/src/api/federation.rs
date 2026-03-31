//! Federation API endpoints
//!
//! RESTful API for managing cooperative federation, attestations, and clearing.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::federation_mgr::FederationManager;
use crate::middleware::{get_claims, require_scope};
use icn_federation::{
    BilateralClearingAgreement, CooperativeInfo, FederatedTrustAttestation, FederationPolicy,
    SettlementInterval, SettlementReport, TrustContext, Vouch,
};
use icn_identity::Did;
use icn_kernel_api::{ClearingPositionView, FederationService};

// ============================================================================
// Request/Response Models
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct InitFederationRequest {
    pub coop_id: String,
    pub name: String,
    pub gateway_endpoint: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterCoopRequest {
    pub coop_id: String,
    pub name: String,
    pub public_did: String,
    pub gateway_endpoints: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VouchRequest {
    pub target_coop_id: String,
    pub trust_score: f64,
    pub expires_in_days: Option<u64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttestationRequest {
    pub member_did: String,
    pub trust_score: f64,
    pub context: String, // economic, social, governance, general
    pub validity_days: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAgreementRequest {
    pub agreement_id: String,
    pub partner_coop_id: String,
    pub partner_did: String,
    pub max_imbalance: i64,
    pub settlement: String, // daily, weekly, monthly, manual
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FederationStatusResponse {
    pub initialized: bool,
    pub own_coop_id: Option<String>,
    pub own_coop_name: Option<String>,
    pub federated_coops: usize,
    pub clearing_agreements: usize,
}

// ============================================================================
// Status Endpoint
// ============================================================================

/// GET /federation/status - Get federation status
#[get("/status")]
pub async fn get_status(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let (initialized, own_info) = match fed_mgr.get_own_info().await {
        Ok(info) => (true, Some(info)),
        Err(_) => (false, None),
    };

    let federated_coops = if initialized {
        fed_mgr
            .list_cooperatives()
            .await
            .map(|c| c.len())
            .unwrap_or(0)
    } else {
        0
    };

    let clearing_agreements = if initialized {
        fed_mgr
            .list_agreements()
            .await
            .map(|a| a.len())
            .unwrap_or(0)
    } else {
        0
    };

    Ok(HttpResponse::Ok().json(FederationStatusResponse {
        initialized,
        own_coop_id: own_info.as_ref().map(|i| i.coop_id.clone()),
        own_coop_name: own_info.as_ref().map(|i| i.name.clone()),
        federated_coops,
        clearing_agreements,
    }))
}

/// POST /federation/init - Initialize federation with own cooperative info
#[post("/init")]
pub async fn init_federation(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    req: web::Json<InitFederationRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:admin")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let own_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let mut own_info = CooperativeInfo::new(
        req.coop_id.clone(),
        req.name.clone(),
        own_did,
        FederationPolicy::default(),
    );

    if let Some(gw) = &req.gateway_endpoint {
        own_info = own_info.with_gateway(gw.clone());
    }

    fed_mgr.init_registry(own_info).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "initialized",
        "coop_id": req.coop_id
    })))
}

// ============================================================================
// Cooperative Registry Endpoints
// ============================================================================

/// GET /federation/coops - List known cooperatives
#[get("/coops")]
pub async fn list_coops(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let coops = fed_mgr.list_cooperatives().await?;
    Ok(HttpResponse::Ok().json(coops))
}

/// GET /federation/coops/{coop_id} - Get a specific cooperative
#[get("/coops/{coop_id}")]
pub async fn get_coop(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let coop_id = path.into_inner();
    match fed_mgr.get_cooperative(&coop_id).await? {
        Some(coop) => Ok(HttpResponse::Ok().json(coop)),
        None => Err(GatewayError::NotFound(format!(
            "Cooperative not found: {coop_id}"
        ))),
    }
}

/// POST /federation/coops - Register a new cooperative
#[post("/coops")]
pub async fn register_coop(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    req: web::Json<RegisterCoopRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let public_did: Did = req
        .public_did
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let mut info = CooperativeInfo::new(
        req.coop_id.clone(),
        req.name.clone(),
        public_did,
        FederationPolicy::default(),
    );

    for gw in &req.gateway_endpoints {
        info = info.with_gateway(gw.clone());
    }

    for cap in &req.capabilities {
        info = info.with_capability(cap);
    }

    fed_mgr.register_cooperative(info).await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "status": "registered",
        "coop_id": req.coop_id
    })))
}

// ============================================================================
// Vouch Endpoints
// ============================================================================

/// GET /federation/coops/{coop_id}/vouches - Get vouches for a cooperative
#[get("/coops/{coop_id}/vouches")]
pub async fn get_vouches(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let coop_id = path.into_inner();
    let vouches = fed_mgr.get_vouches(&coop_id).await?;

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "coop_id": coop_id,
        "vouches": vouches
    })))
}

/// POST /federation/coops/{coop_id}/vouch - Vouch for a cooperative
#[post("/coops/{coop_id}/vouch")]
pub async fn vouch_for_coop(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
    req: web::Json<VouchRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let voucher_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    // Validate trust score
    if req.trust_score < 0.0 || req.trust_score > 1.0 {
        return Err(GatewayError::BadRequest(
            "Trust score must be between 0.0 and 1.0".to_string(),
        ));
    }

    let own_info = fed_mgr.get_own_info().await?;
    let target_coop_id = path.into_inner();

    let vouch = if let Some(days) = req.expires_in_days {
        let expires_at = icn_time::current_timestamp_secs() + (days * 24 * 60 * 60);
        Vouch::new(
            own_info.coop_id.clone(),
            voucher_did,
            target_coop_id.clone(),
            req.trust_score,
        )
        .with_expiry(expires_at)
    } else {
        Vouch::new(
            own_info.coop_id.clone(),
            voucher_did,
            target_coop_id.clone(),
            req.trust_score,
        )
    };

    fed_mgr.add_vouch(&vouch).await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "status": "vouched",
        "target_coop_id": target_coop_id,
        "trust_score": req.trust_score
    })))
}

// ============================================================================
// Attestation Endpoints
// ============================================================================

/// GET /federation/attestations/{member_did} - Get attestations for a member
#[get("/attestations/{member_did}")]
pub async fn get_attestations(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let member_did_str = path.into_inner();
    let member_did: Did = member_did_str
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let attestations = fed_mgr.get_attestations_for(&member_did).await?;
    Ok(HttpResponse::Ok().json(attestations))
}

/// POST /federation/attestations - Issue an attestation
#[post("/attestations")]
pub async fn issue_attestation(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    req: web::Json<AttestationRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    let source_did: Did = claims
        .sub
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid DID in token: {e}")))?;

    let member_did: Did = req
        .member_did
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid member DID: {e}")))?;

    if req.trust_score < 0.0 || req.trust_score > 1.0 {
        return Err(GatewayError::BadRequest(
            "Trust score must be between 0.0 and 1.0".to_string(),
        ));
    }

    let context = match req.context.to_lowercase().as_str() {
        "economic" => TrustContext::Economic,
        "social" => TrustContext::Social,
        "governance" => TrustContext::Governance,
        "general" => TrustContext::General,
        _ => {
            return Err(GatewayError::BadRequest(
                "Invalid context. Use: economic, social, governance, or general".to_string(),
            ))
        }
    };

    let own_info = fed_mgr.get_own_info().await?;
    let validity_secs = req.validity_days * 24 * 60 * 60;

    let attestation = FederatedTrustAttestation::new(
        own_info.coop_id,
        source_did,
        member_did.clone(),
        req.trust_score,
        context,
        validity_secs,
    );

    fed_mgr.store_attestation(attestation).await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "status": "issued",
        "member_did": member_did.to_string()
    })))
}

// ============================================================================
// Clearing Endpoints
// ============================================================================

/// GET /federation/clearing - List clearing agreements
#[get("/clearing")]
pub async fn list_agreements(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let agreements = fed_mgr.list_agreements().await?;
    Ok(HttpResponse::Ok().json(agreements))
}

/// GET /federation/clearing/{agreement_id} - Get a clearing agreement
#[get("/clearing/{agreement_id}")]
pub async fn get_agreement(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let agreement_id = path.into_inner();
    match fed_mgr.get_agreement(&agreement_id).await? {
        Some(agreement) => Ok(HttpResponse::Ok().json(agreement)),
        None => Err(GatewayError::NotFound(format!(
            "Agreement not found: {agreement_id}"
        ))),
    }
}

/// POST /federation/clearing - Create a clearing agreement
#[post("/clearing")]
pub async fn create_agreement(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    req: web::Json<CreateAgreementRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let own_info = fed_mgr.get_own_info().await?;

    let partner_did: Did = req
        .partner_did
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid partner DID: {e}")))?;

    let settlement_interval = match req.settlement.to_lowercase().as_str() {
        "daily" => SettlementInterval::Daily,
        "weekly" => SettlementInterval::Weekly,
        "monthly" => SettlementInterval::Monthly,
        "manual" => SettlementInterval::Manual,
        _ => {
            return Err(GatewayError::BadRequest(
                "Invalid settlement. Use: daily, weekly, monthly, or manual".to_string(),
            ))
        }
    };

    let mut agreement = BilateralClearingAgreement::new(
        req.agreement_id.clone(),
        own_info.coop_id,
        own_info.public_did,
        req.partner_coop_id.clone(),
        partner_did,
    );
    agreement.max_imbalance = req.max_imbalance;
    agreement.settlement_interval = settlement_interval;

    let id = fed_mgr.create_agreement(agreement).await?;

    Ok(HttpResponse::Created().json(serde_json::json!({
        "status": "created",
        "agreement_id": id
    })))
}

/// GET /federation/clearing/{agreement_id}/position - Get current inter-party clearing position
///
/// Returns the accumulated clearing obligations between the two parties in the agreement:
/// how much each party owes the other (confirmed, unsettled), the net position, and
/// the count of pending (unconfirmed) transfers.
///
/// When the daemon's FederationService is available (normal runtime), this queries the
/// supervisor-owned clearing state — the same state that the settlement scheduler uses.
/// In standalone mode (no daemon), this falls back to the gateway's own clearing manager.
#[get("/clearing/{agreement_id}/position")]
pub async fn get_position(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    federation_service: web::Data<Arc<Option<Arc<dyn FederationService>>>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let agreement_id = path.into_inner();

    // Prefer the supervisor-owned FederationService: it reads from the correct clearing
    // store that governance-triggered agreements and compute-receipt accumulations write to.
    // Fall back to the gateway's own clearing manager only in standalone mode.
    if let Some(ref service) = ***federation_service {
        let view = service
            .get_clearing_position(&agreement_id)
            .map_err(|e| GatewayError::NotFound(e.to_string()))?;
        return Ok(HttpResponse::Ok().json(view));
    }

    // Standalone fallback: gateway's own ClearingManager (may not reflect daemon state).
    let position = fed_mgr.get_position(&agreement_id).await?;
    // Adapt internal ClearingPosition to the party-level response vocabulary.
    let agreement = fed_mgr
        .get_agreement(&agreement_id)
        .await?
        .ok_or_else(|| GatewayError::NotFound(format!("Agreement not found: {agreement_id}")))?;
    let view = ClearingPositionView {
        agreement_id: agreement_id.clone(),
        party_a: agreement.coop_a.clone(),
        party_b: agreement.coop_b.clone(),
        party_a_owes: position.coop_a_owes_b,
        party_b_owes: position.coop_b_owes_a,
        net_position: position.net_position(),
        pending_count: position.pending_transfers.len(),
        last_settlement_ts: position.last_settlement,
    };
    Ok(HttpResponse::Ok().json(view))
}

/// POST /federation/clearing/{agreement_id}/settle - Trigger settlement
#[post("/clearing/{agreement_id}/settle")]
pub async fn trigger_settlement(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let agreement_id = path.into_inner();
    let report = fed_mgr.settle(&agreement_id).await?;

    Ok(HttpResponse::Ok().json(report))
}

/// POST /federation/clearing/settle-scheduled - Process all scheduled settlements
#[utoipa::path(
    post,
    path = "/federation/clearing/settle-scheduled",
    tag = "Federation",
    responses(
        (status = 200, description = "Scheduled settlements processed", body = Vec<SettlementReport>),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
#[post("/clearing/settle-scheduled")]
pub async fn process_scheduled_settlements(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let reports = fed_mgr.process_scheduled_settlements().await?;

    Ok(HttpResponse::Ok().json(reports))
}

/// POST /federation/clearing/netting/{unit} - Perform multilateral netting
#[derive(Debug, Serialize, ToSchema)]
pub struct NettingResultResponse {
    pub cycles_canceled: usize,
    pub amount_reduced: i64,
    pub original_obligations: usize,
    pub netted_obligations: usize,
}

#[utoipa::path(
    post,
    path = "/federation/clearing/netting/{unit}",
    tag = "Federation",
    params(
        ("unit" = String, Path, description = "Unit of account (e.g., USD, hours)")
    ),
    responses(
        (status = 200, description = "Netting analysis completed (positions NOT modified)", body = NettingResultResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
#[post("/clearing/netting/{unit}")]
pub async fn perform_multilateral_netting(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?; // Read-only analysis

    let unit = path.into_inner();
    let result = fed_mgr.perform_multilateral_netting(&unit).await?;

    let response = NettingResultResponse {
        cycles_canceled: result.cycles_canceled.len(),
        amount_reduced: result.amount_reduced,
        original_obligations: result.original.len(),
        netted_obligations: result.netted.len(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /federation/clearing/netting/{unit}/apply - Apply multilateral netting to positions
#[utoipa::path(
    post,
    path = "/federation/clearing/netting/{unit}/apply",
    tag = "Federation",
    params(
        ("unit" = String, Path, description = "Unit of account (e.g., USD, hours)")
    ),
    responses(
        (status = 200, description = "Netting applied and positions updated", body = NettingResultResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
#[post("/clearing/netting/{unit}/apply")]
pub async fn apply_multilateral_netting(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?; // Write permission required

    let unit = path.into_inner();

    // First compute netting
    let result = fed_mgr.perform_multilateral_netting(&unit).await?;

    // Then apply it
    fed_mgr.apply_multilateral_netting(&result).await?;

    let response = NettingResultResponse {
        cycles_canceled: result.cycles_canceled.len(),
        amount_reduced: result.amount_reduced,
        original_obligations: result.original.len(),
        netted_obligations: result.netted.len(),
    };

    Ok(HttpResponse::Ok().json(response))
}

// ============================================================================
// Federation Connect Endpoint
// ============================================================================

/// POST /federation/connect - Connect to a federation peer
///
/// Registers a remote cooperative and initiates federation connectivity.
/// This is a convenience endpoint that combines registration with connection setup.
#[post("/connect")]
pub async fn federation_connect(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    body: web::Json<crate::models::FederationConnectRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?;

    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // Validate address format (host:port with valid port number)
    let addr_parts: Vec<&str> = body.address.rsplitn(2, ':').collect();
    if addr_parts.len() != 2
        || addr_parts[0].parse::<u16>().is_err()
        || addr_parts[1].is_empty()
        || body.address.contains("://")
    {
        return Err(GatewayError::BadRequest(
            "Address must be in host:port format with a valid port (e.g., \"node-b.local:9000\")"
                .to_string(),
        ));
    }

    // Require peer DID for identity verification
    let peer_did: Did = body
        .peer_did
        .as_ref()
        .ok_or_else(|| {
            GatewayError::BadRequest(
                "peer_did is required to identify the remote cooperative".to_string(),
            )
        })?
        .parse()
        .map_err(|e| GatewayError::BadRequest(format!("Invalid peer DID: {e}")))?;

    // Ensure federation is initialized
    let own_info = fed_mgr.get_own_info().await.map_err(|_| {
        GatewayError::BadRequest(
            "Federation not initialized. Call POST /federation/init first.".to_string(),
        )
    })?;

    // Register the peer cooperative
    let peer_coop_id = body
        .coop_id
        .clone()
        .unwrap_or_else(|| format!("peer-{}", &body.address.replace(':', "-")));
    let peer_name = body
        .name
        .clone()
        .unwrap_or_else(|| format!("Peer at {}", body.address));

    let peer_info = CooperativeInfo::new(
        peer_coop_id.clone(),
        peer_name,
        peer_did,
        FederationPolicy::default(),
    )
    .with_gateway(format!("http://{}", body.address));

    // Register the peer (tolerate "already registered" but surface real errors)
    match fed_mgr.register_cooperative(peer_info).await {
        Ok(()) => {}
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already") || msg.contains("exists") {
                tracing::debug!("Peer already registered: {msg}");
            } else {
                return Err(GatewayError::InternalError(format!(
                    "Failed to register peer: {msg}"
                )));
            }
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "connected",
        "peer_coop_id": peer_coop_id,
        "address": body.address,
        "own_coop_id": own_info.coop_id
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_federation_status_route_matches() {
        let fed_mgr = Arc::new(FederationManager::new());

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr.clone()))
                .service(web::scope("/federation").service(get_status)),
        )
        .await;

        // Request without auth claims - should reach handler and fail on require_scope,
        // NOT return 404
        let req = test::TestRequest::get()
            .uri("/federation/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        // If route matches, we get 401/403 (auth/scope error), NOT 404
        assert_ne!(
            resp.status().as_u16(),
            404,
            "Federation status route should match (got 404 = route not found)"
        );
    }

    /// Verify that the clearing position handler prefers the supervisor-owned FederationService
    /// over the gateway-local FederationManager when a service is injected.
    ///
    /// Architectural invariant (ADR 0011): the gateway must never present state from its own
    /// divergent local manager when a supervisor-owned service is available. Reads must route
    /// through the canonical service instance.
    #[actix_web::test]
    async fn test_get_position_prefers_federation_service_over_local_manager() {
        use actix_web::HttpMessage;
        use icn_kernel_api::{
            ClearingPositionView, FederationClearingRequest, FederationClearingResult,
            FederationClearingSettleRequest, FederationClearingSettleResult, FederationJoinRequest,
            FederationJoinResult, FederationService, FederationVouchRequest, FederationVouchResult,
        };

        // Stub service that records calls and returns a known ClearingPositionView.
        // Only get_clearing_position is implemented — all others panic to surface
        // any accidental routing to the wrong method.
        struct StubFederationService;

        impl FederationService for StubFederationService {
            fn join_federation(
                &self,
                _req: FederationJoinRequest,
            ) -> anyhow::Result<FederationJoinResult> {
                unimplemented!("test stub: join_federation should not be called")
            }
            fn vouch_for_cooperative(
                &self,
                _req: FederationVouchRequest,
            ) -> anyhow::Result<FederationVouchResult> {
                unimplemented!("test stub: vouch_for_cooperative should not be called")
            }
            fn establish_clearing(
                &self,
                _req: FederationClearingRequest,
            ) -> anyhow::Result<FederationClearingResult> {
                unimplemented!("test stub: establish_clearing should not be called")
            }
            fn settle_clearing(
                &self,
                _req: FederationClearingSettleRequest,
            ) -> anyhow::Result<FederationClearingSettleResult> {
                unimplemented!("test stub: settle_clearing should not be called")
            }
            fn get_clearing_position(
                &self,
                agreement_id: &str,
            ) -> anyhow::Result<ClearingPositionView> {
                Ok(ClearingPositionView {
                    agreement_id: agreement_id.to_string(),
                    party_a: "coop-alpha".to_string(),
                    party_b: "coop-beta".to_string(),
                    party_a_owes: 100,
                    party_b_owes: 0,
                    net_position: 100,
                    pending_count: 1,
                    last_settlement_ts: 0,
                })
            }
            fn is_registered(&self, _coop_did: &str) -> bool {
                false
            }
            fn get_registration_provenance(&self, _coop_did: &str) -> Option<(String, String)> {
                None
            }
        }

        let fed_mgr = Arc::new(FederationManager::new());
        let stub_service: Arc<dyn FederationService> = Arc::new(StubFederationService);
        // Wrap as the gateway expects: Arc<Option<Arc<dyn FederationService>>>
        let service_for_routes: Arc<Option<Arc<dyn FederationService>>> =
            Arc::new(Some(stub_service));

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr.clone()))
                .app_data(web::Data::new(service_for_routes))
                .service(web::scope("/v1/federation").service(get_position)),
        )
        .await;

        // Inject minimal JWT claims so require_scope("federation:read") passes.
        let claims = crate::auth::TokenClaims {
            sub: "did:icn:test".to_string(),
            iat: 1_000_000_000,
            coop_id: "coop-alpha".to_string(),
            scopes: vec!["federation:read".to_string()],
            exp: 9_999_999_999,
        };
        let req = test::TestRequest::get()
            .uri("/v1/federation/clearing/agr-001/position")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status().as_u16(),
            200,
            "Expected 200 from service-backed path; fallback would 404/500"
        );

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(
            body["party_a"], "coop-alpha",
            "Response must come from StubFederationService, not local FederationManager"
        );
        assert_eq!(body["net_position"], 100);
    }

    /// Verify that empty scopes placed AFTER named scopes don't steal routes.
    /// This is a regression test for the bug where web::scope("") before
    /// web::scope("/federation") caused federation routes to 404.
    #[actix_web::test]
    async fn test_empty_scope_ordering_does_not_steal_routes() {
        let fed_mgr = Arc::new(FederationManager::new());

        // Empty scopes AFTER federation (correct ordering)
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(fed_mgr.clone()))
                .service(
                    web::scope("/v1")
                        .service(web::scope("/federation").service(get_status))
                        .service(web::scope("")), // empty scope AFTER — should not steal
                ),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/v1/federation/status")
            .to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status().as_u16();
        assert_ne!(
            status, 404,
            "Federation status should not return 404 when empty scopes are last (got {status})"
        );
    }
}
