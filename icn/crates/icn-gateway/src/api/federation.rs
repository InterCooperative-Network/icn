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

/// GET /federation/clearing/{agreement_id}/position - Get clearing position
#[get("/clearing/{agreement_id}/position")]
pub async fn get_position(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?;

    let agreement_id = path.into_inner();
    let position = fed_mgr.get_position(&agreement_id).await?;

    Ok(HttpResponse::Ok().json(position))
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

/// POST /federation/clearing/netting/{currency} - Perform multilateral netting
#[derive(Debug, Serialize, ToSchema)]
pub struct NettingResultResponse {
    pub cycles_canceled: usize,
    pub amount_reduced: i64,
    pub original_obligations: usize,
    pub netted_obligations: usize,
}

#[utoipa::path(
    post,
    path = "/federation/clearing/netting/{currency}",
    tag = "Federation",
    params(
        ("currency" = String, Path, description = "Currency code (e.g., USD, hours)")
    ),
    responses(
        (status = 200, description = "Netting analysis completed (positions NOT modified)", body = NettingResultResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
#[post("/clearing/netting/{currency}")]
pub async fn perform_multilateral_netting(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:read")?; // Read-only analysis

    let currency = path.into_inner();
    let result = fed_mgr.perform_multilateral_netting(&currency).await?;

    let response = NettingResultResponse {
        cycles_canceled: result.cycles_canceled.len(),
        amount_reduced: result.amount_reduced,
        original_obligations: result.original.len(),
        netted_obligations: result.netted.len(),
    };

    Ok(HttpResponse::Ok().json(response))
}

/// POST /federation/clearing/netting/{currency}/apply - Apply multilateral netting to positions
#[utoipa::path(
    post,
    path = "/federation/clearing/netting/{currency}/apply",
    tag = "Federation",
    params(
        ("currency" = String, Path, description = "Currency code (e.g., USD, hours)")
    ),
    responses(
        (status = 200, description = "Netting applied and positions updated", body = NettingResultResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
#[post("/clearing/netting/{currency}/apply")]
pub async fn apply_multilateral_netting(
    http_req: HttpRequest,
    fed_mgr: web::Data<Arc<FederationManager>>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "federation:write")?; // Write permission required

    let currency = path.into_inner();

    // First compute netting
    let result = fed_mgr.perform_multilateral_netting(&currency).await?;

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
