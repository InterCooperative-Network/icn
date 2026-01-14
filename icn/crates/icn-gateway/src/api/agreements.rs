//! Inter-Cooperative Agreement API endpoints
//!
//! RESTful API for managing inter-cooperative agreements including:
//! - Agreement lifecycle (create, propose, sign, suspend, resume, terminate)
//! - Party management
//! - Amendment workflow
//! - Status queries

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{GatewayError, Result};
use crate::middleware::{get_claims, require_scope};

// ============================================================================
// Request/Response Models
// ============================================================================

/// Request to create a new agreement draft
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateAgreementRequest {
    /// Agreement title
    pub title: String,
    /// Agreement description
    pub description: String,
    /// Type of agreement
    pub agreement_type: AgreementTypeRequest,
}

/// Agreement type configuration
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgreementTypeRequest {
    Trade {
        items: Vec<TradeItemRequest>,
        currency: String,
    },
    Credit {
        credit_limit: i64,
        interest_rate_bps: u32,
        currency: String,
    },
    ResourceSharing {
        resource_type: String,
        duration_days: u32,
        compensation_model: String,
    },
    FederationMembership {
        federation_id: String,
        min_trust_threshold: f64,
        governance_binding: bool,
    },
    Custom {
        agreement_type_name: String,
        terms_json: String,
    },
}

/// Trade item in a trade agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct TradeItemRequest {
    pub description: String,
    pub quantity: u32,
    pub unit: String,
    pub unit_price: i64,
    pub currency: String,
}

/// Request to add a party to an agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddPartyRequest {
    /// DID of the party
    pub did: String,
    /// Cooperative ID
    pub coop_id: String,
    /// Role: proposer, counterparty, witness, guarantor
    pub role: String,
    /// Optional display name
    pub display_name: Option<String>,
}

/// Request to set agreement terms
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetTermsRequest {
    /// Effective date (Unix timestamp)
    pub effective_date: Option<u64>,
    /// Expiration date (Unix timestamp)
    pub expiration_date: Option<u64>,
    /// Termination notice period in days
    pub termination_notice_days: Option<u32>,
    /// Dispute resolution method
    pub dispute_resolution: Option<String>,
    /// Additional terms
    pub additional_terms: Option<String>,
}

/// Request to suspend an agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct SuspendRequest {
    /// Reason for suspension
    pub reason: String,
}

/// Request to terminate an agreement
#[derive(Debug, Deserialize, ToSchema)]
pub struct TerminateRequest {
    /// Termination reason type: expired, mutual_consent, breach, withdrawal, force_majeure
    pub reason_type: String,
    /// Additional details
    pub details: Option<String>,
}

/// Request to propose an amendment
#[derive(Debug, Deserialize, ToSchema)]
pub struct ProposeAmendmentRequest {
    /// Description of the amendment
    pub description: String,
    /// List of changes
    pub changes: Vec<AmendmentChangeRequest>,
}

/// Amendment change specification
#[derive(Debug, Deserialize, ToSchema)]
#[serde(tag = "change_type", rename_all = "snake_case")]
pub enum AmendmentChangeRequest {
    ExtendDuration { new_expiration: u64 },
    UpdateTerm { field: String, new_value: String },
    AddParty { did: String, coop_id: String, role: String },
    RemoveParty { party_did: String },
}

/// Agreement list response
#[derive(Debug, Serialize, ToSchema)]
pub struct AgreementListResponse {
    pub agreements: Vec<AgreementSummary>,
    pub total: usize,
}

/// Agreement summary for lists
#[derive(Debug, Serialize, ToSchema)]
pub struct AgreementSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub agreement_type: String,
    pub parties_count: usize,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Agreement detail response
#[derive(Debug, Serialize, ToSchema)]
pub struct AgreementDetailResponse {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub status_details: serde_json::Value,
    pub agreement_type: String,
    pub agreement_type_details: serde_json::Value,
    pub parties: Vec<PartyInfo>,
    pub signatures: Vec<SignatureInfo>,
    pub terms: serde_json::Value,
    pub version: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Party information
#[derive(Debug, Serialize, ToSchema)]
pub struct PartyInfo {
    pub did: String,
    pub coop_id: String,
    pub role: String,
    pub display_name: Option<String>,
    pub has_signed: bool,
}

/// Signature information
#[derive(Debug, Serialize, ToSchema)]
pub struct SignatureInfo {
    pub signer_did: String,
    pub coop_id: String,
    pub signed_at: u64,
    pub version_signed: u32,
}

/// Amendment response
#[derive(Debug, Serialize, ToSchema)]
pub struct AmendmentResponse {
    pub id: String,
    pub agreement_id: String,
    pub description: String,
    pub status: String,
    pub proposed_by: String,
    pub proposed_at: u64,
    pub signatures_count: usize,
    pub required_signatures: usize,
}

// ============================================================================
// API Endpoints
// ============================================================================

/// GET /agreements - List all agreements
///
/// Query parameters:
/// - status: Filter by status (draft, proposed, active, suspended, terminated)
/// - party: Filter by party DID
#[utoipa::path(
    get,
    path = "/agreements",
    tag = "agreements",
    responses(
        (status = 200, description = "List of agreements", body = AgreementListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - insufficient scope")
    ),
    security(("bearer_auth" = []))
)]
#[get("")]
pub async fn list_agreements(
    http_req: HttpRequest,
    _query: web::Query<ListAgreementsQuery>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:read")?;

    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // TODO: Get agreements from AgreementManager
    // For now, return empty list as placeholder

    let response = AgreementListResponse {
        agreements: vec![],
        total: 0,
    };

    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
pub struct ListAgreementsQuery {
    pub status: Option<String>,
    pub party: Option<String>,
}

/// POST /agreements - Create a new agreement draft
#[utoipa::path(
    post,
    path = "/agreements",
    tag = "agreements",
    request_body = CreateAgreementRequest,
    responses(
        (status = 201, description = "Agreement created", body = AgreementDetailResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden")
    ),
    security(("bearer_auth" = []))
)]
#[post("")]
pub async fn create_agreement(
    http_req: HttpRequest,
    _req: web::Json<CreateAgreementRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _claims = get_claims(&http_req)
        .ok_or_else(|| GatewayError::AuthenticationFailed("No claims found".to_string()))?;

    // TODO: Create agreement using AgreementManager

    Err(GatewayError::InternalError(
        "Agreement creation not yet integrated".to_string(),
    ))
}

/// GET /agreements/{id} - Get agreement details
#[utoipa::path(
    get,
    path = "/agreements/{id}",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement details", body = AgreementDetailResponse),
        (status = 404, description = "Agreement not found"),
        (status = 401, description = "Unauthorized")
    ),
    security(("bearer_auth" = []))
)]
#[get("/{id}")]
pub async fn get_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:read")?;

    let _agreement_id = path.into_inner();

    // TODO: Get agreement from AgreementManager

    Err(GatewayError::InternalError(
        "Agreement retrieval not yet integrated".to_string(),
    ))
}

/// DELETE /agreements/{id} - Delete a draft agreement
#[utoipa::path(
    delete,
    path = "/agreements/{id}",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 204, description = "Agreement deleted"),
        (status = 404, description = "Agreement not found"),
        (status = 400, description = "Cannot delete non-draft agreement")
    ),
    security(("bearer_auth" = []))
)]
#[delete("/{id}")]
pub async fn delete_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _agreement_id = path.into_inner();

    // TODO: Delete draft agreement

    Err(GatewayError::InternalError(
        "Agreement deletion not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/parties - Add a party to draft agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/parties",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = AddPartyRequest,
    responses(
        (status = 200, description = "Party added", body = AgreementDetailResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Agreement not found")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/parties")]
pub async fn add_party(
    http_req: HttpRequest,
    path: web::Path<String>,
    _req: web::Json<AddPartyRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Add party not yet integrated".to_string(),
    ))
}

/// PUT /agreements/{id}/terms - Set agreement terms
#[utoipa::path(
    put,
    path = "/agreements/{id}/terms",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = SetTermsRequest,
    responses(
        (status = 200, description = "Terms set", body = AgreementDetailResponse),
        (status = 400, description = "Bad request")
    ),
    security(("bearer_auth" = []))
)]
#[put("/{id}/terms")]
pub async fn set_terms(
    http_req: HttpRequest,
    path: web::Path<String>,
    _req: web::Json<SetTermsRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Set terms not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/propose - Propose the agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/propose",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement proposed", body = AgreementDetailResponse),
        (status = 400, description = "Cannot propose - invalid state or missing parties")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/propose")]
pub async fn propose_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Propose agreement not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/sign - Sign the agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/sign",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement signed", body = AgreementDetailResponse),
        (status = 400, description = "Cannot sign - invalid state or not a party")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/sign")]
pub async fn sign_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Sign agreement not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/suspend - Suspend an active agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/suspend",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = SuspendRequest,
    responses(
        (status = 200, description = "Agreement suspended", body = AgreementDetailResponse),
        (status = 400, description = "Cannot suspend")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/suspend")]
pub async fn suspend_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
    _req: web::Json<SuspendRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:admin")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Suspend agreement not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/resume - Resume a suspended agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/resume",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "Agreement resumed", body = AgreementDetailResponse),
        (status = 400, description = "Cannot resume")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/resume")]
pub async fn resume_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:admin")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Resume agreement not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/terminate - Terminate an agreement
#[utoipa::path(
    post,
    path = "/agreements/{id}/terminate",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = TerminateRequest,
    responses(
        (status = 200, description = "Agreement terminated", body = AgreementDetailResponse),
        (status = 400, description = "Cannot terminate")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/terminate")]
pub async fn terminate_agreement(
    http_req: HttpRequest,
    path: web::Path<String>,
    _req: web::Json<TerminateRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:admin")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Terminate agreement not yet integrated".to_string(),
    ))
}

// ============================================================================
// Amendment Endpoints
// ============================================================================

/// GET /agreements/{id}/amendments - List amendments for an agreement
#[utoipa::path(
    get,
    path = "/agreements/{id}/amendments",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    responses(
        (status = 200, description = "List of amendments", body = Vec<AmendmentResponse>)
    ),
    security(("bearer_auth" = []))
)]
#[get("/{id}/amendments")]
pub async fn list_amendments(
    http_req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:read")?;

    let _agreement_id = path.into_inner();

    Ok(HttpResponse::Ok().json(Vec::<AmendmentResponse>::new()))
}

/// POST /agreements/{id}/amendments - Propose an amendment
#[utoipa::path(
    post,
    path = "/agreements/{id}/amendments",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID")
    ),
    request_body = ProposeAmendmentRequest,
    responses(
        (status = 201, description = "Amendment proposed", body = AmendmentResponse),
        (status = 400, description = "Cannot amend")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/amendments")]
pub async fn propose_amendment(
    http_req: HttpRequest,
    path: web::Path<String>,
    _req: web::Json<ProposeAmendmentRequest>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let _agreement_id = path.into_inner();

    Err(GatewayError::InternalError(
        "Propose amendment not yet integrated".to_string(),
    ))
}

/// POST /agreements/{id}/amendments/{amendment_id}/sign - Sign an amendment
#[utoipa::path(
    post,
    path = "/agreements/{id}/amendments/{amendment_id}/sign",
    tag = "agreements",
    params(
        ("id" = String, Path, description = "Agreement ID"),
        ("amendment_id" = String, Path, description = "Amendment ID")
    ),
    responses(
        (status = 200, description = "Amendment signed", body = AmendmentResponse),
        (status = 400, description = "Cannot sign")
    ),
    security(("bearer_auth" = []))
)]
#[post("/{id}/amendments/{amendment_id}/sign")]
pub async fn sign_amendment(
    http_req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    require_scope(&http_req, "agreements:write")?;

    let (_agreement_id, _amendment_id) = path.into_inner();

    Err(GatewayError::InternalError(
        "Sign amendment not yet integrated".to_string(),
    ))
}

// ============================================================================
// Route Configuration
// ============================================================================

/// Configure agreement routes
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/agreements")
            .service(list_agreements)
            .service(create_agreement)
            .service(get_agreement)
            .service(delete_agreement)
            .service(add_party)
            .service(set_terms)
            .service(propose_agreement)
            .service(sign_agreement)
            .service(suspend_agreement)
            .service(resume_agreement)
            .service(terminate_agreement)
            .service(list_amendments)
            .service(propose_amendment)
            .service(sign_amendment),
    );
}
